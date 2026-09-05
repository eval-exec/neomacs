//! JIT dispatch and control-flow shims: the direct-builtin dispatch tables, the spec-call C-ABI shims (call_spec, call_subr_spec, pred/arith/cbsym spec, named builtin), and the control-flow shims (throw, save-excursion/restriction, unwind-protect, catch/handler, switch, backedge).
//!
//! Moved out of `compile.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

pub(crate) type JitBuiltin1 = fn(&mut Context, Value) -> Result<Value, Flow>;
pub(crate) type JitBuiltin2 = fn(&mut Context, Value, Value) -> Result<Value, Flow>;
pub(crate) type JitBuiltin3 = fn(&mut Context, Value, Value, Value) -> Result<Value, Flow>;

pub(crate) use crate::emacs_core::builtins as b;

pub(crate) static JIT_BUILTIN1: [JitBuiltin1; 4] = [
    b::builtin_length_1,          // 0
    b::builtin_symbol_value_1,    // 1
    b::builtin_symbol_function_1, // 2
    b::builtin_nreverse_1,        // 3
];

pub(crate) static JIT_BUILTIN2: [JitBuiltin2; 15] = [
    b::builtin_nth_2,          // 0
    b::builtin_nthcdr_2,       // 1
    b::builtin_elt_2,          // 2
    b::builtin_member_2,       // 3
    b::builtin_memq_2,         // 4
    b::builtin_assq_2,         // 5
    b::builtin_equal_2,        // 6
    b::builtin_setcar_2,       // 7
    b::builtin_setcdr_2,       // 8
    b::builtin_aref_2,         // 9
    b::builtin_set_2,          // 10
    b::builtin_fset_2,         // 11
    b::builtin_get_2,          // 12
    b::builtin_string_equal_2, // 13
    b::builtin_string_lessp_2, // 14
];

pub(crate) static JIT_BUILTIN3: [JitBuiltin3; 1] = [
    b::builtin_put_3, // 0
];

/// Slice-shaped builtins (`fn(&[Value]) -> EvalResult`, no Context) — the
/// exact functions the interpreter's `Nconc`/`Concat`/`Substring` arms call.
pub(crate) type JitBuiltinSlice = fn(&[Value]) -> Result<Value, Flow>;

pub(crate) static JIT_BUILTIN_SLICE: [JitBuiltinSlice; 3] = [
    b::builtin_nconc_slice_values, // 0
    b::builtin_concat_slice,       // 1
    b::builtin_substring_slice,    // 2
];

/// `(nargs, table_index)` for ops lowered through the slice-builtin shim.
/// `Concat`'s arity rides in the opcode; `Nconc`/`Substring` are fixed.
pub(crate) fn slice_builtin_spec(op: &Op) -> Option<(usize, usize)> {
    Some(match op {
        Op::Nconc => (2, 0),
        Op::Concat(n) => (*n as usize, 1),
        Op::Substring => (3, 2),
        _ => return None,
    })
}

/// `(table_arity, table_index)` for ops lowered through the generic
/// direct-builtin shims. (There is no longer a per-op "mutates" flag: every op
/// that needs runtime re-entry already sets `needs_rt`, and these ops always
/// route through the precise-deopt path, so there is nothing to poison.)
pub(crate) fn direct_builtin_spec(op: &Op) -> Option<(u8, usize)> {
    Some(match op {
        Op::Length => (1, 0),
        Op::SymbolValue => (1, 1),
        Op::SymbolFunction => (1, 2),
        Op::Nreverse => (1, 3),
        Op::Nth => (2, 0),
        Op::Nthcdr => (2, 1),
        Op::Elt => (2, 2),
        Op::Member => (2, 3),
        Op::Memq => (2, 4),
        Op::Assq => (2, 5),
        Op::Equal => (2, 6),
        Op::Setcar => (2, 7),
        Op::Setcdr => (2, 8),
        Op::Aref => (2, 9),
        Op::Set => (2, 10),
        Op::Fset => (2, 11),
        Op::Get => (2, 12),
        Op::StringEqual => (2, 13),
        Op::StringLessp => (2, 14),
        Op::Put => (3, 0),
        _ => return None,
    })
}

/// Call a unary direct builtin (`JIT_BUILTIN1[idx]`) — the identical function
/// the interpreter arm calls. Roots the argument across the call (builtins may
/// GC); the generated code rooted the rest of its frame.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin1(ctx: *mut u8, idx: i64, a: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN1[idx as usize](ctx, a) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Binary variant of [`neovm_jit_builtin1`].
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin2(ctx: *mut u8, idx: i64, a: i64, b: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let b = Value::from_bits(b as usize);
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        push_scratch_gc_root(b);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN2[idx as usize](ctx, a, b) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Ternary variant of [`neovm_jit_builtin1`].
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin3(
    ctx: *mut u8,
    idx: i64,
    a: i64,
    b: i64,
    c: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let b = Value::from_bits(b as usize);
        let c = Value::from_bits(c as usize);
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        push_scratch_gc_root(b);
        push_scratch_gc_root(c);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN3[idx as usize](ctx, a, b, c) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// `Op::List`: build a list from `n` operand words (the interpreter's
/// `Value::list_from_slice` on the live stack slice). The values are rooted
/// here across the per-cell allocations; the generated code rooted the rest of
/// its frame. Infallible, context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_list(args_ptr: *const i64, nargs: i64) -> i64 {
    let nargs = nargs as usize;
    let saved = save_scratch_gc_roots();
    let mut args: SmallVec<[Value; 8]> = SmallVec::with_capacity(nargs);
    for i in 0..nargs {
        // SAFETY: the generated code stored exactly `nargs` words at
        // `args_ptr` (its call-args stack slot) immediately before this call.
        let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
        push_scratch_gc_root(v);
        args.push(v);
    }
    let result = Value::list_from_slice(&args).bits() as i64;
    restore_scratch_gc_roots(saved);
    result
}

/// Call a slice-shaped direct builtin (`JIT_BUILTIN_SLICE[idx]`) — the
/// identical function the interpreter arm calls (`nconc`/`concat`/
/// `substring`). Roots the operands across the call (they may allocate);
/// context-free like the interpreter's slice calls.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin_slice(
    idx: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(no_ctx, STATUS_SIGNAL, {
        let nargs = nargs as usize;
        let saved = save_scratch_gc_roots();
        let mut args: SmallVec<[Value; 8]> = SmallVec::with_capacity(nargs);
        for i in 0..nargs {
            // SAFETY: see neovm_jit_list — the same spill-slot contract.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            push_scratch_gc_root(v);
            args.push(v);
        }
        let status = match JIT_BUILTIN_SLICE[idx as usize](&args) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Named-builtin dispatch for `Op::CallBuiltin`/`Op::CallBuiltinSym`/
/// `Op::Aset` — re-enters the runtime through the dedicated `Vm::*_for_jit`
/// helpers, which mirror the interpreter arms exactly (override-aware named
/// dispatch for CallBuiltin/Aset, advice-bypassing direct dispatch for
/// CallBuiltinSym, mutating-first-arg string writeback, trailing quit poll).
/// `variant`: 0 = CallBuiltin, 1 = CallBuiltinSym, 2 = Aset.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_named_builtin(
    ctx: *mut u8,
    variant: i64,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let nargs = nargs as usize;
        let saved = save_scratch_gc_roots();
        let mut args = LispArgVec::new();
        for i in 0..nargs {
            // SAFETY: the generated code stored exactly `nargs` words at
            // `args_ptr` (its call-args stack slot) immediately before this call.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            push_scratch_gc_root(v);
            args.push(v);
        }
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let mut vm = Vm::from_context(ctx);
        let result = match variant {
            0 => vm.callbuiltin_for_jit(SymId(sym as u32), args),
            1 => vm.callbuiltinsym_for_jit(SymId(sym as u32), args),
            _ => vm.aset_for_jit(args[0], args[1], args[2]),
        };
        let status = match result {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// `Op::SaveWindowExcursion` (GNU bytecode.c Bsave_window_excursion): pop the
/// body form list, evaluate `(progn . body)` inside a real
/// window-configuration save/restore — the interpreter arm 1:1, including
/// error precedence (a failed restore wins over the body's flow). The body
/// runs arbitrary lisp: everything live is rooted here, the generated code
/// rooted the rest of its frame.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_window_excursion(ctx: *mut u8, body: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let body = Value::from_bits(body as usize);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let root_scope = save_scratch_gc_roots();
        push_scratch_gc_root(body);
        let progn_form = Value::cons(Value::symbol("progn"), body);
        push_scratch_gc_root(progn_form);
        let status = (|| {
            let saved = match crate::emacs_core::builtins::SavedWindowConfiguration::capture(
                ctx,
                Value::NIL,
            ) {
                Ok(saved) => saved,
                Err(flow) => {
                    stash_pending_flow(flow);
                    return STATUS_SIGNAL;
                }
            };
            let body_result = ctx.with_unwind_scope(|ctx| {
                ctx.record_native_unwind(
                    crate::emacs_core::eval::NativeUnwindAction::RestoreWindowConfiguration {
                        configuration: saved,
                        options:
                            crate::emacs_core::builtins::WindowConfigurationRestoreOptions::default(
                            ),
                    },
                );
                ctx.eval_sub(progn_form)
            });
            match body_result {
                Ok(result) => {
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = result.bits() as i64 };
                    STATUS_OK
                }
                Err(flow) => {
                    stash_pending_flow(flow);
                    STATUS_SIGNAL
                }
            }
        })();
        restore_scratch_gc_roots(root_scope);
        status
    })
}

