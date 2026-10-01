//! The tier spine's generated code (design `p2-0-integration` §3.1, P2.1
//! C6 + W1; [`crate::emacs_core::jit::tier2`] owns the policy): a profiling
//! leaf's prologue countdown, the back-edge poll block's tick counter and
//! loop credit, and the mapping-builtin credit.
//!
//! Every emission here is JIT-only (baked addresses; AOT passes
//! [`super::LeafEmit::NONE`]) and absent unless a report knob (the tick
//! counter, with the entry counter) or `NEOVM_JIT_TIER2` (the rest) asked
//! for it at compile time. With both off the CLIF is unchanged.
//!
//! The countdown and request touch no Lisp heap objects and cannot collect.
//! HOF credit reads a bounded prefix of its sequence, without allocation or
//! Lisp calls. The profiling shims then run the existing call protocol.

use super::lowering::RtCtx;
use super::*;
use crate::emacs_core::jit::feedback::CallSiteFeedback;
use crate::emacs_core::jit::tier2::{T2Emit, neovm_jit_t2_hof_credit};
use cranelift_codegen::isa::CallConv;

/// What a back-edge poll block writes besides the poll ([`RtCtx::poll`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PollEmit {
    /// The leaf's poll tick counter, when entry counting is on.
    pub(crate) count: Option<usize>,
    /// The leaf's countdown, when it profiles.
    pub(crate) t2: Option<T2Emit>,
}

/// `*addr += 1` through a baked address (a load, an add and a store).
fn emit_bump(fb: &mut FunctionBuilder, ptr_ty: Type, addr: usize) {
    let a = fb.ins().iconst(ptr_ty, addr as i64);
    let n = fb.ins().load(types::I64, MemFlagsData::trusted(), a, 0);
    let n1 = fb.ins().iadd_imm_s(n, 1);
    fb.ins().store(MemFlagsData::trusted(), n1, a, 0);
}

/// Lazily imported request refs for a profiling leaf with no other runtime
/// calls. Compiler scratch only; refs belong to one function and are never
/// shared by mutators. With profiling off, callers never ask for these.
pub(crate) fn pure_entry_refs<S: sink::LeafSink>(
    sink: &mut S,
    func: &mut Function,
    call_conv: CallConv,
    ptr_ty: Type,
) -> Result<RtRefs, CompileError> {
    let groups = ShimGroups {
        subr_spec: false,
        cbsym_spec: false,
        tier2_profile: true,
    };
    Ok(RtRefs::new(
        sink.shim_ids(call_conv, ptr_ty, groups)?,
        groups,
        func,
        call_conv,
        ptr_ty,
    ))
}

/// The request call through the same lazy shim table as other runtime calls.
fn emit_request_call(fb: &mut FunctionBuilder, ptr_ty: Type, refs: &RtRefs, obs: usize) {
    let request = refs
        .try_get(fb.func, Shim::TierRequest)
        .expect("profiling refs");
    let obs_v = fb.ins().iconst(ptr_ty, obs as i64);
    fb.ins().call(request, &[obs_v]);
}

/// `budget -= credit`; at or below zero a cold block makes the request.
/// Both paths continue in a fresh block, current on return; values of the
/// block this started in stay usable there (it dominates the
/// continuation).
fn emit_take(
    fb: &mut FunctionBuilder,
    ptr_ty: Type,
    refs: &RtRefs,
    t2: T2Emit,
    credit: u32,
    test: IntCC,
) {
    let addr = fb.ins().iconst(ptr_ty, t2.budget as i64);
    let budget = fb.ins().load(types::I64, MemFlagsData::trusted(), addr, 0);
    let left = fb.ins().iadd_imm_s(budget, -i64::from(credit));
    fb.ins().store(MemFlagsData::trusted(), left, addr, 0);
    let out = fb.ins().icmp_imm_s(test, left, 0);
    let req = fb.create_block();
    let cont = fb.create_block();
    fb.set_cold_block(req);
    fb.ins().brif(out, req, &[], cont, &[]);
    fb.switch_to_block(req);
    fb.seal_block(req);
    emit_request_call(fb, ptr_ty, refs, t2.obs);
    fb.ins().jump(cont, &[]);
    fb.switch_to_block(cont);
    fb.seal_block(cont);
}

