//! Customization and buffer-local variable system.
//!
//! GNU Lisp owns `defcustom`, `defgroup`, `setq-default`, and `custom-*`.
//! The live Rust-side responsibility here is the buffer-local/default-value
//! machinery that the evaluator still needs directly.

use super::error::{EvalResult, Flow, signal};
use super::intern::{SymId, intern, resolve_sym};
use super::value::*;
use crate::buffer::BufferId;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::gc_trace::GcTrace;

/// Rust-side registry for customization state.
///
/// The `auto_buffer_local` `HashSet<SymId>` that used to live here
/// was a pure mirror of the LOCALIZED redirect + BLV `local_if_set`
/// flag. It was removed in Phase D of the symbol-redirect refactor.
/// Readers now consult `Obarray::blv(id).local_if_set` directly.
#[derive(Clone, Debug, Default)]
pub struct CustomManager {}

impl CustomManager {
    pub fn new() -> Self {
        Self {}
    }
}

impl GcTrace for CustomManager {
    fn trace_roots(&self, _roots: &mut Vec<Value>) {}
}

// ---------------------------------------------------------------------------
// Pure builtins (no evaluator needed)
// ---------------------------------------------------------------------------

/// GNU's `KBOARD_OBJFWDP` guard, shared by `Fmake_variable_buffer_local`
/// (`src/data.c:2220-2223`) and `Fmake_local_variable` (`src/data.c:2286-2288`).
///
/// A `DEFVAR_KBOARD` variable's storage is a slot in `struct KBOARD`, which is
/// per-terminal, not per-buffer; there is nowhere for a buffer-local binding
/// of one to live, so both entry points refuse it by name rather than
/// producing a binding that could not be read back.  This is the only
/// Lisp-visible behaviour that separates `Lisp_Fwd_Kboard_Obj` from
/// `Lisp_Fwd_Obj`, which is why the two are separate variants here.
fn keyboard_variable_may_not_be_buffer_local(
    obarray: &crate::emacs_core::symbol::Obarray,
    resolved: crate::emacs_core::intern::SymId,
    reported: Value,
) -> Result<(), Flow> {
    use crate::emacs_core::forward::LispFwdType;
    if obarray.forward_type(resolved) != Some(LispFwdType::KboardObj) {
        return Ok(());
    }
    let name = crate::emacs_core::intern::resolve_sym(reported.as_symbol_id().unwrap_or(resolved));
    Err(signal(
        "error",
        vec![Value::string(format!(
            "Symbol {name} may not be buffer-local"
        ))],
    ))
}

/// `(make-variable-buffer-local VARIABLE)` -- mark variable as automatically buffer-local.
pub(crate) fn builtin_make_variable_buffer_local(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (obarray, custom) = (&mut eval.obarray, &mut eval.custom);
    builtin_make_variable_buffer_local_with_state(obarray, custom, args)
}

pub(crate) fn builtin_make_variable_buffer_local_with_state(
    obarray: &mut crate::emacs_core::symbol::Obarray,
    _custom: &mut CustomManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("make-variable-buffer-local", &args, 1)?;
    let symbol = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };
    let resolved_id = super::builtins::resolve_variable_alias_id_in_obarray(obarray, symbol)?;
    // GNU signals from inside the redirect switch, BEFORE the constant check
    // below (`src/data.c:2220-2223` then `:2230-2231`).
    keyboard_variable_may_not_be_buffer_local(obarray, resolved_id, args[0])?;
    if obarray.is_constant_id(resolved_id) {
        return Err(signal(LispCondition::SettingConstant, vec![args[0]]));
    }

    // Flip the symbol's redirect tag to LOCALIZED and mark it as
    // auto-buffer-local at first set. Mirrors GNU
    // `Fmake_variable_buffer_local` (`data.c:2142-2207`).
    // GNU operates on the exact symbol object after following aliases;
    // it does not intern by name.  This matters for `(make-symbol ...)`
    // variables, whose value cells and BLVs are independent from any
    // interned symbol with the same print name.
    let default_value = obarray.find_symbol_value(resolved_id).unwrap_or(Value::NIL);
    obarray.make_symbol_localized(resolved_id, default_value);
    obarray.set_blv_local_if_set(resolved_id, true);
    Ok(args[0])
}

