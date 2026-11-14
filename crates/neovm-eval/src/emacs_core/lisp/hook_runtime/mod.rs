use super::builtins;
use super::eval::{ConditionFrame, Context, ResumeTarget};
#[doc(hidden)]
pub use neovm_value::error::{EvalResult, Flow, FlowKind, FlowResultExt};
use neovm_value::intern::{SymId, intern};
use neovm_value::value::ValueKind;
use neovm_value::value::*;

pub(crate) trait HookRuntime {
    fn hook_context(&self) -> &Context;
    fn hook_context_mut(&mut self) -> &mut Context;
    fn call_hook_callable(
        &mut self,
        function: Value,
        args: &[Value],
        authorization: &HookWalkAuthorization<'_>,
    ) -> EvalResult;
    fn report_safe_hook_error(
        &mut self,
        hook_sym: SymId,
        function: Value,
        signal: &neovm_value::error::SignalData,
    ) -> EvalResult;
    fn remove_hook_function_after_error(&mut self, hook_sym: SymId, function: Value) -> EvalResult;
    fn with_hook_root_scope<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Flow>,
    ) -> Result<T, Flow>;
    fn push_hook_root(&mut self, value: Value);
}

impl HookRuntime for Context {
    fn hook_context(&self) -> &Context {
        self
    }

    fn hook_context_mut(&mut self) -> &mut Context {
        self
    }

    fn call_hook_callable(
        &mut self,
        function: Value,
        args: &[Value],
        authorization: &HookWalkAuthorization<'_>,
    ) -> EvalResult {
        let _ = authorization;
        self.apply(function, args.to_vec())
    }

    fn report_safe_hook_error(
        &mut self,
        hook_sym: SymId,
        function: Value,
        signal: &neovm_value::error::SignalData,
    ) -> EvalResult {
        let error_object = neovm_value::error::make_signal_binding_value(signal);
        let args = vec![
            Value::string("Error in %s (%S): %S"),
            Value::from_sym_id(hook_sym),
            function,
            error_object,
        ];
        builtins::dispatch_builtin(self, "message", args)
            .expect("`message` must remain a registered builtin")
    }

    fn remove_hook_function_after_error(&mut self, hook_sym: SymId, function: Value) -> EvalResult {
        remove_hook_function_after_error_in_context(self, hook_sym, function)
    }

    fn with_hook_root_scope<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Flow>,
    ) -> Result<T, Flow> {
        let roots = self.save_specpdl_roots();
        let result = f(self);
        self.restore_specpdl_roots(roots);
        result
    }

    fn push_hook_root(&mut self, value: Value) {
        self.push_specpdl_root(value);
    }
}

pub(crate) fn resolve_hook_symbol(ctx: &Context, hook_symbol: Value) -> Result<SymId, Flow> {
    let symbol = builtins::expect_symbol_id(&hook_symbol, ctx.symbol_mode())?;
    builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, symbol)
}

#[doc(hidden)]
pub fn hook_symbol_by_name(ctx: &Context, hook_name: &str) -> SymId {
    builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, intern(hook_name))
        .unwrap_or_else(|_| intern(hook_name))
}

/// Alias-resolving variant of [`hook_symbol_by_name`] for a PRE-INTERNED hook
/// symbol: same per-call alias resolution, no per-call intern (the intern was
/// a measured per-change-signal cost on insert-heavy code).
#[doc(hidden)]
pub fn hook_symbol_by_id(ctx: &Context, hook_sym: SymId) -> SymId {
    builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, hook_sym).unwrap_or(hook_sym)
}