/// Sentinel `SpecSlot::epoch` value marking a site the AOT LOADER left DISARMED
/// because its live-obarray re-classification disagreed with the kind baked into
/// the generated code (see `CompiledLeaf::from_aot`'s arming). A disarmed site
/// must NEVER re-arm: the shims' fast paths are keyed on the BAKED kind (e.g.
/// `neovm_jit_pred_spec`'s hardcoded `is_record` tag test), so re-arming against a
/// now-different binding would run the WRONG op. Both `neovm_jit_call_spec` and
/// `subr_spec_armed` short-circuit to their not-armed (generic / strict-symbol)
/// path on this value BEFORE any re-validate/re-arm. `u64::MAX` is RESERVED: the
/// live obarray `function_epoch` skips it on wrap (`advance_function_epoch`), so a
/// legitimately-armed slot never collides with it. JIT leaves never store it, so
/// the extra compare is a perfectly-predicted never-taken branch there (~0 tax).
pub(crate) const SPEC_EPOCH_DISARMED: u64 = u64::MAX;

/// Speculated direct call (`Op::Call` whose callee slot provably holds a
/// constant symbol that was fbound to a bytecode object at compile time).
/// Quit poll FIRST (the interpreter's Op::Call order — quit processing can run
/// lisp, including fset), then the validity check: if `ctx.obarray`'s
/// function_epoch still equals this site's armed epoch, NO function binding
/// anywhere has changed since the binding was observed equal to `expected`,
/// so the callee object is still reachable through the obarray and calling it
/// directly is exactly equivalent to resolving the symbol — minus the
/// resolution. On an epoch move, re-validate THIS binding: unchanged -> re-arm
/// the slot and proceed direct; changed -> strict symbol call (fset/advice
/// take effect immediately, GNU default-settings parity).
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `slot` points into the
/// owning CompiledLeaf's spec_slots (alive whenever its code runs).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_call_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        // Debug-build evidence that speculation actually engages (tests assert on
        // it; release builds carry no counter).
        #[cfg(debug_assertions)]
        SPEC_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // Build a rooted LispArgVec from the caller's call-args slot — used only by
        // the strict-call fallback paths (call_for_jit), inside their own
        // scratch-root scope. The native-to-native fast path passes `args_ptr`
        // straight through and touches no scratch-root state at all.
        let read_rooted_args = || {
            let mut args = LispArgVec::new();
            for i in 0..nargs {
                // SAFETY: the generated code stored exactly `nargs` argument words
                // at `args_ptr` (its call-args slot) immediately before this call.
                let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
                push_scratch_gc_root(v);
                args.push(v);
            }
            args
        };
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        // Same split as the interpreter's Op::Call poll: the loads-only fast
        // condition runs first; the full poll only when it has work.
        let quit = if ctx.maybe_quit_hot_ok() {
            Ok(())
        } else {
            ctx.maybe_quit()
        };
        let status = match quit {
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
            Ok(()) => {
                // SAFETY: slot points into the executing leaf's spec_slots.
                let slot = unsafe { &*(slot as *const SpecSlot) };
                let slot_epoch = slot.epoch.load(Ordering::Relaxed);
                // A loader-DISARMED site (epoch == SPEC_EPOCH_DISARMED) is
                // permanently not-armed: its baked callee/kind disagreed with the
                // live obarray at load, so re-validating could re-arm the WRONG
                // binding. Check it FIRST (before the epoch load / re-validate) and
                // fall to the strict-symbol-call path. JIT never sets DISARMED, so
                // this compare is perfectly-predicted never-taken there.
                let armed = if slot_epoch == SPEC_EPOCH_DISARMED {
                    false
                } else {
                    let epoch = ctx.obarray.function_epoch();
                    // NEOVM_JIT_FORCE_SLOW_SPEC pretends the armed epoch is stale so
                    // every call exercises the re-validate/re-arm branch.
                    (!jit_force_slow_spec() && slot_epoch == epoch) || {
                        let cur = ctx.obarray.symbol_function_id(SymId(sym as u32));
                        if cur.is_some_and(|v| v.bits() as i64 == expected) {
                            slot.epoch.store(epoch, Ordering::Relaxed);
                            true
                        } else {
                            // The binding changed: drop any cached callee leaf so
                            // a later re-arm can't reuse a stale callee.
                            slot.leaf.store(0, Ordering::Relaxed);
                            false
                        }
                    }
                };
                // Armed: the symbol still names the compile-time bytecode object.
                // Try the fast path (cached leaf, native-to-native pass-through when
                // the callee is a pure fixed-arity match — no arg marshaling at
                // all). Fall back to the strict call on the VALUE if it can't be
                // fast-pathed (arity / not compilable). Not armed: strict call on
                // the SYMBOL (resolves the new binding — fset/advice take effect
                // immediately).
                use crate::emacs_core::jit::cache::NativeCallOutcome;
                let outcome = if armed {
                    let target = Value::from_bits(expected as usize);
                    // No scratch rooting and no Vm construction on the armed
                    // fast path: nothing between the epoch proof and the
                    // callee's backtrace push (which roots the target for the
                    // whole native run) can reach a GC safe point, and
                    // `Vm::from_context`'s eager cache zero-fill was a
                    // measured per-call tax.
                    match Vm::call_armed_callee_native(ctx, target, &slot.leaf, args_ptr, nargs) {
                        Some(o) => o,
                        None => {
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(target);
                            let mut vm = Vm::from_context(ctx);
                            let res = vm.call_for_jit(target, read_rooted_args());
                            restore_scratch_gc_roots(saved);
                            NativeCallOutcome::from_result(res)
                        }
                    }
                } else {
                    let target = Value::from_sym_id(SymId(sym as u32));
                    let saved = save_scratch_gc_roots();
                    push_scratch_gc_root(target);
                    let mut vm = Vm::from_context(ctx);
                    let res = vm.call_for_jit(target, read_rooted_args());
                    restore_scratch_gc_roots(saved);
                    NativeCallOutcome::from_result(res)
                };
                match outcome {
                    NativeCallOutcome::Value(value) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    NativeCallOutcome::FlowStashed => STATUS_SIGNAL,
                    // from_result never produces Fallback, and
                    // call_armed_callee_native resolves its Fallbacks itself.
                    NativeCallOutcome::Fallback => {
                        unreachable!("Fallback outcome at the spec-shim boundary")
                    }
                }
            }
        };
        status
    })
}