/// `(make-local-variable VARIABLE)` -- make variable local in current buffer.
///
/// Mirrors GNU `Fmake_local_variable` (`data.c:2209-2312`). Differs
/// from `make-variable-buffer-local` in that it creates a per-buffer
/// binding *only* in the current buffer, without setting
/// `local_if_set` (which would auto-create on every subsequent
/// `setq` in any buffer).
pub(crate) fn builtin_make_local_variable(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("make-local-variable", &args, 1)?;
    let symbol = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };
    let resolved = super::builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, symbol)?;
    // Same refusal, same place in the order (`src/data.c:2286-2288` then
    // `:2293-2294`).
    keyboard_variable_may_not_be_buffer_local(&ctx.obarray, resolved, args[0])?;
    if ctx.obarray.is_constant_id(resolved) {
        return Err(signal(LispCondition::SettingConstant, vec![args[0]]));
    }

    {
        static BUFFER_UNDO_LIST: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
            std::sync::OnceLock::new();
        if resolved == *BUFFER_UNDO_LIST.get_or_init(|| intern("buffer-undo-list")) {
            return Ok(args[0]);
        }
    }

    // Already Localized with a binding in this buffer: nothing left to do
    // (GNU Fmake_local_variable finds the existing `local_var_alist` entry
    // via Fassq and skips the cons; every step below is idempotent state
    // this buffer already has). `setq-local` on flush paths calls this per
    // keystroke — the full re-localization walk was 7.6K Ir per call.
    if ctx.obarray.is_localized(resolved)
        && let Some(buf_id) = ctx.buffers.current_buffer_id()
        && let Some(buf) = ctx.buffers.get(buf_id)
        && buf.has_buffer_local_by_sym_id(resolved)
    {
        return Ok(args[0]);
    }

    // Phase 10E: for FORWARDED BUFFER_OBJFWD symbols, just flip the
    // per-buffer local-flags bit on the current buffer. Mirrors GNU
    // `Fmake_local_variable` SYMBOL_FORWARDED arm at data.c:2263-2272:
    //
    //     if (forwarded && BUFFER_OBJFWDP (valcontents.fwd)) {
    //       int idx = PER_BUFFER_IDX (offset);
    //       eassert (idx);
    //       if (idx > 0)
    //         SET_PER_BUFFER_VALUE_P (current_buffer, idx, true);
    //       return variable;
    //     }
    //
    // The slot remains the source of truth — DO NOT replace it with
    // a fresh BLV via make_symbol_localized.
    {
        use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
        use crate::emacs_core::symbol::SymbolRedirect;
        let buf_objfwd = ctx
            .obarray
            .get_by_id(resolved)
            .filter(|s| s.redirect() == SymbolRedirect::Forwarded)
            .and_then(|s| {
                let fwd = unsafe { &*s.val.fwd };
                if matches!(fwd.ty, LispFwdType::BufferObj) {
                    let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                    crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset)
                } else {
                    None
                }
            });
        if let Some(slot) = buf_objfwd {
            if let Some(buf_id) = ctx.buffers.current_buffer_id()
                && let Some(buf) = ctx.buffers.get_mut(buf_id)
            {
                buf.set_slot_local_flag(slot, true);
            }
            return Ok(args[0]);
        }
    }

    // Phase 6 of the symbol-redirect refactor: flip the symbol to
    // LOCALIZED (preserving its current value as the default) and
    // seed the current buffer's local_var_alist with `(sym . default)`
    // if it doesn't already have an entry. This is the new GNU-shape
    // path.
    //
    // For a void symbol, seed the alist with `Qunbound` as the cdr —
    // mirrors GNU `Fmake_local_variable` which does `Fcons (variable,
    // XCDR (blv->defcell))` at `data.c:2289`, and `blv->defcell` is
    // `(variable . Qunbound)` when the symbol has no value.
    let default_value = ctx
        .obarray
        .find_symbol_value(resolved)
        .unwrap_or(Value::UNBOUND);
    ctx.obarray.make_symbol_localized(resolved, default_value);
    if let Some(current_id) = ctx.buffers.current_buffer_id() {
        let current_buf = Value::make_buffer(current_id);
        if let Some(blv) = ctx.obarray.blv_mut(resolved)
            && crate::emacs_core::value::eq_value(&blv.where_buf, &current_buf)
        {
            // GNU `Fmake_local_variable` calls `swap_in_global_binding`
            // before consing the new `(sym . val)` alist entry when the
            // BLV cache is currently loaded for this buffer.
            blv.where_buf = Value::NIL;
            blv.found = false;
            blv.valcell = blv.defcell;
        }
        if let Some(buf) = ctx.buffers.get_mut(current_id) {
            // Only seed when no entry exists yet (idempotent — calling
            // make-local-variable twice doesn't double-prepend).
            if !buf.has_buffer_local_by_sym_id(resolved) {
                buf.set_buffer_local_by_sym_id(resolved, default_value);
            }
        }
    }
    Ok(args[0])
}

