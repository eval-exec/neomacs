//! Bytecode → native lowering.
//!
//! Real compilation of neovm-core bytecode to machine code. Coverage is now
//! broad rather than a narrow leaf subset: **87 of the 88 bytecode opcodes
//! lower**, `MakeClosure` being the sole exception — and that op is a legacy
//! NeoVM-compiler form no runtime path emits any more: every closure created
//! at run time is an ordinary `(make-closure PROTO ...)` `Call`, which lowers.
//! The closure story that DOES matter is on the callee side: `make-closure`
//! instances share one tiering state and one leaf per source, with the patched
//! constant prefix loaded through the executing callee (`dynamic_prefix`;
//! `jit::Runtime`). `&optional`/`&rest`,
//! `catch`/`throw`, `condition-case`, `unwind-protect`, dynamic binding
//! (`varbind`/`unbind`), `save-excursion`/`save-restriction`/
//! `save-current-buffer`, static `switch` tables, `eq`/`symbolp` and
//! `Div`/`Rem` are all supported — the pre-2026-07 header claiming otherwise
//! long outlived the code and kept re-seeding a false "JIT coverage is narrow"
//! backlog item.
//!
//! Non-fixnum arithmetic does **not** refuse compilation: floats and bignums
//! **deopt at runtime** through fixnum tag guards, precisely and mid-function
//! ([`NativeRun::DeoptAt`], resumed by `Vm::run_resumed_frame`).
//!
//! Whole-function bails ([`CompileError`]) are correspondingly narrow:
//! dynamically-bound parameters ([`CompileError::TakesArguments`]), malformed
//! or unbalanced bytecode, dynamic `switch` jump tables, and Cranelift backend
//! failures.
//!
//! The remaining refusal is deliberate and is the real constraint on JIT
//! reach: [`CompileError::NotProfitable`], decided by `body_is_jit_profitable`
//! (`calls <= arith`). The baseline tier removes per-op interpreter *dispatch*,
//! but a native call costs more than a VM call — it GC-roots its live operands
//! and trampolines through a runtime shim. Call-dominated bodies therefore
//! measured net-negative, so widening opcode coverage cannot reach them;
//! lowering per-call overhead is what would.
//!
//! Two tiers live here. The optimizing Tier-2 path builds MIR (`jit/mir.rs`)
//! for pure required-only bodies and lowers it via `lower_mir_pure`; anything
//! else falls back to the baseline single-pass lowering. Baseline control flow
//! builds a CLIF basic-block CFG (`analyze_cfg` + `lower_leaf`); the operand
//! stack flows across edges through per-slot SSA variables, so Cranelift
//! inserts the phi nodes and branches carry no explicit block arguments.
//!
//! ## Speculation + deopt
//!
//! The arithmetic ops are *speculative*: native code assumes the operands are
//! fixnums and the result stays in fixnum range — exactly the interpreter's
//! fast path (`vm.rs` `Op::Add`). Each assumption is a **guard**; if a guard
//! fails at run time the function **deoptimizes**: it returns a 0 flag and the
//! caller re-runs the body on the Tier-0 interpreter, which handles the slow
//! cases (non-numbers signal, out-of-range promotes to a bignum). Because every
//! op in the supported subset is pure (no heap writes, no calls, no side
//! effects), re-running from the start after a deopt is always correct.
//!
//! ABI: `extern "C" fn(args: *const i64, out: *mut i64) -> i64`. Reads the
//! function's fixed arguments from `args` (seeding the operand stack), returns 1
//! and writes the result's raw tagged bits through `out` on success; returns 0
//! (deopt) otherwise, leaving `out` untouched.
//!
//! Allocation (`cons`) calls a C-ABI runtime shim. Because that may trigger GC,
//! live `Value`s held across it are kept alive by pushing them onto the
//! GC-traced scratch-root stack (see the `neovm_jit_*` shims); the GC is
//! non-moving, so the JIT's SSA registers stay valid afterward without a reload.
//! No vmctx is needed yet (`cons` uses the thread-local heap directly); that
//! arrives with `Call`/`Apply`.
//!
//! The bytecode operand stack is modelled at *compile time* as a `Vec` of
//! Cranelift SSA values (abstract interpretation). A `Value` is opaque to native
//! code: it flows as its `usize` bit pattern (`i64` in CLIF), exactly as the
//! interpreter stores it.

use crate::emacs_core::error::LispCondition;
use cranelift_codegen::ir::Value as ClifValue;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, FuncRef, Function, InstBuilder, MemFlagsData, Signature, StackSlot,
    StackSlotData, StackSlotKind, Type, UserFuncName, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use smallvec::SmallVec;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use super::backend::BackendError;
use super::mir;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::vm::condition_frame_resume;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::dynamic_module::panic_message;
use crate::emacs_core::error::{Flow, make_signal_binding_value, signal};
use crate::emacs_core::eval::{
    ConditionFrame, Context, LispArgVec, ModuleBoundarySnapshot, ResumeTarget,
    lookup_global_subr_entry, push_scratch_gc_root, restore_scratch_gc_roots,
    save_scratch_gc_roots, subr_entry_from_value,
};
use crate::emacs_core::intern::{SymId, intern, resolve_sym};
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::{Value, ValueKind};
use crate::tagged::header::{ConsCell, SubrDispatchKind, SubrFn};
use crate::tagged::value::{
    FIXNUM_CHECK_MASK, FIXNUM_CHECK_VALUE, FIXNUM_SHIFT, TAG_BITS, TAG_CONS, TAG_MASK, TAG_STRING,
    TAG_SYMBOL,
};

// ---------------------------------------------------------------------------
// Direct-builtin tables: the SAME typed `builtins::builtin_*` functions the
// interpreter opcode arms call, exposed to generated code through three
// arity-shaped generic shims. Single source of truth — the JIT cannot drift
// from the interpreter's semantics for these ops.
// ---------------------------------------------------------------------------

type JitBuiltin1 = fn(&mut Context, Value) -> Result<Value, Flow>;
type JitBuiltin2 = fn(&mut Context, Value, Value) -> Result<Value, Flow>;
type JitBuiltin3 = fn(&mut Context, Value, Value, Value) -> Result<Value, Flow>;

use crate::emacs_core::builtins as b;

static JIT_BUILTIN1: [JitBuiltin1; 4] = [
    b::builtin_length_1,          // 0
    b::builtin_symbol_value_1,    // 1
    b::builtin_symbol_function_1, // 2
    b::builtin_nreverse_1,        // 3
];

