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
    let marker = crate::buffer::MarkerRef::from_value(marker).expect("marker value");
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
    buf.register_marker_at_emacs_byte_pos(marker_ptr, EmacsBytePos::new(pos), insertion_type);
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