/// `(local-variable-p VARIABLE &optional BUFFER)` -- test if variable is local.
pub(crate) fn builtin_local_variable_p(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("local-variable-p", &args, 1)?;
    expect_max_args("local-variable-p", &args, 2)?;
    let sym_id = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };
    let resolved_id = super::builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, sym_id)?;
    let buf = if args.len() > 1 {
        match args[1].kind() {
            ValueKind::Nil => ctx.buffers.current_buffer(),
            ValueKind::Veclike(VecLikeType::Buffer) => {
                ctx.buffers.get(args[1].as_buffer_id().unwrap())
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("bufferp"), args[1]],
                ));
            }
        }
    } else {
        ctx.buffers.current_buffer()
    };

    let Some(b) = buf else {
        return Ok(Value::NIL);
    };

    // Phase 10E: route LOCALIZED checks through the BLV machinery.
    // Mirrors GNU `Flocal_variable_p` SYMBOL_LOCALIZED arm at
    // `data.c:2399-2412`: walk the buffer's local_var_alist (or
    // trust the BLV cache if `where == buf`).
    use crate::emacs_core::symbol::SymbolRedirect;
    if let Some(sym_slot) = ctx.obarray.get_by_id(resolved_id)
        && sym_slot.redirect() == SymbolRedirect::Localized
    {
        return Ok(Value::bool_val(b.has_buffer_local_by_sym_id(resolved_id)));
    }

    Ok(Value::bool_val(b.has_buffer_local_by_sym_id(resolved_id)))
}

/// `(buffer-local-variables &optional BUFFER)` -- list all local variables.
///
/// Mirrors GNU `Fbuffer_local_variables` (`buffer.c:1453-1520`), which
/// walks `BVAR(buf, local_var_alist)` and `FOR_EACH_PER_BUFFER_OBJECT_AT`
/// and prepends each entry with `Fcons`. The net effect is:
///
///   result = [alist walked forward, prepended]
///            ++ [slots walked forward, prepended]
///
/// which reverses within-group iteration order. Entries whose alist cdr
/// is `Qunbound` are emitted as the bare symbol (no cons) — that's
/// what `(memq SYMBOL (buffer-local-variables))` keys off of.
pub(crate) fn builtin_buffer_local_variables(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("buffer-local-variables", &args, 1)?;

    let id = match args.first() {
        None => ctx
            .buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?,
        Some(v) if v.is_nil() => ctx
            .buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?,
        Some(v) if v.is_buffer() => v.as_buffer_id().unwrap(),
        Some(other) => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("bufferp"), *other],
            ));
        }
    };

    let buf = ctx
        .buffers
        .get(id)
        .ok_or_else(|| signal("error", vec![Value::string("No such live buffer")]))?;

    // Build in GNU prepend order: start with slots (forward iter,
    // appended to a Vec, later reversed), then alist (same pattern).
    // The final entries list ends up as [alist-reversed, slots-reversed]
    // which matches GNU's prepend-based construction.
    let ordered = buf.ordered_buffer_local_bindings();
    let entries: Vec<Value> = ordered
        .into_iter()
        .rev()
        .map(|(sym_id, value)| match value.as_value() {
            Some(value) => Value::cons(Value::from_sym_id(sym_id), value),
            None => Value::from_sym_id(sym_id),
        })
        .collect();
    Ok(Value::list(entries))
}

fn buffer_arg_or_current(
    ctx: &mut super::eval::Context,
    _fn_name: &str,
    arg: Option<Value>,
) -> Result<BufferId, Flow> {
    match arg {
        None => ctx
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(value) => match value.kind() {
            ValueKind::Nil => ctx
                .buffers
                .current_buffer_id()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let id = value.as_buffer_id().expect("buffer value has id");
                if ctx.buffers.get(id).is_some() {
                    Ok(id)
                } else {
                    Err(signal(
                        "error",
                        vec![Value::string("Selecting deleted buffer")],
                    ))
                }
            }
            _ => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("bufferp"), value],
            )),
        },
    }
}