static JIT_BUILTIN2: [JitBuiltin2; 15] = [
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

static JIT_BUILTIN3: [JitBuiltin3; 1] = [
    b::builtin_put_3, // 0
];

/// Slice-shaped builtins (`fn(&[Value]) -> EvalResult`, no Context) — the
/// exact functions the interpreter's `Nconc`/`Concat`/`Substring` arms call.
type JitBuiltinSlice = fn(&[Value]) -> Result<Value, Flow>;

static JIT_BUILTIN_SLICE: [JitBuiltinSlice; 3] = [
    b::builtin_nconc_slice_values, // 0
    b::builtin_concat_slice,       // 1
    b::builtin_substring_slice,    // 2
];

/// `(nargs, table_index)` for ops lowered through the slice-builtin shim.
/// `Concat`'s arity rides in the opcode; `Nconc`/`Substring` are fixed.
fn slice_builtin_spec(op: &Op) -> Option<(usize, usize)> {
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
fn direct_builtin_spec(op: &Op) -> Option<(u8, usize)> {
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
fn arith_intrinsic_op_by_name(name: &str, nargs: usize) -> Option<u8> {
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
fn subr_spec_armed(ctx: &Context, sym: i64, expected: i64, slot: &SpecSlot) -> bool {
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
fn ash_fixnum_fast(value: i64, count: i64) -> Option<i64> {
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
        let armed = !force_cbsym_generic()
            && lookup_global_subr_entry(sym_id)
                .is_some_and(|e| e.dispatch_kind == SubrDispatchKind::Builtin);
        if !armed {
            #[cfg(debug_assertions)]
            CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        // The name-canonical SUBR value the arm's funcall_general dispatches on.
        let subr_value = Value::subr_from_sym_id(sym_id);
        let saved = save_scratch_gc_roots();
        // Root the args on the GC-traced bc_buf (like neovm_jit_call_subr_spec),
        // then build the exact-length arg vector funcall_general copies into its
        // backtrace frame.
        let args_start = ctx.bc_buf.len();
        for i in 0..nargs {
            // SAFETY: the generated code stored exactly `nargs` argument words at
            // `args_ptr` (its call-args slot) immediately before this call.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            ctx.bc_buf.push(v);
        }
        let args: LispArgVec = ctx.bc_buf[args_start..args_start + nargs]
            .iter()
            .copied()
            .collect();
        let res = ctx.funcall_general(subr_value, args);
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
fn cbsym_read_expected_nargs(which: u8) -> usize {
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
const JIT_SWITCH_MISS: i64 = -1;
/// `Op::Switch` lookup result: the table no longer matches what was compiled
/// (a value mutated to a non-fixnum); the shim stashed a signal.
const JIT_SWITCH_STALE: i64 = -2;

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

/// Coarse opcode category for [`CompileError::UnsupportedOp`] diagnostics.
fn op_category(op: &Op) -> &'static str {
    match op {
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem | Op::Add1 | Op::Sub1 | Op::Negate => {
            "arithmetic"
        }
        Op::Call(_) | Op::Apply(_) => "call",
        Op::VarRef(_) | Op::VarSet(_) | Op::VarBind(_) | Op::Unbind(_) => "variable",
        Op::Goto(_)
        | Op::GotoIfNil(_)
        | Op::GotoIfNotNil(_)
        | Op::GotoIfNilElsePop(_)
        | Op::GotoIfNotNilElsePop(_)
        | Op::Switch => "control-flow",
        Op::StackSet(_) | Op::DiscardN(_) => "stack-mutate",
        _ => "other",
    }
}

/// Resolve the `SymId` a `VarRef`/`VarSet` operand names, at compile time —
/// mirrors the interpreter's `sym_id_at` (symbol or symbol-with-pos), except
/// that exotic constants bail to the interpreter instead of falling back to
/// `nil`.
fn const_sym_id(constants: &[Value], idx: u16) -> Result<u32, CompileError> {
    let v = constants
        .get(idx as usize)
        .ok_or(CompileError::BadOperand)?;
    v.as_symbol_id()
        .or_else(|| v.as_symbol_with_pos_sym().and_then(|s| s.as_symbol_id()))
        .map(|id| id.0)
        .ok_or(CompileError::BadOperand)
}

/// Materialize a session-specific op-operand SymId as a Cranelift `i64`.
///
/// The JIT bakes `iconst(sym)` (valid same-session). AOT must RELOC it BY NAME:
/// the normalized `Value::symbol(sym)` is collected into the per-leaf reloc
/// vector (`collect_baseline_aot_relocs`), so its tagged bits are loaded from
/// `reloc_base[idx]` and the SymId recovered via `bits >> TAG_BITS`
/// (TAG_SYMBOL == 0b000). Keyed on `reloc_index` PRESENCE: the JIT reloc set
/// never holds op-symbols, so JIT always bakes → byte-identical. Mirrors the
/// CallBuiltinSym callee site (audit CRITICAL #2: VarRef/VarSet/VarBind baked a
/// session SymId here).
fn materialize_op_sym_id(
    fb: &mut FunctionBuilder,
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
    sym: u32,
) -> ClifValue {
    let key = (sym as usize) << TAG_BITS | TAG_SYMBOL;
    match reloc_index.get(&key) {
        Some(&idx) => {
            let base = reloc_base.expect("reloc_base set when an op-symbol is reloc'd");
            let sym_bits =
                fb.ins()
                    .load(types::I64, MemFlagsData::trusted(), base, (idx * 8) as i32);
            fb.ins().ushr_imm_u(sym_bits, TAG_BITS as i64)
        }
        None => fb.ins().iconst(types::I64, sym as i64),
    }
}

/// Materialize an `Op::Call` spec site's `expected` (subr/bytecode VALUE bits) for
/// the shim call. JIT (`aot=false`) bakes it as an `iconst` (valid same-session);
/// AOT (`aot=true`) loads it from `spec_expected_base[slot_idx]` — the per-thread
/// array the loader (`from_aot`) fills from the LIVE cell (the bits are
/// session-specific, so they must never be baked). Keyed on `aot`, NOT presence, so
/// the JIT stays byte-identical to before B2.
fn materialize_spec_expected(
    fb: &mut FunctionBuilder,
    aot: bool,
    spec_expected_base: Option<ClifValue>,
    expected: u64,
    slot_idx: usize,
) -> ClifValue {
    if aot {
        let base =
            spec_expected_base.expect("AOT sets spec_expected_base at an Op::Call spec site");
        fb.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            base,
            (slot_idx * 8) as i32,
        )
    } else {
        fb.ins().iconst(types::I64, expected as i64)
    }
}

/// Materialize an `Op::Call` spec site's `SpecSlot` pointer for the shim call. JIT
/// (`aot=false`) bakes the slot address as an `iconst`; AOT (`aot=true`) computes
/// `spec_slot_base + slot_idx * size_of::<SpecSlot>()` off the per-thread base the
/// loader put in the sidecar (the address is session-specific). Byte-identical to
/// before B2 for the JIT.
fn materialize_spec_slot(
    fb: &mut FunctionBuilder,
    aot: bool,
    spec_slot_base: Option<ClifValue>,
    slot_ptr: i64,
    slot_idx: usize,
) -> ClifValue {
    if aot {
        let base = spec_slot_base.expect("AOT sets spec_slot_base at an Op::Call spec site");
        fb.ins()
            .iadd_imm_u(base, (slot_idx * core::mem::size_of::<SpecSlot>()) as i64)
    } else {
        fb.ins().iconst(types::I64, slot_ptr)
    }
}

/// True iff this function's parameters are pushed onto the operand stack at
/// entry (so the body's `StackRef` opcodes reach them) — mirrors the
/// interpreter's `params_on_stack` in `vm.rs` `run_frame`. Dynamic-binding
/// bytecode binds params via `varref` instead and is not supported here.
fn params_on_stack(f: &ByteCodeFunction) -> bool {
    f.lexical
        || f.env.is_some()
        || matches!(
            f.arglist.kind(),
            crate::emacs_core::value::ValueKind::Fixnum(_)
        )
}

/// Compile a [`ByteCodeFunction`] whose parameters live on the operand stack
/// (lexical bytecode); otherwise bail.
///
/// `&optional` and `&rest` are supported: the native frame has one slot per
/// non-rest parameter plus one for the rest list, and [`CompiledLeaf::call`]
/// normalizes each incoming argument list to that frame (nil-padding, rest-list
/// construction) exactly as the interpreter's `run_frame` seeds it.
/// Dynamic-binding bytecode (params bound via `varbind`, not on the stack)
/// still bails.
pub fn compile_bytecode_function(f: &ByteCodeFunction) -> Result<CompiledLeaf, CompileError> {
    compile_bytecode_function_with(f, None)
}

/// [`compile_bytecode_function`] with the compiling thread's obarray for
/// direct-call speculation.
/// Profiling chokepoint (env-gated, zero cost when off): record the op-mix of
/// every distinct bytecode function the JIT attempts to compile, so a real
/// workload can be characterized — is hot elisp arithmetic-heavy (unboxing
/// helps), call-heavy (inlining helps), or dispatch/alloc-bound (an MIR tier
/// helps little)? Set `NEOVM_JIT_PROFILE=<path>` to append one CSV row per
/// function. Used to justify (or not) the optimizing Tier-2 investment.
fn jit_profile_path() -> Option<&'static str> {
    use std::sync::OnceLock;
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var("NEOVM_JIT_PROFILE").ok())
        .as_deref()
}

/// Verification harness (J0): when `NEOVM_JIT_FORCE_DEOPT=1`, EVERY speculation
/// guard (`emit_guard`) is forced to fail, so every guarded native fast path
/// takes its deopt path instead. Running the full suite with this on (ideally
/// with `NEOVM_JIT_THRESHOLD=1` so every function compiles) exercises every deopt
/// site and must produce results identical to the interpreter — the JIT analogue
/// of `NEOVM_GC_STRESS`/`gc_stress`. Catches deopt-frame-reconstruction bugs (the
/// riskiest part of speculation) before the optimizing Tier-2 adds more guards.
fn jit_force_deopt() -> bool {
    use std::sync::OnceLock;
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| std::env::var("NEOVM_JIT_FORCE_DEOPT").as_deref() == Ok("1"))
}

/// Verification harness (Gap 1): when `NEOVM_JIT_FORCE_SLOW_SPEC=1`, EVERY
/// speculated-call shim (the bytecode `neovm_jit_call_spec` + the three subr
/// spec shims) treats its per-site armed epoch as stale on every call, forcing
/// the epoch-mismatch/re-validate branch each time: the binding is re-read
/// from the obarray and compared against the baked expectation before any
/// direct dispatch. A suite run with this on (ideally with
/// `NEOVM_JIT_THRESHOLD=1`) stress-tests the slow/re-arm paths everywhere the
/// armed fast path would normally short-circuit — the spec-machinery analogue
/// of [`jit_force_deopt`].
fn jit_force_slow_spec() -> bool {
    use std::sync::OnceLock;
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| std::env::var("NEOVM_JIT_FORCE_SLOW_SPEC").as_deref() == Ok("1"))
}

/// Verification harness (R2): when `NEOVM_JIT_FORCE_CBSYM_GENERIC=1`, EVERY
/// CallBuiltinSym intrinsic shim ([`neovm_jit_cbsym_spec`] +
/// `neovm_jit_cbsym_read`) bounces to [`STATUS_NEED_GENERIC`] on every call,
/// forcing the per-site generated fallback (the general CBSym lowering →
/// `Vm::callbuiltinsym_for_jit`). A differential run with this on
/// (`NEOVM_JIT_THRESHOLD=1`) proves the fallback path is byte-identical to the
/// fast path — the CBSym analogue of [`jit_force_slow_spec`] / `jit_force_deopt`.
fn force_cbsym_generic() -> bool {
    use std::sync::OnceLock;
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| std::env::var("NEOVM_JIT_FORCE_CBSYM_GENERIC").as_deref() == Ok("1"))
}

fn jit_profile_emit(f: &ByteCodeFunction, obarray: Option<&Obarray>, compiled: bool) {
    let Some(path) = jit_profile_path() else {
        return;
    };
    let ops = f.executable_ops();
    let mut arith = 0u32;
    let mut calls = 0u32;
    let mut alloc = 0u32;
    let mut listops = 0u32;
    let mut varops = 0u32;
    let mut preds = 0u32;
    let mut backedges = 0u32;
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Add1
            | Op::Sub1
            | Op::Negate
            | Op::Max
            | Op::Min
            | Op::Eqlsign
            | Op::Lss
            | Op::Gtr
            | Op::Leq
            | Op::Geq => arith += 1,
            Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) | Op::CallBuiltinSym(..) => calls += 1,
            Op::Cons | Op::List(_) | Op::Concat(_) | Op::Nconc => alloc += 1,
            Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe => listops += 1,
            Op::VarRef(_) | Op::VarSet(_) | Op::VarBind(_) | Op::Unbind(_) => varops += 1,
            Op::Null
            | Op::Not
            | Op::Consp
            | Op::Stringp
            | Op::Listp
            | Op::Symbolp
            | Op::Integerp
            | Op::Numberp => preds += 1,
            Op::Goto(t)
            | Op::GotoIfNil(t)
            | Op::GotoIfNotNil(t)
            | Op::GotoIfNilElsePop(t)
            | Op::GotoIfNotNilElsePop(t)
                if (*t as usize) <= i =>
            {
                backedges += 1;
            }
            _ => {}
        }
    }
    // Inlinable call sites: those whose callee is a constant symbol currently
    // fbound to a BYTECODE object (the only directly-inlinable target) — the
    // Bytecode-kind subset of what `find_spec_sites` detects (Gap 1 added
    // subr-kind sites, which are NOT inlinable — keep this metric's meaning).
    // `calls - inlinable` are subr / dynamic / non-bytecode callees inlining
    // can't directly take. This sizes inlining's TRUE surface (vs the
    // call-bearing upper bound).
    let arity =
        f.params.required.len() + f.params.optional.len() + usize::from(f.params.rest.is_some());
    let inlinable = match obarray {
        Some(ob) => analyze_cfg(ops, &f.constants, f.executable_gnu_byte_offset_map(), arity)
            .map(|cfg| {
                find_spec_sites(ops, &f.constants, &cfg.leaders, ob)
                    .values()
                    .filter(|site| site.kind == SpecCalleeKind::Bytecode)
                    .count()
            })
            .unwrap_or(0),
        None => 0,
    };
    let line = format!(
        "{},{},{},{},{},{},{},{},{},{},{}\n",
        ops.len(),
        arith,
        calls,
        alloc,
        listops,
        varops,
        preds,
        backedges,
        u8::from(backedges > 0),
        u8::from(compiled),
        inlinable,
    );
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
thread_local! {
    /// Per-thread override for the profitability gate, set by tests that need to
    /// compile a deliberately call-dominated body to exercise the call/spec
    /// machinery (which production would correctly decline to compile).
    static PROFIT_GATE_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force the profitability gate on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_profit_gate_for_test(on: bool) {
    PROFIT_GATE_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Is the JIT profitability gate enabled? Default yes; `NEOVM_JIT_PROFIT=off`
/// disables it, so the gate can be A/B-measured against the old behavior in a
/// single build.
fn jit_profit_gate_on() -> bool {
    #[cfg(test)]
    if let Some(o) = PROFIT_GATE_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_PROFIT").as_deref() != Ok("off"))
}

#[cfg(test)]
std::thread_local! {
    static GATE_RELAX_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force the call-heavy gate-relaxation on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_gate_relax_for_test(on: bool) {
    GATE_RELAX_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Is the call-heavy gate relaxation enabled? **Default NO** — reverted to the
/// conservative `calls <= arith` on 2026-07-21 after it regressed byte-compilation
/// (see below). `NEOVM_JIT_GATE_RELAX=on` opts in (stops counting user-function
/// `Op::Call`/`Op::Apply` against profitability, so user-call-heavy bodies tier).
///
/// HISTORY: briefly default-ON (commit 20cb6190a). Motivating measurements looked
/// good on SYNTHETICS — a hot user-fn call loop 2.31x, trivial-builtin 1.21x, real
/// font-lock 1.013x (neutral) — and the oracle suite showed zero new failures. But
/// those synthetics were unrepresentative: they run the SAME hot body enough to
/// amortize the JIT compile cost. The reverted-to conservative gate exists
/// precisely to protect BYTE-COMPILATION (call-heavy, builtin-heavy, ~one-shot),
/// and a proper `perf stat instructions:u` A/B (byte-compile cl-macs.el x8,
/// release) measured the flip **21% SLOWER** (22.56B on vs 18.58B off, ratio 1.214
/// x3 runs): with the gate on, ~9% goes to runtime regalloc2+cranelift compilation
/// that never amortizes because native ≈ interp for builtin-heavy code (the
/// font-lock 1.013x), so the compile tax is pure loss. LESSON: measure the
/// workload the gate was DESIGNED for (byte-compile), not a favorable synthetic.
/// The right long-term path is AOT (compile the standard library at build time so
/// it never runtime-compiles) + a gate that distinguishes amortizing from
/// one-shot hot bodies — not this blanket relaxation. Knob kept for hot long-
/// running user-call-heavy loops that genuinely amortize.
fn jit_gate_relax_on() -> bool {
    #[cfg(test)]
    if let Some(o) = GATE_RELAX_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_GATE_RELAX").as_deref() == Ok("on"))
}

#[cfg(test)]
std::thread_local! {
    static INLINE_ARITH_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force Level-B inline-arith on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_inline_arith_for_test(on: bool) {
    INLINE_ARITH_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// LEVEL-B: inline `logand`/`logior`/`logxor`/`lognot` (JIT only) as native
/// `band`/`bor`/`bxor`/`ineg` on the TAGGED fixnum bits (the tag `2` survives
/// `&`/`|`, is restored after `^`, and `ineg` maps a tagged fixnum to its
/// `lognot`), guarded by a fixnum check that deopts, instead of the armed
/// `neovm_jit_arith_spec` shim (which marshals 8 args). `mod` inlines its
/// floor-modulo on the untagged values (srem + branchless sign-fixup,
/// zero-divisor deopt). Redefinition is caught by the leaf's `inline_epoch`
/// eviction. `ash` never inlines (overflow/bignum).
/// Default ON since the shim-vs-inline A/B (list workload −11.5% wall, no
/// change elsewhere); `NEOVM_JIT_INLINE_ARITH=off` is the kill switch. AOT
/// always keeps the shim (its loader owns arm/disarm — an inline op has no
/// per-site epoch).
fn jit_inline_arith_on() -> bool {
    #[cfg(test)]
    if let Some(o) = INLINE_ARITH_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_ARITH").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Which `ArithIntrinsic` ops Level-B lowers inline (everything but `ash`, whose
/// overflow/bignum path must stay on the shim).
fn arith_op_inlines(op: u8) -> bool {
    op != ARITH_KIND_ASH as u8
}

/// Every `Op::Call` site whose callee provably holds an intrinsifiable bit-op
/// SYMBOL constant (`logand`/`logior`/`logxor`/`ash`/`lognot` at its intrinsic
/// arity) — the sites `subr_spec_kind` classifies as
/// [`SpecCalleeKind::ArithIntrinsic`] — as `(op_index, callee_sym, arith_op)`.
/// The profitability gate uses the indices to NOT count an intrinsifiable bit-op
/// as a call (it lowers to a native op ≈ arith, not a call+dispatch, so a
/// bit-op-heavy loop is not vetoed as "call-dominated"); Level-B uses the callee
/// syms of the inlinable ops for the leaf's redefinition-eviction dep set.
///
/// A name-only, obarray-free mirror of [`find_spec_sites`]' abstract-tag scan
/// (the callee sits `nargs` below the top; only the stack-shuffling ops carry a
/// tag). It omits `find_spec_sites`' block-leader clearing (which needs the
/// CFG): the byte-compiler always emits a callee push and its `Op::Call` inside
/// one basic block with no jump target between, so the two agree on real
/// bytecode; on hand-crafted bytecode the gate may merely over-/under-count (a
/// heuristic mis-estimate, never a miscompile — `find_spec_sites` independently
/// decides, off the LIVE binding, what actually intrinsifies). Ascending order.
fn arith_intrinsic_call_sites(
    ops: &[Op],
    constants: &[Value],
) -> Vec<(usize, crate::emacs_core::intern::SymId, u8)> {
    let mut out = Vec::new();
    let mut tags: Vec<Option<u16>> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Constant(cidx) => {
                let tag = constants
                    .get(*cidx as usize)
                    .and_then(|v| v.as_symbol_id())
                    .map(|_| *cidx);
                tags.push(tag);
            }
            Op::Nil | Op::True => tags.push(None),
            Op::Dup => {
                let t = tags.last().copied().flatten();
                tags.push(t);
            }
            Op::StackRef(n) => {
                let n = *n as usize;
                let t = if tags.len() > n {
                    tags[tags.len() - 1 - n]
                } else {
                    None
                };
                tags.push(t);
            }
            Op::StackSet(n) => {
                let n = *n as usize;
                let t = tags.pop().flatten();
                if n > 0 && tags.len() >= n {
                    let d = tags.len() - n;
                    tags[d] = t;
                }
            }
            Op::DiscardN(raw) => {
                let preserve_tos = (raw & 0x80) != 0;
                let n = (raw & 0x7F) as usize;
                if preserve_tos && n > 0 {
                    let t = tags.pop().flatten();
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                    tags.push(t);
                } else {
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                }
            }
            Op::Call(n) => {
                let nargs = *n as usize;
                if tags.len() > nargs
                    && let Some(cidx) = tags[tags.len() - 1 - nargs]
                    && let Some(sym_id) =
                        constants.get(cidx as usize).and_then(|v| v.as_symbol_id())
                    && let Some(arith_op) = arith_intrinsic_op_by_name(resolve_sym(sym_id), nargs)
                {
                    out.push((i, sym_id, arith_op));
                }
                for _ in 0..(nargs + 1).min(tags.len()) {
                    tags.pop();
                }
                tags.push(None);
            }
            Op::Apply(n) => {
                let nargs = *n as usize;
                for _ in 0..(nargs + 1).min(tags.len()) {
                    tags.pop();
                }
                tags.push(None);
            }
            other => match simple_effect(other) {
                Ok((needs, delta)) => {
                    let consumed = needs;
                    let produced = (needs as i64 + delta).max(0) as usize;
                    for _ in 0..consumed.min(tags.len()) {
                        tags.pop();
                    }
                    for _ in 0..produced {
                        tags.push(None);
                    }
                }
                Err(_) => tags.clear(),
            },
        }
    }
    out
}

/// The distinct callee symbols of the body's INLINABLE (`arith_op_inlines`) bit-op
/// sites — the redefinition-eviction dep set for a Level-B leaf (an inlined op
/// bakes the native instruction with no per-call arming, so `fset`ing the callee
/// must evict the leaf). `ash` is excluded (it stays on the self-arming shim).
fn inline_arith_callee_syms(
    ops: &[Op],
    constants: &[Value],
) -> Vec<crate::emacs_core::intern::SymId> {
    let mut syms: Vec<crate::emacs_core::intern::SymId> =
        arith_intrinsic_call_sites(ops, constants)
            .into_iter()
            .filter(|&(_, _, op)| arith_op_inlines(op))
            .map(|(_, sym, _)| sym)
            .collect();
    syms.sort_by_key(|s| s.0);
    syms.dedup();
    syms
}

/// Decide whether a bytecode body is worth compiling.
///
/// The baseline tier only removes per-op interpreter *dispatch*. A function call
/// costs MORE in native code than in the VM — each call GC-roots its live
/// operands and trampolines through a runtime shim (`neovm_jit_gc_push` +
/// `neovm_jit_call`). So a call-dominated body pays that overhead with nothing
/// to offset it: measured ~32% SLOWER on real workloads (byte-compilation,
/// font-lock), where ~36 of 48 tiered bodies had zero arithmetic and ~10 calls
/// each — pure call/control code the native frame can only shuffle, not speed
/// up. Compile only when arithmetic is not outnumbered by calls; the genuine win
/// shape (hot arithmetic/control loops — the 7x microbenchmark) clears this, and
/// call-free bodies always pass (`0 <= 0`).
fn body_is_jit_profitable(ops: &[Op], constants: &[Value]) -> bool {
    if !jit_profit_gate_on() {
        return true;
    }
    let relax = jit_gate_relax_on();
    // The intrinsifiable bit-op `Op::Call` sites (logand/logior/logxor/ash/lognot),
    // which lower to native ops — counted as arith below, not calls. Ascending
    // op-index order (linear scan), so `binary_search` is valid.
    let intrinsic: Vec<usize> = arith_intrinsic_call_sites(ops, constants)
        .into_iter()
        .map(|(idx, _, _)| idx)
        .collect();
    let mut arith = 0u32;
    let mut calls = 0u32;
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Add1
            | Op::Sub1
            | Op::Negate
            | Op::Max
            | Op::Min
            | Op::Eqlsign
            | Op::Lss
            | Op::Gtr
            | Op::Leq
            | Op::Geq => arith += 1,
            // Gate-relaxation: USER-function calls (`Op::Call`/`Op::Apply`) are
            // net-POSITIVE tiered (measured 2.31x native vs interp — V3
            // native-to-native + lever-1), so they must not veto compilation. A
            // body dominated by them (font-lock's re-search-forward calls aside —
            // those are subrs, but still Op::Call) tiers to a net win/neutral.
            // Builtin calls stay counted (real builtin-heavy = neutral, not a win).
            Op::Call(_) | Op::Apply(_) if relax => {}
            // A `logand`/`logior`/`logxor` bit-op `Op::Call` that WILL intrinsify
            // (`ArithIntrinsic` → `neovm_jit_arith_spec`) lowers to a GC-free
            // native op, not a call+dispatch — so, exactly like the CBSym
            // intrinsics below, it must NOT veto a bit-op-heavy loop (a pure
            // `(logand (logxor ...))` loop is `calls > arith` → NotProfitable →
            // the intrinsic could never engage). A generic call still vetoes.
            Op::Call(_) if intrinsic.binary_search(&i).is_ok() => {}
            Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) => calls += 1,
            // R2: a CallBuiltinSym that WILL be intrinsified (Tier-A GC-free read
            // or Tier-B dispatch-skip) costs ~an arith op, not a full
            // call+dispatch — so it must NOT veto a buffer-op-heavy loop (point/
            // insert/goto-char/... loops were `calls > arith` -> NotProfitable ->
            // the intrinsic could never engage). Drop those from the call count
            // via the classifier's OWN predicate, so the gate agrees exactly with
            // what tiers. Non-intrinsifiable CBSym ops still count as calls, so a
            // genuinely call-dominated non-spec body stays protected.
            Op::CallBuiltinSym(sym, n) if cbsym_spec_kind(*sym, *n as usize).is_none() => {
                calls += 1;
            }
            _ => {}
        }
    }
    calls <= arith
}

pub fn compile_bytecode_function_with(
    f: &ByteCodeFunction,
    obarray: Option<&Obarray>,
) -> Result<CompiledLeaf, CompileError> {
    let result = compile_bytecode_function_inner(f, obarray);
    if jit_profile_path().is_some() {
        jit_profile_emit(f, obarray, result.is_ok());
    }
    result
}

/// Max instruction budget (excluding `Arg`s) for an inlined callee body. Small
/// pure helpers (`sq`, `1+`-wrappers, accessors) are the target; larger callees
/// stay calls and the baseline handles them.
const MAX_INLINE_INSTS: usize = 8;

/// Resolve a constant call-target symbol to its callee MIR for inlining, or `None`
/// unless it is a required-only lexical bytecode function (so `build_mir`'s
/// argument seeding matches the arity).
fn resolve_inline_callee(ob: &Obarray, sym: Value) -> Option<mir::MirFunction> {
    let sym_id = sym.as_symbol_id()?;
    let binding = ob.symbol_function_id(sym_id)?;
    let bc = binding.get_bytecode_data()?;
    // Required-only lexical, and no captured lexenv: inlining drops the lexenv
    // install, which is only otherwise safe because lexenv-reading ops lower to
    // Opaque and `callee_inlinable` rejects them — keep the safety local here too.
    if !bc.lexical || bc.env.is_some() || !bc.params.optional.is_empty() || bc.params.rest.is_some()
    {
        return None;
    }
    // A patched source's leading constants are per-instance; inlining would
    // bake this instance's captured values into the caller.
    if bc.jit_runtime().patched_prefix() > 0 {
        return None;
    }
    mir::build_mir(bc.executable_ops(), &bc.constants, bc.params.required.len()).ok()
}

fn compile_bytecode_function_inner(
    f: &ByteCodeFunction,
    obarray: Option<&Obarray>,
) -> Result<CompiledLeaf, CompileError> {
    let ops = f.executable_ops();
    let required = f.params.required.len();
    let nonrest = required + f.params.optional.len();
    let has_rest = f.params.rest.is_some();
    let native_arity = nonrest + usize::from(has_rest);
    if native_arity > 0 && !params_on_stack(f) {
        // Params are dynamically bound, not on the stack — `StackRef` would not
        // find them.
        return Err(CompileError::TakesArguments);
    }
    // Typed-MIR Tier-2: for pure required-only functions, build the SSA MIR and
    // lower it with fixnum UNBOXING (raw arithmetic, retag only at boundaries) —
    // faster than the baseline's per-op untag/retag. Fall back to the baseline on
    // any bail (calls, cons, optional/&rest args, ...). Restricted to no
    // optional/&rest so the MIR's argument seeding matches `native_arity`.
    // A `make-closure`-patched source: leading constant slots are per-instance.
    // The MIR tier bakes every constant, so it is skipped; the baseline gets
    // the masked view for its analyses and loads the prefix through the callee.
    let dynamic_prefix = f.jit_runtime().patched_prefix();
    let masked;
    let constants: &[Value] = if dynamic_prefix > 0 {
        masked = mask_dynamic_prefix(&f.constants, dynamic_prefix);
        &masked
    } else {
        &f.constants
    };
    if !has_rest
        && f.params.optional.is_empty()
        && dynamic_prefix == 0
        && let Ok(mut mir) = mir::build_mir(ops, constants, native_arity)
    {
        // Inline pure single-block callees (resolved through the obarray). When
        // a call is inlined the body can become pure (no Opaque), so
        // lower_mir_pure handles it and unboxing/guard-elision flow ACROSS the
        // former call boundary. Record the armed function_epoch so the dispatch
        // re-JITs if any inlined callee is later redefined (see CompiledLeaf
        // ::inline_epoch).
        let mut inlined_syms: Vec<crate::emacs_core::intern::SymId> = Vec::new();
        let inline_epoch = obarray.and_then(|ob| {
            let armed = ob.function_epoch();
            let n = mir::inline_pure_single_block_callees(
                &mut mir,
                &|sym| resolve_inline_callee(ob, sym),
                MAX_INLINE_INSTS,
                &mut inlined_syms,
            );
            (n > 0).then_some(armed)
        });
        if let Ok(mut leaf) = lower_mir_pure(&mir) {
            // Tier gate: a call-bearing MIR leaf (has_side_effects) only earns
            // the MIR tier when it INLINED something — that's the one case the
            // MIR tier beats the baseline (cross-boundary unboxing/elision).
            // For a plain non-inlined call the baseline is strictly better
            // (spec-call native-to-native speculation + battle-tested), so let
            // it fall through. Pure (call-free) leaves always take the MIR tier.
            if !leaf.has_side_effects || inline_epoch.is_some() {
                leaf.required = required;
                leaf.has_rest = has_rest;
                leaf.inline_epoch = inline_epoch;
                // The precise dependency set (registered into INLINE_DEPS at the
                // cache compile-miss site so a redefinition of any inlined callee
                // evicts exactly this leaf).
                leaf.inline_deps = inlined_syms.into();
                return Ok(leaf);
            }
        }
    }
    // The MIR tier above already claimed any body its inlining/unboxing makes
    // worthwhile. What's left goes to the baseline, whose per-op call shims aren't
    // worth it for a call-dominated body — keep those on the interpreter.
    if !body_is_jit_profitable(ops, constants) {
        return Err(CompileError::NotProfitable);
    }
    let mut leaf = lower_leaf_full(
        ops,
        constants,
        native_arity,
        f.executable_gnu_byte_offset_map(),
        obarray,
        dynamic_prefix,
    )?;
    leaf.required = required;
    leaf.has_rest = has_rest;
    // LEVEL-B redefinition guard: an inlined bit-op (logand/logior/logxor/lognot)
    // bakes the native op with NO per-call arming, so the leaf must be evicted if
    // its callee is ever redefined. Use PRECISE inline_deps only (NOT the coarse
    // inline_epoch backstop): `set_symbol_function_id` → `note_function_redefined`
    // → `evict_inline_dependents(sym)` fires on every fset/fmakunbound, so a leaf
    // registered under `logand` is evicted exactly when `logand` changes. The
    // coarse `inline_epoch` backstop is DELIBERATELY omitted — it evicts on ANY
    // function redefinition, which thrash-recompiles bit-op leaves during
    // byte-compile/loadup (measured +13.7%); a bare epoch bump without a cell write
    // means the bit-op is unchanged, so precise eviction loses no correctness.
    if jit_inline_arith_on() && obarray.is_some() {
        let inline_syms = inline_arith_callee_syms(ops, constants);
        if !inline_syms.is_empty() {
            leaf.inline_deps = inline_syms.into();
        }
    }
    Ok(leaf)
}

/// Minimum operand-stack depth a simple op requires, and its net depth change.
/// `Err` for anything outside the supported simple subset.
pub(crate) fn simple_effect(op: &Op) -> Result<(usize, i64), CompileError> {
    if let Some((arity, _)) = direct_builtin_spec(op) {
        // N operands -> one result.
        return Ok((arity as usize, 1 - arity as i64));
    }
    if let Some((nargs, _)) = slice_builtin_spec(op) {
        return Ok((nargs, 1 - nargs as i64));
    }
    Ok(match op {
        Op::List(n) => (*n as usize, 1 - *n as i64),
        Op::CallBuiltin(_, n) | Op::CallBuiltinSym(_, n) => (*n as usize, 1 - *n as i64),
        Op::Aset => (3, -2),
        Op::SaveWindowExcursion => (1, 0),
        Op::Constant(_) | Op::Nil | Op::True => (0, 1),
        Op::StackRef(n) => (*n as usize + 1, 1),
        Op::StackSet(n) => (*n as usize + 1, -1),
        Op::DiscardN(raw) => {
            let n = (*raw & 0x7F) as usize;
            let needs = if (*raw & 0x80) != 0 && n > 0 {
                n + 1
            } else {
                n
            };
            (needs, -(n as i64))
        }
        Op::Dup => (1, 1),
        Op::Pop => (1, -1),
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Rem
        | Op::Eq
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq => (2, -1),
        Op::Add1 | Op::Sub1 | Op::Negate => (1, 0),
        Op::Null | Op::Not | Op::Consp | Op::Stringp | Op::Listp | Op::Symbolp => (1, 0),
        Op::Integerp | Op::Numberp => (1, 0),
        Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe => (1, 0),
        Op::Max | Op::Min => (2, -1),
        Op::Cons => (2, -1),
        // [func a1 .. aN] -> [result]
        Op::Call(n) | Op::Apply(n) => (*n as usize + 1, -(*n as i64)),
        Op::VarRef(_) => (0, 1),
        Op::VarSet(_) => (1, -1),
        Op::VarBind(_) => (1, -1),
        Op::Unbind(_) => (0, 0),
        Op::SaveCurrentBuffer | Op::SaveExcursion | Op::SaveRestriction => (0, 0),
        Op::UnwindProtectPop => (1, -1),
        other => return Err(CompileError::UnsupportedOp(op_category(other))),
    })
}

/// A statically tracked active handler: `(handler target instruction, operand
/// stack depth at the push)`. The list at any program point is the stack of
/// `PushConditionCase`/`PushCatch` frames not yet popped, outermost first.
type HandlerStatic = (usize, usize);

/// What kind of callee a speculation site validated at compile time — decides
/// which shim the armed site calls and how its lowering roots/falls back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpecCalleeKind {
    /// A BYTECODE object: `neovm_jit_call_spec` (epoch logic inside the shim,
    /// V3 native-to-native fast path, strict-symbol fallback inside the shim).
    Bytecode,
    /// A fixed-arity builtin SUBR: `neovm_jit_call_subr_spec` — armed direct
    /// dispatch with a FRESH per-call entry read; NOT-armed returns
    /// [`STATUS_NEED_GENERIC`] and the site's generated fallback block runs
    /// the plain generic call.
    SubrGeneral,
    /// `recordp` (1 arg): `neovm_jit_pred_spec`, pure tag test when armed.
    PredRecordp,
    /// `symbol-with-pos-p` (1 arg): `neovm_jit_pred_spec`, pure tag test.
    PredSymbolWithPos,
    /// `equal-including-properties` (2 args): `neovm_jit_eq_incl_props_spec`,
    /// bitwise-eq hit → `t`; anything else bounces to the generic block.
    EqInclProps,
    /// A bitwise arithmetic intrinsic — `logand`/`logior`/`logxor` (2 args):
    /// `neovm_jit_arith_spec`. When armed AND both args are fixnums, computes the
    /// native `&`/`|`/`^` in-register (bitwise ops on two fixnums always yield a
    /// fixnum — the interpreter's own 2-arg fast path, `builtin_logand_slice` et
    /// al.); anything else (marker/bignum/wrong-type/not-armed) bounces to the
    /// generic block. `op` is an `ARITH_KIND_*` discriminant, baked as an iconst
    /// into the shim call. Each op gets a DISTINCT [`to_spec_disc`] value so the
    /// AOT loader disarms a site whose baked op no longer matches the live callee.
    ArithIntrinsic { op: u8 },
    /// R2 CallBuiltinSym intrinsic, Tier-A (`which` = a `CBSYM_A_*`
    /// discriminant): a provably-trivial buffer/match-state read whose builtin
    /// body allocates no lisp — `neovm_jit_cbsym_read` calls the body GC-free,
    /// bouncing to [`STATUS_NEED_GENERIC`] when the static entry is no longer a
    /// plain builtin (or, for `current-buffer`, when the buffer value was never
    /// materialized). Classified BY NAME off `Op::CallBuiltinSym` (advice/fset
    /// immune — the op name-dispatches the static subr table).
    CbsymTierA { which: u8 },
    /// R2 CallBuiltinSym intrinsic, Tier-B (dispatch-skip): a plain builtin
    /// reached via `neovm_jit_cbsym_spec`, which reproduces the CBSym
    /// interpreter arm EXACTLY (fresh entry re-read, SUBR-value backtrace frame
    /// via `subr_from_sym_id`, arity vs the fresh entry, exact-slice args, NO
    /// `with_bytecode_call_depth`, quit-AFTER) — skipping only the
    /// `resolve_sym` → `builtin_name_id` → special-name string-switch round
    /// trip. Bounces to [`STATUS_NEED_GENERIC`] (→ the general CBSym lowering)
    /// when the fresh entry is not `Some` + `Builtin`.
    CbsymTierB,
}

/// Tier-A `which` discriminants (baked into generated code as an `iconst` and
/// read by [`neovm_jit_cbsym_read`]). Each maps 1:1 to a builtin whose body the
/// shim calls GC-free; a register-read reimplementation would diverge (e.g.
/// match-beginning does a byte→char conversion), so the shim always DELEGATES.
pub(crate) const CBSYM_A_POINT: u8 = 0;
pub(crate) const CBSYM_A_POINT_MIN: u8 = 1;
pub(crate) const CBSYM_A_POINT_MAX: u8 = 2;
pub(crate) const CBSYM_A_BOLP: u8 = 3;
pub(crate) const CBSYM_A_EOLP: u8 = 4;
pub(crate) const CBSYM_A_BOBP: u8 = 5;
pub(crate) const CBSYM_A_EOBP: u8 = 6;
pub(crate) const CBSYM_A_FOLLOWING_CHAR: u8 = 7;
pub(crate) const CBSYM_A_PRECEDING_CHAR: u8 = 8;
pub(crate) const CBSYM_A_CHAR_AFTER: u8 = 9;
pub(crate) const CBSYM_A_CURRENT_BUFFER: u8 = 10;
pub(crate) const CBSYM_A_MATCH_BEGINNING: u8 = 11;
pub(crate) const CBSYM_A_MATCH_END: u8 = 12;

impl SpecCalleeKind {
    /// Kinds whose direct path passes its 1–2 args in REGISTERS with no
    /// call-args spill and no residual rooting (the shims are GC-free by
    /// contract); their generated fallback block spills + roots itself.
    fn is_reg_args(self) -> bool {
        matches!(
            self,
            SpecCalleeKind::PredRecordp
                | SpecCalleeKind::PredSymbolWithPos
                | SpecCalleeKind::EqInclProps
                | SpecCalleeKind::ArithIntrinsic { .. }
        )
    }

    /// The three round-1 direct-SUBR speculation kinds that make
    /// `declare_rt_refs` pull in the JIT-only `neovm_jit_{call_subr,pred,eq_incl_props}_spec`
    /// shims. CBSym kinds are deliberately excluded — they declare their own
    /// shims off a separate flag so a CBSym-only body never imports the
    /// `Op::Call` spec shims.
    fn is_round1_subr(self) -> bool {
        matches!(
            self,
            SpecCalleeKind::SubrGeneral
                | SpecCalleeKind::PredRecordp
                | SpecCalleeKind::PredSymbolWithPos
                | SpecCalleeKind::EqInclProps
                | SpecCalleeKind::ArithIntrinsic { .. }
        )
    }

    /// The CallBuiltinSym intrinsic kinds (Tier-A read shims / Tier-B
    /// dispatch-skip). Keyed by the op's own SymId, resolved BY NAME.
    fn is_cbsym(self) -> bool {
        matches!(
            self,
            SpecCalleeKind::CbsymTierA { .. } | SpecCalleeKind::CbsymTierB
        )
    }

    /// R2 increment B2: the AOT descriptor discriminant for an `Op::Call` spec
    /// site's baked kind, or `None` for a CBSym kind (name-canonical, epochless —
    /// carries no per-site descriptor entry). The loader RE-CLASSIFIES the live
    /// binding and arms a site ONLY when the fresh `to_spec_disc()` matches the
    /// `kind_disc` baked here — so a re-aliased callee (e.g. `recordp` rebound to a
    /// non-`PredRecordp` subr) DISARMS instead of running the wrong baked op.
    ///
    /// EXHAUSTIVE match (no `_`): a new `SpecCalleeKind` variant is compile-forced
    /// to choose a discriminant (and bump [`DISC_COUNT`](Self::DISC_COUNT), salted
    /// into `ABI_TAG`) rather than silently defaulting.
    pub(crate) fn to_spec_disc(self) -> Option<u8> {
        match self {
            SpecCalleeKind::Bytecode => Some(0),
            SpecCalleeKind::SubrGeneral => Some(1),
            SpecCalleeKind::PredRecordp => Some(2),
            SpecCalleeKind::PredSymbolWithPos => Some(3),
            SpecCalleeKind::EqInclProps => Some(4),
            // A DISTINCT disc per bit-op (5=and, 6=ior, 7=xor). The op is baked as
            // an iconst into the shim call, so the AOT loader — which re-classifies
            // the LIVE callee and arms only on an exact disc match — must disarm a
            // site whose baked op differs from the live binding's op (e.g. a `logand`
            // site whose callee was re-aliased to `logior`). A shared disc would arm
            // it and run the wrong baked op; distinct discs make the mismatch disarm.
            SpecCalleeKind::ArithIntrinsic { op } => {
                debug_assert!(op <= 5, "ArithIntrinsic op discriminant out of range");
                Some(5 + op)
            }
            SpecCalleeKind::CbsymTierA { .. } | SpecCalleeKind::CbsymTierB => None,
        }
    }

    /// Number of distinct `Op::Call` spec discriminants [`to_spec_disc`](Self::to_spec_disc)
    /// assigns (0..DISC_COUNT). Salted into `ABI_TAG` so a renumber/count change
    /// re-tags stale `.so`s.
    pub(crate) const DISC_COUNT: u8 = 11;
}

/// A speculated direct-call site: an `Op::Call` whose callee slot provably
/// holds the constant symbol `sym`, fbound at compile time to the bytecode
/// object or fixed-arity builtin subr `expected_bits` (see `kind`). `slot`
/// indexes the leaf's armed-epoch slots.
#[derive(Clone, Copy)]
struct SpecSite {
    sym: u32,
    expected_bits: u64,
    slot: usize,
    kind: SpecCalleeKind,
}

/// R2 phase 2 — the NAME ALLOWLIST of `SubrFn::Many` builtins that an
/// `Op::Call(n)` site is permitted to speculate on (classified as
/// [`SpecCalleeKind::SubrGeneral`], reusing the round-1 subr shim). Chosen from
/// the real interactive-session profile's `Op::Call` HOT group (re-search-forward
/// 3.4%, parse-partial-sexp 4.65%, looking-at 1.78%, intern-soft 2.57%,
/// put-text-property 1.92%, match-data/set-match-data, scan-sexps, ...) — all
/// registered `SubrFn::Many`, which round-1's fixed-arity gate
/// (`subr_entry_uses_fixed_value_call`) excludes by construction.
///
/// `Op::Call(n)` carries the EXACT nargs, and the armed path passes that exact
/// slice to the Many subr (`call_spec_subr_stack` ->
/// `dispatch_builtin_subr_from_stack_args_unchecked`'s `Many` arm, exact-length
/// `.to_vec()` — NO nil-pad), so it is byte-identical to the generic/interpreter
/// dispatch of the same site.
///
/// Membership is `SubrFn::Many` ONLY, never `ManySlice`: the `ManySlice`
/// variadics (`+`/`logand`/`logior`/`logxor`/`list`/`vector`/`append`/`nconc`/
/// `string-match`, all `max=None`) are rejected by the `SubrFn::Many` match in
/// `subr_spec_kind`, independent of this list (asserted by
/// `subr_spec_kind_rejects_registered_manyslice`).
const SUBR_MANY_ALLOWLIST: &[&str] = &[
    "re-search-forward",
    "looking-at",
    "parse-partial-sexp",
    "match-data",
    "set-match-data",
    "scan-sexps",
    "intern-soft",
    "line-end-position",
    "syntax-table",
    "set-syntax-table",
    "put-text-property",
    // Residual-coverage audit (task A PART 2): the font-lock SUBR-MIX
    // (`vm_subr_mix_fontlock`) ranks `get-text-property` at 11.0% — the 6th
    // hottest builtin and the READ sibling of the already-allowlisted
    // `put-text-property` (11.6%), which #1 shipped WITHOUT its read pair. It is
    // a plain `SubrFn::Many` read (registered `min=2`, `max=Some(3)`, so the
    // Many arm caps `nargs<=3`), no writeback, no SWP-flag — so the exact-slice
    // Many dispatch is byte-identical to generic exactly as `put-text-property`'s
    // is (strictly SIMPLER, being a pure interval-tree read).
    "get-text-property",
];

/// Classify a compile-time function-cell binding for SUBR speculation at an
/// `Op::Call(nargs)` site on `site_sym`, or `None` when the site must stay on
/// the generic path. Every clause is load-bearing:
///
/// * `subr_entry_from_value` + `dispatch_kind == Builtin` — only plain
///   builtins; special forms / context-callables have different call
///   protocols. Checked again at run time (fresh entry read) since entries
///   are rewritten in place.
/// * `aset`/`fillarray` excluded on BOTH the site name and the resolved subr
///   name — their mutating-first-string-arg WRITEBACK protocol
///   (`Vm::mutates_first_arg_name` / `maybe_writeback_mutating_first_arg`)
///   wraps the generic call; the resolved-name check also covers
///   `(fset 'alias (symbol-function 'aset))` aliases, which
///   `writeback_mutating_callable_names` detects through the cell.
/// * `funcall`/`apply`/`eval` excluded (both names) — re-entrant drivers;
///   depth/backtrace conservatism (`eval` IS a fixed-arity A2).
///
/// Then one of two disjoint arms:
///
/// * FIXED-ARITY `A0..A8` (`subr_entry_uses_fixed_value_call`), the round-1
///   surface. `min_args <= nargs <= max_args`, else a call that must signal
///   wrong-number-of-arguments stays generic (byte-identical payload/frame).
///   `vectorp` stays General (correct, just not a tag test): bool-vectors and
///   sentinel char-tables are genuine `VecLikeType::Vector` objects that
///   `builtin_vectorp_1` distinguishes semantically — an inline tag test would
///   return `t` where the builtin returns `nil`. `keywordp`/`symbolp` stay
///   General because their builtins consult `symbols-with-pos-enabled`.
/// * ALLOWLISTED `SubrFn::Many` (R2 phase 2, [`SUBR_MANY_ALLOWLIST`]) — the
///   armed path dispatches the EXACT-length arg slice to the Many subr (no
///   nil-pad, so byte-identical to generic). `nargs >= min_args` always; when
///   `max_args` is `Some`, also `nargs <= max_args` (an over-arity call stays
///   generic for the identical signal). When `max_args` is `None`
///   (`put-text-property`, declared with arity `0..unbounded` and its real 4..5
///   range body-enforced in `textprop.rs`), permit any `nargs >= min_args`: the
///   body self-checks and spec ≡ generic both reach that same body check.
///   The site is CELL-DISPATCHED, so the round-1 guards (per-site epoch,
///   `compiler_function_overrides_active`, fresh-entry re-read in
///   `call_spec_subr_stack`) still deopt on advice/defalias/fset — unlike the
///   name-canonical `Op::CallBuiltinSym` path, which is override-immune.
fn subr_spec_kind(binding: Value, site_sym: SymId, nargs: usize) -> Option<SpecCalleeKind> {
    let (subr_sym, entry) = subr_entry_from_value(binding)?;
    if entry.dispatch_kind != SubrDispatchKind::Builtin {
        return None;
    }
    let site_name = resolve_sym(site_sym);
    let resolved_name = resolve_sym(subr_sym);
    if [site_name, resolved_name]
        .iter()
        .any(|name| matches!(*name, "aset" | "fillarray" | "funcall" | "apply" | "eval"))
    {
        return None;
    }
    // Bitwise-arith intrinsic (`logand`/`logior`/`logxor`, 2 args) — a GC-free
    // native `&`/`|`/`^` via `neovm_jit_arith_spec`. Classified HERE, before the
    // fixed-arity/`SubrFn::Many` branches, because these builtins are `ManySlice`
    // variadics that both branches reject (they otherwise get full generic
    // dispatch — the biggest, easiest win). Match on `resolved_name` (the subr the
    // cell points to) so an alias `(fset 'myand (symbol-function 'logand))` still
    // intrinsifies; the per-site epoch/expected-bits guard deopts on redefinition.
    if let Some(op) = arith_intrinsic_op_by_name(resolved_name, nargs) {
        return Some(SpecCalleeKind::ArithIntrinsic { op });
    }
    if Context::subr_entry_uses_fixed_value_call(entry) {
        if nargs < entry.min_args as usize {
            return None;
        }
        if entry.max_args.is_none_or(|max| nargs > max as usize) {
            return None;
        }
        Some(match (resolved_name, nargs) {
            ("recordp", 1) => SpecCalleeKind::PredRecordp,
            ("symbol-with-pos-p", 1) => SpecCalleeKind::PredSymbolWithPos,
            ("equal-including-properties", 2) => SpecCalleeKind::EqInclProps,
            _ => SpecCalleeKind::SubrGeneral,
        })
    } else if matches!(entry.function, Some(SubrFn::Many(_)))
        && SUBR_MANY_ALLOWLIST.contains(&resolved_name)
    {
        if nargs < entry.min_args as usize {
            return None;
        }
        // Some(max): enforce `nargs <= max`. None (`put-text-property`): the
        // body self-enforces its real range, so permit any `nargs >= min`.
        if entry.max_args.is_some_and(|max| nargs > max as usize) {
            return None;
        }
        Some(SpecCalleeKind::SubrGeneral)
    } else {
        None
    }
}

/// The `dispatch_vm_builtin_unrooted` special names (vm.rs) — VM-internal
/// bytecode operations that are NOT real Elisp subrs (they are handled by an
/// explicit string switch BEFORE `funcall_general`). A CBSym site whose name is
/// one of these must never be intrinsified: the fast shim funnels through
/// `funcall_general`, which would resolve the name-canonical static subr
/// entry — a DIFFERENT dispatch than the special-name arm. None of the R2 ship
/// set collides with these (asserted by `cbsym_shipset_excludes_special_names`
/// in the tests), but the classifier denylists them anyway for defence.
const CBSYM_SPECIAL_NAMES: &[&str] = &[
    "call-interactively",
    "start-kbd-macro",
    "end-kbd-macro",
    "call-last-kbd-macro",
    "execute-kbd-macro",
    "garbage-collect",
    "mapatoms",
    "maphash",
    "store-kbd-macro-event",
    "cancel-kbd-macro-events",
    "%%defvar",
    "%%defconst",
    "%%unimplemented-elc-bytecode",
];

/// Classify an `Op::CallBuiltinSym(sym, nargs)` site for R2 intrinsification, or
/// `None` when it must stay on the general named-builtin lowering. The
/// distinguishing property of CallBuiltinSym (vs the `Op::Call` sites
/// `subr_spec_kind` handles): the op carries the EXACT nargs and name-dispatches
/// the static subr table (`subr_from_sym_id(builtin_name_id(resolve_sym(sym)))`,
/// vm.rs) — it is advice/fset/override-IMMUNE, so there is NO epoch guard and NO
/// `compiler_function_overrides_active` gate; the target is name-canonical and
/// `Box::leak` process-stable. Every clause is load-bearing:
///
/// * `lookup_global_subr_entry(sym)` + `dispatch_kind == Builtin` — the op must
///   currently name a plain builtin. Re-checked FRESH in the shim (entries are
///   rewritten in place); a mismatch there bounces to `STATUS_NEED_GENERIC`.
/// * ALLOWLIST by name (the profiled R2 winners only) — so nothing outside the
///   audited ship set is ever intrinsified. This structurally excludes the
///   `aset`/`fillarray` writeback names, `funcall`/`apply`/`eval`, and every
///   `dispatch_vm_builtin_unrooted` special name (none are in the allowlist);
///   the explicit denylist below is defence-in-depth.
/// * NO fixed-arity gate (unlike `subr_spec_kind`): the Tier-B shim dispatches
///   through `funcall_general` on the exact-length arg vector, so a `Many` subr
///   gets `into_vec()` (byte-identical) and a wrong-arity call signals
///   `wrong-number-of-arguments` with the SUBR payload identically to the
///   interpreter arm — no need to force those to the generic path.
///
/// Obarray-FREE: consults only `lookup_global_subr_entry` (the static subr table)
/// and name resolution, so it classifies identically at JIT emit (`Some(obarray)`,
/// via `find_spec_sites`) AND AOT baseline emit (`obarray=None`, via
/// `find_cbsym_spec_sites` — increment A). The CallBuiltinSym op then takes the
/// Tier-A/B fast shim in BOTH tiers (its op-SymId is reloc'd by name under AOT).
fn cbsym_spec_kind(sym: SymId, _nargs: usize) -> Option<SpecCalleeKind> {
    let entry = lookup_global_subr_entry(sym)?;
    if entry.dispatch_kind != SubrDispatchKind::Builtin {
        return None;
    }
    let name = resolve_sym(sym);
    // Defence-in-depth denylist (the allowlist below already excludes these).
    if CBSYM_SPECIAL_NAMES.contains(&name)
        || matches!(name, "aset" | "fillarray" | "funcall" | "apply" | "eval")
    {
        return None;
    }
    // Tier-A: provably-trivial GC-free reads (COMMIT 5 shims). char-after is
    // Tier-A only in its 0-arg form; a 1-arg / marker call bounces to Tier-B's
    // generic path via the None fall-through here.
    let tier_a = match name {
        "point" => Some(CBSYM_A_POINT),
        "point-min" => Some(CBSYM_A_POINT_MIN),
        "point-max" => Some(CBSYM_A_POINT_MAX),
        "bolp" => Some(CBSYM_A_BOLP),
        "eolp" => Some(CBSYM_A_EOLP),
        "bobp" => Some(CBSYM_A_BOBP),
        "eobp" => Some(CBSYM_A_EOBP),
        "following-char" => Some(CBSYM_A_FOLLOWING_CHAR),
        "preceding-char" => Some(CBSYM_A_PRECEDING_CHAR),
        "char-after" if _nargs == 0 => Some(CBSYM_A_CHAR_AFTER),
        "current-buffer" => Some(CBSYM_A_CURRENT_BUFFER),
        "match-beginning" => Some(CBSYM_A_MATCH_BEGINNING),
        "match-end" => Some(CBSYM_A_MATCH_END),
        _ => None,
    };
    if let Some(which) = tier_a {
        return Some(SpecCalleeKind::CbsymTierA { which });
    }
    // Tier-B: plain builtins reached via `neovm_jit_cbsym_spec` (dispatch skip).
    if matches!(
        name,
        "length"
            | "insert"
            | "set-marker"
            | "goto-char"
            | "delete-region"
            | "forward-line"
            | "forward-char"
            | "buffer-substring"
            | "set-buffer"
            | "skip-chars-forward"
            | "skip-chars-backward"
            | "current-column"
            | "widen"
            | "indent-to"
            // Residual-coverage audit (task A PART 2): `end-of-line` is a
            // dedicated-opcode CBSym motion builtin (GNU op 127) at 2.37% in the
            // font-lock SUBR-MIX — the ONLY member of its own loop's motion set
            // (forward-line/forward-char/current-column, all already Tier-B) left
            // on the generic path. Point-moving side effect (not a pure read ->
            // NOT Tier-A); the Tier-B dispatch-skip reproduces the CBSym
            // interpreter arm exactly, byte-identical, as its siblings do.
            | "end-of-line"
    ) {
        return Some(SpecCalleeKind::CbsymTierB);
    }
    None
}

/// Per-site speculation state, baked into generated code by raw address and
/// read by `neovm_jit_call_spec`. `epoch` is the obarray `function_epoch` at
/// which this site's callee binding was last validated. `leaf` lazily caches a
/// `*const CompiledLeaf` (as `usize` bits; 0 = none) for the armed callee, so
/// repeat calls skip the compiled-cache hash lookup (the V3 fast path). The
/// leaf pointer is cleared whenever revalidation fails (the binding changed),
/// and is sound while set because the tagged-heap identity is stable during
/// native execution (so `cache::clear()` cannot fire mid-call to free the leaf —
/// see `resolve_compiled_leaf_ptr`; NOT "the cache never evicts", audit #1).
/// `repr(C)` pins the field order the baked pointer arithmetic relies on.
#[repr(C)]
pub(crate) struct SpecSlot {
    epoch: AtomicU64,
    leaf: AtomicU64,
}

/// Find direct-call speculation sites: the byte-compiler's standard call
/// shape `Constant(f) arg-push* Call(n)` where every op between the callee
/// push and its call only PUSHES new slots (Constant/Nil/True/Dup/StackRef —
/// the callee slot can't be rewritten), no jump target lands inside the
/// window, and `f` is currently fbound to either
///
/// * a BYTECODE object ([`SpecCalleeKind::Bytecode`]) — an epoch-equal check
///   on a bytecode binding proves it still names the same immutable bytecode
///   object; or
/// * a FIXED-ARITY builtin SUBR passing [`subr_spec_kind`]'s constraints
///   (Gap 1) — subr VALUE bits are stable (`Box::leak`'d, rewritten in place
///   by `update_static_subr_object_entry` keeping bits identical, and
///   `Context::install_subr` path bumps `function_epoch` on every rewrite), so the
///   same epoch/bits validation applies; the armed shims re-read the ENTRY
///   fresh per call precisely because of those in-place rewrites.
///
/// This full pass runs ONLY under `Some(obarray)` (see `lower_leaf_full`), so its
/// `Op::Call` SUBR speculation auto-excludes AOT — no AOT object bakes subr-spec
/// slot state or references the round-1/`Op::Call` spec shims (that is increment
/// B). The CBSym pass it ends with ([`append_cbsym_spec_sites`]) is obarray-FREE,
/// and the AOT baseline emit runs it separately ([`find_cbsym_spec_sites`]), so
/// AOT bodies DO reference the CBSym shims (increment A).
fn find_spec_sites(
    ops: &[Op],
    constants: &[Value],
    leaders: &[usize],
    obarray: &Obarray,
) -> HashMap<usize, SpecSite> {
    let mut sites = HashMap::new();
    let mut next_slot = 0usize;
    // Block-local abstract operand stack: a SUFFIX of the real stack where a
    // tracked slot is `Some(const-idx)` iff it provably holds that SYMBOL
    // constant (pushed by `Op::Constant` in this block, copied only through
    // the stack-shuffling ops modeled below). An `Op::Call(n)` whose callee
    // slot (n below the top) carries a tag becomes a speculation site — this
    // generalizes the old scan, which required every argument between the
    // callee push and the call to be a TRIVIAL push and therefore rejected
    // any call with a computed argument (e.g. a self-recursive
    // `(fib (- n 1))`, whose `Op::Sub` disqualified the site and pinned every
    // recursive call to the generic shim).
    //
    // Soundness split: this pass SELECTS sites; the `Op::Call` lowering only
    // emits the spec shim after independently PROVING (SSA inspection,
    // `callee_is_symbol_const`) that the callee slot's value is the site's
    // symbol constant — so a divergence between this model and the lowering
    // degrades to the generic call, never a wrong-callee speculation.
    //
    // Suffix discipline: cleared at every block leader (unknown entry stack)
    // and on any unmodeled op; pops past the suffix bottom just empty it (the
    // values below were already unknown) and later pushes re-grow it, so
    // top-relative reads inside the suffix stay exact.
    let mut tags: Vec<Option<u16>> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        if leaders.binary_search(&i).is_ok() {
            tags.clear();
        }
        match op {
            Op::Constant(cidx) => {
                let tag = constants
                    .get(*cidx as usize)
                    .and_then(|v| v.as_symbol_id())
                    .map(|_| *cidx);
                tags.push(tag);
            }
            Op::Nil | Op::True => tags.push(None),
            Op::Dup => {
                let t = tags.last().copied().flatten();
                tags.push(t);
            }
            Op::StackRef(n) => {
                let n = *n as usize;
                let t = if tags.len() > n {
                    tags[tags.len() - 1 - n]
                } else {
                    None
                };
                tags.push(t);
            }
            Op::StackSet(n) => {
                // Interpreter semantics: the top value moves into the slot `n`
                // below it, then the top is dropped (n == 0 is a plain pop).
                let n = *n as usize;
                let t = tags.pop().flatten();
                if n > 0 && tags.len() >= n {
                    let d = tags.len() - n;
                    tags[d] = t;
                }
            }
            Op::DiscardN(raw) => {
                let preserve_tos = (raw & 0x80) != 0;
                let n = (raw & 0x7F) as usize;
                if preserve_tos && n > 0 {
                    // The top survives, landing n slots lower.
                    let t = tags.pop().flatten();
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                    tags.push(t);
                } else {
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                }
            }
            op @ (Op::Call(n) | Op::Apply(n)) => {
                let nargs = *n as usize;
                // Site check BEFORE applying the call's stack effect: the
                // callee sits nargs below the top (Apply never speculates).
                if matches!(op, Op::Call(_))
                    && tags.len() > nargs
                    && let Some(cidx) = tags[tags.len() - 1 - nargs]
                    && let Some(sym_val) = constants.get(cidx as usize)
                    && let Some(sym_id) = sym_val.as_symbol_id()
                    && let Some(binding) = obarray.symbol_function_id(sym_id)
                {
                    let kind = if binding.is_bytecode() {
                        Some(SpecCalleeKind::Bytecode)
                    } else {
                        subr_spec_kind(binding, sym_id, nargs)
                    };
                    if let Some(kind) = kind {
                        sites.insert(
                            i,
                            SpecSite {
                                sym: sym_id.0,
                                expected_bits: binding.bits() as u64,
                                slot: next_slot,
                                kind,
                            },
                        );
                        next_slot += 1;
                    }
                }
                // [func a1 .. aN] -> [result]
                for _ in 0..(nargs + 1).min(tags.len()) {
                    tags.pop();
                }
                tags.push(None);
            }
            other => match simple_effect(other) {
                Ok((needs, delta)) => {
                    // For every op reaching this arm, `needs` IS the consumed
                    // operand count and `needs + delta` the produced count.
                    // The ops where that identity does NOT hold — the
                    // stack-shuffling Dup/StackRef/StackSet/DiscardN (reads
                    // without consuming / writes in place) — are all handled
                    // explicitly above, as are Constant/Nil/True/Call/Apply.
                    let consumed = needs;
                    let produced = (needs as i64 + delta).max(0) as usize;
                    for _ in 0..consumed.min(tags.len()) {
                        tags.pop();
                    }
                    for _ in 0..produced {
                        tags.push(None);
                    }
                }
                // Control flow / anything unmodeled: forget everything.
                Err(_) => tags.clear(),
            },
        }
    }
    // R2: CallBuiltinSym intrinsic sites (self-contained per-op, classified by
    // NAME — obarray-free). Runs for BOTH JIT (here) and AOT baseline emit (via
    // `find_cbsym_spec_sites`, increment A); continues the slot numbering from the
    // `Op::Call` pass above so the JIT map is byte-identical.
    append_cbsym_spec_sites(ops, &mut sites, &mut next_slot);
    sites
}