#[doc(hidden)]
pub fn hook_value_by_id(ctx: &Context, hook_sym: SymId) -> Option<Value> {
    ctx.visible_runtime_variable_value_by_id_resolved(hook_sym)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HookValueLayout {
    Empty,
    Single,
    FunctionList,
}

fn hook_value_layout(ctx: &Context, hook_value: Value) -> HookValueLayout {
    match hook_value.kind() {
        ValueKind::Nil => HookValueLayout::Empty,
        ValueKind::Cons if !builtins::value_is_function(ctx, hook_value) => {
            HookValueLayout::FunctionList
        }
        _ => HookValueLayout::Single,
    }
}

fn collect_hook_functions_impl(
    ctx: &Context,
    hook_sym: SymId,
    hook_value: Value,
    inherit_global: bool,
    out: &mut Vec<Value>,
) {
    match hook_value_layout(ctx, hook_value) {
        HookValueLayout::Empty => {}
        HookValueLayout::Single => out.push(hook_value),
        HookValueLayout::FunctionList => {
            let mut cursor = hook_value;
            while cursor.is_cons() {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if pair_car.is_t() {
                    if inherit_global {
                        let global_value = ctx
                            .variables()
                            .default_value(hook_sym)
                            .unwrap_or(Value::NIL);
                        collect_hook_functions_impl(ctx, hook_sym, global_value, false, out);
                    }
                } else {
                    out.push(pair_car);
                }
                cursor = pair_cdr;
            }
        }
    }
}

#[doc(hidden)]
pub fn collect_hook_functions_in_state(
    ctx: &Context,
    hook_sym: SymId,
    hook_value: Value,
    inherit_global: bool,
) -> Vec<Value> {
    let mut functions = Vec::new();
    collect_hook_functions_impl(ctx, hook_sym, hook_value, inherit_global, &mut functions);
    functions
}

/// GNU `run_hook_with_args` returns nil when `Vrun_hooks` is nil
/// (`eval.c:3006-3007`). That static becomes nil in `shut_down_emacs`
/// and is not restored; `ShutdownPhase::Dying` is that state.
fn run_hooks_disabled<R: HookRuntime>(runtime: &R) -> bool {
    runtime.hook_context().shutdown_phase == super::eval::ShutdownPhase::Dying
}

/// Capability for one hook walk that passed the entry check.
///
/// Not [`Copy`] or [`Clone`]. The lifetime is the stack frame of
/// [`with_hook_walk`], so a caller cannot store the token and use it for
/// a later walk. Crate-private: a cross-crate walker uses
/// [`with_live_hook_walk`] and keeps its own iteration.
pub(crate) struct HookWalkAuthorization<'walk> {
    _scope: &'walk WalkScope,
}

/// Private so the token literal has a single construction site.
struct WalkScope(());

/// Run `body` with a token when hooks are live.
///
/// `when_dying` is the GNU return for a walk that starts while
/// `ShutdownPhase::Dying`. That path receives no token. A walk that already
/// holds a token keeps calling its remaining callbacks after the phase
/// changes, because the token is not re-checked.
pub(crate) fn with_hook_walk<R, T>(
    runtime: &mut R,
    when_dying: impl FnOnce() -> Result<T, Flow>,
    body: impl FnOnce(&mut R, &HookWalkAuthorization<'_>) -> Result<T, Flow>,
) -> Result<T, Flow>
where
    R: HookRuntime,
{
    if run_hooks_disabled(runtime) {
        return when_dying();
    }
    let scope = WalkScope(());
    let authorization = HookWalkAuthorization { _scope: &scope };
    body(runtime, &authorization)
}

/// Entry for a cross-crate walker that iterates a live cons itself.
///
/// GNU gates `run_window_change_functions` (`xdisp.c:17562`) and
/// `run_window_configuration_change_hook` (`window.c:3797`) before any
/// callback. The token stays in this frame. `body` is not a snapshot walk:
/// the caller reads the cons cdr after each callback.
#[doc(hidden)]
pub fn with_live_hook_walk<T>(
    ctx: &mut Context,
    when_dying: impl FnOnce() -> Result<T, Flow>,
    body: impl FnOnce(&mut Context) -> Result<T, Flow>,
) -> Result<T, Flow> {
    with_hook_walk(ctx, when_dying, |ctx, authorization| {
        let _ = authorization;
        body(ctx)
    })
}

#[doc(hidden)]
pub fn run_hook_value<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
) -> EvalResult {
    // GNU `run-hooks` / `run-hook-with-args` return nil when the walk starts
    // with `Vrun_hooks` nil (`eval.c:3004-3006`).
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            run_hook_value_authorized(
                runtime,
                hook_sym,
                hook_value,
                hook_args,
                inherit_global,
                authorization,
            )
        },
    )
}

