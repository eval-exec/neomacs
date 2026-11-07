use super::*;

use neovm_value::error::LispCondition;
use neovm_value::error::{expect_args, expect_args_range};

use neovm_edit::buffer::{EmacsByteRange, LispCharPos1};
use neovm_eval::emacs_core::eval::Context;
use neovm_value::error::EvalResult;
use neovm_value::value::Value;

use neovm_edit::emacs_core::marker::*;

use crate::emacs_core::moved_edit::emacs_core::marker::builtin_marker_position;
use std::cell::RefCell;

#[test]
fn make_marker_creates_heap_marker() {
    crate::test_utils::init_test_tracing();
    let m = make_marker_value(None, Some(LispCharPos1::new(42)), false);
    assert!(is_marker(&m));
}

#[test]
fn make_marker_empty() {
    crate::test_utils::init_test_tracing();
    let m = make_marker_value(None, None, false);
    assert!(is_marker(&m));
    assert!(marker_position_value(&m).is_nil());
    assert!(marker_buffer_value(&m).is_nil());
}

#[test]
fn builtin_markerp_works() {
    crate::test_utils::init_test_tracing();
    let m = make_marker_value(None, None, false);
    assert!(builtin_markerp(vec![m]).unwrap().is_truthy());
    assert!(builtin_markerp(vec![Value::fixnum(5)]).unwrap().is_nil());
}

#[test]
fn builtin_marker_position_returns_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let m = make_registered_buffer_marker(
        &mut eval.edit.buffers,
        buffer_id,
        LispCharPos1::new(10),
        false,
    );
    let pos = builtin_marker_position(&mut eval, vec![m]).unwrap();
    assert!(pos.is_fixnum());
}

#[test]
fn builtin_marker_position_returns_nil_when_unset() {
    crate::test_utils::init_test_tracing();
    let m = make_marker_value(None, None, false);
    let pos = call_marker_position(vec![m]).unwrap();
    assert!(pos.is_nil());
}

#[test]
fn builtin_marker_buffer_returns_live_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let marker = make_marker_value(Some(buffer_id), Some(LispCharPos1::new(1)), false);
    let buf = builtin_marker_buffer_in_buffers(&eval.edit.buffers, &[marker]).unwrap();
    assert_eq!(buf, Value::make_buffer(buffer_id));
}

#[test]
fn builtin_marker_insertion_type_roundtrip() {
    crate::test_utils::init_test_tracing();
    let m = make_marker_value(None, None, false);
    assert!(builtin_marker_insertion_type(vec![m]).unwrap().is_nil());

    call_set_marker_insertion_type(vec![m, Value::T]).unwrap();
    assert!(builtin_marker_insertion_type(vec![m]).unwrap().is_truthy());
}

#[test]
fn builtin_copy_marker_from_marker() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let m = make_registered_buffer_marker(
        &mut eval.edit.buffers,
        buffer_id,
        LispCharPos1::new(5),
        true,
    );
    let copy = builtin_copy_marker_in_buffers(&mut eval.edit.buffers, &vec![m]).unwrap();
    assert!(is_marker(&copy));
    assert!(marker_position_value(&copy).is_fixnum());
}

#[test]
fn builtin_copy_marker_from_integer() {
    crate::test_utils::init_test_tracing();
    // GNU verified: `(copy-marker 99)` produces a marker bound to the
    // current buffer (clamped to point-min/point-max). It is *not* a
    // bufferless marker like `(make-marker)`. Mirror that.
    let copy = call_copy_marker(vec![Value::fixnum(99)]).unwrap();
    assert!(is_marker(&copy));
    assert!(marker_position_value(&copy).is_fixnum());
    assert!(
        !marker_buffer_value(&copy).is_nil(),
        "copy-marker on an integer must bind to the current buffer"
    );
}

#[test]
fn builtin_copy_marker_from_integer_clips_like_set_marker() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.edit
        .buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abc");

    let before_min =
        builtin_copy_marker_in_buffers(&mut eval.edit.buffers, &vec![Value::fixnum(-10)])
            .expect("copy marker before point-min");
    assert_eq!(marker_position_value(&before_min), Value::fixnum(1));

    let after_max =
        builtin_copy_marker_in_buffers(&mut eval.edit.buffers, &vec![Value::fixnum(99)])
            .expect("copy marker after point-max");
    assert_eq!(marker_position_value(&after_max), Value::fixnum(4));
}