/// The CallBuiltinSym intrinsic classification pass of [`find_spec_sites`], split
/// out so the AOT baseline emit (`obarray=None`) can run it WITHOUT the `Op::Call`
/// subr pass (increment B). A `CallBuiltinSym` op is self-contained (it carries
/// its callee SymId + the EXACT nargs), so there is no callee-slot-rewrite window
/// to validate and no jump-target hazard — classify in place by NAME.
/// [`cbsym_spec_kind`] consults ONLY the static subr table + name resolution (NO
/// obarray), so it classifies identically at JIT emit (`Some(obarray)`) and AOT
/// emit (`None`). Sites are keyed at the op's own index (disjoint from the
/// `Op::Call` indices: an index is one op) and numbered from `*next_slot`.
///
/// CBSym is SLOTLESS/EPOCHLESS: a site carries a `slot` index for uniformity with
/// the `Op::Call` sites, but its lowering (`Op::CallBuiltinSym` arm) reads ONLY
/// the site's `kind` (Tier-A `which` / Tier-B) — it never bakes the slot pointer
/// or `expected_bits`. So an AOT body needs no per-site descriptor entry.
fn append_cbsym_spec_sites(
    ops: &[Op],
    sites: &mut HashMap<usize, SpecSite>,
    next_slot: &mut usize,
) {
    for (i, op) in ops.iter().enumerate() {
        let Op::CallBuiltinSym(sym, n) = op else {
            continue;
        };
        let Some(kind) = cbsym_spec_kind(*sym, *n as usize) else {
            continue;
        };
        sites.insert(
            i,
            SpecSite {
                sym: sym.0,
                // Unused for CBSym: the target is name-canonical (no epoch
                // guard); the shim recomputes `subr_from_sym_id(sym)` itself.
                expected_bits: 0,
                slot: *next_slot,
                kind,
            },
        );
        *next_slot += 1;
    }
}

