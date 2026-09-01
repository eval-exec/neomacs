//! Buffer text storage.
//!
//! GNU Emacs separates per-buffer metadata from the underlying text object.
//! `BufferText` is the first local seam toward that design. It owns the
//! Lisp-visible text semantics while the concrete byte storage backend remains
//! hidden behind a private enum.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::emacs_core::value::Value;
use crate::gc_trace::GcTrace;

use super::buffer::{BufferId, InsertionType};
use super::gap_buffer::{GAP_BYTES_DFL, GAP_BYTES_MIN};
use super::marker_data::{
    apply_marker_data_delta, marker_data_anchor, marker_data_byte_pos, set_marker_data_anchor,
};
use super::position::{
    CharLen, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange, TextPositionAnchor,
    TextPositionBounds,
};
use super::text::backend::TextBackend;
use super::text::{
    BufferTextBackendKind, BufferTextBytesSnapshot, GapCompatState,
    ImplementedBufferTextBackendKind, TextEditRange, TextExtent, TextExtentDelta, TextInsertion,
    TextMetrics, TextReplacement,
};
#[cfg(test)]
use super::text::{GapDebugLayout, TextBackendDebugLayout};
use super::text_props::{ObjectIntervalRun, PropertyInterval, TextPropertyTable};

#[cfg(test)]
static CHAR_POS_TO_EMACS_BYTE_POS_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_char_pos_to_emacs_byte_pos_call_count() {
    CHAR_POS_TO_EMACS_BYTE_POS_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn char_pos_to_emacs_byte_pos_call_count() -> usize {
    CHAR_POS_TO_EMACS_BYTE_POS_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Last successful char↔byte conversion. Reused on a subsequent query if the
/// buffer text has not changed since the entry was stored. Mirrors GNU
/// `marker.c:202-203` but uses a `BufferText` content epoch rather than
/// `chars_modiff`, so direct `BufferText` tests and non-buffer callers get the
/// same invalidation semantics as full `insdel.rs` edits.
/// One memoized syntax-prop-free run (see
/// `syntax_prop_free_run_end_at_char_pos`). `epoch == 0` never matches a live
/// buffer epoch, so the default entry is inert.
#[derive(Clone, Copy, Default)]
struct SyntaxRunMemoEntry {
    epoch: u64,
    tick: u64,
    start: CharPos0,
    end: CharPos0,
}

/// Byte-coordinate twin of [`SyntaxRunMemoEntry`], carrying the RESOLVED
/// `syntax-table` value for the run so the byte-addressed scanners' refill
/// needs no byte<->char conversions and no interval lookup on a hit. The
/// value bits are only handed out while (epoch, tick) match, and a live
/// value is rooted by the interval plist it came from.
#[derive(Clone, Copy, Default)]
struct SyntaxByteRunMemoEntry {
    epoch: u64,
    tick: u64,
    start: u64,
    end: u64,
    value_bits: u64,
    value_present: bool,
}

/// Char-coordinate twin of [`SyntaxByteRunMemoEntry`], for the char-indexed
/// parse loop's `SyntaxPropRange`: a hit hands the resolved run to a fresh
/// scan with no interval descent and no coalescing walk. Same soundness
/// contract: value bits only leave while (epoch, tick) match, and a live
/// value is rooted by the interval plist it came from.
#[derive(Clone, Copy, Default)]
struct SyntaxCharRunMemoEntry {
    epoch: u64,
    tick: u64,
    start: u64,
    end: u64,
    value_bits: u64,
    value_present: bool,
}

/// One content mutation, in the terms the char<->byte position anchors need to
/// survive it: the edited span's start (byte, and char when the span is
/// non-empty) and the old/new extents. See
/// `BufferText::finish_backend_content_mutation_with_edit`.
#[derive(Clone, Copy, Debug)]
struct PositionEdit {
    at_byte: EmacsBytePos,
    /// `None` for a pure insertion (the anchors never need the char position
    /// of an empty span: nothing ends there).
    at_char: Option<CharPos0>,
    old: TextExtent,
    new: TextExtent,
}

impl PositionEdit {
    fn insert(at_byte: EmacsBytePos, extent: TextExtent) -> Self {
        Self {
            at_byte,
            at_char: None,
            old: TextExtent::ZERO,
            new: extent,
        }
    }

    fn replace(old_range: TextEditRange, new: TextExtent) -> Self {
        Self {
            at_byte: old_range.byte_start(),
            at_char: Some(old_range.char_start()),
            old: TextExtent::new(
                old_range
                    .char_end()
                    .saturating_offset_from(old_range.char_start()),
                old_range.byte_len(),
            ),
            new,
        }
    }

    /// Where `anchor` lands after this edit; `None` if it pointed inside the
    /// replaced span (its coordinates no longer name a position).
    fn adjust(&self, anchor: TextPositionAnchor) -> Option<TextPositionAnchor> {
        let a_byte = anchor.emacs_byte_pos().get();
        let at = self.at_byte.get();
        let old_end = at + self.old.emacs_bytes().get();
        if a_byte <= at {
            // Before the span, or exactly at its start (GNU: a marker at the
            // insertion point stays before the inserted text).
            return Some(anchor);
        }
        if a_byte < old_end {
            return None;
        }
        if a_byte == old_end {
            // At the end of a non-empty replaced span: it now names the end of
            // the replacement.
            let at_char = self.at_char?;
            return Some(TextPositionAnchor::new(
                at_char.add_len(self.new.chars()),
                self.at_byte.add_len(self.new.emacs_bytes()),
            ));
        }
        // Strictly after: shift by the delta (signed).
        let chars = (anchor.char_pos().get() as i64 + self.new.chars().get() as i64
            - self.old.chars().get() as i64) as usize;
        let bytes = (a_byte as i64 + self.new.emacs_bytes().get() as i64
            - self.old.emacs_bytes().get() as i64) as usize;
        Some(TextPositionAnchor::new(
            CharPos0::new(chars),
            EmacsBytePos::new(bytes),
        ))
    }
}

#[derive(Clone, Copy, Default)]
struct PositionCache {
    /// BufferText content epoch when this entry was stored. Zero = invalid.
    epoch: u64,
    anchor: TextPositionAnchor,
}

impl PositionCache {
    fn is_valid_for(self, content_epoch: u64) -> bool {
        self.epoch != 0 && self.epoch == content_epoch
    }
}

struct BufferTextStorage {
    metrics: TextMetrics,
    backend: TextBackend,
    content_epoch: u64,
    virtual_gap: GapCompatState,
    modified_tick: i64,
    chars_modified_tick: i64,
    /// Tracks `modified_tick` at the last text-PROPERTY change (GNU has no
    /// direct analog; mirrors `chars_modified_tick` for char edits). Lets
    /// redisplay distinguish "appearance changed" (face/display/invisible
    /// props) from "text changed" without conflating the two. Bumped only via
    /// [`BufferText::record_text_property_modification`].
    props_modified_tick: i64,
    /// GNU `BEG_UNCHANGED` analog (incremental-layout Phase 3, spec §6): the
    /// number of chars at the buffer START that are unchanged since the last
    /// redisplay ack. `i64::MAX` means "fully unchanged" (no edit since reset).
    /// Composed by `min` across edits; survives position shifts because it is a
    /// COUNT from the end, not an absolute position.
    beg_unchanged: i64,
    /// GNU `END_UNCHANGED` analog: chars at the buffer END unchanged since ack.
    end_unchanged: i64,
    save_modified_tick: i64,
    /// Copy-on-write text-property table. Shared via `Rc` so a layout snapshot
    /// (`BufferText::clone` / `Buffer::text_snapshot`, taken every redisplay)
    /// is O(1) instead of deep-copying the whole interval tree per frame.
    /// Mutations go through `Rc::make_mut`, which copies only while a snapshot
    /// still holds the old table -- and snapshots are dropped before the next
    /// mutation, so it copies ~never. Because you cannot obtain `&mut
    /// TextPropertyTable` from `&Rc<_>` except via `make_mut`, the compiler
    /// forces every mutation site to opt in; no site can silently share-mutate.
    text_props: Rc<TextPropertyTable>,
    /// Head of the intrusive per-buffer marker chain (GNU `buffer->own_text.markers`).
    /// Authoritative since T6; the parallel `Vec<MarkerEntry>` was deleted in T7.
    markers_head: *mut crate::tagged::header::MarkerObj,
    /// Interior-mutable last-query cache for char↔byte conversion.
    pos_cache: Cell<PositionCache>,
    /// Internal (non-Lisp-visible) anchor positions populated on long scans.
    /// Invalidated wholesale when the content epoch advances.
    anchor_cache: RefCell<Vec<TextPositionAnchor>>,
    /// Content epoch at which the anchor_cache is valid.
    /// Mismatch triggers a wholesale clear on next read.
    anchor_cache_key: Cell<u64>,
    /// Round-robin replacement cursor for the anchor ring.
    anchor_cache_cursor: Cell<usize>,
    /// Memo ring for `syntax_prop_free_run_end_at_char_pos` (see its doc).
    syntax_run_memo: RefCell<[SyntaxRunMemoEntry; 4]>,
    syntax_run_memo_cursor: Cell<usize>,
    /// Byte-coordinate run memo for the byte-addressed syntax scanners.
    syntax_byte_run_memo: RefCell<[SyntaxByteRunMemoEntry; 4]>,
    syntax_byte_run_memo_cursor: Cell<usize>,
    /// Char-coordinate resolved-run memo for the parse loop's prop cache.
    syntax_char_run_memo: RefCell<[SyntaxCharRunMemoEntry; 4]>,
    syntax_char_run_memo_cursor: Cell<usize>,
}

impl BufferTextStorage {
    fn gap_compat_state(&self) -> GapCompatState {
        self.backend
            .real_gap_compat_state()
            .unwrap_or(self.virtual_gap)
    }

    fn uses_virtual_gap_compat_state(&self) -> bool {
        self.backend.real_gap_compat_state().is_none()
    }
}

fn emacs_multibyte_candidate_len(lead: u8) -> usize {
    if lead < 0x80 || (0x80..0xC0).contains(&lead) {
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else if lead < 0xF8 {
        4
    } else if lead == 0xF8 {
        crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH
    } else {
        1
    }
}

fn multibyte_chunk_contains_char_code(chunk: &[u8], code: u32, carry: &mut Vec<u8>) -> bool {
    let mut pos = 0;
    if !carry.is_empty() {
        let expected = emacs_multibyte_candidate_len(carry[0]);
        let take = expected.saturating_sub(carry.len()).min(chunk.len());
        carry.extend_from_slice(&chunk[..take]);
        pos += take;
        if carry.len() < expected {
            return false;
        }

        let (decoded, len) = crate::emacs_core::emacs_char::string_char(carry);
        if decoded == code {
            return true;
        }
        if len < carry.len() {
            let remaining = carry[len..].to_vec();
            carry.clear();
            if multibyte_chunk_contains_char_code(&remaining, code, carry) {
                return true;
            }
            if !carry.is_empty() {
                if multibyte_chunk_contains_char_code(&chunk[pos..], code, carry) {
                    return true;
                }
                return false;
            }
        } else {
            carry.clear();
        }
    }

    while pos < chunk.len() {
        let expected = emacs_multibyte_candidate_len(chunk[pos]);
        let available = chunk.len() - pos;
        if available < expected {
            carry.extend_from_slice(&chunk[pos..]);
            return false;
        }

        let (decoded, len) =
            crate::emacs_core::emacs_char::string_char(&chunk[pos..pos + expected]);
        if decoded == code {
            return true;
        }
        pos += len;
    }
    false
}

impl Clone for BufferTextStorage {
    fn clone(&self) -> Self {
        Self {
            metrics: self.metrics,
            backend: self.backend.clone(),
            content_epoch: self.content_epoch,
            virtual_gap: self.virtual_gap,
            modified_tick: self.modified_tick,
            chars_modified_tick: self.chars_modified_tick,
            props_modified_tick: self.props_modified_tick,
            beg_unchanged: self.beg_unchanged,
            end_unchanged: self.end_unchanged,
            save_modified_tick: self.save_modified_tick,
            text_props: self.text_props.clone(),
            // Chain head intentionally not cloned: chain pointers are unique
            // per TaggedHeap; a cloned buffer starts with an empty chain and
            // rebuilds it via register_marker.
            markers_head: std::ptr::null_mut(),
            pos_cache: self.pos_cache.clone(),
            anchor_cache: self.anchor_cache.clone(),
            anchor_cache_key: self.anchor_cache_key.clone(),
            anchor_cache_cursor: self.anchor_cache_cursor.clone(),
            syntax_run_memo: self.syntax_run_memo.clone(),
            syntax_run_memo_cursor: self.syntax_run_memo_cursor.clone(),
            syntax_byte_run_memo: self.syntax_byte_run_memo.clone(),
            syntax_byte_run_memo_cursor: self.syntax_byte_run_memo_cursor.clone(),
            syntax_char_run_memo: self.syntax_char_run_memo.clone(),
            syntax_char_run_memo_cursor: self.syntax_char_run_memo_cursor.clone(),
        }
    }
}

pub struct BufferText {
    storage: Rc<RefCell<BufferTextStorage>>,
}

impl Clone for BufferText {
    fn clone(&self) -> Self {
        let storage = self.storage.borrow().clone();
        Self {
            storage: Rc::new(RefCell::new(storage)),
        }
    }
}

impl Default for BufferText {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferText {
    fn gap_compat_at_text_end(metrics: TextMetrics, byte_len: EmacsByteLen) -> GapCompatState {
        GapCompatState::new(metrics.char_end(), byte_len)
    }

    fn initial_virtual_gap_for_backend(
        backend: &TextBackend,
        metrics: TextMetrics,
        non_gap_size: EmacsByteLen,
    ) -> GapCompatState {
        backend
            .real_gap_compat_state()
            .unwrap_or_else(|| Self::gap_compat_at_text_end(metrics, non_gap_size))
    }

    fn from_backend(backend: TextBackend) -> Self {
        let metrics = backend.metrics();
        let virtual_gap = Self::initial_virtual_gap_for_backend(
            &backend,
            metrics,
            EmacsByteLen::new(GAP_BYTES_MIN),
        );
        Self {
            storage: Rc::new(RefCell::new(BufferTextStorage {
                metrics,
                backend,
                content_epoch: 1,
                virtual_gap,
                modified_tick: 1,
                chars_modified_tick: 1,
                props_modified_tick: 1,
                beg_unchanged: i64::MAX,
                end_unchanged: i64::MAX,
                save_modified_tick: 1,
                text_props: Rc::new(TextPropertyTable::new()),
                markers_head: std::ptr::null_mut(),
                pos_cache: Cell::new(PositionCache::default()),
                anchor_cache: RefCell::new(Vec::new()),
                anchor_cache_key: Cell::new(0),
                anchor_cache_cursor: Cell::new(0),
                syntax_run_memo: RefCell::new([SyntaxRunMemoEntry::default(); 4]),
                syntax_run_memo_cursor: Cell::new(0),
                syntax_byte_run_memo: RefCell::new([SyntaxByteRunMemoEntry::default(); 4]),
                syntax_byte_run_memo_cursor: Cell::new(0),
                syntax_char_run_memo: RefCell::new([SyntaxCharRunMemoEntry::default(); 4]),
                syntax_char_run_memo_cursor: Cell::new(0),
            })),
        }
    }

    /// Test-only: how many scan anchors the position cache currently holds.
    #[cfg(test)]
    pub(crate) fn scan_anchor_ring_len_for_test(&self) -> usize {
        self.storage.borrow().anchor_cache.borrow().len()
    }

    fn refresh_backend_metrics(storage: &mut BufferTextStorage) {
        storage.metrics = storage.backend.metrics();
    }

    fn finish_backend_content_mutation(storage: &mut BufferTextStorage) {
        Self::refresh_backend_metrics(storage);
        storage.content_epoch = storage.content_epoch.wrapping_add(1).max(1);
        Self::invalidate_position_caches(storage);
    }

    /// Like [`Self::finish_backend_content_mutation`], but for a mutation whose
    /// shape is known: the position anchors (the char<->byte cache) are
    /// ADJUSTED like markers instead of dropped. Anchors before the edited
    /// span keep their coordinates, anchors after it shift by the span's
    /// char/byte delta, anchors inside the replaced span are forgotten. GNU
    /// keeps `buf_charpos_to_bytepos` cheap across edits exactly this way (its
    /// cache is the marker chain, which `adjust_markers_for_insert/delete`
    /// move); wholesale invalidation made every syntax scan after a single
    /// inserted character re-walk thousands of bytes (`scan_backward` was 2.4%
    /// of the type-sim window for 189 `char_at` calls). The syntax-run memos
    /// are content-dependent and are still invalidated.
    fn finish_backend_content_mutation_with_edit(
        storage: &mut BufferTextStorage,
        edit: PositionEdit,
    ) {
        Self::refresh_backend_metrics(storage);
        storage.content_epoch = storage.content_epoch.wrapping_add(1).max(1);
        let epoch = storage.content_epoch;
        *storage.syntax_run_memo.borrow_mut() = [SyntaxRunMemoEntry::default(); 4];
        *storage.syntax_byte_run_memo.borrow_mut() = [SyntaxByteRunMemoEntry::default(); 4];
        *storage.syntax_char_run_memo.borrow_mut() = [SyntaxCharRunMemoEntry::default(); 4];
        // The single most-recent-position cache.
        let cached = storage.pos_cache.get();
        storage.pos_cache.set(match edit.adjust(cached.anchor) {
            Some(anchor) if cached.epoch != 0 => PositionCache { epoch, anchor },
            _ => PositionCache::default(),
        });
        // The scan-anchor ring (valid only if it was keyed to the previous
        // epoch; otherwise it is stale for an older reason and must clear).
        let ring_was_current =
            storage.anchor_cache_key.get() == epoch.wrapping_sub(1) && epoch.wrapping_sub(1) != 0;
        let mut ring = storage.anchor_cache.borrow_mut();
        if ring_was_current {
            ring.retain_mut(|anchor| match edit.adjust(*anchor) {
                Some(moved) => {
                    *anchor = moved;
                    true
                }
                None => false,
            });
            let len = ring.len();
            if storage.anchor_cache_cursor.get() >= len {
                storage.anchor_cache_cursor.set(0);
            }
            drop(ring);
            storage.anchor_cache_key.set(epoch);
        } else {
            ring.clear();
            drop(ring);
            storage.anchor_cache_key.set(0);
        }
    }

    fn finish_backend_shape_change(storage: &mut BufferTextStorage) {
        Self::refresh_backend_metrics(storage);
        Self::invalidate_position_caches(storage);
    }

    fn virtual_gap_consume_bytes(storage: &mut BufferTextStorage, bytes: EmacsByteLen) {
        let bytes = bytes.get();
        let mut size = storage.virtual_gap.byte_len().get();
        if size < bytes {
            let need = bytes - size;
            size += need.saturating_add(GAP_BYTES_DFL);
        }
        storage.virtual_gap = storage
            .virtual_gap
            .with_byte_len(EmacsByteLen::new(size.saturating_sub(bytes)));
    }

    fn note_virtual_gap_insert(
        storage: &mut BufferTextStorage,
        pos: EmacsBytePos,
        extent: TextExtent,
    ) {
        if !storage.uses_virtual_gap_compat_state() {
            return;
        }
        let start_char = storage.backend.emacs_byte_pos_to_char_pos(pos);
        storage.virtual_gap = storage
            .virtual_gap
            .with_pos(start_char.add_len(extent.chars()));
        Self::virtual_gap_consume_bytes(storage, extent.emacs_bytes());
    }

    fn note_virtual_gap_delete(storage: &mut BufferTextStorage, range: TextEditRange) {
        if !storage.uses_virtual_gap_compat_state() {
            return;
        }
        storage.virtual_gap = GapCompatState::new(
            range.char_start(),
            storage.virtual_gap.byte_len().add_len(range.byte_len()),
        );
    }

    fn note_virtual_gap_replace(storage: &mut BufferTextStorage, replacement: TextReplacement) {
        if !storage.uses_virtual_gap_compat_state() {
            return;
        }
        let char_start = replacement.old_range().char_start();
        storage.virtual_gap = GapCompatState::new(
            char_start.add_len(replacement.new_extent().chars()),
            storage
                .virtual_gap
                .byte_len()
                .add_len(replacement.old_range().byte_len()),
        );
        Self::virtual_gap_consume_bytes(storage, replacement.new_extent().emacs_bytes());
    }

    fn note_virtual_gap_same_len_replace(
        storage: &mut BufferTextStorage,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        if !storage.uses_virtual_gap_compat_state() {
            return;
        }
        let old_range = replacement.old_range();
        let gap_byte = storage
            .backend
            .char_pos_to_emacs_byte_pos(storage.virtual_gap.pos());
        let before_gap_len = if old_range.byte_start() < gap_byte {
            old_range
                .byte_end()
                .min(gap_byte)
                .saturating_offset_from(old_range.byte_start())
        } else {
            EmacsByteLen::ZERO
        };
        if before_gap_len.is_empty() {
            return;
        }

        let split_char = if before_gap_len == old_range.byte_len() {
            old_range.char_end()
        } else {
            storage
                .backend
                .emacs_byte_pos_to_char_pos(old_range.byte_start().add_len(before_gap_len))
        };
        let old_before_chars = split_char.saturating_offset_from(old_range.char_start());
        let new_before_chars = TextExtent::from_emacs_bytes(
            &bytes[..before_gap_len.get()],
            storage.backend.is_multibyte(),
        )
        .chars();
        if old_before_chars != new_before_chars {
            let delta = new_before_chars.get() as isize - old_before_chars.get() as isize;
            let pos = storage.virtual_gap.pos().get().saturating_add_signed(delta);
            storage.virtual_gap = storage.virtual_gap.with_pos(CharPos0::new(pos));
        }
    }

    fn invalidate_position_caches(storage: &mut BufferTextStorage) {
        storage.pos_cache.set(PositionCache::default());
        storage.anchor_cache.borrow_mut().clear();
        storage.anchor_cache_key.set(0);
        // Default entries have epoch 0, which never matches a live epoch.
        *storage.syntax_run_memo.borrow_mut() = [SyntaxRunMemoEntry::default(); 4];
        *storage.syntax_byte_run_memo.borrow_mut() = [SyntaxByteRunMemoEntry::default(); 4];
        *storage.syntax_char_run_memo.borrow_mut() = [SyntaxCharRunMemoEntry::default(); 4];
    }

    fn byte_range_to_char_range_with_storage(
        storage: &BufferTextStorage,
        range: EmacsByteRange,
    ) -> CharRange {
        CharRange::new(
            storage.backend.emacs_byte_pos_to_char_pos(range.start()),
            storage.backend.emacs_byte_pos_to_char_pos(range.end()),
        )
    }

    pub(crate) fn byte_range_to_char_range(&self, range: EmacsByteRange) -> CharRange {
        // Route through the anchored conversion (pos cache + stride anchors +
        // marker chain + CONSIDER interpolation), NOT the raw backend: the
        // backend path re-scans from its own sparse {0, gap, end} anchors,
        // and this helper sits under every byte-addressed text-property
        // query. The `_with_storage` variant remains for callers already
        // holding the storage borrow (edit-path bookkeeping).
        CharRange::new(
            self.emacs_byte_pos_to_char_pos(range.start()),
            self.emacs_byte_pos_to_char_pos(range.end()),
        )
    }

    pub(crate) fn edit_range_for_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
    ) -> TextEditRange {
        let char_range = self.byte_range_to_char_range(byte_range);
        TextEditRange::from_start_end(
            TextPositionAnchor::new(char_range.start(), byte_range.start()),
            TextPositionAnchor::new(char_range.end(), byte_range.end()),
        )
    }

    pub(crate) fn edit_range_for_char_range(&self, char_range: CharRange) -> TextEditRange {
        let byte_range = EmacsByteRange::new(
            self.char_pos_to_emacs_byte_pos(char_range.start()),
            self.char_pos_to_emacs_byte_pos(char_range.end()),
        );
        TextEditRange::from_start_end(
            TextPositionAnchor::new(char_range.start(), byte_range.start()),
            TextPositionAnchor::new(char_range.end(), byte_range.end()),
        )
    }

    pub(crate) fn edit_range_at_emacs_byte_pos(&self, byte_pos: EmacsBytePos) -> TextEditRange {
        let char_pos = self.emacs_byte_pos_to_char_pos(byte_pos);
        TextEditRange::empty_at(byte_pos, char_pos)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn insertion_at_emacs_byte_pos(
        &self,
        byte_pos: EmacsBytePos,
        extent: TextExtent,
    ) -> TextInsertion {
        TextInsertion::at_anchor(
            TextPositionAnchor::new(self.emacs_byte_pos_to_char_pos(byte_pos), byte_pos),
            extent,
        )
    }

    pub fn new() -> Self {
        Self::from_backend(TextBackend::new(
            ImplementedBufferTextBackendKind::GAP_BUFFER,
        ))
    }

    pub fn try_new_with_backend_kind(
        kind: BufferTextBackendKind,
    ) -> Result<Self, BufferTextBackendKind> {
        Ok(Self::new_with_backend_kind(kind.try_into()?))
    }

    pub(crate) fn new_with_backend_kind(kind: ImplementedBufferTextBackendKind) -> Self {
        Self::from_backend(TextBackend::new(kind))
    }

    pub fn from_str(text: &str) -> Self {
        Self::from_backend(TextBackend::from_str(
            text,
            ImplementedBufferTextBackendKind::GAP_BUFFER,
        ))
    }

    pub fn try_from_str_with_backend_kind(
        text: &str,
        kind: BufferTextBackendKind,
    ) -> Result<Self, BufferTextBackendKind> {
        Ok(Self::from_str_with_backend_kind(text, kind.try_into()?))
    }

    pub(crate) fn from_str_with_backend_kind(
        text: &str,
        kind: ImplementedBufferTextBackendKind,
    ) -> Self {
        Self::from_backend(TextBackend::from_str(text, kind))
    }

    pub fn from_lisp_string(text: &crate::heap_types::LispString) -> Self {
        Self::from_lisp_string_with_backend_kind(text, ImplementedBufferTextBackendKind::GAP_BUFFER)
    }

    pub fn try_from_lisp_string_with_backend_kind(
        text: &crate::heap_types::LispString,
        kind: BufferTextBackendKind,
    ) -> Result<Self, BufferTextBackendKind> {
        Ok(Self::from_lisp_string_with_backend_kind(
            text,
            kind.try_into()?,
        ))
    }

    pub(crate) fn from_lisp_string_with_backend_kind(
        text: &crate::heap_types::LispString,
        kind: ImplementedBufferTextBackendKind,
    ) -> Self {
        Self::from_backend(TextBackend::from_emacs_bytes(
            text.as_bytes(),
            text.is_multibyte(),
            kind,
        ))
    }

    pub fn backend_kind(&self) -> BufferTextBackendKind {
        self.storage.borrow().backend.kind().public_kind()
    }

    pub(crate) fn implemented_backend_kind(&self) -> ImplementedBufferTextBackendKind {
        self.storage.borrow().backend.kind()
    }

    pub fn try_convert_backend_kind(
        &self,
        kind: BufferTextBackendKind,
    ) -> Result<(), BufferTextBackendKind> {
        self.convert_backend_kind(kind.try_into()?);
        Ok(())
    }

    pub(crate) fn convert_backend_kind(&self, kind: ImplementedBufferTextBackendKind) {
        let mut storage = self.storage.borrow_mut();
        if storage.backend.kind() == kind {
            return;
        }
        let gap_compat = storage.gap_compat_state();
        let snapshot = storage.backend.snapshot();
        storage.backend =
            TextBackend::from_snapshot_with_gap_compat_state(snapshot, kind, gap_compat);
        storage.virtual_gap = gap_compat;
        Self::finish_backend_shape_change(&mut storage);
    }

    pub fn is_multibyte(&self) -> bool {
        self.storage.borrow().backend.is_multibyte()
    }

    pub fn set_multibyte(&self, multibyte: bool) {
        let mut storage = self.storage.borrow_mut();
        if storage.backend.is_multibyte() == multibyte {
            return;
        }
        storage.backend.set_multibyte(multibyte);
        Self::finish_backend_content_mutation(&mut storage);
    }

    pub fn is_empty(&self) -> bool {
        self.storage.borrow().metrics.is_empty()
    }

    pub fn char_count(&self) -> CharLen {
        self.storage.borrow().metrics.char_len()
    }

    pub fn emacs_byte_len(&self) -> EmacsByteLen {
        self.storage.borrow().metrics.emacs_byte_len()
    }

    pub fn metrics(&self) -> TextMetrics {
        self.storage.borrow().metrics
    }

    pub(crate) fn gap_position_lisp(&self) -> i64 {
        self.storage.borrow().gap_compat_state().lisp_position()
    }

    pub(crate) fn gap_size_lisp(&self) -> i64 {
        self.storage.borrow().gap_compat_state().lisp_size()
    }

    #[cfg(test)]
    pub(in crate::buffer) fn backend_debug_layout(&self) -> TextBackendDebugLayout {
        self.storage.borrow().backend.debug_layout()
    }

    #[cfg(test)]
    pub(in crate::buffer) fn gap_debug_layout(&self) -> Option<GapDebugLayout> {
        self.storage.borrow().backend.debug_layout().gap()
    }

    pub fn modified_tick(&self) -> i64 {
        self.storage.borrow().modified_tick
    }

    pub fn chars_modified_tick(&self) -> i64 {
        self.storage.borrow().chars_modified_tick
    }

    pub fn props_modified_tick(&self) -> i64 {
        self.storage.borrow().props_modified_tick
    }

    pub fn save_modified_tick(&self) -> i64 {
        self.storage.borrow().save_modified_tick
    }

    pub fn byte_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> u8 {
        self.storage.borrow().backend.byte_at_emacs_byte_pos(pos)
    }

    pub fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.storage.borrow().backend.emacs_byte_at_pos(pos)
    }

    pub(crate) fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        self.storage.borrow().backend.char_at_emacs_byte_pos(pos)
    }

    pub(crate) fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        self.storage
            .borrow()
            .backend
            .char_code_at_emacs_byte_pos(pos)
    }

    /// See `PhysicalTextBackend::contiguous_window_at`. The returned pointer
    /// outlives the internal borrow taken here and is valid only until the
    /// next text mutation; callers must confine it to a pure-read scan.
    pub(crate) fn contiguous_window_at(&self, pos: usize) -> Option<(usize, *const u8, usize)> {
        self.storage.borrow().backend.contiguous_window_at(pos)
    }

    pub(crate) fn text_emacs_byte_range(&self, range: EmacsByteRange) -> String {
        self.storage.borrow().backend.text_emacs_byte_range(range)
    }

    pub(crate) fn full_text_string(&self) -> String {
        self.text_emacs_byte_range(EmacsByteRange::from_start_len(
            EmacsBytePos::ZERO,
            self.emacs_byte_len(),
        ))
    }

    pub fn copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
        self.storage
            .borrow()
            .backend
            .copy_emacs_byte_range_to(range, out);
    }

    pub(crate) fn for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.storage
            .borrow()
            .backend
            .for_each_emacs_byte_range_chunk(range, f)
    }

    pub(crate) fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool {
        self.storage
            .borrow()
            .backend
            .has_contiguous_emacs_byte_range(range)
    }

    /// See [`TextBackend::try_make_emacs_byte_range_contiguous`].
    /// Logically const — text content and every logical position are
    /// unchanged; only the gap backend's physical gap placement moves
    /// (like GNU `move_gap`, which runs freely during "read-only"
    /// searches).
    pub(crate) fn try_make_emacs_byte_range_contiguous(&self, range: EmacsByteRange) -> bool {
        self.storage
            .borrow_mut()
            .backend
            .try_make_emacs_byte_range_contiguous(range)
    }

    pub(crate) fn with_contiguous_emacs_byte_range<R>(
        &self,
        range: EmacsByteRange,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        self.storage
            .borrow()
            .backend
            .with_contiguous_emacs_byte_range(range, f)
    }

    fn emacs_byte_end_pos(&self) -> EmacsBytePos {
        EmacsBytePos::ZERO.add_len(self.storage.borrow().metrics.emacs_byte_len())
    }

    /// First `\n` in the logical emacs-byte range `[from, limit)`, or `None`.
    /// Scans the backend's contiguous chunks (gap segments) in place with no
    /// copy and stops at the first match, mirroring GNU `find_newline`
    /// (search.c) scanning forward between `BUFFER_CEILING_OF` boundaries.
    pub(crate) fn next_newline_emacs_byte(
        &self,
        from: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let total = self.emacs_byte_end_pos();
        let from = from.min(total);
        let limit = limit.min(total);
        if from >= limit {
            return None;
        }
        let mut base = from;
        let mut found = None;
        let _ =
            self.for_each_emacs_byte_range_chunk::<()>(EmacsByteRange::new(from, limit), |chunk| {
                match chunk.iter().position(|&b| b == b'\n') {
                    Some(off) => {
                        found = Some(base.add_len(EmacsByteLen::new(off)));
                        Err(())
                    }
                    None => {
                        base = base.add_len(EmacsByteLen::new(chunk.len()));
                        Ok(())
                    }
                }
            });
        found
    }

    /// Last `\n` in the logical emacs-byte range `[floor, from)`, or `None`.
    /// Uses a backward galloping window so the work is O(distance back to the
    /// newline), not O(from) -- the in-place analog of GNU `find_newline`
    /// scanning backward with `memrchr`.  Each window is bulk-scanned.
    pub(crate) fn prev_newline_emacs_byte(
        &self,
        from: EmacsBytePos,
        floor: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let total = self.emacs_byte_end_pos();
        let from = from.min(total);
        let floor = floor.min(total);
        if from <= floor {
            return None;
        }
        let mut hi = from;
        let mut window = EmacsByteLen::new(256);
        while hi > floor {
            let lo = hi.saturating_sub_len(window).max(floor);
            let mut base = lo;
            let mut last = None;
            let _ =
                self.for_each_emacs_byte_range_chunk::<()>(EmacsByteRange::new(lo, hi), |chunk| {
                    if let Some(off) = memchr::memrchr(b'\n', chunk) {
                        last = Some(base.add_len(EmacsByteLen::new(off)));
                    }
                    base = base.add_len(EmacsByteLen::new(chunk.len()));
                    Ok::<(), ()>(())
                });
            if last.is_some() {
                return last;
            }
            if lo == floor {
                return None;
            }
            hi = lo;
            window = window.add_len(window);
        }
        None
    }

    /// Number of `\n` in the logical emacs-byte range `[from, limit)`, counted
    /// over the backend's contiguous chunks with no copy.
    pub(crate) fn count_newlines_emacs_byte(
        &self,
        from: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> usize {
        let total = self.emacs_byte_end_pos();
        let from = from.min(total);
        let limit = limit.min(total);
        if from >= limit {
            return 0;
        }
        let mut count = 0usize;
        let _ =
            self.for_each_emacs_byte_range_chunk::<()>(EmacsByteRange::new(from, limit), |chunk| {
                count += memchr::memchr_iter(b'\n', chunk).count();
                Ok::<(), ()>(())
            });
        count
    }

    pub(crate) fn insert_measured_emacs_bytes(
        &mut self,
        pos: EmacsBytePos,
        bytes: &[u8],
        extent: TextExtent,
    ) {
        if bytes.is_empty() {
            return;
        }
        let mut storage = self.storage.borrow_mut();
        Self::note_virtual_gap_insert(&mut storage, pos, extent);
        storage
            .backend
            .insert_measured_emacs_bytes(pos, bytes, extent);
        Self::finish_backend_content_mutation_with_edit(
            &mut storage,
            PositionEdit::insert(pos, extent),
        );
    }

    pub(crate) fn delete_measured_range(&mut self, range: TextEditRange) {
        if range.is_empty() {
            return;
        }
        let mut storage = self.storage.borrow_mut();
        Self::note_virtual_gap_delete(&mut storage, range);
        storage.backend.delete_measured_range(range);
        Self::finish_backend_content_mutation_with_edit(
            &mut storage,
            PositionEdit::replace(range, TextExtent::ZERO),
        );
    }

    pub(crate) fn replace_measured_range(&mut self, replacement: TextReplacement, bytes: &[u8]) {
        if replacement.old_range().is_empty() && bytes.is_empty() {
            return;
        }
        let mut storage = self.storage.borrow_mut();
        Self::note_virtual_gap_replace(&mut storage, replacement);
        storage.backend.replace_measured_range(replacement, bytes);
        Self::finish_backend_content_mutation_with_edit(
            &mut storage,
            PositionEdit::replace(replacement.old_range(), replacement.new_extent()),
        );
    }

    pub(crate) fn replace_same_len_measured_range(
        &mut self,
        replacement: TextReplacement,
        bytes: &[u8],
    ) {
        if replacement.old_range().is_empty() {
            return;
        }
        assert_eq!(
            replacement.old_byte_len(),
            replacement.new_byte_len(),
            "replace_same_len_range: measured old and new byte lengths must match"
        );
        let mut storage = self.storage.borrow_mut();
        Self::note_virtual_gap_same_len_replace(&mut storage, replacement, bytes);
        storage
            .backend
            .replace_same_len_measured_range(replacement, bytes);
        Self::finish_backend_content_mutation_with_edit(
            &mut storage,
            PositionEdit::replace(replacement.old_range(), replacement.new_extent()),
        );
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn replace_same_len_emacs_byte_range(
        &mut self,
        range: EmacsByteRange,
        replacement: &[u8],
    ) {
        if range.is_empty() {
            return;
        }
        let edit_range = self.edit_range_for_emacs_byte_range(range);
        let new_extent = TextExtent::from_emacs_bytes(replacement, self.is_multibyte());
        self.replace_same_len_measured_range(
            TextReplacement::new(edit_range, new_extent),
            replacement,
        );
    }

    pub fn shared_clone(&self) -> Self {
        Self {
            storage: Rc::clone(&self.storage),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.storage, &other.storage)
    }

    pub(crate) fn dump_text(&self) -> Vec<u8> {
        self.storage.borrow().backend.dump_text()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) fn from_snapshot(snapshot: BufferTextBytesSnapshot) -> Self {
        Self::from_backend(TextBackend::from_snapshot(
            snapshot,
            ImplementedBufferTextBackendKind::GAP_BUFFER,
        ))
    }

    pub(crate) fn from_snapshot_with_backend_kind(
        snapshot: BufferTextBytesSnapshot,
        kind: ImplementedBufferTextBackendKind,
    ) -> Self {
        Self::from_backend(TextBackend::from_snapshot(snapshot, kind))
    }

    pub fn set_modification_state(
        &self,
        modified_tick: i64,
        chars_modified_tick: i64,
        save_modified_tick: i64,
    ) {
        let mut storage = self.storage.borrow_mut();
        storage.modified_tick = modified_tick;
        storage.chars_modified_tick = chars_modified_tick;
        storage.save_modified_tick = save_modified_tick;
    }

    pub fn set_modified_tick(&self, tick: i64) {
        self.storage.borrow_mut().modified_tick = tick;
    }

    pub fn set_save_modified_tick(&self, tick: i64) {
        self.storage.borrow_mut().save_modified_tick = tick;
    }

    pub fn increment_modified_tick(&self, delta: i64) {
        self.storage.borrow_mut().modified_tick += delta;
    }

    pub fn record_char_modification(&self, delta: i64) {
        let mut storage = self.storage.borrow_mut();
        storage.modified_tick += delta;
        storage.chars_modified_tick = storage.modified_tick;
    }

    /// Record a text-PROPERTY modification: advances `modified_tick` and
    /// rejoins `props_modified_tick` to it, leaving `chars_modified_tick`
    /// untouched. The single choke point for the props tick; mirrors
    /// [`Self::record_char_modification`] for char edits.
    pub fn record_text_property_modification(&self) {
        let mut storage = self.storage.borrow_mut();
        storage.modified_tick += 1;
        storage.props_modified_tick = storage.modified_tick;
    }

    /// Compose a changed char region `[start, end)` (OLD positions, before the
    /// edit) into the unchanged-region accumulator (GNU `BUF_COMPUTE_UNCHANGED`,
    /// incremental-layout Phase 3 / spec §6). `old_z` is the buffer's char count
    /// before the edit. The prefix `[0, start)` and suffix `[end, old_z)` remain
    /// unchanged; their lengths compose by `min` across multiple edits, so the
    /// accumulated dirty span is the union of every edit since the last ack.
    pub fn note_changed_char_region(&self, start: i64, end: i64, old_z: i64) {
        let mut storage = self.storage.borrow_mut();
        storage.beg_unchanged = storage.beg_unchanged.min(start.max(0));
        storage.end_unchanged = storage.end_unchanged.min((old_z - end).max(0));
    }

    /// Reset the unchanged-region accumulator to "fully unchanged" — the
    /// redisplay ack, performed at the committed (accepted) layout break, NOT on
    /// a retry/`continue` (which would under-invalidate, spec §6).
    pub fn reset_unchanged_region(&self) {
        let mut storage = self.storage.borrow_mut();
        storage.beg_unchanged = i64::MAX;
        storage.end_unchanged = i64::MAX;
    }

    /// The accumulated dirty char range `[beg, end)` since the last ack, or
    /// `None` when nothing changed. `current_z` is the buffer's CURRENT char
    /// count — the suffix length is measured from the end, so this stays correct
    /// after the edits' position shifts (insertions/deletions).
    pub fn changed_char_range(&self, current_z: i64) -> Option<(i64, i64)> {
        let storage = self.storage.borrow();
        if storage.beg_unchanged == i64::MAX && storage.end_unchanged == i64::MAX {
            return None;
        }
        let start = storage.beg_unchanged.min(current_z).max(0);
        let end = (current_z - storage.end_unchanged.min(current_z)).max(start);
        Some((start, end))
    }

    pub(crate) fn emacs_byte_range_contains_char_code(
        &self,
        range: EmacsByteRange,
        code: u32,
    ) -> bool {
        if range.is_empty() {
            return false;
        }
        // Walk backend chunks directly, matching GNU's split-around-gap scan
        // shape without forcing piece-tree/rope ranges into one temporary
        // byte vector.
        let storage = self.storage.borrow();
        if !storage.backend.is_multibyte() {
            if code > 0xFF {
                return false;
            }
            return storage
                .backend
                .for_each_emacs_byte_range_chunk(range, |chunk| {
                    if chunk.iter().any(|&b| b as u32 == code) {
                        Err(())
                    } else {
                        Ok(())
                    }
                })
                .is_err();
        }

        let mut carry = Vec::with_capacity(crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH);
        if storage
            .backend
            .for_each_emacs_byte_range_chunk(range, |chunk| {
                if multibyte_chunk_contains_char_code(chunk, code, &mut carry) {
                    Err(())
                } else {
                    Ok(())
                }
            })
            .is_err()
        {
            return true;
        }
        !carry.is_empty() && crate::emacs_core::emacs_char::string_char(&carry).0 == code
    }

    pub fn text_props_is_empty(&self) -> bool {
        self.storage.borrow().text_props.is_empty()
    }

    pub fn text_props_snapshot(&self) -> TextPropertyTable {
        // Callers here (transpose-regions, pdump, buffer-copy) want an owned,
        // independent table, so deep-clone the shared inner table -- not the Rc.
        self.storage.borrow().text_props.as_ref().clone()
    }

    pub fn text_props_replace(&self, table: TextPropertyTable) {
        let mut storage = self.storage.borrow_mut();
        storage.text_props = Rc::new(table);
        // A swapped-in table carries its own mutation-tick lineage, which can
        // collide with the memo's stored ticks — drop the memo outright.
        *storage.syntax_run_memo.borrow_mut() = [SyntaxRunMemoEntry::default(); 4];
        *storage.syntax_byte_run_memo.borrow_mut() = [SyntaxByteRunMemoEntry::default(); 4];
        *storage.syntax_char_run_memo.borrow_mut() = [SyntaxCharRunMemoEntry::default(); 4];
    }

    pub fn replace_storage(&self, text: &str, multibyte: bool, text_props: TextPropertyTable) {
        let bytes =
            crate::emacs_core::string_escape::storage_string_to_buffer_bytes(text, multibyte);
        let string = if multibyte {
            crate::heap_types::LispString::from_emacs_bytes(bytes)
        } else {
            crate::heap_types::LispString::from_unibyte(bytes)
        };
        self.replace_lisp_string(&string, text_props);
    }
    pub fn replace_lisp_string(
        &self,
        text: &crate::heap_types::LispString,
        text_props: TextPropertyTable,
    ) {
        let mut storage = self.storage.borrow_mut();
        let kind = storage.backend.kind();
        storage.backend = TextBackend::from_emacs_bytes(text.as_bytes(), text.is_multibyte(), kind);
        Self::finish_backend_content_mutation(&mut storage);
        storage.virtual_gap = Self::initial_virtual_gap_for_backend(
            &storage.backend,
            storage.metrics,
            EmacsByteLen::new(GAP_BYTES_DFL),
        );
        storage.text_props = Rc::new(text_props);
    }

    /// Walk the intrusive marker chain and remap each marker's cached
    /// `(charpos, bytepos)` pair through the caller-supplied closure. GNU
    /// keeps those two coordinates together on `struct Lisp_Marker`; Neomacs
    /// uses `TextPositionAnchor` at this semantic boundary so callers cannot
    /// accidentally update one coordinate without the other.
    pub fn remap_marker_anchors<F>(&self, mut remap: F)
    where
        F: FnMut(TextPositionAnchor) -> TextPositionAnchor,
    {
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: `curr` walks live chain-owned MarkerObj pointers from
        // `markers_head` until null. Each non-null node was spliced in via
        // `chain_splice_at_head`, so its `data.next_marker` is a valid
        // chain link or null.
        unsafe {
            while !curr.is_null() {
                let data = &mut (*curr).data;
                let new_position = remap(marker_data_anchor(data));
                set_marker_data_anchor(data, new_position);
                curr = data.next_marker;
            }
        }
    }

    pub fn text_props_put_property_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
        name: Value,
        value: Value,
    ) -> bool {
        // GNU intervals are character-indexed; BufferText owns the conversion
        // from buffer byte offsets into interval positions.
        let (range, object_len) = {
            let storage = self.storage.borrow();
            (
                Self::byte_range_to_char_range_with_storage(&storage, byte_range),
                storage.metrics.char_len(),
            )
        };
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .put_property_for_object_char_len(range, object_len, name, value)
    }

    pub fn text_props_get_property_at_char_pos(&self, pos: CharPos0, name: Value) -> Option<Value> {
        self.storage
            .borrow()
            .text_props
            .get_property_at_char_pos(pos, name)
    }

    /// Conservative whole-table presence of property `name` (see
    /// [`TextProps::property_name_presence`]).
    pub fn text_props_property_name_presence(
        &self,
        name: Value,
    ) -> super::text_props::PropertyNamePresence {
        self.storage
            .borrow()
            .text_props
            .property_name_presence(name)
    }

    /// See [`text_props::TextPropertyTable::interval_plist_at_char_pos`].
    pub fn interval_plist_at_char_pos(&self, pos: CharPos0) -> Option<Value> {
        self.storage
            .borrow()
            .text_props
            .interval_plist_at_char_pos(pos)
    }

    /// See [`text_props::TextPropertyTable::interval_plist_at_char_pos`].
    pub fn interval_plist_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<Value> {
        let pos = self.byte_range_to_char_range(EmacsByteRange::new(pos, pos));
        self.storage
            .borrow()
            .text_props
            .interval_plist_at_char_pos(pos.start())
    }

    pub fn text_props_get_property_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<Value> {
        let pos = self.byte_range_to_char_range(EmacsByteRange::new(pos, pos));
        self.storage
            .borrow()
            .text_props
            .get_property_at_char_pos(pos.start(), name)
    }

    /// Returns property `name` at char `pos` plus the `[start, end)` char run
    /// over which it is constant.  Lets a per-char scanner cache the run (GNU
    /// `gl_state.b_property` / `e_property`) and avoid an interval lookup on
    /// every char; the bounds come straight from the interval containing
    /// `pos`, so a char-indexed scan range-checks without any conversion.
    pub fn get_property_run_at_char_pos(
        &self,
        pos: CharPos0,
        name: Value,
    ) -> (Option<Value>, CharPos0, CharPos0) {
        let storage = self.storage.borrow();
        let total = storage.metrics.char_len().get();
        storage
            .text_props
            .get_property_run_at_char_pos(pos, name, total)
    }

    /// See [`text_props::TextPropertyTable::interval_plist_run_at_char_pos`].
    pub fn interval_plist_run_at_char_pos(
        &self,
        pos: CharPos0,
    ) -> (Option<Value>, CharPos0, CharPos0) {
        let storage = self.storage.borrow();
        let total = storage.metrics.char_len().get();
        storage
            .text_props
            .interval_plist_run_at_char_pos(pos, total)
    }

    /// Next char position in `(pos, cap)` where any of `keys` changes value.
    /// See [`text_props::TextPropertyTable::next_watched_property_change`].
    pub fn next_watched_property_change_at_char_pos(
        &self,
        pos: CharPos0,
        cap: CharPos0,
        keys: &[Value],
    ) -> CharPos0 {
        self.storage
            .borrow()
            .text_props
            .next_watched_property_change(pos, cap, keys)
    }

    /// See [`crate::buffer::text_props::TextPropertyTable::syntax_prop_free_run_end`].
    ///
    /// Memoized in a small ring keyed by (content epoch, property mutation
    /// tick): a fontification pass runs thousands of scans between edits, and
    /// each scan's per-scan run cache starts cold — without this memo every
    /// scan re-walked the same face-interval chain. A hit may return an end
    /// short of `cap` (the stored walk's own lookahead); that is a valid,
    /// merely conservative answer — the caller just refills again there.
    pub fn syntax_prop_free_run_end_at_char_pos(&self, pos: CharPos0, cap: CharPos0) -> CharPos0 {
        let storage = self.storage.borrow();
        let epoch = storage.content_epoch;
        let tick = storage.text_props.syntax_prop_tick();
        for entry in storage.syntax_run_memo.borrow().iter() {
            if entry.epoch == epoch && entry.tick == tick && pos >= entry.start && pos < entry.end {
                return entry.end.min(cap);
            }
        }
        let end = storage.text_props.syntax_prop_free_run_end(pos, cap);
        if end > pos {
            let mut memo = storage.syntax_run_memo.borrow_mut();
            let slot = storage.syntax_run_memo_cursor.get() % memo.len();
            memo[slot] = SyntaxRunMemoEntry {
                epoch,
                tick,
                start: pos,
                end,
            };
            storage.syntax_run_memo_cursor.set(slot + 1);
        }
        end
    }

    /// Byte-run memo lookup for the byte-addressed syntax scanners: on a hit
    /// the refill needs no conversion and no interval lookup. Returns
    /// `(start_byte, end_byte, resolved_value)`.
    pub fn syntax_byte_run_memo_lookup(
        &self,
        byte_pos: EmacsBytePos,
    ) -> Option<(u64, u64, Option<Value>)> {
        let storage = self.storage.borrow();
        let epoch = storage.content_epoch;
        let tick = storage.text_props.syntax_prop_tick();
        let pos = byte_pos.get() as u64;
        for entry in storage.syntax_byte_run_memo.borrow().iter() {
            if entry.epoch == epoch && entry.tick == tick && pos >= entry.start && pos < entry.end {
                let value = entry
                    .value_present
                    .then(|| Value::from_bits(entry.value_bits as usize));
                return Some((entry.start, entry.end, value));
            }
        }
        None
    }

    /// Store a computed byte run (see `syntax_byte_run_memo_lookup`).
    pub fn syntax_byte_run_memo_store(&self, start: u64, end: u64, value: Option<Value>) {
        if end <= start {
            return;
        }
        let storage = self.storage.borrow();
        let epoch = storage.content_epoch;
        let tick = storage.text_props.syntax_prop_tick();
        let mut memo = storage.syntax_byte_run_memo.borrow_mut();
        let slot = storage.syntax_byte_run_memo_cursor.get() % memo.len();
        memo[slot] = SyntaxByteRunMemoEntry {
            epoch,
            tick,
            start,
            end,
            value_bits: value.map_or(0, |v| v.bits() as u64),
            value_present: value.is_some(),
        };
        storage.syntax_byte_run_memo_cursor.set(slot + 1);
    }

    /// Char-run memo lookup for the char-indexed parse loop (see
    /// [`Self::syntax_byte_run_memo_lookup`] — same contract in char
    /// coordinates). Returns `(start_char, end_char, resolved_value)`.
    pub fn syntax_char_run_memo_lookup(&self, pos: usize) -> Option<(u64, u64, Option<Value>)> {
        let storage = self.storage.borrow();
        let epoch = storage.content_epoch;
        let tick = storage.text_props.syntax_prop_tick();
        let pos = pos as u64;
        for entry in storage.syntax_char_run_memo.borrow().iter() {
            if entry.epoch == epoch && entry.tick == tick && pos >= entry.start && pos < entry.end {
                let value = entry
                    .value_present
                    .then(|| Value::from_bits(entry.value_bits as usize));
                return Some((entry.start, entry.end, value));
            }
        }
        None
    }

    /// Store a computed char run (see [`Self::syntax_char_run_memo_lookup`]).
    pub fn syntax_char_run_memo_store(&self, start: u64, end: u64, value: Option<Value>) {
        if end <= start {
            return;
        }
        let storage = self.storage.borrow();
        let epoch = storage.content_epoch;
        let tick = storage.text_props.syntax_prop_tick();
        let mut memo = storage.syntax_char_run_memo.borrow_mut();
        let slot = storage.syntax_char_run_memo_cursor.get() % memo.len();
        memo[slot] = SyntaxCharRunMemoEntry {
            epoch,
            tick,
            start,
            end,
            value_bits: value.map_or(0, |v| v.bits() as u64),
            value_present: value.is_some(),
        };
        storage.syntax_char_run_memo_cursor.set(slot + 1);
    }

    /// See [`text_props::TextPropertyTable::has_any_non_nil_property_in_char_range`].
    pub fn has_any_non_nil_property_in_char_range(&self, range: CharRange, keys: &[Value]) -> bool {
        self.storage
            .borrow()
            .text_props
            .has_any_non_nil_property_in_char_range(range, keys)
    }

    pub fn text_props_get_properties_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> HashMap<Value, Value> {
        let pos = self.byte_range_to_char_range(EmacsByteRange::new(pos, pos));
        self.storage
            .borrow()
            .text_props
            .get_properties_at_char_pos(pos.start())
    }

    pub fn text_props_get_properties_ordered_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Vec<(Value, Value)> {
        let pos = self.byte_range_to_char_range(EmacsByteRange::new(pos, pos));
        self.storage
            .borrow()
            .text_props
            .get_properties_ordered_at_char_pos(pos.start())
    }

    pub fn text_props_get_properties_plist_value_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Value {
        let pos = self.byte_range_to_char_range(EmacsByteRange::new(pos, pos));
        self.storage
            .borrow()
            .text_props
            .get_properties_plist_value_at_char_pos(pos.start())
    }

    pub fn text_props_range_has_all_properties_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
        properties: &[(Value, Value)],
    ) -> bool {
        let range = self.byte_range_to_char_range(byte_range);
        self.storage
            .borrow()
            .text_props
            .range_has_all_properties_in_char_range(range, properties)
    }

    pub fn text_props_range_has_any_property_named_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
        names: &[Value],
    ) -> bool {
        let range = self.byte_range_to_char_range(byte_range);
        self.storage
            .borrow()
            .text_props
            .range_has_any_property_named_in_char_range(range, names)
    }

    pub fn text_props_range_has_any_interval_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
    ) -> bool {
        let range = self.byte_range_to_char_range(byte_range);
        self.storage
            .borrow()
            .text_props
            .range_has_any_interval_in_char_range(range)
    }

    pub fn text_props_remove_property_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
        name: Value,
    ) -> bool {
        let range = self.byte_range_to_char_range(byte_range);
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .remove_property_in_char_range(range, name)
    }

    pub fn text_props_remove_properties_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
        names: &[Value],
    ) -> bool {
        let range = self.byte_range_to_char_range(byte_range);
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .remove_properties_in_char_range(range, names)
    }

    pub fn text_props_remove_all_in_emacs_byte_range(&self, byte_range: EmacsByteRange) {
        let range = self.byte_range_to_char_range(byte_range);
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .remove_all_properties_in_char_range(range);
    }

    pub fn text_props_set_properties_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
        plist: Vec<(Value, Value)>,
    ) {
        let (range, object_len) = {
            let storage = self.storage.borrow();
            (
                Self::byte_range_to_char_range_with_storage(&storage, byte_range),
                storage.metrics.char_len(),
            )
        };
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .set_properties_for_object_char_len(range, object_len, plist);
    }

    /// Like `text_props_set_properties_in_emacs_byte_range` but takes the char
    /// range directly, skipping the byte->char conversion.  Callers that
    /// already know the char range -- e.g. `insert`, which knows point and the
    /// inserted char length -- should use this to avoid re-deriving it (a
    /// per-insert cost that shows up heavily during byte-compilation).
    pub fn text_props_set_properties_in_char_range(
        &self,
        range: CharRange,
        plist: Vec<(Value, Value)>,
    ) {
        let object_len = self.storage.borrow().metrics.char_len();
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .set_properties_for_object_char_len(range, object_len, plist);
    }

    pub fn text_props_next_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(pos, pos))
            .start();
        let next = {
            self.storage
                .borrow()
                .text_props
                .next_property_change_after_char_pos(char_pos)
        };
        next.map(|next| self.char_pos_to_emacs_byte_pos(next))
    }

    /// Like `text_props_next_change_after_emacs_byte_pos`, but reports only a
    /// change of the single text property `name` (compared by `eq`), matching
    /// the text-property half of GNU `next_single_char_property_change`.
    pub fn text_props_next_single_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(pos, pos))
            .start();
        let next = {
            self.storage
                .borrow()
                .text_props
                .next_single_property_change_after_char_pos(char_pos, name)
        };
        next.map(|next| self.char_pos_to_emacs_byte_pos(next))
    }

    /// Display-engine bounded variant: like
    /// [`Self::text_props_next_single_change_after_emacs_byte_pos`] but stops
    /// scanning at `limit` (an emacs byte position), returning it as a soft
    /// boundary if `name` has not changed by then. Used only by the layout
    /// invisible/display scans; see
    /// `TextPropertyTable::next_single_property_change_after_char_pos_bounded`.
    pub fn text_props_next_single_change_after_emacs_byte_pos_bounded(
        &self,
        pos: EmacsBytePos,
        name: Value,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        // Callers derive `limit` as `pos + distance`, which can run past the
        // buffer end; clamp before converting so byte->char never overflows.
        // A limit at the buffer end means "no cap in range", and the scan then
        // reduces to the exact unbounded answer.
        let mut limit = limit.min(self.emacs_byte_end_pos());
        // The same byte arithmetic can land INSIDE a multibyte character
        // (trailing bytes are 10xxxxxx, GNU CHAR_HEAD_P); converting an
        // unaligned byte position to a char position panics. Snap the soft
        // cap UP to the next character head — up, not down, so the cap never
        // collapses onto `pos` and the caller's re-scan always progresses.
        // The buffer end is always a boundary, so this loop terminates at
        // the clamp above at worst.
        let end = self.emacs_byte_end_pos();
        while limit < end && (self.byte_at_emacs_byte_pos(limit) & 0xC0) == 0x80 {
            limit = EmacsBytePos::new(limit.get() + 1);
        }
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(pos, pos))
            .start();
        let limit_char = self
            .byte_range_to_char_range(EmacsByteRange::new(limit, limit))
            .start();
        let next = {
            self.storage
                .borrow()
                .text_props
                .next_single_property_change_after_char_pos_bounded(char_pos, name, limit_char)
        };
        next.map(|next| self.char_pos_to_emacs_byte_pos(next))
    }

    pub fn text_props_previous_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(pos, pos))
            .start();
        let prev = {
            self.storage
                .borrow()
                .text_props
                .previous_property_change_before_char_pos(char_pos)
        };
        prev.map(|prev| self.char_pos_to_emacs_byte_pos(prev))
    }

    pub fn text_props_previous_single_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(pos, pos))
            .start();
        let previous = self
            .storage
            .borrow()
            .text_props
            .previous_single_property_change_before_char_pos(char_pos, name);
        previous.map(|previous| self.char_pos_to_emacs_byte_pos(previous))
    }

    /// See `TextPropertyTable::for_each_interval_from_char_pos`.  The callback
    /// runs under the storage borrow, so it must not touch this buffer's text.
    pub fn text_props_for_each_interval_from_char_pos<F>(&self, pos: CharPos0, f: F)
    where
        F: FnMut(CharPos0, CharPos0, Value) -> bool,
    {
        self.storage
            .borrow()
            .text_props
            .for_each_interval_from_char_pos(pos, f)
    }

    pub fn text_props_next_interval_boundary_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(pos, pos))
            .start();
        let next = {
            self.storage
                .borrow()
                .text_props
                .next_interval_boundary_after_char_pos(char_pos)
        };
        next.map(|next| self.char_pos_to_emacs_byte_pos(next))
    }

    pub fn text_props_first_interval_pos_with_property_eq_in_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
        name: Value,
        value: Value,
    ) -> Option<EmacsBytePos> {
        let range = self.byte_range_to_char_range(byte_range);
        let pos = self
            .storage
            .borrow()
            .text_props
            .first_interval_pos_with_property_eq_in_char_range(range, name, value)?;
        Some(self.char_pos_to_emacs_byte_pos(pos))
    }

    pub fn text_props_previous_interval_boundary_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(pos, pos))
            .start();
        let prev = {
            self.storage
                .borrow()
                .text_props
                .previous_interval_boundary_before_char_pos(char_pos)
        };
        prev.map(|prev| self.char_pos_to_emacs_byte_pos(prev))
    }

    pub fn text_props_append_shifted_at_emacs_byte_pos(
        &self,
        other: &TextPropertyTable,
        byte_pos: EmacsBytePos,
    ) {
        let char_pos = self
            .byte_range_to_char_range(EmacsByteRange::new(byte_pos, byte_pos))
            .start();
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .append_shifted_at_char_pos(other, char_pos);
    }

    pub fn text_props_merge_missing_shifted_at_emacs_byte_pos(
        &self,
        other: &TextPropertyTable,
        byte_pos: EmacsBytePos,
    ) {
        let char_offset = self
            .byte_range_to_char_range(EmacsByteRange::new(byte_pos, byte_pos))
            .start()
            .get();
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .merge_missing_shifted_at_char_offset(other, CharLen::new(char_offset));
    }

    pub fn text_props_merge_adjacent_equal_around_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
    ) {
        let range = self.byte_range_to_char_range(byte_range);
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .merge_adjacent_equal_properties_around_char_range(range);
    }

    pub fn text_props_slice_emacs_byte_range(
        &self,
        byte_range: EmacsByteRange,
    ) -> TextPropertyTable {
        let range = self.byte_range_to_char_range(byte_range);
        self.storage.borrow().text_props.slice_char_range(range)
    }

    pub fn text_props_intervals_snapshot(&self) -> Vec<PropertyInterval> {
        self.storage.borrow().text_props.intervals_snapshot()
    }

    pub fn text_props_object_interval_runs(&self, len: CharLen) -> Vec<ObjectIntervalRun> {
        self.storage
            .borrow()
            .text_props
            .object_interval_runs_for_char_len(len)
    }

    pub(crate) fn text_props_try_for_each_interval_in_emacs_byte_range<E>(
        &self,
        byte_range: EmacsByteRange,
        f: impl FnMut(CharRange, &[(Value, Value)]) -> Result<(), E>,
    ) -> Result<(), E> {
        let range = self.byte_range_to_char_range(byte_range);
        self.storage
            .borrow()
            .text_props
            .try_for_each_interval_in_char_range(range, f)
    }

    /// Plist-Value twin of the above — no per-interval pair-slice
    /// materialization (see `try_for_each_interval_plist_in_char_range`).
    pub(crate) fn text_props_try_for_each_interval_plist_in_emacs_byte_range<E>(
        &self,
        byte_range: EmacsByteRange,
        f: impl FnMut(CharRange, Value) -> Result<(), E>,
    ) -> Result<(), E> {
        let range = self.byte_range_to_char_range(byte_range);
        self.storage
            .borrow()
            .text_props
            .try_for_each_interval_plist_in_char_range(range, f)
    }

    pub(crate) fn adjust_text_props_for_insert_at(&self, pos: CharPos0, len: CharLen) {
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .adjust_for_insert_at_char_pos(pos, len);
    }

    pub(crate) fn adjust_text_props_for_delete_range(&self, range: CharRange) {
        Rc::make_mut(&mut self.storage.borrow_mut().text_props).adjust_for_delete_char_range(range);
    }

    pub(crate) fn adjust_text_props_for_replace_at(
        &self,
        start: CharPos0,
        old_len: CharLen,
        new_len: CharLen,
    ) {
        Rc::make_mut(&mut self.storage.borrow_mut().text_props)
            .adjust_for_replace_at_char_pos(start, old_len, new_len);
    }

    pub fn trace_text_prop_roots(&self, roots: &mut Vec<Value>) {
        self.storage.borrow().text_props.trace_roots(roots);
    }

    /// Register a marker in this buffer. Updates `LispMarker` fields
    /// authoritatively (buffer/bytepos/charpos/marker_id/insertion_type)
    /// and splices the marker into this buffer's intrusive chain at head.
    ///
    /// **Precondition:** `marker_ptr.data.next_marker` is null, i.e. the
    /// marker is not currently on any chain. Callers re-binding a marker
    /// must `chain_unlink` from the old buffer first; the
    /// `debug_assert!` in `chain_splice_at_head` catches violations.
    pub(crate) fn register_marker(
        &self,
        marker_ptr: *mut crate::tagged::header::MarkerObj,
        buffer_id: BufferId,
        marker_id: u64,
        position: TextPositionAnchor,
        insertion_type: InsertionType,
    ) {
        // Update LispMarker so its fields are authoritative before the
        // chain ever exposes this marker.
        //
        // SAFETY: `marker_ptr` is a live MarkerObj allocated via
        // `TaggedHeap::alloc_marker`; writes through a raw pointer are
        // sound for the heap's lifetime. The chain precondition is
        // enforced by `chain_splice_at_head`'s debug_assert below.
        unsafe {
            (*marker_ptr).data.buffer = Some(buffer_id);
            (*marker_ptr).data.marker_id = Some(marker_id);
            set_marker_data_anchor(&mut (*marker_ptr).data, position);
            (*marker_ptr).data.last_position_valid = true;
            (*marker_ptr).data.insertion_type = insertion_type == InsertionType::After;
        }
        self.chain_splice_at_head(marker_ptr);
    }

    /// Walk the intrusive marker chain head→tail and invoke `f` on each
    /// live `LispMarker` by reference. Read-only counterpart to
    /// `chain_walk_mut`; used by pdump (v26) to serialize the chain
    /// without materializing an intermediate Vec.
    ///
    /// SAFETY: walks live chain-owned MarkerObj pointers from
    /// `storage.markers_head` until null; each `(*curr).data` reference
    /// stays valid for the duration of the call because the GC sweep
    /// runs `unchain_dead_markers` between mark and free.
    pub fn chain_walk_data<F: FnMut(&crate::heap_types::LispMarker)>(&self, mut f: F) {
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        unsafe {
            while !curr.is_null() {
                let data = &(*curr).data;
                f(data);
                curr = data.next_marker;
            }
        }
    }

    /// Return GNU `record_marker_adjustments` entries for markers whose
    /// Lisp character positions are in the deleted range `[from, to]`.
    pub fn marker_adjustments_for_delete(
        &self,
        range: TextEditRange,
    ) -> Vec<(crate::emacs_core::value::Value, i64)> {
        let from1 = range.char_start().to_lisp().as_i64();
        let to1 = range.char_end().to_lisp().as_i64();
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        let mut adjustments = Vec::new();
        unsafe {
            while !curr.is_null() {
                let data = &(*curr).data;
                let charpos1 = marker_data_anchor(data).char_pos().to_lisp().as_i64();
                if from1 <= charpos1 && charpos1 <= to1 {
                    let target = if data.insertion_type { to1 } else { from1 };
                    let adjustment = target - charpos1;
                    if adjustment != 0 {
                        adjustments.push((
                            crate::emacs_core::value::Value::from_veclike_ptr(
                                curr as *const crate::tagged::header::VecLikeHeader,
                            ),
                            adjustment,
                        ));
                    }
                }
                curr = data.next_marker;
            }
        }
        adjustments
    }

    pub(crate) fn move_marker_to_anchor(&self, marker_id: u64, position: TextPositionAnchor) {
        let ptr = self.chain_find_by_id(marker_id);
        if ptr.is_null() {
            return;
        }
        unsafe {
            set_marker_data_anchor(&mut (*ptr).data, position);
        }
    }

    /// Walk this buffer's intrusive marker chain and return the raw
    /// MarkerObj pointer for the first node whose `marker_id` matches,
    /// or null when none found. Used by pdump load (v26) to resolve
    /// `BufferStateMarkers` (pt/begv/zv) ids back to chain pointers
    /// after the chain has been reconstructed.
    ///
    /// Pointer lifetime: the returned `*mut MarkerObj` is only valid while
    /// `self`'s chain still holds it. Any subsequent splice/unlink on this
    /// buffer's chain, or a GC cycle that runs `unchain_dead_markers`, may
    /// detach the node from the chain — callers must use the pointer
    /// before doing anything that could mutate the chain, and must not
    /// re-enter the chain (or invoke arbitrary Lisp) between lookup and
    /// use.
    pub fn chain_find_by_id(&self, marker_id: u64) -> *mut crate::tagged::header::MarkerObj {
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: chain walks live chain-owned MarkerObj pointers from
        // `storage.markers_head` until null.
        unsafe {
            while !curr.is_null() {
                if (*curr).data.marker_id == Some(marker_id) {
                    return curr;
                }
                curr = (*curr).data.next_marker;
            }
        }
        std::ptr::null_mut()
    }

    /// Walk the intrusive chain and return the LispMarker-derived position
    /// anchor and insertion type for the marker with the given id, or `None`
    /// if no live chain node carries that id.
    ///
    /// Production code should prefer reading `LispMarker` directly off a
    /// Lisp `Value`. This helper exists for internal buffer-manager
    /// callers (e.g. `clone_marker_in_buffer`) that track markers by id
    /// without holding the Lisp value.
    pub fn marker_chain_anchor_lookup(
        &self,
        marker_id: u64,
    ) -> Option<(TextPositionAnchor, InsertionType)> {
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: chain walks live chain-owned MarkerObj pointers until null.
        unsafe {
            while !curr.is_null() {
                let data = &(*curr).data;
                if data.marker_id == Some(marker_id) {
                    let ins = if data.insertion_type {
                        InsertionType::After
                    } else {
                        InsertionType::Before
                    };
                    return Some((marker_data_anchor(data), ins));
                }
                curr = data.next_marker;
            }
        }
        None
    }

    pub fn remove_marker(&self, marker_id: u64) {
        // Post-T8: `unchain_dead_markers` splices unmarked MarkerObjs
        // out of this buffer's chain between the mark and sweep GC
        // phases, so a chain walk between GC cycles never dereferences
        // a freed allocation. Walk the chain directly and splice the
        // matching node.
        let marker_ptr: Option<*mut crate::tagged::header::MarkerObj> = {
            let storage = self.storage.borrow();
            let mut curr = storage.markers_head;
            let mut found = None;
            // SAFETY: chain walks live chain-owned MarkerObj pointers
            // from `storage.markers_head` until null.
            unsafe {
                while !curr.is_null() {
                    if (*curr).data.marker_id == Some(marker_id) {
                        found = Some(curr);
                        break;
                    }
                    curr = (*curr).data.next_marker;
                }
            }
            found
        };
        if let Some(ptr) = marker_ptr {
            self.chain_unlink(ptr);
            // SAFETY: `ptr` was read from this buffer's chain; chain-
            // owned allocations stay live until the next GC sweep.
            // `chain_unlink` left it detached; field writes are sound.
            unsafe {
                (*ptr).data.buffer = None;
                // GNU `unchain_marker` (marker.c:684) preserves charpos so
                // `marker-last-position` can still report the marker's last
                // attached location.  `last_position_valid` stays true.
            }
        }
    }

    pub fn update_marker_insertion_type(&self, marker_id: u64, insertion_type: InsertionType) {
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: chain walks live chain-owned MarkerObj pointers until null.
        unsafe {
            while !curr.is_null() {
                if (*curr).data.marker_id == Some(marker_id) {
                    (*curr).data.insertion_type = insertion_type == InsertionType::After;
                    return;
                }
                curr = (*curr).data.next_marker;
            }
        }
    }

    /// Return true iff a marker with `marker_id` is currently spliced
    /// into this buffer's chain. Used by BufferManager to pick the
    /// correct buffer when updating insertion type across buffers.
    pub fn has_marker(&self, marker_id: u64) -> bool {
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: chain walks live chain-owned MarkerObj pointers until null.
        unsafe {
            while !curr.is_null() {
                if (*curr).data.marker_id == Some(marker_id) {
                    return true;
                }
                curr = (*curr).data.next_marker;
            }
        }
        false
    }

    pub(crate) fn adjust_markers_for_insert_extent(
        &self,
        insert_pos: EmacsBytePos,
        extent: TextExtent,
    ) {
        if extent.emacs_bytes().is_empty() {
            return;
        }
        let insert_delta = TextExtentDelta::insertion(extent);
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: `curr` walks live chain-owned MarkerObj pointers from
        // `markers_head` until null. Each non-null node was spliced in via
        // `chain_splice_at_head`, so its `data.next_marker` is a valid
        // chain link or null.
        unsafe {
            while !curr.is_null() {
                let data = &mut (*curr).data;
                let marker_byte_pos = marker_data_byte_pos(data);
                if marker_byte_pos > insert_pos {
                    apply_marker_data_delta(data, insert_delta);
                } else if marker_byte_pos == insert_pos && data.insertion_type {
                    // insertion_type == true means "after" in GNU terms.
                    apply_marker_data_delta(data, insert_delta);
                }
                curr = data.next_marker;
            }
        }
    }

    /// Like normal insert marker adjustment, but ignores `insertion_type` for
    /// markers AT `insert_pos`. Used by the GNU-equivalent replace path, where
    /// markers that ended up at `from_byte` (after the prior delete collapsed
    /// inside-region markers there) must NOT advance past the inserted text —
    /// matching GNU `adjust_markers_for_replace` (insdel.c:341).
    pub(crate) fn adjust_markers_for_insert_extent_strict_after(
        &self,
        insert_pos: EmacsBytePos,
        extent: TextExtent,
    ) {
        if extent.emacs_bytes().is_empty() {
            return;
        }
        let insert_delta = TextExtentDelta::insertion(extent);
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: same invariant as adjust_markers_for_insert.
        unsafe {
            while !curr.is_null() {
                let data = &mut (*curr).data;
                if marker_data_byte_pos(data) > insert_pos {
                    apply_marker_data_delta(data, insert_delta);
                }
                curr = data.next_marker;
            }
        }
    }

    pub(crate) fn adjust_markers_for_delete_range(&self, range: TextEditRange) {
        if range.is_empty() {
            return;
        }
        let delete_delta = TextExtentDelta::deletion(range.extent());
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: same invariant as adjust_markers_for_insert.
        unsafe {
            while !curr.is_null() {
                let data = &mut (*curr).data;
                let marker_byte_pos = marker_data_byte_pos(data);
                if marker_byte_pos >= range.byte_end() {
                    apply_marker_data_delta(data, delete_delta);
                } else if marker_byte_pos > range.byte_start() {
                    set_marker_data_anchor(data, range.start_anchor());
                }
                curr = data.next_marker;
            }
        }
    }

    pub(crate) fn adjust_markers_for_replace_range(
        &self,
        old_range: TextEditRange,
        new_extent: TextExtent,
    ) {
        if old_range.is_empty() {
            self.adjust_markers_for_insert_extent(old_range.byte_start(), new_extent);
            return;
        }

        let replace_delta = TextExtentDelta::replacement(old_range.extent(), new_extent);
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: same invariant as adjust_markers_for_insert.
        unsafe {
            while !curr.is_null() {
                let data = &mut (*curr).data;
                let marker_byte_pos = marker_data_byte_pos(data);
                if marker_byte_pos >= old_range.byte_end() {
                    apply_marker_data_delta(data, replace_delta);
                } else if marker_byte_pos > old_range.byte_start() {
                    set_marker_data_anchor(data, old_range.start_anchor());
                }
                curr = data.next_marker;
            }
        }
    }

    pub(crate) fn advance_markers_at_position(&self, pos: EmacsBytePos, extent: TextExtent) {
        if extent.emacs_bytes().is_empty() {
            return;
        }
        let insert_delta = TextExtentDelta::insertion(extent);
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: same invariant as adjust_markers_for_insert.
        unsafe {
            while !curr.is_null() {
                let data = &mut (*curr).data;
                if marker_data_byte_pos(data) == pos {
                    apply_marker_data_delta(data, insert_delta);
                }
                curr = data.next_marker;
            }
        }
    }

    /// Unlink every chain node whose `LispMarker.buffer` is in `killed`.
    /// Sole entry point for kill-buffer marker cleanup: covers both the
    /// kill-root case (killed_set contains the root and all its
    /// indirects, so every marker on the shared chain matches) and the
    /// kill-indirect case (killed_set is just the dying indirect; root
    /// and sibling-indirect markers stay attached).
    pub fn remove_markers_for_buffers(&self, killed: &std::collections::HashSet<BufferId>) {
        let mut storage = self.storage.borrow_mut();
        let mut prev_slot: *mut *mut crate::tagged::header::MarkerObj = &mut storage.markers_head;
        // SAFETY: analogous to `chain_unlink`. Every non-null `*prev_slot`
        // was installed via `chain_splice_at_head`, i.e. a live GC-managed
        // MarkerObj with a valid `data.next_marker` link.
        unsafe {
            while !(*prev_slot).is_null() {
                let curr = *prev_slot;
                let data = &mut (*curr).data;
                let belongs_to_killed = data.buffer.map(|id| killed.contains(&id)).unwrap_or(false);
                if belongs_to_killed {
                    *prev_slot = data.next_marker;
                    data.next_marker = std::ptr::null_mut();
                    data.buffer = None;
                    // Preserve charpos/bytepos and last_position_valid so
                    // `marker-last-position` keeps GNU semantics across
                    // kill-buffer (cf. unchain_marker, marker.c:684).
                } else {
                    prev_slot = &mut data.next_marker;
                }
            }
        }
    }

    /// Retarget marker owners after `buffer-swap-text` moves this whole
    /// text object to another buffer. GNU swaps the marker chains with
    /// `struct buffer_text`, then rewrites each marker's `buffer` slot to
    /// the new owning buffer.
    pub(crate) fn retarget_markers_for_buffer_swap(&self, from: BufferId, to: BufferId) {
        let storage = self.storage.borrow();
        let mut curr = storage.markers_head;
        // SAFETY: same intrusive-chain invariant as `chain_unlink`; each node
        // was linked through this BufferText and remains live while present.
        unsafe {
            while !curr.is_null() {
                let data = &mut (*curr).data;
                if data.buffer == Some(from) {
                    data.buffer = Some(to);
                }
                curr = data.next_marker;
            }
        }
    }

    /// Raw pointer to the `markers_head` slot inside this buffer's
    /// storage. ONLY for GC use — bypasses RefCell's runtime borrow
    /// checks. Callers must hold exclusive access to the tagged heap and
    /// have no outstanding storage borrows (GC is stop-the-world).
    ///
    /// Used by `TaggedHeap::unchain_dead_markers` to splice unmarked
    /// MarkerObj nodes out of the intrusive per-buffer chain before
    /// `sweep_objects` frees them.
    pub unsafe fn markers_head_slot_raw(&self) -> *mut *mut crate::tagged::header::MarkerObj {
        let storage_ptr: *mut BufferTextStorage = self.storage.as_ptr();
        unsafe { &mut (*storage_ptr).markers_head as *mut _ }
    }

    /// Splice `marker` at the head of this buffer's marker chain.
    /// Overwrites `marker.next_marker` with the old head.
    /// Caller sets `marker.buffer` / `marker.bytepos` / `marker.charpos` —
    /// this helper only manipulates chain topology.
    ///
    /// **Precondition:** `marker.next_marker` must be null (marker is not
    /// currently on any chain). Violating this silently truncates the
    /// other chain. `debug_assert!` enforces it in debug builds.
    pub(crate) fn chain_splice_at_head(&self, marker: *mut crate::tagged::header::MarkerObj) {
        let mut storage = self.storage.borrow_mut();
        let old_head = storage.markers_head;
        unsafe {
            // SAFETY: `marker` must be a live MarkerObj allocated via
            // TaggedHeap::alloc_marker and not currently on any other chain
            // (see precondition above). Writing through the pointer is sound
            // because the heap retains ownership for the lifetime of the
            // MarkerObj.
            debug_assert!(
                (*marker).data.next_marker.is_null(),
                "chain_splice_at_head: marker is already on a chain"
            );
            (*marker).data.next_marker = old_head;
        }
        storage.markers_head = marker;
    }

    /// Unlink `marker` from this buffer's chain. Silent no-op if not present.
    /// Does NOT clear `marker.buffer` / positions — caller owns semantic cleanup.
    ///
    /// Unlike GNU `unchain_marker` (marker.c:684), which hard-asserts that
    /// the marker is in the chain, we tolerate absent markers. This is
    /// defensive: callers currently include code paths that may be
    /// double-invoked during GC sweep and kill-buffer cleanup in T8/T9.
    pub(crate) fn chain_unlink(&self, marker: *mut crate::tagged::header::MarkerObj) {
        let mut storage = self.storage.borrow_mut();
        let mut prev_slot: *mut *mut crate::tagged::header::MarkerObj = &mut storage.markers_head;
        // SAFETY: `prev_slot` walks the intrusive chain starting at
        // `storage.markers_head`. Every non-null `*prev_slot` is a
        // `*mut MarkerObj` previously installed via `chain_splice_at_head`,
        // i.e. a live GC-managed allocation whose `.data.next_marker` is
        // the next chain slot. We never read past a null terminator, and
        // mutations only rewrite chain-owned `next_marker` fields.
        unsafe {
            while !(*prev_slot).is_null() {
                let curr = *prev_slot;
                if curr == marker {
                    *prev_slot = (*curr).data.next_marker;
                    (*curr).data.next_marker = std::ptr::null_mut();
                    return;
                }
                prev_slot = &mut (*curr).data.next_marker;
            }
        }
    }

    /// Walk the chain from head to tail, collecting raw pointers in order.
    /// Test-only helper.
    #[cfg(test)]
    pub fn chain_walk_collect(&self) -> Vec<*mut crate::tagged::header::MarkerObj> {
        let storage = self.storage.borrow();
        let mut out = Vec::new();
        let mut curr = storage.markers_head;
        // SAFETY: Same invariant as `chain_unlink` — `curr` walks live
        // chain-owned MarkerObj pointers from `storage.markers_head`
        // until a null terminator.
        unsafe {
            while !curr.is_null() {
                out.push(curr);
                curr = (*curr).data.next_marker;
            }
        }
        out
    }

    /// Remember a completed scan's endpoint as a reusable anchor, replacing
    /// round-robin once the ring is full.
    fn remember_scan_anchor(storage: &BufferTextStorage, anchor: TextPositionAnchor) {
        let mut cache = storage.anchor_cache.borrow_mut();
        if cache.len() < POSITION_ANCHOR_RING_CAP {
            cache.push(anchor);
        } else {
            let slot = storage.anchor_cache_cursor.get() % POSITION_ANCHOR_RING_CAP;
            cache[slot] = anchor;
            storage.anchor_cache_cursor.set(slot + 1);
        }
    }

    fn ensure_position_anchor_cache_current(storage: &BufferTextStorage, content_epoch: u64) {
        if storage.anchor_cache_key.get() != content_epoch {
            storage.anchor_cache.borrow_mut().clear();
            storage.anchor_cache_key.set(content_epoch);
        }
    }

    fn char_position_bounds(storage: &BufferTextStorage, target: CharPos0) -> TextPositionBounds {
        let metrics = storage.metrics;
        let mut bounds = TextPositionBounds::new(TextPositionAnchor::new(
            metrics.char_end(),
            metrics.emacs_byte_end(),
        ));

        storage
            .backend
            .storage_position_hint()
            .consider_char_anchor(&mut bounds, target);

        let cached = storage.pos_cache.get();
        if cached.is_valid_for(storage.content_epoch) {
            bounds.consider_char_anchor(target, cached.anchor);
        }

        for &anchor in storage.anchor_cache.borrow().iter() {
            bounds.consider_char_anchor(target, anchor);
        }

        let mut distance = CharLen::new(POSITION_DISTANCE_BASE);
        // T7: marker chain walk. The chain carries the same (char, byte)
        // pairs that the deleted Vec<MarkerEntry> used to.
        //
        // SAFETY: `curr` walks live chain-owned MarkerObj pointers from
        // `storage.markers_head` until null. Each non-null node was spliced in
        // via `chain_splice_at_head`, so its `data.next_marker` is a valid
        // chain link or null.
        let mut curr = storage.markers_head;
        unsafe {
            while !curr.is_null() {
                let data = &(*curr).data;
                bounds.consider_char_anchor(target, marker_data_anchor(data));
                if bounds.char_target_is_near(target, distance) {
                    break;
                }
                distance = distance.add_len(CharLen::new(POSITION_DISTANCE_INCR));
                curr = data.next_marker;
            }
        }

        bounds
    }

    fn byte_position_bounds(
        storage: &BufferTextStorage,
        target: EmacsBytePos,
    ) -> TextPositionBounds {
        let metrics = storage.metrics;
        let mut bounds = TextPositionBounds::new(TextPositionAnchor::new(
            metrics.char_end(),
            metrics.emacs_byte_end(),
        ));

        storage
            .backend
            .storage_position_hint()
            .consider_byte_anchor(&mut bounds, target);

        let cached = storage.pos_cache.get();
        if cached.is_valid_for(storage.content_epoch) {
            bounds.consider_byte_anchor(target, cached.anchor);
        }

        for &anchor in storage.anchor_cache.borrow().iter() {
            bounds.consider_byte_anchor(target, anchor);
        }

        let mut distance = EmacsByteLen::new(POSITION_DISTANCE_BASE);
        // T7: marker chain walk. See sibling comment in
        // `char_position_bounds` for the SAFETY rationale.
        let mut curr = storage.markers_head;
        unsafe {
            while !curr.is_null() {
                let data = &(*curr).data;
                bounds.consider_byte_anchor(target, marker_data_anchor(data));
                if bounds.byte_target_is_near(target, distance) {
                    break;
                }
                distance = distance.add_len(EmacsByteLen::new(POSITION_DISTANCE_INCR));
                curr = data.next_marker;
            }
        }

        bounds
    }

    /// Convert a character position to a logical Emacs byte offset using an
    /// anchor-bracketed cached search. Mirrors GNU `buf_charpos_to_bytepos`
    /// (`src/marker.c:167`).
    /// Chars==bytes identity head inline; anchor machinery out-of-line (see
    /// `emacs_byte_pos_to_char_pos`).
    #[inline]
    pub fn char_pos_to_emacs_byte_pos(&self, target: CharPos0) -> EmacsBytePos {
        #[cfg(test)]
        CHAR_POS_TO_EMACS_BYTE_POS_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        {
            let storage = self.storage.borrow();
            let metrics = storage.metrics;
            if metrics.char_len().get() == metrics.emacs_byte_len().get() {
                return if target >= metrics.char_end() {
                    metrics.emacs_byte_end()
                } else {
                    EmacsBytePos::new(target.get())
                };
            }
        }
        self.char_pos_to_emacs_byte_pos_multibyte(target)
    }

    fn char_pos_to_emacs_byte_pos_multibyte(&self, target: CharPos0) -> EmacsBytePos {
        let storage = self.storage.borrow();
        let metrics = storage.metrics;
        let content_epoch = storage.content_epoch;
        let total_chars = storage.metrics.char_len();
        let total_bytes = storage.metrics.emacs_byte_len();

        if target >= metrics.char_end() {
            return metrics.emacs_byte_end();
        }

        // Unibyte fast path: char == byte, no scan needed.
        if total_chars.get() == total_bytes.get() {
            return EmacsBytePos::new(target.get());
        }

        if storage.backend.position_lookup().uses_backend_index() {
            let result = storage.backend.char_pos_to_emacs_byte_pos(target);
            storage.pos_cache.set(PositionCache {
                epoch: content_epoch,
                anchor: TextPositionAnchor::new(target, result),
            });
            return result;
        }

        Self::ensure_position_anchor_cache_current(&storage, content_epoch);

        // Cheap-first bracket; see the byte->char direction above.
        {
            let mut mini = TextPositionBounds::new(TextPositionAnchor::new(
                storage.metrics.char_end(),
                storage.metrics.emacs_byte_end(),
            ));
            storage
                .backend
                .storage_position_hint()
                .consider_char_anchor(&mut mini, target);
            let cached = storage.pos_cache.get();
            if cached.is_valid_for(storage.content_epoch) {
                mini.consider_char_anchor(target, cached.anchor);
            }
            if let Some(result) = mini.interpolate_char(target) {
                storage.pos_cache.set(PositionCache {
                    epoch: content_epoch,
                    anchor: TextPositionAnchor::new(target, result),
                });
                return result;
            }
        }

        let bounds = Self::char_position_bounds(&storage, target);

        // GNU marker.c CONSIDER: an all-single-byte bracket converts by
        // arithmetic. Worth checking before every scan -- a source file is
        // multibyte for a handful of curly quotes, so the whole-buffer fast
        // path above misses while nearly every local span still qualifies.
        if let Some(result) = bounds.interpolate_char(target) {
            storage.pos_cache.set(PositionCache {
                epoch: content_epoch,
                anchor: TextPositionAnchor::new(target, result),
            });
            return result;
        }

        let nearest_anchor = bounds.nearest_char_anchor(target);
        let result = if nearest_anchor.char_pos() <= target {
            scan_forward(&storage.backend, nearest_anchor, target)
        } else {
            scan_backward(&storage.backend, nearest_anchor, target)
        };

        // Mirror GNU marker.c:238-241: insert an anchor when the scan actually
        // walked more than POSITION_ANCHOR_STRIDE positions.
        let walked = bounds.min_char_walk(target);
        if walked.get() > POSITION_ANCHOR_STRIDE {
            Self::remember_scan_anchor(&storage, TextPositionAnchor::new(target, result));
        }

        storage.pos_cache.set(PositionCache {
            epoch: content_epoch,
            anchor: TextPositionAnchor::new(target, result),
        });
        result
    }

    /// Convert a logical Emacs byte position to a character position. Symmetric
    /// to `buf_charpos_to_bytepos` — shares the same anchor + cache machinery.
    ///
    /// The chars==bytes identity head is split out inline: match-data
    /// publication converts two positions per match, and for ASCII-content
    /// buffers the whole conversion is a borrow + two compares — the
    /// out-of-line anchor machinery below kept it at ~110 Ir per call.
    #[inline]
    pub fn emacs_byte_pos_to_char_pos(&self, target: EmacsBytePos) -> CharPos0 {
        {
            let storage = self.storage.borrow();
            let metrics = storage.metrics;
            if metrics.char_len().get() == metrics.emacs_byte_len().get() {
                return if target >= metrics.emacs_byte_end() {
                    metrics.char_end()
                } else {
                    CharPos0::new(target.get())
                };
            }
        }
        self.emacs_byte_pos_to_char_pos_multibyte(target)
    }

    fn emacs_byte_pos_to_char_pos_multibyte(&self, target: EmacsBytePos) -> CharPos0 {
        let storage = self.storage.borrow();
        let metrics = storage.metrics;
        let content_epoch = storage.content_epoch;
        let total_chars = storage.metrics.char_len();
        let total_bytes = storage.metrics.emacs_byte_len();

        if target >= metrics.emacs_byte_end() {
            return metrics.char_end();
        }

        // Unibyte fast path: char == byte, no scan needed.
        if total_chars.get() == total_bytes.get() {
            return CharPos0::new(target.get());
        }

        if storage.backend.position_lookup().uses_backend_index()
            && storage
                .backend
                .emacs_byte_at_pos(target)
                .is_none_or(|byte| (byte & 0xC0) != 0x80)
        {
            let result = storage.backend.emacs_byte_pos_to_char_pos(target);
            storage.pos_cache.set(PositionCache {
                epoch: content_epoch,
                anchor: TextPositionAnchor::new(result, target),
            });
            return result;
        }

        Self::ensure_position_anchor_cache_current(&storage, content_epoch);

        // Cheap-first bracket: structural anchors plus the position cache
        // only. Sequential queries (font-lock walking a buffer) land next to
        // the cache anchor and interpolate immediately; building the FULL
        // bounds first walked the stride-anchor vec and the marker chain on
        // every call, and measured 6 percent self time after the byte-range
        // reroute made this the single conversion entry.
        {
            let mut mini = TextPositionBounds::new(TextPositionAnchor::new(
                storage.metrics.char_end(),
                storage.metrics.emacs_byte_end(),
            ));
            storage
                .backend
                .storage_position_hint()
                .consider_byte_anchor(&mut mini, target);
            let cached = storage.pos_cache.get();
            if cached.is_valid_for(storage.content_epoch) {
                mini.consider_byte_anchor(target, cached.anchor);
            }
            if let Some(result) = mini.interpolate_byte(target) {
                storage.pos_cache.set(PositionCache {
                    epoch: content_epoch,
                    anchor: TextPositionAnchor::new(result, target),
                });
                return result;
            }
        }

        let bounds = Self::byte_position_bounds(&storage, target);

        // GNU marker.c CONSIDER, mirrored from the char->byte direction: an
        // all-single-byte bracket converts by arithmetic, no scan. The target
        // is necessarily a character boundary inside such a span, so this
        // cannot seed the bogus mid-character anchor guarded against below.
        if let Some(result) = bounds.interpolate_byte(target) {
            storage.pos_cache.set(PositionCache {
                epoch: content_epoch,
                anchor: TextPositionAnchor::new(result, target),
            });
            return result;
        }

        let nearest_anchor = bounds.nearest_byte_anchor(target);
        let result = if nearest_anchor.emacs_byte_pos() <= target {
            scan_forward_bytes(&storage.backend, nearest_anchor, target)
        } else {
            scan_backward_bytes(&storage.backend, nearest_anchor, target)
        };

        // Only cache `(char, byte)` pairs whose byte is a character boundary.
        // A byte in the middle of a multibyte character (a continuation byte,
        // `(b & 0xC0) == 0x80`) does not begin `result`, so caching it would
        // seed a bogus anchor: a later forward scan from `(result, mid_byte)`
        // would over-count, since `mid_byte < real_char_start`. GNU's
        // charpos<->bytepos cache (marker.c) only ever holds boundary pairs.
        let target_is_char_boundary = storage
            .backend
            .emacs_byte_at_pos(target)
            .is_none_or(|byte| (byte & 0xC0) != 0x80);

        if target_is_char_boundary {
            // Mirror GNU marker.c:238-241: insert an anchor when the scan
            // actually walked more than POSITION_ANCHOR_STRIDE positions.
            let walked = bounds.min_byte_walk(target);
            if walked.get() > POSITION_ANCHOR_STRIDE {
                Self::remember_scan_anchor(&storage, TextPositionAnchor::new(result, target));
            }

            storage.pos_cache.set(PositionCache {
                epoch: content_epoch,
                anchor: TextPositionAnchor::new(result, target),
            });
        }
        result
    }

    #[cfg(test)]
    pub fn anchor_cache_len(&self) -> usize {
        self.storage.borrow().anchor_cache.borrow().len()
    }
}