/// Predicate discriminator for [`neovm_jit_pred_spec`] (baked as an iconst by
/// the lowering): `recordp`.
pub(crate) const PRED_KIND_RECORDP: i64 = 0;
/// Predicate discriminator for [`neovm_jit_pred_spec`]: `symbol-with-pos-p`.
pub(crate) const PRED_KIND_SYMBOL_WITH_POS_P: i64 = 1;

/// Op discriminators for [`neovm_jit_arith_spec`] (baked as an iconst by the
/// lowering, and — offset by 5 — the [`SpecCalleeKind::to_spec_disc`] value):
/// `logand`.
pub(crate) const ARITH_KIND_LOGAND: i64 = 0;
/// Op discriminator for [`neovm_jit_arith_spec`]: `logior`.
pub(crate) const ARITH_KIND_LOGIOR: i64 = 1;
/// Op discriminator for [`neovm_jit_arith_spec`]: `logxor`.
pub(crate) const ARITH_KIND_LOGXOR: i64 = 2;
/// Op discriminator for [`neovm_jit_arith_spec`]: `ash` (2-arg arithmetic shift).
/// LEFT shifts can overflow fixnum range → the shim bounces to generic (bignum).
pub(crate) const ARITH_KIND_ASH: i64 = 3;
/// Op discriminator for [`neovm_jit_arith_spec`]: `lognot` (1-arg bitwise NOT).
pub(crate) const ARITH_KIND_LOGNOT: i64 = 4;
/// Op discriminator for [`neovm_jit_arith_spec`]: `mod` (2-arg floor-modulo).
/// The fixnum fast path is GC-free: `|a % b| < |b|` and the sign-fixup add
/// stays within the divisor's magnitude, so the result is always a fixnum; a
/// zero divisor bounces to generic (arith-error), non-fixnums (floats,
/// bignums, markers) bounce to generic.
pub(crate) const ARITH_KIND_MOD: i64 = 5;

/// Classify a callee SYMBOL name + arity as a bitwise-arithmetic intrinsic op,
/// or `None`. Shared by [`subr_spec_kind`] (which additionally proves the live
/// binding is the real subr) and the profitability gate's arith-intrinsic scan
/// (name-only). The intrinsifiable forms are the ones whose fixnum fast path is
/// GC-free (or bounces to generic when it would allocate):
///
/// * `logand`/`logior`/`logxor` (2 args) — bitwise of two fixnums is always a
///   fixnum, never overflows (these are `ManySlice` → full generic dispatch
///   today, the biggest win).
/// * `ash` (2 args) — the interpreter has NO fixnum fast path (it always
///   materializes a bignum), so a fixnum shift is a real win; LEFT-shift overflow
///   bounces to generic.
/// * `lognot` (1 arg) — `!n` of a fixnum is always a fixnum.
/// * `mod` (2 args) — floor-modulo of two fixnums is always a fixnum
///   (`|a % b| < |b|`, and the sign-fixup add stays below the divisor's
///   magnitude); a zero divisor bounces to generic (arith-error).
///
/// Other arities stay on the generic path (0 → identity const, ≥3 → reduction).
/// `lsh` is deliberately absent: it is an elisp `defun` (subr.el), not a subr,
/// so it takes the Bytecode spec path and its `ash` call intrinsifies transitively.
pub(crate) fn arith_intrinsic_op_by_name(name: &str, nargs: usize) -> Option<u8> {
    let op = match (name, nargs) {
        ("logand", 2) => ARITH_KIND_LOGAND,
        ("logior", 2) => ARITH_KIND_LOGIOR,
        ("logxor", 2) => ARITH_KIND_LOGXOR,
        ("ash", 2) => ARITH_KIND_ASH,
        ("lognot", 1) => ARITH_KIND_LOGNOT,
        ("mod", 2) => ARITH_KIND_MOD,
        _ => return None,
    };
    Some(op as u8)
}

/// Shared arming check for the three subr spec shims: TRUE iff the site's
/// direct fast path is still valid — the symbol's function cell (validated via
/// the per-site epoch, re-validated on any epoch move exactly like
/// [`neovm_jit_call_spec`]) still holds the compile-time subr VALUE, and no
/// compiler function overrides are active. The overrides check has no
/// bytecode-spec counterpart but is REQUIRED here for interpreter parity: the
/// generic path's `direct_subr_call_target` refuses direct subr dispatch for a
/// SYMBOL callee while overrides are active (they shadow function cells in
/// `resolve_named_call_target_by_id` and live in a VARIABLE, invisible to
/// `function_epoch`), so an armed site must bounce to the generic block then
/// too. Never allocates, never runs lisp — callers rely on this being GC-free.
#[inline]
pub(crate) fn subr_spec_armed(ctx: &Context, sym: i64, expected: i64, slot: &SpecSlot) -> bool {
    let slot_epoch = slot.epoch.load(Ordering::Relaxed);
    // A loader-DISARMED site never re-arms (see SPEC_EPOCH_DISARMED /
    // neovm_jit_call_spec): the pred/eq shims' fast paths are keyed on the BAKED
    // kind, so re-arming a now-different binding would run the WRONG op. Check it
    // FIRST; JIT never sets DISARMED (perfectly-predicted here).
    if slot_epoch == SPEC_EPOCH_DISARMED {
        return false;
    }
    if ctx.compiler_function_overrides_active() {
        return false;
    }
    let epoch = ctx.obarray.function_epoch();
    // NEOVM_JIT_FORCE_SLOW_SPEC pretends the armed epoch is stale so every
    // call exercises the re-validate/re-arm branch.
    (!jit_force_slow_spec() && slot_epoch == epoch) || {
        let cur = ctx.obarray.symbol_function_id(SymId(sym as u32));
        if cur.is_some_and(|v| v.bits() as i64 == expected) {
            slot.epoch.store(epoch, Ordering::Relaxed);
            true
        } else {
            // The subr shims never populate `slot.leaf`; nothing to clear.
            false
        }
    }
}