/// R2 increment A: the CBSym-only speculation-site map for the AOT baseline emit
/// (`obarray=None`). The `Op::Call` subr speculation stays JIT-only (needs a live
/// obarray binding — increment B), but CBSym is name-canonical + obarray-free, so
/// AOT bodies get the Tier-A/B fast shims too. Slots number from 0; the sites'
/// slot pointers are never baked (CBSym is slotless — see [`append_cbsym_spec_sites`]).
fn find_cbsym_spec_sites(ops: &[Op]) -> HashMap<usize, SpecSite> {
    let mut sites = HashMap::new();
    let mut next_slot = 0usize;
    append_cbsym_spec_sites(ops, &mut sites, &mut next_slot);
    sites
}

/// R2 increment B2 tier-pivot: does this body have ≥1 `Op::Call` subr/bytecode
/// speculation site against the LIVE `obarray`? Used by the AOT EMIT path
/// (`compile_leaf_to_object` / `build_preload_object`) to force a spec-bearing body
/// to the BASELINE tier (only it bakes the `Op::Call` spec fast paths) AND by the
/// LOAD path (`live_reloc_for_emit_tier`) to pick the matching baseline reloc
/// collector — so both agree and the leaf actually serves. Obarray-classified, so a
/// callee redefined away from a spec kind between dump + load simply yields `false`
/// (the recipe-compare / re-classify then keep it sound — bail-to-JIT or DISARM).
pub(crate) fn has_op_call_spec_sites(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    obarray: &Obarray,
) -> bool {
    let Ok(cfg) = analyze_cfg(ops, constants, None, arity) else {
        return false;
    };
    find_spec_sites(ops, constants, &cfg.leaders, obarray)
        .values()
        .any(|s| s.kind.to_spec_disc().is_some())
}