impl fmt::Display for BufferText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.storage.borrow().backend.fmt(f)
    }
}

impl fmt::Debug for BufferText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufferText")
            .field("emacs_byte_len", &self.emacs_byte_len().get())
            .field("chars", &self.char_count().get())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Position conversion helpers
// ---------------------------------------------------------------------------

/// GNU `marker.c:162` — initial bracket-bail distance.
const POSITION_DISTANCE_BASE: usize = 50;
/// GNU `marker.c:162` — bracket-bail distance grows by this per marker checked.
const POSITION_DISTANCE_INCR: usize = 50;
/// Auto-insert an anchor when a scan walks more than this many positions.
/// GNU `marker.c:238-241` uses 5000, but GNU also gains a fresh anchor from
/// every marker the session creates; without that ambient supply, repeated
/// ~3KB scans between two hot regions (kill at line N / yank at EOB) never
/// earned an anchor and re-walked the same span forever. The ring below is
/// capped, so a lower threshold costs at most RING_CAP compares per lookup.
const POSITION_ANCHOR_STRIDE: usize = 512;
/// Bound on remembered scan anchors per epoch (round-robin replacement):
/// keeps the per-conversion consider loop O(1) while covering several hot
/// regions at once.
const POSITION_ANCHOR_RING_CAP: usize = 16;

