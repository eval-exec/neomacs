//! Rust mirror of GNU Emacs `src/data.c`.

use super::eval::{Context, ForwardStoreSite};
use crate::emacs_core::builtins::from_value::FromValue;
use neovm_value::error::EvalResult;
use neovm_value::error::{LispCondition, expect_args, signal};
use neovm_value::intern::{SymId, intern, resolve_sym};
use neovm_value::symbol::CanonicalAccess;
use neovm_value::symbol::SetInternalBind;
use neovm_value::value::{Value, ValueKind};

/// GNU `set_default_internal` (`src/data.c`).
///
/// This is the complete Lisp-facing write path for default values. Eval-owned
/// bind/unbind machinery passes its mode here as well, rather than reproducing
/// watcher policy or the redirect-specific storage switch.
pub(crate) fn set_default_internal(
    ctx: &mut Context,
    reported_symbol: Value,
    value: Value,
    bindflag: SetInternalBind,
) -> EvalResult {
    let symbol = match reported_symbol.kind() {
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        ValueKind::Symbol(id) => id,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), reported_symbol],
            ));
        }
    };
    // Only an alias walks (GNU's `start:` loop); any other symbol is its own
    // base, as `resolve_variable_alias_id` would answer.
    if ctx.obarray().is_alias_id(symbol) {
        return set_default_internal_through_alias(ctx, reported_symbol, symbol, value, bindflag);
    }
    set_default_internal_with(ctx, reported_symbol, symbol, symbol, value, bindflag)
}

/// [`set_default_internal`] of an alias.
///
/// A thread-projected base runs the whole operation on the variable it
/// projects. Its own store path ([`store_projected_default_internal`]) meets
/// the projection only after the trapped-write phase, which can no longer
/// tell an alias from the base's own name: GNU notifies a plain base twice
/// through an alias (that phase, then `set_internal`'s) and once by name.
#[cold]
#[inline(never)]
fn set_default_internal_through_alias(
    ctx: &mut Context,
    reported_symbol: Value,
    symbol: SymId,
    value: Value,
    bindflag: SetInternalBind,
) -> EvalResult {
    let resolved = super::builtins::resolve_variable_alias_id(ctx, symbol)?;
    if ctx.obarray().thread_projection_of(resolved).is_some() {
        return ctx.with_revealed_projection(resolved, |ctx| {
            set_default_internal_with(ctx, reported_symbol, symbol, resolved, value, bindflag)
        });
    }
    set_default_internal_with(ctx, reported_symbol, symbol, resolved, value, bindflag)
}

/// [`set_default_internal`] entered after alias resolution.
///
/// GNU records the unaliased symbol on the specpdl (`eval.c` "it should not
/// be aliased"), so dynamic binding and unwinding re-enter default storage
/// with an already-resolved symbol; re-running the intern/alias phase for
/// them only re-derives what the specpdl entry proves. The reported symbol
/// and watcher-phase redirect are the resolved symbol's own, exactly as the
/// pre-split binding callers passed them.
pub(crate) fn set_default_internal_resolved(
    ctx: &mut Context,
    resolved: SymId,
    value: Value,
    bindflag: SetInternalBind,
) -> EvalResult {
    set_default_internal_with(
        ctx,
        Value::from_sym_id(resolved),
        resolved,
        resolved,
        value,
        bindflag,
    )
}

