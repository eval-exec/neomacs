use super::*;
use crate::buffer::text::{BufferTextBytesSnapshot, ImplementedBufferTextBackendKind};
use crate::buffer::{CharRange, LispCharPos1};
use crate::emacs_core::value::ValueKind;
use crate::heap_types::{LispString, OverlayData};

#[test]
fn forwarded_slot_predicates_are_closed_typed_contracts() {
    use BufferSlotPredicate::{
        Fraction, Integer, Number, OverwriteMode, String as StringPredicate, Symbol, Unrestricted,
        VerticalScrollBar,
    };
    use BufferSlotPredicateError::{Choice, Range, WrongType};

    for predicate in [
        Unrestricted,
        StringPredicate,
        Symbol,
        Integer,
        Number,
        Fraction,
        VerticalScrollBar,
        OverwriteMode,
    ] {
        assert_eq!(predicate.check(Value::NIL), Ok(()));
    }

    assert_eq!(
        StringPredicate.check(Value::symbol("not-a-string")),
        Err(WrongType("stringp"))
    );
    assert_eq!(
        Symbol.check(Value::string("not-a-symbol")),
        Err(WrongType("symbolp"))
    );
    assert_eq!(
        Integer.check(Value::make_float(1.0)),
        Err(WrongType("integerp"))
    );
    assert_eq!(Number.check(Value::fixnum(1)), Ok(()));
    assert_eq!(
        Fraction.check(Value::string("far")),
        Err(WrongType("numberp"))
    );
    assert_eq!(
        Fraction.check(Value::make_float(2.0)),
        Err(Range("Value should be from 0.0 to 1.0"))
    );
    assert_eq!(VerticalScrollBar.check(Value::symbol("left")), Ok(()));
    assert_eq!(
        VerticalScrollBar.check(Value::symbol("middle")),
        Err(Choice("One of nil, t, left or right should be specified"))
    );
    assert_eq!(
        OverwriteMode.check(Value::symbol("overwrite-mode-binary")),
        Ok(())
    );
    assert_eq!(
        OverwriteMode.check(Value::symbol("replace-everything")),
        Err(Choice(
            "One of nil, overwrite-mode-textual or overwrite-mode-binary should be specified"
        ))
    );

    assert_eq!(
        lookup_buffer_slot("buffer-read-only").unwrap().predicate,
        Unrestricted,
        "GNU permits non-boolean sentinel values such as `read-mostly`"
    );
    assert_eq!(lookup_buffer_slot("major-mode").unwrap().predicate, Symbol);
    assert_eq!(
        lookup_buffer_slot("overwrite-mode").unwrap().predicate,
        OverwriteMode
    );
    assert_eq!(
        lookup_buffer_slot("scroll-up-aggressively")
            .unwrap()
            .predicate,
        Fraction
    );
}

// -----------------------------------------------------------------------
// Helper: create a buffer with some text and correct zv.
// -----------------------------------------------------------------------
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

fn marker_position_anchor(byte_pos: usize, char_pos: usize) -> TextPositionAnchor {
    TextPositionAnchor::from_usize(char_pos, byte_pos)
}

fn marker_chain_anchor_for_test(buf: &Buffer, marker_id: u64) -> Option<TextPositionAnchor> {
    marker_chain_lookup_for_test(buf, marker_id)
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

fn buffer_text_property_snapshot(buf: &Buffer) -> Vec<ObjectIntervalRun> {
    buf.text_props_intervals_snapshot_for_test()
        .into_iter()
        .map(|interval| {
            let properties = interval
                .key_order
                .iter()
                .copied()
                .map(|key| (key, interval.properties[&key]))
                .collect();
            ObjectIntervalRun::new(
                CharPos0::new(interval.start),
                CharPos0::new(interval.end),
                properties,
            )
        })
        .collect()
}

fn byte_pos_for_char(buf: &Buffer, char_pos: usize) -> usize {
    buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
        .get()
}

fn char_pos_for_byte(buf: &Buffer, byte_pos: usize) -> usize {
    buf.emacs_byte_pos_to_char_pos_clamped(EmacsBytePos::new(byte_pos))
        .get()
}

fn overlay_start_for_test(buf: &Buffer, overlay: Value) -> Option<usize> {
    buf.overlays
        .overlay_start_emacs_byte_pos(overlay)
        .map(EmacsBytePos::get)
}

fn overlay_end_for_test(buf: &Buffer, overlay: Value) -> Option<usize> {
    buf.overlays
        .overlay_end_emacs_byte_pos(overlay)
        .map(EmacsBytePos::get)
}

fn marker_chain_lookup_for_test(
    buf: &Buffer,
    marker_id: u64,
) -> Option<(usize, usize, InsertionType)> {
    buf.marker_chain_anchor_lookup(marker_id)
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
fn register_marker_for_test(
    buf: &mut Buffer,
    marker_id: u64,
    pos: usize,
    insertion_type: InsertionType,
) -> Value {
    let marker_value = Value::make_marker(crate::heap_types::LispMarker {
        buffer: Some(buf.id),
        insertion_type: insertion_type == InsertionType::After,
        marker_id: Some(marker_id),
        bytepos: 0,
        charpos: 0,
        last_position_valid: true,
        next_marker: std::ptr::null_mut(),
    });
    let marker_ptr = marker_value
        .as_veclike_ptr()
        .expect("freshly allocated marker should have a veclike ptr")
        as *mut crate::tagged::header::MarkerObj;
    buf.register_marker_at_emacs_byte_pos(
        marker_ptr,
        marker_id,
        EmacsBytePos::new(pos),
        insertion_type,
    );
    marker_value
}

// -----------------------------------------------------------------------
// Buffer creation & naming
// -----------------------------------------------------------------------

#[test]
fn new_buffer_is_empty() {
    crate::test_utils::init_test_tracing();
    let buf = Buffer::new(
        BufferId(1),
        Value::string("*scratch*"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(buf.name_value(), Value::string("*scratch*"));
    assert_eq!(buf.point_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 0);
    assert_eq!(buf.total_char_len().get(), 0);
    assert!(!buf.is_modified());
    assert!(!buf.get_read_only());
    assert!(buf.get_multibyte());
    assert!(buf.file_name_value().is_nil());
    assert!(buf.mark_emacs_byte_pos().map(|pos| pos.get()).is_none());
}

#[test]
fn buffer_manager_gc_traces_buffer_and_dead_buffer_names() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let live_id = mgr.create_buffer("live");
    let dead_id = mgr.create_buffer("dead");
    assert!(mgr.kill_buffer(dead_id));

    let live_name = mgr.get(live_id).expect("live buffer").name_value();
    let dead_name = mgr
        .dead_buffer_last_name_value(dead_id)
        .expect("dead buffer name");

    let mut roots = Vec::new();
    mgr.trace_roots(&mut roots);

    assert!(roots.contains(&live_name));
    assert!(roots.contains(&dead_name));
}

#[test]
fn buffer_id_equality() {
    crate::test_utils::init_test_tracing();
    let a = BufferId(1);
    let b = BufferId(1);
    let c = BufferId(2);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn create_indirect_buffer_shares_root_text_and_updates_siblings() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        let mut mgr = manager_with_text_backend(kind);
        let base_id = mgr.current_buffer_id().expect("scratch buffer");

        let _ = mgr.insert_into_buffer(base_id, "abcd");
        let indirect_id = mgr
            .create_indirect_buffer(base_id, "*indirect*", false)
            .expect("indirect buffer");

        let base = mgr.get(base_id).expect("base buffer");
        let indirect = mgr.get(indirect_id).expect("indirect buffer");
        assert_eq!(base.text_backend_kind(), kind);
        assert_eq!(indirect.text_backend_kind(), kind);
        assert_eq!(indirect.base_buffer, Some(base_id));
        assert!(base.shares_text_storage_with(indirect));
        assert_eq!(indirect.buffer_string(), "abcd");

        let _ = mgr.goto_buffer_emacs_byte_pos(base_id, crate::buffer::EmacsBytePos::new(0));
        let _ = mgr.insert_into_buffer(base_id, "zz");
        assert_eq!(mgr.get(base_id).unwrap().buffer_string(), "zzabcd");
        assert_eq!(mgr.get(indirect_id).unwrap().buffer_string(), "zzabcd");

        let _ = mgr.delete_buffer_emacs_byte_range(
            indirect_id,
            crate::buffer::EmacsByteRange::from_usize(2, 4),
        );
        assert_eq!(mgr.get(base_id).unwrap().buffer_string(), "zzcd");
        assert_eq!(mgr.get(indirect_id).unwrap().buffer_string(), "zzcd");
    }
}

/// GNU `Fmake_indirect_buffer` runs `reset_buffer` on the new buffer
/// (`src/buffer.c:896`), which sets `b->modtime` to the unknown sentinel
/// (`src/buffer.c:1092`), and sets `b->base_buffer` -- the pointer
/// `record_first_change` dereferences to read a modtime (`src/undo.c:213-214`).
///
/// So an indirect buffer has NO modtime of its own even when CLONE copies the
/// base's other state (GNU 31.0.90 reports `(visited-file-modtime)` = 0 in a
/// `(make-indirect-buffer base "i" t)` buffer whose base visits a file), while
/// its first change records the base's -- read live, so a `save-buffer` in the
/// base after the indirect buffer was made is visible.
#[test]
fn an_indirect_buffer_has_no_modtime_of_its_own_and_follows_its_bases() {
    crate::test_utils::init_test_tracing();
    use crate::buffer::VisitedFileModtime;

    for clone in [false, true] {
        let mut mgr = BufferManager::new();
        let base_id = mgr.current_buffer_id().expect("scratch buffer");
        mgr.get_mut(base_id)
            .expect("base buffer")
            .set_visited_file_modtime(VisitedFileModtime::Known { sec: 11, nsec: 22 });

        let indirect_id = mgr
            .create_indirect_buffer(base_id, "*indirect-modtime*", clone)
            .expect("indirect buffer");

        let base_cell = mgr.get(base_id).expect("base buffer").share_modtime_cell();
        let indirect = mgr.get(indirect_id).expect("indirect buffer");
        assert_eq!(
            indirect.visited_file_modtime(),
            VisitedFileModtime::Unknown,
            "an indirect buffer visits no file, whatever CLONE ({clone}) copies"
        );
        assert!(
            indirect.follows_modtime_cell_of(&base_cell),
            "the first-change recorder must reach the base's live cell (clone: {clone})"
        );

        // A later change to the base is what the indirect buffer records.
        mgr.get_mut(base_id)
            .expect("base buffer")
            .set_visited_file_modtime(VisitedFileModtime::Known { sec: 33, nsec: 44 });
        let recorded = mgr
            .get(indirect_id)
            .expect("indirect buffer")
            .first_change_modtime()
            .to_lisp_value();
        let base_reported = mgr
            .get(base_id)
            .expect("base buffer")
            .visited_file_modtime_value();
        assert_eq!(
            crate::emacs_core::print::print_value(&recorded),
            crate::emacs_core::print::print_value(&base_reported),
            "the (t . TIME) datum is the base's current modtime (clone: {clone})"
        );
    }
}

#[test]
fn create_indirect_buffer_flattens_double_indirection() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let base_id = mgr.current_buffer_id().expect("scratch buffer");
    let first_id = mgr
        .create_indirect_buffer(base_id, "*indirect-one*", false)
        .expect("first indirect");
    let second_id = mgr
        .create_indirect_buffer(first_id, "*indirect-two*", false)
        .expect("second indirect");

    assert_eq!(mgr.get(first_id).unwrap().base_buffer, Some(base_id));
    assert_eq!(mgr.get(second_id).unwrap().base_buffer, Some(base_id));
    let base = mgr.get(base_id).expect("base buffer");
    let second = mgr.get(second_id).expect("second indirect buffer");
    assert!(base.shares_text_storage_with(second));
}

/// Build a manager whose scratch buffer holds `text` with undo enabled,
/// point at the end, and `point_before_command_or_undo` set to the end —
/// the precondition GNU establishes before a case-region command.
fn casify_manager_with_text(text: &str) -> (BufferManager, BufferId) {
    let mut mgr = BufferManager::new();
    let id = mgr.current_buffer_id().expect("scratch buffer");
    {
        // Enable undo before filling so the insert records `(BEG . END)` and
        // the first-change `(t . 0)` exactly like the GNU repro flow.
        mgr.get_mut(id)
            .expect("scratch buffer")
            .set_undo_list(Value::NIL);
    }
    let _ = mgr.insert_into_buffer(id, text);
    {
        let buf = mgr.get_mut(id).expect("scratch buffer");
        let end_char = buf.point_char_pos();
        buf.saved_point_before_command.save(id, end_char);
        // GNU inserts an undo boundary between the buffer fill and the case op.
        let mut ul = buf.get_undo_list();
        crate::buffer::undo::undo_list_boundary(&mut ul);
        buf.set_undo_list(ul);
    }
    (mgr, id)
}

#[test]
fn casify_region_records_gnu_undo_shape_when_changed() {
    crate::test_utils::init_test_tracing();
    // GNU `casify_region` (casefiddle.c) records `record_delete (start,
    // ORIGINAL)` then `record_insert (start, NEW_LEN)`, yielding the undo
    // shape `((1 . 6) ("hello" . 1) 12 nil ...)` for `(upcase-region 1 6)`
    // on "hello world" with point at end.
    let (mut mgr, id) = casify_manager_with_text("hello world");

    mgr.casify_replace_buffer_emacs_byte_range_lisp_string(
        id,
        crate::buffer::EmacsByteRange::from_usize(0, 5),
        &LispString::from_utf8("HELLO"),
    );

    let buf = mgr.get(id).expect("buffer");
    assert_eq!(buf.buffer_string(), "HELLO world");
    let ul = buf.get_undo_list();

    // 0: (1 . 6) — insertion range at start.
    let insert_entry = undo_nth(ul, 0);
    assert_eq!(insert_entry.cons_car(), Value::fixnum(1));
    assert_eq!(insert_entry.cons_cdr(), Value::fixnum(6));
    // 1: ("hello" . 1) — original-text deletion at start.
    let delete_entry = undo_nth(ul, 1);
    assert_eq!(delete_entry.cons_car().as_utf8_str(), Some("hello"));
    assert_eq!(delete_entry.cons_cdr(), Value::fixnum(1));
    // 2: 12 — point entry.
    assert_eq!(undo_nth(ul, 2), Value::fixnum(12));
    // 3: nil — boundary.
    assert!(undo_nth(ul, 3).is_nil());
}

#[test]
fn casify_region_records_undo_even_when_unchanged() {
    crate::test_utils::init_test_tracing();
    // GNU records the delete+insert even when no character changes
    // (`modify_text` + `record_delete` + `record_insert` always run).  With
    // point at the end of the region (6), the original-text delete pos is
    // negative: `((1 . 6) ("HELLO" . -1) 6 nil ...)`.
    let mut mgr = BufferManager::new();
    let id = mgr.current_buffer_id().expect("scratch buffer");
    mgr.get_mut(id)
        .expect("scratch buffer")
        .set_undo_list(Value::NIL);
    let _ = mgr.insert_into_buffer(id, "HELLO world");
    {
        let buf = mgr.get_mut(id).expect("scratch buffer");
        // Point at buffer position 6 (end of region) == Emacs byte pos 5.
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(5));
        buf.saved_point_before_command
            .save(id, buf.point_char_pos());
        let mut ul = buf.get_undo_list();
        crate::buffer::undo::undo_list_boundary(&mut ul);
        buf.set_undo_list(ul);
    }

    mgr.casify_replace_buffer_emacs_byte_range_lisp_string(
        id,
        crate::buffer::EmacsByteRange::from_usize(0, 5),
        &LispString::from_utf8("HELLO"),
    );

    let buf = mgr.get(id).expect("buffer");
    assert_eq!(buf.buffer_string(), "HELLO world");
    let ul = buf.get_undo_list();
    let insert_entry = undo_nth(ul, 0);
    assert_eq!(insert_entry.cons_car(), Value::fixnum(1));
    assert_eq!(insert_entry.cons_cdr(), Value::fixnum(6));
    let delete_entry = undo_nth(ul, 1);
    assert_eq!(
        delete_entry.cons_car().as_utf8_str(),
        Some("HELLO"),
        "undo must record the (unchanged) original text"
    );
    assert_eq!(delete_entry.cons_cdr(), Value::fixnum(-1));
    assert_eq!(undo_nth(ul, 2), Value::fixnum(6));
}