struct ForwardMultibytePositionScan {
    byte_pos: EmacsBytePos,
    char_pos: CharPos0,
    pending: [u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH],
    pending_len: usize,
}

impl ForwardMultibytePositionScan {
    fn new(anchor: TextPositionAnchor) -> Self {
        Self {
            byte_pos: anchor.emacs_byte_pos(),
            char_pos: anchor.char_pos(),
            pending: [0; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH],
            pending_len: 0,
        }
    }

    fn finish_pending_char(&mut self, chunk: &[u8], offset: &mut usize) -> bool {
        if self.pending_len == 0 {
            return true;
        }

        let expected = emacs_multibyte_candidate_len(self.pending[0]);
        let take = expected.saturating_sub(self.pending_len).min(chunk.len());
        self.pending[self.pending_len..self.pending_len + take].copy_from_slice(&chunk[..take]);
        self.pending_len += take;
        self.byte_pos = self.byte_pos.add_len(EmacsByteLen::new(take));
        *offset += take;

        if self.pending_len < expected {
            return false;
        }

        let (_, len) = crate::emacs_core::emacs_char::string_char(&self.pending[..expected]);
        debug_assert_eq!(
            len, expected,
            "forward position scan should only complete valid Emacs characters"
        );
        self.pending_len = 0;
        self.char_pos = self.char_pos.add_len(CharLen::new(1));
        true
    }