#[test]
fn builtin_make_marker_returns_empty() {
    crate::test_utils::init_test_tracing();
    let m = builtin_make_marker(&vec![]).unwrap();
    assert!(is_marker(&m));
    assert!(marker_position_value(&m).is_nil());
    assert!(marker_buffer_value(&m).is_nil());
    assert!(marker_insertion_type_value(&m).is_nil());
}

#[test]
fn wrong_type_signals_error() {
    crate::test_utils::init_test_tracing();
    let result = call_marker_position(vec![Value::fixnum(5)]);
    assert!(result.is_err());
}

#[test]
fn marker_accessors_require_zero_arguments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    assert!(builtin_point_marker_in_buffers(&mut eval.edit.buffers, vec![Value::NIL]).is_err());
    assert!(builtin_point_min_marker_in_buffers(&mut eval.edit.buffers, vec![Value::NIL]).is_err());
    assert!(builtin_point_max_marker_in_buffers(&mut eval.edit.buffers, vec![Value::NIL]).is_err());
    assert!(builtin_mark_marker(&mut eval.edit.buffers, vec![Value::NIL]).is_err());
}

#[test]
fn numeric_comparisons_use_live_marker_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(insert "abcdef\n123456\n")
           (goto-char 9)
           (let ((m (copy-marker (line-end-position))))
             (delete-region 1 2)
             (delete-region 7 8)
             (list (marker-position m)
                   (<= (point-max) m)
                   (<= (1- (point-max)) m)))"#,
        )
        .expect("evaluation succeeds");
    assert_eq!(
        neovm_eval::emacs_core::print::format_eval_result(&Ok(result)),
        "OK (12 nil t)"
    );
}

#[test]
fn point_min_and_max_markers_follow_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buf_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let _ = eval.edit.buffers.insert_into_buffer(buf_id, "ééz");
    let _ = eval.edit.buffers.narrow_buffer_to_emacs_byte_range(
        buf_id,
        EmacsByteRange::from_usize('é'.len_utf8(), "ééz".len()),
    );

    let min_marker = builtin_point_min_marker_in_buffers(&mut eval.edit.buffers, vec![])
        .expect("point-min-marker");
    let max_marker = builtin_point_max_marker_in_buffers(&mut eval.edit.buffers, vec![])
        .expect("point-max-marker");

    assert_eq!(
        call_marker_position(vec![min_marker]).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        call_marker_position(vec![max_marker]).unwrap(),
        Value::fixnum(4)
    );
}

#[test]
fn mark_marker_follows_cached_mark_char_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buf_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let _ = eval.edit.buffers.insert_into_buffer(buf_id, "ééz");
    let _ = eval.edit.buffers.set_buffer_mark_emacs_byte_pos(
        buf_id,
        neovm_edit::buffer::EmacsBytePos::new('é'.len_utf8()),
    );

    let marker = builtin_mark_marker(&mut eval.edit.buffers, vec![]).expect("mark-marker");
    assert_eq!(
        call_marker_position(vec![marker]).unwrap(),
        Value::fixnum(2)
    );
}

#[test]
fn mark_marker_with_no_mark_returns_detached() {
    // T7 I-1 follow-up: when the current buffer has no mark set,
    // `(mark-marker)` returns a detached marker. Post-T7 the stale
    // `LispMarker.position` cache is gone, so "unset" is discriminated
    // solely by `buffer.is_none() && charpos == 0`. This pins the
    // invariant by checking that both `(marker-buffer ...)` and
    // `(marker-position ...)` report nil.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // Don't set a mark; default buffer has no mark.
    let marker = builtin_mark_marker(&mut eval.edit.buffers, vec![]).expect("mark-marker");
    assert!(
        call_marker_buffer(vec![marker]).unwrap().is_nil(),
        "(marker-buffer (mark-marker)) should be nil when mark is unset"
    );
    assert!(
        call_marker_position(vec![marker]).unwrap().is_nil(),
        "(marker-position (mark-marker)) should be nil when mark is unset"
    );
}

#[test]
fn copy_marker_from_integer_tracks_insertions_before_it() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(insert "abc")
           (let ((m (copy-marker (point-max) t)))
             (goto-char 2)
             (insert "X")
             (list (marker-position m)
                   (buffer-string)))"#,
        )
        .expect("evaluation succeeds");
    assert_eq!(
        neovm_eval::emacs_core::print::format_eval_result(&Ok(result)),
        r#"OK (5 "aXbc")"#
    );
}