/// `casify_region_undo_restores_original_text' used to drive
/// `BufferManager::undo_buffer' -- the third undo replay loop, deleted with
/// the Rust `undo' subr that was its only caller (DIVERGENCES.md 150).
/// Replay is `primitive-undo' (lisp/simple.el:3645), which is Lisp, so the
/// round trip is asked of the runtime.  The test above still pins what
/// casification RECORDS, which is this layer's actual job.
///
/// Measured under GNU Emacs 31.0.90 `-Q --batch' first
/// (tmp/pw56-moved-tests-gnu.txt).
#[test]
fn casify_region_undo_restores_original_text_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_all(
            r#"
(with-temp-buffer
  (insert "hello world")
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (upcase-region 1 6)
  (let ((after (buffer-string)))
    (undo-boundary)
    (setq last-command nil)
    (undo)
    (list after (buffer-string))))
"#,
        ),
        vec!["OK (\"HELLO world\" \"hello world\")"],
    );
}

/// An indirect buffer sees its base buffer's undo history and undoing through
/// it edits the shared text.
///
/// GNU keeps that true by copying `undo_list' between base and indirect on
/// every `set_buffer_internal_1' (src/buffer.c:2357,2367), so this is a
/// property of the pair, not of either buffer.  It used to be asserted
/// against `BufferManager::undo_buffer'; that loop is gone
/// (DIVERGENCES.md 150) and the replay is `lisp/simple.el's' `undo'.
///
/// Measured under GNU Emacs 31.0.90 `-Q --batch' first
/// (tmp/pw56-moved-tests-gnu.txt).
#[test]
fn indirect_buffers_keep_undo_state_in_sync_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_all(
            r#"
(let ((base (generate-new-buffer "neo-base")))
  (with-current-buffer base (buffer-enable-undo) (setq buffer-undo-list nil))
  (let ((ind (make-indirect-buffer base "neo-ind")))
    (with-current-buffer base (insert "abc"))
    (prog1
        (with-current-buffer ind
          (let ((seen (and buffer-undo-list (not (eq buffer-undo-list t)))))
            (undo-boundary)
            (setq last-command nil)
            (undo)
            (list seen
                  (with-current-buffer base (buffer-string))
                  (buffer-string))))
      (kill-buffer ind) (kill-buffer base))))
"#,
        ),
        vec!["OK (t \"\" \"\")"],
    );
}

#[test]
fn from_dump_restores_indirect_buffer_shared_text_state() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        let implemented_kind = require_implemented_kind(kind);
        let mut mgr = manager_with_text_backend(kind);
        let base_id = mgr.current_buffer_id().expect("scratch buffer");
        let _ = mgr.insert_into_buffer(base_id, "abcdef");
        let indirect_id = mgr
            .create_indirect_buffer(base_id, "*indirect-restored*", false)
            .expect("indirect buffer");
        let _ = mgr.put_buffer_text_property_in_emacs_byte_range(
            base_id,
            EmacsByteRange::from_usize(1, 4),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
        let _ = mgr.insert_into_buffer(base_id, "z");

        let mut dumped = mgr.dump_buffers().clone();
        let independent_indirect = dumped.get(&indirect_id).expect("indirect buffer").clone();
        let indirect = dumped.get_mut(&indirect_id).expect("indirect buffer");
        indirect.replace_text_snapshot_for_test(
            BufferTextBytesSnapshot::new(
                independent_indirect.dump_text_bytes(),
                independent_indirect.get_multibyte(),
            ),
            independent_indirect.dump_text_backend_kind(),
        );
        indirect.replace_text_props_for_test(independent_indirect.text_props_snapshot());
        indirect.undo_state =
            SharedUndoState::from_parts(independent_indirect.get_undo_list(), false, false);

        let restored = BufferManager::from_dump(
            dumped,
            mgr.dump_current(),
            mgr.dump_next_id(),
            mgr.dump_next_marker_id(),
            None,
            None,
            implemented_kind,
            crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
        );

        let base = restored.get(base_id).expect("base buffer");
        let indirect = restored.get(indirect_id).expect("indirect buffer");
        assert_eq!(base.text_backend_kind(), kind);
        assert_eq!(indirect.text_backend_kind(), kind);
        assert!(base.shares_text_storage_with(indirect));
        assert!(base.undo_state.shares_with(&indirect.undo_state));
        assert_eq!(
            indirect.text_props_get_property_at_emacs_byte_pos(
                crate::buffer::EmacsBytePos::new(1),
                Value::symbol("face")
            ),
            Some(Value::symbol("bold"))
        );
    }
}

#[test]
fn from_dump_preserves_dumped_buffer_order() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let scratch = mgr.current_buffer_id().expect("scratch buffer");
    let one = mgr.create_buffer("one");
    let two = mgr.create_buffer("two");
    let three = mgr.create_buffer("three");

    let restored = BufferManager::from_dump(
        mgr.dump_buffers().clone(),
        Some(three),
        mgr.dump_next_id(),
        mgr.dump_next_marker_id(),
        Some(&[two, scratch, three, one]),
        None,
        require_implemented_kind(BufferTextBackendKind::GapBuffer),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );

    assert_eq!(restored.buffer_list(), vec![two, scratch, three, one]);
}

#[test]
fn indirect_buffers_preserve_narrowing_across_shared_edits() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        let mut mgr = manager_with_text_backend(kind);
        let base_id = mgr.current_buffer_id().expect("scratch buffer");
        let _ = mgr.insert_into_buffer(base_id, "abcdef");
        let indirect_id = mgr
            .create_indirect_buffer(base_id, "*indirect-narrow*", false)
            .expect("indirect buffer");

        let _ =
            mgr.narrow_buffer_to_emacs_byte_range(indirect_id, EmacsByteRange::from_usize(2, 6));
        let _ = mgr.goto_buffer_emacs_byte_pos(indirect_id, crate::buffer::EmacsBytePos::new(4));

        let _ = mgr.goto_buffer_emacs_byte_pos(base_id, crate::buffer::EmacsBytePos::new(0));
        let _ = mgr.insert_into_buffer(base_id, "zz");

        let indirect = mgr.get(indirect_id).expect("indirect buffer");
        assert_eq!(indirect.text_backend_kind(), kind);
        assert_eq!(indirect.point_min_emacs_byte_pos().get(), 4);
        assert_eq!(indirect.point_max_emacs_byte_pos().get(), 8);
        assert_eq!(indirect.point_emacs_byte_pos().get(), 6);
        assert_eq!(indirect.buffer_string(), "cdef");

        let _ = mgr.delete_buffer_emacs_byte_range(
            base_id,
            crate::buffer::EmacsByteRange::from_usize(0, 2),
        );

        let indirect = mgr.get(indirect_id).expect("indirect buffer");
        assert_eq!(indirect.text_backend_kind(), kind);
        assert_eq!(indirect.point_min_emacs_byte_pos().get(), 2);
        assert_eq!(indirect.point_max_emacs_byte_pos().get(), 6);
        assert_eq!(indirect.point_emacs_byte_pos().get(), 4);
        assert_eq!(indirect.buffer_string(), "cdef");
    }
}

#[test]
fn cloned_indirect_buffers_do_not_share_base_state_markers() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let base_id = mgr.current_buffer_id().expect("scratch buffer");
    let _ = mgr.insert_into_buffer(base_id, "aaa\nbbb\nccc\n");

    // The first indirect forces the base buffer to allocate the hidden
    // PT/BEGV/ZV markers GNU uses to preserve per-buffer state.
    let _first = mgr
        .create_indirect_buffer(base_id, "*indirect-state-first*", true)
        .expect("first indirect buffer");
    let second = mgr
        .create_indirect_buffer(base_id, "*indirect-state-second*", true)
        .expect("second indirect buffer");

    mgr.set_current(second);
    let _ = mgr.narrow_buffer_to_emacs_byte_range(second, EmacsByteRange::from_usize(4, 7));
    mgr.set_current(base_id);

    let base = mgr.get(base_id).expect("base buffer");
    assert_eq!(base.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(
        base.point_max_emacs_byte_pos().get(),
        base.total_emacs_byte_len().get()
    );

    let second = mgr.get(second).expect("second indirect buffer");
    assert_eq!(second.point_min_emacs_byte_pos().get(), 4);
    assert_eq!(second.point_max_emacs_byte_pos().get(), 7);
}

#[test]
fn cloned_indirect_buffers_do_not_share_base_mark_marker() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let base_id = mgr.current_buffer_id().expect("scratch buffer");
    let _ = mgr.insert_into_buffer(base_id, "abcdef");
    mgr.get_mut(base_id)
        .expect("base buffer")
        .set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(2));

    let indirect = mgr
        .create_indirect_buffer(base_id, "*indirect-mark-clone*", true)
        .expect("indirect buffer");
    mgr.get_mut(indirect)
        .expect("indirect buffer")
        .set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(5));

    assert_eq!(
        mgr.get(base_id)
            .expect("base buffer")
            .mark_emacs_byte_pos()
            .map(|pos| pos.get()),
        Some(2)
    );
    assert_eq!(
        mgr.get(indirect)
            .expect("indirect buffer")
            .mark_emacs_byte_pos()
            .map(|pos| pos.get()),
        Some(5)
    );
}

#[test]
fn cloned_indirect_buffers_copy_overlay_objects_and_indexes() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let base_id = mgr.current_buffer_id().expect("scratch buffer");
    let _ = mgr.insert_into_buffer(base_id, "abcdef");
    let original = Value::make_overlay(OverlayData {
        serial: 0,
        plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        buffer: Some(base_id),
        start: 1,
        end: 4,
        position_handle: None,
        front_advance: true,
        rear_advance: false,
    });
    mgr.get_mut(base_id)
        .expect("base buffer")
        .overlays
        .insert_overlay(original);

    let indirect_id = mgr
        .create_indirect_buffer(base_id, "*indirect-overlay-clone*", true)
        .expect("indirect buffer");
    let copied = mgr
        .get(indirect_id)
        .expect("indirect buffer")
        .overlays
        .overlays_in_gnu_lists_order();
    assert_eq!(copied.len(), 1);
    let copied = copied[0];
    assert!(!crate::emacs_core::value::eq_value(&original, &copied));
    assert_eq!(copied.as_overlay_data().unwrap().buffer, Some(indirect_id));
    assert!(!crate::emacs_core::value::eq_value(
        &original.as_overlay_data().unwrap().plist,
        &copied.as_overlay_data().unwrap().plist,
    ));
    assert_eq!(
        overlay_start_for_test(mgr.get(indirect_id).unwrap(), copied),
        Some(1)
    );
    assert_eq!(
        overlay_end_for_test(mgr.get(indirect_id).unwrap(), copied),
        Some(4)
    );

    mgr.get_mut(indirect_id)
        .unwrap()
        .overlays
        .move_overlay_to_emacs_byte_range(copied, EmacsByteRange::from_usize(2, 5));
    assert_eq!(
        overlay_start_for_test(mgr.get(base_id).unwrap(), original),
        Some(1)
    );
    assert_eq!(
        overlay_start_for_test(mgr.get(indirect_id).unwrap(), copied),
        Some(2)
    );
}

#[test]
fn indirect_buffer_overlays_track_shared_edits() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        let mut mgr = manager_with_text_backend(kind);
        let base_id = mgr.current_buffer_id().expect("scratch buffer");
        let _ = mgr.insert_into_buffer(base_id, "abcdef");
        let indirect_id = mgr
            .create_indirect_buffer(base_id, "*indirect-overlays*", false)
            .expect("indirect buffer");

        let overlay = Value::make_overlay(OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(indirect_id),
            start: 2,
            end: 4,
            position_handle: None,
            front_advance: false,
            rear_advance: false,
        });
        mgr.get_mut(indirect_id)
            .expect("indirect buffer")
            .overlays
            .insert_overlay(overlay);

        let _ = mgr.goto_buffer_emacs_byte_pos(base_id, crate::buffer::EmacsBytePos::new(0));
        let _ = mgr.insert_into_buffer(base_id, "zz");
        let indirect = mgr.get(indirect_id).expect("indirect buffer");
        assert_eq!(indirect.text_backend_kind(), kind);
        assert_eq!(overlay_start_for_test(indirect, overlay), Some(4));
        assert_eq!(overlay_end_for_test(indirect, overlay), Some(6));

        let _ = mgr.goto_buffer_emacs_byte_pos(base_id, crate::buffer::EmacsBytePos::new(4));
        let _ = mgr.insert_into_buffer_before_markers(base_id, "yy");
        let indirect = mgr.get(indirect_id).expect("indirect buffer");
        assert_eq!(indirect.text_backend_kind(), kind);
        assert_eq!(overlay_start_for_test(indirect, overlay), Some(6));
        assert_eq!(overlay_end_for_test(indirect, overlay), Some(8));

        let _ = mgr.delete_buffer_emacs_byte_range(
            base_id,
            crate::buffer::EmacsByteRange::from_usize(0, 2),
        );
        let indirect = mgr.get(indirect_id).expect("indirect buffer");
        assert_eq!(indirect.text_backend_kind(), kind);
        assert_eq!(overlay_start_for_test(indirect, overlay), Some(4));
        assert_eq!(overlay_end_for_test(indirect, overlay), Some(6));
    }
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

fn buffer_swap_payload_snapshot(buf: &Buffer) -> BufferSwapPayloadSnapshot {
    BufferSwapPayloadSnapshot {
        backend_kind: buf.text_backend_kind(),
        buffer_string: buf.buffer_string(),
        point_byte: buf.point_emacs_byte_pos().get(),
        point_min_byte: buf.point_min_emacs_byte_pos().get(),
        point_max_byte: buf.point_max_emacs_byte_pos().get(),
        mark_byte: buf.mark_emacs_byte_pos().map(|pos| pos.get()),
        undo_list: buf.get_undo_list(),
        text_properties: buffer_text_property_snapshot(buf),
    }
}

