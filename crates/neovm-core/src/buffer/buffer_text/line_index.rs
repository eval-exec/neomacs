//! `BufferText`'s side of the text line index ([`crate::buffer::text_index`]):
//! where the index lives, when it is built, the queries it answers and its
//! maintenance on every edit.
//!
//! - **Where.** `BufferTextStorage::text_index`, outside the shared text
//!   bytes (P3.0 §3.9); a layout snapshot starts without one.
//! - **When.** On the first query the index would serve, once the text
//!   reaches `min_buffer_bytes`. The build borrows only the index slot,
//!   never the storage mutably, so it is safe under a read borrow redisplay
//!   holds (the trap of the ASCII-prefix attempt).
//! - **Edits.** The four measured mutators update the index: a deletion
//!   before the backend loses the bytes (they are counted), an insertion
//!   after. An edit larger than `max(256 KiB, text / 4)` and every wholesale
//!   mutator drop it.

use std::rc::Rc;

use super::{BufferText, BufferTextStorage, LineEnd};
use crate::buffer::position::{EmacsBytePos, EmacsByteRange};
use crate::buffer::text_index::{
    IndexedText, TextLineIndex, TextLineIndexConfig, TextLineIndexEvent, note_event,
    report_mismatch, text_line_index_config,
};

/// An edit of more bytes than this, and than a quarter of the text, drops
/// the index instead of updating it (an `erase-buffer`, a restore).
const DROP_EDIT_BYTES: usize = 256 * 1024;

fn edit_drops_index(edit_bytes: usize, text_bytes: usize) -> bool {
    edit_bytes > DROP_EDIT_BYTES.max(text_bytes / 4)
}

/// The index for an edit (copied first if anything else still shares it).
fn index_for_edit(
    shared: &mut Rc<TextLineIndex>,
    config: TextLineIndexConfig,
) -> &mut TextLineIndex {
    if Rc::get_mut(shared).is_none() {
        note_event(config, TextLineIndexEvent::CowCopy);
    }
    Rc::make_mut(shared)
}

impl BufferTextStorage {
    /// The index, built now if the text is large enough for one. `None`:
    /// the caller scans.
    fn line_index(&self, config: TextLineIndexConfig) -> Option<Rc<TextLineIndex>> {
        if let Some(index) = self.text_index.try_borrow().ok()?.as_ref() {
            return Some(Rc::clone(index));
        }
        if self.metrics.emacs_byte_len().get() < config.min_buffer_bytes {
            return None;
        }
        self.build_line_index(config)
    }

    #[cold]
    #[inline(never)]
    fn build_line_index(&self, config: TextLineIndexConfig) -> Option<Rc<TextLineIndex>> {
        let mut slot = self.text_index.try_borrow_mut().ok()?;
        let index = Rc::new(TextLineIndex::build(&*self.backend, config.chunk_bytes));
        *slot = Some(Rc::clone(&index));
        note_event(config, TextLineIndexEvent::Build);
        tracing::trace!(
            target: "neovm::text_line_index",
            bytes = index.total().bytes,
            chunks = index.chunk_count(),
            "built a text line index"
        );
        Some(index)
    }

    /// Forget the index (a wholesale mutation).
    pub(super) fn drop_line_index(&mut self) {
        if self.text_index.get_mut().take().is_some() {
            note_event(text_line_index_config(), TextLineIndexEvent::Drop);
        }
    }

    /// Verify mode: recount the chunks around AT after an edit; a
    /// disagreement is reported and the index dropped.
    fn check_line_index_edit(&mut self, at: EmacsBytePos) {
        let Self {
            backend,
            text_index,
            ..
        } = self;
        let slot = text_index.get_mut();
        let Some(index) = slot.as_ref() else {
            return;
        };
        let checked = if index.is_multibyte() != backend.is_multibyte() {
            Err("multibyte flag disagrees with the text".to_owned())
        } else {
            index.check_around(&**backend, at.get())
        };
        if let Err(err) = checked {
            *slot = None;
            report_mismatch("edit maintenance", &err);
        }
    }
}

