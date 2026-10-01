use super::*;

#[test]
fn sequential_contexts_on_a_second_thread_reset_heap_owned_caches() {
    // Initialize the runtime on one thread before using another. A global
    // registration guard incorrectly suppresses the second thread's hooks.
    std::thread::spawn(|| {
        let mut context = Context::new();
        context.eval_str("(standard-syntax-table)").unwrap();
    })
    .join()
    .expect("first runtime thread");

    std::thread::spawn(|| {
        let mut first = Context::new();
        first.eval_str("(standard-syntax-table)").unwrap();

        // Fail before dropping/replacing the heap when hooks are missing,
        // rather than dereferencing a stale syntax object and crashing the
        // entire test process on the old implementation.
        PDUMP_RUNTIME_STATE.with(|state| {
            assert!(
                state.borrow().load_hooks.iter().any(|hook| {
                    hook_identity(*hook)
                        == hook_identity(crate::emacs_core::syntax::reset_syntax_thread_locals)
                }),
                "second runtime thread must register its own syntax reset hook"
            );
        });

        drop(first);
        let mut second = Context::new();
        let result = second
            .eval_str(
                "(progn (set-syntax-table (copy-syntax-table))
                        (syntax-table-p (syntax-table)))",
            )
            .expect("syntax operations after replacing the thread's heap");
        assert_eq!(result, Value::T);
    })
    .join()
    .expect("second runtime thread");
}
