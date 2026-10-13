use super::*;

fn spilled_aux(mark_base: usize) -> InterpreterFrameAux {
    // Handler::Condition is currently zero-sized; its SmallVec cannot spill.
    // The binding marks force a real spilled allocation in every such state.
    InterpreterFrameAux::new(
        std::iter::repeat_n(Handler::Condition, 12).collect(),
        (mark_base..mark_base + 24).collect(),
    )
}

fn push_sparse_owner(
    stack: &mut InterpreterFrameAuxStack,
    depth: InterpreterDriverDepth,
    mark_base: usize,
) {
    *stack.current_mut() = spilled_aux(mark_base);
    stack.suspend_current(depth);
}

#[test]
fn aux_stack_drop_layout_is_transparent_and_thread_confined() {
    static_assertions::assert_not_impl_any!(InterpreterFrameAuxStack: Send, Sync, Clone);
    static_assertions::assert_impl_all!(InterpreterFrameAuxStack: std::fmt::Debug);
    static_assertions::const_assert!(std::mem::needs_drop::<InterpreterFrameAuxStack>());
    static_assertions::assert_eq_size!(
        std::mem::ManuallyDrop<InterpreterFrameAux>,
        InterpreterFrameAux
    );
    static_assertions::assert_eq_size!(
        std::mem::ManuallyDrop<Vec<SuspendedInterpreterFrameAux>>,
        Vec<SuspendedInterpreterFrameAux>
    );
}

#[test]
fn aux_stack_entry_transfer_keeps_spilled_storage_and_pool_allocation() {
    let entry = spilled_aux(100);
    let binding_storage = entry.bind_stack.as_ptr();
    let mut stack = InterpreterFrameAuxStack::with_suspended(
        entry.handlers,
        entry.bind_stack,
        Vec::with_capacity(16),
    );
    let suspended_storage = stack.suspended.as_ptr();
    let suspended_capacity = stack.suspended.capacity();

    let (handlers, binds) = stack.take_entry();
    assert_eq!(binds.as_ptr(), binding_storage);
    assert_eq!(handlers.len(), 12);
    assert!(
        handlers
            .iter()
            .all(|handler| matches!(handler, Handler::Condition))
    );
    assert_eq!(binds.as_slice(), &(100..124).collect::<Vec<_>>());
    assert!(stack.current.is_empty());
    assert!(!stack.current.handlers.spilled());
    assert_eq!(stack.current.bind_stack.capacity(), 8);

    let suspended = stack.into_suspended();
    assert!(suspended.is_empty());
    assert_eq!(suspended.as_ptr(), suspended_storage);
    assert_eq!(suspended.capacity(), suspended_capacity);
    // The extracted entry remains independently owned after the container is
    // consumed. Its spilled allocations were not retired with the container.
    assert_eq!(handlers.len(), 12);
    assert_eq!(binds.as_slice().last(), Some(&123));

    let mut pool = InterpreterStackPool::new();
    pool.give_back(InterpreterStacks {
        frames: Vec::new(),
        suspended,
    });
    let reused = pool.take();
    assert!(reused.suspended.is_empty());
    assert_eq!(reused.suspended.as_ptr(), suspended_storage);
    assert_eq!(reused.suspended.capacity(), suspended_capacity);
}

#[test]
fn aux_stack_consuming_transfer_preserves_nonempty_sparse_owners() {
    let entry = spilled_aux(1_000);
    let mut stack = InterpreterFrameAuxStack::new(entry.handlers, entry.bind_stack);
    stack.suspend_current(InterpreterDriverDepth::ROOT);
    push_sparse_owner(
        &mut stack,
        InterpreterDriverDepth::from_suspended_callers(3),
        2_000,
    );
    *stack.current_mut() = spilled_aux(3_000);
    let storage = stack.suspended.as_ptr();
    let capacity = stack.suspended.capacity();

    // Exercise the private consuming method with a populated current state,
    // not just run_loop's usual fresh-empty take_entry successor.
    let suspended = stack.into_suspended();
    assert_eq!(suspended.as_ptr(), storage);
    assert_eq!(suspended.capacity(), capacity);
    assert_eq!(suspended.len(), 2);
    assert_eq!(suspended[0].depth, InterpreterDriverDepth::ROOT);
    assert_eq!(suspended[0].state.handlers.len(), 12);
    assert_eq!(suspended[0].state.bind_stack.as_slice()[0], 1_000);
    assert_eq!(suspended[1].state.bind_stack.as_slice()[0], 2_000);
}