/// Speculated direct SUBR call (`Op::Call` whose callee slot provably holds a
/// constant symbol fbound at compile time to a fixed-arity builtin subr — see
/// `find_spec_sites`' subr classification). Quit poll FIRST (interpreter
/// `Op::Call` order), then the validity check ([`subr_spec_armed`]):
///
/// * ARMED — the cell still holds the compile-time `#<subr>` VALUE. Dispatch
///   it directly via `Vm::call_spec_subr_stack`, which replicates the generic
///   path's subr protocol exactly (depth guard, backtrace frame recording the
///   SYMBOL, arity signal against the FRESH entry, A0..A8 stack-args dispatch,
///   debugger hook) minus the symbol resolution. The `SubrEntry` is re-read
///   from the subr object on EVERY call: `update_static_subr_object_entry`
///   rewrites entries IN PLACE keeping the value bits identical, so the fn
///   pointer must never be cached across calls (only the stable VALUE bits are
///   baked).
/// * NOT ARMED — return [`STATUS_NEED_GENERIC`] with no side effects; the
///   generated fallback block re-does this site as a plain generic call on the
///   SYMBOL (fset/advice/overrides take effect immediately, GNU parity).
///
/// GC rooting: the args are pushed onto the GC-traced `bc_buf` (rooted across
/// the subr call, which can allocate/GC/signal) — the same rooting scheme as
/// [`neovm_jit_call`]; the generated code rooted its residual operand stack
/// before this call. The recursion-depth guard is applied inside
/// `call_spec_subr_stack` via `with_bytecode_call_depth`, exactly where
/// `call_for_jit_stack` applies it on the generic path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `slot` points into the
/// owning CompiledLeaf's spec_slots (alive whenever its code runs).
///
/// AOT-importable (increment B2): the `Op::Call` subr spec sites are now emitted
/// by the AOT baseline tier too (`build_baseline_leaf_object` with `Some(obarray)`),
/// so an AOT `.so` may import this symbol. It is therefore host-exported
/// (`#[unsafe(no_mangle)] pub`) + in `shim_names.rs`/`MIR_SHIM_NAMES` (salted into
/// `ABI_TAG`) + anchored by `JIT_SHIM_ANCHOR`. Cross-session soundness rides on the
/// per-site epoch guard + the loader's DISARM sentinel (`SPEC_EPOCH_DISARMED`), not
/// on any baked address — the shim re-reads the live entry every call.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_call_subr_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if !subr_spec_armed(ctx, sym, expected, slot) {
            #[cfg(debug_assertions)]
            SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let target = Value::from_bits(expected as usize);
        let saved = save_scratch_gc_roots();
        // Push the args straight onto bc_buf (GC-traced → rooted across the subr,
        // and the stack-args dispatcher reads them in place — no LispArgVec). The
        // callee needs no root: static subr objects are Box::leak'd, never freed.
        let args_start = ctx.bc_buf.len();
        for i in 0..nargs {
            // SAFETY: the generated code stored exactly `nargs` argument words at
            // `args_ptr` (its call-args slot) immediately before this call.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            ctx.bc_buf.push(v);
        }
        let mut vm = Vm::from_context(ctx);
        let res = vm.call_spec_subr_stack(SymId(sym as u32), target, args_start, nargs);
        vm.bc_buf_truncate(args_start);
        let status = match res {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Speculated PREDICATE call: `recordp` / `symbol-with-pos-p` sites collapse
/// to a hardcoded tag test when armed. Both predicates are pure single-tag
/// checks, verified exact against their builtins (`builtin_recordp_1` =
/// `Value::is_record`, `builtin_symbol_with_pos_p_1` =
/// `Value::is_symbol_with_pos`; both independent of
/// `symbols-with-pos-enabled` — which is why `keywordp`/`symbolp` are General
/// only). NOT ARMED → [`STATUS_NEED_GENERIC`]; the generated fallback block
/// runs the plain generic call (full rooting, override/redefinition parity).
///
/// GC-FREE INVARIANT (load-bearing): the generated code does NOT root its
/// residual operand stack around this call, so no path here may allocate on
/// the lisp heap or run lisp code. That holds: `maybe_quit`'s quit/throw Flow
/// construction is pure Rust-heap (`signal(LispCondition::Quit, vec![])` — no lisp
/// allocation), `subr_spec_armed` only reads, and the armed test is a tag
/// check on an immediate/heap header. The skipped backtrace frame is
/// unobservable: the armed path cannot signal, GC, or run lisp between frame
/// push and pop.
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_pred_spec(
    ctx: *mut u8,
    kind: i64,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if !subr_spec_armed(ctx, sym, expected, slot) {
            #[cfg(debug_assertions)]
            SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let v = Value::from_bits(a as usize);
        // `is_record`/`is_symbol_with_pos` = tag check BEFORE any header deref
        // (`veclike_type` guards on `is_veclike` first) — safe on immediates.
        let truth = if kind == PRED_KIND_RECORDP {
            v.is_record()
        } else {
            debug_assert_eq!(kind, PRED_KIND_SYMBOL_WITH_POS_P);
            v.is_symbol_with_pos()
        };
        let result = if truth { Value::T } else { Value::NIL };
        // SAFETY: `out` is the generated code's result stack slot.
        unsafe { *out = result.bits() as i64 };
        STATUS_OK
    })
}

/// Speculated `equal-including-properties` call (2 args): when armed and the
/// arguments are BITWISE equal, the answer is `t` with zero dispatch —
/// bit-equal ⟹ same object ⟹ equal-including-properties, which is literally
/// the builtin's own first check (`try_equal_value_inner`'s
/// `left.bits() == right.bits()`), so this covers same-object strings/conses,
/// identical NaN boxes, fixnums, and interned symbols alike.
///
/// On a bitwise MISS the shim returns [`STATUS_NEED_GENERIC`] instead of
/// invoking the builtin here: the deep comparison can allocate (its
/// depth-overflow arm signals with a freshly allocated lisp string), and this
/// site's direct path deliberately skips the residual-stack rooting — calling
/// the builtin from here could GC with the caller's live registers unrooted.
/// The generic fallback block does the fully-rooted call instead; the miss
/// cost is one epoch check + bit compare on top of a deep structural
/// comparison, i.e. noise. This keeps the same GC-FREE INVARIANT as
/// [`neovm_jit_pred_spec`] (no allocation / no lisp on ANY path here).
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_eq_incl_props_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    b: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if subr_spec_armed(ctx, sym, expected, slot) && a == b {
            #[cfg(debug_assertions)]
            SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
            // SAFETY: `out` is the generated code's result stack slot.
            unsafe { *out = Value::T.bits() as i64 };
            return STATUS_OK;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        STATUS_NEED_GENERIC
    })
}

/// Fixnum fast path for `(ash value count)` — mirrors GNU `Fash` /
/// `builtin_ash_slice` for the both-fixnum case, or `None` when the result would
/// leave fixnum range (so the shim bounces to the allocating generic path):
///
/// * `count == 0` → `value` unchanged.
/// * `count > 0` (left shift) → `value << count`, but ONLY when it is exactly
///   reversible (`(sh >> count) == value` rejects any bit lost to i64 overflow)
///   AND lands in fixnum range; otherwise `None` (the real result is a bignum).
///   `count >= 64` is rejected up front (an i64 shift by ≥64 is undefined).
/// * `count < 0` (arithmetic right shift, floor toward −∞ = `mpz_fdiv_q_2exp`) →
///   always a fixnum (magnitude only shrinks); huge shifts clamp to a 63-bit
///   sign fill (−1 for negative `value`, 0 otherwise), matching GNU.
pub(crate) fn ash_fixnum_fast(value: i64, count: i64) -> Option<i64> {
    if count == 0 {
        Some(value)
    } else if count > 0 {
        if count >= 64 {
            return None;
        }
        let sh = value.checked_shl(count as u32)?;
        if (sh >> count) == value
            && (Value::MOST_NEGATIVE_FIXNUM..=Value::MOST_POSITIVE_FIXNUM).contains(&sh)
        {
            Some(sh)
        } else {
            None
        }
    } else {
        // count < 0. `-count` is safe (count ≥ MOST_NEGATIVE_FIXNUM > i64::MIN).
        let bits = if count <= -63 { 63 } else { (-count) as u32 };
        Some(value >> bits)
    }
}