/// Shared body: `reported_symbol`/`original` preserve the caller-visible
/// identity (an alias keeps its own name in errors and its own redirect in
/// the alias-watcher phase); `resolved` owns storage.
fn set_default_internal_with(
    ctx: &mut Context,
    reported_symbol: Value,
    original: SymId,
    resolved: SymId,
    value: Value,
    bindflag: SetInternalBind,
) -> EvalResult {
    let symbol = original;
    // GNU `set_default_internal` (`src/data.c`) switches on `trapped_write`
    // first: an untrapped symbol is neither a constant (`SYMBOL_NOWRITE`)
    // nor watched (`SYMBOL_TRAPPED_WRITE`), so it goes straight to the
    // store.  This is the `let` of a per-buffer variable being unwound.
    if bindflag != SetInternalBind::ThreadSwitch
        && ctx.obarray().get_by_id(resolved).is_some_and(|sym| {
            sym.trapped_write() == neovm_value::symbol::SymbolTrappedWrite::Untrapped
        })
    {
        debug_assert!(
            super::builtins::constant_set_outcome_in_obarray(
                ctx.obarray(),
                resolved,
                reported_symbol,
                value
            )
            .is_none(),
            "an untrapped symbol must not be a constant"
        );
        debug_assert!(!ctx.watchers.has_watchers(resolved));
        store_default_internal(ctx, resolved, value, bindflag)?;
        return Ok(value);
    }
    if let Some(result) = super::builtins::constant_set_outcome_in_obarray(
        ctx.obarray(),
        resolved,
        reported_symbol,
        value,
    ) {
        return result;
    }

    // GNU first notifies for any non-plain original redirect (including an
    // alias), reporting this set-default phase as `set`. It then falls through
    // set_internal for plain and non-buffer/non-kboard forwarders, which emits
    // the operation implied by BINDFLAG. Keeping both phases explicit
    // preserves GNU's two callbacks for alias→plain and integer/object
    // forwarders without duplicating callbacks for LOCALIZED/BUFFER_OBJFWD.
    if bindflag != SetInternalBind::ThreadSwitch {
        use neovm_value::forward::LispFwdType;
        use neovm_value::symbol::SymbolRedirect;

        let original_redirect = ctx
            .obarray()
            .get_by_id(symbol)
            .map(|sym| sym.redirect())
            .unwrap_or(SymbolRedirect::Plainval);
        if original_redirect != SymbolRedirect::Plainval {
            ctx.run_variable_watchers_by_id(resolved, &value, &Value::NIL, "set")?;
        }

        let resolved_redirect = ctx
            .obarray()
            .get_by_id(resolved)
            .map(|sym| sym.redirect())
            .unwrap_or(SymbolRedirect::Plainval);
        let falls_through_set_internal = match resolved_redirect {
            SymbolRedirect::Plainval => true,
            SymbolRedirect::Forwarded => !matches!(
                ctx.obarray().forward_type(resolved),
                Some(LispFwdType::BufferObj | LispFwdType::KboardObj)
            ),
            SymbolRedirect::Localized | SymbolRedirect::Varalias => false,
        };
        if falls_through_set_internal {
            let operation = match bindflag {
                SetInternalBind::Set => {
                    if value.is_unbound() {
                        "makunbound"
                    } else {
                        "set"
                    }
                }
                SetInternalBind::Bind => "let",
                SetInternalBind::Unbind => "unlet",
                SetInternalBind::ThreadSwitch => unreachable!("filtered above"),
            };
            let reported_value = if value.is_unbound() {
                Value::NIL
            } else {
                value
            };
            ctx.run_variable_watchers_by_id(resolved, &reported_value, &Value::NIL, operation)?;
            // These arms ARE a `set_internal` call (`src/data.c:2067`,
            // `:2123`), which stores on the arm the watcher left: a
            // `make-local-variable` in it sends a `set-default` to the new
            // buffer-local binding.
            ctx.set_internal_after_watchers(resolved, value, bindflag)?;
            if bindflag == SetInternalBind::Set {
                ctx.note_macro_expansion_mutation();
            }
            return Ok(value);
        }
    }

    store_default_internal(ctx, resolved, value, bindflag)?;
    Ok(value)
}

