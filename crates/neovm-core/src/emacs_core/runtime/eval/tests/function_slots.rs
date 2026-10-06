//! GNU 31.1 alloc.c:Fmake_byte_code, bytecode.c:setup_frame and
//! eval.c:Ffunc_arity preserve the original slots and decode arity exactly.
use crate::emacs_core::{eval::Context, print::print_value};

fn answer(source: &str) -> String {
    let mut context = Context::new();
    print_value(
        &context
            .eval_str(source)
            .expect("GNU-backed form must succeed"),
    )
}

#[test]
fn p6_negative_argument_descriptor_constructs_without_allocating() {
    assert_eq!(
        answer(r#"(progn (make-byte-code -1 "\300\207" [nil] 1) 'made)"#),
        "made"
    );
}

#[test]
fn p6_large_argument_descriptor_preserves_full_signed_arity() {
    assert_eq!(
        answer(r#"(func-arity (make-byte-code (ash 1 40) "\300\207" [nil] 1))"#),
        "(0 . 4294967296)"
    );
}

#[test]
fn p6_argument_descriptor_checks_original_bounds_before_execution() {
    assert_eq!(
        answer(r#"(func-arity (make-byte-code 383 "\300\207" [42] 1))"#),
        "(127 . 1)"
    );
    assert_eq!(
        answer(
            r#"(condition-case err (funcall (make-byte-code 1 "\300\207" [1] 1) 3) (error err))"#
        ),
        "(wrong-number-of-arguments (1 . 0) 1)"
    );
}

#[test]
fn p6_nonsymbol_function_parameters_signal_invalid_function_on_call() {
    assert_eq!(
        answer(
            r#"(condition-case err (funcall (make-byte-code '(1 2) "\300\207" [42] 1) 1 2) (error (car err)))"#
        ),
        "invalid-function"
    );
}

#[test]
fn p6_stack_depth_preserves_large_original_slot_and_executes() {
    assert_eq!(
        answer(r#"(aref (make-byte-code 0 "\300\207" [1] 65537) 3)"#),
        "65537"
    );
    assert_eq!(
        answer(r#"(funcall (make-byte-code 0 "\300\207" [42] 65536))"#),
        "42"
    );
}

#[test]
fn p6_reader_accepts_negative_argument_descriptor() {
    assert_eq!(
        answer(r##"(byte-code-function-p (car (read-from-string "#[-1 \"\\300\\207\" [] 1]")))"##),
        "t"
    );
}

#[test]
fn p6_dynamic_formals_root_detached_cursor_through_watcher_gc() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let collections_before = context.gc_count;
    let result = context
        .eval_str(
            r#"(progn
                 (setq p6-watcher-formals
                       (list 'p6-watcher-a 'p6-watcher-b 'p6-watcher-before))
                 (setq p6-watcher-a 100 p6-watcher-b 200
                       p6-watcher-before 300 p6-watcher-after 400)
                 (fset 'p6-watcher-detach-and-collect
                       (lambda (_symbol _value operation _where)
                         (if (eq operation 'let)
                             (progn
                               (setcar (cdr (cdr p6-watcher-formals))
                                       'p6-watcher-after)
                               (setcdr p6-watcher-formals nil)
                               (garbage-collect))
                           nil)))
                 (add-variable-watcher 'p6-watcher-b
                                       'p6-watcher-detach-and-collect)
                 (unwind-protect
                     (list
                      (funcall
                       (make-byte-code p6-watcher-formals
                                       (unibyte-string 10 135)
                                       [p6-watcher-a p6-watcher-b p6-watcher-after]
                                       1)
                       11 22 33)
                      p6-watcher-formals)
                   (remove-variable-watcher 'p6-watcher-b
                                            'p6-watcher-detach-and-collect)))"#,
        )
        .unwrap();
    // GNU eval.c:3397,3433-3441 reads the next CDR after each specbind.
    // The watcher detaches the active b->after suffix from the function's
    // arglist before collecting: only the formal cursor's scratch root holds
    // that suffix through the callback. Binding after the callback must read
    // the mutated third name. Bvarref2/Breturn (bytecode.c:84,209,627-645,892)
    // return that argument through its new formal name.
    assert_eq!(print_value(&result), "(33 (p6-watcher-a))");
    assert!(context.gc_count > collections_before);
}