    fn consume_chunk_until_char(&mut self, chunk: &[u8], target: CharPos0) -> Option<EmacsBytePos> {
        let mut offset = 0;
        if !self.finish_pending_char(chunk, &mut offset) {
            return None;
        }
        if self.char_pos >= target {
            return Some(self.byte_pos);
        }

        while offset < chunk.len() {
            // ASCII fast path: a maximal run of ASCII bytes is one char per byte,
            // so cross it in bulk (bounded by the chars still needed) instead of
            // decoding each char. The `position` scan over `b >= 0x80`
            // auto-vectorizes; GNU's marker charpos<->bytepos scan walks
            // char-by-char, so this beats it on the dominant ASCII code path.
            if chunk[offset] < 0x80 {
                // Scan only as far as we could possibly advance (the chars still
                // needed), NOT to the next non-ASCII byte in the whole chunk --
                // otherwise a short conversion over all-ASCII text scans the
                // entire remaining segment.
                let window = (target.get() - self.char_pos.get()).min(chunk.len() - offset);
                let ascii_run = chunk[offset..offset + window]
                    .iter()
                    .position(|&b| b >= 0x80)
                    .unwrap_or(window);
                offset += ascii_run;
                self.byte_pos = self.byte_pos.add_len(EmacsByteLen::new(ascii_run));
                self.char_pos = self.char_pos.add_len(CharLen::new(ascii_run));
                if self.char_pos >= target {
                    return Some(self.byte_pos);
                }
                continue;
            }
            let expected = emacs_multibyte_candidate_len(chunk[offset]);
            let available = chunk.len() - offset;
            if available < expected {
                self.pending[..available].copy_from_slice(&chunk[offset..]);
                self.pending_len = available;
                self.byte_pos = self.byte_pos.add_len(EmacsByteLen::new(available));
                return None;
            }

            let (_, len) =
                crate::emacs_core::emacs_char::string_char(&chunk[offset..offset + expected]);
            offset += len;
            self.byte_pos = self.byte_pos.add_len(EmacsByteLen::new(len));
            self.char_pos = self.char_pos.add_len(CharLen::new(1));
            if self.char_pos >= target {
                return Some(self.byte_pos);
            }
        }
        None
    }