/// [`store_default_internal`] of a thread-projected variable: GNU
/// `set_default_internal`'s redirect switch (`src/data.c:2034-2128`) on the
/// variable it projects, entered where the projection's per-buffer header
/// meets that switch, after [`set_default_internal_with`]'s trapped-write
/// phase.
///
/// That phase notified the watchers once, as `set`, for the projected cell's
/// forwarded redirect. For a localized or per-buffer variable GNU notifies
/// there and only there, then stores the default. For a builtin-forwarded one
/// GNU's `set_internal` notifies again, with BINDFLAG's operation, which this
/// does. For a plain variable set by its own name GNU's one notification is
/// `set_internal`'s, and the phase already made it: only `set-default`
/// reaches a plain variable here, so its operation is that `set`. (Through an
/// alias, GNU notifies a plain variable twice; [`set_default_internal_through_alias`]
/// reveals before that phase, so an alias never arrives here.)
///
/// The default this owner binds, if it binds one, takes the store; canonical
/// storage otherwise. Watchers run on the projected cell, never on a revealed
/// one.
#[cold]
#[inline(never)]
fn store_projected_default_internal(
    ctx: &mut Context,
    resolved: SymId,
    value: Value,
    bindflag: SetInternalBind,
) -> EvalResult {
    use neovm_value::forward::{CanonicalArm, LispFwdType};

    let Some(projection) = ctx.obarray().thread_projection_of(resolved) else {
        return store_default_internal(ctx, resolved, value, bindflag);
    };
    let arm = projection.arm();
    let falls_through_set_internal = match arm {
        CanonicalArm::Plain => true,
        CanonicalArm::Forwarded(builtin) => !matches!(
            builtin.descriptor().ty(),
            LispFwdType::BufferObj | LispFwdType::KboardObj
        ),
        CanonicalArm::Localized(_) => false,
    };
    if !falls_through_set_internal {
        if ctx.owner_default_binding(resolved).is_some() {
            let stored = super::eval::check_forwarded_store_at(
                ctx.obarray(),
                &ctx.edit.buffers,
                &ctx.specpdl,
                resolved,
                value,
                ForwardStoreSite::SetDefault,
            )?
            .value();
            ctx.store_owner_default(resolved, stored);
            if bindflag == SetInternalBind::Set {
                ctx.note_macro_expansion_mutation();
            }
            if ctx.runtime_binding_has_projection(resolved) {
                let visible = ctx.visible_variable_value_or_nil_by_id(resolved);
                ctx.publish_runtime_binding_write_by_resolved_id(resolved, visible);
            }
            return Ok(value);
        }
        return ctx.with_revealed_projection(resolved, |ctx| {
            store_default_internal(ctx, resolved, value, bindflag)
        });
    }
    if bindflag != SetInternalBind::ThreadSwitch && arm != CanonicalArm::Plain {
        let operation = match bindflag {
            SetInternalBind::Set if value.is_unbound() => "makunbound",
            SetInternalBind::Set => "set",
            SetInternalBind::Bind => "let",
            SetInternalBind::Unbind => "unlet",
            SetInternalBind::ThreadSwitch => unreachable!("filtered above"),
        };
        let reported_value = if value.is_unbound() {
            Value::NIL
        } else {
            value
        };
        ctx.run_variable_watchers_by_id(resolved, &reported_value, &Value::NIL, operation)?;
    }
    // GNU's `set_internal` (`src/data.c:2067`, `:2123`): the owner's binding
    // of the channel it meets, or canonical storage.
    ctx.set_internal_after_watchers(resolved, value, bindflag)?;
    if bindflag == SetInternalBind::Set {
        ctx.note_macro_expansion_mutation();
    }
    Ok(value)
}

/// The resolved storage half of GNU `set_default_internal`.
///
/// `eval.c` uses this path for default-valued dynamic bindings as well as for
/// ordinary `set-default`. Keeping the bind mode explicit prevents binding,
/// unwind, and thread-switch callers from rebuilding the BUFFER_OBJFWD /
/// LOCALIZED storage switch and forgetting host-cache invalidation.
fn store_default_internal(
    ctx: &mut Context,
    resolved: SymId,
    value: Value,
    bindflag: SetInternalBind,
) -> EvalResult {
    let stored = super::eval::check_forwarded_store_at(
        ctx.obarray(),
        &ctx.edit.buffers,
        &ctx.specpdl,
        resolved,
        value,
        ForwardStoreSite::SetDefault,
    )?
    .value();

    if ctx.obarray().forward_type(resolved) == Some(neovm_value::forward::LispFwdType::BufferObj) {
        match neovm_edit::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved) {
            // GNU's BUFFER_OBJFWD arm updates the shared default and
            // propagates it only to live buffers that do not have a local
            // value.
            Some(info) => ctx.edit.buffers.set_buffer_default_slot(info, stored),
            // Only a thread projection's header has no slot.
            None => return store_projected_default_internal(ctx, resolved, value, bindflag),
        }
    } else {
        // For PLAINVAL this is the currently installed dynamic value cell; for
        // LOCALIZED it is the BLV default cell; other forwarders store through
        // their descriptor. This is the same storage split as GNU data.c.
        ctx.obarray_mut()
            .set_canonical_value(CanonicalAccess::STORAGE, resolved, stored);
    }

    if bindflag == SetInternalBind::Set {
        ctx.note_macro_expansion_mutation();
    }
    // A default write need not change the value visible in the current buffer:
    // an explicit local binding still wins. Derived host-side state must track
    // that visible value, not blindly adopt the newly stored default.
    // Only a handful of symbols project anywhere; for the rest the visible
    // value (a lexenv scan plus a full variable lookup, ~460 Ir) was computed
    // and discarded on every `let` of a per-buffer default.
    if ctx.runtime_binding_has_projection(resolved) {
        let visible = ctx.visible_variable_value_or_nil_by_id(resolved);
        ctx.publish_runtime_binding_write_by_resolved_id(resolved, visible);
    }
    Ok(value)
}