#[test]
fn set_marker_uses_live_source_marker_position_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(insert "abc")
           (let ((src (copy-marker (point-max) t))
                 (dst (make-marker)))
             (goto-char 2)
             (insert "X")
             (set-marker dst src)
             (marker-position dst))"#,
        )
        .expect("evaluation succeeds");
    assert_eq!(
        neovm_eval::emacs_core::print::format_eval_result(&Ok(result)),
        "OK 5"
    );
}

/// `set-marker` into the buffer the marker is ALREADY in must not touch the
/// buffer's marker chain.
///
/// GNU `attach_marker' (src/marker.c) writes `m->charpos'/`m->bytepos'
/// unconditionally and only unchains + re-chains when `m->buffer != b'.  We
/// unlinked and re-spliced every time, and BOTH of those walk the buffer's
/// intrusive marker chain -- so `set-marker' cost O(markers in the buffer).
///
/// Moving one marker over and over hides it, because the re-splice puts that
/// marker back at the head where the next unlink finds it immediately.  The
/// shape that bites is cyclic access -- walking a list of markers and moving
/// each, which is what org and magit do with their per-section marker pairs.
/// 40,000 such calls took 12.7ms over a 100-marker chain but 163.8ms over a
/// 4,000-marker chain, against a FLAT 8.5ms in GNU Emacs 31.1.
///
/// Chain order is the observable: a re-splice hoists the moved marker to the
/// head, so asserting the order survived pins the in-place update.
#[test]
fn set_marker_within_the_same_buffer_leaves_the_marker_chain_alone() {
    crate::test_utils::init_test_tracing();
    MARKER_TEST_CTX.with(|slot| {
        let mut ctx = Box::new(crate::test_utils::new_context());
        let buffer_id = ctx.edit.buffers.create_buffer("marker-chain-order");
        assert!(ctx.edit.buffers.switch_current(buffer_id));
        ctx.edit
            .buffers
            .insert_lisp_string_into_buffer(
                buffer_id,
                &neovm_value::heap_types::LispString::from_utf8("abcdefghij"),
            )
            .expect("insert buffer text");

        // Three markers, spliced at the head in turn, so the chain reads
        // [6, 4, 2] and the one we move is in the MIDDLE.
        let markers: Vec<Value> = [2i64, 4, 6]
            .into_iter()
            .map(|pos| {
                let pos = LispCharPos1::new(pos);
                let marker = make_marker_value(Some(buffer_id), Some(pos), false);
                register_marker_in_buffers(
                    &mut ctx.edit.buffers,
                    &marker,
                    Some(buffer_id),
                    Some(pos),
                );
                marker
            })
            .collect();

        let before = ctx.edit.buffers.get(buffer_id).unwrap().marker_chain_ptrs();
        assert_eq!(before.len(), 3, "three markers are on the chain");

        builtin_set_marker_in_buffers(&mut ctx.edit.buffers, &[markers[1], Value::fixnum(8)])
            .expect("set-marker");

        let after = ctx.edit.buffers.get(buffer_id).unwrap().marker_chain_ptrs();
        assert_eq!(
            before, after,
            "set-marker inside the same buffer re-chained the marker"
        );
        let moved = builtin_marker_position(&mut ctx, vec![markers[1]]).expect("marker-position");
        assert_eq!(moved.as_fixnum(), Some(8), "the position still moved");
        let others: Vec<Option<i64>> = [0usize, 2]
            .into_iter()
            .map(|i| {
                builtin_marker_position(&mut ctx, vec![markers[i]])
                    .expect("marker-position")
                    .as_fixnum()
            })
            .collect();
        assert_eq!(others, vec![Some(2), Some(6)], "the others did not move");

        // Changing buffer still unchains and re-chains: GNU's `m->buffer != b'
        // arm is the half that must survive the shortcut.
        let other_id = ctx.edit.buffers.create_buffer("marker-chain-order-other");
        ctx.edit
            .buffers
            .insert_lisp_string_into_buffer(
                other_id,
                &neovm_value::heap_types::LispString::from_utf8("zyxwv"),
            )
            .expect("insert other buffer text");
        builtin_set_marker_in_buffers(
            &mut ctx.edit.buffers,
            &[markers[1], Value::fixnum(3), Value::make_buffer(other_id)],
        )
        .expect("set-marker across buffers");
        assert_eq!(
            ctx.edit.buffers.get(buffer_id).unwrap().marker_chain_len(),
            2,
            "the marker left the old buffer's chain"
        );
        assert_eq!(
            ctx.edit.buffers.get(other_id).unwrap().marker_chain_len(),
            1,
            "the marker joined the new buffer's chain"
        );
        let moved = builtin_marker_position(&mut ctx, vec![markers[1]]).expect("marker-position");
        assert_eq!(moved.as_fixnum(), Some(3));

        *slot.borrow_mut() = Some(neovm_value::thread_exit_leak::ThreadExitLeak::new(*ctx));
    });
}

