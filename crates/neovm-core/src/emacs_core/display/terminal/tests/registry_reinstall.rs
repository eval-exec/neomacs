use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::pdump::clone_active_evaluator;

fn terminal_list_ids() -> Vec<u64> {
    let terminals = builtin_terminal_list(vec![]).expect("terminal-list");
    list_to_vec(&terminals)
        .expect("terminal-list is a list")
        .iter()
        .map(|handle| terminal_handle_id(handle).expect("registered terminal handle"))
        .collect()
}

fn rebuild_native_terminal_manager() {
    // A different Context's pdump restore replaces the native manager without
    // clearing this Context's registry. Isolate that native reset here; calling
    // reset_terminal_thread_locals in the active heap would clear its registry.
    TERMINAL_MANAGER.with(|state| {
        *state
            .get()
            .expect("initialized terminal manager")
            .borrow_mut() = TerminalManager::new();
    });
}

fn assert_deleted_terminal(ctx: &mut Context, deleted: Value) {
    assert_eq!(terminal_list_ids(), vec![TERMINAL_ID]);
    assert!(
        builtin_terminal_live_p(ctx, vec![deleted])
            .expect("terminal-live-p accepts a deleted handle")
            .is_nil()
    );
    assert!(
        builtin_frame_initial_p(ctx, vec![deleted])
            .expect("frame-initial-p accepts a deleted handle")
            .is_nil()
    );
    match builtin_delete_terminal(ctx, vec![terminal_handle_value()]) {
        Err(Flow::Signal(condition)) => {
            assert_eq!(condition.symbol_name(), "error");
            assert_eq!(
                condition.data,
                vec![Value::string(
                    "Attempt to delete the sole active display terminal"
                )]
            );
        }
        result => panic!("deleting the sole remaining terminal must signal, got {result:?}"),
    }
}

#[test]
fn deleted_terminal_stays_dead_after_context_migration() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut ctx = Context::new();
    let deleted =
        ensure_terminal_runtime_owner(1, "secondary-terminal", TerminalRuntimeConfig::inactive());
    let key = Value::symbol("terminal-reinstall-live-parameter");
    let payload = Value::vector(vec![Value::fixnum(42)]);
    builtin_set_terminal_parameter(&mut ctx, vec![terminal_handle_value(), key, payload])
        .expect("set the live terminal's parameter");
    builtin_delete_terminal(&mut ctx, vec![deleted]).expect("delete the secondary terminal");
    crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);

    let mut ctx = std::thread::spawn(move || {
        ctx.setup_thread_locals();
        assert_deleted_terminal(&mut ctx, deleted);
        assert_eq!(
            builtin_terminal_parameter(&mut ctx, vec![terminal_handle_value(), key])
                .expect("live terminal parameters survive migration")
                .bits(),
            payload.bits()
        );
        crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);
        ctx
    })
    .join()
    .expect("migrated Context keeps deleted terminals dead");

    ctx.setup_thread_locals();
    assert_deleted_terminal(&mut ctx, deleted);
}

#[test]
fn deleted_terminal_stays_dead_after_another_contexts_pdump_restore() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut original = Context::new();
    let deleted =
        ensure_terminal_runtime_owner(1, "secondary-terminal", TerminalRuntimeConfig::inactive());
    builtin_delete_terminal(&mut original, vec![deleted]).expect("delete secondary terminal");

    let mut other = Context::new();
    let restored = clone_active_evaluator(&mut other).expect("restore another Context's pdump");
    original.setup_thread_locals();
    assert_deleted_terminal(&mut original, deleted);
    drop(restored);
    drop(other);
}

#[test]
fn deleted_initial_terminal_stays_dead_after_native_manager_reset() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut ctx = Context::new();
    let deleted = terminal_handle_value();
    builtin_delete_terminal(&mut ctx, vec![deleted, Value::T])
        .expect("force deletion of the initial terminal");
    rebuild_native_terminal_manager();
    ctx.setup_thread_locals();
    assert!(terminal_list_ids().is_empty());
    assert!(
        builtin_terminal_live_p(&mut ctx, vec![deleted])
            .expect("deleted terminal-live-p")
            .is_nil()
    );
    assert!(live_terminal_ids_in_keyboard_poll_order().is_empty());
    assert!(terminal_handle_value().is_nil());
    assert!(terminal_handle_value_for_id(TERMINAL_ID).is_none());
    reset_terminal_handle();
    ctx.setup_thread_locals();
    assert!(terminal_list_ids().is_empty());
}

#[test]
fn terminal_creation_order_survives_native_manager_rebuild_and_migration() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut ctx = Context::new();
    // GNU prepends native terminals, then Fterminal_list conses them again:
    // Lisp sees oldest first, whereas the keyboard poll walks newest first.
    // Nonmonotonic ids also distinguish creation order from numeric sorting.
    let mut creation_order = vec![TERMINAL_ID];
    for index in 0..32 {
        let id = 1 + ((index * 17) % 32);
        ensure_terminal_runtime_owner(
            id,
            format!("ordered-terminal-{id}"),
            TerminalRuntimeConfig::inactive(),
        );
        creation_order.push(id);
    }
    let poll_order = creation_order.iter().rev().copied().collect::<Vec<_>>();
    assert_eq!(terminal_list_ids(), creation_order);
    assert_eq!(live_terminal_ids_in_keyboard_poll_order(), poll_order);

    for _ in 0..3 {
        rebuild_native_terminal_manager();
        ctx.setup_thread_locals();
        assert_eq!(terminal_list_ids(), creation_order);
        assert_eq!(live_terminal_ids_in_keyboard_poll_order(), poll_order);
    }
    crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);
    let mut ctx = std::thread::spawn(move || {
        ctx.setup_thread_locals();
        assert_eq!(terminal_list_ids(), creation_order);
        assert_eq!(live_terminal_ids_in_keyboard_poll_order(), poll_order);
        crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);
        ctx
    })
    .join()
    .expect("terminal order survives Context migration");
    ctx.setup_thread_locals();
}