/// The prologue countdown of a profiling leaf (one per native entry),
/// emitted in the entry block after the entry counter. The rest of the
/// entry code continues in the block current on return.
pub(crate) fn emit_entry_countdown(
    fb: &mut FunctionBuilder,
    ptr_ty: Type,
    refs: &RtRefs,
    t2: T2Emit,
) {
    emit_take(fb, ptr_ty, refs, t2, 1, IntCC::Equal);
}

/// The poll block's extras, emitted first in it: the tick counter, then
/// the loop credit (W1). The credit only takes from the countdown and, when
/// it ran out, requests; either way the block continues into the poll, so
/// the quit cadence (every 255 taken back edges) is unchanged.
pub(crate) fn emit_poll_extras(fb: &mut FunctionBuilder, rt: &RtCtx) {
    let ptr_ty = rt.ptr_ty;
    if let Some(addr) = rt.poll.count {
        emit_bump(fb, ptr_ty, addr);
    }
    if let Some(t2) = rt.poll.t2
        && t2.loop_credit > 0
    {
        emit_take(
            fb,
            ptr_ty,
            &rt.refs,
            t2,
            t2.loop_credit,
            IntCC::SignedLessThanOrEqual,
        );
    }
}

/// The mapping builtins whose sequence argument the credit measures.
fn is_mapping_builtin(sym: SymId) -> bool {
    use std::sync::OnceLock;
    static MAPPING: OnceLock<[SymId; 4]> = OnceLock::new();
    MAPPING
        .get_or_init(|| ["mapc", "mapcar", "mapcan", "mapconcat"].map(intern))
        .contains(&sym)
}

/// Whether this call runs inside the tier profiling window. Selected only at
/// compile time; the shim's helper stops crediting when that window closes.
fn profiling_call(rt: &RtCtx) -> Option<T2Emit> {
    rt.poll.t2.filter(|t2| t2.loop_credit != 0)
}

/// A speculated HOF call uses its profiling shim, with no separate generated
/// credit call and no changes to the existing subr shim's hot path.
pub(crate) fn subr_profiler(rt: &RtCtx, expected: u64) -> Option<T2Emit> {
    profiling_call(rt).filter(|_| {
        Value::from_bits(expected as usize)
            .as_subr_id()
            .is_some_and(is_mapping_builtin)
    })
}

/// A generic call's T2 profiling shim. Applies only to ordinary calls: an
/// `apply` needs to spread its tail first and keeps its existing protocol.
pub(crate) fn emit_generic_prof_call(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    apply: bool,
    args: &[ClifValue],
) -> Option<cranelift_codegen::ir::Inst> {
    if apply {
        return None;
    }
    let t2 = profiling_call(rt)?;
    let shim = rt
        .refs
        .try_get(fb.func, Shim::T2CallProf)
        .expect("profiling refs");
    let mut args = args.to_vec();
    args.push(fb.ins().iconst(rt.ptr_ty, t2.obs as i64));
    Some(fb.ins().call(shim, &args))
}

/// Combine call-target recording with HOF credit inside one `_prof` shim.
pub(crate) fn emit_feedback_prof_call(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    apply: bool,
    windowed: bool,
    args: &[ClifValue],
) -> Option<cranelift_codegen::ir::Inst> {
    if apply {
        return None;
    }
    let t2 = profiling_call(rt)?;
    let name = if windowed {
        Shim::T2CallFeedbackProf
    } else {
        Shim::T2CallFeedbackCensus
    };
    let shim = rt.refs.try_get(fb.func, name).expect("profiling refs");
    let mut args = args.to_vec();
    args.push(fb.ins().iconst(rt.ptr_ty, t2.obs as i64));
    Some(fb.ins().call(shim, &args))
}