/// R2 increment B2: finalize a baseline leaf's speculation-site map for AOT emit —
/// DROP any `Op::Call` spec site whose callee symbol isn't in the reloc vector
/// (`materialize_op_sym_id` would otherwise bake a session SymId; the site then
/// falls to the generic `None` arm — the leaf still emits, never bails), RENUMBER
/// the surviving slots densely (`Op::Call` spec sites 0..K, then the slotless CBSym
/// sites K..N) so the loader's descriptor position IS the codegen slot index, and
/// return the surviving `Op::Call` sites as [`AotSpecSite`]s in slot order.
///
/// (In practice a callee push is always the `Op::Constant` that
/// `collect_baseline_aot_relocs` collected, so the drop path is defence-in-depth.)
fn finalize_baseline_spec_sites(
    spec_sites: &mut HashMap<usize, SpecSite>,
    ops: &[Op],
    reloc_index: &std::collections::HashMap<usize, u32>,
) -> Vec<super::aot::AotSpecSite> {
    // Snapshot (op_idx, site) in the original slot order (find_spec_sites numbers
    // Op::Call sites first, then CBSym), then rebuild the map with dense slots.
    let mut ordered: Vec<(usize, SpecSite)> = spec_sites.iter().map(|(&op, s)| (op, *s)).collect();
    ordered.sort_by_key(|(_, s)| s.slot);
    spec_sites.clear();
    let mut aot_sites: Vec<super::aot::AotSpecSite> = Vec::new();
    let mut next_slot = 0usize;
    // Pass 1: the Op::Call subr/bytecode spec sites (own the descriptor entries).
    for (op_idx, mut site) in ordered
        .iter()
        .copied()
        .filter(|(_, s)| s.kind.to_spec_disc().is_some())
    {
        let key = (site.sym as usize) << TAG_BITS | TAG_SYMBOL;
        let Some(&callee_reloc_idx) = reloc_index.get(&key) else {
            continue; // DROP — un-reloc'd callee → generic None arm; do NOT bail.
        };
        let nargs = match ops.get(op_idx) {
            Some(Op::Call(n)) => *n,
            _ => continue, // spec sites are keyed at an Op::Call; be defensive.
        };
        let kind_disc = site
            .kind
            .to_spec_disc()
            .expect("filtered to an Op::Call spec disc");
        site.slot = next_slot;
        next_slot += 1;
        spec_sites.insert(op_idx, site);
        aot_sites.push(super::aot::AotSpecSite {
            kind_disc,
            which: 0,
            nargs,
            callee_reloc_idx,
        });
    }
    // Pass 2: the slotless CBSym sites (no descriptor entry), renumbered after.
    for (op_idx, mut site) in ordered
        .iter()
        .copied()
        .filter(|(_, s)| s.kind.to_spec_disc().is_none())
    {
        site.slot = next_slot;
        next_slot += 1;
        spec_sites.insert(op_idx, site);
    }
    aot_sites
}

/// Resolve the static target set of the `Op::Switch` at `i`: the byte
/// compiler always pushes the jump table as a constant immediately before the
/// switch, so require `ops[i-1]` to be that `Constant`, the constant to be a
/// hash table, and every table value to be a fixnum address resolving (through
/// the GNU byte-offset map when present) to an in-range instruction index.
/// Returns deduplicated `(raw address, instruction index)` pairs; anything
/// else bails to the interpreter.
fn switch_static_targets(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    i: usize,
) -> Result<Vec<(i64, usize)>, CompileError> {
    let table = match i.checked_sub(1).map(|p| &ops[p]) {
        Some(Op::Constant(idx)) => constants
            .get(*idx as usize)
            .ok_or(CompileError::BadOperand)?,
        _ => return Err(CompileError::UnsupportedOp("switch-dynamic")),
    };
    let Some(ht) = table.as_hash_table() else {
        return Err(CompileError::UnsupportedOp("switch-dynamic"));
    };
    let mut out: Vec<(i64, usize)> = Vec::with_capacity(ht.data.len());
    for v in ht.data.values() {
        let ValueKind::Fixnum(raw) = v.kind() else {
            return Err(CompileError::UnsupportedOp("switch-dynamic"));
        };
        let raw_addr = usize::try_from(raw).map_err(|_| CompileError::BadOperand)?;
        let target = match offset_map {
            Some(map) => map
                .binary_search_by_key(&raw_addr, |e| e.byte_offset)
                .map(|k| map[k].instruction_index)
                .map_err(|_| CompileError::BadOperand)?,
            None => raw_addr,
        };
        if target >= ops.len() {
            return Err(CompileError::BadOperand);
        }
        if !out.iter().any(|&(r, _)| r == raw) {
            out.push((raw, target));
        }
    }
    Ok(out)
}

/// Basic-block analysis: sorted block leaders, the operand-stack depth at each
/// block's entry, the active-handler stack at each block's entry, the resolved
/// static target sets of every `Op::Switch`, and the max depth seen at any
/// block boundary.
pub(crate) struct Cfg {
    pub(crate) leaders: Vec<usize>,
    pub(crate) entry_depth: HashMap<usize, usize>,
    pub(crate) entry_handlers: HashMap<usize, Vec<HandlerStatic>>,
    pub(crate) switch_targets: HashMap<usize, Vec<(i64, usize)>>,
    pub(crate) max_depth: usize,
}

/// Record that `target` is entered with stack depth `d`, outstanding dynamic
/// bind count `binds`, and active handler stack `handlers`, scheduling it for
/// analysis on first sight. Depth, bind count, and handler stack must be
/// non-negative and consistent across all paths (the byte-compiler guarantees
/// a single static value per program point), so each block is analyzed once.
#[allow(clippy::too_many_arguments)] // dataflow successor state remains split for in-place updates
fn push_succ(
    entry_depth: &mut HashMap<usize, usize>,
    entry_binds: &mut HashMap<usize, usize>,
    entry_handlers: &mut HashMap<usize, Vec<HandlerStatic>>,
    work: &mut Vec<usize>,
    target: usize,
    d: i64,
    binds: usize,
    handlers: &[HandlerStatic],
) -> Result<(), CompileError> {
    if d < 0 {
        return Err(CompileError::StackUnderflow);
    }
    let d = d as usize;
    match entry_depth.get(&target) {
        Some(&existing) if existing != d => {
            Err(CompileError::UnsupportedOp("inconsistent stack depth"))
        }
        Some(_) => {
            if entry_binds.get(&target).copied().unwrap_or(0) != binds {
                return Err(CompileError::UnsupportedOp("inconsistent bind depth"));
            }
            if entry_handlers
                .get(&target)
                .is_none_or(|existing| existing != handlers)
            {
                return Err(CompileError::UnsupportedOp("inconsistent handler stack"));
            }
            Ok(())
        }
        None => {
            entry_depth.insert(target, d);
            entry_binds.insert(target, binds);
            entry_handlers.insert(target, handlers.to_vec());
            work.push(target);
            Ok(())
        }
    }
}