#[test]
fn aux_stack_sparse_restore_retires_old_spilled_current_state() {
    let entry = spilled_aux(10);
    let mut stack = InterpreterFrameAuxStack::new(entry.handlers, entry.bind_stack);
    stack.suspend_current(InterpreterDriverDepth::ROOT);
    // Empty ordinary callers do not materialize sparse records.
    stack.suspend_current(InterpreterDriverDepth::from_suspended_callers(1));
    assert_eq!(stack.materialized_suspended_len(), 1);
    push_sparse_owner(
        &mut stack,
        InterpreterDriverDepth::from_suspended_callers(2),
        100,
    );
    *stack.current_mut() = spilled_aux(200);
    stack.restore_current(InterpreterDriverDepth::from_suspended_callers(2));
    assert_eq!(stack.current.bind_stack.as_slice()[0], 100);
    assert_eq!(stack.current.handlers.len(), 12);
    assert_eq!(stack.materialized_suspended_len(), 1);

    // The gap has no saved state; it retires the just-restored populated state.
    stack.restore_current(InterpreterDriverDepth::from_suspended_callers(1));
    assert!(stack.current.is_empty());
    stack.restore_current(InterpreterDriverDepth::ROOT);
    assert_eq!(stack.current.bind_stack.as_slice()[0], 10);
    assert!(stack.suspended.is_empty());
    let (handlers, binds) = stack.take_entry();
    let pooled = stack.into_suspended();
    assert!(pooled.is_empty());
    assert_eq!(handlers.len(), 12);
    assert_eq!(binds.as_slice()[0], 10);
}

#[test]
fn aux_stack_drop_handles_spilled_current_and_sparse_owners() {
    let entry = spilled_aux(1);
    let mut stack = InterpreterFrameAuxStack::new(entry.handlers, entry.bind_stack);
    for depth in 0..12 {
        push_sparse_owner(
            &mut stack,
            InterpreterDriverDepth::from_suspended_callers(depth),
            depth * 100,
        );
    }
    *stack.current_mut() = spilled_aux(2_000);
    assert_eq!(stack.materialized_suspended_len(), 12);
    assert_eq!(stack.current.handlers.len(), 12);
    assert!(stack.current.bind_stack.capacity() > 8);
    // Both owned field collections are retired by the explicit Drop boundary.
    // Their elements have no arbitrary user destructor or callback.
    drop(stack);
}

#[test]
fn aux_stack_panic_retires_spilled_owners_and_allows_a_fresh_entry() {
    crate::test_utils::init_test_tracing();
    let result = std::panic::catch_unwind(|| {
        let entry = spilled_aux(1);
        let mut stack = InterpreterFrameAuxStack::new(entry.handlers, entry.bind_stack);
        for depth in 0..12 {
            push_sparse_owner(
                &mut stack,
                InterpreterDriverDepth::from_suspended_callers(depth),
                depth * 100,
            );
        }
        *stack.current_mut() = spilled_aux(2_000);
        panic!("auxiliary-owner retirement control");
    });
    assert!(result.is_err());

    let entry = spilled_aux(3_000);
    let mut next = InterpreterFrameAuxStack::new(entry.handlers, entry.bind_stack);
    let (handlers, binds) = next.take_entry();
    assert!(next.into_suspended().is_empty());
    assert_eq!(handlers.len(), 12);
    assert_eq!(binds.as_slice()[0], 3_000);
}
