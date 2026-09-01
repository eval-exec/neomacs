use super::*;
use crate::buffer::{EmacsByteRange, LispCharPos1};

// Test helpers keep Context alive in a thread_local so the heap
// objects in the returned Value survive until the next call.
// Previously the bare `let mut eval = ...; builtin(&mut eval, args)`
// pattern dropped the Context at end of block, destroying the
// tagged heap and leaving the returned Value pointing at freed
// memory.
use std::cell::RefCell;
thread_local! {
    static MARKER_TEST_CTX: RefCell<Option<Box<super::super::eval::Context>>> =
        const { RefCell::new(None) };
}

fn call_marker_position(args: Vec<Value>) -> EvalResult {
    MARKER_TEST_CTX.with(|slot| {
        let mut ctx = Box::new(super::super::eval::Context::new());
        let result = builtin_marker_position(&mut ctx, args);
        *slot.borrow_mut() = Some(ctx);
        result
    })
}

fn call_marker_buffer(args: Vec<Value>) -> EvalResult {
    MARKER_TEST_CTX.with(|slot| {
        let mut ctx = Box::new(super::super::eval::Context::new());
        let result = builtin_marker_buffer(&mut ctx, args);
        *slot.borrow_mut() = Some(ctx);
        result
    })
}

fn call_set_marker_insertion_type(args: Vec<Value>) -> EvalResult {
    MARKER_TEST_CTX.with(|slot| {
        let mut ctx = Box::new(super::super::eval::Context::new());
        let result = builtin_set_marker_insertion_type(&mut ctx, args);
        *slot.borrow_mut() = Some(ctx);
        result
    })
}

fn call_copy_marker(args: Vec<Value>) -> EvalResult {
    MARKER_TEST_CTX.with(|slot| {
        let mut ctx = Box::new(super::super::eval::Context::new());
        let result = builtin_copy_marker(&mut ctx, args);
        *slot.borrow_mut() = Some(ctx);
        result
    })
}

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
fn is_marker_rejects_non_markers() {
    crate::test_utils::init_test_tracing();
    assert!(!is_marker(&Value::NIL));
    assert!(!is_marker(&Value::fixnum(42)));
    assert!(!is_marker(&Value::vector(vec![Value::fixnum(1)])));
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
    let mut eval = super::super::eval::Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let m =
        make_registered_buffer_marker(&mut eval.buffers, buffer_id, LispCharPos1::new(10), false);
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
    let mut eval = super::super::eval::Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let marker = make_marker_value(Some(buffer_id), Some(LispCharPos1::new(1)), false);
    let buf = builtin_marker_buffer(&mut eval, vec![marker]).unwrap();
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
    let mut eval = super::super::eval::Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let m = make_registered_buffer_marker(&mut eval.buffers, buffer_id, LispCharPos1::new(5), true);
    let copy = builtin_copy_marker(&mut eval, vec![m]).unwrap();
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
    let mut eval = super::super::eval::Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abc");

    let before_min = builtin_copy_marker(&mut eval, vec![Value::fixnum(-10)])
        .expect("copy marker before point-min");
    assert_eq!(marker_position_value(&before_min), Value::fixnum(1));

    let after_max = builtin_copy_marker(&mut eval, vec![Value::fixnum(99)])
        .expect("copy marker after point-max");
    assert_eq!(marker_position_value(&after_max), Value::fixnum(4));
}

// `builtin_move_marker_matches_set_marker_behavior' is gone with the subr it
// called.  GNU has no DEFUN `move-marker'; `lisp/subr.el:2280' aliases it to
// `set-marker', so the behaviour is asked of the loaded runtime in
// `move_marker_is_the_set_marker_alias_like_gnu'
// (`builtins/lisp_only_predicates_and_aliases_test.rs').  DIVERGENCES.md 148.

#[test]
fn builtin_make_marker_returns_empty() {
    crate::test_utils::init_test_tracing();
    let m = builtin_make_marker(vec![]).unwrap();
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
    let mut eval = super::super::eval::Context::new();

    assert!(builtin_point_marker(&mut eval, vec![Value::NIL]).is_err());
    assert!(builtin_point_min_marker(&mut eval, vec![Value::NIL]).is_err());
    assert!(builtin_point_max_marker(&mut eval, vec![Value::NIL]).is_err());
    assert!(builtin_mark_marker(&mut eval, vec![Value::NIL]).is_err());
}

#[test]
fn numeric_comparisons_use_live_marker_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
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
        crate::emacs_core::error::format_eval_result(&Ok(result)),
        "OK (12 nil t)"
    );
}

#[test]
fn point_min_and_max_markers_follow_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("current buffer");
    let _ = eval.buffers.insert_into_buffer(buf_id, "ééz");
    let _ = eval.buffers.narrow_buffer_to_emacs_byte_range(
        buf_id,
        EmacsByteRange::from_usize('é'.len_utf8(), "ééz".len()),
    );

    let min_marker = builtin_point_min_marker(&mut eval, vec![]).expect("point-min-marker");
    let max_marker = builtin_point_max_marker(&mut eval, vec![]).expect("point-max-marker");

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
    let mut eval = super::super::eval::Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("current buffer");
    let _ = eval.buffers.insert_into_buffer(buf_id, "ééz");
    let _ = eval
        .buffers
        .set_buffer_mark_emacs_byte_pos(buf_id, crate::buffer::EmacsBytePos::new('é'.len_utf8()));

    let marker = builtin_mark_marker(&mut eval, vec![]).expect("mark-marker");
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
    let mut eval = super::super::eval::Context::new();
    // Don't set a mark; default buffer has no mark.
    let marker = builtin_mark_marker(&mut eval, vec![]).expect("mark-marker");
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
    let mut eval = super::super::eval::Context::new();
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
        crate::emacs_core::error::format_eval_result(&Ok(result)),
        r#"OK (5 "aXbc")"#
    );
}

#[test]
fn set_marker_uses_live_source_marker_position_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
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
        crate::emacs_core::error::format_eval_result(&Ok(result)),
        "OK 5"
    );
}