/// `set-marker-insertion-type` writes one bit on the marker and must not
/// search for it.
///
/// GNU `Fset_marker_insertion_type' (src/marker.c:784) is its whole body:
///
/// ```c
/// XMARKER (marker)->insertion_type = ! NILP (type);
/// ```
///
/// `insertion_type` is a bitfield on `struct Lisp_Marker`, and that struct IS
/// the chain node, so there is nothing else to update.  Ours is the same: the
/// `MarkerObj` the Lisp value points at is the pointer spliced into the
/// buffer's chain.  It nevertheless went on to walk every buffer's chain
/// looking for the marker by id, to write the same field on the same object
/// a second time -- 79.8ms for 5,000 calls against a chain of 8,000 markers,
/// where GNU is a flat 0.7ms.
///
/// The insert rows are what make this safe to delete: an insert exactly AT
/// the marker moves an `After` marker and leaves a `Before` one, and that
/// behaviour is driven by the bit on the CHAIN node.  If the Lisp-visible
/// marker and the chain entry were ever different objects, these rows would
/// catch it.  Pinned to GNU Emacs 31.1.
#[test]
fn set_marker_insertion_type_writes_the_bit_the_chain_reads() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
             (insert "abcdefghij")
             (let ((m (copy-marker 5))
                   (n (copy-marker 5 t))
                   out)
               (dolist (ty '(nil t nil t))
                 (set-marker-insertion-type m ty)
                 (goto-char 5)
                 (insert "X")
                 (push (list ty (marker-insertion-type m) (marker-position m)) out)
                 (delete-char -1))
               (push (list 'born-t (marker-insertion-type n) (marker-position n)) out)
               (set-marker-insertion-type n nil)
               (push (list 'now-nil (marker-insertion-type n)) out)
               (let ((d (make-marker)))
                 (set-marker-insertion-type d t)
                 (push (list 'detached (marker-insertion-type d) (marker-position d)) out))
               (nreverse out)))"#,
    );
    assert_eq!(
        observed,
        "OK ((nil nil 5) (t t 6) (nil nil 5) (t t 6) (born-t t 5) (now-nil nil) (detached t nil))"
    );
}

thread_local! {
    /// One thread-lifetime evaluator for the marker fixtures, as the
    /// pre-move helper had.  Replacing it per call dropped the previous
    /// Context (and its heap) while earlier returned Values were still
    /// live.  nextest gives each test its own process.
    ///
    /// Held through the one typed thread-exit leak wrapper: a Context
    /// dropped inside TLS destruction drops its World and TaggedHeap, whose
    /// Collector::join emits a tracing event after tracing's own TLS may
    /// already be destroyed.  Deliberate leak, bounded by one process per
    /// nextest test.
    static MARKER_TEST_CTX: RefCell<
        Option<
            neovm_value::thread_exit_leak::ThreadExitLeak<
                crate::emacs_core::eval::Context,
            >,
        >,
    > = const { RefCell::new(None) };
}

fn marker_test_ctx<R>(f: impl FnOnce(&mut crate::emacs_core::eval::Context) -> R) -> R {
    MARKER_TEST_CTX.with(|slot| {
        let mut slot = slot.borrow_mut();
        let ctx = slot.get_or_insert_with(|| {
            neovm_value::thread_exit_leak::ThreadExitLeak::new(crate::test_utils::new_context())
        });
        f(ctx.get_mut())
    })
}

fn call_marker_position(args: Vec<Value>) -> EvalResult {
    marker_test_ctx(|ctx| builtin_marker_position_in_buffers(&ctx.edit.buffers, &args))
}

fn call_marker_buffer(args: Vec<Value>) -> EvalResult {
    marker_test_ctx(|ctx| builtin_marker_buffer_in_buffers(&ctx.edit.buffers, &args))
}

fn call_set_marker_insertion_type(args: Vec<Value>) -> EvalResult {
    marker_test_ctx(|ctx| builtin_set_marker_insertion_type_in_buffers(&mut ctx.edit.buffers, args))
}

fn call_copy_marker(args: Vec<Value>) -> EvalResult {
    marker_test_ctx(|ctx| builtin_copy_marker_in_buffers(&mut ctx.edit.buffers, &args))
}
