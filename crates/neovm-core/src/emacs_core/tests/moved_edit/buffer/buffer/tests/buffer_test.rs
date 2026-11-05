use neovm_edit::buffer::buffer::*;

#[doc(hidden)]
pub use neovm_edit::buffer::text::{BufferTextBytesSnapshot, ImplementedBufferTextBackendKind};

use neovm_edit::buffer::{CharRange, LispCharPos1};

use neovm_value::heap_types::{LispString, OverlayDataInit};

use neovm_value::value::ValueKind;

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
    use neovm_edit::buffer::VisitedFileModtime;

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

#[test]
fn state_markers_survive_gc_without_lisp_references() {
    // Post-T8 invariant: BufferStateMarkers.pt_marker_ptr / begv_marker_ptr /
    // zv_marker_ptr must survive GC even when no Lisp value holds them. If the
    // chain is the only structural reference AND the chain isn't rooted, an
    // unmarked marker would be spliced out by unchain_dead_markers and freed,
    // leaving the state_markers struct pointing at freed memory.
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    // Create a base buffer with some text, then make an indirect buffer.
    // `create_indirect_buffer` is the code path that calls
    // `ensure_buffer_state_markers` on both the root and the indirect,
    // which is what materialises the pt/begv/zv state markers.
    let base_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("scratch buffer");
    let _ = eval.edit.buffers.insert_into_buffer(base_id, "hello world");
    let indirect_id = eval
        .edit
        .buffers
        .create_indirect_buffer(base_id, "*gc-state-marker-indirect*", false)
        .expect("indirect buffer");

    // Snapshot the raw pointers from state_markers before GC. Use the
    // indirect buffer because that is where the noncurrent-state markers
    // conceptually live (the root also gets one for the same reason).
    let (pt_ptr, begv_ptr, zv_ptr, expected_buf) = {
        let buffer = eval
            .edit
            .buffers
            .get(indirect_id)
            .expect("indirect buffer present");
        let sm = buffer
            .state_markers
            .as_ref()
            .expect("state markers populated by create_indirect_buffer");
        (
            sm.pt_marker.as_ptr(),
            sm.begv_marker.as_ptr(),
            sm.zv_marker.as_ptr(),
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
            .edit
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
        let pt_buffer = (*pt_ptr).data.buffer_id();
        let begv_buffer = (*begv_ptr).data.buffer_id();
        let zv_buffer = (*zv_ptr).data.buffer_id();

        assert_eq!(pt_buffer, Some(expected_buf), "pt_marker survived GC");
        assert_eq!(begv_buffer, Some(expected_buf), "begv_marker survived GC");
        assert_eq!(zv_buffer, Some(expected_buf), "zv_marker survived GC");

        assert!(
            (*pt_ptr).data.chained(),
            "pt_marker is still spliced into the chain"
        );
        assert!(
            (*begv_ptr).data.chained(),
            "begv_marker is still spliced into the chain"
        );
        assert!(
            (*zv_ptr).data.chained(),
            "zv_marker is still spliced into the chain"
        );
    }

    // The chain must STILL contain all three state markers after GC;
    // `unchain_dead_markers` splices out anything with `header.gc.marked`
    // false, so a post-GC chain containing them proves they were marked
    // (i.e. they were treated as reachable by the mark phase).
    let chain_contains_after = unsafe {
        let buffer = eval
            .edit
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
    let mut eval = Context::new();
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("scratch buffer");
    let _ = eval
        .edit
        .buffers
        .insert_into_buffer(buffer_id, "alpha\nbeta\n");
    let _ = eval
        .edit
        .buffers
        .set_buffer_mark_emacs_byte_pos(buffer_id, neovm_edit::buffer::EmacsBytePos::new(1));

    let mark_ptr = eval
        .edit
        .buffers
        .get(buffer_id)
        .expect("buffer present")
        .mark_marker
        .as_ptr();
    assert!(!mark_ptr.is_null(), "mark marker should be materialized");

    eval.gc_collect_exact();

    let mark = neovm_edit::emacs_core::marker::builtin_mark_marker(&mut eval.edit.buffers, vec![])
        .expect("mark-marker should return a live marker after GC");
    assert!(mark.is_marker(), "mark-marker remains a marker after GC");
    let position = neovm_edit::emacs_core::marker::marker_position_as_int_with_buffers(
        &eval.edit.buffers,
        &mark,
    )
    .expect("marker-position should accept the buffer mark after GC");
    assert_eq!(position, 2);

    unsafe {
        assert_eq!(
            (*mark_ptr).data.buffer_id(),
            Some(buffer_id),
            "raw buffer mark pointer still refers to its buffer"
        );
    }
    // The typed reference kept by the buffer is the SAME object GC kept
    // alive: `mark-marker` returns it by pointer identity.
    let same = neovm_edit::emacs_core::marker::builtin_mark_marker(&mut eval.edit.buffers, vec![])
        .expect("mark-marker after GC");
    assert_eq!(
        same.as_veclike_ptr().map(|p| p as *const u8),
        Some(mark_ptr as *const u8),
        "mark-marker must return the rooted marker object"
    );
}

#[test]
fn mark_marker_object_survives_gc_before_any_mark_is_set() {
    // GNU gives every buffer a real detached mark marker at creation
    // (`bset_mark (b, Fmake_marker ())`, buffer.c) and `mark_buffer` marks
    // it. Neomacs materializes that marker lazily; `Buffer::mark_marker`
    // must root it even while it points nowhere, and it must stay the same
    // object so `(mark-marker)` identity is stable across GC.
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let first = neovm_edit::emacs_core::marker::builtin_mark_marker(&mut eval.edit.buffers, vec![])
        .expect("mark-marker");
    assert!(first.is_marker());
    assert_eq!(
        first.as_marker_data().unwrap().buffer_id(),
        None,
        "a fresh buffer's mark points nowhere"
    );
    let first_ptr = first.as_veclike_ptr().expect("marker veclike pointer");

    eval.gc_collect_exact();

    let again = neovm_edit::emacs_core::marker::builtin_mark_marker(&mut eval.edit.buffers, vec![])
        .expect("mark-marker after GC");
    assert_eq!(
        again.as_veclike_ptr(),
        Some(first_ptr),
        "the detached mark marker is rooted by Buffer::mark_marker"
    );
}

/// The automatic-composition memo must answer from cache only while every
/// input it reads is unchanged, and must recompute when any of them moves.
/// A cache that goes stale silently is worse than no cache: the scan feeds
/// glyph composition, so a stale answer is a wrong screen.
#[test]
fn automatic_composition_memo_recomputes_when_any_input_it_reads_changes() {
    crate::test_utils::init_test_tracing();
    // A bare Buffer outside a Context has no evaluator construction to
    // publish the text-layer composition scans (stage-3 C7), so install them
    // here explicitly; inside an editor the constructor already did.
    crate::emacs_core::composite::install_substrate_seams();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("compose"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert("hello");

    // Not a char-table: the scan declines, but the memo still has to behave.
    let table = Value::NIL;
    let scans =
        || crate::emacs_core::composite::BYTES_SCANNED.load(std::sync::atomic::Ordering::Relaxed);

    let before = scans();
    let first = buf.automatic_composition_spans(table);
    let after_first = scans();
    assert!(after_first > before, "the first call must actually scan");

    // Same inputs: served from the memo, no further scanning.
    let second = buf.automatic_composition_spans(table);
    assert_eq!(scans(), after_first, "an unchanged buffer must not rescan");
    assert!(
        std::rc::Rc::ptr_eq(&first, &second),
        "the memo must hand back the same allocation"
    );

    // Text changed: rescan.
    buf.insert("!");
    let _ = buf.automatic_composition_spans(table);
    let after_edit = scans();
    assert!(after_edit > after_first, "an edit must invalidate the memo");

    // Syntax table changed with no edit: the regexes consult it, so rescan.
    // Keying on the text ticks alone would go stale here.
    buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()] = Value::string("a different table");
    let _ = buf.automatic_composition_spans(table);
    assert!(
        scans() > after_edit,
        "a syntax-table change must invalidate the memo"
    );
}

/// Killing a base buffer kills its indirect children, whether or not the
/// session had made an indirect buffer before.
///
/// `kill-buffer` finds those children by asking every buffer who its base is,
/// and that walk is now skipped when no live buffer is indirect.  The skip is
/// only sound while the "no live buffer is indirect" answer is exact, so this
/// exercises the transition in both directions: kill with a child present,
/// then kill an ordinary buffer once the child is gone.
///
/// Measured on the pinned GNU Emacs 31.1, `emacs -Q --batch`:
///
/// ```elisp
/// (let* ((base (generate-new-buffer "neo-base"))
///        (ind (make-indirect-buffer base "neo-ind")))
///   (kill-buffer base)
///   (list (buffer-live-p base) (buffer-live-p ind)))   ;; => (nil nil)
/// ```
#[test]
fn killing_a_base_buffer_kills_its_indirect_children() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_all(
            r#"
(let* ((base (generate-new-buffer "neo-base"))
       (ind (make-indirect-buffer base "neo-ind"))
       (killed (progn (kill-buffer base)
                      (list (buffer-live-p base) (buffer-live-p ind))))
       (plain (generate-new-buffer "neo-plain")))
  (kill-buffer plain)
  (list killed (buffer-live-p plain)))
"#,
        ),
        vec!["OK ((nil nil) nil)"],
    );
}