/// Read-only HOF credit. Runtime state is leaf-owned and passed explicitly;
/// no global or thread-local Lisp state is added. The original shim owns the
/// Context and sequence rooting throughout the subsequent call.
#[inline]
fn credit_generic(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    obs: *const LeafObs,
) {
    if nargs < 2 || !credit_window_open(obs) {
        return;
    }
    let callee = Value::from_bits(func_bits as usize);
    // SAFETY: same Context contract as the existing generic call shim. This
    // borrow ends before the call, and the credit cannot collect or run Lisp.
    let ctx = unsafe { &*(ctx as *const Context) };
    // A direct subr value ignores symbol redefinitions. A symbol must still
    // name the native mapping subr; a closure or advice wrapper gets no credit.
    let native = callee.as_subr_id().or_else(|| {
        callee
            .as_symbol_id()
            .and_then(|sym| ctx.obarray.symbol_function_id(sym))
            .and_then(|v| v.as_subr_id())
    });
    if !native.is_some_and(is_mapping_builtin) {
        return;
    }
    // SAFETY: the generated call buffer contains nargs argument words.
    neovm_jit_t2_hof_credit(obs, unsafe { *args_ptr.add(1) });
}

/// The profiling shims receive a live, mutator-owned leaf observation.
/// Closed windows inspect no Lisp values and immediately use the plain shim.
#[inline]
fn credit_window_open(obs: *const LeafObs) -> bool {
    // SAFETY: generated callers hold the containing leaf for the activation.
    let obs = unsafe { &*obs };
    obs.t2.state.get() == crate::emacs_core::jit::tier2::T2State::Idle
        && obs.t2.budget.get() != crate::emacs_core::jit::tier2::DISARMED
}

/// The generic `_prof` call: HOF credit, then the complete existing call
/// protocol. Generated code supplies a live leaf-owned obs and call buffer.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_t2_call_prof(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    obs: *const LeafObs,
) -> i64 {
    credit_generic(ctx, func_bits, args_ptr, nargs, obs);
    neovm_jit_call(ctx, func_bits, args_ptr, nargs, out)
}

/// The subr `_prof` call: credit only an already-armed common path. A declined
/// site receives its credit in the generic fallback instead. Every other
/// state executes the original quit/arming/debugger/error protocol unchanged.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_t2_call_subr_prof(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    obs: *const LeafObs,
) -> i64 {
    if nargs < 2 || !credit_window_open(obs) {
        return neovm_jit_call_subr_spec(ctx, sym, expected, slot, args_ptr, nargs, out);
    }
    // SAFETY: the caller owns live Context, leaf spec slot and call args.
    let common = unsafe {
        let ctx = &*(ctx as *const Context);
        let slot = &*(slot as *const SpecSlot);
        ctx.attention_clear(crate::emacs_core::eval::AttentionMask::SPEC_SUBR)
            && slot.epoch.load(std::sync::atomic::Ordering::Relaxed) == ctx.obarray.function_epoch()
            && !ctx.debug_on_next_call_is_armed()
    };
    if common {
        // SAFETY: the generated call buffer contains nargs argument words.
        neovm_jit_t2_hof_credit(obs, unsafe { *args_ptr.add(1) });
    }
    neovm_jit_call_subr_spec(ctx, sym, expected, slot, args_ptr, nargs, out)
}

/// Target recording and HOF credit share one profiling shim. The site's
/// atomics retain the existing record-mode window; tier credit has its own
/// leaf window, so neither operation extends the other's lifetime.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_t2_call_feedback_prof(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    site: *const CallSiteFeedback,
    obs: *const LeafObs,
) -> i64 {
    credit_generic(ctx, func_bits, args_ptr, nargs, obs);
    super::call_feedback::neovm_jit_call_prof::<true>(ctx, func_bits, args_ptr, nargs, out, site)
}

/// Census variant of [`neovm_jit_t2_call_feedback_prof`]. HOF credit still
/// stops at the tier window even when the call-target census runs forever.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_t2_call_feedback_census(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    site: *const CallSiteFeedback,
    obs: *const LeafObs,
) -> i64 {
    credit_generic(ctx, func_bits, args_ptr, nargs, obs);
    super::call_feedback::neovm_jit_call_prof::<false>(ctx, func_bits, args_ptr, nargs, out, site)
}

#[cfg(test)]
#[path = "t2_profile/tests/emission_test.rs"]
mod tests;
