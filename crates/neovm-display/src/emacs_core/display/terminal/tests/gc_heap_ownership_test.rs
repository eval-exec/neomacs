use super::*;

/// Runtime-startup comparisons keep two evaluators alive on one thread.
/// A terminal cached by the second one is not a root of the first heap.
#[test]
fn terminal_gc_roots_exclude_another_live_heap() {
    let collecting = Context::new();
    let other = Context::new();

    let mut roots = Vec::new();
    collect_terminal_gc_roots(&mut roots, collecting.tagged_heap.identity());
    assert!(
        roots
            .iter()
            .all(|root| collecting.tagged_heap.owns_heap_value_for_test(*root)),
        "the terminal root walk retained another evaluator's handle"
    );
    let mut active_roots = Vec::new();
    collect_terminal_gc_roots(&mut active_roots, other.tagged_heap.identity());
    let active_handle = builtin_selected_terminal(vec![]).expect("selected terminal");
    assert!(active_roots.contains(&active_handle));
}

/// Switching the allocation heap must refresh handles before returning
/// them, even while the evaluator that supplied the old handle is alive.
#[test]
fn terminal_gc_handle_refreshes_when_the_active_heap_changes() {
    let mut first = Context::new();
    let lifecycle = Rc::new(RefCell::new(Vec::new()));
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    set_terminal_host(Box::new(RecordingTerminalHost {
        log: Rc::clone(&lifecycle),
    }));
    let _other = Context::new();
    first.setup_thread_locals();

    let handle = builtin_selected_terminal(vec![]).expect("selected terminal");
    assert!(
        first.tagged_heap.owns_heap_value_for_test(handle),
        "selected-terminal returned a handle allocated by another evaluator"
    );
    assert_eq!(terminal_runtime_color_cells(), 256);
    builtin_suspend_tty(&mut first, vec![]).expect("suspend preserved terminal host");
    assert_eq!(*lifecycle.borrow(), ["suspend"]);
}

/// Fresh evaluators preserve physical terminal configuration, but Lisp
/// parameter objects from a dropped heap must not survive that transition.
#[test]
fn terminal_gc_parameters_are_cleared_after_their_heap_is_dropped() {
    {
        let mut first = Context::new();
        let key = Value::symbol("terminal-gc-owned-parameter");
        let payload = Value::vector(vec![Value::fixnum(42)]);
        builtin_set_terminal_parameter(&mut first, vec![Value::NIL, key, payload])
            .expect("set terminal parameter");
    }

    let mut next = Context::new();
    let key = Value::symbol("terminal-gc-owned-parameter");
    assert!(
        builtin_terminal_parameter(&mut next, vec![Value::NIL, key])
            .expect("terminal parameter")
            .is_nil(),
        "terminal-parameter returned an object from a dropped heap"
    );
}
