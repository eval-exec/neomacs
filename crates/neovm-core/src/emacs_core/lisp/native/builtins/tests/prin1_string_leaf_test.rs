//! Native invariants for the helper's existing scope and buffer behavior.
//! GNU print.c:175-177, 814-841 supports real binding and unlet-before-buffer
//! restoration; this helper does not select GNU's private printing buffer.
use super::*;
use crate::emacs_core::eval::{Context, SpecBinding, VarCacheTier, set_var_cache_tiers_for_test};
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

fn leaf_context() -> Context {
    crate::test_utils::init_test_tracing();
    set_var_cache_tiers_for_test(&VarCacheTier::ALL);
    let mut ctx = Context::new();
    ctx.obarray
        .set_symbol_value_id(intern("print-circle"), Value::NIL);
    ctx.obarray
        .set_symbol_value_id(intern("print-escape-nonascii"), Value::NIL);
    ctx
}

/// This fixture establishes ALL before changing it. Restore that known state
/// even on panic; the existing TLS test API has no read/reset operation.
struct CacheOffGuard(std::marker::PhantomData<std::rc::Rc<()>>);
impl CacheOffGuard {
    fn new() -> Self {
        set_var_cache_tiers_for_test(&[]);
        Self(std::marker::PhantomData)
    }
}
static_assertions::assert_not_impl_any!(CacheOffGuard: Send, Sync);
impl Drop for CacheOffGuard {
    fn drop(&mut self) {
        set_var_cache_tiers_for_test(&VarCacheTier::ALL);
    }
}

#[test]
fn prin1_string_leaf_binds_default_bool_without_saved_buffer_and_restores_it() {
    let mut ctx = leaf_context();
    let escape = intern("print-escape-nonascii");
    assert!(ctx.obarray.bool_forwarder(escape).is_some());
    let origin = ctx.buffers.current_buffer_id();
    let depth = ctx.specpdl.len();
    let mut scope = super::super::print::PrintEvaluationGuard::new(&mut ctx);
    let prepared = prepare_prin1_string(
        scope.context(),
        Value::string("leaf"),
        ObjectEscaping::Escaped,
        Value::NIL,
    )
    .unwrap();
    assert!(matches!(
        prepared,
        Prin1StringPreparation::CallbackFreeLeaf { .. }
    ));
    {
        let ctx = scope.context();
        assert_eq!(ctx.specpdl.len(), depth + 1);
        assert!(matches!(ctx.specpdl[depth], SpecBinding::Let { sym_id, .. } if sym_id == escape));
        assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::T);
    }
    scope.finish(Ok(())).unwrap();
    assert_eq!(ctx.specpdl.len(), depth);
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
    assert_eq!(ctx.buffers.current_buffer_id(), origin);
}

#[test]
fn prin1_string_leaf_keeps_unibyte_escaping_and_number_table_reset() {
    let mut ctx = leaf_context();
    let depth = ctx.specpdl.len();
    let table = intern("print-number-table");
    ctx.obarray.set_symbol_value_id(table, Value::fixnum(99));
    let value = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xe9]));
    let result = prin1_to_lisp_string_value_in_state_with_overrides(
        &mut ctx,
        &value,
        ObjectEscaping::Escaped,
        None,
    )
    .unwrap();
    // GNU (prin1-to-string (unibyte-string 233)) => "\"\\351\"".
    assert_eq!(result.as_bytes(), b"\"\\351\"");
    assert!(!result.is_multibyte());
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(table), Value::NIL);
    assert_eq!(
        ctx.obarray
            .symbol_value_id_or_nil(intern("print-escape-nonascii")),
        Value::NIL
    );
    assert_eq!(ctx.specpdl.len(), depth);
}

#[test]
fn prin1_string_cache_off_retains_saved_buffer_and_real_binding() {
    let mut ctx = leaf_context();
    let _cache_off = CacheOffGuard::new();
    let escape = intern("print-escape-nonascii");
    let depth = ctx.specpdl.len();
    let mut scope = super::super::print::PrintEvaluationGuard::new(&mut ctx);
    let prepared = prepare_prin1_string(
        scope.context(),
        Value::string("leaf"),
        ObjectEscaping::Escaped,
        Value::NIL,
    )
    .unwrap();
    assert!(matches!(prepared, Prin1StringPreparation::SavedBuffer(_)));
    {
        let ctx = scope.context();
        assert_eq!(ctx.specpdl.len(), depth + 2);
        assert!(matches!(
            ctx.specpdl[depth],
            SpecBinding::SaveCurrentBuffer { .. }
        ));
        assert!(
            matches!(ctx.specpdl[depth + 1], SpecBinding::Let { sym_id, .. } if sym_id == escape)
        );
        assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::T);
    }
    scope.finish(Ok(())).unwrap();
    assert_eq!(ctx.specpdl.len(), depth);
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
}