    fn consume_chunk_until_byte(&mut self, chunk: &[u8], target: EmacsBytePos) -> Option<CharPos0> {
        let mut offset = 0;
        if !self.finish_pending_char(chunk, &mut offset) {
            return None;
        }
        if self.byte_pos >= target {
            return Some(self.char_pos);
        }

        while offset < chunk.len() {
            // ASCII fast path: cross a run of ASCII bytes (one char per byte) in
            // bulk, bounded by the bytes still needed. See consume_chunk_until_char.
            if chunk[offset] < 0x80 {
                // Bound the scan by the bytes still needed (see
                // consume_chunk_until_char); each ASCII byte is one char.
                let window = (target.get() - self.byte_pos.get()).min(chunk.len() - offset);
                let ascii_run = chunk[offset..offset + window]
                    .iter()
                    .position(|&b| b >= 0x80)
                    .unwrap_or(window);
                offset += ascii_run;
                self.byte_pos = self.byte_pos.add_len(EmacsByteLen::new(ascii_run));
                self.char_pos = self.char_pos.add_len(CharLen::new(ascii_run));
                if self.byte_pos >= target {
                    return Some(self.char_pos);
                }
                continue;
            }
            let expected = emacs_multibyte_candidate_len(chunk[offset]);
            let available = chunk.len() - offset;
            if available < expected {
                self.pending[..available].copy_from_slice(&chunk[offset..]);
                self.pending_len = available;
                self.byte_pos = self.byte_pos.add_len(EmacsByteLen::new(available));
                return None;
            }

            let (_, len) =
                crate::emacs_core::emacs_char::string_char(&chunk[offset..offset + expected]);
            offset += len;
            self.byte_pos = self.byte_pos.add_len(EmacsByteLen::new(len));
            self.char_pos = self.char_pos.add_len(CharLen::new(1));
            if self.byte_pos >= target {
                return Some(self.char_pos);
            }
        }
        None
    }
}