fn run_hook_value_authorized<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
    authorization: &HookWalkAuthorization<'_>,
) -> EvalResult {
    let funcs = collect_hook_functions_in_state(
        runtime.hook_context(),
        hook_sym,
        hook_value,
        inherit_global,
    );
    // Most hooks are empty most of the time (every command runs several).
    // With nothing to call, nothing allocates, so no GC can see the roots
    // the scope below would push and restore.  Mirrors GNU
    // `run_hook_with_args` returning on a nil hook value before it binds
    // anything.
    if funcs.is_empty() {
        return Ok(Value::NIL);
    }
    runtime.with_hook_root_scope(|runtime| {
        for func in funcs.iter().copied() {
            runtime.push_hook_root(func);
        }
        for arg in hook_args.iter().copied() {
            runtime.push_hook_root(arg);
        }
        for func in funcs {
            let _ = runtime.call_hook_callable(func, hook_args, authorization)?;
        }
        Ok(Value::NIL)
    })
}

fn remove_eq_from_hook_list(hook_value: Value, function: Value) -> Option<Value> {
    let mut cursor = hook_value;
    let mut kept = Vec::new();
    let mut found = false;

    while cursor.is_cons() {
        let item = cursor.cons_car();
        if eq_value(&item, &function) {
            found = true;
        } else {
            kept.push(item);
        }
        cursor = cursor.cons_cdr();
    }

    found.then(|| Value::list(kept))
}

fn remove_hook_function_after_error_in_context(
    ctx: &mut Context,
    hook_sym: SymId,
    function: Value,
) -> EvalResult {
    if let Some(hook_value) = hook_value_by_id(ctx, hook_sym)
        && let Some(new_value) = remove_eq_from_hook_list(hook_value, function)
    {
        // GNU safe_run_hooks_error calls native Fset after its Qt handler
        // has been popped. Watchers and their nonlocal exits remain real.
        return ctx.with_hook_root_scope(|ctx| {
            ctx.push_hook_root(new_value);
            builtins::builtin_set_2(ctx, Value::from_sym_id(hook_sym), new_value)
        });
    }

    let default_value = ctx
        .variables()
        .default_value(hook_sym)
        .unwrap_or(Value::NIL);
    if let Some(new_value) = remove_eq_from_hook_list(default_value, function) {
        return ctx.with_hook_root_scope(|ctx| {
            ctx.push_hook_root(new_value);
            let symbol = Value::from_sym_id(hook_sym);
            super::data::set_default(ctx, vec![symbol, new_value])
        });
    }
    Ok(Value::NIL)
}

/// Run a hook with error recovery. Mirrors GNU
/// `keyboard.c:1908-1941` (`safe_run_hooks_error`) which logs
/// the error via `(message "Error in %s (%S): %S" hook fun error)`,
/// removes the broken function from the visible hook value or default
/// hook value, then continues running later hook functions.
#[doc(hidden)]
pub fn safe_run_hook_value<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
) -> EvalResult {
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            safe_run_hook_value_authorized(
                runtime,
                hook_sym,
                hook_value,
                hook_args,
                inherit_global,
                authorization,
            )
        },
    )
}

fn safe_run_hook_value_authorized<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
    authorization: &HookWalkAuthorization<'_>,
) -> EvalResult {
    let funcs = collect_hook_functions_in_state(
        runtime.hook_context(),
        hook_sym,
        hook_value,
        inherit_global,
    );
    // Nothing to call: no roots to push (see `run_hook_value`).
    if funcs.is_empty() {
        return Ok(Value::NIL);
    }
    runtime.with_hook_root_scope(|runtime| {
        for func in funcs.iter().copied() {
            runtime.push_hook_root(func);
        }
        for arg in hook_args.iter().copied() {
            runtime.push_hook_root(arg);
        }
        for func in funcs {
            if let Some(signal) = call_safe_hook_callable(runtime, func, hook_args, authorization)?
            {
                let _ = runtime.report_safe_hook_error(hook_sym, func, &signal)?;
                runtime.remove_hook_function_after_error(hook_sym, func)?;
            }
        }
        Ok(Value::NIL)
    })
}