#[test]
fn prin1_string_traversal_objects_keep_saved_buffer_with_cache_enabled() {
    let mut ctx = leaf_context();
    let escape = intern("print-escape-nonascii");
    let string = Value::string_with_text_properties(
        "x",
        vec![crate::emacs_core::value::StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );
    for value in [Value::list(vec![Value::fixnum(7)]), string] {
        let depth = ctx.specpdl.len();
        let mut scope = super::super::print::PrintEvaluationGuard::new(&mut ctx);
        let prepared =
            prepare_prin1_string(scope.context(), value, ObjectEscaping::Escaped, Value::NIL)
                .unwrap();
        assert!(matches!(prepared, Prin1StringPreparation::SavedBuffer(_)));
        {
            let ctx = scope.context();
            assert_eq!(ctx.specpdl.len(), depth + 2);
            assert!(matches!(
                ctx.specpdl[depth],
                SpecBinding::SaveCurrentBuffer { .. }
            ));
            assert!(
                matches!(ctx.specpdl[depth + 1], SpecBinding::Let { sym_id, .. } if sym_id == escape)
            );
        }
        scope.finish(Ok(())).unwrap();
        assert_eq!(ctx.specpdl.len(), depth);
        assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
    }
}

#[test]
fn prin1_string_circle_fallback_saves_before_cached_let() {
    let mut ctx = leaf_context();
    let escape = intern("print-escape-nonascii");
    let circle = intern("print-circle");
    // Built-in printer BOOLs are forwarded and cannot become aliases. A real
    // circle-enabled call must preserve Save-before-Let cleanup ordering.
    ctx.obarray.set_symbol_value_id(circle, Value::T);
    let origin = ctx.buffers.current_buffer_id().unwrap();
    let depth = ctx.specpdl.len();
    let mut scope = super::super::print::PrintEvaluationGuard::new(&mut ctx);
    let prepared = prepare_prin1_string(
        scope.context(),
        Value::string("leaf"),
        ObjectEscaping::Escaped,
        Value::NIL,
    )
    .unwrap();
    assert!(matches!(prepared, Prin1StringPreparation::SavedBuffer(_)));
    assert!(prepared.into_options().print_circle);
    {
        let ctx = scope.context();
        assert_eq!(ctx.specpdl.len(), depth + 2);
        assert!(
            matches!(ctx.specpdl[depth], SpecBinding::SaveCurrentBuffer { buffer_id } if buffer_id == origin)
        );
        assert!(
            matches!(ctx.specpdl[depth + 1], SpecBinding::Let { sym_id, .. } if sym_id == escape)
        );
    }
    scope.finish(Ok(())).unwrap();
    assert_eq!(ctx.specpdl.len(), depth);
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(circle), Value::T);
    assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
}

#[test]
fn prin1_string_watched_bind_refusal_keeps_unlet_in_watchers_buffer() {
    fn watch_escape(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        if args.get(2) == Some(&Value::symbol("let")) {
            let origin = ctx.buffers.current_buffer_id().unwrap();
            ctx.obarray
                .set_symbol_value_id(intern("leaf-bind-seen-buffer"), Value::make_buffer(origin));
            let other = ctx
                .obarray
                .symbol_value_id_or_nil(intern("leaf-watcher-other-buffer"))
                .as_buffer_id()
                .unwrap();
            ctx.set_current_buffer_unrecorded(other)?;
        } else if args.get(2) == Some(&Value::symbol("unlet")) {
            let current = ctx.buffers.current_buffer_id().unwrap();
            ctx.obarray.set_symbol_value_id(
                intern("leaf-unlet-seen-buffer"),
                Value::make_buffer(current),
            );
        }
        Ok(Value::NIL)
    }
    // Pin the preexisting caller-buffer fallback, not GNU scratch selection:
    // let sees the caller; unlet sees the watcher-selected buffer; Save then
    // restores the caller. Moving Save after Let would break the second step.
    let mut ctx = leaf_context();
    let origin = ctx.buffers.current_buffer_id().unwrap();
    let other = ctx.buffers.create_buffer("leaf-watcher-buffer");
    let escape = intern("print-escape-nonascii");
    ctx.obarray.set_symbol_value_id(
        intern("leaf-watcher-other-buffer"),
        Value::make_buffer(other),
    );
    ctx.register_subr(SubrSpec::new(
        "leaf-escape-watcher",
        NativeFn::ContextVec(watch_escape),
        SubrArity::new(4, Some(4)),
    ));
    crate::emacs_core::advice::builtin_add_variable_watcher(
        &mut ctx,
        vec![
            Value::from_sym_id(escape),
            Value::symbol("leaf-escape-watcher"),
        ],
    )
    .unwrap();
    let depth = ctx.specpdl.len();
    let result = prin1_to_lisp_string_value_in_state_with_overrides(
        &mut ctx,
        &Value::string("x"),
        ObjectEscaping::Escaped,
        None,
    )
    .unwrap();
    assert_eq!(result.as_bytes(), b"\"x\"");
    assert_eq!(
        ctx.obarray
            .symbol_value_id_or_nil(intern("leaf-bind-seen-buffer")),
        Value::make_buffer(origin)
    );
    assert_eq!(
        ctx.obarray
            .symbol_value_id_or_nil(intern("leaf-unlet-seen-buffer")),
        Value::make_buffer(other)
    );
    assert_eq!(ctx.buffers.current_buffer_id(), Some(origin));
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
    assert_eq!(ctx.specpdl.len(), depth);
}

#[test]
fn prin1_string_leaf_panic_keeps_native_restore_without_saved_buffer() {
    let mut ctx = leaf_context();
    let escape = intern("print-escape-nonascii");
    let origin = ctx.buffers.current_buffer_id();
    let depth = ctx.specpdl.len();
    let table = intern("neovm--standard-case-table-object");
    ctx.obarray.makunbound_id(table);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = super::super::print::PrintEvaluationGuard::new(&mut ctx);
        let prepared = prepare_prin1_string(
            scope.context(),
            Value::fixnum(7),
            ObjectEscaping::Escaped,
            Value::NIL,
        )
        .unwrap();
        assert!(matches!(
            prepared,
            Prin1StringPreparation::CallbackFreeLeaf { .. }
        ));
        panic!("contained unwind after real cached implicit binding");
    }));
    assert!(caught.is_err());
    assert_eq!(ctx.specpdl.len(), depth);
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(escape), Value::NIL);
    assert_eq!(ctx.buffers.current_buffer_id(), origin);
    assert!(ctx.obarray.symbol_value_id_copied(table).is_none());
}