/// Walk forward from `anchor` to reach `target` chars.
/// Returns the byte position.
fn scan_forward(
    backend: &TextBackend,
    anchor: TextPositionAnchor,
    target: CharPos0,
) -> EmacsBytePos {
    let cp = anchor.char_pos();
    let bp = anchor.emacs_byte_pos();
    if cp >= target {
        return bp;
    }
    if !backend.is_multibyte() {
        return bp.add_len(EmacsByteLen::new(target.saturating_offset_from(cp).get()));
    }

    let range = EmacsByteRange::new(bp, backend.metrics().emacs_byte_end());
    let mut scan = ForwardMultibytePositionScan::new(anchor);
    match backend.for_each_emacs_byte_range_chunk(range, |chunk| {
        scan.consume_chunk_until_char(chunk, target)
            .map_or(Ok(()), Err)
    }) {
        Ok(()) => scan.byte_pos,
        Err(result) => result,
    }
}

/// Walk backward from `anchor` to reach `target` chars.
/// Returns the byte position.
fn scan_backward(
    backend: &TextBackend,
    anchor: TextPositionAnchor,
    target: CharPos0,
) -> EmacsBytePos {
    let mut cp = anchor.char_pos();
    let mut bp = anchor.emacs_byte_pos();
    while cp > target {
        if !backend.is_multibyte() {
            bp = bp.saturating_sub_len(EmacsByteLen::new(1));
            cp = cp.saturating_sub_len(CharLen::new(1));
            continue;
        }
        bp = previous_multibyte_char_start(backend, bp);
        cp = cp.saturating_sub_len(CharLen::new(1));
    }
    bp
}