fn local_toplevel_binding_value(
    specpdl: &[super::eval::SpecBinding],
    sym_id: SymId,
    buffer_id: BufferId,
) -> Option<Value> {
    specpdl.iter().find_map(|binding| match binding {
        super::eval::SpecBinding::LetLocal {
            sym_id: binding_sym,
            old_value,
            buffer_id: binding_buffer,
        } if *binding_sym == sym_id && *binding_buffer == buffer_id => Some(*old_value),
        _ => None,
    })
}

fn set_local_toplevel_binding_value(
    specpdl: &mut [super::eval::SpecBinding],
    sym_id: SymId,
    buffer_id: BufferId,
    value: Value,
) -> bool {
    for binding in specpdl.iter_mut() {
        if let super::eval::SpecBinding::LetLocal {
            sym_id: binding_sym,
            old_value,
            buffer_id: binding_buffer,
        } = binding
            && *binding_sym == sym_id
            && *binding_buffer == buffer_id
        {
            *old_value = value;
            return true;
        }
    }
    false
}

/// `(buffer-local-toplevel-value SYMBOL &optional BUFFER)`.
///
/// Mirrors GNU `Fbuffer_local_toplevel_value` (`eval.c`): first require an
/// actual buffer-local binding in the target buffer, then return the saved
/// `SPECPDL_LET_LOCAL` old value if an active `let` is shadowing it.
pub(crate) fn builtin_buffer_local_toplevel_value(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("buffer-local-toplevel-value", &args, 1)?;
    expect_max_args("buffer-local-toplevel-value", &args, 2)?;

    let symbol = super::builtins::symbols::expect_symbol_id(&args[0])?;
    let resolved = super::builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, symbol)?;
    let buffer_id =
        buffer_arg_or_current(ctx, "buffer-local-toplevel-value", args.get(1).copied())?;
    let buffer_value = Value::make_buffer(buffer_id);

    let is_local = builtin_local_variable_p(ctx, vec![args[0], buffer_value])?;
    if is_local.is_nil() {
        return Err(signal(LispCondition::VoidVariable, vec![args[0]]));
    }

    if let Some(value) = local_toplevel_binding_value(ctx.specpdl.as_slice(), resolved, buffer_id) {
        return Ok(value);
    }

    super::builtins::builtin_buffer_local_value(ctx, vec![args[0], buffer_value])
}

/// `(set-buffer-local-toplevel-value SYMBOL VALUE &optional BUFFER)`.
///
/// Mirrors GNU `Fset_buffer_local_toplevel_value`: update the saved
/// `SPECPDL_LET_LOCAL` old value when one is active; otherwise temporarily
/// select the target buffer, make the variable local, and set it there.
pub(crate) fn builtin_set_buffer_local_toplevel_value(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-buffer-local-toplevel-value", &args, 2)?;
    expect_max_args("set-buffer-local-toplevel-value", &args, 3)?;

    let symbol = super::builtins::symbols::expect_symbol_id(&args[0])?;
    let resolved = super::builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, symbol)?;
    let buffer_id =
        buffer_arg_or_current(ctx, "set-buffer-local-toplevel-value", args.get(2).copied())?;
    if set_local_toplevel_binding_value(ctx.specpdl.as_mut_slice(), resolved, buffer_id, args[1]) {
        return Ok(Value::NIL);
    }

    let saved_current = ctx.buffers.current_buffer_id();
    let switched = saved_current != Some(buffer_id);
    if switched {
        ctx.set_current_buffer_unrecorded(buffer_id)?;
    }

    let result = (|| {
        builtin_make_local_variable(ctx, vec![args[0]])?;
        super::eval::set_runtime_binding_in_state(ctx, resolved, args[1])?;
        Ok(Value::NIL)
    })();

    if switched && let Some(saved) = saved_current {
        ctx.restore_current_buffer_if_live(saved);
    }

    result
}

/// `(kill-local-variable VARIABLE)` -- remove local binding in current buffer.
pub(crate) fn builtin_kill_local_variable(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let outcome = builtin_kill_local_variable_impl(ctx, &args)?;
    Ok(outcome.result)
}