fn buffer_byte_pos_for_char(mgr: &BufferManager, id: BufferId, char_pos: usize) -> usize {
    byte_pos_for_char(mgr.get(id).expect("buffer"), char_pos)
}

#[test]
fn implemented_text_backends_match_buffer_swap_text_side_effects() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");

    for left_kind in implemented_text_backends() {
        for right_kind in implemented_text_backends() {
            let mut mgr = BufferManager::new();
            let left_id = mgr.current_buffer_id().expect("scratch buffer");
            let right_id = mgr.create_buffer("*swap-right*");
            mgr.get_mut(left_id)
                .expect("left buffer")
                .convert_text_backend_kind(require_implemented_kind(left_kind));
            mgr.get_mut(right_id)
                .expect("right buffer")
                .convert_text_backend_kind(require_implemented_kind(right_kind));

            mgr.insert_into_buffer(left_id, "aébc日本")
                .expect("left insert");
            mgr.insert_into_buffer(right_id, "uvwxyzλ")
                .expect("right insert");

            let left_prop_start = buffer_byte_pos_for_char(&mgr, left_id, 1);
            let left_prop_end = buffer_byte_pos_for_char(&mgr, left_id, 5);
            mgr.put_buffer_text_property_in_emacs_byte_range(
                left_id,
                EmacsByteRange::from_usize(left_prop_start, left_prop_end),
                face,
                bold,
            )
            .expect("left property");
            let right_prop_start = buffer_byte_pos_for_char(&mgr, right_id, 2);
            let right_prop_end = buffer_byte_pos_for_char(&mgr, right_id, 7);
            mgr.put_buffer_text_property_in_emacs_byte_range(
                right_id,
                EmacsByteRange::from_usize(right_prop_start, right_prop_end),
                face,
                italic,
            )
            .expect("right property");

            let left_marker_pos = buffer_byte_pos_for_char(&mgr, left_id, 3);
            let left_marker = register_marker_for_test(
                mgr.get_mut(left_id).expect("left buffer"),
                501,
                left_marker_pos,
                InsertionType::After,
            );
            let right_marker_pos = buffer_byte_pos_for_char(&mgr, right_id, 4);
            let right_marker = register_marker_for_test(
                mgr.get_mut(right_id).expect("right buffer"),
                502,
                right_marker_pos,
                InsertionType::Before,
            );

            let left_overlay_start = buffer_byte_pos_for_char(&mgr, left_id, 1);
            let left_overlay_end = buffer_byte_pos_for_char(&mgr, left_id, 4);
            let left_overlay = Value::make_overlay(OverlayData {
                serial: 0,
                plist: Value::NIL,
                buffer: Some(left_id),
                start: left_overlay_start,
                end: left_overlay_end,
                position_handle: None,
                front_advance: false,
                rear_advance: true,
            });
            mgr.get_mut(left_id)
                .expect("left buffer")
                .overlays
                .insert_overlay(left_overlay);

            let right_overlay_start = buffer_byte_pos_for_char(&mgr, right_id, 2);
            let right_overlay_end = buffer_byte_pos_for_char(&mgr, right_id, 6);
            let right_overlay = Value::make_overlay(OverlayData {
                serial: 0,
                plist: Value::NIL,
                buffer: Some(right_id),
                start: right_overlay_start,
                end: right_overlay_end,
                position_handle: None,
                front_advance: true,
                rear_advance: false,
            });
            mgr.get_mut(right_id)
                .expect("right buffer")
                .overlays
                .insert_overlay(right_overlay);

            let left_point = buffer_byte_pos_for_char(&mgr, left_id, 3);
            let left_mark = buffer_byte_pos_for_char(&mgr, left_id, 4);
            let left_narrow_start = buffer_byte_pos_for_char(&mgr, left_id, 1);
            let left_narrow_end = buffer_byte_pos_for_char(&mgr, left_id, 5);
            mgr.narrow_buffer_to_emacs_byte_range(
                left_id,
                EmacsByteRange::from_usize(left_narrow_start, left_narrow_end),
            )
            .expect("left narrow");
            mgr.goto_buffer_emacs_byte_pos(left_id, crate::buffer::EmacsBytePos::new(left_point))
                .expect("left point");
            mgr.set_buffer_mark_emacs_byte_pos(
                left_id,
                crate::buffer::EmacsBytePos::new(left_mark),
            )
            .expect("left mark");

            let right_point = buffer_byte_pos_for_char(&mgr, right_id, 5);
            let right_mark = buffer_byte_pos_for_char(&mgr, right_id, 1);
            let right_narrow_start = buffer_byte_pos_for_char(&mgr, right_id, 2);
            let right_narrow_end = buffer_byte_pos_for_char(&mgr, right_id, 7);
            mgr.narrow_buffer_to_emacs_byte_range(
                right_id,
                EmacsByteRange::from_usize(right_narrow_start, right_narrow_end),
            )
            .expect("right narrow");
            mgr.goto_buffer_emacs_byte_pos(right_id, crate::buffer::EmacsBytePos::new(right_point))
                .expect("right point");
            mgr.set_buffer_mark_emacs_byte_pos(
                right_id,
                crate::buffer::EmacsBytePos::new(right_mark),
            )
            .expect("right mark");

            mgr.get_mut(left_id)
                .expect("left buffer")
                .set_undo_list(Value::symbol("left-undo"));
            mgr.get_mut(right_id)
                .expect("right buffer")
                .set_undo_list(Value::symbol("right-undo"));

            let left_before = buffer_swap_payload_snapshot(mgr.get(left_id).expect("left buffer"));
            let right_before =
                buffer_swap_payload_snapshot(mgr.get(right_id).expect("right buffer"));

            mgr.swap_buffer_text(left_id, right_id)
                .expect("swap should succeed");

            let left_after = mgr.get(left_id).expect("left buffer");
            let right_after = mgr.get(right_id).expect("right buffer");
            assert_eq!(
                buffer_swap_payload_snapshot(left_after),
                right_before,
                "left buffer after swap should have right payload for {left_kind:?}->{right_kind:?}"
            );
            assert_eq!(
                buffer_swap_payload_snapshot(right_after),
                left_before,
                "right buffer after swap should have left payload for {left_kind:?}->{right_kind:?}"
            );

            assert_eq!(left_after.overlays.len(), 1);
            assert_eq!(right_after.overlays.len(), 1);
            assert_eq!(
                right_overlay.as_overlay_data().unwrap().buffer,
                Some(left_id)
            );
            assert_eq!(
                left_overlay.as_overlay_data().unwrap().buffer,
                Some(right_id)
            );
            assert_eq!(
                overlay_start_for_test(left_after, right_overlay),
                Some(right_overlay_start)
            );
            assert_eq!(
                overlay_start_for_test(right_after, left_overlay),
                Some(left_overlay_start)
            );

            assert_eq!(right_marker.as_marker_data().unwrap().buffer, Some(left_id));
            assert_eq!(left_marker.as_marker_data().unwrap().buffer, Some(right_id));
            assert_eq!(
                marker_chain_lookup_for_test(&left_after, 502).map(|(b, c, _)| (b, c)),
                Some((
                    right_marker_pos,
                    char_pos_for_byte(left_after, right_marker_pos)
                ))
            );
            assert_eq!(
                marker_chain_lookup_for_test(&right_after, 501).map(|(b, c, _)| (b, c)),
                Some((
                    left_marker_pos,
                    char_pos_for_byte(right_after, left_marker_pos)
                ))
            );
        }
    }
}

// -----------------------------------------------------------------------
// Point movement
// -----------------------------------------------------------------------

#[test]
fn goto_char_clamps_to_accessible_region() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
    assert_eq!(buf.point_emacs_byte_pos().get(), 3);

    // Past end — clamped to zv.
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(999));
    assert_eq!(
        buf.point_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get()
    );

    // Before start — clamped to begv.
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        2,
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    assert_eq!(buf.point_emacs_byte_pos().get(), 2);
}

#[test]
fn point_char_converts_byte_to_char_pos() {
    crate::test_utils::init_test_tracing();
    // "cafe\u{0301}" — 'e' + combining acute = 5 bytes, 5 chars in UTF-8
    let mut buf = buf_with_text("hello");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
    assert_eq!(buf.point_char_pos().get(), 3);
}

#[test]
fn gnu_style_buffer_fields_track_char_and_byte_positions() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("éz");
    assert_eq!(buf.point_min_char_pos().get(), 0);
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_max_char_pos().get(), 2);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 3);

    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new('é'.len_utf8()));
    assert_eq!(buf.point_char_pos().get(), 1);
    assert_eq!(buf.point_emacs_byte_pos().get(), 2);

    buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
    assert_eq!(
        buf.mark_emacs_byte_pos(),
        Some(crate::buffer::EmacsBytePos::new(3))
    );
    assert_eq!(buf.mark_emacs_byte_pos().map(|pos| pos.get()), Some(3));
    assert_eq!(buf.mark_char_pos().map(|pos| pos.get()), Some(2));
}

#[test]
fn byte_position_accessors_report_current_state() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 9));
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(7));
    buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));

    assert_eq!(
        buf.point_emacs_byte_pos(),
        crate::buffer::EmacsBytePos::new(7)
    );
    assert_eq!(
        buf.point_min_emacs_byte_pos(),
        crate::buffer::EmacsBytePos::new(2)
    );
    assert_eq!(
        buf.point_max_emacs_byte_pos(),
        crate::buffer::EmacsBytePos::new(9)
    );
    assert_eq!(
        buf.mark_emacs_byte_pos(),
        Some(crate::buffer::EmacsBytePos::new(4))
    );
}

#[test]
fn accessible_region_snapshot_restores_saved_bounds() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        let mut buf = buf_with_text_backend("aébc", kind);
        buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 4));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));

        let saved = buf.accessible_region_snapshot();
        buf.widen();
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.total_emacs_byte_len().get(),
        ));
        buf.insert("Z");
        buf.restore_accessible_region(saved);

        assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
        assert_eq!(buf.point_max_emacs_byte_pos().get(), 4);
        assert_eq!(buf.point_min_char_pos().get(), 1);
        assert_eq!(buf.point_max_char_pos().get(), 3);
        assert_eq!(buf.point_emacs_byte_pos().get(), 4);
    }
}

#[test]
fn accessible_region_snapshot_can_restore_end_to_current_full_buffer() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        let mut buf = buf_with_text_backend("aébc", kind);
        buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            1,
            buf.total_emacs_byte_len().get(),
        ));

        let saved = buf.accessible_region_snapshot();
        buf.widen();
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.total_emacs_byte_len().get(),
        ));
        buf.insert("Z");
        buf.restore_accessible_region_with_current_full_end(saved);

        assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
        assert_eq!(
            buf.point_max_emacs_byte_pos().get(),
            buf.total_emacs_byte_len().get()
        );
        assert_eq!(buf.point_min_char_pos().get(), 1);
        assert_eq!(buf.point_max_char_pos().get(), buf.total_char_len().get());
        assert_eq!(
            buf.point_emacs_byte_pos().get(),
            buf.total_emacs_byte_len().get()
        );
    }
}

#[test]
fn cached_char_positions_track_multibyte_edits_and_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("ééz");
    assert_eq!(buf.point_max_char_pos().get(), 3);

    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new('é'.len_utf8()));
    assert_eq!(buf.point_char_pos().get(), 1);

    buf.insert("ß");
    assert_eq!(buf.point_emacs_byte_pos().get(), 4);
    assert_eq!(buf.point_char_pos().get(), 2);
    assert_eq!(buf.point_max_char_pos().get(), 4);

    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        'é'.len_utf8(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    assert_eq!(buf.point_min_char_pos().get(), 1);
    assert_eq!(buf.point_max_char_pos().get(), 4);

    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 4));
    assert_eq!(buf.point_emacs_byte_pos().get(), 2);
    assert_eq!(buf.point_char_pos().get(), 1);
    assert_eq!(buf.point_max_char_pos().get(), 3);
    assert_eq!(buf.buffer_string(), "éz");
}

#[test]
fn char_position_conversions_clamp_to_buffer_and_accessible_bounds() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("ééz");
    assert_eq!(buf.total_char_len().get(), 3);
    assert_eq!(
        buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(99))
            .get(),
        "ééz".len()
    );
    assert_eq!(
        buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(99)).get(),
        "ééz".len()
    );

    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        'é'.len_utf8(),
        "ééz".len(),
    ));
    assert_eq!(buf.point_min_char_pos().get(), 1);
    assert_eq!(buf.point_max_char_pos().get(), 3);
    assert_eq!(
        buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(1))
            .get(),
        'é'.len_utf8()
    );
    assert_eq!(
        buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(99))
            .get(),
        "ééz".len()
    );
}

// -----------------------------------------------------------------------
// Insertion
// -----------------------------------------------------------------------

#[test]
fn insert_at_point_advances_point() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    // zv starts at 0 for an empty buffer; insert should extend it.
    buf.insert("hello");
    assert_eq!(buf.point_emacs_byte_pos().get(), 5);
    assert_eq!(buf.buffer_string(), "hello");
    assert_eq!(buf.accessible_char_len().get(), 5);
    assert!(buf.is_modified());
}

#[test]
fn insert_in_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("helo");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
    buf.insert("l");
    assert_eq!(buf.buffer_string(), "hello");
    assert_eq!(buf.point_emacs_byte_pos().get(), 4);
}

#[test]
fn insert_adjusts_mark() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("ab");
    buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    buf.insert("X");
    // Mark was at 1, insert at 0 pushes it to 2.
    assert_eq!(buf.mark_emacs_byte_pos().map(|pos| pos.get()), Some(2));
    assert_eq!(buf.mark_char_pos().map(|pos| pos.get()), Some(2));
}

#[test]
fn insert_empty_string_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(2));
    buf.insert("");
    assert_eq!(buf.buffer_string(), "hello");
    assert!(!buf.is_modified()); // still unmodified from initial state
}

// -----------------------------------------------------------------------
// Deletion
// -----------------------------------------------------------------------

#[test]
fn delete_region_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(11)); // at end
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(5, 11));
    assert_eq!(buf.buffer_string(), "hello");
    assert_eq!(buf.point_emacs_byte_pos().get(), 5); // was past deleted range
}

#[test]
fn delete_region_adjusts_point_inside() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("abcdef");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3)); // in middle of deleted range
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 5));
    assert_eq!(buf.point_emacs_byte_pos().get(), 1); // collapsed to start of deletion
    assert_eq!(buf.buffer_string(), "af");
}

#[test]
fn delete_region_adjusts_point_at_end_boundary() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("abcdef");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(5));
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 5));
    assert_eq!(buf.point_emacs_byte_pos().get(), 1);
    assert_eq!(buf.point_char_pos().get(), 1);
}

