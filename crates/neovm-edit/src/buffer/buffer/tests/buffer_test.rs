use super::*;

#[doc(hidden)]
pub use crate::buffer::text::{BufferTextBytesSnapshot, ImplementedBufferTextBackendKind};

use crate::buffer::{CharRange, LispCharPos1};

use neovm_value::heap_types::{LispString, OverlayDataInit};

use neovm_value::value::ValueKind;

#[test]
fn buffer_id_equality() {
    crate::test_utils::init_test_tracing();
    let a = BufferId(1);
    let b = BufferId(1);
    let c = BufferId(2);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

fn buf_with_text(text: &str) -> Buffer {
    buf_with_text_backend(text, BufferTextBackendKind::GapBuffer)
}

fn buf_with_text_backend(text: &str, kind: BufferTextBackendKind) -> Buffer {
    let implemented_kind = require_implemented_kind(kind);
    let mut buf = Buffer::new_with_text_backend_kind(
        BufferId(1),
        Value::string("test"),
        implemented_kind,
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert(text);
    buf.widen();
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    buf.set_undo_list(Value::NIL);
    buf.set_modified(false);
    buf
}

fn require_implemented_kind(kind: BufferTextBackendKind) -> ImplementedBufferTextBackendKind {
    kind.implemented()
        .expect("test backend should be implemented")
}

fn marker_chain_anchor_for_test(buf: &Buffer, marker: &Value) -> Option<TextPositionAnchor> {
    marker_chain_lookup_for_test(buf, marker)
        .map(|(byte_pos, char_pos, _)| marker_position_anchor(byte_pos, char_pos))
}

#[derive(Debug, PartialEq, Eq)]
struct OptionalTextPositionSnapshot {
    char_pos: Option<CharPos0>,
    emacs_byte_pos: Option<EmacsBytePos>,
}

impl OptionalTextPositionSnapshot {
    fn new(char_pos: Option<CharPos0>, emacs_byte_pos: Option<EmacsBytePos>) -> Self {
        Self {
            char_pos,
            emacs_byte_pos,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct OptionalEmacsByteRangeSnapshot {
    start: Option<EmacsBytePos>,
    end: Option<EmacsBytePos>,
}

impl OptionalEmacsByteRangeSnapshot {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn from_usize(start: Option<usize>, end: Option<usize>) -> Self {
        Self {
            start: start.map(EmacsBytePos::new),
            end: end.map(EmacsBytePos::new),
        }
    }
}

fn implemented_text_backends() -> impl Iterator<Item = BufferTextBackendKind> {
    BufferTextBackendKind::implemented_variants()
}

fn manager_with_text_backend(kind: BufferTextBackendKind) -> BufferManager {
    let implemented_kind = require_implemented_kind(kind);
    let mut mgr = BufferManager::new();
    mgr.set_default_text_backend_kind(implemented_kind);
    if let Some(id) = mgr.current_buffer_id() {
        mgr.get_mut(id)
            .expect("scratch buffer")
            .convert_text_backend_kind(implemented_kind);
    }
    mgr
}

fn byte_pos_for_char(buf: &Buffer, char_pos: usize) -> usize {
    buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
        .get()
}

fn marker_chain_lookup_for_test(
    buf: &Buffer,
    marker: &Value,
) -> Option<(usize, usize, InsertionType)> {
    // SAFETY: `marker` is the live Value that roots this MarkerObj.
    let marker = unsafe { crate::buffer::MarkerRef::from_value(marker) }.expect("marker value");
    buf.marker_chain_anchor_lookup(marker)
        .map(|(anchor, insertion_type)| {
            (
                anchor.emacs_byte_pos_usize(),
                anchor.char_pos_usize(),
                insertion_type,
            )
        })
}

/// Test helper: allocate a scratch `MarkerObj` via the tagged heap and
/// register it on `buf` at an Emacs byte position.
fn register_marker_for_test(buf: &mut Buffer, pos: usize, insertion_type: InsertionType) -> Value {
    let marker_value = Value::make_marker(neovm_value::heap_types::LispMarker::new(
        neovm_value::heap_types::MarkerBuffer::attached(buf.id),
        crate::buffer::CharPos0::new(0),
        crate::buffer::EmacsBytePos::new(0),
        insertion_type == InsertionType::After,
        true,
    ));
    let marker_ptr = marker_value
        .as_veclike_ptr()
        .expect("freshly allocated marker should have a veclike ptr")
        as *mut neovm_value::tagged::header::MarkerObj;
    // SAFETY: `marker_value` was just allocated and roots `marker_ptr`.
    unsafe {
        buf.register_marker_at_emacs_byte_pos(marker_ptr, EmacsBytePos::new(pos), insertion_type);
    }
    marker_value
}

#[derive(Debug, PartialEq, Eq)]
enum UndoEntrySnapshot {
    Boundary,
    Point(i64),
    Insert {
        beg1: i64,
        end1: i64,
    },
    Delete {
        text: String,
        pos1: i64,
    },
    FirstChange(i64),
    PropertyChange {
        prop: String,
        old_value: String,
        beg1: i64,
        end1: i64,
    },
    MarkerAdjustment {
        /// Marker identity is the object pointer, which differs between
        /// equivalent runs; the undo snapshot is compared structurally, so
        /// the marker is labelled by its buffer and positions instead.
        marker_buffer: Option<u64>,
        marker_bytepos: usize,
        marker_charpos: usize,
        adjustment: i64,
    },
    Other(String),
    ImproperTail(String),
}

#[derive(Debug, PartialEq, Eq)]
struct ManagerEditEntrypointSnapshot {
    buffer_string: String,
    point_byte: usize,
    point_char: usize,
    mark_byte: Option<usize>,
    mark_char: Option<usize>,
    marker_position: Option<TextPositionAnchor>,
    text_properties: Vec<ObjectIntervalRun>,
    undo: Vec<UndoEntrySnapshot>,
}

#[derive(Debug, PartialEq, Eq)]
struct SharedInsertPolicySnapshot {
    base_string: String,
    indirect_string: String,
    base_point: TextPositionAnchor,
    indirect_point: TextPositionAnchor,
    base_mark: OptionalTextPositionSnapshot,
    indirect_mark: OptionalTextPositionSnapshot,
    marker_after_position: Option<TextPositionAnchor>,
    marker_before_position: Option<TextPositionAnchor>,
    indirect_overlay_range: OptionalEmacsByteRangeSnapshot,
    text_properties: Vec<ObjectIntervalRun>,
}

#[derive(Debug, PartialEq, Eq)]
struct BackendMigrationSnapshot {
    backend_kind: BufferTextBackendKind,
    buffer_string: String,
    char_to_byte_4: usize,
    byte_to_char_at_char_4: usize,
    marker_position: Option<TextPositionAnchor>,
    overlay_range: OptionalEmacsByteRangeSnapshot,
    text_properties: Vec<ObjectIntervalRun>,
}

fn marker_position_anchor(byte_pos: usize, char_pos: usize) -> TextPositionAnchor {
    TextPositionAnchor::from_usize(char_pos, byte_pos)
}

#[derive(Debug, PartialEq, Eq)]
struct BufferSwapPayloadSnapshot {
    backend_kind: BufferTextBackendKind,
    buffer_string: String,
    point_byte: usize,
    point_min_byte: usize,
    point_max_byte: usize,
    mark_byte: Option<usize>,
    undo_list: Value,
    text_properties: Vec<ObjectIntervalRun>,
}

#[derive(Debug, PartialEq, Eq)]
struct BackendEditSnapshot {
    buffer_string: String,
    point_byte: usize,
    point_char: usize,
    mark_byte: Option<usize>,
    mark_char: Option<usize>,
    marker_position: Option<TextPositionAnchor>,
    text_properties: Vec<ObjectIntervalRun>,
}

#[cfg(test)]
#[path = "fixnum_positions_test.rs"]
mod fixnum_positions_test;

#[path = "gnu_beg_unchanged_test.rs"]
mod gnu_beg_unchanged_test;

#[cfg(test)]
#[path = "transpose_anchor_test.rs"]
mod transpose_anchor_test;

#[cfg(test)]
#[path = "foreign_marker_probe_test.rs"]
mod foreign_marker_probe_test;

/// GNU `compute_stop_pos`'s delimiter scan must report the delimiter's logical
/// buffer position even when the scan range straddles the gap buffer's hole.
/// `for_each_emacs_byte_range_chunk` yields two slices for such a range --
/// `[start..gap_start)` and then `[gap_end..end)` -- so the reported position
/// is the running logical offset plus the in-chunk index, not
/// `range.start() + index`.
#[test]
fn composition_delimiter_scan_offsets_chunks_across_the_gap() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text(&format!("{}\n", "a".repeat(200)));
    // Move the gap into the scanned range: point at emacs byte 50 and insert
    // one character, which leaves the gap immediately after the insertion.
    // Redisplay does this on every edit, so the straddle is the normal case
    // for a hook stop near edited text.
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(50));
    buf.insert("x");
    assert_eq!(buf.total_emacs_byte_len().get(), 202);
    assert_eq!(
        buf.first_composition_delimiter_emacs_byte(
            crate::buffer::EmacsBytePos::new(40),
            crate::buffer::EmacsBytePos::new(210),
        ),
        Some(crate::buffer::EmacsBytePos::new(201)),
        "the newline at emacs byte 201 is in the chunk after the gap"
    );
}

#[test]
fn long_line_refresh_ignores_text_property_only_writes() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text(&"short commit line\n".repeat(100));
    let before = super::LONG_LINE_FULL_SCANS.load(std::sync::atomic::Ordering::Relaxed);
    assert!(!buf.refresh_long_line_optimizations(1_000));
    let after_initial = super::LONG_LINE_FULL_SCANS.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        after_initial - before,
        1,
        "initial refresh should scan once"
    );
    buf.reset_unchanged_region();
    assert_eq!(buf.unchanged_modified_tick(), buf.modified_tick());

    // 20 text-property writes advance MODIFF by 20 while CHARS_MODIFF stays flat.
    let chars_tick = buf.chars_modified_tick();
    for i in 0..20usize {
        buf.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(i * 18), EmacsBytePos::new(i * 18 + 5)),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
        buf.record_text_property_modification();
    }
    assert_eq!(buf.chars_modified_tick(), chars_tick);
    assert!(buf.modified_tick() >= chars_tick + 20);

    // GNU xdisp.c:20648 gates rescan on CHARS_MODIFF - UNCHANGED_MODIFIED > 8.
    assert!(!buf.refresh_long_line_optimizations(1_000));
    let after_props = super::LONG_LINE_FULL_SCANS.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        after_props, after_initial,
        "text-property-only writes must not trigger a full-buffer long-line rescan"
    );
}

