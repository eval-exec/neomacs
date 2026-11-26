//! The redisplay range-view copy memo keeps heap Values inside the live
//! buffer's root set. A concurrent major must keep the copies reachable from
//! the start seed and from its termination rescan, and a write or a clear in
//! the middle of a cycle must publish the replacement through the same root
//! set rather than mutating one of its values in place.

use neovm_value::gc_trace::GcTrace;
use neovm_value::heap_types::byte_positions::EmacsBytePos;
use neovm_value::value::Value;

fn face() -> Value {
    Value::symbol("face")
}

fn live_roots(
    eval: &neovm_eval::emacs_core::eval::Context,
    id: neovm_edit::buffer::BufferId,
) -> Vec<Value> {
    let mut roots = Vec::new();
    eval.buffer_manager()
        .get(id)
        .expect("the overlay buffer is alive")
        .overlays()
        .trace_roots(&mut roots);
    roots
}

#[test]
fn overlay_view_memo_copies_survive_a_mid_cycle_replacement_and_clear() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str(
        "(progn
           (set-buffer (get-buffer-create \"*r3-overlay-view-gc*\"))
           (erase-buffer)
           (insert (make-string 400 ?x))
           (let ((i 0))
             (while (< i 100)
               (let ((o (make-overlay (* i 3) (+ (* i 3) 2))))
                 (overlay-put o 'face 'bold))
               (setq i (+ i 1))))
           t)",
    )
    .expect("set up the overlay buffer");
    let id = eval
        .buffer_manager()
        .current_buffer()
        .expect("a current buffer")
        .id();

    let (view, copy) = {
        let buffer = eval.buffer_manager().get(id).expect("buffer");
        let view = buffer.overlay_snapshot_in_char_span(0, 300);
        let copy = view.overlays_at_emacs_byte_pos(EmacsBytePos::new(0))[0];
        (view, copy)
    };
    assert_eq!(
        view.overlay_get_named(copy, face()),
        Some(Value::symbol("bold"))
    );

    // The memo copy is a root of the live list: an exact cycle keeps the copy
    // and the plist spine it owns.
    eval.gc_collect_exact();
    assert!(eval.tagged_heap().owns_heap_value_for_test(copy));
    assert_eq!(
        view.overlay_get_named(copy, face()),
        Some(Value::symbol("bold"))
    );

    // Start a concurrent cycle, replace the overlay's copy while it is open,
    // then finish exactly. The new memo entry is inserted into the root set
    // the termination rescan reads; the superseded entry is removed, not
    // overwritten in place.
    eval.gc_collect_from_current_roots_impl(false);
    eval.eval_str("(overlay-put (car (overlays-at 0)) 'face 'italic)")
        .expect("mutate the overlay mid-cycle");
    let (next_view, next_copy) = {
        let buffer = eval.buffer_manager().get(id).expect("buffer");
        let view = buffer.overlay_snapshot_in_char_span(0, 300);
        let copy = view.overlays_at_emacs_byte_pos(EmacsBytePos::new(0))[0];
        (view, copy)
    };
    assert_ne!(next_copy.bits(), copy.bits());
    eval.gc_collect_exact();
    assert!(eval.tagged_heap().owns_heap_value_for_test(next_copy));
    assert_eq!(
        next_view.overlay_get_named(next_copy, face()),
        Some(Value::symbol("italic"))
    );
    let roots = live_roots(&eval, id);
    assert!(
        roots.iter().any(|value| value.bits() == next_copy.bits()),
        "the replacement memo entry is not a root after the cycle"
    );
    assert!(
        !roots.iter().any(|value| value.bits() == copy.bits()),
        "a superseded memo copy is still rooted"
    );

    // Clear the whole list mid-cycle: the memo is dropped from the root set
    // without touching an observer that already holds a copy. A real
    // consumer (the layout capture) retains its copies in the evaluator's
    // root registry; root this one the same way so the collector can see it.
    eval.obarray_mut()
        .set_symbol_value("r3-observer-copy", next_copy);
    eval.gc_collect_from_current_roots_impl(false);
    eval.eval_str(
        "(let ((rest (append (car (overlay-lists)) (cdr (overlay-lists)))))
           (while rest
             (delete-overlay (car rest))
             (setq rest (cdr rest))))",
    )
    .expect("delete every overlay mid-cycle");
    let empty = {
        let buffer = eval.buffer_manager().get(id).expect("buffer");
        buffer.overlay_snapshot_in_char_span(0, 300)
    };
    assert!(
        empty
            .overlays_at_emacs_byte_pos(EmacsBytePos::new(0))
            .is_empty()
    );
    eval.gc_collect_exact();
    let roots = live_roots(&eval, id);
    assert!(
        !roots.iter().any(|value| value.bits() == next_copy.bits()),
        "clearing the list left a memo copy rooted"
    );
    // The observer view captured before the clear still answers with what it
    // captured: dropping the memo did not mutate or free the retained copy.
    assert!(eval.tagged_heap().owns_heap_value_for_test(next_copy));
    assert_eq!(
        next_view.overlay_get_named(next_copy, face()),
        Some(Value::symbol("italic"))
    );
}