/// Partition `ops` into basic blocks and compute the operand-stack depth at each
/// block boundary, validating that every op is supported, jump targets are in
/// range, depth never underflows, and every path ends in `Return`.
pub(crate) fn analyze_cfg(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
) -> Result<Cfg, CompileError> {
    let n = ops.len();
    if n == 0 {
        return Err(CompileError::NoReturn);
    }

    // 1. Block leaders: index 0, every jump target, and every index following a
    //    branch/goto/return.
    let mut leader_set: BTreeSet<usize> = BTreeSet::new();
    let mut switch_targets: HashMap<usize, Vec<(i64, usize)>> = HashMap::new();
    leader_set.insert(0);
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Switch => {
                // Resolve the static target set now (bails for non-constant
                // tables); every target is a leader, plus the miss
                // fall-through.
                let targets = switch_static_targets(ops, constants, offset_map, i)?;
                for &(_, t) in &targets {
                    leader_set.insert(t);
                }
                if i + 1 < n {
                    leader_set.insert(i + 1);
                }
                switch_targets.insert(i, targets);
            }
            Op::Goto(t)
            | Op::GotoIfNil(t)
            | Op::GotoIfNotNil(t)
            | Op::GotoIfNilElsePop(t)
            | Op::GotoIfNotNilElsePop(t) => {
                let t = *t as usize;
                if t >= n {
                    return Err(CompileError::BadOperand);
                }
                leader_set.insert(t);
                if i + 1 < n {
                    leader_set.insert(i + 1);
                }
            }
            // Handler pushes end their block (the lowering emits an anchor
            // edge to the handler target) and make the target a leader.
            Op::PushConditionCase(t) | Op::PushConditionCaseRaw(t) | Op::PushCatch(t) => {
                let t = *t as usize;
                if t >= n {
                    return Err(CompileError::BadOperand);
                }
                leader_set.insert(t);
                if i + 1 < n {
                    leader_set.insert(i + 1);
                }
            }
            Op::Return | Op::Throw if i + 1 < n => {
                leader_set.insert(i + 1);
            }
            _ => {}
        }
    }
    let leaders: Vec<usize> = leader_set.into_iter().collect();
    let next_leader = |idx: usize| leaders.iter().copied().find(|&l| l > idx).unwrap_or(n);

    // 2. Propagate entry depths over the CFG (worklist). Guards deopt at a
    // PRECISE pc (the interpreter resumes mid-function with the live state),
    // so side effects before a guard are fine — no poisoning dimension.
    let mut entry_depth: HashMap<usize, usize> = HashMap::new();
    let mut entry_binds: HashMap<usize, usize> = HashMap::new();
    let mut entry_handlers: HashMap<usize, Vec<HandlerStatic>> = HashMap::new();
    entry_depth.insert(0, arity);
    entry_binds.insert(0, 0);
    entry_handlers.insert(0, Vec::new());
    let mut work = vec![0usize];
    let mut max_depth = arity;

    while let Some(l) = work.pop() {
        let mut cur = entry_depth[&l] as i64;
        let mut binds = entry_binds.get(&l).copied().unwrap_or(0);
        let mut handlers = entry_handlers.get(&l).cloned().unwrap_or_default();
        let end = next_leader(l);
        let mut terminated = false;
        for op in &ops[l..end] {
            // The terminator (if any) is always the last op of the block.
            match op {
                Op::Return => {
                    if cur < 1 {
                        return Err(CompileError::StackUnderflow);
                    }
                    // Returning with outstanding binds is fine: the frame
                    // unwind in CompiledLeaf::call unbinds to the entry base,
                    // exactly like cleanup_bytecode_frame.
                    terminated = true;
                    break;
                }
                Op::Throw => {
                    // [tag value] -> non-local exit; a terminator for compiled
                    // code (no local handlers exist — handler opcodes bail).
                    if cur < 2 {
                        return Err(CompileError::StackUnderflow);
                    }
                    terminated = true;
                    break;
                }
                Op::Goto(t) => {
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                    if cur < 1 {
                        return Err(CompileError::StackUnderflow);
                    }
                    cur -= 1; // pop the condition
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    // fall-through
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                    if cur < 1 {
                        return Err(CompileError::StackUnderflow);
                    }
                    // The jump preserves TOS (depth cur); the fall-through pops it.
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur - 1,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::Switch => {
                    // [dispatch table] -> jump to a static target or fall
                    // through on a miss. The target set was resolved in the
                    // leader pass.
                    if end >= n {
                        return Err(CompileError::NoReturn);
                    }
                    if cur < 2 {
                        return Err(CompileError::StackUnderflow);
                    }
                    cur -= 2;
                    // The Switch is the last op of its block (pass 1 made the
                    // following index a leader), so `end` is the fall-through.
                    let i = end - 1;
                    for &(_, t) in switch_targets.get(&i).expect("resolved in pass 1") {
                        push_succ(
                            &mut entry_depth,
                            &mut entry_binds,
                            &mut entry_handlers,
                            &mut work,
                            t,
                            cur,
                            binds,
                            &handlers,
                        )?;
                    }
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::PushConditionCase(t) | Op::PushConditionCaseRaw(t) | Op::PushCatch(t) => {
                    if end >= n {
                        // A push as the final op would fall off the end.
                        return Err(CompileError::NoReturn);
                    }
                    // Raw/Catch consume the conditions/tag operand first.
                    if !matches!(op, Op::PushConditionCase(_)) {
                        if cur < 1 {
                            return Err(CompileError::StackUnderflow);
                        }
                        cur -= 1;
                    }
                    // Handler edge: entered with the push-time stack plus the
                    // error value, the handler stack as of BEFORE this push
                    // (the matched frame and everything above it were popped
                    // by the unwind), and the push-time bind count (the catch
                    // restored the specpdl/bind state).
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur + 1,
                        binds,
                        &handlers,
                    )?;
                    if cur as usize + 1 > max_depth {
                        max_depth = cur as usize + 1;
                    }
                    handlers.push((*t as usize, cur as usize));
                    // Fall-through edge: same stack, handler now active.
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::PopHandler => {
                    // Normal exit from a protected extent: drop the innermost
                    // static handler. No stack effect; non-poisoning (the pop
                    // is a silent registration change — a deopt-rerun re-pushes
                    // and re-pops it after the frame unwind truncated ours).
                    if handlers.pop().is_none() {
                        return Err(CompileError::UnsupportedOp("unbalanced-pophandler"));
                    }
                }
                other => {
                    let (needs, delta) = simple_effect(other)?;
                    if cur < needs as i64 {
                        return Err(CompileError::StackUnderflow);
                    }
                    cur += delta;
                    if cur as usize > max_depth {
                        max_depth = cur as usize;
                    }
                    match other {
                        Op::VarBind(_)
                        | Op::SaveCurrentBuffer
                        | Op::SaveExcursion
                        | Op::SaveRestriction
                        | Op::UnwindProtectPop => binds += 1,
                        Op::Unbind(un) => {
                            let un = *un as usize;
                            if un > binds {
                                // Unbinding more than this function bound —
                                // bail to the interpreter (its bind_stack
                                // saturation handles it).
                                return Err(CompileError::UnsupportedOp("unbalanced-unbind"));
                            }
                            binds -= un;
                        }
                        _ => {}
                    }
                }
            }
        }
        if !terminated {
            // Block falls through into the next leader (guaranteed to exist and
            // be < n; a block running off the end with no Return is invalid).
            if end >= n {
                return Err(CompileError::NoReturn);
            }
            push_succ(
                &mut entry_depth,
                &mut entry_binds,
                &mut entry_handlers,
                &mut work,
                end,
                cur,
                binds,
                &handlers,
            )?;
        }
    }

    for &d in entry_depth.values() {
        max_depth = max_depth.max(d);
    }
    Ok(Cfg {
        leaders,
        entry_depth,
        entry_handlers,
        switch_targets,
        max_depth,
    })
}

/// Apply one non-terminator op's effect to the known-fixnum operand-stack model
/// `k` (parallel to the real operand stack: `k[i]` is `true` iff position `i` is
/// PROVABLY a fixnum). Returns `Err(())` for any op this analysis does not model
/// precisely, so the caller bails the whole function (conservative — no guard is
/// elided). Fixnum constants and fixnum arithmetic results are `true`;
/// StackRef/Dup/StackSet/DiscardN move bits; everything else is `false`.
fn apply_known_fixnum_op(op: &Op, constants: &[Value], k: &mut Vec<bool>) -> Result<(), ()> {
    match op {
        Op::Constant(idx) => {
            let is_fix = constants
                .get(*idx as usize)
                .map(|v| (v.bits() & FIXNUM_CHECK_MASK) == FIXNUM_CHECK_VALUE)
                .unwrap_or(false);
            k.push(is_fix);
        }
        Op::Nil | Op::True => k.push(false),
        Op::StackRef(j) => {
            let n = *j as usize;
            let v = *k.get(k.len().checked_sub(1 + n).ok_or(())?).ok_or(())?;
            k.push(v);
        }
        Op::Dup => {
            let v = *k.last().ok_or(())?;
            k.push(v);
        }
        Op::StackSet(j) => {
            let n = *j as usize;
            let v = k.pop().ok_or(())?;
            if n >= 1 {
                let idx = k.len().checked_sub(n).ok_or(())?;
                k[idx] = v;
            }
        }
        Op::Pop => {
            k.pop().ok_or(())?;
        }
        Op::DiscardN(raw) => {
            let n = (*raw & 0x7F) as usize;
            let preserve = (*raw & 0x80) != 0 && n > 0;
            if preserve {
                let tos = k.pop().ok_or(())?;
                let keep = k.len().checked_sub(n).ok_or(())?;
                k.truncate(keep);
                k.push(tos);
            } else {
                let keep = k.len().checked_sub(n).ok_or(())?;
                k.truncate(keep);
            }
        }
        // Fixnum arithmetic: the result is range-checked + retagged -> fixnum.
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem | Op::Max | Op::Min => {
            k.pop().ok_or(())?;
            k.pop().ok_or(())?;
            k.push(true);
        }
        Op::Add1 | Op::Sub1 | Op::Negate => {
            k.pop().ok_or(())?;
            k.push(true);
        }
        // Operand-consuming ops whose result is NOT a known fixnum: pop `needs`
        // (== the operands consumed for these) and push the results as unknown.
        // `simple_effect` is authoritative for the depth change.
        Op::Eq
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq
        | Op::Null
        | Op::Not
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Symbolp
        | Op::Integerp
        | Op::Numberp
        | Op::Car
        | Op::Cdr
        | Op::CarSafe
        | Op::CdrSafe
        | Op::Cons
        | Op::Aset
        | Op::List(_)
        | Op::Call(_)
        | Op::Apply(_)
        | Op::CallBuiltin(..)
        | Op::CallBuiltinSym(..)
        | Op::VarRef(_)
        | Op::VarSet(_)
        | Op::VarBind(_)
        | Op::Unbind(_)
        | Op::UnwindProtectPop
        | Op::SaveCurrentBuffer
        | Op::SaveExcursion
        | Op::SaveRestriction => {
            let (needs, delta) = simple_effect(op).map_err(|_| ())?;
            // These ops (unlike StackRef/Dup/StackSet/DiscardN) consume exactly
            // their top `needs` operands, so popping `needs` keeps `k` aligned.
            for _ in 0..needs {
                k.pop().ok_or(())?;
            }
            let pushes = needs as i64 + delta;
            for _ in 0..pushes.max(0) {
                k.push(false);
            }
        }
        // Direct/slice builtin dispatch (e.g. `1+`-as-subr): pop nargs, push one
        // unknown result. Detected the same way `simple_effect` does.
        other if direct_builtin_spec(other).is_some() || slice_builtin_spec(other).is_some() => {
            let (needs, delta) = simple_effect(other).map_err(|_| ())?;
            for _ in 0..needs {
                k.pop().ok_or(())?;
            }
            let pushes = needs as i64 + delta;
            for _ in 0..pushes.max(0) {
                k.push(false);
            }
        }
        // Anything else (Switch/handler ops/unmodeled) -> bail.
        _ => return Err(()),
    }
    Ok(())
}

/// **Cross-block redundant-guard elimination — the analysis.** Wired into
/// `lower_leaf_full` (and disabled under OSR); the "UNWIRED" this doc once
/// carried was stale.
///
/// Forward dataflow fixpoint over the CFG: for each block leader, the operand-
/// stack SLOTS provably fixnum at block entry. A slot is known-fixnum at entry
/// iff it is known-fixnum on EVERY predecessor edge (meet = AND); loops need the
/// fixpoint (the back-edge induction value depends on the slot's own bit). It is
/// a MUST analysis, so non-entry blocks start at TOP (all-`true`) and are
/// narrowed by predecessors; the entry block starts all-`false` (args untyped).
///
/// Conservative: returns an EMPTY map (no elision anywhere) for any function
/// containing an op this analysis does not model precisely (Switch, catch/
/// condition-case handlers, ...). NOT yet wired into `lower_leaf_full`; the
/// integration that consumes this is a follow-up. `cfg` must come from
/// [`analyze_cfg`] on the same `ops`.
fn compute_known_fixnum_slots(
    ops: &[Op],
    constants: &[Value],
    cfg: &Cfg,
) -> HashMap<usize, Vec<bool>> {
    let n = ops.len();
    let next_leader = |idx: usize| cfg.leaders.iter().copied().find(|&l| l > idx).unwrap_or(n);
    let empty = HashMap::new();

    // in[leader] = known-fixnum bits at block entry. Entry (0) is all-false;
    // every other block starts at TOP for the AND fixpoint.
    let mut in_sets: HashMap<usize, Vec<bool>> = HashMap::new();
    for &l in &cfg.leaders {
        let d = cfg.entry_depth.get(&l).copied().unwrap_or(0);
        in_sets.insert(l, vec![l != 0; d]);
    }

    // AND a predecessor contribution into a successor's in-set; report narrowing.
    fn meet(into: &mut [bool], contrib: &[bool]) -> bool {
        let mut changed = false;
        for (slot, &c) in into.iter_mut().zip(contrib.iter()) {
            if *slot && !c {
                *slot = false;
                changed = true;
            }
        }
        changed
    }

    let mut iterate = true;
    while iterate {
        iterate = false;
        for &l in &cfg.leaders {
            let mut k = in_sets[&l].clone();
            let end = next_leader(l);
            let mut edges: Vec<(usize, Vec<bool>)> = Vec::new();
            let mut terminated = false;
            for op in &ops[l..end] {
                match op {
                    Op::Return | Op::Throw => {
                        terminated = true;
                        break;
                    }
                    Op::Goto(t) => {
                        edges.push((*t as usize, k.clone()));
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                        if k.pop().is_none() {
                            return empty;
                        }
                        edges.push((*t as usize, k.clone()));
                        edges.push((end, k.clone()));
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                        // The jump preserves TOS; the fall-through pops it.
                        edges.push((*t as usize, k.clone()));
                        let mut ft = k.clone();
                        if ft.pop().is_none() {
                            return empty;
                        }
                        edges.push((end, ft));
                        terminated = true;
                        break;
                    }
                    other => {
                        if apply_known_fixnum_op(other, constants, &mut k).is_err() {
                            // Unmodeled op (Switch / handler / ...): bail entirely.
                            return empty;
                        }
                    }
                }
            }
            if !terminated {
                if end >= n {
                    return empty;
                }
                edges.push((end, k.clone()));
            }
            for (t, contrib) in &edges {
                if let Some(into) = in_sets.get_mut(t)
                    && meet(into, contrib)
                {
                    iterate = true;
                }
            }
        }
    }
    in_sets
}

/// Write the live operand `stack` back into the slot variables so a successor
/// block can read it (the variable/SSA machinery inserts the needed phis).
fn write_stack_to_vars(fb: &mut FunctionBuilder, vars: &[Variable], stack: &[ClifValue]) {
    for (k, &v) in stack.iter().enumerate() {
        fb.def_var(vars[k], v);
    }
}

/// Emit a backward jump with the interpreter's `branch_to!` parity: bump the
/// u8 quit counter; on every wrap (each 255th backward jump — counter resets to
/// 1, exactly like the interpreter) root the live operand stack and call the
/// back-edge service poll (GC safepoint + `maybe_quit`), propagating a signaled
/// `Flow` via the shared signal-exit block. The caller has already written the
/// operand stack to `vars` (the target's entry state).
#[allow(clippy::too_many_arguments)]
fn emit_backedge_jump(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    counter_slot: StackSlot,
    signal_exit: &mut Option<Block>,
    vars: &[Variable],
    target_depth: usize,
    target_block: Block,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
) {
    let c = fb.ins().stack_load(rt.ptr_ty, types::I64, counter_slot, 0);
    let c1 = fb.ins().iadd_imm_u(c, 1);
    let c1m = fb.ins().band_imm_u(c1, 0xFF);
    fb.ins().stack_store(rt.ptr_ty, c1m, counter_slot, 0);
    let wrapped = fb.ins().icmp_imm_u(IntCC::Equal, c1m, 0);
    let poll = fb.create_block();
    fb.ins().brif(wrapped, poll, &[], target_block, &[]);

    fb.switch_to_block(poll);
    fb.seal_block(poll);
    let one = fb.ins().iconst(types::I64, 1);
    fb.ins().stack_store(rt.ptr_ty, one, counter_slot, 0);
    // The live operand stack at the jump (already written to vars): rooted
    // across the poll, and the handler-entry snapshot if a quit signal lands
    // in a protected extent (condition-case catching `quit` around a loop).
    let vals: Vec<ClifValue> = (0..target_depth).map(|k| fb.use_var(vars[k])).collect();
    let saved = if vals.is_empty() {
        CondRoots::NONE
    } else {
        emit_cond_residual_roots_pre(fb, rt, &vals)
    };
    let vmctx = fb.use_var(rt.vmctx_var);
    let call = fb.ins().call(rt.refs.backedge, &[vmctx]);
    let status = fb.inst_results(call)[0];
    emit_cond_residual_roots_post(fb, rt, saved);
    let se = signal_target_for_site(fb, signal_exit, handlers, pending, &vals);
    let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
    fb.ins().brif(ok, target_block, &[], se, &[]);
}

/// Lower a leaf bytecode body taking `arity` fixed arguments to native code.
///
/// Whether the body has a BACKWARD jump (a loop) — needs the back-edge poll
/// (GC safepoint + quit), mirroring the interpreter's `branch_to!` wrap. Switch
/// targets count: a jump-table edge can also close a loop. Single source of truth
/// for both the JIT (`lower_leaf_full`) and the baseline-AOT emit (R2-E).
pub(crate) fn baseline_has_backedge(ops: &[Op], cfg: &Cfg) -> bool {
    ops.iter().enumerate().any(|(i, o)| match o {
        Op::Goto(t)
        | Op::GotoIfNil(t)
        | Op::GotoIfNotNil(t)
        | Op::GotoIfNilElsePop(t)
        | Op::GotoIfNotNilElsePop(t) => (*t as usize) <= i,
        _ => false,
    }) || cfg
        .switch_targets
        .iter()
        .any(|(i, ts)| ts.iter().any(|&(_, t)| t <= *i))
}

/// Whether the body re-enters the runtime (needs vmctx + the `neovm_jit_*` shim
/// scaffolding): a back-edge polls through vmctx; Eq/Symbolp use the
/// symbols-with-pos slow path; VarRef/VarSet/VarBind/Unbind hit the variable
/// machinery; Call/Apply/named-builtins/handlers re-enter elisp. Single source of
/// truth for both the JIT and the baseline-AOT emit (R2-E).
pub(crate) fn baseline_needs_rt(ops: &[Op], has_backedge: bool) -> bool {
    has_backedge
        || ops.iter().any(|o| {
            direct_builtin_spec(o).is_some()
                || slice_builtin_spec(o).is_some()
                || matches!(
                    o,
                    Op::List(_)
                        | Op::CallBuiltin(..)
                        | Op::CallBuiltinSym(..)
                        | Op::Aset
                        | Op::SaveWindowExcursion
                )
                || matches!(
                    o,
                    Op::Cons
                        | Op::Call(_)
                        | Op::Apply(_)
                        | Op::Eq
                        | Op::Symbolp
                        | Op::VarRef(_)
                        | Op::VarSet(_)
                        | Op::VarBind(_)
                        | Op::Unbind(_)
                        | Op::SaveCurrentBuffer
                        | Op::SaveExcursion
                        | Op::SaveRestriction
                        | Op::UnwindProtectPop
                        | Op::Throw
                        | Op::Integerp
                        | Op::Numberp
                        | Op::PushConditionCase(_)
                        | Op::PushConditionCaseRaw(_)
                        | Op::PushCatch(_)
                        | Op::PopHandler
                        | Op::Switch
                )
        })
}

/// Handles arbitrary intra-function control flow (`Goto`/`GotoIf*`) by building a
/// CLIF basic-block CFG: each bytecode basic block becomes a CLIF block, and the
/// operand stack flows across edges through per-slot SSA variables (Cranelift
/// inserts the phis). The `arity` arguments are loaded and seed the bottom of the
/// stack (arg0 deepest), exactly as the interpreter's `run_frame` pushes them.
pub fn lower_leaf(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
) -> Result<CompiledLeaf, CompileError> {
    lower_leaf_with_map(ops, constants, arity, None)
}

/// [`lower_leaf`] with the function's GNU byte-offset map, needed to resolve
/// `Op::Switch` jump-table addresses to instruction indices (GNU bytecode
/// stores byte offsets; natively compiled chunks store indices directly).
pub fn lower_leaf_with_map(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
) -> Result<CompiledLeaf, CompileError> {
    lower_leaf_full(ops, constants, arity, offset_map, None, 0)
}

/// [`lower_leaf_with_map`] plus the compiling thread's obarray, enabling
/// direct-call speculation (constant-symbol callees bound to bytecode get
/// epoch-validated direct calls; see [`find_spec_sites`]).
pub fn lower_leaf_full(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    obarray: Option<&Obarray>,
    dynamic_prefix: usize,
) -> Result<CompiledLeaf, CompileError> {
    lower_leaf_full_osr(
        ops,
        constants,
        arity,
        offset_map,
        obarray,
        None,
        dynamic_prefix,
    )
}

/// The compile-time view of a source with a `make-closure` patched prefix: the
/// per-instance slots read as `nil` so that NO analysis (symbol tags, spec
/// sites, known-fixnum elision, reloc collection, arith intrinsics) treats the
/// triggering instance's captured value — or the prototype's `V0..Vn`
/// placeholder — as a property of the SOURCE. The emitter never consults the
/// masked value for those slots: it loads them through the executing callee.
fn mask_dynamic_prefix(constants: &[Value], dynamic_prefix: usize) -> Vec<Value> {
    constants
        .iter()
        .enumerate()
        .map(|(i, v)| if i < dynamic_prefix { Value::NIL } else { *v })
        .collect()
}

/// [`lower_leaf_full`] plus an optional OSR entry pc (on-stack replacement,
/// JIT-only): when `Some(osr_pc)`, the compiled function's entry seeds the live
/// operand stack (from the `args` pointer) and jumps to the loop-header block at
/// `osr_pc`, letting the interpreter transfer a hot loop into native code
/// mid-execution. Cross-block known-fixnum elision is DISABLED for OSR (the
/// analysis assumes the normal block-0 entry as the sole root; the OSR entry adds
/// a predecessor it never saw, so every fixnum op guards — a non-fixnum simply
/// deopts, always sound).
pub fn lower_leaf_full_osr(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    obarray: Option<&Obarray>,
    osr_pc: Option<usize>,
    dynamic_prefix: usize,
) -> Result<CompiledLeaf, CompileError> {
    // Every analysis and the reloc collection below see the MASKED view; only
    // the emitter's `Op::Constant` arm knows the prefix (it loads those slots
    // through the callee at run time).
    let masked;
    let constants: &[Value] = if dynamic_prefix > 0 {
        masked = mask_dynamic_prefix(constants, dynamic_prefix);
        &masked
    } else {
        constants
    };
    let cfg = analyze_cfg(ops, constants, offset_map, arity)?;
    // Cross-block redundant-guard elimination: per-block-entry known-fixnum slots
    // (empty if the function has an op the analysis doesn't model -> no elision).
    // Disabled under OSR (see the doc): the OSR entry is an unanalyzed predecessor.
    let known_fixnum_slots = if osr_pc.is_some() {
        HashMap::new()
    } else {
        compute_known_fixnum_slots(ops, constants, &cfg)
    };
    let n = ops.len();
    // Direct-call speculation sites + their armed-epoch slots. The Box's heap
    // storage is address-stable: slot pointers are baked into the generated
    // code as immediates and the Box moves into the CompiledLeaf at the end.
    let (spec_sites, spec_slots): (HashMap<usize, SpecSite>, Box<[SpecSlot]>) = match obarray {
        Some(ob) => {
            let sites = find_spec_sites(ops, constants, &cfg.leaders, ob);
            let slots: Box<[SpecSlot]> = (0..sites.len())
                .map(|_| SpecSlot {
                    epoch: AtomicU64::new(0),
                    leaf: AtomicU64::new(0),
                })
                .collect();
            // Arm every slot with the epoch the bindings were observed at; any
            // bump before first execution self-heals via shim re-validation.
            let epoch = ob.function_epoch();
            for site in sites.values() {
                slots[site.slot].epoch.store(epoch, Ordering::Relaxed);
            }
            (sites, slots)
        }
        None => (HashMap::new(), Box::from([])),
    };
    // Precise-deopt buffers: live operand-stack spill (max depth) + the
    // pc/depth/handler-count cells. Address-stable Boxes owned by the leaf;
    // generated code writes through baked raw addresses.
    let deopt_spill: Box<[core::cell::Cell<i64>]> = (0..cfg.max_depth)
        .map(|_| core::cell::Cell::new(0))
        .collect();
    let deopt_meta: Box<DeoptCells> = Box::new(DeoptCells {
        pc: core::cell::Cell::new(0),
        depth: core::cell::Cell::new(0),
        handlers: core::cell::Cell::new(0),
    });

    // R1a: per-leaf heap-constant reloc vector (see lower_mir_pure). The baseline's
    // Op::Constant loads from reloc_data[idx] instead of baking a heap pointer, so
    // the code is GC-pointer-free + AOT-portable. Allocated here (before the
    // FunctionBuilder) so reloc_data.as_ptr() is stable when the load bakes its base.
    let mut reloc_vals: Vec<Value> = Vec::new();
    let mut reloc_index: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    for op in ops {
        if let Op::Constant(idx) = op
            && let Some(v) = constants.get(*idx as usize)
            && v.is_heap_object()
            && !reloc_index.contains_key(&v.bits())
        {
            reloc_index.insert(v.bits(), reloc_vals.len() as u32);
            reloc_vals.push(*v);
        }
    }
    let reloc_data: Box<[Value]> = reloc_vals.into_boxed_slice();

    // Baseline tier runs Cranelift at the default opt_level="none": its job is
    // FAST compilation (low tier-up latency; the soak compiles every function).
    // Measured opt_level="speed" (2026-06-13): no runtime win on fib (call-
    // bound) or the arithmetic loop, because Cranelift sees our tagged Values
    // as opaque i64 — it can't unbox, drop fixnum guards, or reason about lisp
    // effects. The real headroom is semantic (unboxing/inlining), which needs
    // an MIR-level optimizing Tier-2; opt_level="speed" belongs there, not at
    // this tier where it would only cost compile time.
    let mut builder = JITBuilder::with_isa(jit_isa()?, default_libcall_names());
    builder.symbol("neovm_jit_gc_save", neovm_jit_gc_save as *const u8);
    builder.symbol("neovm_jit_gc_push", neovm_jit_gc_push as *const u8);
    builder.symbol(
        "neovm_jit_gc_push_many",
        neovm_jit_gc_push_many as *const u8,
    );
    builder.symbol("neovm_jit_gc_restore", neovm_jit_gc_restore as *const u8);
    builder.symbol(
        "neovm_jit_rootwin_grow",
        neovm_jit_rootwin_grow as *const u8,
    );
    builder.symbol("neovm_jit_cons", neovm_jit_cons as *const u8);
    builder.symbol("neovm_jit_call", neovm_jit_call as *const u8);
    builder.symbol("neovm_jit_apply", neovm_jit_apply as *const u8);
    builder.symbol("neovm_jit_eq_slow", neovm_jit_eq_slow as *const u8);
    builder.symbol(
        "neovm_jit_symbolp_slow",
        neovm_jit_symbolp_slow as *const u8,
    );
    builder.symbol("neovm_jit_varref", neovm_jit_varref as *const u8);
    builder.symbol("neovm_jit_varset", neovm_jit_varset as *const u8);
    builder.symbol("neovm_jit_varbind", neovm_jit_varbind as *const u8);
    builder.symbol("neovm_jit_unbind", neovm_jit_unbind as *const u8);
    builder.symbol("neovm_jit_backedge", neovm_jit_backedge as *const u8);
    builder.symbol(
        "neovm_jit_save_current_buffer",
        neovm_jit_save_current_buffer as *const u8,
    );
    builder.symbol(
        "neovm_jit_save_excursion",
        neovm_jit_save_excursion as *const u8,
    );
    builder.symbol(
        "neovm_jit_save_restriction",
        neovm_jit_save_restriction as *const u8,
    );
    builder.symbol("neovm_jit_throw", neovm_jit_throw as *const u8);
    builder.symbol(
        "neovm_jit_integerp_slow",
        neovm_jit_integerp_slow as *const u8,
    );
    builder.symbol(
        "neovm_jit_numberp_slow",
        neovm_jit_numberp_slow as *const u8,
    );
    builder.symbol(
        "neovm_jit_unwind_protect",
        neovm_jit_unwind_protect as *const u8,
    );
    builder.symbol("neovm_jit_builtin1", neovm_jit_builtin1 as *const u8);
    builder.symbol("neovm_jit_builtin2", neovm_jit_builtin2 as *const u8);
    builder.symbol("neovm_jit_builtin3", neovm_jit_builtin3 as *const u8);
    builder.symbol("neovm_jit_push_cc", neovm_jit_push_cc as *const u8);
    builder.symbol("neovm_jit_push_cc_raw", neovm_jit_push_cc_raw as *const u8);
    builder.symbol("neovm_jit_push_catch", neovm_jit_push_catch as *const u8);
    builder.symbol("neovm_jit_pop_handler", neovm_jit_pop_handler as *const u8);
    builder.symbol(
        "neovm_jit_match_handler",
        neovm_jit_match_handler as *const u8,
    );
    builder.symbol("neovm_jit_switch", neovm_jit_switch as *const u8);
    builder.symbol(
        "neovm_jit_switch_stale",
        neovm_jit_switch_stale as *const u8,
    );
    builder.symbol("neovm_jit_list", neovm_jit_list as *const u8);
    builder.symbol(
        "neovm_jit_builtin_slice",
        neovm_jit_builtin_slice as *const u8,
    );
    builder.symbol(
        "neovm_jit_named_builtin",
        neovm_jit_named_builtin as *const u8,
    );
    builder.symbol(
        "neovm_jit_save_window_excursion",
        neovm_jit_save_window_excursion as *const u8,
    );
    builder.symbol("neovm_jit_call_spec", neovm_jit_call_spec as *const u8);
    // Gap 1 (JIT-only, by address — never in the dynamic symbol table): the
    // subr-speculation shims. Registered unconditionally (harmless when the
    // body has no subr sites; the declares are gated instead).
    builder.symbol(
        "neovm_jit_call_subr_spec",
        neovm_jit_call_subr_spec as *const u8,
    );
    builder.symbol("neovm_jit_pred_spec", neovm_jit_pred_spec as *const u8);
    builder.symbol(
        "neovm_jit_eq_incl_props_spec",
        neovm_jit_eq_incl_props_spec as *const u8,
    );
    builder.symbol("neovm_jit_arith_spec", neovm_jit_arith_spec as *const u8);
    // R2 CallBuiltinSym intrinsic shims (Tier-B dispatch-skip + Tier-A GC-free
    // read). Unconditional registration (declares are gated on a CBSym spec
    // site); JIT-only.
    builder.symbol("neovm_jit_cbsym_spec", neovm_jit_cbsym_spec as *const u8);
    builder.symbol("neovm_jit_cbsym_read", neovm_jit_cbsym_read as *const u8);
    let mut module = JITModule::new(builder);
    // has_backedge + needs_rt via the shared single-source helpers (R2-E) — same
    // logic as before, just factored so the baseline-AOT emit can reuse it.
    let has_backedge = baseline_has_backedge(ops, &cfg);
    let needs_rt = baseline_needs_rt(ops, has_backedge);

    // Build + define the leaf into the module via the module-generic seam
    // (`build_leaf_fn`). Buffers (`spec_slots`/`deopt_*`/`reloc_data`) are owned
    // here, threaded in by reference so their baked addresses stay stable, and
    // moved into the returned `CompiledLeaf` below.
    let fid = build_leaf_fn(
        &mut module,
        ops,
        constants,
        arity,
        &cfg,
        &known_fixnum_slots,
        &spec_sites,
        &spec_slots,
        n,
        &deopt_spill,
        &deopt_meta,
        &reloc_data,
        &reloc_index,
        has_backedge,
        needs_rt,
        /*aot=*/ false,
        "__neovm_jit_leaf",
        Linkage::Local,
        osr_pc,
        dynamic_prefix,
    )?;

    // --- JIT-only module epilogue (the wrapper). ----------------------------
    module
        .finalize_definitions()
        .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;

    let entry = module.get_finalized_function(fid);
    Ok(CompiledLeaf {
        tier: LeafTier::Baseline,
        arity,
        // Plain fixed-arity defaults; compile_bytecode_function overrides for
        // &optional/&rest lambda lists.
        required: arity,
        has_rest: false,
        // The baseline never inlines; only the MIR tier sets this.
        inline_epoch: None,
        // The baseline is all-precise (every guard is STATUS_DEOPT_AT, never a
        // rerun-from-start after a call), so it never needs the refuse-to-rerun.
        has_side_effects: false,
        // The baseline never inlines.
        inline_deps: Box::from([]),
        has_binds: ops.iter().any(|o| {
            matches!(
                o,
                Op::VarBind(_)
                    | Op::Unbind(_)
                    | Op::SaveCurrentBuffer
                    | Op::SaveExcursion
                    | Op::SaveRestriction
                    | Op::UnwindProtectPop
            )
        }),
        has_handlers: ops.iter().any(|o| {
            matches!(
                o,
                Op::PushConditionCase(_) | Op::PushConditionCaseRaw(_) | Op::PushCatch(_)
            )
        }),
        spec_slots,
        // JIT bakes each site's `expected` as an iconst; no sidecar array needed.
        spec_expected: Box::from([]),
        deopt_spill,
        deopt_meta,
        reloc_data,
        // JIT bakes its bases as iconst; the 4th entry arg is the executing
        // callee's constant base, read only when `dynamic_prefix > 0`.
        sidecar: None,
        dynamic_prefix: u32::try_from(dynamic_prefix).expect("patched prefix fits u32"),
        entry,
        _backing: LeafBacking::Jit(module),
    })
}

/// R2-E: the BASELINE-tier metadata an AOT leaf's descriptor needs (the loader
/// sizes the per-thread deopt buffers + records the frame shape from this).
pub(crate) struct BaselineAotMeta {
    pub arity: usize,
    /// `cfg.max_depth` — the deopt operand-stack spill size.
    pub max_depth: usize,
    /// VarBind/Unbind/save-* present (the loader's `has_binds`).
    pub has_binds: bool,
    /// condition-case/catch handlers present.
    pub has_handlers: bool,
    /// R2 increment B2: the `Op::Call` subr/bytecode spec sites baked into this
    /// leaf (in slot order — descriptor position == codegen slot index). Empty when
    /// emitted without a live obarray (CBSym-only, increment A). The descriptor
    /// carries these so the loader can re-classify + arm each runtime `SpecSlot`.
    pub spec_sites: Vec<super::aot::AotSpecSite>,
}

/// R2-E (baseline-tier AOT emit): lower a bytecode body through the BASELINE tier
/// into `module` as an AOT object entry (`build_leaf_fn::<M>(aot=true)`), for
/// bodies the MIR tier rejects (Switch/Throw/handlers/CallBuiltin(Sym)/VarRef...).
///
/// Replicates [`lower_leaf_full`]'s analysis prologue but: (1) `obarray=None`, so
/// the `Op::Call` subr speculation is skipped (increment B), but the CBSym
/// intrinsic sites ARE classified (name-canonical + obarray-free — increment A);
/// CBSym is slotless, so there is still no per-spec-slot baked state and the deopt
/// fits the sidecar's existing bases; (2) the reloc set is the
/// caller's (covering const-relocs + the named-builtin op-symbols, #16/#17), with
/// the index keyed by tagged bits; (3) `aot=true` so reloc/deopt bases load from
/// the sidecar; (4) the entry is `Linkage::Export` under `entry_name` for the
/// loader to `dlsym`. The deopt buffers are sized-but-unbaked (AOT reads bases
/// from the sidecar, not these addresses) — throwaway here.
///
/// Returns the [`BaselineAotMeta`] for the descriptor.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_baseline_leaf_object<M: Module>(
    module: &mut M,
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    reloc_data: &[Value],
    reloc_index: &std::collections::HashMap<usize, u32>,
    entry_name: &str,
    // R2 increment B2: the compiling thread's obarray. `Some` → the `Op::Call`
    // subr/bytecode speculation pass runs (spec fast paths baked, descriptor
    // entries emitted); `None` → CBSym-only (increment A, obarray-free).
    obarray: Option<&Obarray>,
) -> Result<BaselineAotMeta, CompileError> {
    let cfg = analyze_cfg(ops, constants, None, arity)?;
    let known_fixnum_slots = compute_known_fixnum_slots(ops, constants, &cfg);
    let n = ops.len();
    // Classify speculation sites. With a LIVE obarray (increment B2) the full
    // `Op::Call` subr/bytecode pass runs alongside the obarray-free CBSym pass;
    // without one, only CBSym (increment A). CBSym is SLOTLESS/EPOCHLESS (its
    // lowering reads only the `kind`, never the slot/expected), so it needs no
    // descriptor entry; the `Op::Call` spec sites DO (the loader re-classifies +
    // arms each). `finalize_baseline_spec_sites` DROPs an un-reloc'd callee, dense-
    // renumbers the surviving slots, and returns the descriptor sites in slot order.
    let mut spec_sites: HashMap<usize, SpecSite> = match obarray {
        Some(ob) => find_spec_sites(ops, constants, &cfg.leaders, ob),
        None => find_cbsym_spec_sites(ops),
    };
    let aot_spec_sites = finalize_baseline_spec_sites(&mut spec_sites, ops, reloc_index);
    let spec_slots: Box<[SpecSlot]> = (0..spec_sites.len())
        .map(|_| SpecSlot {
            epoch: AtomicU64::new(0),
            leaf: AtomicU64::new(0),
        })
        .collect();
    let has_backedge = baseline_has_backedge(ops, &cfg);
    let needs_rt = baseline_needs_rt(ops, has_backedge);
    // Sized-but-unbaked deopt buffers: in AOT mode build_leaf_fn loads the spill +
    // meta bases from the SIDECAR (not these addresses), so these are throwaway
    // (the per-thread real buffers are allocated by the loader from the descriptor).
    let deopt_spill: Box<[core::cell::Cell<i64>]> = (0..cfg.max_depth)
        .map(|_| core::cell::Cell::new(0))
        .collect();
    let deopt_meta: Box<DeoptCells> = Box::new(DeoptCells {
        pc: core::cell::Cell::new(0),
        depth: core::cell::Cell::new(0),
        handlers: core::cell::Cell::new(0),
    });
    let max_depth = cfg.max_depth;
    build_leaf_fn(
        module,
        ops,
        constants,
        arity,
        &cfg,
        &known_fixnum_slots,
        &spec_sites,
        &spec_slots,
        n,
        &deopt_spill,
        &deopt_meta,
        reloc_data,
        reloc_index,
        has_backedge,
        needs_rt,
        /*aot=*/ true,
        entry_name,
        Linkage::Export,
        /*osr_pc=*/ None, // OSR is JIT-only
        /*dynamic_prefix=*/ 0, // AOT never targets a patched source
    )?;
    Ok(BaselineAotMeta {
        arity,
        max_depth,
        has_binds: ops.iter().any(|o| {
            matches!(
                o,
                Op::VarBind(_)
                    | Op::Unbind(_)
                    | Op::SaveCurrentBuffer
                    | Op::SaveExcursion
                    | Op::SaveRestriction
                    | Op::UnwindProtectPop
            )
        }),
        has_handlers: ops.iter().any(|o| {
            matches!(
                o,
                Op::PushConditionCase(_) | Op::PushConditionCaseRaw(_) | Op::PushCatch(_)
            )
        }),
        spec_sites: aot_spec_sites,
    })
}

