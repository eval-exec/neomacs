//! Rust mirror of GNU Emacs `src/data.c`.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use super::error::EvalResult;
use super::eval::{Context, ForwardStoreSite};
use super::intern::{SymId, intern, resolve_sym};
use super::symbol::SetInternalBind;
use super::value::{Value, ValueKind};
use crate::emacs_core::builtins::from_value::FromValue;
use crate::emacs_core::error::{LispCondition, expect_args, signal};

/// The storage descriptor for a GNU `BUFFER_OBJFWD` variable.
///
/// Keeping this lookup typed avoids turning a resolved symbol identity back
/// into a string merely to recover the corresponding buffer slot.
fn forwarded_buffer_slot_info(
    ctx: &Context,
    resolved: SymId,
) -> Option<&'static crate::buffer::buffer::BufferSlotInfo> {
    use super::forward::LispFwdType;

    (ctx.obarray().forward_type(resolved) == Some(LispFwdType::BufferObj))
        .then(|| crate::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved))
        .flatten()
}

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
    let resolved = super::builtins::resolve_variable_alias_id(ctx, symbol)?;
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
        use super::forward::LispFwdType;
        use super::symbol::SymbolRedirect;

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
        }
    }

    store_default_internal(ctx, resolved, value, bindflag)?;
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
        &ctx.buffers,
        &ctx.specpdl,
        resolved,
        value,
        ForwardStoreSite::SetDefault,
    )?
    .value();

    if let Some(info) = forwarded_buffer_slot_info(ctx, resolved) {
        // GNU's BUFFER_OBJFWD arm updates the shared default and propagates it
        // only to live buffers that do not have a local value.
        ctx.buffers.set_buffer_default_slot(info, stored);
    } else {
        // For PLAINVAL this is the currently installed dynamic value cell; for
        // LOCALIZED it is the BLV default cell; other forwarders store through
        // their descriptor. This is the same storage split as GNU data.c.
        ctx.obarray_mut().set_symbol_value_id(resolved, stored);
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
        ctx.publish_runtime_binding_write_by_id(resolved, visible);
    }
    Ok(value)
}

/// `(set-default SYMBOL VALUE)`.
pub(crate) fn set_default(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("set-default", &args, 2)?;
    set_default_internal(ctx, args[0], args[1], SetInternalBind::Set)
}

/// `(default-boundp SYMBOL)`.
pub(crate) fn default_boundp(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("default-boundp", &args, 1)?;
    let symbol = SymId::from_value(ctx, args[0])?;
    let resolved = super::builtins::resolve_variable_alias_id_in_obarray(ctx.obarray(), symbol)?;
    Ok(Value::bool_val(
        ctx.obarray().boundp_id(resolved) || ctx.obarray().is_constant_id(resolved),
    ))
}

/// `(default-value SYMBOL)` -- get the default (global) value of a variable.
pub(crate) fn default_value(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
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
pub(crate) fn default_value_by_id(ctx: &Context, resolved: SymId) -> Option<Value> {
    default_value_in_state(ctx.obarray(), Some(&ctx.buffers.buffer_defaults), resolved)
}

/// State-only form of [`default_value_by_id`] for callers that already hold
/// split obarray and buffer-default borrows.
pub(crate) fn default_value_in_state(
    obarray: &super::symbol::Obarray,
    buffer_defaults: Option<&[Value]>,
    resolved: SymId,
) -> Option<Value> {
    // GNU `Fdefault_value` dispatches BUFFER_OBJFWD through the live shared
    // buffer default rather than the forwarded symbol cell.
    if obarray.forward_type(resolved) == Some(super::forward::LispFwdType::BufferObj)
        && let Some(info) = crate::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved)
    {
        let offset = info.offset.index();
        if let Some(defaults) = buffer_defaults
            && offset < defaults.len()
        {
            return Some(defaults[offset]);
        }
        if let Some(default) = obarray.forwarder(resolved).and_then(|forwarder| {
            use super::forward::{LispBufferObjFwd, LispFwdType};

            matches!(forwarder.ty, LispFwdType::BufferObj)
                .then(|| unsafe { (&*(forwarder as *const _ as *const LispBufferObjFwd)).default })
        }) {
            return Some(default);
        }
    }

    obarray.default_value_id(resolved).copied()
}

#[cfg(test)]
mod tests;