/// GNU keyboard.c:1944-1961 installs internal_condition_case_n(Qt) for
/// each function, not the whole walk. Preserve the caller's interpreter or
/// VM adapter and keep caught signal roots owned until reporting finishes.
/// The handler is removed before safe_run_hooks_error: reporting/removal
/// failures must escape this boundary. A normal callback value is discarded.
pub(crate) fn call_safe_hook_callable<R: HookRuntime>(
    runtime: &mut R,
    function: Value,
    args: &[Value],
    authorization: &HookWalkAuthorization<'_>,
) -> Result<Option<Box<neovm_value::error::SignalData>>, Flow> {
    let condition_stack_base = runtime.hook_context().condition_stack_len();
    // Same native Qt callable-boundary identity used by Context::safe_funcall.
    let resume = ResumeTarget::SafeFuncall {
        condition_stack_base,
    };
    runtime
        .hook_context_mut()
        .push_condition_frame(ConditionFrame::ConditionCase {
            conditions: Value::T,
            resume: resume.clone(),
        });
    let result = runtime.call_hook_callable(function, args, authorization);
    let result = runtime
        .hook_context_mut()
        .dispatch_signal_result_if_needed(result);
    runtime
        .hook_context_mut()
        .truncate_condition_stack(condition_stack_base);
    match result.kinded() {
        Ok(_) => Ok(None),
        Err(FlowKind::Signal(signal)) if signal.selected_resume().as_ref() == Some(&resume) => {
            Ok(Some(signal))
        }
        Err(flow) => Err(Flow::from_kind(flow)),
    }
}

pub(crate) fn run_hook_value_until_success<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
) -> EvalResult {
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            run_hook_value_until_success_authorized(
                runtime,
                hook_sym,
                hook_value,
                hook_args,
                inherit_global,
                authorization,
            )
        },
    )
}

fn run_hook_value_until_success_authorized<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
    authorization: &HookWalkAuthorization<'_>,
) -> EvalResult {
    let funcs = collect_hook_functions_in_state(
        runtime.hook_context(),
        hook_sym,
        hook_value,
        inherit_global,
    );
    // Nothing to call: no roots to push (see `run_hook_value`).
    if funcs.is_empty() {
        return Ok(Value::NIL);
    }
    runtime.with_hook_root_scope(|runtime| {
        for func in funcs.iter().copied() {
            runtime.push_hook_root(func);
        }
        for arg in hook_args.iter().copied() {
            runtime.push_hook_root(arg);
        }
        for func in funcs {
            let value = runtime.call_hook_callable(func, hook_args, authorization)?;
            if value.is_truthy() {
                return Ok(value);
            }
        }
        Ok(Value::NIL)
    })
}

#[doc(hidden)]
pub fn run_hook_value_until_failure<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
) -> EvalResult {
    // GNU `run_hook_with_args` returns nil when `Vrun_hooks` is nil, and
    // `run-hook-with-args-until-failure` inverts that nil to t
    // (`eval.c:2966`, `:3004-3006`).
    with_hook_walk(
        runtime,
        || Ok(Value::T),
        |runtime, authorization| {
            run_hook_value_until_failure_authorized(
                runtime,
                hook_sym,
                hook_value,
                hook_args,
                inherit_global,
                authorization,
            )
        },
    )
}

fn run_hook_value_until_failure_authorized<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    hook_args: &[Value],
    inherit_global: bool,
    authorization: &HookWalkAuthorization<'_>,
) -> EvalResult {
    let funcs = collect_hook_functions_in_state(
        runtime.hook_context(),
        hook_sym,
        hook_value,
        inherit_global,
    );
    // Nothing to call: no roots to push (see `run_hook_value`).
    if funcs.is_empty() {
        return Ok(Value::T);
    }
    runtime.with_hook_root_scope(|runtime| {
        for func in funcs.iter().copied() {
            runtime.push_hook_root(func);
        }
        for arg in hook_args.iter().copied() {
            runtime.push_hook_root(arg);
        }
        for func in funcs {
            let value = runtime.call_hook_callable(func, hook_args, authorization)?;
            if value.is_nil() {
                return Ok(Value::NIL);
            }
        }
        Ok(Value::T)
    })
}

pub(crate) fn run_hook_value_wrapped<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    wrapper: Value,
    wrapped_args: &[Value],
    inherit_global: bool,
) -> EvalResult {
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            run_hook_value_wrapped_authorized(
                runtime,
                hook_sym,
                hook_value,
                wrapper,
                wrapped_args,
                inherit_global,
                authorization,
            )
        },
    )
}