/// Walk forward from `anchor` to reach `target` bytepos.
/// Returns the char position.
fn scan_forward_bytes(
    backend: &TextBackend,
    anchor: TextPositionAnchor,
    target: EmacsBytePos,
) -> CharPos0 {
    let bp = anchor.emacs_byte_pos();
    let cp = anchor.char_pos();
    if bp >= target {
        return cp;
    }
    if !backend.is_multibyte() {
        return cp.add_len(CharLen::new(target.saturating_offset_from(bp).get()));
    }

    let range = EmacsByteRange::new(bp, backend.metrics().emacs_byte_end());
    let mut scan = ForwardMultibytePositionScan::new(anchor);
    match backend.for_each_emacs_byte_range_chunk(range, |chunk| {
        scan.consume_chunk_until_byte(chunk, target)
            .map_or(Ok(()), Err)
    }) {
        Ok(()) => scan.char_pos,
        Err(result) => result,
    }
}

/// Walk backward from `anchor` to reach `target` bytepos.
/// Returns the char position.
fn scan_backward_bytes(
    backend: &TextBackend,
    anchor: TextPositionAnchor,
    target: EmacsBytePos,
) -> CharPos0 {
    let mut bp = anchor.emacs_byte_pos();
    let mut cp = anchor.char_pos();
    while bp > target {
        if !backend.is_multibyte() {
            bp = bp.saturating_sub_len(EmacsByteLen::new(1));
            cp = cp.saturating_sub_len(CharLen::new(1));
            continue;
        }
        bp = previous_multibyte_char_start(backend, bp);
        cp = cp.saturating_sub_len(CharLen::new(1));
    }
    cp
}

fn previous_multibyte_char_start(backend: &TextBackend, pos: EmacsBytePos) -> EmacsBytePos {
    let mut prev = pos.saturating_sub_len(EmacsByteLen::new(1));
    while prev > EmacsBytePos::ZERO && (backend.byte_at_emacs_byte_pos(prev) & 0xC0) == 0x80 {
        prev = prev.saturating_sub_len(EmacsByteLen::new(1));
    }
    prev
}

#[cfg(test)]
#[path = "buffer_text_test.rs"]
mod tests;

#[cfg(test)]
#[path = "buffer_text_chain_test.rs"]
mod chain_test;