/// Module-generic build seam for [`lower_leaf_full`]: sets up the leaf ABI
/// signature, lowers the bytecode `ops` through a `FunctionBuilder`, then
/// declares + defines the function into `module`, returning its `FuncId`. CLIF
/// output is byte-identical to the previous in-line lowering (pure extraction).
///
/// Generic over `M: Module` so the same lowering drives the `JITModule` JIT
/// path today and an `ObjectModule` AOT path later, unchanged. The
/// address-stable buffers (`spec_slots`/`deopt_spill`/`deopt_meta`/`reloc_data`)
/// are borrowed: their addresses are baked into the generated code, and the
/// caller retains ownership to move them into the `CompiledLeaf`.
///
/// This fn deliberately contains NONE of the three ObjectModule-incompatible
/// JIT seams, which stay in the [`lower_leaf_full`] wrapper:
///   * `builder.symbol(...)`    — AOT: `Linkage::Import` resolved via dlopen.
///   * `finalize_definitions()` — AOT: `ObjectModule::finish()`.
///   * `get_finalized_function` — AOT: `dlsym` of the exported entry symbol.
#[allow(clippy::too_many_arguments)]
fn build_leaf_fn<M: Module>(
    module: &mut M,
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    cfg: &Cfg,
    known_fixnum_slots: &HashMap<usize, Vec<bool>>,
    spec_sites: &HashMap<usize, SpecSite>,
    spec_slots: &[SpecSlot],
    n: usize,
    deopt_spill: &[core::cell::Cell<i64>],
    deopt_meta: &DeoptCells,
    reloc_data: &[Value],
    reloc_index: &std::collections::HashMap<usize, u32>,
    has_backedge: bool,
    needs_rt: bool,
    // R2-E (baseline-tier AOT): false → JIT (bases baked as `iconst`, byte-identical
    // to before); true → AOT (reloc-base + deopt bases loaded from the per-thread
    // `LeafSidecar` 4th entry arg, since the addresses are session-specific). Mirrors
    // `build_mir_leaf_fn`'s `aot` flag. Same RESULTS either way.
    aot: bool,
    // The exported entry symbol + its linkage. JIT: `("__neovm_jit_leaf", Local)`.
    // AOT: the content-hash entry name + `Export` so the loader can `dlsym` it.
    entry_name: &str,
    entry_linkage: Linkage,
    // OSR (on-stack replacement, JIT-only): when `Some(osr_pc)`, the function
    // entry does NOT seed args + jump to block 0; instead it seeds the operand
    // stack (`entry_depth[osr_pc]` tagged Values read from the `args` pointer)
    // and jumps STRAIGHT to the loop-header block at `osr_pc`, so the interpreter
    // can transfer a hot loop into native code mid-execution. Blocks unreachable
    // from `osr_pc` (the pre-loop prologue) are pruned so no dangling SSA survives.
    osr_pc: Option<usize>,
    // `make-closure` patched prefix of the source (JIT only): those leading
    // constant slots load through the callee constant base in the 4th entry
    // param instead of baking. 0 = plain function / AOT.
    dynamic_prefix: usize,
) -> Result<cranelift_module::FuncId, CompileError> {
    LAST_IR_STATS.with(|c| c.set((0, 0, 0, 0)));
    let frontend_config = module.target_config();
    let call_conv = frontend_config.default_call_conv;
    let ptr_ty = frontend_config.pointer_type();

    // ABI: fn(vmctx: *mut Context, args: *const i64, out: *mut i64) -> i64.
    // Reads `arity` argument words from `args`; returns STATUS_OK + writes the
    // result bits via `out` on success, STATUS_DEOPT on a failed guard, or
    // STATUS_SIGNAL when a runtime call raised a Flow (stashed for
    // `take_pending_flow`). `vmctx` is only used by runtime-call shims.
    // Unified 4-param entry ABI: fn(vmctx, args, out, sidecar) -> status (see
    // build_mir_leaf_fn). JIT (`aot=false`) declares but ignores `sidecar` (bases
    // stay `iconst`); AOT (`aot=true`) reads its reloc/deopt bases from it.
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty)); // vmctx
    sig.params.push(AbiParam::new(ptr_ty)); // args
    sig.params.push(AbiParam::new(ptr_ty)); // out
    sig.params.push(AbiParam::new(ptr_ty)); // sidecar (*const LeafSidecar)
    sig.returns.push(AbiParam::new(types::I64));

    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);

        // Declare the runtime-call machinery into this function if the body
        // re-enters the runtime (`Cons` / `Call`).
        let rt = if needs_rt {
            // Declare the JIT-only round-1 subr-speculation shims iff the body has
            // round-1 subr-kind spec sites. The AOT baseline emit classifies ONLY
            // CBSym sites (`find_cbsym_spec_sites`; the `Op::Call` subr pass is
            // increment B), so this is always false for AOT — an ObjectModule
            // never declares the round-1 subr import names. CBSym-kind sites are
            // deliberately NOT counted here: they get their own R2 shims, so a
            // CBSym-only body never imports the `Op::Call` spec shims.
            let subr_spec = spec_sites.values().any(|site| site.kind.is_round1_subr());
            // The R2 CallBuiltinSym intrinsic shims (Tier-A read / Tier-B
            // dispatch-skip), declared when a CBSym-kind site exists. UNLIKE the
            // round-1 shims these are NOW emitted by AOT too (increment A): CBSym
            // classification is obarray-free, so `find_cbsym_spec_sites` populates
            // this for the baseline `ObjectModule` and the two shims become imports
            // resolved against the host at `dlopen`.
            let cbsym_spec = spec_sites.values().any(|site| site.kind.is_cbsym());
            // `module` is already `&mut M`; reborrow it for the call.
            let refs = declare_rt_refs(
                &mut *module,
                fb.func,
                call_conv,
                ptr_ty,
                subr_spec,
                cbsym_spec,
            )?;
            let vmctx_var = fb.declare_var(ptr_ty);
            let max_call_args = ops
                .iter()
                .filter_map(|o| match o {
                    Op::Call(n) | Op::Apply(n) | Op::List(n) | Op::Concat(n) => Some(*n as usize),
                    Op::CallBuiltin(_, n) | Op::CallBuiltinSym(_, n) => Some(*n as usize),
                    Op::Nconc => Some(2),
                    Op::Substring | Op::Aset => Some(3),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let call_args_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (max_call_args.max(1) * 8) as u32,
                3,
            ));
            let call_result_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            // Lever 2: residual gather buffer, sized to the operand-stack depth
            // (an upper bound on any site's residual count).
            let residual_buf_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (cfg.max_depth.max(1) * 8) as u32,
                3,
            ));
            let gc_saved_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            Some(RtCtx {
                refs,
                vmctx_var,
                ptr_ty,
                call_args_slot,
                call_result_slot,
                residual_buf_slot,
                gc_saved_slot,
            })
        } else {
            None
        };

        // SSA variables: one I64 slot per operand-stack position (carries the
        // stack across block edges), plus one for the out pointer (used by
        // `Return` in any block).
        let vars: Vec<Variable> = (0..cfg.max_depth)
            .map(|_| fb.declare_var(types::I64))
            .collect();
        let out_var = fb.declare_var(ptr_ty);

        // One CLIF block per bytecode basic block.
        let block_for: HashMap<usize, Block> = cfg
            .leaders
            .iter()
            .map(|&l| (l, fb.create_block()))
            .collect();
        // Deopt-buffer base addresses (for the JIT `iconst` path). The CLIF
        // `DeoptRefs` is materialized in the entry block below (the baseline tier
        // has no AOT path yet, so always the `iconst` form).
        let spill_base_addr = deopt_spill.as_ptr() as i64;
        let meta_pc_addr = &deopt_meta.pc as *const core::cell::Cell<i64> as i64;
        let meta_depth_addr = &deopt_meta.depth as *const core::cell::Cell<i64> as i64;
        let meta_handlers_addr = &deopt_meta.handlers as *const core::cell::Cell<i64> as i64;
        // Shared signal-propagation block (returns STATUS_SIGNAL), created
        // lazily by the first `Call` lowering.
        let mut signal_exit: Option<Block> = None;
        // Backward-jump quit counter (the interpreter's u8 `quitcounter`), kept
        // in a stack slot so every block can bump it.
        let backedge_counter: Option<StackSlot> = has_backedge.then(|| {
            fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3))
        });

        // Function-entry block: stash vmctx + the out pointer, load args into
        // the slot variables, then jump into bytecode block 0.
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        let vmctx_param = fb.block_params(entry)[0];
        let args_ptr = fb.block_params(entry)[1];
        let out_ptr = fb.block_params(entry)[2];
        // R2-E: the 4th entry param (the per-thread `*const LeafSidecar`). Read only
        // in AOT mode; JIT ignores it. The entry block dominates every block, so a
        // base materialized here is valid in any (incl. cold deopt) block.
        let sidecar_param = aot.then(|| fb.block_params(entry)[3]);
        // JIT leaf of a `make-closure`-patched source: the same 4th entry param is
        // the EXECUTING CALLEE's constant base (`CompiledLeaf::call_consts`); the
        // patched slots load off it (`lower_simple_op` `Op::Constant`). Bound in
        // the entry block so it dominates every block.
        debug_assert!(
            !(aot && dynamic_prefix > 0),
            "AOT never targets a patched source"
        );
        let consts_base = (!aot && dynamic_prefix > 0).then(|| fb.block_params(entry)[3]);
        // R1a: base address of the heap-constant reloc vector, materialized once in
        // entry (dominates all blocks); the baseline Op::Constant loads off it by
        // index. JIT bakes the Box address as `iconst`; AOT loads it from the
        // sidecar (session-specific). `None` when the body references no heap consts.
        let reloc_base = if reloc_data.is_empty() {
            None
        } else if aot {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            Some(fb.ins().load(
                ptr_ty,
                MemFlagsData::trusted(),
                sc,
                LeafSidecar::OFF_RELOC_BASE,
            ))
        } else {
            Some(fb.ins().iconst(ptr_ty, reloc_data.as_ptr() as i64))
        };
        // R2 increment B2: the AOT sidecar's spec-slot / spec-expected array bases,
        // loaded ONCE here (the entry block dominates every block, incl. cold deopt)
        // iff this is an AOT body with at least one `Op::Call` subr/bytecode spec
        // site (CBSym sites are slotless — they read neither base). JIT
        // (`aot=false`) emits NOTHING here (bases stay `None`; each site bakes its
        // slot/expected as `iconst`), so the JIT lowering is byte-identical to
        // pre-B2. `None` when the leaf has no such site, so a null sidecar base is
        // never loaded/indexed.
        let has_op_call_spec = spec_sites.values().any(|s| s.kind.to_spec_disc().is_some());
        let (spec_slot_base, spec_expected_base) = if aot && has_op_call_spec {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            (
                Some(fb.ins().load(
                    ptr_ty,
                    MemFlagsData::trusted(),
                    sc,
                    LeafSidecar::OFF_SPEC_SLOT_BASE,
                )),
                Some(fb.ins().load(
                    ptr_ty,
                    MemFlagsData::trusted(),
                    sc,
                    LeafSidecar::OFF_SPEC_EXPECTED_BASE,
                )),
            )
        } else {
            (None, None)
        };
        // Deopt-buffer bases as entry-block values: JIT (`aot=false`) → the `iconst`
        // form (deferred to the cold deopt blocks, byte-identical to pre-R2-E); AOT
        // (`aot=true`) → loaded from the sidecar. The baseline's precise deopt spills
        // operand-stack + pc/depth/handlers — exactly the sidecar's carried bases (at
        // D0 there are no spec slots, so no per-spec-slot state beyond these).
        let deopt_refs = materialize_deopt_refs(
            &mut fb,
            ptr_ty,
            aot,
            /*has_precise_deopt=*/ true,
            sidecar_param,
            spill_base_addr,
            meta_pc_addr,
            meta_depth_addr,
            meta_handlers_addr,
        );
        if let Some(rt) = &rt {
            fb.def_var(rt.vmctx_var, vmctx_param);
        }
        fb.def_var(out_var, out_ptr);
        if let Some(slot) = backedge_counter {
            // The interpreter starts quitcounter at 1.
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().stack_store(ptr_ty, one, slot, 0);
        }
        // Entry seeding + jump target. Normal: seed the `arity` args into the
        // bottom slots and jump to bytecode block 0. OSR: the `args` pointer holds
        // the live OPERAND STACK snapshot (`entry_depth[osr_pc]` tagged Values), so
        // seed those slots and jump STRAIGHT to the loop-header block at `osr_pc`.
        let (seed_count, jump_target) = match osr_pc {
            Some(p) => (cfg.entry_depth[&p], block_for[&p]),
            None => (arity, block_for[&0]),
        };
        for (i, var) in vars.iter().take(seed_count).enumerate() {
            let v = fb.ins().load(
                types::I64,
                MemFlagsData::trusted(),
                args_ptr,
                (i * 8) as i32,
            );
            fb.def_var(*var, v);
        }
        fb.ins().jump(jump_target, &[]);

        let next_leader = |idx: usize| cfg.leaders.iter().copied().find(|&l| l > idx).unwrap_or(n);

        for &l in &cfg.leaders {
            let blk = block_for[&l];
            fb.switch_to_block(blk);
            // Materialize the incoming operand stack from the slot variables.
            let depth = cfg.entry_depth[&l];
            let mut stack: Vec<ClifValue> = (0..depth).map(|k| fb.use_var(vars[k])).collect();
            // Cross-op unboxing: incoming slots are tagged (loaded from vars). Raw
            // (untagged) fixnums live only WITHIN a block; the mask resets to
            // all-tagged at each block entry (no cross-block raw in this increment).
            let mut stack_raw: Vec<bool> = vec![false; depth];
            // Cross-block known-fixnum operands at this block's entry: each slot
            // the dataflow analysis proved fixnum maps to its just-materialized
            // ClifValue. StackRef/Dup keep the same ClifValue, so the set stays
            // valid as the block runs; `guard_fixnum` elides guards for members.
            let known_fixnum: HashSet<ClifValue> = known_fixnum_slots
                .get(&l)
                .map(|slots| {
                    slots
                        .iter()
                        .enumerate()
                        .filter_map(|(k, &is_fix)| {
                            (is_fix).then(|| stack.get(k).copied()).flatten()
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Active handler frames at block entry (static), kept in sync as
            // PopHandler ops run; signal sites inside a protected extent queue
            // a dispatch block here, filled after the block's terminator.
            let mut handlers: Vec<HandlerStatic> =
                cfg.entry_handlers.get(&l).cloned().unwrap_or_default();
            let mut pending: Vec<PendingDispatch> = Vec::new();
            let mut pending_deopt: Vec<PendingDeopt> = Vec::new();

            let end = next_leader(l);
            let mut terminated = false;
            for (off, op) in ops[l..end].iter().enumerate() {
                let i = l + off;
                // Terminators consume / snapshot / spill the operand stack as tagged
                // Values; force-tag any raw slots first (the block's raw state is
                // discarded after the terminator, so no per-pop lockstep is needed
                // past this point).
                if matches!(
                    op,
                    Op::Return
                        | Op::Throw
                        | Op::Goto(_)
                        | Op::GotoIfNil(_)
                        | Op::GotoIfNotNil(_)
                        | Op::GotoIfNilElsePop(_)
                        | Op::GotoIfNotNilElsePop(_)
                        | Op::Switch
                        | Op::PushConditionCase(_)
                        | Op::PushConditionCaseRaw(_)
                        | Op::PushCatch(_)
                ) {
                    retag_all_raw(&mut fb, &mut stack, &mut stack_raw);
                }
                match op {
                    Op::Return => {
                        let result = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let out = fb.use_var(out_var);
                        fb.ins().store(MemFlagsData::trusted(), result, out, 0);
                        let one = fb.ins().iconst(types::I64, 1);
                        fb.ins().return_(&[one]);
                        terminated = true;
                        break;
                    }
                    Op::Throw => {
                        // Stash Flow::Throw{tag, value} and exit via the signal
                        // path; inside a protected extent that path is the
                        // handler dispatch (a same-function `catch` is caught
                        // natively via the match shim).
                        let value = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let tag = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let rt = rt.as_ref().ok_or(CompileError::UnsupportedOp("throw"))?;
                        fb.ins().call(rt.refs.throw_flow, &[tag, value]);
                        let se = signal_target_for_site(
                            &mut fb,
                            &mut signal_exit,
                            &handlers,
                            &mut pending,
                            &stack,
                        );
                        fb.ins().jump(se, &[]);
                        terminated = true;
                        break;
                    }
                    Op::Goto(t) => {
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let tu = *t as usize;
                        if tu <= i {
                            // Backward jump: bump the quit counter and poll on
                            // wrap, exactly like the interpreter's branch_to!.
                            let (rt, slot) = (
                                rt.as_ref().expect("backedge implies rt"),
                                backedge_counter.expect("backedge implies counter"),
                            );
                            emit_backedge_jump(
                                &mut fb,
                                rt,
                                slot,
                                &mut signal_exit,
                                &vars,
                                cfg.entry_depth[&tu],
                                block_for[&tu],
                                &handlers,
                                &mut pending,
                            );
                        } else {
                            fb.ins().jump(block_for[&tu], &[]);
                        }
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                        let cond = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let is_nil =
                            fb.ins()
                                .icmp_imm_u(IntCC::Equal, cond, Value::NIL.bits() as i64);
                        let tu = *t as usize;
                        let mut target = block_for[&tu];
                        let fallthrough = block_for[&(i + 1)];
                        let backedge = (tu <= i).then(|| fb.create_block());
                        if let Some(tramp) = backedge {
                            target = tramp;
                        }
                        // brif takes the `then` block when the condition is true.
                        if matches!(op, Op::GotoIfNil(_)) {
                            fb.ins().brif(is_nil, target, &[], fallthrough, &[]);
                        } else {
                            fb.ins().brif(is_nil, fallthrough, &[], target, &[]);
                        }
                        if let Some(tramp) = backedge {
                            // Taken-edge trampoline carrying the back-edge poll.
                            fb.switch_to_block(tramp);
                            fb.seal_block(tramp);
                            let (rt, slot) = (
                                rt.as_ref().expect("backedge implies rt"),
                                backedge_counter.expect("backedge implies counter"),
                            );
                            emit_backedge_jump(
                                &mut fb,
                                rt,
                                slot,
                                &mut signal_exit,
                                &vars,
                                cfg.entry_depth[&tu],
                                block_for[&tu],
                                &handlers,
                                &mut pending,
                            );
                        }
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                        // Peek the condition without popping; write the FULL stack
                        // (cond on top) to vars. The jump-taken successor reads it
                        // all (depth D); the fall-through (depth D-1) ignores the
                        // top slot — implementing the "ElsePop".
                        let cond = *stack.last().ok_or(CompileError::StackUnderflow)?;
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let is_nil =
                            fb.ins()
                                .icmp_imm_u(IntCC::Equal, cond, Value::NIL.bits() as i64);
                        let tu = *t as usize;
                        let mut target = block_for[&tu];
                        let fallthrough = block_for[&(i + 1)];
                        let backedge = (tu <= i).then(|| fb.create_block());
                        if let Some(tramp) = backedge {
                            target = tramp;
                        }
                        if matches!(op, Op::GotoIfNilElsePop(_)) {
                            fb.ins().brif(is_nil, target, &[], fallthrough, &[]);
                        } else {
                            fb.ins().brif(is_nil, fallthrough, &[], target, &[]);
                        }
                        if let Some(tramp) = backedge {
                            fb.switch_to_block(tramp);
                            fb.seal_block(tramp);
                            let (rt, slot) = (
                                rt.as_ref().expect("backedge implies rt"),
                                backedge_counter.expect("backedge implies counter"),
                            );
                            emit_backedge_jump(
                                &mut fb,
                                rt,
                                slot,
                                &mut signal_exit,
                                &vars,
                                cfg.entry_depth[&tu],
                                block_for[&tu],
                                &handlers,
                                &mut pending,
                            );
                        }
                        terminated = true;
                        break;
                    }
                    Op::Switch => {
                        // [dispatch table] -> shim lookup (the interpreter's
                        // exact hash-key semantics) returning the raw fixnum
                        // address; map it onto the statically resolved targets
                        // with a compare chain. Miss -> fall through. A raw
                        // address outside the static set or a mutated table ->
                        // loud signal (out-of-contract self-modification).
                        let rt_ref = rt.as_ref().ok_or(CompileError::UnsupportedOp("switch"))?;
                        let table = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let dispatch = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let vmctx = fb.use_var(rt_ref.vmctx_var);
                        let call = fb
                            .ins()
                            .call(rt_ref.refs.switch_lookup, &[vmctx, dispatch, table]);
                        let addr = fb.inst_results(call)[0];
                        let targets = cfg.switch_targets.get(&i).expect("resolved in analyze");
                        let sig = signal_target_for_site(
                            &mut fb,
                            &mut signal_exit,
                            &handlers,
                            &mut pending,
                            &stack,
                        );
                        let fall = block_for[&(i + 1)];
                        // miss -> fall through
                        let miss = fb.ins().icmp_imm_u(IntCC::Equal, addr, JIT_SWITCH_MISS);
                        let chain = fb.create_block();
                        fb.ins().brif(miss, fall, &[], chain, &[]);
                        fb.switch_to_block(chain);
                        fb.seal_block(chain);
                        // stale (-2): the shim stashed the flow already.
                        let stale = fb.ins().icmp_imm_u(IntCC::Equal, addr, JIT_SWITCH_STALE);
                        let mut cur_blk = fb.create_block();
                        fb.ins().brif(stale, sig, &[], cur_blk, &[]);
                        for &(raw, target) in targets {
                            fb.switch_to_block(cur_blk);
                            fb.seal_block(cur_blk);
                            let next = fb.create_block();
                            let hit = fb.ins().icmp_imm_u(IntCC::Equal, addr, raw);
                            if target <= i {
                                // Backward jump-table edge: poll through a
                                // trampoline, exactly like Goto back-edges.
                                let tramp = fb.create_block();
                                fb.ins().brif(hit, tramp, &[], next, &[]);
                                fb.switch_to_block(tramp);
                                fb.seal_block(tramp);
                                let (rt_b, slot) = (
                                    rt.as_ref().expect("backedge implies rt"),
                                    backedge_counter.expect("backedge implies counter"),
                                );
                                emit_backedge_jump(
                                    &mut fb,
                                    rt_b,
                                    slot,
                                    &mut signal_exit,
                                    &vars,
                                    cfg.entry_depth[&target],
                                    block_for[&target],
                                    &handlers,
                                    &mut pending,
                                );
                            } else {
                                fb.ins().brif(hit, block_for[&target], &[], next, &[]);
                            }
                            cur_blk = next;
                        }
                        // Exhausted: a hit whose address is not in the static
                        // set — stash the stale-table signal and propagate.
                        fb.switch_to_block(cur_blk);
                        fb.seal_block(cur_blk);
                        fb.ins().call(rt_ref.refs.switch_stale, &[]);
                        fb.ins().jump(sig, &[]);
                        terminated = true;
                        break;
                    }
                    Op::PushConditionCase(t) | Op::PushConditionCaseRaw(t) | Op::PushCatch(t) => {
                        // Register the handler frame via the shim (interpreter
                        // arm parity), then end the block with an "anchor"
                        // edge: a never-taken branch to the handler target
                        // that (a) guarantees the target block always has a
                        // Cranelift predecessor with every entry var defined
                        // (its real entries are the runtime match dispatches)
                        // and (b) falls through to the protected body.
                        let rt_ref = rt.as_ref().ok_or(CompileError::UnsupportedOp("handler"))?;
                        let tu = *t as usize;
                        let vmctx = fb.use_var(rt_ref.vmctx_var);
                        let t_v = fb.ins().iconst(types::I64, tu as i64);
                        match op {
                            Op::PushConditionCase(_) => {
                                let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                fb.ins().call(rt_ref.refs.push_cc, &[vmctx, t_v, d_v]);
                            }
                            Op::PushConditionCaseRaw(_) => {
                                let conditions = stack.pop().ok_or(CompileError::StackUnderflow)?;
                                let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                fb.ins()
                                    .call(rt_ref.refs.push_cc_raw, &[vmctx, t_v, d_v, conditions]);
                            }
                            Op::PushCatch(_) => {
                                let tag = stack.pop().ok_or(CompileError::StackUnderflow)?;
                                let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                fb.ins()
                                    .call(rt_ref.refs.push_catch, &[vmctx, t_v, d_v, tag]);
                            }
                            _ => unreachable!("matched Push* above"),
                        }
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        // Placeholder error-value slot for the never-taken
                        // anchor edge (real entries define it from the shim).
                        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
                        fb.def_var(vars[stack.len()], nil);
                        let never = fb.ins().iconst(types::I8, 0);
                        fb.ins()
                            .brif(never, block_for[&tu], &[], block_for[&(i + 1)], &[]);
                        terminated = true;
                        break;
                    }
                    Op::PopHandler => {
                        // Normal exit from the protected extent: drop the
                        // runtime frame and the static tracking entry.
                        let rt_ref = rt.as_ref().ok_or(CompileError::UnsupportedOp("handler"))?;
                        let vmctx = fb.use_var(rt_ref.vmctx_var);
                        fb.ins().call(rt_ref.refs.pop_handler, &[vmctx]);
                        handlers
                            .pop()
                            .ok_or(CompileError::UnsupportedOp("unbalanced-pophandler"))?;
                    }
                    other => {
                        let spec = spec_sites.get(&i).map(|site| {
                            (
                                site.sym,
                                site.expected_bits,
                                &spec_slots[site.slot] as *const SpecSlot as i64,
                                // R2 increment B2: the slot index the AOT sidecar's
                                // spec-slot / spec-expected arrays are keyed by.
                                site.slot,
                                site.kind,
                            )
                        });
                        lower_simple_op(
                            &mut fb,
                            i,
                            &mut pending_deopt,
                            &mut signal_exit,
                            constants,
                            &mut stack,
                            &mut stack_raw,
                            rt.as_ref(),
                            &handlers,
                            &mut pending,
                            spec,
                            other,
                            &known_fixnum,
                            reloc_base,
                            reloc_index,
                            aot,
                            spec_slot_base,
                            spec_expected_base,
                            dynamic_prefix,
                            consts_base,
                        )?;
                        // Re-sync the raw mask after the op: raw-preserving ops keep
                        // it in lockstep (assert); every other op force-tagged the
                        // stack at the top, so reset the mask to all-tagged here.
                        if op_preserves_raw(other) {
                            debug_assert_eq!(
                                stack.len(),
                                stack_raw.len(),
                                "raw-preserving op left stack_raw desynced"
                            );
                        } else {
                            stack_raw.resize(stack.len(), false);
                        }
                    }
                }
            }
            if !terminated {
                // Fall through into the next leader block (analyze guaranteed it
                // exists and is < n). vars carry tagged Values across the edge.
                retag_all_raw(&mut fb, &mut stack, &mut stack_raw);
                write_stack_to_vars(&mut fb, &vars, &stack);
                fb.ins().jump(block_for[&end], &[]);
            }
            // Fill the precise-deopt exit blocks queued by this block's guards.
            emit_pending_deopts(&mut fb, deopt_refs, &mut pending_deopt);
            // Fill the handler-dispatch blocks queued by this block's signal
            // sites (the builder can switch blocks now that it's terminated).
            if !pending.is_empty() {
                let rt_ref = rt.as_ref().expect("pending dispatches imply rt");
                emit_pending_dispatches(
                    &mut fb,
                    rt_ref,
                    &mut signal_exit,
                    &vars,
                    &block_for,
                    &mut pending,
                )?;
            }
        }

        // Terminate the shared signal block (return STATUS_SIGNAL) iff used.
        if let Some(sb) = signal_exit {
            fb.switch_to_block(sb);
            let code = fb.ins().iconst(types::I64, STATUS_SIGNAL);
            fb.ins().return_(&[code]);
        }

        fb.seal_all_blocks();
        fb.finalize(frontend_config);
    }
    LAST_IR_STATS.with(|c| {
        let (_, _, sites, slots) = c.get();
        c.set((
            func.dfg.num_insts() as u32,
            func.layout.blocks().count() as u32,
            sites,
            slots,
        ));
    });

    let fid = module
        .declare_function(entry_name, entry_linkage, &sig)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    let mut ctx = module.make_context();
    ctx.func = func;
    module
        .define_function(fid, &mut ctx)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    module.clear_context(&mut ctx);

    Ok(fid)
}

mod leaf;
pub use leaf::*;

mod lowering;
pub use lowering::*;

mod shims;
pub use shims::*;
#[cfg(test)]
#[path = "compile_tests.rs"]
mod tests;
