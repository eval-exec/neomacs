use crate::emacs_core::eval::Context;

#[test]
fn lisp_property_churn_reuses_interval_slots() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        "(progn (set-buffer (get-buffer-create \" interval-retention\"))
                (setq buffer-undo-list t)
                (insert (make-string 32 ?x))
                (let ((i 0))
                  (while (< i 2000)
                    (set-text-properties 9 25 '(retention-probe 1))
                    (set-text-properties 1 33 nil)
                    (setq i (1+ i)))))",
    )
    .unwrap();
    let buffer = eval.buffers.current_buffer().unwrap();
    let (slots, capacity) = buffer
        .text
        .storage
        .borrow()
        .text_props
        .arena_slot_counts_for_test();
    assert!(
        slots <= 3 && capacity <= 4,
        "set-text-properties churn retained {slots} slots, capacity {capacity}"
    );
}