/// Speculated bitwise-arithmetic call: `logand`/`logior`/`logxor`/`ash` (2 args)
/// and `lognot` (1 arg). When armed AND the argument(s) are fixnums, computes the
/// native op with zero dispatch — EXACTLY the interpreter's fixnum semantics:
/// `&`/`|`/`^` (`builtin_logand_slice` et al., always a fixnum, never overflows),
/// `!n` (`builtin_lognot`, always a fixnum), and the fixnum `ash` fast path
/// ([`ash_fixnum_fast`], which the interpreter LACKS — it always materializes a
/// bignum). `Value::fixnum` never overflows on these results and the shim never
/// allocates.
///
/// Anything else — NOT armed (callee redefined), an arg not a fixnum (marker /
/// bignum / wrong-type), or an `ash` LEFT-shift whose result leaves fixnum range
/// — returns [`STATUS_NEED_GENERIC`], and the generated fallback runs the plain
/// generic call reaching the SAME builtin body (bignum via GMP, marker→position,
/// wrong-type signal, `ash` overflow→bignum/overflow-error). The
/// deep/allocating cases therefore only run on the fully-rooted fallback path,
/// keeping the GC-FREE INVARIANT of [`neovm_jit_eq_incl_props_spec`] (no
/// allocation / no lisp on ANY path here; `maybe_quit` is Rust-heap only).
/// `op` is baked as an iconst (`ARITH_KIND_*`); each op also has a distinct
/// [`SpecCalleeKind::to_spec_disc`], so an AOT site whose baked op no longer
/// matches the live callee is disarmed by the loader (never run with a wrong op).
/// For `lognot` (1 arg) the generated code passes a dummy `b`; the shim ignores it.
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_arith_spec(
    ctx: *mut u8,
    kind: i64,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    b: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if subr_spec_armed(ctx, sym, expected, slot) {
            // Single `match kind` (jump table) so the hot and/or/xor path has no
            // extra dispatch. `None` on any non-fixnum arg or an `ash` overflow ->
            // the generic bounce below. lognot is 1-arg (dummy `b`, ignored); the
            // 2-arg ops read both operands only inside their own arm.
            let fix2 = || match (
                Value::from_bits(a as usize).as_fixnum(),
                Value::from_bits(b as usize).as_fixnum(),
            ) {
                (Some(l), Some(r)) => Some((l, r)),
                _ => None,
            };
            let res: Option<i64> = match kind {
                ARITH_KIND_LOGAND => fix2().map(|(l, r)| l & r),
                ARITH_KIND_LOGIOR => fix2().map(|(l, r)| l | r),
                ARITH_KIND_LOGXOR => fix2().map(|(l, r)| l ^ r),
                ARITH_KIND_ASH => fix2().and_then(|(l, r)| ash_fixnum_fast(l, r)),
                // GNU Fmod integer branch: truncated rem, then pull the
                // result onto the divisor's side of zero.
                ARITH_KIND_MOD => fix2().and_then(|(l, r)| {
                    if r == 0 {
                        return None;
                    }
                    let m = l % r;
                    Some(if m != 0 && ((m < 0) != (r < 0)) {
                        m + r
                    } else {
                        m
                    })
                }),
                _ => {
                    debug_assert_eq!(kind, ARITH_KIND_LOGNOT);
                    Value::from_bits(a as usize).as_fixnum().map(|n| !n)
                }
            };
            if let Some(res) = res {
                #[cfg(debug_assertions)]
                SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = Value::fixnum(res).bits() as i64 };
                return STATUS_OK;
            }
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        STATUS_NEED_GENERIC
    })
}