#[test]
fn delete_region_adjusts_mark() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("abcdef");
    buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 3));
    // mark was at 4, past deleted range end (3), so shifts by 2
    assert_eq!(buf.mark_emacs_byte_pos().map(|pos| pos.get()), Some(2));
    assert_eq!(buf.mark_char_pos().map(|pos| pos.get()), Some(2));
}

#[test]
fn delete_region_moves_marker_at_end_to_start() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("0123456789ABCDEF");
    register_marker_for_test(&mut buf, 1, 12, InsertionType::Before);
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(5, 12));
    let (byte_pos, char_pos, _ins) = marker_chain_lookup_for_test(&buf, 1).expect("marker");
    assert_eq!(byte_pos, 5);
    assert_eq!(char_pos, 5);
}

#[test]
fn mark_char_tracks_multibyte_edits() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("ééz");
    buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new('é'.len_utf8()));
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new('é'.len_utf8()));
    buf.insert("ß");
    assert_eq!(buf.mark_emacs_byte_pos().map(|pos| pos.get()), Some(2));
    assert_eq!(buf.mark_char_pos().map(|pos| pos.get()), Some(1));

    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(0, 2));
    assert_eq!(buf.mark_emacs_byte_pos().map(|pos| pos.get()), Some(0));
    assert_eq!(buf.mark_char_pos().map(|pos| pos.get()), Some(0));
}

#[test]
fn delete_region_adjusts_zv() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("abcdef");
    assert_eq!(buf.point_max_char_pos().get(), 6);
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 4));
    assert_eq!(buf.point_max_char_pos().get(), 4);
}

#[test]
fn delete_empty_range_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello");
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 2));
    assert_eq!(buf.buffer_string(), "hello");
}

// -----------------------------------------------------------------------
// Substring / buffer_string
// -----------------------------------------------------------------------

#[test]
fn buffer_substring_range() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("hello world");
    assert_eq!(
        buf.buffer_substring_range(EmacsByteRange::from_usize(6, 11)),
        "world"
    );
}

#[test]
fn buffer_string_returns_accessible() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(6, 11));
    assert_eq!(buf.buffer_string(), "world");
}

// -----------------------------------------------------------------------
// char_after / char_before
// -----------------------------------------------------------------------

#[test]
fn char_after_basic() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("hello");
    assert_eq!(
        buf.char_after_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0)),
        Some('h')
    );
    assert_eq!(
        buf.char_after_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4)),
        Some('o')
    );
    assert_eq!(
        buf.char_after_emacs_byte_pos(crate::buffer::EmacsBytePos::new(5)),
        None
    );
}

#[test]
fn char_before_basic() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("hello");
    assert_eq!(
        buf.char_before_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0)),
        None
    );
    assert_eq!(
        buf.char_before_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1)),
        Some('h')
    );
    assert_eq!(
        buf.char_before_emacs_byte_pos(crate::buffer::EmacsBytePos::new(5)),
        Some('o')
    );
}

#[test]
fn char_after_multibyte() {
    crate::test_utils::init_test_tracing();
    // Each Chinese character is 3 bytes in UTF-8.
    let buf = buf_with_text("\u{4f60}\u{597d}"); // "nihao" in Chinese
    assert_eq!(
        buf.char_after_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0)),
        Some('\u{4f60}')
    );
    assert_eq!(
        buf.char_after_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3)),
        Some('\u{597d}')
    );
}

#[test]
fn char_before_multibyte() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("\u{4f60}\u{597d}");
    assert_eq!(
        buf.char_before_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3)),
        Some('\u{4f60}')
    );
    assert_eq!(
        buf.char_before_emacs_byte_pos(crate::buffer::EmacsBytePos::new(6)),
        Some('\u{597d}')
    );
}

#[test]
fn char_width_helpers_use_typed_emacs_byte_positions() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        let buf = buf_with_text_backend("a\u{4f60}b", kind);

        assert_eq!(
            buf.char_after_emacs_byte_len(crate::buffer::EmacsBytePos::new(0))
                .map(crate::buffer::EmacsByteLen::get),
            Some(1)
        );
        assert_eq!(
            buf.char_after_emacs_byte_len(crate::buffer::EmacsBytePos::new(1))
                .map(crate::buffer::EmacsByteLen::get),
            Some('\u{4f60}'.len_utf8())
        );
        assert_eq!(
            buf.char_after_emacs_byte_len(crate::buffer::EmacsBytePos::new(4))
                .map(crate::buffer::EmacsByteLen::get),
            Some(1)
        );
        assert_eq!(
            buf.char_after_emacs_byte_len(crate::buffer::EmacsBytePos::new(5))
                .map(crate::buffer::EmacsByteLen::get),
            None
        );

        assert_eq!(
            buf.char_before_emacs_byte_len(crate::buffer::EmacsBytePos::new(1))
                .map(crate::buffer::EmacsByteLen::get),
            Some(1)
        );
        assert_eq!(
            buf.char_before_emacs_byte_len(crate::buffer::EmacsBytePos::new(4))
                .map(crate::buffer::EmacsByteLen::get),
            Some('\u{4f60}'.len_utf8())
        );
        assert_eq!(
            buf.char_before_emacs_byte_len(crate::buffer::EmacsBytePos::new(5))
                .map(crate::buffer::EmacsByteLen::get),
            Some(1)
        );
        assert_eq!(
            buf.char_before_emacs_byte_len(crate::buffer::EmacsBytePos::new(0))
                .map(crate::buffer::EmacsByteLen::get),
            None
        );
    }
}

// -----------------------------------------------------------------------
// Narrowing
// -----------------------------------------------------------------------

#[test]
fn narrow_and_widen() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(8));
    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(6, 11));
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 6);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 11);
    assert_eq!(buf.accessible_char_len().get(), 5);
    assert_eq!(buf.buffer_string(), "world");
    // Point was 8 — still within [6, 11].
    assert_eq!(buf.point_emacs_byte_pos().get(), 8);

    buf.widen();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 11);
}

#[test]
fn narrow_clamps_point() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(2));
    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(5, 11));
    // Point 2 < begv 5 => clamped to 5.
    assert_eq!(buf.point_emacs_byte_pos().get(), 5);
}

// -----------------------------------------------------------------------
// Markers
// -----------------------------------------------------------------------

#[test]
fn marker_tracks_insertion_after() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("ab");
    register_marker_for_test(&mut buf, 1, 1, InsertionType::After);
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    buf.insert("XY");
    // Marker was at 1 with After => advances to 3.
    let (byte_pos, char_pos, _ins) = marker_chain_lookup_for_test(&buf, 1).expect("marker");
    assert_eq!(byte_pos, 3);
    assert_eq!(char_pos, 3);
}

#[test]
fn marker_stays_on_insertion_before() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("ab");
    register_marker_for_test(&mut buf, 1, 1, InsertionType::Before);
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    buf.insert("XY");
    // Marker was at 1 with Before => stays at 1.
    let (byte_pos, char_pos, _ins) = marker_chain_lookup_for_test(&buf, 1).expect("marker");
    assert_eq!(byte_pos, 1);
    assert_eq!(char_pos, 1);
}

#[test]
fn marker_adjusts_on_deletion() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("abcdef");
    register_marker_for_test(&mut buf, 1, 4, InsertionType::After);
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 3));
    // Marker was at 4 (past deleted range [1,3)), shifts by 2 => 2.
    let (byte_pos, char_pos, _ins) = marker_chain_lookup_for_test(&buf, 1).expect("marker");
    assert_eq!(byte_pos, 2);
    assert_eq!(char_pos, 2);
}

/// Walk `list` and return its Nth (0-indexed) element, treating it as a
/// proper Lisp list.
fn undo_nth(list: Value, n: usize) -> Value {
    let mut cur = list;
    for _ in 0..n {
        assert!(cur.is_cons(), "undo list shorter than expected");
        cur = cur.cons_cdr();
    }
    assert!(cur.is_cons(), "undo list shorter than expected");
    cur.cons_car()
}

#[test]
fn delete_records_point_entry_before_marker_adjustment() {
    crate::test_utils::init_test_tracing();
    // Regression for GNU `record_delete` ordering (undo.c): the
    // point-position entry must be recorded *before* the marker-adjustment
    // entries, otherwise `primitive-undo` restores point to the wrong place.
    //
    // Repro: (insert "abcdef"); marker (After) at char 4; point at char 7;
    // undo-boundary; delete-region chars 3..5 (bytes [2,4)).  The marker is
    // inside the deleted region so it records an adjustment; GNU's resulting
    // list is `(("cd" . 3) (#<marker> . 1) 7 (t . 0))`.
    // Mirror the repro flow: the buffer is filled with undo enabled (so it is
    // already modified and the first-change `(t . 0)` is already recorded),
    // then an undo boundary is added, then the delete happens.
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_undo_list(Value::NIL);
    buf.insert("abcdef");
    // Marker at buffer position 4 (1-indexed) == Emacs byte pos 3.
    register_marker_for_test(&mut buf, 1, 3, InsertionType::After);
    // Point at buffer position 7 (1-indexed) == Emacs byte pos 6.
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(6));
    {
        let mut ul = buf.get_undo_list();
        crate::buffer::undo::undo_list_boundary(&mut ul);
        buf.set_undo_list(ul);
    }
    // GNU records `point_before_last_command_or_undo`; here point == 7
    // (0-indexed char 6).
    buf.saved_point_before_command
        .save(buf.id, crate::buffer::CharPos0::new(6));

    // delete-region chars 3..5 (1-indexed) == Emacs bytes [2, 4).
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 4));

    let ul = buf.get_undo_list();

    // 0: ("cd" . 3) — deletion entry.
    let delete_entry = undo_nth(ul, 0);
    assert!(delete_entry.is_cons());
    assert_eq!(delete_entry.cons_car().as_utf8_str(), Some("cd"));
    assert_eq!(delete_entry.cons_cdr(), Value::fixnum(3));

    // 1: (#<marker> . 1) — marker adjustment (cdr is the adjustment).
    let marker_entry = undo_nth(ul, 1);
    assert!(marker_entry.is_cons());
    assert!(marker_entry.cons_car().is_marker());
    assert_eq!(marker_entry.cons_cdr(), Value::fixnum(1));

    // 2: 7 — the point-position entry MUST be present and placed *after*
    // the marker adjustment in list order (i.e. recorded before it).  This
    // is the bug: without the fix the entry is dropped entirely.
    let point_entry = undo_nth(ul, 2);
    assert_eq!(
        point_entry,
        Value::fixnum(7),
        "point-position entry (7) must precede the marker adjustment"
    );
}

#[test]
fn delete_records_point_entry_without_marker_adjustment() {
    crate::test_utils::init_test_tracing();
    // The no-marker path must keep recording the point entry (no regression).
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_undo_list(Value::NIL);
    buf.insert("abcdef");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(6));
    {
        let mut ul = buf.get_undo_list();
        crate::buffer::undo::undo_list_boundary(&mut ul);
        buf.set_undo_list(ul);
    }
    buf.saved_point_before_command
        .save(buf.id, crate::buffer::CharPos0::new(6));

    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 4));

    let ul = buf.get_undo_list();
    // 0: ("cd" . 3) deletion, 1: 7 point entry (no marker entries here).
    let delete_entry = undo_nth(ul, 0);
    assert_eq!(delete_entry.cons_car().as_utf8_str(), Some("cd"));
    assert_eq!(undo_nth(ul, 1), Value::fixnum(7));
}

/// Every bare fixnum in `list` -- `primitive-undo`'s `((integerp next)
/// (goto-char next))` arm is the only thing that restores point across an
/// undo, so the presence or absence of such an entry is the whole question.
fn undo_point_entries(mut list: Value) -> Vec<i64> {
    let mut points = Vec::new();
    while list.is_cons() {
        if let ValueKind::Fixnum(n) = list.cons_car().kind() {
            points.push(n);
        }
        list = list.cons_cdr();
    }
    points
}

#[test]
fn a_command_in_another_buffer_supersedes_the_saved_point() {
    crate::test_utils::init_test_tracing();
    // GNU keeps the saved point in a pair of GLOBALS --
    // `point_before_last_command_or_undo` / `buffer_before_last_command_or_undo`
    // (src/keyboard.c:232-233), documented as "the location of point
    // immediately before THE LAST COMMAND was executed" (src/keyboard.h:257-266)
    // -- written together by every command-loop iteration
    // (src/keyboard.c:1536-1537) and by `undo-boundary` (src/undo.c:278-279).
    // A command that runs in ANOTHER buffer therefore supersedes the point
    // saved for the buffer that a later command edits, and `record_point`'s
    // third guard (src/undo.c:73-75) then drops the point entry.
    //
    // Every `M-x` is that shape: the minibuffer keystrokes are command-loop
    // iterations in the minibuffer, and the command they choose runs after
    // them.  Measured on GNU 31.0.90 with "abcdefghij", point at 1, and a
    // command whose body is `(save-excursion (goto-char 6) (delete-region 3 6))`:
    //
    //   (execute-kbd-macro (kbd "M-x probe-cmd RET"))
    //     buffer-undo-list => (("cde" . -3) (t . 0))    ; NO point entry
    //     C-_              => point 6
    //
    //   bound to a key instead, so no minibuffer read intervenes:
    //     buffer-undo-list => (("cde" . -3) 1 (t . 0))
    //     C-_              => point 1
    let mut mgr = BufferManager::new();
    let edited = mgr.current_buffer_id().expect("scratch buffer");
    {
        let buf = mgr.get_mut(edited).expect("scratch buffer");
        buf.set_undo_list(Value::NIL);
        buf.insert("abcdefghij");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
        let mut ul = buf.get_undo_list();
        crate::buffer::undo::undo_list_boundary(&mut ul);
        buf.set_undo_list(ul);
    }

    // The `M-x` command-loop iteration: the edited buffer is current, point 1.
    mgr.record_undo_point_before_command(edited)
        .expect("save point for the M-x iteration");

    // One minibuffer command-loop iteration.  In GNU this overwrites both
    // globals, so the point saved above is no longer usable by anything.
    let minibuffer = mgr.create_buffer(" *Minibuf-1*");
    mgr.record_undo_point_before_command(minibuffer)
        .expect("save point for the minibuffer iteration");

    // The chosen command runs back in the edited buffer: point moves to 6 and
    // chars 3..6 ("cde") are deleted, so `record_delete` stores the negative
    // position (PT == beg + SCHARS) and `primitive-undo` would restore point
    // past the reinserted text.
    mgr.goto_buffer_emacs_byte_pos(edited, crate::buffer::EmacsBytePos::new(5))
        .expect("goto end of the range about to be deleted");
    mgr.delete_buffer_emacs_byte_range(edited, crate::buffer::EmacsByteRange::from_usize(2, 5))
        .expect("delete region");

    let ul = mgr.get(edited).expect("scratch buffer").get_undo_list();
    let delete_entry = undo_nth(ul, 0);
    assert_eq!(delete_entry.cons_car().as_utf8_str(), Some("cde"));
    assert_eq!(delete_entry.cons_cdr(), Value::fixnum(-3));
    assert_eq!(
        undo_point_entries(ul),
        Vec::<i64>::new(),
        "the point saved for the M-x iteration was superseded by the \
         minibuffer's own iteration, so GNU records no point entry"
    );
}

