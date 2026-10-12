//! Scope teardown preserves existing traversal prefixes after Rust unwinding.
use super::super::*;
use crate::emacs_core::error::EvalResult;
use crate::emacs_core::eval::{Context, SpecBinding};
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

#[test]
fn closure_cycle_stack_guard_restores_prefix_after_rust_unwind() {
    crate::test_utils::init_test_tracing();
    let outer = PrintObjectStackGuard::enter(PrintObjectRef::Lambda(usize::MAX))
        .expect("outer test identity");
    let saved = PRINT_OBJECT_STACK.with(|stack| stack.borrow().clone());
    let result = std::panic::catch_unwind(|| {
        with_print_object_guard(
            PrintObjectRef::Lambda(usize::MAX - 1),
            |_| panic!("unexpected cycle"),
            || panic!("render unwind"),
        );
    });
    assert!(result.is_err());
    assert_eq!(
        PRINT_OBJECT_STACK.with(|stack| stack.borrow().clone()),
        saved
    );
    drop(outer);
}

#[test]
fn depth_and_traversal_guards_restore_prefix_after_rust_unwind() {
    crate::test_utils::init_test_tracing();
    let mut state = PrintState {
        options: PrintOptions::default(),
        circle: None,
        context: PrinterContext::Standalone(None),
        failure: None,
        settings_source: PrintSettingsSource::Globals,
        callback_number_table: CallbackNumberTable::Captured,
        depth: 7,
        _mutator: std::marker::PhantomData,
        default_print_stack: vec![Some(11)],
        object_stack: vec![13],
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut depth = PrintDepthGuard::enter(&mut state);
        let mut stacks = PrintTraversalStackGuard::new(depth.state());
        stacks.state().default_print_stack.push(Some(17));
        stacks.state().object_stack.push(19);
        panic!("render unwind");
    }));
    assert!(result.is_err());
    assert_eq!(state.depth, 7);
    assert_eq!(state.default_print_stack, vec![Some(11)]);
    assert_eq!(state.object_stack, vec![13]);
}

/// GNU's ordinary printer cleanup runs Lisp unlet watchers. A Rust panic must
/// recover native state without calling those watchers or seeding lazy tables.
#[test]
fn printer_evaluation_body_panic_restores_storage_without_lisp_or_table_seeding() {
    fn record_unlet(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        if args.get(2) == Some(&Value::symbol("unlet")) {
            ctx.obarray
                .set_symbol_value_id(intern("printer-panic-unlet-ran"), Value::T);
        }
        Ok(Value::NIL)
    }
    for saved_table in [None, Some(Value::fixnum(99))] {
        let mut ctx = Context::new();
        let original = ctx.buffers.current_buffer_id().unwrap();
        let other = ctx.buffers.create_buffer("printer-body-panic-other");
        let symbol = intern("printer-body-panic-binding");
        let table = intern("neovm--standard-case-table-object");
        ctx.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
        ctx.obarray
            .set_symbol_value_id(intern("printer-panic-unlet-ran"), Value::NIL);
        ctx.register_subr(SubrSpec::new(
            "printer-body-panic-watcher",
            NativeFn::ContextVec(record_unlet),
            SubrArity::new(4, Some(4)),
        ));
        let depth = ctx.specpdl.len();
        let had_root_frame = ctx.vm_frame_root_slots_checked(0, 0).is_some();
        let mut allocations = None;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scope = PrintEvaluationGuard::new(&mut ctx);
            scope.context().push_specpdl_root(Value::fixnum(17));
            scope
                .context()
                .specpdl
                .push(SpecBinding::SaveCurrentBuffer {
                    buffer_id: original,
                });
            scope
                .context()
                .set_current_buffer_unrecorded(other)
                .unwrap();
            scope
                .context()
                .try_specbind(symbol, Value::fixnum(20))
                .unwrap();
            crate::emacs_core::advice::builtin_add_variable_watcher(
                scope.context(),
                vec![
                    Value::from_sym_id(symbol),
                    Value::symbol("printer-body-panic-watcher"),
                ],
            )
            .unwrap();
            match saved_table {
                Some(value) => scope.context().obarray.set_symbol_value_id(table, value),
                None => scope.context().obarray.makunbound_id(table),
            }
            allocations = Some(scope.context().tagged_heap.allocated_count());
            panic!("contained Rust panic in printer body");
        }));
        assert!(caught.is_err());
        assert_eq!(ctx.specpdl.len(), depth);
        assert_eq!(
            ctx.vm_frame_root_slots_checked(0, 0).is_some(),
            had_root_frame
        );
        assert_eq!(
            ctx.obarray.symbol_value_id_or_nil(symbol),
            Value::fixnum(10)
        );
        assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
        assert_eq!(
            ctx.threads
                .thread_current_buffer(ctx.threads.current_thread_id()),
            Some(original)
        );
        assert_eq!(
            ctx.obarray
                .symbol_value_id_or_nil(intern("printer-panic-unlet-ran")),
            Value::NIL
        );
        assert_eq!(ctx.obarray.symbol_value_id_copied(table), saved_table);
        assert_eq!(Some(ctx.tagged_heap.allocated_count()), allocations);
    }
}