impl BufferText {
    /// Before the backend loses RANGE: take its bytes out of the index, or
    /// drop the index for a large deletion.
    #[inline(never)]
    pub(super) fn line_index_before_delete(storage: &mut BufferTextStorage, range: EmacsByteRange) {
        let config = text_line_index_config();
        let BufferTextStorage {
            backend,
            text_index,
            ..
        } = storage;
        let slot = text_index.get_mut();
        let Some(shared) = slot.as_mut() else {
            return;
        };
        let len = range.len().get();
        if len == 0 {
            return;
        }
        if edit_drops_index(len, backend.byte_len()) {
            *slot = None;
            note_event(config, TextLineIndexEvent::Drop);
            return;
        }
        index_for_edit(shared, config).note_delete(
            &**backend,
            range.start().get(),
            range.end().get(),
        );
    }

    /// After the backend gained BYTES at AT: add them to the index, or drop
    /// the index for a large insertion. Then the verify-mode check.
    #[inline(never)]
    pub(super) fn line_index_after_insert(
        storage: &mut BufferTextStorage,
        at: EmacsBytePos,
        bytes: &[u8],
    ) {
        let config = text_line_index_config();
        {
            let BufferTextStorage {
                backend,
                text_index,
                ..
            } = &mut *storage;
            let slot = text_index.get_mut();
            let Some(shared) = slot.as_mut() else {
                return;
            };
            if !bytes.is_empty() {
                if edit_drops_index(bytes.len(), backend.byte_len() - bytes.len()) {
                    *slot = None;
                    note_event(config, TextLineIndexEvent::Drop);
                    return;
                }
                index_for_edit(shared, config).note_insert(&**backend, at.get(), bytes);
            }
        }
        if config.verifies() {
            storage.check_line_index_edit(at);
        }
    }

    /// After an edit that only deleted: the verify-mode check.
    #[inline(never)]
    pub(super) fn line_index_after_delete(storage: &mut BufferTextStorage, at: EmacsBytePos) {
        if text_line_index_config().verifies() {
            storage.check_line_index_edit(at);
        }
    }

    /// Line ends in the clamped, non-empty `[from, limit)` from the index, or
    /// `None` to scan.
    #[inline]
    pub(super) fn line_index_count(
        &self,
        from: EmacsBytePos,
        limit: EmacsBytePos,
        line_end: LineEnd,
    ) -> Option<usize> {
        let config = text_line_index_config();
        if !config.enabled() || limit.get() - from.get() < config.min_query_bytes {
            return None;
        }
        self.indexed_line_end_count(config, from, limit, line_end)
    }

    #[inline(never)]
    fn indexed_line_end_count(
        &self,
        config: TextLineIndexConfig,
        from: EmacsBytePos,
        limit: EmacsBytePos,
        line_end: LineEnd,
    ) -> Option<usize> {
        let count = {
            let storage = self.storage.borrow();
            let index = storage.line_index(config)?;
            let text = &*storage.backend;
            (index.line_ends_before(text, limit.get(), line_end)
                - index.line_ends_before(text, from.get(), line_end)) as usize
        };
        note_event(config, TextLineIndexEvent::CountServed);
        if config.verifies() {
            let scanned = self.scan_line_ends_emacs_byte(from, limit, line_end);
            if scanned != count {
                report_mismatch(
                    "line count",
                    &format!("[{from:?}, {limit:?}) {line_end:?}: index {count}, scan {scanned}"),
                );
                return Some(scanned);
            }
        }
        Some(count)
    }

    /// `nth_newline_emacs_byte` from the index for the clamped, non-empty
    /// `[from, limit)` and `n >= 1`, or `None` to scan.
    #[inline]
    pub(super) fn line_index_nth_newline(
        &self,
        from: EmacsBytePos,
        limit: EmacsBytePos,
        n: usize,
    ) -> Option<(EmacsBytePos, usize)> {
        let config = text_line_index_config();
        if !config.enabled() || n <= config.min_query_lines {
            return None;
        }
        self.indexed_nth_newline(config, from, limit, n)
    }