/// `(set-default SYMBOL VALUE)`.
#[doc(hidden)]
pub fn set_default(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("set-default", &args, 2)?;
    set_default_internal(ctx, args[0], args[1], SetInternalBind::Set)
}

/// `(default-boundp SYMBOL)`.
#[doc(hidden)]
pub fn default_boundp(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("default-boundp", &args, 1)?;
    let symbol = SymId::from_value(ctx, args[0])?;
    let resolved = super::builtins::resolve_variable_alias_id_in_obarray(ctx.obarray(), symbol)?;
    if let Some(binding) = ctx.owner_default_binding(resolved) {
        return Ok(Value::bool_val(binding.bound().is_some()));
    }
    Ok(Value::bool_val(
        ctx.obarray()
            .canonical_boundp_id(CanonicalAccess::STORAGE, resolved)
            || ctx.obarray().is_constant_id(resolved),
    ))
}

/// `(default-value SYMBOL)` -- get the default (global) value of a variable.
#[doc(hidden)]
pub fn default_value(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("default-value", &args, 1)?;
    let symbol = match args[0].kind() {
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        ValueKind::Symbol(id) => id,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };
    let resolved = super::builtins::resolve_variable_alias_id_in_obarray(ctx.obarray(), symbol)?;
    let resolved_name = resolve_sym(resolved);

    match default_value_by_id(ctx, resolved) {
        Some(value) => Ok(value),
        None if super::builtins::is_canonical_symbol_id(resolved)
            && resolved_name.starts_with(':') =>
        {
            Ok(Value::from_kw_id(resolved))
        }
        None => Err(signal(LispCondition::VoidVariable, vec![args[0]])),
    }
}

/// Read a resolved symbol's current default storage.
///
/// The obarray's BUFFER_OBJFWD descriptor contains only its installation-time
/// fallback. The mutable default lives in `BufferManager::buffer_defaults`, so
/// binding/thread-switch code must use this projection rather than reading the
/// static descriptor directly.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn default_value_by_id(ctx: &Context, resolved: SymId) -> Option<Value> {
    // The default this owner binds, if it binds one (GNU's `default_value`
    // reads the running thread's binding).
    if let Some(binding) = ctx.owner_default_binding(resolved) {
        return binding.bound();
    }
    default_value_in_state(
        ctx.obarray(),
        Some(&ctx.edit.buffers.buffer_defaults),
        resolved,
    )
}

/// State-only form of [`default_value_by_id`] for callers that already hold
/// split obarray and buffer-default borrows.
pub(crate) fn default_value_in_state(
    obarray: &neovm_value::symbol::Obarray,
    buffer_defaults: Option<&[Value]>,
    resolved: SymId,
) -> Option<Value> {
    // GNU `Fdefault_value` dispatches BUFFER_OBJFWD through the live shared
    // buffer default rather than the forwarded symbol cell.
    if obarray.forward_type(resolved) == Some(neovm_value::forward::LispFwdType::BufferObj) {
        let Some(info) = neovm_edit::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved) else {
            // Only a thread projection's header has no slot.
            return obarray
                .thread_projection_of(resolved)
                .and_then(|projection| obarray.projected_default_value(projection));
        };
        let offset = info.offset.index();
        if let Some(defaults) = buffer_defaults
            && offset < defaults.len()
        {
            return Some(defaults[offset]);
        }
        if let Some(default) = obarray
            .forwarder(resolved)
            .and_then(|forwarder| forwarder.as_buffer_obj_fwd())
            .map(|buf_fwd| buf_fwd.default)
        {
            return Some(default);
        }
    }

    obarray.canonical_default_value(CanonicalAccess::STORAGE, resolved)
}

#[cfg(test)]
#[path = "tests/data_test.rs"]
mod tests;

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod test_support {
    // A1 rows (post-split option A, peer sign-off batch): additive accessors
    // for tests re-homed into core. No product visibility change.
    use super::{Context, SymId};

    pub fn default_value_by_id(
        ctx: &Context,
        resolved: SymId,
    ) -> Option<neovm_value::value::Value> {
        super::default_value_by_id(ctx, resolved)
    }
}

#[doc(hidden)]
pub mod subrs;
#[doc(hidden)]
pub use subrs::{SUBRS, register_subrs};