#[test]
fn an_undo_boundary_in_an_undo_disabled_buffer_saves_no_point() {
    crate::test_utils::init_test_tracing();
    // `Fundo_boundary` returns BEFORE it touches the globals when the buffer
    // has undo turned off (`if (EQ (BVAR (current_buffer, undo_list), Qt))
    // return Qnil;`, src/undo.c:258-259; the assignment is at :278-279).  A
    // buffer that records nothing must not be able to spend another buffer's
    // saved point, and `lisp/` calls `(undo-boundary)` unconditionally in three
    // dozen places, each in whatever buffer happens to be current.
    //
    // Measured on GNU 31.0.90: buffer A has undo off; B saves a point of 1 via
    // `undo-boundary`; `undo-boundary` runs in A; then B deletes chars 3..5.
    //   GNU                => (("rl" . 3) 1 (t . 0) nil (1 . 6) (t . 0))
    //   Neomacs before fix => (("rl" . 3)   (t . 0) nil (1 . 6) (t . 0))
    let mut mgr = BufferManager::new();
    let recording = mgr.current_buffer_id().expect("scratch buffer");
    let disabled = mgr.create_buffer("undo-off");
    {
        let buf = mgr.get_mut(disabled).expect("undo-off buffer");
        buf.set_undo_list(Value::T);
        buf.insert("hello");
    }
    {
        let buf = mgr.get_mut(recording).expect("scratch buffer");
        buf.set_undo_list(Value::NIL);
        buf.insert("world");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    // The saved point that must survive: point 1 in the recording buffer.
    mgr.add_undo_boundary(recording)
        .expect("boundary in the recording buffer");
    // GNU's early return: this must not touch the editor-global pair.
    mgr.add_undo_boundary(disabled)
        .expect("boundary in the undo-disabled buffer");

    mgr.goto_buffer_emacs_byte_pos(recording, crate::buffer::EmacsBytePos::new(2))
        .expect("goto");
    mgr.delete_buffer_emacs_byte_range(recording, crate::buffer::EmacsByteRange::from_usize(2, 4))
        .expect("delete region");

    let ul = mgr.get(recording).expect("scratch buffer").get_undo_list();
    let delete_entry = undo_nth(ul, 0);
    assert_eq!(delete_entry.cons_car().as_utf8_str(), Some("rl"));
    assert_eq!(
        undo_point_entries(ul),
        vec![1],
        "a boundary in an undo-disabled buffer must leave the saved point alone"
    );
    // The disabled buffer still records nothing at all.
    assert!(
        mgr.get(disabled)
            .expect("undo-off buffer")
            .get_undo_list()
            .is_t(),
        "undo-boundary must not turn recording back on"
    );
}

#[test]
fn every_buffer_of_one_editor_shares_the_saved_point_cell() {
    crate::test_utils::init_test_tracing();
    // The superseding behaviour above is only true because there is exactly
    // ONE saved-point cell per editor, matching GNU's single pair of globals
    // (src/keyboard.c:232-233).  A buffer that reached the manager with a cell
    // of its own would silently go back to recording stale point entries, so
    // pin the sharing for every way a buffer comes into existence.
    let mut mgr = BufferManager::new();
    let scratch = mgr.current_buffer_id().expect("scratch buffer");
    let plain = mgr.create_buffer("plain");
    let indirect = mgr
        .create_indirect_buffer(plain, "*indirect*", false)
        .expect("indirect buffer");
    let cloned = mgr
        .create_indirect_buffer(plain, "*clone*", true)
        .expect("cloned indirect buffer");

    let expected = mgr
        .get(scratch)
        .expect("scratch buffer")
        .saved_point_before_command
        .clone();
    for id in [scratch, plain, indirect, cloned] {
        assert!(
            mgr.get(id)
                .expect("live buffer")
                .saved_point_before_command
                .shares_cell_with(&expected),
            "buffer {id:?} must share the editor's one saved-point cell"
        );
    }
}

#[test]
fn marker_inside_deleted_range_collapses() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("abcdef");
    register_marker_for_test(&mut buf, 1, 2, InsertionType::After);
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 5));
    // Marker at 2 inside [1,5) => collapses to 1.
    let (byte_pos, char_pos, _ins) = marker_chain_lookup_for_test(&buf, 1).expect("marker");
    assert_eq!(byte_pos, 1);
    assert_eq!(char_pos, 1);
}

#[test]
fn marker_char_pos_tracks_multibyte_edits() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("ééz");
    register_marker_for_test(&mut buf, 1, 'é'.len_utf8(), InsertionType::After);
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new('é'.len_utf8()));
    buf.insert("ß");
    let (byte_pos, char_pos, _ins) = marker_chain_lookup_for_test(&buf, 1).expect("marker");
    assert_eq!(byte_pos, 4);
    assert_eq!(char_pos, 2);

    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 4));
    let (byte_pos, char_pos, _ins) = marker_chain_lookup_for_test(&buf, 1).expect("marker");
    assert_eq!(byte_pos, 2);
    assert_eq!(char_pos, 1);
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

fn run_backend_edit_script(kind: BufferTextBackendKind) -> BackendEditSnapshot {
    let mut buf = buf_with_text_backend("éaßbc", kind);
    assert_eq!(buf.text_backend_kind(), kind);

    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    assert!(buf.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(2, 6),
        face,
        bold
    ));
    register_marker_for_test(&mut buf, 42, 3, InsertionType::After);
    buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(2));

    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
    buf.insert("日本");

    let delete_start = byte_pos_for_char(&buf, 2);
    let delete_end = byte_pos_for_char(&buf, 4);
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        delete_start,
        delete_end,
    ));

    let replace_start = byte_pos_for_char(&buf, 1);
    let replace_end = byte_pos_for_char(&buf, 3);
    let replacement = LispString::from_utf8("Ωx");
    buf.replace_emacs_byte_range_lisp_string(
        crate::buffer::EmacsByteRange::from_usize(replace_start, replace_end),
        &replacement,
    );

    let marker_position = marker_chain_anchor_for_test(&buf, 42);

    BackendEditSnapshot {
        buffer_string: buf.buffer_string(),
        point_byte: buf.point_emacs_byte_pos().get(),
        point_char: buf.point_char_pos().get(),
        mark_byte: buf.mark_emacs_byte_pos().map(|pos| pos.get()),
        mark_char: buf.mark_char_pos().map(|pos| pos.get()),
        marker_position,
        text_properties: buffer_text_property_snapshot(&buf),
    }
}

fn run_backend_transpose_script(kind: BufferTextBackendKind) -> BackendEditSnapshot {
    let mut buf = buf_with_text_backend("αβ--日本--Ωx", kind);
    assert_eq!(buf.text_backend_kind(), kind);

    let face = Value::symbol("face");
    let first_face = Value::symbol("first");
    let second_face = Value::symbol("second");
    assert!(buf.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(
            byte_pos_for_char(&buf, 0),
            byte_pos_for_char(&buf, 2),
        ),
        face,
        first_face,
    ));
    assert!(buf.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(
            byte_pos_for_char(&buf, 4),
            byte_pos_for_char(&buf, 6),
        ),
        face,
        second_face,
    ));
    let marker_byte = byte_pos_for_char(&buf, 1);
    register_marker_for_test(&mut buf, 42, marker_byte, InsertionType::Before);
    buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(byte_pos_for_char(&buf, 5)));
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(byte_pos_for_char(&buf, 8)));

    let transposition = buf.text_transposition_for_char_ranges(
        CharRange::from_usize(0, 2),
        CharRange::from_usize(4, 6),
    );
    buf.transpose_regions(transposition, false);

    let marker_position = marker_chain_anchor_for_test(&buf, 42);

    BackendEditSnapshot {
        buffer_string: buf.buffer_string(),
        point_byte: buf.point_emacs_byte_pos().get(),
        point_char: buf.point_char_pos().get(),
        mark_byte: buf.mark_emacs_byte_pos().map(|pos| pos.get()),
        mark_char: buf.mark_char_pos().map(|pos| pos.get()),
        marker_position,
        text_properties: buffer_text_property_snapshot(&buf),
    }
}

#[test]
fn implemented_text_backends_match_edit_marker_and_property_side_effects() {
    crate::test_utils::init_test_tracing();
    let baseline = run_backend_edit_script(BufferTextBackendKind::GapBuffer);
    for kind in implemented_text_backends() {
        assert_eq!(
            run_backend_edit_script(kind),
            baseline,
            "{kind:?} edit side effects diverged from gap buffer"
        );
    }
}

#[test]
fn implemented_text_backends_match_transpose_side_effects() {
    crate::test_utils::init_test_tracing();
    let baseline = run_backend_transpose_script(BufferTextBackendKind::GapBuffer);
    for kind in implemented_text_backends() {
        assert_eq!(
            run_backend_transpose_script(kind),
            baseline,
            "{kind:?} transpose side effects diverged from gap buffer"
        );
    }
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
        marker_id: Option<u64>,
        adjustment: i64,
    },
    Other(String),
    ImproperTail(String),
}

fn undo_value_label(value: Value) -> String {
    if value.is_nil() {
        return "nil".to_owned();
    }
    if value.is_t() {
        return "t".to_owned();
    }
    if let Some(name) = value.as_symbol_name() {
        return format!("symbol:{name}");
    }
    if let Some(n) = value.as_fixnum() {
        return format!("fixnum:{n}");
    }
    if value.is_string() {
        return format!(
            "string:{}",
            value
                .as_runtime_string_owned()
                .expect("string value should have string payload")
        );
    }
    if value.is_marker() {
        return format!(
            "marker:{:?}",
            value
                .as_marker_data()
                .expect("marker value should have marker payload")
                .marker_id
        );
    }
    format!("{:?}", value.kind())
}

fn undo_entry_snapshot(entry: Value) -> UndoEntrySnapshot {
    match entry.kind() {
        ValueKind::Nil => UndoEntrySnapshot::Boundary,
        ValueKind::Fixnum(point) => UndoEntrySnapshot::Point(point),
        ValueKind::Cons => {
            let car = entry.cons_car();
            let cdr = entry.cons_cdr();
            match (car.kind(), cdr.kind()) {
                (ValueKind::Fixnum(beg1), ValueKind::Fixnum(end1)) => {
                    UndoEntrySnapshot::Insert { beg1, end1 }
                }
                (ValueKind::String, ValueKind::Fixnum(pos1)) => UndoEntrySnapshot::Delete {
                    text: car
                        .as_runtime_string_owned()
                        .expect("delete undo text should be a string"),
                    pos1,
                },
                (ValueKind::T, ValueKind::Fixnum(modtime)) => {
                    UndoEntrySnapshot::FirstChange(modtime)
                }
                (_, ValueKind::Fixnum(adjustment)) if car.is_marker() => {
                    UndoEntrySnapshot::MarkerAdjustment {
                        marker_id: car
                            .as_marker_data()
                            .expect("marker value should have marker payload")
                            .marker_id,
                        adjustment,
                    }
                }
                (ValueKind::Nil, _) if cdr.is_cons() => {
                    let prop = cdr.cons_car();
                    let rest1 = cdr.cons_cdr();
                    if rest1.is_cons() {
                        let old_value = rest1.cons_car();
                        let rest2 = rest1.cons_cdr();
                        if rest2.is_cons()
                            && let (Some(beg1), Some(end1)) =
                                (rest2.cons_car().as_fixnum(), rest2.cons_cdr().as_fixnum())
                        {
                            return UndoEntrySnapshot::PropertyChange {
                                prop: undo_value_label(prop),
                                old_value: undo_value_label(old_value),
                                beg1,
                                end1,
                            };
                        }
                    }
                    UndoEntrySnapshot::Other("malformed-property-change".to_owned())
                }
                _ => UndoEntrySnapshot::Other(format!(
                    "cons:{}:{}",
                    undo_value_label(car),
                    undo_value_label(cdr)
                )),
            }
        }
        _ => UndoEntrySnapshot::Other(undo_value_label(entry)),
    }
}

fn undo_list_snapshot(mut list: Value) -> Vec<UndoEntrySnapshot> {
    let mut entries = Vec::new();
    while list.is_cons() {
        entries.push(undo_entry_snapshot(list.cons_car()));
        list = list.cons_cdr();
    }
    if !list.is_nil() {
        entries.push(UndoEntrySnapshot::ImproperTail(undo_value_label(list)));
    }
    entries
}