pub(crate) struct KillLocalVariableOutcome {
    pub result: Value,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub removed: bool,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub resolved_id: SymId,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub buffer_id: Option<crate::buffer::BufferId>,
}

pub(crate) fn builtin_kill_local_variable_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: &[Value],
) -> Result<KillLocalVariableOutcome, Flow> {
    expect_args("kill-local-variable", args, 1)?;
    let symbol = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };

    let resolved = super::builtins::resolve_variable_alias_id_in_obarray(&ctx.obarray, symbol)?;
    let resolved_name = resolve_sym(resolved);
    let mut removed = false;
    let buffer_id = ctx.buffers.current_buffer_id();

    // GNU `Fkill_local_variable` handles SYMBOL_FORWARDED BUFFER_OBJFWD
    // variables before watcher notification (`data.c:2328-2345`):
    // conditional per-buffer slots clear their local flag and reload the
    // current default value; always-local slots are left alone.
    {
        use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
        use crate::emacs_core::symbol::SymbolRedirect;
        let forwarded_slot = ctx
            .obarray
            .get_by_id(resolved)
            .filter(|s| s.redirect() == SymbolRedirect::Forwarded)
            .and_then(|s| {
                let fwd = unsafe { &*s.val.fwd };
                if matches!(fwd.ty, LispFwdType::BufferObj) {
                    let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                    crate::buffer::buffer::lookup_buffer_slot(resolved_name)
                        .zip(crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset))
                } else {
                    None
                }
            });
        if let Some((info, slot)) = forwarded_slot {
            let offset = slot.index();
            let default_value = ctx.buffers.buffer_defaults.get(offset).copied();
            if info.local_flags_idx >= 0
                && let Some(buffer_id) = buffer_id
                && let Some(buf) = ctx.buffers.get_mut(buffer_id)
            {
                buf.set_slot_local_flag(slot, false);
                if let Some(default_value) = default_value {
                    buf.slots[offset] = default_value;
                }
            }
            return Ok(KillLocalVariableOutcome {
                result: args[0],
                removed: false,
                resolved_id: resolved,
                buffer_id,
            });
        }
    }

    // Phase 10E: for LOCALIZED symbols, remove the entry from
    // `Buffer::local_var_alist` and reset the BLV cache. Mirrors
    // GNU `Fkill_local_variable` SYMBOL_LOCALIZED arm at
    // `data.c:2349-2378` which does:
    //
    //     swap_in_global_binding (sym);
    //     XSETSYMBOL (variable, sym);
    //     bset_local_var_alist (current_buffer,
    //                           Fdelq (Fassq (variable,
    //                                         BVAR (current_buffer, local_var_alist)),
    //                                  BVAR (current_buffer, local_var_alist)));
    use crate::emacs_core::symbol::SymbolRedirect;
    if let Some(buffer_id) = buffer_id {
        let is_localized = ctx
            .obarray
            .get_by_id(resolved)
            .map(|s| s.redirect() == SymbolRedirect::Localized)
            .unwrap_or(false);
        if is_localized {
            // GNU `Fkill_local_variable` notifies watchers before removing the
            // buffer's local alist entry or swapping the BLV back to the
            // global binding, so the callback still observes the local value.
            ctx.run_variable_watchers_by_id_with_where(
                resolved,
                &Value::NIL,
                &Value::NIL,
                "makunbound",
                &Value::make_buffer(buffer_id),
            )?;

            // Reset the BLV cache so subsequent reads re-swap to
            // the global default. Equivalent to GNU's
            // `swap_in_global_binding`.
            if let Some(blv) = ctx.obarray.blv_mut(resolved) {
                blv.where_buf = crate::emacs_core::value::Value::NIL;
                blv.found = false;
                blv.valcell = blv.defcell;
            }
            // Walk the buffer's alist and remove any (sym . val)
            // pair. Returns whether anything was removed.
            if let Some(buf) = ctx.buffers.get_mut(buffer_id) {
                removed = buf.kill_buffer_local_by_sym_id(resolved).is_some();
            }
        } else {
            removed = ctx
                .buffers
                .remove_buffer_local_property(buffer_id, resolved_name)
                .flatten()
                .is_some();
        }
    }

    Ok(KillLocalVariableOutcome {
        result: args[0],
        removed,
        resolved_id: resolved,
        buffer_id,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