    #[inline(never)]
    fn indexed_nth_newline(
        &self,
        config: TextLineIndexConfig,
        from: EmacsBytePos,
        limit: EmacsBytePos,
        n: usize,
    ) -> Option<(EmacsBytePos, usize)> {
        let found = {
            let storage = self.storage.borrow();
            let index = storage.line_index(config)?;
            let text = &*storage.backend;
            let base = index.line_ends_before(text, from.get(), LineEnd::Newline);
            match index.newline_position(text, base + n as u64) {
                Some(at) if at < limit.get() => (EmacsBytePos::new(at + 1), n),
                // Fewer than N newlines before LIMIT: past the last one
                // crossed, as the scan reports when it runs out.
                _ => {
                    let before_limit = index.line_ends_before(text, limit.get(), LineEnd::Newline);
                    let crossed = before_limit - base;
                    if crossed == 0 {
                        (from, 0)
                    } else {
                        let last = index.newline_position(text, before_limit)?;
                        (EmacsBytePos::new(last + 1), crossed as usize)
                    }
                }
            }
        };
        note_event(config, TextLineIndexEvent::ForwardServed);
        if config.verifies() {
            let scanned = self.scan_nth_newline_emacs_byte(from, limit, n);
            if scanned != found {
                report_mismatch(
                    "forward lines",
                    &format!("[{from:?}, {limit:?}) n={n}: index {found:?}, scan {scanned:?}"),
                );
                return Some(scanned);
            }
        }
        Some(found)
    }

    /// `forward-line -N`'s motion within `[floor, from]`: the start of the
    /// Nth line before the one holding FROM, or FLOOR when there are fewer,
    /// and how many lines were moved -- from the index, or `None` when the
    /// index does not serve it and the caller walks back line by line.
    pub(crate) fn lines_backward_emacs_byte(
        &self,
        from: EmacsBytePos,
        floor: EmacsBytePos,
        n: usize,
    ) -> Option<(EmacsBytePos, usize)> {
        let config = text_line_index_config();
        if !config.enabled() || n <= config.min_query_lines {
            return None;
        }
        self.indexed_lines_backward(config, from, floor, n)
    }

    #[inline(never)]
    fn indexed_lines_backward(
        &self,
        config: TextLineIndexConfig,
        from: EmacsBytePos,
        floor: EmacsBytePos,
        n: usize,
    ) -> Option<(EmacsBytePos, usize)> {
        let total = self.emacs_byte_end_pos();
        let from = from.min(total);
        let floor = floor.min(from);
        let found = {
            let storage = self.storage.borrow();
            let index = storage.line_index(config)?;
            let text = &*storage.backend;
            let base = index.line_ends_before(text, floor.get(), LineEnd::Newline);
            // The newlines in [floor, from): the lines above FROM's line.
            let above = index.line_ends_before(text, from.get(), LineEnd::Newline) - base;
            let moved = above.min(n as u64);
            if above > moved {
                let newline = index.newline_position(text, base + above - moved)?;
                (EmacsBytePos::new(newline + 1), moved as usize)
            } else {
                (floor, moved as usize)
            }
        };
        note_event(config, TextLineIndexEvent::BackwardServed);
        if config.verifies() {
            let scanned = self.scan_lines_backward_emacs_byte(from, floor, n);
            if scanned != found {
                report_mismatch(
                    "backward lines",
                    &format!("[{floor:?}, {from:?}] n={n}: index {found:?}, scan {scanned:?}"),
                );
                return Some(scanned);
            }
        }
        Some(found)
    }

    /// The line-by-line walk `forward-line -N` does without the index
    /// (`move_by_lines_narrowed`): the reference the verify mode checks
    /// against.
    fn scan_lines_backward_emacs_byte(
        &self,
        from: EmacsBytePos,
        floor: EmacsBytePos,
        n: usize,
    ) -> (EmacsBytePos, usize) {
        let bol = |pos: EmacsBytePos| {
            self.prev_newline_emacs_byte(pos, floor)
                .map_or(floor, |newline| EmacsBytePos::new(newline.get() + 1))
        };
        let mut pos = from;
        let mut moved = 0usize;
        for _ in 0..n {
            let start = bol(pos);
            if start <= floor {
                pos = floor;
                break;
            }
            pos = bol(EmacsBytePos::new(start.get() - 1));
            moved += 1;
        }
        (pos, moved)
    }

    /// Whether this text holds an index now (tests).
    #[cfg(test)]
    pub(crate) fn has_line_index_for_test(&self) -> bool {
        self.storage.borrow().text_index.borrow().is_some()
    }

    /// Recount the whole index against the text (tests).
    #[cfg(test)]
    pub(crate) fn check_line_index_for_test(&self) -> Result<(), String> {
        let storage = self.storage.borrow();
        let slot = storage.text_index.borrow();
        match slot.as_ref() {
            Some(index) => index.check_all(&*storage.backend),
            None => Ok(()),
        }
    }
}