/// The three text backends must record AND replay undo identically.
///
/// This used to run the edit script against `BufferManager` and replay it
/// with `BufferManager::undo_buffer'.  That loop is deleted
/// (DIVERGENCES.md 150): replay is `primitive-undo' (lisp/simple.el:3645),
/// which is Lisp, so the script now runs in the runtime with the backend
/// chosen through `neomacs-set-default-buffer-text-backend'.
///
/// The move makes the test stronger rather than weaker.  Before, the three
/// backends were compared only with each other, so all three could have been
/// wrong together.  Now the gap-buffer answer is pinned to GNU Emacs 31.0.90
/// `-Q --batch' (tmp/pw56-moved-tests-gnu.txt) and the other two are required
/// to equal it -- multibyte text, a marker, the mark, point, a text property
/// laid down across the edited region, an insert, a delete and a replace.
#[test]
fn implemented_text_backends_match_undo_recording_and_execution() {
    crate::test_utils::init_test_tracing();

    const UNDO_SCRIPT: &str = r#"
(progn
  (neomacs-set-default-buffer-text-backend '{backend})
  (with-temp-buffer
    (insert "aébc日本z")
    (set-mark 3)
    (deactivate-mark)
    (put-text-property 2 6 'face 'bold)
    (let ((mk (copy-marker 5)))
      (buffer-enable-undo)
      (setq buffer-undo-list nil)
      (goto-char 3)
      (insert "Ω")
      (put-text-property 2 7 'face 'italic)
      (delete-region 5 7)
      (goto-char 2)
      (delete-region 2 4)
      (insert "qλ")
      (undo-boundary)
      (setq last-command nil)
      (undo)
      (list (neomacs-buffer-text-backend)
            (buffer-substring-no-properties (point-min) (point-max))
            (point)
            (marker-position (mark-marker))
            (marker-position mk)
            (mapcar (lambda (i) (get-text-property i 'face))
                    (number-sequence 1 (1- (point-max))))))))
"#;

    for kind in implemented_text_backends() {
        let backend = kind.symbol_name();
        let result =
            crate::test_utils::runtime_startup_eval_one(&UNDO_SCRIPT.replace("{backend}", backend));
        assert_eq!(
            result,
            format!("OK ({backend} \"aébc日本z\" 3 3 5 (nil bold bold bold bold nil nil))"),
            "{kind:?} undo semantics diverged from GNU",
        );
    }
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

fn run_manager_edit_entrypoint_script(
    kind: BufferTextBackendKind,
    measured_entrypoints: bool,
) -> ManagerEditEntrypointSnapshot {
    let mut mgr = manager_with_text_backend(kind);
    let id = mgr.current_buffer_id().expect("scratch buffer");
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");

    {
        let buf = mgr.get_mut(id).expect("scratch buffer");
        buf.insert("aébc日本z");
        buf.widen();
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
        buf.set_mark_emacs_byte_pos(crate::buffer::EmacsBytePos::new(byte_pos_for_char(buf, 5)));
        let prop_start = byte_pos_for_char(buf, 1);
        let prop_end = byte_pos_for_char(buf, 6);
        let marker_pos = byte_pos_for_char(buf, 4);
        assert!(buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(prop_start, prop_end),
            face,
            bold
        ));
        register_marker_for_test(buf, 99, marker_pos, InsertionType::After);
        buf.set_undo_list(Value::NIL);
    }

    let delete_start = buffer_byte_pos_for_char(&mgr, id, 2);
    let delete_end = buffer_byte_pos_for_char(&mgr, id, 4);
    if measured_entrypoints {
        let delete_range = mgr
            .edit_range_for_buffer_emacs_byte_range(
                id,
                EmacsByteRange::from_usize(delete_start, delete_end),
            )
            .expect("measure delete range");
        mgr.delete_buffer_measured_region(id, delete_range)
            .expect("delete measured range");
    } else {
        mgr.delete_buffer_emacs_byte_range(
            id,
            crate::buffer::EmacsByteRange::from_usize(delete_start, delete_end),
        )
        .expect("delete raw range");
    }

    let replace_start = buffer_byte_pos_for_char(&mgr, id, 1);
    let replace_end = buffer_byte_pos_for_char(&mgr, id, 3);
    let replacement = LispString::from_utf8("λq");
    if measured_entrypoints {
        let replace_range = mgr
            .edit_range_for_buffer_emacs_byte_range(
                id,
                EmacsByteRange::from_usize(replace_start, replace_end),
            )
            .expect("measure replace range");
        mgr.replace_buffer_measured_region_lisp_string(id, replace_range, &replacement)
            .expect("replace measured range");
    } else {
        mgr.replace_buffer_region_lisp_string(id, replace_start, replace_end, &replacement)
            .expect("replace raw range");
    }

    let buf = mgr.get(id).expect("scratch buffer");
    ManagerEditEntrypointSnapshot {
        buffer_string: buf.buffer_string(),
        point_byte: buf.point_emacs_byte_pos().get(),
        point_char: buf.point_char_pos().get(),
        mark_byte: buf.mark_emacs_byte_pos().map(|pos| pos.get()),
        mark_char: buf.mark_char_pos().map(|pos| pos.get()),
        marker_position: marker_chain_anchor_for_test(&buf, 99),
        text_properties: buffer_text_property_snapshot(buf),
        undo: undo_list_snapshot(buf.get_undo_list()),
    }
}

#[test]
fn measured_manager_edit_entrypoints_match_typed_range_entrypoints() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        assert_eq!(
            run_manager_edit_entrypoint_script(kind, true),
            run_manager_edit_entrypoint_script(kind, false),
            "{kind:?} measured edit entrypoints diverged from typed range entrypoints"
        );
    }
}

#[test]
fn lisp_string_insert_into_unibyte_buffer_preserves_gnu_chars_and_properties() {
    for kind in implemented_text_backends() {
        let mut buf = buf_with_text_backend("", kind);
        buf.set_multibyte_value(false);

        let face = Value::symbol("face");
        let bold = Value::symbol("bold");
        let mut text = LispString::from_utf8("é日本");
        assert!(text.intervals_mut().put_property_in_char_range(
            CharRange::from_usize(0, 3),
            face,
            bold
        ));

        buf.insert_lisp_string(&text);

        assert_eq!(buf.total_char_len().get(), 3, "backend {kind:?}");
        assert_eq!(buf.total_emacs_byte_len().get(), 3, "backend {kind:?}");
        assert_eq!(
            buf.buffer_substring_bytes_range(EmacsByteRange::from_usize(0, 3)),
            vec![0xE9, 0xE5, 0x2C],
            "backend {kind:?}"
        );
        assert_eq!(
            buffer_text_property_snapshot(&buf),
            vec![(0, 3, vec![(face, bold)])],
            "backend {kind:?}"
        );
    }
}

#[test]
fn manager_replace_lisp_string_grafts_converted_intervals_once() {
    for kind in implemented_text_backends() {
        let mut mgr = manager_with_text_backend(kind);
        let id = mgr.current_buffer_id().expect("scratch buffer");
        {
            let buf = mgr.get_mut(id).expect("scratch buffer");
            buf.set_multibyte_value(false);
            buf.insert("AB");
            buf.widen();
            buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
        }

        let face = Value::symbol("face");
        let bold = Value::symbol("bold");
        let mut replacement = LispString::from_utf8("é日本");
        assert!(replacement.intervals_mut().put_property_in_char_range(
            CharRange::from_usize(0, 3),
            face,
            bold
        ));

        let range = mgr
            .edit_range_for_buffer_emacs_byte_range(id, EmacsByteRange::from_usize(0, 1))
            .expect("replace range");
        mgr.replace_buffer_measured_region_lisp_string(id, range, &replacement)
            .expect("replace text");

        let buf = mgr.get(id).expect("scratch buffer");
        assert_eq!(buf.total_char_len().get(), 4, "backend {kind:?}");
        assert_eq!(
            buf.buffer_substring_bytes_range(EmacsByteRange::from_usize(0, 4)),
            vec![0xE9, 0xE5, 0x2C, b'B'],
            "backend {kind:?}"
        );
        assert_eq!(
            buffer_text_property_snapshot(buf),
            vec![(0, 3, vec![(face, bold)])],
            "backend {kind:?}"
        );
    }
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

fn run_shared_insert_policy_script(kind: BufferTextBackendKind) -> SharedInsertPolicySnapshot {
    let mut mgr = manager_with_text_backend(kind);
    let base_id = mgr.current_buffer_id().expect("scratch buffer");
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let insert = LispString::from_utf8("Ω");
    let before_markers_insert = LispString::from_utf8("λ");
    let replacement = LispString::from_utf8("qr");

    mgr.insert_into_buffer(base_id, "aébc日本z")
        .expect("initial insert");
    let indirect_id = mgr
        .create_indirect_buffer(base_id, "*shared-insert-policy*", false)
        .expect("indirect buffer");

    let prop_start = buffer_byte_pos_for_char(&mgr, base_id, 1);
    let prop_end = buffer_byte_pos_for_char(&mgr, base_id, 6);
    mgr.put_buffer_text_property_in_emacs_byte_range(
        base_id,
        EmacsByteRange::from_usize(prop_start, prop_end),
        face,
        bold,
    )
    .expect("text property");

    let insert_pos = buffer_byte_pos_for_char(&mgr, base_id, 2);
    register_marker_for_test(
        mgr.get_mut(base_id).expect("base buffer"),
        701,
        insert_pos,
        InsertionType::After,
    );
    register_marker_for_test(
        mgr.get_mut(indirect_id).expect("indirect buffer"),
        702,
        insert_pos,
        InsertionType::Before,
    );

    let overlay_end = buffer_byte_pos_for_char(&mgr, indirect_id, 5);
    let indirect_overlay = Value::make_overlay(OverlayData {
        serial: 0,
        plist: Value::NIL,
        buffer: Some(indirect_id),
        start: insert_pos,
        end: overlay_end,
        position_handle: None,
        front_advance: false,
        rear_advance: true,
    });
    mgr.get_mut(indirect_id)
        .expect("indirect buffer")
        .overlays
        .insert_overlay(indirect_overlay);

    mgr.set_buffer_mark_emacs_byte_pos(
        base_id,
        crate::buffer::EmacsBytePos::new(buffer_byte_pos_for_char(&mgr, base_id, 5)),
    )
    .expect("base mark");
    mgr.set_buffer_mark_emacs_byte_pos(
        indirect_id,
        crate::buffer::EmacsBytePos::new(buffer_byte_pos_for_char(&mgr, indirect_id, 4)),
    )
    .expect("indirect mark");
    mgr.goto_buffer_emacs_byte_pos(
        indirect_id,
        crate::buffer::EmacsBytePos::new(buffer_byte_pos_for_char(&mgr, indirect_id, 3)),
    )
    .expect("indirect point");

    mgr.goto_buffer_emacs_byte_pos(base_id, crate::buffer::EmacsBytePos::new(insert_pos))
        .expect("base insert point");
    mgr.insert_lisp_string_into_buffer(base_id, &insert)
        .expect("normal insert");

    let before_markers_pos = buffer_byte_pos_for_char(&mgr, base_id, 3);
    mgr.goto_buffer_emacs_byte_pos(
        base_id,
        crate::buffer::EmacsBytePos::new(before_markers_pos),
    )
    .expect("before-markers insert point");
    mgr.insert_lisp_string_into_buffer_before_markers(base_id, &before_markers_insert)
        .expect("before-markers insert");

    let replace_start = buffer_byte_pos_for_char(&mgr, indirect_id, 1);
    let replace_end = buffer_byte_pos_for_char(&mgr, indirect_id, 3);
    mgr.replace_buffer_region_lisp_string(indirect_id, replace_start, replace_end, &replacement)
        .expect("replace through indirect buffer");

    let base = mgr.get(base_id).expect("base buffer");
    let indirect = mgr.get(indirect_id).expect("indirect buffer");
    assert_eq!(base.text_backend_kind(), kind);
    assert_eq!(indirect.text_backend_kind(), kind);
    SharedInsertPolicySnapshot {
        base_string: base.buffer_string(),
        indirect_string: indirect.buffer_string(),
        base_point: TextPositionAnchor::new(base.point_char_pos(), base.point_emacs_byte_pos()),
        indirect_point: TextPositionAnchor::new(
            indirect.point_char_pos(),
            indirect.point_emacs_byte_pos(),
        ),
        base_mark: OptionalTextPositionSnapshot::new(
            base.mark_char_pos(),
            base.mark_emacs_byte_pos(),
        ),
        indirect_mark: OptionalTextPositionSnapshot::new(
            indirect.mark_char_pos(),
            indirect.mark_emacs_byte_pos(),
        ),
        marker_after_position: marker_chain_anchor_for_test(base, 701),
        marker_before_position: marker_chain_anchor_for_test(indirect, 702),
        indirect_overlay_range: OptionalEmacsByteRangeSnapshot::from_usize(
            overlay_start_for_test(indirect, indirect_overlay),
            overlay_end_for_test(indirect, indirect_overlay),
        ),
        text_properties: buffer_text_property_snapshot(base),
    }
}

#[test]
fn implemented_text_backends_match_shared_insert_policy_side_effects() {
    crate::test_utils::init_test_tracing();
    let baseline = run_shared_insert_policy_script(BufferTextBackendKind::GapBuffer);
    for kind in implemented_text_backends() {
        assert_eq!(
            run_shared_insert_policy_script(kind),
            baseline,
            "{kind:?} shared insert side effects diverged from gap buffer"
        );
    }
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

fn run_backend_migration_script(
    initial_kind: BufferTextBackendKind,
    target_kind: BufferTextBackendKind,
    convert: bool,
) -> BackendMigrationSnapshot {
    let mut buf = buf_with_text_backend("aébc日本z", initial_kind);

    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let prop_start = byte_pos_for_char(&buf, 1);
    let prop_end = byte_pos_for_char(&buf, 5);
    assert!(buf.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(prop_start, prop_end),
        face,
        bold
    ));
    register_marker_for_test(&mut buf, 77, 4, InsertionType::After);
    let overlay = Value::make_overlay(OverlayData {
        serial: 0,
        plist: Value::NIL,
        buffer: Some(buf.id),
        start: 3,
        end: 8,
        position_handle: None,
        front_advance: false,
        rear_advance: true,
    });
    buf.overlays.insert_overlay(overlay);

    let cached_byte = byte_pos_for_char(&buf, 4);
    assert_eq!(char_pos_for_byte(&buf, cached_byte), 4);

    if convert {
        buf.convert_text_backend_kind(require_implemented_kind(target_kind));
    }
    assert_eq!(buf.text_backend_kind(), target_kind);
    assert_eq!(byte_pos_for_char(&buf, 4), cached_byte);
    assert_eq!(char_pos_for_byte(&buf, cached_byte), 4);

    let insert_pos = byte_pos_for_char(&buf, 2);
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(insert_pos));
    buf.insert("Ω");

    let delete_start = byte_pos_for_char(&buf, 5);
    let delete_end = byte_pos_for_char(&buf, 6);
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        delete_start,
        delete_end,
    ));

    let char_to_byte_4 = byte_pos_for_char(&buf, 4);
    BackendMigrationSnapshot {
        backend_kind: buf.text_backend_kind(),
        buffer_string: buf.buffer_string(),
        char_to_byte_4,
        byte_to_char_at_char_4: char_pos_for_byte(&buf, char_to_byte_4),
        marker_position: marker_chain_anchor_for_test(&buf, 77),
        overlay_range: OptionalEmacsByteRangeSnapshot::from_usize(
            overlay_start_for_test(&buf, overlay),
            overlay_end_for_test(&buf, overlay),
        ),
        text_properties: buffer_text_property_snapshot(&buf),
    }
}

#[test]
fn buffer_text_backend_migration_preserves_side_data_and_post_edit_semantics() {
    crate::test_utils::init_test_tracing();
    for target in implemented_text_backends() {
        let baseline = run_backend_migration_script(target, target, false);
        for source in implemented_text_backends() {
            assert_eq!(
                run_backend_migration_script(source, target, true),
                baseline,
                "{source:?} -> {target:?} backend migration diverged from direct target backend"
            );
        }
    }
}

// -----------------------------------------------------------------------
// Buffer-local variables
// -----------------------------------------------------------------------

#[test]
fn buffer_local_get_set() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert!(buf.get_buffer_local("tab-width").is_none());

    buf.set_buffer_local("tab-width", Value::fixnum(4));
    let val = buf.get_buffer_local("tab-width").unwrap();
    assert!(val.is_fixnum());

    buf.set_buffer_local("tab-width", Value::fixnum(8));
    let val = buf.get_buffer_local("tab-width").unwrap();
    assert!(val.is_fixnum());
}

#[test]
fn buffer_local_multiple_vars() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_buffer_local("fill-column", Value::fixnum(80));
    buf.set_buffer_local("major-mode", Value::symbol("text-mode"));

    assert!(buf.get_buffer_local("fill-column").is_some());
    assert!(buf.get_buffer_local("major-mode").is_some());
    assert!(buf.get_buffer_local("nonexistent").is_none());
}