/// R2 Tier-B CallBuiltinSym intrinsic (dispatch-skip): reproduce the
/// `Op::CallBuiltinSym` interpreter arm EXACTLY, skipping only the
/// `resolve_sym` → `builtin_name_id` → `dispatch_vm_builtin_unrooted`
/// special-name string-switch round trip. CallBuiltinSym name-dispatches the
/// STATIC subr table (`subr_from_sym_id(builtin_name_id(resolve_sym(sym)))`,
/// vm.rs) and is advice/fset/override IMMUNE, so there is NO epoch guard and NO
/// `compiler_function_overrides_active` gate: the shim RE-READS the live static
/// entry each call (`lookup_global_subr_entry`, safe against in-place rewrites)
/// and either
///
/// * `Some` + `dispatch_kind == Builtin` — dispatches via `funcall_general` on
///   the name-canonical SUBR value (`subr_from_sym_id(sym)`; canonicalizes a
///   name-alias, `Box::leak` process-stable). `funcall_general` IS the arm's
///   own dispatch (vm.rs → `dispatch_vm_builtin_unrooted` → `funcall_general`),
///   so this is byte-identical: a SUBR-value backtrace frame, the arity check
///   vs the FRESH entry signalling `wrong-number-of-arguments` with the SUBR
///   payload, exact-slice args (`Many` gets `into_vec`; fixed arity nil-pads
///   after the arity gate), the debugger `dispatch_signal_result_if_needed`,
///   and `unbind_to_with_result` — WITHOUT `with_bytecode_call_depth` (open-
///   coded ops add no lisp-eval-depth level). The `maybe_quit` poll runs AFTER
///   the op (GNU `Op::CallBuiltinSym` order), only on the Ok path. The R2 ship
///   set excludes `aset`/`fillarray`, so the arm's mutating-first-arg writeback
///   never applies and is correctly absent here.
/// * anything else — [`STATUS_NEED_GENERIC`]. The per-site generated fallback
///   re-runs the ORIGINAL general CBSym lowering (`neovm_jit_named_builtin`
///   variant 1 → `Vm::callbuiltinsym_for_jit`), which reproduces
///   void-function / invalid-function / the special-name arm exactly. Deferring
///   every edge case there keeps THIS shim's correctness obligation to the
///   clean `Some`+`Builtin` case only.
///
/// GC rooting: args are pushed onto the GC-traced `bc_buf` (rooted across the
/// subr, which can allocate/GC/signal) — the [`neovm_jit_call_subr_spec`]
/// scheme; the generated code rooted its residual operand stack via
/// gc_save/gc_push before this call, and `funcall_general` additionally roots
/// the args in its backtrace frame. `subr_from_sym_id`'s canonical subr object
/// is `Box::leak`'d, so the callee value needs no root.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
///
/// R2 increment A (CBSym-in-AOT): EXPORTED (`#[unsafe(no_mangle)] pub` + listed
/// in `shim_names.rs`/`MIR_SHIM_NAMES` + anchored by `JIT_SHIM_ANCHOR`). The
/// CBSym classification (`cbsym_spec_kind`) is name-canonical + obarray-free
/// (static subr table + name resolution only), so the AOT baseline emit
/// (`build_baseline_leaf_object`, `obarray=None`) now classifies CBSym sites too —
/// an AOT `.so` may import this shim and binds it against the host at `dlopen`.
/// (The round-1 `Op::Call` subr-spec shims stay JIT-only: their `find_spec_sites`
/// pass still requires `Some(obarray)` — that's increment B.)
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_cbsym_spec(
    ctx: *mut u8,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let sym_id = SymId(sym as u32);
        // Advice/fset/override-immune: NO epoch guard. Re-read the live static entry
        // (in-place-rewrite safe). Not a plain builtin now → bounce so the general
        // shim reproduces void/invalid-function exactly. FORCE_CBSYM_GENERIC forces
        // every classified site down this bounce (harness).
        // GNU's inline opcodes (bytecode.c `Bpoint`..`Bwiden`,
        // `Bset_marker`..`Bdowncase`) call the C primitive directly: no
        // funcall, no backtrace frame, no arity check. Dispatch the registered
        // builtin straight off the argument slot the same way the interpreter's
        // `Op::CallBuiltinSym` arm does; the VM-owned specials and anything not
        // registered as a plain builtin bounce to the generic shim.
        let function = if force_cbsym_generic() {
            None
        } else {
            Vm::inline_builtin_function(sym_id)
        };
        let Some(function) = function else {
            #[cfg(debug_assertions)]
            CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        };
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let saved = save_scratch_gc_roots();
        let args_start = ctx.bc_buf.len();
        for i in 0..nargs {
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            ctx.bc_buf.push(v);
        }
        let res = Vm::call_inline_builtin_from_stack(ctx, function, sym_id, args_start, nargs);
        ctx.bc_buf.truncate(args_start);
        let status = match res {
            Ok(value) => {
                // Quit poll AFTER the op (GNU Op::CallBuiltinSym order; the arm polls
                // maybe_quit only once the result is computed, and never on the
                // error path).
                match ctx.maybe_quit() {
                    Ok(()) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    Err(flow) => {
                        stash_pending_flow(flow);
                        STATUS_SIGNAL
                    }
                }
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// The fixed arity of a Tier-A CallBuiltinSym read (all 0-arg except
/// match-beginning/match-end, which take the group number). The shim bounces to
/// STATUS_NEED_GENERIC on an arity mismatch so the general path signals
/// wrong-number-of-arguments identically to the interpreter.
#[inline]
pub(crate) fn cbsym_read_expected_nargs(which: u8) -> usize {
    match which {
        CBSYM_A_MATCH_BEGINNING | CBSYM_A_MATCH_END => 1,
        _ => 0,
    }
}

/// R2 Tier-A CallBuiltinSym intrinsic (GC-free read): DELEGATE to the REAL
/// builtin body for `which`, never reimplement — a byte→char conversion
/// (match-beginning) or a three-case boundary (bolp) can then never drift. Every
/// OK path returns an IMMEDIATE (fixnum / t / nil / a pre-materialized buffer
/// value), so the shim needs NO residual-stack rooting (the generated code does
/// not root it — like the round-1 predicate shims); an Err returns STATUS_SIGNAL,
/// which DISCARDS the unrooted residual stack (no UAF even if the body's error
/// path allocates). NO epoch guard (CallBuiltinSym name-dispatches the static
/// table; advice/fset/override immune): re-read the live entry and bounce to
/// STATUS_NEED_GENERIC when it is no longer a plain builtin, when the arity is
/// unexpected, or under the FORCE_CBSYM_GENERIC harness — the general shim
/// reproduces void/invalid-function / wrong-number-of-arguments exactly.
///
/// `current-buffer` is the MISSED-HAZARD case: `builtin_current_buffer` calls
/// `make_buffer` (ALLOCATES → would corrupt the unrooted residual stack), so this
/// shim reads the ALREADY-materialized buffer value from the heap's buffer
/// registry (non-allocating) and bounces to STATUS_NEED_GENERIC when it was never
/// materialized (the general path materializes it, with a rooted residual stack).
///
/// ANTI-REQUIREMENT (upheld by the lowering): the JIT must NEVER value-number or
/// cache buffer state (current buffer / point / BEGV / ZV) across a CallBuiltinSym
/// op — `set-buffer`/`insert`/`goto-char`/`widen` change it — so every CBSym op
/// stays an opaque shim call, re-reading state each time.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
///
/// R2 increment A (CBSym-in-AOT): EXPORTED (`#[unsafe(no_mangle)] pub` + listed
/// in `shim_names.rs`/`MIR_SHIM_NAMES` + anchored by `JIT_SHIM_ANCHOR`). Tier-A
/// classification is name-canonical + obarray-free, so the AOT baseline emit
/// (`obarray=None`) now emits Tier-A sites too — an AOT `.so` may import this shim
/// and binds it against the host at `dlopen`.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_cbsym_read(
    ctx: *mut u8,
    which: i64,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let which = which as u8;
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let sym_id = SymId(sym as u32);
        // Advice/fset/override-immune: NO epoch guard. Bounce (general path) when the
        // static entry is no longer a plain builtin, the arity is unexpected, or the
        // harness forces it.
        let armed = !force_cbsym_generic()
            && nargs == cbsym_read_expected_nargs(which)
            && lookup_global_subr_entry(sym_id)
                .is_some_and(|e| e.dispatch_kind == SubrDispatchKind::Builtin);
        if !armed {
            #[cfg(debug_assertions)]
            CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        // current-buffer: NEVER allocate. Read the already-materialized buffer value;
        // bounce when it was never made (general path -> make_buffer, rooted).
        if which == CBSYM_A_CURRENT_BUFFER {
            let Some(buf_id) = ctx.buffers.current_buffer().map(|b| b.id) else {
                #[cfg(debug_assertions)]
                CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
                return STATUS_NEED_GENERIC;
            };
            return match crate::tagged::gc::with_tagged_heap(|h| h.buffer_value(buf_id)) {
                Some(v) => {
                    #[cfg(debug_assertions)]
                    CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = v.bits() as i64 };
                    STATUS_OK
                }
                None => {
                    #[cfg(debug_assertions)]
                    CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
                    STATUS_NEED_GENERIC
                }
            };
        }
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        // DELEGATE to the builtin body (GC-free OK path). match-beginning/end read
        // ctx.match_data through the same published-register interface, so a
        // separate register-read reimplementation could drift from Lisp.
        use crate::emacs_core::builtins::search;
        use crate::emacs_core::{editfns, navigation};
        let res = match which {
            CBSYM_A_POINT => crate::emacs_core::buffer::builtin_point_0(ctx),
            CBSYM_A_POINT_MIN => crate::emacs_core::buffer::builtin_point_min_0(ctx),
            CBSYM_A_POINT_MAX => crate::emacs_core::buffer::builtin_point_max_0(ctx),
            CBSYM_A_BOLP => navigation::builtin_bolp(ctx, Vec::new()),
            CBSYM_A_EOLP => navigation::builtin_eolp(ctx, Vec::new()),
            CBSYM_A_BOBP => navigation::builtin_bobp(ctx, Vec::new()),
            CBSYM_A_EOBP => navigation::builtin_eobp(ctx, Vec::new()),
            CBSYM_A_FOLLOWING_CHAR => editfns::builtin_following_char_0(ctx),
            CBSYM_A_PRECEDING_CHAR => editfns::builtin_preceding_char(ctx, Vec::new()),
            // Tier-A char-after is 0-arg (nargs gated above): reads at point.
            CBSYM_A_CHAR_AFTER => crate::emacs_core::buffer::builtin_char_after(ctx, Vec::new()),
            CBSYM_A_MATCH_BEGINNING => {
                // SAFETY: the generated code stored exactly nargs==1 word at args_ptr.
                let group = Value::from_bits(unsafe { *args_ptr } as usize);
                search::builtin_match_beginning_with_state(&ctx.match_data, &[group])
            }
            CBSYM_A_MATCH_END => {
                // SAFETY: the generated code stored exactly nargs==1 word at args_ptr.
                let group = Value::from_bits(unsafe { *args_ptr } as usize);
                search::builtin_match_end_with_state(&ctx.match_data, &[group])
            }
            // Unknown discriminant (unreachable for a classified site): bounce.
            _ => return STATUS_NEED_GENERIC,
        };
        // Run the SAME signal post-processing the interpreter arm gets for free via
        // funcall_general (`dispatch_signal_result_if_needed`): a no-op on Ok (stays
        // GC-free), and on a signal it runs the debugger dispatch + sets
        // `search_complete` — so the Err Flow is byte-identical to the interpreter's
        // (which routes match-beginning/end through funcall_general). The residual
        // stack is discarded on STATUS_SIGNAL, so any allocation here is UAF-safe.
        let res = ctx.dispatch_signal_result_if_needed(res);
        match res {
            Ok(value) => {
                // Quit poll AFTER (GNU CallBuiltinSym order). maybe_quit's Flow is
                // Rust-heap, so this stays GC-free.
                match ctx.maybe_quit() {
                    Ok(()) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    Err(flow) => {
                        stash_pending_flow(flow);
                        STATUS_SIGNAL
                    }
                }
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        }
    })
}

/// `Op::Throw`: stash `Flow::Throw{tag, value}` for the signal-exit path.
/// Compiled bodies have no local handlers (handler opcodes bail), so a throw
/// always propagates out — exactly the interpreter's `resume_nonlocal` once no
/// local handler matches. Context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_throw(tag: i64, value: i64) {
    jit_shim_contain!(no_ctx, (), {
        stash_pending_flow(Flow::throw(
            Value::from_bits(tag as usize),
            Value::from_bits(value as usize),
        ));
    })
}

