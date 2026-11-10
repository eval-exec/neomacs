use super::builtins;
use super::eval::Context;
#[doc(hidden)]
pub use neovm_value::error::{EvalResult, Flow, FlowKind, FlowResultExt};
use neovm_value::intern::{SymId, intern};
use neovm_value::value::ValueKind;
use neovm_value::value::*;

pub(crate) trait HookRuntime {
    fn hook_context(&self) -> &Context;
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
    fn remove_hook_function_after_error(&mut self, hook_sym: SymId, function: Value);
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

    fn remove_hook_function_after_error(&mut self, hook_sym: SymId, function: Value) {
        remove_hook_function_after_error_in_context(self, hook_sym, function);
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
) {
    if let Some(hook_value) = hook_value_by_id(ctx, hook_sym)
        && let Some(new_value) = remove_eq_from_hook_list(hook_value, function)
    {
        let _ = ctx.try_set_runtime_binding_by_id(hook_sym, new_value);
        return;
    }

    let default_value = ctx
        .variables()
        .default_value(hook_sym)
        .unwrap_or(Value::NIL);
    if let Some(new_value) = remove_eq_from_hook_list(default_value, function) {
        let symbol = Value::from_sym_id(hook_sym);
        if let Err(flow) = super::data::set_default(ctx, vec![symbol, new_value]) {
            let rendered = crate::emacs_core::print::format_flow_with_eval(ctx, &flow);
            tracing::warn!(
                "failed to remove broken hook function {} from default {}: {}",
                function,
                neovm_value::intern::resolve_sym(hook_sym),
                rendered
            );
        }
    }
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
            match runtime
                .call_hook_callable(func, hook_args, authorization)
                .kinded()
            {
                Ok(_) => {}
                Err(FlowKind::Signal(ref sig)) => {
                    let _ = runtime.report_safe_hook_error(hook_sym, func, sig)?;
                    runtime.remove_hook_function_after_error(hook_sym, func);
                }
                Err(flow) => return Err(Flow::from_kind(flow)),
            }
        }
        Ok(Value::NIL)
    })
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
    use crate::emacs_core::eval::keyboard::{FatalSignal, FatalSignalShutdown};
    use crate::emacs_core::eval::{ShutdownPhase, ShutdownRequest};

    struct CountingRuntime {
        ctx: Context,
        calls: u32,
        signal_first: bool,
    }

    impl HookRuntime for CountingRuntime {
        fn hook_context(&self) -> &Context {
            &self.ctx
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

        fn remove_hook_function_after_error(&mut self, _hook_sym: SymId, _function: Value) {}

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

    /// Lisp variable on the Context that ran the dispatch. Each Context has
    /// its own binding, so two Contexts do not share a log.
    fn dispatch_log_symbol(ctx: &Context) -> SymId {
        hook_symbol_by_name(ctx, "neomacs--shutdown-dispatch-log")
    }

    fn cons_list(items: Vec<Value>) -> Value {
        items
            .into_iter()
            .rev()
            .fold(Value::NIL, |cdr, car| Value::cons(car, cdr))
    }

    fn list_items(mut list: Value) -> Vec<Value> {
        let mut items = Vec::new();
        while !list.is_nil() {
            assert!(list.is_cons(), "dispatch list");
            items.push(list.cons_car());
            list = list.cons_cdr();
        }
        items
    }

    /// Recording stand-in for the upper subr the unit runtime otherwise
    /// refuses. Installed per test through [`Context::register_subr`], the
    /// same registration the unit runtime uses for that table. The census
    /// stays on the refusing entry. The record is a Lisp list on this
    /// Context: `(args phase-fixnum . stored-flag)`, consed onto
    /// `neomacs--shutdown-dispatch-log`.
    fn recording_shutdown_subr(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        let phase = ctx.shutdown_phase;
        let request_stored = ctx.shutdown_request().is_some();
        let stored = if request_stored { Value::T } else { Value::NIL };
        let record = Value::cons(
            cons_list(args),
            Value::cons(Value::fixnum(phase as u8 as i64), stored),
        );
        let sym = dispatch_log_symbol(ctx);
        let prev = hook_value_by_id(ctx, sym).unwrap_or(Value::NIL);
        ctx.assign_by_id(sym, Value::cons(record, prev));
        Ok(Value::NIL)
    }

    fn install_recording_shutdown_subr(ctx: &mut Context) {
        ctx.assign_by_id(dispatch_log_symbol(ctx), Value::NIL);
        ctx.register_subr(crate::emacs_core::subr::SubrSpec::new(
            "do-auto-save",
            crate::emacs_core::subr::NativeFn::ContextVec(recording_shutdown_subr),
            crate::emacs_core::subr::SubrArity::new(0, Some(2)),
        ));
    }

    fn assert_shutdown_dispatches(ctx: &Context, count: usize) {
        let list = hook_value_by_id(ctx, dispatch_log_symbol(ctx)).unwrap_or(Value::NIL);
        let mut rows = list_items(list);
        rows.reverse();
        assert_eq!(rows.len(), count, "shutdown dispatch count");
        for (index, record) in rows.iter().enumerate() {
            assert!(record.is_cons(), "dispatch record");
            let args = list_items(record.cons_car());
            let tail = record.cons_cdr();
            assert!(tail.is_cons(), "dispatch phase");
            assert_eq!(
                args,
                vec![Value::T, Value::NIL],
                "dispatch {index} arguments"
            );
            assert_eq!(
                tail.cons_car().as_fixnum(),
                Some(ShutdownPhase::Dying as u8 as i64),
                "dispatch {index} phase"
            );
            assert!(
                tail.cons_cdr().is_nil(),
                "dispatch {index} saw a stored request"
            );
        }
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
    fn kill_emacs_hook_runs_once_before_dying_and_later_walks_stay_silent() {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.eval_str(
            r#"(progn
  (setq n 0)
  (setq during nil)
  (setq probe-ran nil)
  (setq kill-emacs-hook (lambda ()
    (setq during inhibit-redisplay)
    (setq n (1+ n))))
  (setq probe-hook (lambda () (setq probe-ran t))))"#,
        )
        .expect("install hooks");
        install_recording_shutdown_subr(&mut ctx);
        let flow = ctx
            .shutdown_with_hooks(ShutdownRequest {
                exit_code: 0,
                restart: false,
            })
            .expect_err("shutdown");
        assert!(flow.is_shutdown());
        assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
        assert!(ctx.eval_str("during").expect("during").is_nil());
        assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(1));
        assert!(ctx.eval_str("inhibit-redisplay").expect("inhibit").is_t());
        assert!(
            ctx.eval_str("(run-hooks 'probe-hook)")
                .expect("run-hooks")
                .is_nil()
        );
        assert!(
            ctx.eval_str("(run-hook-with-args-until-success 'probe-hook)")
                .expect("until-success")
                .is_nil()
        );
        assert!(
            ctx.eval_str("(run-hook-with-args-until-failure 'probe-hook)")
                .expect("until-failure")
                .is_t()
        );
        assert!(ctx.eval_str("probe-ran").expect("probe-ran").is_nil());
        assert_shutdown_dispatches(&ctx, 1);
    }

    #[test]
    fn hook_callback_kill_emacs_does_not_run_the_remaining_callbacks() {
        // GNU-observable shape: `Fkill_emacs` does not return, so the rest of
        // the walk does not run. Here `Flow::shutdown` propagates through the
        // walker and is not swallowed.
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.eval_str(
            r#"(progn
  (setq first nil)
  (setq second nil)
  (setq probe-hook (list
    (lambda () (setq first t) (kill-emacs 0))
    (lambda () (setq second t)))))"#,
        )
        .expect("install probe");
        install_recording_shutdown_subr(&mut ctx);
        let sym = hook_symbol_by_name(&ctx, "probe-hook");
        let value = hook_value_by_id(&ctx, sym).expect("probe value");
        let flow = run_hook_value(&mut ctx, sym, value, &[], true).expect_err("shutdown flow");
        assert!(flow.is_shutdown());
        assert_eq!(ctx.eval_str("first").expect("first"), Value::T);
        assert!(ctx.eval_str("second").expect("second").is_nil());
        assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
        assert_shutdown_dispatches(&ctx, 1);
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

    /// The eval unit runtime refuses the display subrs. These stand-ins answer
    /// from this Context's frame table, the same live-frame rule as
    /// `frame-live-p`, and `t` for a terminal object while a frame is live
    /// (TTY `terminal-live-p`).
    fn install_display_liveness_standins(ctx: &mut Context) {
        use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
        use neovm_value::value::{ValueKind, VecLikeType};

        fn selected_frame(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
            Ok(match ctx.frame_manager().selected_frame() {
                Some(frame) => Value::make_frame(frame.id.0),
                None => Value::NIL,
            })
        }
        fn frame_live_p(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
            let live = args
                .first()
                .and_then(|value| value.as_frame_id())
                .and_then(|id| {
                    ctx.frame_manager()
                        .get(neovm_value::display_vocab::FrameId(id))
                })
                .is_some();
            Ok(Value::bool_val(live))
        }
        fn frame_terminal(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
            Ok(match ctx.frame_manager().selected_frame() {
                Some(frame) => Value::make_terminal(frame.terminal_id),
                None => Value::NIL,
            })
        }
        fn terminal_live_p(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
            let terminal = matches!(
                args.first().copied().map(Value::kind),
                Some(ValueKind::Veclike(VecLikeType::Terminal))
            );
            let frame_live = ctx
                .frame_manager()
                .frame_list()
                .into_iter()
                .any(|id| ctx.frame_manager().get(id).is_some());
            Ok(Value::bool_val(terminal && frame_live))
        }
        ctx.register_subr(SubrSpec::new(
            "selected-frame",
            NativeFn::ContextVec(selected_frame),
            SubrArity::new(0, Some(0)),
        ));
        ctx.register_subr(SubrSpec::new(
            "frame-live-p",
            NativeFn::ContextVec(frame_live_p),
            SubrArity::new(1, Some(1)),
        ));
        ctx.register_subr(SubrSpec::new(
            "frame-terminal",
            NativeFn::ContextVec(frame_terminal),
            SubrArity::new(0, Some(1)),
        ));
        ctx.register_subr(SubrSpec::new(
            "terminal-live-p",
            NativeFn::ContextVec(terminal_live_p),
            SubrArity::new(1, Some(1)),
        ));
    }

    fn insert_live_tty_frame(ctx: &mut Context) {
        use crate::emacs_core::eval::data::window_state::{Frame, Window};
        use neovm_edit::emacs_core::display_metrics::{Rect, WindowId};

        let scratch = ctx.buffer_manager_mut().create_buffer("*scratch*");
        let frames = ctx.frame_manager_mut();
        let frame_id = neovm_value::display_vocab::FrameId(frames.next_frame_id);
        frames.next_frame_id += 1;
        let window_id = WindowId(frames.next_window_id);
        frames.next_window_id += 1;
        let minibuffer_id = WindowId(frames.next_window_id);
        frames.next_window_id += 1;
        let root = Window::new_leaf(window_id, scratch, Rect::new(0.0, 0.0, 80.0, 24.0));
        let frame = Frame::new(
            frame_id,
            Value::string("F1"),
            crate::emacs_core::eval::protocol::tty_host::TERMINAL_ID,
            80,
            24,
            root,
            minibuffer_id,
        );
        frames.frames.insert(frame_id, frame);
        frames.selected = Some(frame_id);
        frames.terminal_top_frames.insert(
            crate::emacs_core::eval::protocol::tty_host::TERMINAL_ID,
            frame_id,
        );
    }

    fn install_last_terminal_hook(ctx: &mut Context) {
        insert_live_tty_frame(ctx);
        install_display_liveness_standins(ctx);
        ctx.eval_str(
            r#"(progn
  (setq n 0)
  (setq during nil)
  (setq frame-was-live nil)
  (setq terminal-was-live nil)
  (setq kill-emacs-hook (lambda ()
    (setq during inhibit-redisplay)
    (setq n (1+ n))
    (setq frame-was-live (frame-live-p (selected-frame)))
    (setq terminal-was-live (terminal-live-p (frame-terminal))))))"#,
        )
        .expect("install hook");
        install_recording_shutdown_subr(ctx);
    }

    fn assert_last_terminal_exit(ctx: &mut Context) {
        assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
        assert!(ctx.eval_str("during").expect("during").is_nil());
        assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(1));
        assert!(ctx.eval_str("frame-was-live").expect("frame").is_t());
        assert!(ctx.eval_str("terminal-was-live").expect("terminal").is_t());
        assert!(ctx.eval_str("inhibit-redisplay").expect("inhibit").is_t());
        assert_eq!(
            ctx.shutdown_request().expect("request"),
            ShutdownRequest::fatal_signal(FatalSignal::Hangup)
        );
        assert_shutdown_dispatches(ctx, 1);
    }

    #[test]
    fn last_terminal_disconnect_enters_dying_after_kill_emacs_hook() {
        // GNU Emacs 31.1 a360712c9d, keyboard.c:8113-8124 and emacs.c:441-462.
        // Last-terminal hangup exits with SIGHUP (code 1) after one
        // kill-emacs-hook call that still sees a live frame and terminal.
        // Delete hooks do not run. Accepted pty evidence:
        // escape-gnu-tty-hangup-2821-artifacts-20261110T210907Z
        // Unattested GNU, not the attested oracle. Source contract at a360712c9d,
        // kill.log "KILL frame=t terminal=t", delete.log absent, exit 1.
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        install_last_terminal_hook(&mut ctx);
        ctx.handle_display_terminal_disconnect();
        assert_last_terminal_exit(&mut ctx);

        // GNU's SIGHUP path calls `Fkill_emacs` again while the first call
        // is still on the stack (`emacs.c:453-460`). A disconnect during
        // InProgress re-enters the hook instead of returning nil.
        let mut during = Context::new(crate::test_utils::unit_runtime());
        install_last_terminal_hook(&mut during);
        during.shutdown_phase = ShutdownPhase::InProgress;
        during.handle_display_terminal_disconnect();
        assert_last_terminal_exit(&mut during);
    }

    #[test]
    fn nested_kill_emacs_inside_the_hook_reenters_and_keeps_the_inner_code() {
        // GNU probe: hook A calls `(kill-emacs 5)` on its first run, hook B
        // is later on the list, outer `(kill-emacs 0)`. The inner call runs
        // the list again and the process exits 5. Log shape: HOOK 1, HOOK 2,
        // SECOND-HOOK. B runs once, inside the inner run.
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.eval_str(
            r#"(progn
  (setq n 0)
  (setq second 0)
  (setq kill-emacs-hook (list
    (lambda ()
      (setq n (1+ n))
      (if (= n 1) (progn (kill-emacs 5))))
    (lambda ()
      (setq second (1+ second))))))"#,
        )
        .expect("install hooks");
        install_recording_shutdown_subr(&mut ctx);
        ctx.eval_str("(kill-emacs 0)")
            .expect_err("inner shutdown propagates");
        assert_eq!(ctx.shutdown_request().expect("inner").exit_code, 5);
        assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(2));
        assert_eq!(ctx.eval_str("second").expect("second"), Value::fixnum(1));
        assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
        assert!(ctx.eval_str("inhibit-redisplay").expect("inhibit").is_t());
        assert_shutdown_dispatches(&ctx, 1);
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

    #[test]
    fn dying_retry_skips_the_hook_and_keeps_a_published_request() {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.eval_str(
            r#"(progn
  (setq n 0)
  (setq kill-emacs-hook (lambda () (setq n (1+ n)))))"#,
        )
        .expect("install hook");
        install_recording_shutdown_subr(&mut ctx);
        ctx.shutdown_phase = ShutdownPhase::Dying;
        let flow = ctx
            .shutdown_with_hooks(ShutdownRequest {
                exit_code: 3,
                restart: false,
            })
            .expect_err("retry");
        assert!(flow.is_shutdown());
        assert_eq!(ctx.shutdown_request().expect("published").exit_code, 3);
        assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(0));
        assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
        let again = ctx
            .shutdown_with_hooks(ShutdownRequest {
                exit_code: 0,
                restart: false,
            })
            .expect_err("published request stays");
        assert!(again.is_shutdown());
        assert_eq!(ctx.shutdown_request().expect("still 3").exit_code, 3);
        assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(0));
        assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
        assert_shutdown_dispatches(&ctx, 1);
    }
}