#[test]
fn long_line_refresh_skips_multibyte_byte_to_char_conversion_on_short_lines() {
    crate::test_utils::init_test_tracing();
    // Multibyte lines (~20 bytes each, well below threshold 1000).
    let mut buf = buf_with_text(&"café — commit\n".repeat(500));
    assert_ne!(
        buf.total_char_len().get(),
        buf.total_emacs_byte_len().get(),
        "buffer must be genuinely multibyte"
    );
    let before = crate::buffer::buffer_text::BufferText::multibyte_byte_to_char_calls_for_test();
    assert!(!buf.refresh_long_line_optimizations(1_000));
    let after = crate::buffer::buffer_text::BufferText::multibyte_byte_to_char_calls_for_test();
    assert_eq!(
        after - before,
        0,
        "short lines (byte_len <= threshold) must never call emacs_byte_pos_to_char_pos_multibyte"
    );
    assert!(!buf.long_line_optimizations_p());

    // A multibyte line whose byte length exceeds threshold (600 * 2 = 1200 bytes > 1000)
    // but whose character count does not (600 chars <= 1000) must NOT set the flag.
    let mut mb_below = buf_with_text(&format!("{}\n", "é".repeat(600)));
    assert!(!mb_below.refresh_long_line_optimizations(1_000));
    assert!(!mb_below.long_line_optimizations_p());

    // A multibyte line whose character count exceeds threshold (1100 > 1000) MUST set the flag.
    let mut mb_above = buf_with_text(&format!("{}\n", "é".repeat(1_100)));
    assert!(mb_above.refresh_long_line_optimizations(1_000));
    assert!(mb_above.long_line_optimizations_p());
}