fn run_hook_value_wrapped_authorized<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_value: Value,
    wrapper: Value,
    wrapped_args: &[Value],
    inherit_global: bool,
    authorization: &HookWalkAuthorization<'_>,
) -> EvalResult {
    let funcs = collect_hook_functions_in_state(
        runtime.hook_context(),
        hook_sym,
        hook_value,
        inherit_global,
    );
    // Nothing to call: no roots to push (see `run_hook_value`).
    if funcs.is_empty() {
        return Ok(Value::NIL);
    }
    runtime.with_hook_root_scope(|runtime| {
        for func in funcs.iter().copied() {
            runtime.push_hook_root(func);
        }
        runtime.push_hook_root(wrapper);
        for arg in wrapped_args.iter().copied() {
            runtime.push_hook_root(arg);
        }
        for func in funcs {
            let mut call_args = Vec::with_capacity(wrapped_args.len() + 1);
            call_args.push(func);
            call_args.extend(wrapped_args.iter().copied());
            let value = runtime.call_hook_callable(wrapper, &call_args, authorization)?;
            if value.is_truthy() {
                return Ok(value);
            }
        }
        Ok(Value::NIL)
    })
}

#[doc(hidden)]
pub fn run_named_hook<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_args: &[Value],
) -> EvalResult {
    let hook_value = hook_value_by_id(runtime.hook_context(), hook_sym).unwrap_or(Value::NIL);
    run_hook_value(runtime, hook_sym, hook_value, hook_args, true)
}

#[doc(hidden)]
pub fn safe_run_named_hook<R: HookRuntime>(
    runtime: &mut R,
    hook_sym: SymId,
    hook_args: &[Value],
) -> EvalResult {
    let hook_value = hook_value_by_id(runtime.hook_context(), hook_sym).unwrap_or(Value::NIL);
    safe_run_hook_value(runtime, hook_sym, hook_value, hook_args, true)
}

pub(crate) fn run_named_hooks<R: HookRuntime>(
    runtime: &mut R,
    hook_symbols: &[Value],
) -> EvalResult {
    // One entry check for the whole invocation, before any symbol resolution
    // (`eval.c:3004-3008` returns before `find_symbol_value`).
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            for hook_symbol in hook_symbols {
                let hook_sym = resolve_hook_symbol(runtime.hook_context(), *hook_symbol)?;
                let hook_value =
                    hook_value_by_id(runtime.hook_context(), hook_sym).unwrap_or(Value::NIL);
                let _ = run_hook_value_authorized(
                    runtime,
                    hook_sym,
                    hook_value,
                    &[],
                    true,
                    authorization,
                )?;
            }
            Ok(Value::NIL)
        },
    )
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn run_named_hook_with_args<R: HookRuntime>(
    runtime: &mut R,
    args: &[Value],
) -> EvalResult {
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            let hook_sym = resolve_hook_symbol(runtime.hook_context(), args[0])?;
            let hook_value =
                hook_value_by_id(runtime.hook_context(), hook_sym).unwrap_or(Value::NIL);
            run_hook_value_authorized(
                runtime,
                hook_sym,
                hook_value,
                &args[1..],
                true,
                authorization,
            )
        },
    )
}

pub(crate) fn run_named_hook_with_args_until_success<R: HookRuntime>(
    runtime: &mut R,
    args: &[Value],
) -> EvalResult {
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            let hook_sym = resolve_hook_symbol(runtime.hook_context(), args[0])?;
            let hook_value =
                hook_value_by_id(runtime.hook_context(), hook_sym).unwrap_or(Value::NIL);
            run_hook_value_until_success_authorized(
                runtime,
                hook_sym,
                hook_value,
                &args[1..],
                true,
                authorization,
            )
        },
    )
}

pub(crate) fn run_named_hook_with_args_until_failure<R: HookRuntime>(
    runtime: &mut R,
    args: &[Value],
) -> EvalResult {
    // Dying returns t here, before resolution. GNU inverts the nil from
    // `run_hook_with_args` (`eval.c:2966`, `:3004-3008`).
    with_hook_walk(
        runtime,
        || Ok(Value::T),
        |runtime, authorization| {
            let hook_sym = resolve_hook_symbol(runtime.hook_context(), args[0])?;
            let hook_value =
                hook_value_by_id(runtime.hook_context(), hook_sym).unwrap_or(Value::NIL);
            run_hook_value_until_failure_authorized(
                runtime,
                hook_sym,
                hook_value,
                &args[1..],
                true,
                authorization,
            )
        },
    )
}

pub(crate) fn run_named_hook_wrapped<R: HookRuntime>(
    runtime: &mut R,
    args: &[Value],
) -> EvalResult {
    with_hook_walk(
        runtime,
        || Ok(Value::NIL),
        |runtime, authorization| {
            let hook_sym = resolve_hook_symbol(runtime.hook_context(), args[0])?;
            let wrapper = args[1];
            let hook_value =
                hook_value_by_id(runtime.hook_context(), hook_sym).unwrap_or(Value::NIL);
            run_hook_value_wrapped_authorized(
                runtime,
                hook_sym,
                hook_value,
                wrapper,
                &args[2..],
                true,
                authorization,
            )
        },
    )
}