/// Keep the outer saved depth armed while GNU's explicit unlet callback runs:
/// the popped Let and the remaining saved-buffer/root prefix have separate
/// recovery owners if the callback unwinds through Rust.
#[test]
fn printer_evaluation_finish_watcher_panic_restores_remaining_scope() {
    fn panic_unlet(_: &mut Context, args: Vec<Value>) -> EvalResult {
        if args.get(2) == Some(&Value::symbol("unlet")) {
            panic!("contained Rust panic in printer cleanup watcher");
        }
        Ok(Value::NIL)
    }
    for finish_value in [false, true] {
        let mut ctx = Context::new();
        let original = ctx.buffers.current_buffer_id().unwrap();
        let other = ctx.buffers.create_buffer("printer-finish-panic-other");
        let symbol = intern("printer-finish-panic-binding");
        ctx.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
        ctx.register_subr(SubrSpec::new(
            "printer-finish-panic-watcher",
            NativeFn::ContextVec(panic_unlet),
            SubrArity::new(4, Some(4)),
        ));
        let depth = ctx.specpdl.len();
        let had_root_frame = ctx.vm_frame_root_slots_checked(0, 0).is_some();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scope = PrintEvaluationGuard::new(&mut ctx);
            scope.context().push_specpdl_root(Value::fixnum(17));
            scope
                .context()
                .specpdl
                .push(SpecBinding::SaveCurrentBuffer {
                    buffer_id: original,
                });
            scope
                .context()
                .set_current_buffer_unrecorded(other)
                .unwrap();
            scope
                .context()
                .try_specbind(symbol, Value::fixnum(20))
                .unwrap();
            crate::emacs_core::advice::builtin_add_variable_watcher(
                scope.context(),
                vec![
                    Value::from_sym_id(symbol),
                    Value::symbol("printer-finish-panic-watcher"),
                ],
            )
            .unwrap();
            if finish_value {
                let _ = scope.finish_value(Ok(Value::NIL));
            } else {
                let _ = scope.finish(Ok(()));
            }
        }));
        assert!(caught.is_err());
        assert_eq!(
            ctx.obarray.symbol_value_id_or_nil(symbol),
            Value::fixnum(10)
        );
        assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
        assert_eq!(ctx.specpdl.len(), depth);
        assert_eq!(
            ctx.vm_frame_root_slots_checked(0, 0).is_some(),
            had_root_frame
        );
        assert!(!ctx.active_variable_watchers.contains(&symbol));
    }
}

#[test]
fn printer_settings_buffer_panic_preserves_absent_or_invalid_standard_table() {
    for saved_table in [None, Some(Value::fixnum(99))] {
        let mut ctx = Context::new();
        let original = ctx.buffers.current_buffer_id().unwrap();
        let other = ctx.buffers.create_buffer("printer-settings-panic-other");
        let table = intern("neovm--standard-case-table-object");
        let mut allocations = None;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scope = PrintSettingsBufferGuard::new(&mut ctx);
            scope.select(other).unwrap();
            match saved_table {
                Some(value) => scope.context.obarray.set_symbol_value_id(table, value),
                None => scope.context.obarray.makunbound_id(table),
            }
            allocations = Some(scope.context.tagged_heap.allocated_count());
            panic!("contained Rust panic in temporary printer selection");
        }));
        assert!(caught.is_err());
        assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
        assert_eq!(
            ctx.threads
                .thread_current_buffer(ctx.threads.current_thread_id()),
            Some(original)
        );
        assert_eq!(ctx.obarray.symbol_value_id_copied(table), saved_table);
        assert_eq!(Some(ctx.tagged_heap.allocated_count()), allocations);
    }
}

/// Ordinary Lisp errors still use normal runtime restoration, which retains
/// the existing lazy standard-table initialization on a real buffer switch.
#[test]
fn printer_settings_buffer_finish_restores_runtime_on_flow_error() {
    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().unwrap();
    let other = ctx.buffers.create_buffer("printer-settings-flow-other");
    let table = intern("neovm--standard-case-table-object");
    let mut scope = PrintSettingsBufferGuard::new(&mut ctx);
    scope.select(other).unwrap();
    scope.context.obarray.makunbound_id(table);
    let result = scope.finish(Err::<(), _>(crate::emacs_core::error::signal(
        "error",
        vec![],
    )));
    assert!(result.is_err());
    assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
    assert_eq!(
        ctx.threads
            .thread_current_buffer(ctx.threads.current_thread_id()),
        Some(original)
    );
    assert!(ctx.obarray.symbol_value_id_copied(table).is_some());
}