/// Slow path for `integerp` when the value isn't a fixnum: bignums are
/// veclikes, so delegate to the value layer's own predicate. Context-free,
/// pure, never allocates or signals.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_integerp_slow(v: i64) -> i64 {
    let v = Value::from_bits(v as usize);
    (if v.is_integer() {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

/// Slow path for `numberp` when the value isn't a fixnum (floats, bignums).
/// Context-free, pure, never allocates or signals.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_numberp_slow(v: i64) -> i64 {
    let v = Value::from_bits(v as usize);
    (if v.is_number() {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

/// `Op::SaveCurrentBuffer`: record the current buffer on the specpdl + the
/// bind stack, exactly like the interpreter arm (conditional + infallible).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_current_buffer(ctx: *mut u8) {
    use crate::emacs_core::eval::SpecBinding;
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(buffer_id) = ctx.buffers.current_buffer().map(|buffer| buffer.id) {
        JIT_BIND_STACK.with(|s| s.borrow_mut().push(ctx.specpdl.len()));
        ctx.specpdl
            .push(SpecBinding::SaveCurrentBuffer { buffer_id });
    }
}

/// `Op::SaveExcursion`: record point/mark/buffer via the same Context helper
/// the interpreter uses (`record_save_excursion` pushes the specpdl record and
/// returns the pre-push depth for the bind stack).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_excursion(ctx: *mut u8) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(count) = ctx.record_save_excursion() {
        JIT_BIND_STACK.with(|s| s.borrow_mut().push(count));
    }
}

/// `Op::SaveRestriction`: record the narrowing state, exactly like the
/// interpreter arm (conditional + infallible).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_restriction(ctx: *mut u8) {
    use crate::emacs_core::eval::SpecBinding;
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(saved) = ctx.buffers.save_current_restriction_state() {
        JIT_BIND_STACK.with(|s| s.borrow_mut().push(ctx.specpdl.len()));
        ctx.specpdl.push(SpecBinding::save_restriction(saved));
    }
}

/// `Op::UnwindProtectPop`: register an unwind-protect cleanup form as a
/// specpdl record (the interpreter arm mirrored 1:1 — same `SpecBinding`
/// entry, same captured lexenv). The cleanup runs whenever `unbind_to` crosses
/// it: the matching `Unbind`, or the frame unwind on any exit — shared
/// machinery with the interpreter, including the signal path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_unwind_protect(ctx: *mut u8, forms: i64) {
    use crate::emacs_core::eval::SpecBinding;
    let forms = Value::from_bits(forms as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    JIT_BIND_STACK.with(|s| s.borrow_mut().push(ctx.specpdl.len()));
    let lexenv = ctx.lexenv;
    ctx.specpdl
        .push(SpecBinding::UnwindProtect { forms, lexenv });
}