#[cfg(test)]
mod hook_walk_authorization_tests {
    use super::*;
    use crate::emacs_core::eval::ShutdownPhase;

    struct CountingRuntime {
        ctx: Context,
        calls: u32,
        signal_first: bool,
    }

    impl HookRuntime for CountingRuntime {
        fn hook_context(&self) -> &Context {
            &self.ctx
        }

        fn hook_context_mut(&mut self) -> &mut Context {
            &mut self.ctx
        }

        fn call_hook_callable(
            &mut self,
            _function: Value,
            _args: &[Value],
            authorization: &HookWalkAuthorization<'_>,
        ) -> EvalResult {
            let _ = authorization;
            self.calls += 1;
            if self.calls == 1 {
                self.ctx.shutdown_phase = ShutdownPhase::Dying;
                if self.signal_first {
                    return self
                        .ctx
                        .eval_str("(signal 'error '(\"probe\"))")
                        .map_err(neovm_value::error::flow_from_eval_error);
                }
            }
            Ok(Value::NIL)
        }

        fn report_safe_hook_error(
            &mut self,
            _hook_sym: SymId,
            _function: Value,
            _signal: &neovm_value::error::SignalData,
        ) -> EvalResult {
            Ok(Value::NIL)
        }

        fn remove_hook_function_after_error(
            &mut self,
            _hook_sym: SymId,
            _function: Value,
        ) -> EvalResult {
            Ok(Value::NIL)
        }

        fn with_hook_root_scope<T>(
            &mut self,
            f: impl FnOnce(&mut Self) -> Result<T, Flow>,
        ) -> Result<T, Flow> {
            f(self)
        }

        fn push_hook_root(&mut self, _value: Value) {}
    }

    fn probe_value(runtime: &mut CountingRuntime) -> (SymId, Value) {
        runtime
            .ctx
            .eval_str("(setq probe-hook '(a b))")
            .expect("hook list");
        let sym = hook_symbol_by_name(&runtime.ctx, "probe-hook");
        let value = hook_value_by_id(&runtime.ctx, sym).expect("probe-hook value");
        (sym, value)
    }

    #[test]
    fn in_flight_walk_keeps_calling_after_phase_becomes_dying() {
        // Unit specification of the entry token. A normal return after the
        // phase becomes Dying still runs the remaining callbacks on that
        // same token. The GNU-observable form of an in-flight walk is the
        // oracle carried in E'.
        let mut runtime = CountingRuntime {
            ctx: Context::new(crate::test_utils::unit_runtime()),
            calls: 0,
            signal_first: false,
        };
        let (sym, value) = probe_value(&mut runtime);
        run_hook_value(&mut runtime, sym, value, &[], true).expect("walk returns");
        assert_eq!(runtime.calls, 2);
        assert_eq!(runtime.ctx.shutdown_phase, ShutdownPhase::Dying);
    }

    #[test]
    fn dying_entry_returns_gnu_values_without_calling() {
        let mut runtime = CountingRuntime {
            ctx: Context::new(crate::test_utils::unit_runtime()),
            calls: 0,
            signal_first: false,
        };
        runtime.ctx.shutdown_phase = ShutdownPhase::Dying;
        let (sym, value) = probe_value(&mut runtime);
        assert!(
            run_hook_value(&mut runtime, sym, value, &[], true)
                .expect("run-hooks")
                .is_nil()
        );
        assert!(
            run_hook_value_until_success(&mut runtime, sym, value, &[], true)
                .expect("until-success")
                .is_nil()
        );
        assert!(
            run_hook_value_until_failure(&mut runtime, sym, value, &[], true)
                .expect("until-failure")
                .is_t()
        );
        assert_eq!(runtime.calls, 0);
    }