#[test]
fn buffer_local_gated_skips_alist_scan_but_keeps_slot_and_undo() {
    use crate::emacs_core::intern::intern;
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );

    // A non-slot alist entry.
    let my_local = intern("neo-gate-test-local");
    buf.set_buffer_local_by_sym_id(my_local, Value::fixnum(42));
    // A slot-backed name.
    let tab_width = intern("tab-width");
    buf.set_buffer_local_by_sym_id(tab_width, Value::fixnum(4));

    // localized=true -> the alist entry is found (unchanged behavior).
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(my_local, true),
        Some(Value::fixnum(42))
    );
    // localized=false -> the alist walk is skipped. Callers only pass false for
    // symbols the obarray reports as non-Localized, which by invariant are never
    // in the alist; this pins the gate's contract.
    assert_eq!(buf.get_buffer_local_by_sym_id_gated(my_local, false), None);

    // Slot-backed names resolve regardless of `localized` (the slot check runs
    // before the gate) -- a global (non-localized) slot var must not be dropped.
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(tab_width, false),
        Some(Value::fixnum(4))
    );
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(tab_width, true),
        Some(Value::fixnum(4))
    );

    // A symbol absent from the alist: None either way (no scan when false).
    let absent = intern("neo-gate-test-absent");
    assert_eq!(buf.get_buffer_local_by_sym_id_gated(absent, false), None);
    assert_eq!(buf.get_buffer_local_by_sym_id_gated(absent, true), None);
}

#[test]
fn repeated_buffer_local_lookup_indexes_the_lisp_alist_once() {
    use crate::emacs_core::intern::intern;
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    let deepest = intern("neo-index-test-deepest");
    let middle = intern("neo-index-test-middle");
    let head = intern("neo-index-test-head");
    buf.set_buffer_local_by_sym_id(deepest, Value::fixnum(1));
    buf.set_buffer_local_by_sym_id(middle, Value::fixnum(2));
    buf.set_buffer_local_by_sym_id(head, Value::fixnum(3));

    reset_local_var_alist_entry_probes();
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(deepest, true),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(deepest, true),
        Some(Value::fixnum(1))
    );

    assert_eq!(
        local_var_alist_entry_probes(),
        3,
        "the first lookup should build one identity index and the second should reuse it"
    );
}

#[test]
fn buffer_local_lookup_index_stays_coherent_across_value_and_structure_changes() {
    use crate::emacs_core::intern::intern;
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    let first = intern("neo-index-coherence-first");
    let second = intern("neo-index-coherence-second");
    let third = intern("neo-index-coherence-third");
    buf.set_buffer_local_by_sym_id(first, Value::fixnum(1));
    buf.set_buffer_local_by_sym_id(second, Value::fixnum(2));

    reset_local_var_alist_entry_probes();
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(first, true),
        Some(Value::fixnum(1))
    );
    assert_eq!(local_var_alist_entry_probes(), 2);

    // Existing values are changed in place. The index stores the binding cons,
    // so its cdr remains the live source of truth without a rebuild.
    buf.set_buffer_local_by_sym_id(first, Value::fixnum(11));
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(first, true),
        Some(Value::fixnum(11))
    );
    assert_eq!(local_var_alist_entry_probes(), 2);

    // Prepending and removing bindings are structural changes and must
    // invalidate the index before the next lookup.
    buf.set_buffer_local_by_sym_id(third, Value::fixnum(3));
    assert_eq!(
        buf.get_buffer_local_by_sym_id_gated(third, true),
        Some(Value::fixnum(3))
    );
    assert_eq!(local_var_alist_entry_probes(), 5);

    assert_eq!(
        buf.kill_buffer_local_by_sym_id(second),
        Some(RuntimeBindingValue::Bound(Value::fixnum(2)))
    );
    assert_eq!(buf.get_buffer_local_by_sym_id_gated(second, true), None);
    assert_eq!(local_var_alist_entry_probes(), 7);
}

#[test]
fn buffer_local_defaults_include_builtin_per_buffer_vars() {
    crate::test_utils::init_test_tracing();
    let buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );

    assert_eq!(
        buf.buffer_local_value("major-mode"),
        Some(Value::symbol("fundamental-mode"))
    );
    assert_eq!(
        buf.buffer_local_value("mode-name"),
        Some(Value::string("Fundamental"))
    );
    assert_eq!(buf.buffer_local_value("buffer-file-name"), Some(Value::NIL));
    assert_eq!(
        buf.buffer_local_value("buffer-auto-save-file-name"),
        Some(Value::NIL)
    );
    assert_eq!(
        buf.buffer_local_value("buffer-display-count"),
        Some(Value::fixnum(0))
    );
    assert_eq!(
        buf.buffer_local_value("buffer-display-time"),
        Some(Value::NIL)
    );
    assert_eq!(
        buf.buffer_local_value("buffer-invisibility-spec"),
        Some(Value::T)
    );
    assert_eq!(buf.buffer_local_value("buffer-undo-list"), Some(Value::NIL));
}

#[test]
fn ordered_buffer_local_bindings_use_symbol_ids() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_buffer_local("fill-column", Value::fixnum(80));
    buf.set_buffer_local("major-mode", Value::symbol("text-mode"));

    let ordered = buf.ordered_buffer_local_bindings();
    assert!(
        ordered
            .iter()
            .any(|(sym_id, _)| *sym_id == crate::emacs_core::intern::intern("fill-column"))
    );
    assert!(
        ordered
            .iter()
            .any(|(sym_id, _)| *sym_id == crate::emacs_core::intern::intern("major-mode"))
    );
    assert!(
        ordered
            .iter()
            .any(|(sym_id, _)| *sym_id == crate::emacs_core::intern::intern("buffer-undo-list"))
    );
}

#[test]
fn buffer_file_name_variable_tracks_slot_backed_state() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(buf.buffer_local_value("buffer-file-name"), Some(Value::NIL));

    buf.set_buffer_local("buffer-file-name", Value::string("/tmp/demo.txt"));
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some("/tmp/demo.txt")
    );
    assert_eq!(buf.file_name_value(), Value::string("/tmp/demo.txt"));
    assert_eq!(
        buf.buffer_local_value("buffer-file-name"),
        Some(Value::string("/tmp/demo.txt"))
    );

    buf.set_buffer_local("buffer-file-name", Value::NIL);
    assert!(buf.file_name_value().is_nil());
    assert_eq!(buf.buffer_local_value("buffer-file-name"), Some(Value::NIL));
}

#[test]
fn buffer_auto_save_file_name_variable_tracks_slot_backed_state() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(
        buf.buffer_local_value("buffer-auto-save-file-name"),
        Some(Value::NIL)
    );

    buf.set_buffer_local(
        "buffer-auto-save-file-name",
        Value::string("/tmp/#demo.txt#"),
    );
    assert_eq!(
        buf.auto_save_file_name_runtime_string_owned().as_deref(),
        Some("/tmp/#demo.txt#")
    );
    assert_eq!(
        buf.auto_save_file_name_value(),
        Value::string("/tmp/#demo.txt#")
    );
    assert_eq!(
        buf.buffer_local_value("buffer-auto-save-file-name"),
        Some(Value::string("/tmp/#demo.txt#"))
    );

    buf.set_buffer_local("buffer-auto-save-file-name", Value::NIL);
    assert!(buf.auto_save_file_name_value().is_nil());
    assert_eq!(
        buf.buffer_local_value("buffer-auto-save-file-name"),
        Some(Value::NIL)
    );
}

// -----------------------------------------------------------------------
// Modified flag
// -----------------------------------------------------------------------

#[test]
fn modified_flag() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert!(!buf.is_modified());
    buf.insert("x");
    assert!(buf.is_modified());
    buf.set_modified(false);
    assert!(!buf.is_modified());
}

#[test]
fn modified_state_tracks_autosaved_semantics() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(buf.modified_state_value(), Value::NIL);
    assert!(!buf.recent_auto_save_p());
    assert_eq!(buf.modified_tick(), 1);
    assert_eq!(buf.chars_modified_tick(), 1);

    assert_eq!(buf.restore_modified_state(Value::T), Value::T);
    assert_eq!(buf.modified_state_value(), Value::T);
    assert_eq!(buf.modified_tick(), 2);
    assert_eq!(buf.chars_modified_tick(), 1);
    assert!(!buf.recent_auto_save_p());

    assert_eq!(
        buf.restore_modified_state(Value::symbol("autosaved")),
        Value::symbol("autosaved")
    );
    assert_eq!(buf.modified_state_value(), Value::symbol("autosaved"));
    assert_eq!(buf.modified_tick(), 2);
    assert_eq!(buf.chars_modified_tick(), 1);
    assert!(buf.recent_auto_save_p());

    assert_eq!(buf.restore_modified_state(Value::NIL), Value::NIL);
    assert_eq!(buf.modified_state_value(), Value::NIL);
    assert_eq!(buf.modified_tick(), 2);
    assert_eq!(buf.chars_modified_tick(), 1);
    assert!(!buf.recent_auto_save_p());
}

#[test]
fn modification_ticks_track_content_changes() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(buf.modified_tick(), 1);
    assert_eq!(buf.chars_modified_tick(), 1);

    buf.insert("abcdef");
    assert_eq!(buf.modified_tick(), 4);
    assert_eq!(buf.chars_modified_tick(), 4);

    buf.set_modified(false);
    assert_eq!(buf.modified_tick(), 4);
    assert_eq!(buf.chars_modified_tick(), 4);
    assert_eq!(buf.modified_state_value(), Value::NIL);

    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(0, 6));
    assert_eq!(buf.modified_tick(), 7);
    assert_eq!(buf.chars_modified_tick(), 7);
    assert_eq!(buf.modified_state_value(), Value::T);
}

#[test]
fn props_modified_tick_tracks_text_property_changes_independently_of_chars() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(buf.modified_tick(), 1);
    assert_eq!(buf.chars_modified_tick(), 1);
    assert_eq!(buf.props_modified_tick(), 1);

    // A char edit bumps the chars tick (and modiff) but NOT the props tick:
    // redisplay must be able to tell "text changed" from "appearance changed".
    buf.insert("abcdef");
    let after_insert_modiff = buf.modified_tick();
    assert_eq!(buf.chars_modified_tick(), after_insert_modiff);
    assert_eq!(
        buf.props_modified_tick(),
        1,
        "a char edit must not move the props tick"
    );

    // A text-property modification bumps the props tick (and modiff) but NOT
    // the chars tick. This is the signal the cursor-only / edit classifier
    // needs: `put-text-property` of a face/display/invisible prop changes
    // appearance without changing buffer text.
    buf.record_text_property_modification();
    assert!(buf.modified_tick() > after_insert_modiff);
    assert_eq!(
        buf.props_modified_tick(),
        buf.modified_tick(),
        "the props tick rejoins modiff on a text-property change"
    );
    assert_eq!(
        buf.chars_modified_tick(),
        after_insert_modiff,
        "a text-property change must not move the chars tick"
    );
}

#[test]
fn chars_modified_tick_rejoins_modiff_after_non_char_modification() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("test"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(buf.restore_modified_state(Value::T), Value::T);
    assert_eq!(buf.modified_tick(), 2);
    assert_eq!(buf.chars_modified_tick(), 1);

    buf.insert("x");
    assert_eq!(buf.modified_tick(), 3);
    assert_eq!(buf.chars_modified_tick(), 3);
    assert_eq!(buf.modified_state_value(), Value::T);
}

// -----------------------------------------------------------------------
// BufferManager — creation, lookup, kill
// -----------------------------------------------------------------------

#[test]
fn manager_starts_with_scratch() {
    crate::test_utils::init_test_tracing();
    let mgr = BufferManager::new();
    let scratch = mgr.find_buffer_by_name("*scratch*");
    assert!(scratch.is_some());
    assert!(mgr.current_buffer().is_some());
    assert_eq!(
        mgr.current_buffer().unwrap().name_value(),
        Value::string("*scratch*")
    );
}

#[test]
fn manager_create_and_lookup() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = mgr.create_buffer("foo.el");
    assert!(mgr.get(id).is_some());
    assert_eq!(mgr.get(id).unwrap().name_value(), Value::string("foo.el"));
    assert_eq!(mgr.find_buffer_by_name("foo.el"), Some(id));
    assert_eq!(mgr.find_buffer_by_name("bar.el"), None);
}

#[test]
fn manager_set_current() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let a = mgr.create_buffer("a");
    let b = mgr.create_buffer("b");
    mgr.set_current(a);
    assert_eq!(
        mgr.current_buffer().unwrap().name_value(),
        Value::string("a")
    );
    mgr.set_current(b);
    assert_eq!(
        mgr.current_buffer().unwrap().name_value(),
        Value::string("b")
    );
}

#[test]
fn indirect_buffer_reads_undo_list_from_shared_state() {
    // Phase 10F: `buffer-undo-list` now reads directly from
    // `SharedUndoState` via `Buffer::get_undo_list`, so both
    // base and indirect buffers observe the same value without
    // any per-buffer cache. The previous version of this test
    // verified the cache-refresh behavior that the old
    // `BufferLocals::lisp_bindings` mirror needed — that
    // mirror is gone, and so is the refresh dance.
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let base_id = mgr.current_buffer_id().expect("scratch buffer");
    let indirect_id = mgr
        .create_indirect_buffer(base_id, "*switch-current-indirect*", false)
        .expect("indirect buffer");
    let _ = mgr.insert_into_buffer(base_id, "abc");

    let shared = mgr.get(base_id).expect("base buffer").get_undo_list();
    assert_eq!(
        mgr.get(indirect_id)
            .expect("indirect buffer")
            .get_buffer_local("buffer-undo-list"),
        Some(shared)
    );
}

#[test]
fn manager_kill_buffer() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = mgr.create_buffer("doomed");
    assert!(mgr.kill_buffer(id));
    assert!(mgr.get(id).is_none());
    assert!(!mgr.kill_buffer(id)); // already dead
}

#[test]
fn manager_kill_current_clears_current() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let scratch = mgr.find_buffer_by_name("*scratch*").unwrap();
    mgr.set_current(scratch);
    mgr.kill_buffer(scratch);
    assert!(mgr.current_buffer().is_none());
}

#[test]
fn manager_buffer_list() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let scratch = mgr.find_buffer_by_name("*scratch*").expect("scratch");
    let a = mgr.create_buffer("a");
    let b = mgr.create_buffer("b");
    assert_eq!(mgr.buffer_list(), vec![scratch, a, b]);
}

#[test]
fn manager_recorded_switch_records_even_when_buffer_is_already_current() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let scratch = mgr.find_buffer_by_name("*scratch*").expect("scratch");
    let a = mgr.create_buffer("a");
    let b = mgr.create_buffer("b");

    mgr.switch_current(a);
    mgr.switch_current(b);
    assert_eq!(mgr.buffer_list(), vec![b, a, scratch]);

    mgr.switch_current_unrecorded(a);
    assert_eq!(mgr.current_buffer_id(), Some(a));
    assert_eq!(mgr.buffer_list(), vec![b, a, scratch]);

    mgr.switch_current(a);
    assert_eq!(mgr.buffer_list(), vec![a, b, scratch]);
}