/// `Op::PushConditionCase`: register a `condition-case` handler frame on the
/// ctx-level condition stack, mirroring the interpreter arm exactly — implicit
/// `error` conditions, a `VmConditionCase` resume carrying the bytecode target,
/// the static operand-stack depth at the push, the current specpdl depth, and
/// the current JIT bind-stack length (this frame's analogue of the
/// interpreter's frame-local `bind_stack`). Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_cc(ctx: *mut u8, target: i64, stack_len: i64) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = JIT_BIND_STACK.with(|s| s.borrow().len());
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::ConditionCase {
        conditions: Value::symbol("error"),
        resume: ResumeTarget::VmConditionCase {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PushConditionCaseRaw`: like [`neovm_jit_push_cc`] but the handler
/// pattern (conditions) was popped from the operand stack by the generated
/// code and is passed in. Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_cc_raw(
    ctx: *mut u8,
    target: i64,
    stack_len: i64,
    conditions: i64,
) {
    let conditions = Value::from_bits(conditions as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = JIT_BIND_STACK.with(|s| s.borrow().len());
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::ConditionCase {
        conditions,
        resume: ResumeTarget::VmConditionCase {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PushCatch`: register a `catch` frame (tag popped by the generated
/// code), mirroring the interpreter arm. Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_catch(ctx: *mut u8, target: i64, stack_len: i64, tag: i64) {
    let tag = Value::from_bits(tag as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = JIT_BIND_STACK.with(|s| s.borrow().len());
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::Catch {
        tag,
        resume: ResumeTarget::VmCatch {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PopHandler`: drop the innermost handler frame (normal exit from a
/// protected extent). The static handler-depth analysis guarantees balance.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_pop_handler(ctx: *mut u8) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    ctx.pop_condition_frame();
}

/// Handler-match dispatch: called on the cold path after a runtime call inside
/// a protected extent returned [`STATUS_SIGNAL`], with `ours` = the number of
/// condition frames this *native frame* has active at the site (static). The
/// per-frame invariant "callees pop their own frames on every exit" means the
/// top `ours` frames of `ctx.condition_stack` are exactly ours.
///
/// Mirrors `Vm::resume_nonlocal` 1:1:
/// - `Throw`: select via `matching_catch_resume` (whole-stack scan, like the
///   interpreter); pop our frames innermost-first looking for the selected
///   resume. Found -> unwind (`unbind_to` to the frame's spec depth, truncate
///   the JIT bind stack), write the thrown value through `out`, and return the
///   0-based miss count `m` (0 = innermost handler matched). Selected-but-outer
///   -> all ours popped, rethrow (-1). No catch anywhere -> `no-catch` signal.
/// - `Signal`: `kill-emacs` propagates untouched (frames left for the frame
///   unwind, like the interpreter's early return). Otherwise run
///   `dispatch_signal_if_needed` (signal hooks + handler-bind — may run lisp,
///   GC, or itself raise: loop on the new flow, the interpreter's recursion),
///   then unwind to `selected_resume` among our frames; on a match the error
///   object (`make_signal_binding_value`) goes through `out`.
///
/// The generated code keeps its live operand-stack values rooted across this
/// call (the lisp run by cleanups/hooks can collect) and maps the returned
/// ordinal back to the statically known handler target.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `out` is the generated
/// code's result stack slot.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_match_handler(ctx: *mut u8, ours: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, -1, {
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let ours = ours as usize;
        // A contained panic skipped the panicked extent's cleanup; heal its
        // residue against the leaf-entry bases BEFORE taking/matching, so
        // the count-based pops below and the innermost-match scan operate on
        // exactly this leaf's own frames.
        if shim_panic_pending() {
            heal_shim_panic_residue_before_match(ctx, ours);
        }
        let mut flow = take_pending_flow().expect("match shim runs only after STATUS_SIGNAL");
        let mut remaining = ours;
        let mut popped_ordinal_base = 0usize;
        'resume: loop {
            match flow {
                Flow::ThreadBlocked(_) | Flow::Shutdown(_) => {
                    stash_pending_flow(flow);
                    return -1;
                }
                Flow::Throw(thrown) => {
                    let (tag, value) = (thrown.tag, thrown.value);
                    let Some(selected) = ctx.matching_catch_resume(&tag) else {
                        // No matching catch anywhere: unwind all our frames and
                        // propagate `no-catch` (resume_nonlocal parity).
                        for _ in 0..remaining {
                            ctx.pop_condition_frame();
                        }
                        stash_pending_flow(signal(LispCondition::NoCatch, vec![tag, value]));
                        return -1;
                    };
                    for m in 0..remaining {
                        let frame = ctx
                            .pop_condition_frame()
                            .expect("JIT handler frames missing from condition stack");
                        let resume = condition_frame_resume(frame);
                        if resume == selected {
                            let ResumeTarget::VmCatch {
                                spec_depth,
                                bind_stack_len,
                                ..
                            } = resume
                            else {
                                unreachable!("JIT catch frame carries a VmCatch resume");
                            };
                            // unbind_to may run unwind-protect cleanups (lisp ->
                            // GC); keep the carried values alive across it.
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(tag);
                            push_scratch_gc_root(value);
                            let unwind = ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                            JIT_BIND_STACK.with(|s| s.borrow_mut().truncate(bind_stack_len));
                            restore_scratch_gc_roots(saved);
                            if let Err(next) = unwind {
                                let popped = m + 1;
                                remaining -= popped;
                                popped_ordinal_base += popped;
                                flow = next;
                                continue 'resume;
                            }
                            // SAFETY: `out` is the generated code's result slot.
                            unsafe { *out = value.bits() as i64 };
                            return (popped_ordinal_base + m) as i64;
                        }
                    }
                    // The selected catch belongs to an outer frame: ours are all
                    // popped; rethrow for the frame unwind + outer handlers.
                    stash_pending_flow(Flow::throw(tag, value));
                    return -1;
                }
                Flow::Signal(sig) => {
                    if sig.symbol == intern("kill-emacs") {
                        // Interpreter parity: propagate immediately, frames left
                        // to the frame-exit truncation.
                        stash_pending_flow(Flow::Signal(sig));
                        return -1;
                    }
                    // Signal hooks / handler-bind handlers may run lisp and GC;
                    // root the signal payload across the dispatch.
                    let saved = save_scratch_gc_roots();
                    push_scratch_gc_root(Value::from_sym_id(sig.symbol));
                    for v in sig.data.iter().copied() {
                        push_scratch_gc_root(v);
                    }
                    if let Some(raw) = sig.raw_data {
                        push_scratch_gc_root(raw);
                    }
                    let dispatched = ctx.dispatch_signal_if_needed(sig);
                    restore_scratch_gc_roots(saved);
                    let sig = match dispatched {
                        Ok(sig) => sig,
                        // A hook/handler raised: restart matching on the new flow
                        // (resume_nonlocal recurses here).
                        Err(next) => {
                            flow = next;
                            continue;
                        }
                    };
                    let Some(selected) = sig.selected_resume.clone() else {
                        for _ in 0..remaining {
                            ctx.pop_condition_frame();
                        }
                        stash_pending_flow(Flow::Signal(sig));
                        return -1;
                    };
                    for m in 0..remaining {
                        let frame = ctx
                            .pop_condition_frame()
                            .expect("JIT handler frames missing from condition stack");
                        let resume = condition_frame_resume(frame);
                        if resume == selected {
                            let ResumeTarget::VmConditionCase {
                                spec_depth,
                                bind_stack_len,
                                ..
                            } = resume
                            else {
                                unreachable!(
                                    "JIT condition-case frame carries a VmConditionCase resume"
                                );
                            };
                            // unbind_to runs cleanups and the error object below
                            // allocates: root the signal payload throughout.
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(Value::from_sym_id(sig.symbol));
                            for v in sig.data.iter().copied() {
                                push_scratch_gc_root(v);
                            }
                            if let Some(raw) = sig.raw_data {
                                push_scratch_gc_root(raw);
                            }
                            let unwind = ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                            JIT_BIND_STACK.with(|s| s.borrow_mut().truncate(bind_stack_len));
                            if let Err(next) = unwind {
                                restore_scratch_gc_roots(saved);
                                let popped = m + 1;
                                remaining -= popped;
                                popped_ordinal_base += popped;
                                flow = next;
                                continue 'resume;
                            }
                            let binding = make_signal_binding_value(&sig);
                            restore_scratch_gc_roots(saved);
                            // SAFETY: `out` is the generated code's result slot.
                            unsafe { *out = binding.bits() as i64 };
                            return (popped_ordinal_base + m) as i64;
                        }
                    }
                    stash_pending_flow(Flow::Signal(sig));
                    return -1;
                }
            }
        }
    })
}

/// `Op::Switch` lookup result: the dispatch value is not in the jump table —
/// fall through (interpreter parity).
pub(crate) const JIT_SWITCH_MISS: i64 = -1;
/// `Op::Switch` lookup result: the table no longer matches what was compiled
/// (a value mutated to a non-fixnum); the shim stashed a signal.
pub(crate) const JIT_SWITCH_STALE: i64 = -2;

/// `Op::Switch`: look the dispatch value up in the (statically verified
/// compile-time constant) hash-table jump table, with the interpreter's exact
/// key semantics (`to_hash_key_swp` under the table's own test). Returns the
/// raw fixnum target address on a hit ([`JIT_SWITCH_MISS`]/[`JIT_SWITCH_STALE`]
/// otherwise); the generated code maps raw addresses onto the statically
/// resolved target blocks. Pure lookup — no allocation, no lisp.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_switch(ctx: *mut u8, dispatch: i64, table: i64) -> i64 {
    jit_shim_contain!(ctx, JIT_SWITCH_STALE, {
        let table = Value::from_bits(table as usize);
        let dispatch = Value::from_bits(dispatch as usize);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let Some(ht) = table.as_hash_table() else {
            // Statically verified a hash table; only runtime mutation of the
            // constant pool itself could change that.
            stash_pending_flow(signal(
                "error",
                vec![Value::string("jit: switch jump table mutated at runtime")],
            ));
            return JIT_SWITCH_STALE;
        };
        let key = dispatch.to_hash_key_swp(&ht.test, ctx.symbols_with_pos_enabled);
        match ht.data.get(&key).copied() {
            Some(v) => match v.kind() {
                ValueKind::Fixnum(addr) if addr >= 0 => addr,
                _ => {
                    stash_pending_flow(signal(
                        "error",
                        vec![Value::string("jit: switch jump table mutated at runtime")],
                    ));
                    JIT_SWITCH_STALE
                }
            },
            None => JIT_SWITCH_MISS,
        }
    })
}

/// Cold path for a switch hit whose raw address is not in the statically
/// compiled target set (the jump table was mutated after compilation — code
/// the byte-compiler never produces). Stash a loud signal; the generated code
/// routes to its signal path. Context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_switch_stale() {
    jit_shim_contain!(no_ctx, (), {
        stash_pending_flow(signal(
            "error",
            vec![Value::string("jit: switch jump table mutated at runtime")],
        ));
    })
}

/// Back-edge service poll: GC safepoint + `maybe_quit`, via the same shared
/// Context helper the interpreter's `branch_to!` wrap path uses
/// (`bytecode_branch_maybe_gc_and_quit`). Generated code calls this every 255
/// backward jumps (the interpreter's u8 `quitcounter` cadence), with its live
/// operand-stack values rooted by the caller — the poll may collect.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_backedge(ctx: *mut u8) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        match ctx.bytecode_branch_maybe_gc_and_quit() {
            Ok(()) => STATUS_OK,
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        }
    })
}