    #[test]
    fn named_hook_value_started_while_dying_does_not_call() {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.eval_str("(setq direct-hook (lambda () (setq direct-ran t)))")
            .expect("direct hook");
        ctx.eval_str("(setq direct-ran nil)").expect("flag");
        ctx.shutdown_phase = ShutdownPhase::Dying;
        let sym = hook_symbol_by_name(&ctx, "direct-hook");
        let value = hook_value_by_id(&ctx, sym).expect("direct value");
        let result = crate::emacs_core::eval::redisplay_hooks::named_hook_value(
            &mut ctx,
            sym,
            value,
            &[],
            true,
            crate::emacs_core::eval::redisplay_hooks::NamedHookCall::Ordinary,
        )
        .expect("direct walk");
        assert!(result.is_nil());
        assert!(ctx.eval_str("direct-ran").expect("direct-ran").is_nil());
    }

    #[test]
    fn caught_signal_inside_a_callback_continues_on_the_same_token() {
        // The callback catches the signal and returns normally. The walk
        // that already holds the token runs the remaining callback.
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.eval_str(
            r#"(progn
  (setq first nil)
  (setq second nil)
  (setq probe-hook (list
    (lambda ()
      (setq first t)
      (condition-case _ (signal 'error '("probe")) (error nil)))
    (lambda () (setq second t)))))"#,
        )
        .expect("install probe");
        let sym = hook_symbol_by_name(&ctx, "probe-hook");
        let value = hook_value_by_id(&ctx, sym).expect("probe value");
        run_hook_value(&mut ctx, sym, value, &[], true).expect("walk returns");
        assert_eq!(ctx.eval_str("first").expect("first"), Value::T);
        assert_eq!(ctx.eval_str("second").expect("second"), Value::T);
    }

    #[test]
    fn uncaught_signal_follows_ordinary_and_safe_policy() {
        let mut ordinary = CountingRuntime {
            ctx: Context::new(crate::test_utils::unit_runtime()),
            calls: 0,
            signal_first: true,
        };
        let (sym, value) = probe_value(&mut ordinary);
        let flow = run_hook_value(&mut ordinary, sym, value, &[], true).expect_err("ordinary");
        assert!(flow.is_signal());
        assert_eq!(ordinary.calls, 1);
        assert_eq!(ordinary.ctx.shutdown_phase, ShutdownPhase::Dying);

        let mut safe = CountingRuntime {
            ctx: Context::new(crate::test_utils::unit_runtime()),
            calls: 0,
            signal_first: true,
        };
        let (sym, value) = probe_value(&mut safe);
        safe_run_hook_value(&mut safe, sym, value, &[], true).expect("safe continues");
        assert_eq!(safe.calls, 2);
        assert_eq!(safe.ctx.shutdown_phase, ShutdownPhase::Dying);
    }

    #[test]
    fn dying_named_entry_skips_symbol_resolution() {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.eval_str("(defvaralias 'cyc-a 'cyc-b)")
            .expect("initial alias");
        // Public defvaralias rejects cycles, as GNU requires. Build the
        // resolver's cyclic-state fixture through the existing raw edge API,
        // as in runtime/jit/tests/varref_inline_test.rs.
        let alias_id = neovm_value::intern::intern("cyc-a");
        let base_id = neovm_value::intern::intern("cyc-b");
        ctx.obarray_mut()
            .make_alias(base_id, alias_id)
            .expect("raw alias cycle");
        let alias = ctx.eval_str("'cyc-a").expect("alias symbol");
        let malformed = Value::fixnum(123);
        let running = run_named_hook_with_args(&mut ctx, &[malformed]);
        assert_eq!(
            running
                .expect_err("malformed while running")
                .as_signal()
                .expect("signal")
                .symbol_name(),
            "wrong-type-argument"
        );
        let cyclic = run_named_hook_with_args(&mut ctx, &[alias]);
        assert_eq!(
            cyclic
                .expect_err("cyclic while running")
                .as_signal()
                .expect("signal")
                .symbol_name(),
            "cyclic-variable-indirection"
        );
        ctx.shutdown_phase = ShutdownPhase::Dying;
        assert!(
            run_named_hook_with_args(&mut ctx, &[malformed])
                .expect("malformed while dying")
                .is_nil()
        );
        assert!(
            run_named_hook_with_args_until_failure(&mut ctx, &[malformed])
                .expect("until-failure malformed")
                .is_t()
        );
        assert!(
            run_named_hook_with_args(&mut ctx, &[alias])
                .expect("cyclic while dying")
                .is_nil()
        );
        assert!(
            run_named_hook_with_args_until_failure(&mut ctx, &[alias])
                .expect("until-failure cyclic")
                .is_t()
        );
    }
}
