//! Temporary printer selection owns no dynamic bindings; the outer scope does.
use super::*;

#[test]
fn print_binding_selection_finish_keeps_binding_for_outer_unwind() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let origin = ctx.buffers.current_buffer_id().unwrap();
    let target = ctx.buffers.create_buffer("print-binding-target");
    let name = intern("print-binding-scope-test-value");
    ctx.obarray.set_symbol_value_id(name, Value::fixnum(10));
    let depth = ctx.specpdl.len();
    let mut scope = PrintBindingBufferGuard::new(&mut ctx);
    scope.select(target).unwrap();
    scope
        .context()
        .try_specbind(name, Value::fixnum(20))
        .unwrap();
    scope.finish(Ok(())).unwrap();
    assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(name), Value::fixnum(20));
    assert!(ctx.specpdl.len() > depth);
    ctx.unbind_to(depth);
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(name), Value::fixnum(10));
}

#[test]
fn print_binding_selection_finish_restores_buffer_on_lisp_error() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let origin = ctx.buffers.current_buffer_id().unwrap();
    let target = ctx.buffers.create_buffer("print-binding-error-target");
    let mut scope = PrintBindingBufferGuard::new(&mut ctx);
    scope.select(target).unwrap();
    let error = signal(LispCondition::Error, vec![]);
    assert!(scope.finish(Err(error)).is_err());
    assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
}

#[test]
fn print_binding_selection_panic_restores_buffer_without_seeding_case_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let origin = ctx.buffers.current_buffer_id().unwrap();
    let target = ctx.buffers.create_buffer("print-binding-panic-target");
    let table_symbol = intern("neovm--standard-case-table-object");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = PrintBindingBufferGuard::new(&mut ctx);
        scope.select(target).unwrap();
        scope.context().obarray.makunbound_id(table_symbol);
        panic!("temporary printer buffer selection");
    }));
    assert!(outcome.is_err());
    assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
    assert!(ctx.obarray.symbol_value_id_copied(table_symbol).is_none());
}

/// The target helper retains one real forwarded Let for the outer printer
/// scope, for both buffer encodings and both direct destination shapes.
#[test]
fn print_target_escape_cached_forwarded_bind_survives_selection_restore() {
    use crate::buffer::LispCharPos1;
    use crate::emacs_core::eval::{
        VarCacheEvent, VarCacheTier, reset_var_cache_events, set_var_cache_tiers_for_test,
        var_cache_event_count,
    };
    use crate::emacs_core::marker::make_registered_buffer_marker;

    for multibyte in [false, true] {
        for marker in [false, true] {
            crate::test_utils::init_test_tracing();
            set_var_cache_tiers_for_test(&VarCacheTier::ALL);
            let mut ctx = crate::emacs_core::eval::Context::new();
            let origin = ctx.buffers.current_buffer_id().unwrap();
            let target_buffer = ctx.buffers.create_buffer("cached-print-target");
            ctx.buffers
                .set_buffer_multibyte_flag(target_buffer, multibyte)
                .unwrap();
            for name in ["print-escape-nonascii", "print-escape-multibyte"] {
                ctx.obarray.set_symbol_value_id(intern(name), Value::NIL);
            }
            let escape = intern(if multibyte {
                "print-escape-nonascii"
            } else {
                "print-escape-multibyte"
            });
            let target = if marker {
                make_registered_buffer_marker(
                    &mut ctx.buffers,
                    target_buffer,
                    LispCharPos1::new(1),
                    false,
                )
            } else {
                Value::make_buffer(target_buffer)
            };
            let depth = ctx.specpdl.len();
            let mut scope = super::super::print::PrintEvaluationGuard::new(&mut ctx);
            reset_var_cache_events();
            apply_print_target_escape_bindings(scope.context(), target).unwrap();
            {
                let ctx = scope.context();
                assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
                assert_eq!(ctx.specpdl.len(), depth + 1);
                assert!(
                    matches!(ctx.specpdl[depth], crate::emacs_core::eval::SpecBinding::Let { sym_id, .. } if sym_id == escape)
                );
                assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::T);
                assert_eq!(var_cache_event_count(VarCacheEvent::BindForwarded), 1);
            }
            scope.finish(Ok(())).unwrap();
            assert_eq!(ctx.specpdl.len(), depth);
            assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
            assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
        }
    }
}

/// Preserve the native helper's existing target-let/caller-unlet selection
/// behavior on watched refusal. This does not assert GNU's later scratch or
/// stream-buffer selection lifetime; no buffer guard is removed by the patch.
#[test]
fn print_target_escape_watched_refusal_preserves_binding_and_buffer_cleanup() {
    use crate::emacs_core::eval::{Context, VarCacheTier, set_var_cache_tiers_for_test};
    use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
    fn watcher(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        let seen = Value::make_buffer(ctx.buffers.current_buffer_id().unwrap());
        if args.get(2) == Some(&Value::symbol("let")) {
            ctx.obarray
                .set_symbol_value_id(intern("target-let-buffer"), seen);
            let other = ctx
                .obarray
                .symbol_value_id_or_nil(intern("target-watch-other"))
                .as_buffer_id()
                .unwrap();
            ctx.set_current_buffer_unrecorded(other)?;
        } else if args.get(2) == Some(&Value::symbol("unlet")) {
            ctx.obarray
                .set_symbol_value_id(intern("target-unlet-buffer"), seen);
        }
        Ok(Value::NIL)
    }
    crate::test_utils::init_test_tracing();
    set_var_cache_tiers_for_test(&VarCacheTier::ALL);
    let mut ctx = Context::new();
    let origin = ctx.buffers.current_buffer_id().unwrap();
    let target_buffer = ctx.buffers.create_buffer("watched-print-target");
    let other = ctx.buffers.create_buffer("watched-print-other");
    let escape = intern("print-escape-nonascii");
    ctx.obarray.set_symbol_value_id(escape, Value::NIL);
    ctx.obarray
        .set_symbol_value_id(intern("target-watch-other"), Value::make_buffer(other));
    ctx.register_subr(SubrSpec::new(
        "target-escape-watcher",
        NativeFn::ContextVec(watcher),
        SubrArity::new(4, Some(4)),
    ));
    crate::emacs_core::advice::builtin_add_variable_watcher(
        &mut ctx,
        vec![
            Value::from_sym_id(escape),
            Value::symbol("target-escape-watcher"),
        ],
    )
    .unwrap();
    let depth = ctx.specpdl.len();
    let mut scope = super::super::print::PrintEvaluationGuard::new(&mut ctx);
    apply_print_target_escape_bindings(scope.context(), Value::make_buffer(target_buffer)).unwrap();
    {
        let ctx = scope.context();
        assert_eq!(
            ctx.specpdl.len(),
            depth + 1,
            "cache refusal appended no extra binding"
        );
        assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
        assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::T);
        assert_eq!(
            ctx.obarray
                .symbol_value_id_or_nil(intern("target-let-buffer")),
            Value::make_buffer(target_buffer)
        );
    }
    scope.finish(Ok(())).unwrap();
    assert_eq!(ctx.specpdl.len(), depth);
    assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
    assert_eq!(
        ctx.obarray
            .symbol_value_id_or_nil(intern("target-unlet-buffer")),
        Value::make_buffer(origin)
    );
}