#[test]
fn long_line_refresh_rearms_on_real_character_insert_and_widen() {
    crate::test_utils::init_test_tracing();
    // Case A: after redisplay ack + text-property writes, a real character insertion
    // rejoins CHARS_MODIFF to MODIFF (CHARS_MODIFF - UNCHANGED_MODIFIED > 8) and
    // re-arms the scan immediately.
    let mut buf = buf_with_text(&"short line\n".repeat(50));
    assert!(!buf.refresh_long_line_optimizations(1_000));
    buf.reset_unchanged_region();
    for i in 0..20usize {
        buf.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(i * 11), EmacsBytePos::new(i * 11 + 4)),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
        buf.record_text_property_modification();
    }
    assert!(!buf.refresh_long_line_optimizations(1_000));
    buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    buf.insert(&"x".repeat(1_200));
    assert!(
        buf.refresh_long_line_optimizations(1_000),
        "real character insertion after text-property writes must re-arm long-line check"
    );
    assert!(buf.long_line_optimizations_p());

    // Case B: > 8 real character insertions with no text-property writes re-arm the check.
    let mut buf2 = buf_with_text(&"short\n".repeat(100));
    assert!(!buf2.refresh_long_line_optimizations(1_000));
    buf2.reset_unchanged_region();
    buf2.goto_emacs_byte_pos(buf2.point_max_emacs_byte_pos());
    for _ in 0..9 {
        buf2.insert(&"y".repeat(150));
    }
    assert!(
        buf2.refresh_long_line_optimizations(1_000),
        "> 8 character insertions must re-arm long-line check"
    );
    assert!(buf2.long_line_optimizations_p());

    // Case C: widening a narrowed buffer (clip_changed, CHARS_MODIFF unchanged) re-arms.
    let mut buf3 = buf_with_text(&format!("short\n{}\n", "z".repeat(1_200)));
    buf3.narrow_to_emacs_byte_range(EmacsByteRange::new(
        EmacsBytePos::new(0),
        EmacsBytePos::new(6),
    ));
    assert!(!buf3.refresh_long_line_optimizations(1_000));
    assert!(!buf3.long_line_optimizations_p());
    buf3.reset_unchanged_region();
    let chars_before_widen = buf3.chars_modified_tick();
    buf3.widen();
    assert_eq!(buf3.chars_modified_tick(), chars_before_widen);
    assert!(
        buf3.refresh_long_line_optimizations(1_000),
        "widening into a long line (clip_changed) must re-arm long-line check"
    );
    assert!(buf3.long_line_optimizations_p());

    // Case D (GNU UNCHANGED_MODIFIED contract): interleaved 1-char edit + 1 prop write
    // per redisplay ack keeps CHARS_MODIFF - UNCHANGED_MODIFIED == 1 <= 8 every frame,
    // so it never rescans even after the line grows past threshold.
    let mut buf4 = buf_with_text(&format!("{}\n", "a".repeat(995)));
    assert!(!buf4.refresh_long_line_optimizations(1_000));
    buf4.reset_unchanged_region();
    let scans_before_interleaved =
        super::LONG_LINE_FULL_SCANS.load(std::sync::atomic::Ordering::Relaxed);
    for _ in 0..15 {
        buf4.goto_emacs_byte_pos(EmacsBytePos::new(0));
        buf4.insert("a");
        buf4.record_text_property_modification();
        assert!(
            !buf4.refresh_long_line_optimizations(1_000),
            "interleaved 1-char edit + 1 prop write per redisplay ack must not rescan"
        );
        buf4.reset_unchanged_region();
    }
    let scans_after_interleaved =
        super::LONG_LINE_FULL_SCANS.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scans_after_interleaved, scans_before_interleaved,
        "CHARS_MODIFF - UNCHANGED_MODIFIED stayed <= 8 on every frame"
    );
    assert!(!buf4.long_line_optimizations_p());

    // Case E (GNU UNCHANGED_MODIFIED contract): 10 prop writes + 1-char insert before
    // redisplay ack advances CHARS_MODIFF - UNCHANGED_MODIFIED to 11 > 8, so the next
    // refresh scans and observes the 1011-char line.
    for _ in 0..10 {
        buf4.record_text_property_modification();
    }
    buf4.goto_emacs_byte_pos(EmacsBytePos::new(0));
    buf4.insert("a");
    assert!(
        buf4.chars_modified_tick() - buf4.unchanged_modified_tick() > 8,
        "10 prop writes + 1 char insert must advance CHARS_MODIFF - UNCHANGED_MODIFIED > 8"
    );
    assert!(
        buf4.refresh_long_line_optimizations(1_000),
        "prop burst + 1-char insert before redisplay ack must trigger scan matching GNU"
    );
    assert!(buf4.long_line_optimizations_p());
}