#[test]
fn manager_order_after_relinks_without_selecting_or_recording_head() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let scratch = mgr.find_buffer_by_name("*scratch*").expect("scratch");
    let messages = mgr.create_buffer("*Messages*");
    let minibuf = mgr.create_buffer(" *Minibuf-0*");

    assert_eq!(mgr.current_buffer_id(), Some(scratch));
    assert!(mgr.note_buffer_order_after(messages, minibuf));
    assert_eq!(mgr.current_buffer_id(), Some(scratch));
    assert_eq!(mgr.buffer_list(), vec![scratch, minibuf, messages]);
}

#[test]
fn manager_generate_new_buffer_name_unique() {
    crate::test_utils::init_test_tracing();
    let mgr = BufferManager::new();
    // "*scratch*" is taken, "foo" is not.
    assert_eq!(mgr.generate_new_buffer_name("foo"), "foo");
    assert_eq!(mgr.generate_new_buffer_name("*scratch*"), "*scratch*<2>");
}

#[test]
fn manager_generate_new_buffer_name_increments() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    mgr.create_buffer("buf");
    assert_eq!(mgr.generate_new_buffer_name("buf"), "buf<2>");
    mgr.create_buffer("buf<2>");
    assert_eq!(mgr.generate_new_buffer_name("buf"), "buf<3>");
}

#[test]
fn manager_generate_new_buffer_name_honors_ignore_candidate() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    mgr.create_buffer("buf");
    mgr.create_buffer("buf<2>");
    assert_eq!(
        mgr.generate_new_buffer_name_ignoring("buf", Some("buf<2>")),
        "buf<2>"
    );
    assert_eq!(
        mgr.generate_new_buffer_name_ignoring("buf", Some("buf<3>")),
        "buf<3>"
    );
}

#[test]
fn manager_generate_new_buffer_name_hidden_buffer_uses_random_suffix() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    mgr.create_buffer(" hidden");
    assert_eq!(
        mgr.generate_new_buffer_name_ignoring_with_random(" hidden", None, || 123_456),
        " hidden-123456"
    );
}

#[test]
fn manager_generate_new_buffer_name_hidden_buffer_falls_back_after_random_collision() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    mgr.create_buffer(" hidden");
    mgr.create_buffer(" hidden-123456");
    assert_eq!(
        mgr.generate_new_buffer_name_ignoring_with_random(" hidden", None, || 123_456),
        " hidden-123456<2>"
    );
}

// -----------------------------------------------------------------------
// BufferManager — markers
// -----------------------------------------------------------------------

#[test]
fn manager_create_and_query_marker() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = mgr.create_buffer("m");
    // Insert some text so there is room for a marker.
    mgr.insert_into_buffer(id, "abcdef").expect("insert text");

    let (mid, _) =
        mgr.create_marker_at_emacs_byte_pos(id, EmacsBytePos::new(3), InsertionType::After);
    assert_eq!(
        mgr.marker_emacs_byte_pos(id, mid).map(EmacsBytePos::get),
        Some(3)
    );
    assert_eq!(mgr.marker_char_pos(id, mid).map(CharPos0::get), Some(3));
}

#[test]
fn manager_marker_clamped_to_buffer_len() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = mgr.create_buffer("m");
    // Buffer is empty (len = 0), marker at 100 should be clamped.
    let (mid, _) =
        mgr.create_marker_at_emacs_byte_pos(id, EmacsBytePos::new(100), InsertionType::Before);
    assert_eq!(
        mgr.marker_emacs_byte_pos(id, mid).map(EmacsBytePos::get),
        Some(0)
    );
    assert_eq!(mgr.marker_char_pos(id, mid).map(CharPos0::get), Some(0));
}

#[test]
fn manager_marker_nonexistent_buffer() {
    crate::test_utils::init_test_tracing();
    let mgr = BufferManager::new();
    let pos = mgr.marker_emacs_byte_pos(BufferId(9999), 1);
    assert_eq!(pos, None);
}

#[test]
fn manager_labeled_widen_uses_innermost_and_without_restriction_reaches_full_buffer() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = mgr.create_buffer("labeled");
    mgr.set_current(id);
    mgr.get_mut(id).unwrap().insert("abcdef");

    let _ = mgr.internal_labeled_narrow_to_emacs_byte_range(
        id,
        EmacsByteRange::from_usize(1, 4),
        Value::symbol("tag"),
    );
    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 4);

    let _ = mgr.widen_buffer(id);
    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 4);

    let _ = mgr.internal_labeled_widen(id, &Value::symbol("tag"));
    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 6);
}

#[test]
fn manager_save_restriction_state_restores_labeled_stack() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = mgr.create_buffer("saved-labeled");
    mgr.set_current(id);
    mgr.get_mut(id).unwrap().insert("abcdefgh");
    let _ = mgr.internal_labeled_narrow_to_emacs_byte_range(
        id,
        EmacsByteRange::from_usize(1, 5),
        Value::symbol("tag"),
    );

    let saved = mgr
        .save_current_restriction_state()
        .expect("restriction state should save");
    let _ = mgr.internal_labeled_widen(id, &Value::symbol("tag"));
    let _ = mgr.narrow_buffer_to_emacs_byte_range(id, EmacsByteRange::from_usize(2, 3));
    mgr.restore_saved_restriction_state(saved);

    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 5);

    let _ = mgr.widen_buffer(id);
    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 5);
}

#[test]
fn manager_reset_outermost_restrictions_restores_current_innermost_after_mutation() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = mgr.create_buffer("redisplay-labeled");
    mgr.set_current(id);
    mgr.get_mut(id).unwrap().insert("abcdef");

    let _ = mgr.internal_labeled_narrow_to_emacs_byte_range(
        id,
        EmacsByteRange::from_usize(1, 5),
        Value::symbol("outer"),
    );
    let _ = mgr.internal_labeled_narrow_to_emacs_byte_range(
        id,
        EmacsByteRange::from_usize(2, 4),
        Value::symbol("inner"),
    );

    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 2);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 4);

    let saved = mgr.reset_outermost_restrictions();
    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 6);

    let _ = mgr.internal_labeled_widen(id, &Value::symbol("inner"));
    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 5);

    mgr.restore_outermost_restrictions(saved);
    let buf = mgr.get(id).unwrap();
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 1);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 5);
}

// -----------------------------------------------------------------------
// BufferManager — current_buffer_mut
// -----------------------------------------------------------------------

#[test]
fn manager_current_buffer_mut_insert() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let current = mgr.current_buffer_id().unwrap();
    mgr.insert_into_buffer(current, "hello");
    assert_eq!(mgr.current_buffer().unwrap().buffer_string(), "hello");
}

#[test]
fn manager_replace_buffer_contents_resets_narrowing_and_point() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let current = mgr.current_buffer_id().unwrap();
    let buf = mgr.get_mut(current).unwrap();
    buf.insert("abcdefgh");
    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 6));
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));

    mgr.replace_buffer_contents(current, "xy");

    let buf = mgr.get(current).unwrap();
    assert_eq!(buf.buffer_string(), "xy");
    assert_eq!(buf.point_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 2);
}

// -----------------------------------------------------------------------
// Integration: multiple operations
// -----------------------------------------------------------------------

#[test]
fn integration_edit_narrow_widen() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("work"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert("abcdefghij");
    assert_eq!(buf.buffer_string(), "abcdefghij");

    buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 8));
    assert_eq!(buf.buffer_string(), "cdefgh");

    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(5));
    buf.insert("XX");
    assert_eq!(buf.buffer_string(), "cdeXXfgh");

    buf.widen();
    assert_eq!(buf.buffer_string(), "abcdeXXfghij");
}

// -----------------------------------------------------------------------
// T8 C-1 regression: state markers must survive GC without Lisp refs
// -----------------------------------------------------------------------

#[test]
fn state_markers_survive_gc_without_lisp_references() {
    // Post-T8 invariant: BufferStateMarkers.pt_marker_ptr / begv_marker_ptr /
    // zv_marker_ptr must survive GC even when no Lisp value holds them. If the
    // chain is the only structural reference AND the chain isn't rooted, an
    // unmarked marker would be spliced out by unchain_dead_markers and freed,
    // leaving the state_markers struct pointing at freed memory.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // Create a base buffer with some text, then make an indirect buffer.
    // `create_indirect_buffer` is the code path that calls
    // `ensure_buffer_state_markers` on both the root and the indirect,
    // which is what materialises the pt/begv/zv state markers.
    let base_id = eval.buffers.current_buffer_id().expect("scratch buffer");
    let _ = eval.buffers.insert_into_buffer(base_id, "hello world");
    let indirect_id = eval
        .buffers
        .create_indirect_buffer(base_id, "*gc-state-marker-indirect*", false)
        .expect("indirect buffer");

    // Snapshot the raw pointers from state_markers before GC. Use the
    // indirect buffer because that is where the noncurrent-state markers
    // conceptually live (the root also gets one for the same reason).
    let (pt_ptr, begv_ptr, zv_ptr, expected_buf) = {
        let buffer = eval
            .buffers
            .get(indirect_id)
            .expect("indirect buffer present");
        let sm = buffer
            .state_markers
            .as_ref()
            .expect("state markers populated by create_indirect_buffer");
        (
            sm.pt_marker_ptr,
            sm.begv_marker_ptr,
            sm.zv_marker_ptr,
            buffer.id,
        )
    };

    // Sanity: all three pointers are non-null and distinct.
    assert!(!pt_ptr.is_null(), "pt_marker_ptr populated");
    assert!(!begv_ptr.is_null(), "begv_marker_ptr populated");
    assert!(!zv_ptr.is_null(), "zv_marker_ptr populated");

    // Walk the indirect buffer's marker chain BEFORE GC: we expect pt,
    // begv, zv to all be present (the chain-head slot ultimately points
    // at one of them, and each one's `next_marker` eventually reaches
    // the others). This is our positive baseline — if the pre-GC chain
    // does not contain these three, the test setup is wrong.
    let chain_contains_before = unsafe {
        let buffer = eval
            .buffers
            .get(indirect_id)
            .expect("indirect buffer present");
        buffer.marker_chain_contains_raw_for_test([pt_ptr, begv_ptr, zv_ptr])
    };
    assert!(
        chain_contains_before.iter().all(|&b| b),
        "pre-GC baseline: chain must contain all three state markers, got {chain_contains_before:?}"
    );

    // Force a full GC. No Lisp value references these three markers; the
    // only structural references are (a) the intrusive marker chain and
    // (b) the `BufferStateMarkers` raw pointers. If neither is treated as
    // a GC root, `unchain_dead_markers` will splice them out and
    // `sweep_objects` will free them.
    eval.gc_collect_exact();

    // After GC, the pointers must still point at LIVE markers whose
    // header has the expected tag and whose data still reflects the
    // buffer binding. Reading a freed allocation is UB; we can't make
    // this test segfault-proof without ASAN, but if the allocation was
    // reused for something else, `data.buffer` will almost certainly no
    // longer match `expected_buf`.
    unsafe {
        let pt_buffer = (*pt_ptr).data.buffer;
        let begv_buffer = (*begv_ptr).data.buffer;
        let zv_buffer = (*zv_ptr).data.buffer;

        assert_eq!(pt_buffer, Some(expected_buf), "pt_marker survived GC");
        assert_eq!(begv_buffer, Some(expected_buf), "begv_marker survived GC");
        assert_eq!(zv_buffer, Some(expected_buf), "zv_marker survived GC");

        assert!(
            (*pt_ptr).data.marker_id.is_some(),
            "pt_marker retains its marker_id"
        );
        assert!(
            (*begv_ptr).data.marker_id.is_some(),
            "begv_marker retains its marker_id"
        );
        assert!(
            (*zv_ptr).data.marker_id.is_some(),
            "zv_marker retains its marker_id"
        );
    }

    // The chain must STILL contain all three state markers after GC;
    // `unchain_dead_markers` splices out anything with `header.gc.marked`
    // false, so a post-GC chain containing them proves they were marked
    // (i.e. they were treated as reachable by the mark phase).
    let chain_contains_after = unsafe {
        let buffer = eval
            .buffers
            .get(indirect_id)
            .expect("indirect buffer present");
        buffer.marker_chain_contains_raw_for_test([pt_ptr, begv_ptr, zv_ptr])
    };
    assert!(
        chain_contains_after.iter().all(|&b| b),
        "post-GC: all three state markers must remain on the chain; got {chain_contains_after:?}. \
         A `false` here proves C-1: an unmarked state marker was spliced out and its allocation \
         freed, leaving BufferStateMarkers with a dangling pointer."
    );
}

#[test]
fn buffer_mark_marker_survives_gc_without_lisp_reference() {
    // GNU stores the mark as `BVAR (buffer, mark)`, so `mark_buffer`
    // roots it with the rest of the buffer object.  Neomacs mirrors that
    // with Buffer::mark_marker_ptr; the raw pointer must be traced even
    // when no Lisp variable currently holds `(mark-marker)`.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("scratch buffer");
    let _ = eval.buffers.insert_into_buffer(buffer_id, "alpha\nbeta\n");
    let _ = eval
        .buffers
        .set_buffer_mark_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(1));

    let mark_ptr = eval
        .buffers
        .get(buffer_id)
        .expect("buffer present")
        .mark_marker_ptr;
    assert!(!mark_ptr.is_null(), "mark marker should be materialized");

    eval.gc_collect_exact();

    let mark = crate::emacs_core::marker::builtin_mark_marker(&mut eval, vec![])
        .expect("mark-marker should return a live marker after GC");
    assert!(mark.is_marker(), "mark-marker remains a marker after GC");
    let position =
        crate::emacs_core::marker::marker_position_as_int_with_buffers(&eval.buffers, &mark)
            .expect("marker-position should accept the buffer mark after GC");
    assert_eq!(position, 2);

    unsafe {
        assert_eq!(
            (*mark_ptr).data.buffer,
            Some(buffer_id),
            "raw buffer mark pointer still refers to its buffer"
        );
    }
}

#[test]
fn changed_char_range_tracks_real_edits() {
    let mut buf = buf_with_text("hello world\nfoo bar\nbaz qux\n");
    // Ack to start from a clean (fully-unchanged) state.
    buf.reset_unchanged_region();
    assert_eq!(buf.changed_char_range(), None);

    // Insert one char at position 6.
    buf.goto_emacs_byte_pos(EmacsBytePos::new(6));
    buf.insert("X");
    assert_eq!(buf.changed_char_range(), Some((6, 7)));

    // A second insert at 0 unions in; the earlier X has shifted to 7, so the
    // dirty span grows to [0, 8).
    buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    buf.insert("Y");
    assert_eq!(buf.changed_char_range(), Some((0, 8)));

    // Ack clears it.
    buf.reset_unchanged_region();
    assert_eq!(buf.changed_char_range(), None);

    // A delete of 3 chars at [10, 13) leaves an (empty) change marker at 10.
    buf.delete_emacs_byte_range(EmacsByteRange::from_start_len(
        EmacsBytePos::new(10),
        EmacsByteLen::new(3),
    ));
    assert_eq!(buf.changed_char_range(), Some((10, 10)));
}