/// The JIT bakes [`Buffer::POINT_CHAR_POS_OFFSET`] and its two companions into
/// generated code and loads through them directly, so a wrong offset would not
/// fail to compile -- it would silently read a neighbouring field. Read through
/// the raw offsets the way generated code does and require the same answer the
/// accessors give, after point and the accessible region have all been moved to
/// distinct values so no two fields could be confused for each other.
#[test]
fn the_baked_position_offsets_address_the_fields_the_accessors_read() {
    let mut buf = buf_with_text("hello world");
    buf.set_accessible_region_and_point_from_emacs_bytes(
        EmacsByteRange::from_usize(2, 8),
        EmacsBytePos::new(5),
    );

    // Distinct on purpose: BEGV 2, point 5, ZV 8. An offset that landed on the
    // wrong anchor would still read a plausible position, so the three must not
    // be allowed to coincide.
    assert_eq!(buf.point_char_pos().get(), 5);
    assert_eq!(buf.point_min_char_pos().get(), 2);
    assert_eq!(buf.point_max_char_pos().get(), 8);

    let base = std::ptr::from_ref(&buf).cast::<u8>();
    // SAFETY: each offset is `offset_of!` within this very buffer object, so
    // the address is inside `buf` and holds the `usize` the anchor stores.
    let read = |offset: usize| unsafe { base.add(offset).cast::<usize>().read() };

    assert_eq!(
        read(Buffer::POINT_CHAR_POS_OFFSET),
        buf.point_char_pos().get()
    );
    assert_eq!(
        read(Buffer::BEGV_CHAR_POS_OFFSET),
        buf.point_min_char_pos().get()
    );
    assert_eq!(
        read(Buffer::ZV_CHAR_POS_OFFSET),
        buf.point_max_char_pos().get()
    );
}

