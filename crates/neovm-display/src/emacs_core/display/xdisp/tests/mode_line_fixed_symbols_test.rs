use super::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::intern::{intern_call_names, intern_calls, reset_intern_calls};
use strum::IntoEnumIterator;

fn context() -> Context {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let buffer = ctx.edit.buffers.create_buffer("*ttykey-k4-mode-fixed*");
    ctx.edit.buffers.set_current(buffer);
    ctx
}

fn compare(ctx: &Context, key: ModeLineSymbol, name: &str) -> Option<Value> {
    let fixed = mode_line_fixed_symbol_value_in_state(ctx.variables(), &ctx.edit.buffers, key);
    assert_eq!(
        fixed,
        mode_line_symbol_value_in_state(ctx.variables(), &[], &ctx.edit.buffers, name),
        "fixed and generic lookup must observe identical state for {name}"
    );
    fixed
}

#[test]
fn mode_line_fixed_reads_match_default_and_local_name_reads() {
    let mut ctx = context();
    ctx.eval_str(
        r#"(progn
             (set-default 'mode-name "ttykey-default-mode")
             (set-default 'default-directory "/ttykey-default/")
             (kill-local-variable 'mode-name)
             (kill-local-variable 'default-directory))"#,
    )
    .expect("install defaults");
    for (key, name, local) in [
        (ModeLineSymbol::ModeName, "mode-name", "ttykey-local-mode"),
        (
            ModeLineSymbol::DefaultDirectory,
            "default-directory",
            "/ssh:ttykey:/local/",
        ),
    ] {
        compare(&ctx, key, name);
        let buffer = ctx
            .edit
            .buffers
            .current_buffer_id()
            .expect("current buffer");
        ctx.set_buffer_local_binding_by_id(buffer, key.id(), Value::string(local))
            .expect("install local");
        assert_eq!(
            compare(&ctx, key, name).and_then(|value| value.as_utf8_str().map(str::to_owned)),
            Some(local.to_owned())
        );
    }
}

#[test]
fn mode_line_fixed_read_uses_the_running_owner_dynamic_binding() {
    let mut ctx = context();
    // This fixed property's independent variable cell starts plain, allowing
    // the existing owner-projection test seam without changing its plist.
    let key = ModeLineSymbol::RiskyLocalVariable;
    let symbol = key.id();
    ctx.set_variable("risky-local-variable", Value::fixnum(5));
    ctx.project_thread_variable_for_test(symbol)
        .expect("plain variable cell projects");
    assert_eq!(
        compare(&ctx, key, "risky-local-variable"),
        Some(Value::fixnum(5))
    );
    let depth = ctx.specpdl().len();
    ctx.try_specbind(symbol, Value::fixnum(9))
        .expect("bind owner value");
    let bound = compare(&ctx, key, "risky-local-variable");
    ctx.unbind_to(depth);
    assert_eq!(bound, Some(Value::fixnum(9)));
    assert_eq!(
        compare(&ctx, key, "risky-local-variable"),
        Some(Value::fixnum(5))
    );
}

#[test]
fn mode_line_dynamic_name_lookup_keeps_its_named_intern_path() {
    let mut ctx = context();
    let name = "ttykey-k4-arbitrary-mode-line-variable";
    ctx.set_variable(name, Value::fixnum(17));
    // Initialize the buffer-slot guard's independent one-time identity before
    // counting the unchanged general helper's steady-state name lookups.
    assert_eq!(
        mode_line_symbol_value_in_state(ctx.variables(), &[], &ctx.edit.buffers, name),
        Some(Value::fixnum(17))
    );
    reset_intern_calls();
    assert_eq!(
        mode_line_symbol_value_in_state(ctx.variables(), &[], &ctx.edit.buffers, name),
        Some(Value::fixnum(17))
    );
    assert_eq!(intern_call_names(), vec![name.to_owned(), name.to_owned()]);
}

#[test]
fn mode_line_risky_lookup_caches_only_property_identity() {
    let mut ctx = context();
    let name = "ttykey-k4-risky-dynamic-mode-symbol";
    ctx.eval_str("(put 'ttykey-k4-risky-dynamic-mode-symbol 'risky-local-variable t)")
        .expect("set risk property");
    ModeLineSymbol::RiskyLocalVariable.id();
    reset_intern_calls();
    assert!(mode_line_symbol_is_risky(&ctx.obarray_view(), name));
    assert_eq!(intern_call_names(), vec![name.to_owned()]);
    ctx.eval_str("(put 'ttykey-k4-risky-dynamic-mode-symbol 'risky-local-variable nil)")
        .expect("clear risk property");
    assert!(!mode_line_symbol_is_risky(&ctx.obarray_view(), name));
}

#[test]
fn mode_line_fixed_warm_reads_skip_named_intern() {
    let ctx = context();
    for key in ModeLineSymbol::iter() {
        key.id();
    }
    for key in [ModeLineSymbol::ModeName, ModeLineSymbol::DefaultDirectory] {
        mode_line_fixed_symbol_value_in_state(ctx.variables(), &ctx.edit.buffers, key);
    }
    reset_intern_calls();
    for _ in 0..20 {
        mode_line_fixed_symbol_value_in_state(
            ctx.variables(),
            &ctx.edit.buffers,
            ModeLineSymbol::ModeName,
        );
        mode_line_fixed_symbol_value_in_state(
            ctx.variables(),
            &ctx.edit.buffers,
            ModeLineSymbol::DefaultDirectory,
        );
    }
    assert_eq!(intern_calls(), 0, "fixed reads must avoid named interning");
}

#[test]
fn mode_line_fixed_safe_eval_keeps_bindings_and_live_function_cell() {
    let mut ctx = context();
    let form = ctx
        .eval_str("'(list inhibit-redisplay inhibit-quit)")
        .expect("safe form");
    // The subsequent eval_str can collect; keep this reused form in a traced
    // variable cell rather than relying on a Rust local as a GC root.
    ctx.set_variable("ttykey-k4-rooted-mode-line-form", form);
    let depth = ctx.specpdl().len();
    let result = mode_line_flow_policy::eval_form(&mut ctx, &form).expect("first safe eval");
    assert_eq!(neovm_eval::emacs_core::print::print_value(&result), "(t t)");
    assert_eq!(ctx.specpdl().len(), depth);
    ctx.eval_str("(fset 'eval (lambda (_form lexical) (list 'ttykey-k4-live-eval lexical inhibit-redisplay inhibit-quit)))")
        .expect("replace function cell after cache warmup");
    let result = mode_line_flow_policy::eval_form(&mut ctx, &form).expect("live safe eval");
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "(ttykey-k4-live-eval t t t)"
    );
    assert_eq!(ctx.specpdl().len(), depth);
    assert!(
        ctx.variable_value_or_nil(ModeLineSymbol::InhibitRedisplay.id())
            .is_nil()
    );
    assert!(
        ctx.variable_value_or_nil(ModeLineSymbol::InhibitQuit.id())
            .is_nil()
    );
}