#[test]
fn buffer_property_overlay_and_symbol_roots_survive_collection_after_edits() {
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
        (setq gc-cons-threshold 100000000)
        (insert "abcdefghijklmnop")
        (set (make-local-variable 'major-mode) (make-symbol "rooted-mode"))
        (put-text-property 1 9 'rooted-data (vector "property" (make-symbol "property-symbol")))
        (let ((o (make-overlay 3 12)))
          (overlay-put o 'rooted-data (vector "overlay")))
        (make-indirect-buffer (current-buffer) "rooted-indirect" nil))"#,
    )
    .unwrap();
    for edit in [
        "nil",
        r#"(progn (goto-char 4) (insert "XY"))"#,
        r#"(progn (delete-region 4 6) (put-text-property 10 15 'other-data (vector "split")))"#,
        "(remove-text-properties 10 15 '(other-data nil))",
    ] {
        eval.eval_str(edit).unwrap();
        // Keep the evaluator's immutable overlay copy rooted as well as the
        // live overlay objects; GC must preserve both enumeration paths.
        let current = eval.edit.buffers.current_buffer_id().unwrap();
        let _snapshot = eval.edit.buffers.get(current).unwrap().overlays.snapshot();
        eval.gc_collect_exact();
        assert_eq!(
            eval.eval_str(r#"(symbol-name major-mode)"#).unwrap(),
            Value::string("rooted-mode")
        );
        assert_eq!(
            eval.eval_str(r#"(aref (get-text-property 2 'rooted-data) 0)"#)
                .unwrap(),
            Value::string("property")
        );
        assert_eq!(
            eval.eval_str(r#"(symbol-name (aref (get-text-property 2 'rooted-data) 1))"#)
                .unwrap(),
            Value::string("property-symbol")
        );
        assert_eq!(
            eval.eval_str(r#"(aref (overlay-get (car (overlays-at 7)) 'rooted-data) 0)"#)
                .unwrap(),
            Value::string("overlay")
        );
        assert_eq!(eval.eval_str(r#"(let ((current (current-buffer))) (set-buffer "rooted-indirect") (prog1 (aref (get-text-property 2 'rooted-data) 0) (set-buffer current)))"#).unwrap(), Value::string("property"));
    }
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
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert(text);
    buf.widen();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    buf.set_undo_list(Value::NIL);
    buf.set_modified(false);
    buf
}

fn require_implemented_kind(kind: BufferTextBackendKind) -> ImplementedBufferTextBackendKind {
    kind.implemented()
        .expect("test backend should be implemented")
}

impl OptionalTextPositionSnapshot {
    fn new(char_pos: Option<CharPos0>, emacs_byte_pos: Option<EmacsBytePos>) -> Self {
        Self {
            char_pos,
            emacs_byte_pos,
        }
    }
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

#[derive(Debug, PartialEq, Eq)]
struct OptionalTextPositionSnapshot {
    char_pos: Option<CharPos0>,
    emacs_byte_pos: Option<EmacsBytePos>,
}

#[derive(Debug, PartialEq, Eq)]
struct OptionalEmacsByteRangeSnapshot {
    start: Option<EmacsBytePos>,
    end: Option<EmacsBytePos>,
}

#[cfg(test)]
#[path = "shared_overlay_modiff_test.rs"]
mod shared_overlay_modiff_test;
