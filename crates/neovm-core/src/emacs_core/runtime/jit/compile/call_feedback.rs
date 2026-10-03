//! Call-target feedback in compiled code (design `p2-1-feedback-reopt`
//! §3.3, P2.1 C3/C4): the recording call shims a JIT leaf calls at its
//! non-constant call sites while `NEOVM_JIT_FEEDBACK` records, or while a
//! Use-mode T1 leaf profiles under `NEOVM_JIT_TIER2`, and the compile-time
//! view of the source's call-site table.
//!
//! A site records when the source's table has one for its pc
//! (`jit::feedback::CallSites`: a call whose callee the code does not name
//! as a constant, an `apply`, a mapping builtin). Its generic call then goes
//! through [`neovm_jit_call_prof`] / [`neovm_jit_apply_prof`], which count
//! the execution, join the target into the site's lattice and make the call
//! the plain shim would have made; a speculated mapping-builtin call records
//! its callback through [`neovm_jit_record_call_target`] first. The shims
//! here retain Record/Census's JIT-only baked-address calls. Use-mode T1
//! shims in `t2_profile` use optional lazy-table imports and receive the
//! owning leaf's observation pointer to stop at its window boundary. Every
//! site pointer points into the compiling source's table, which the leaf
//! keeps alive ([`FeedbackHolds`]). None is emitted by AOT.
//!
//! With the knob off nothing here is reached and the lowering is unchanged.

use super::lowering::RtCtx;
use super::*;
use crate::emacs_core::jit::RuntimeState;
use crate::emacs_core::jit::feedback::arrays::ArraySiteFeedback;
use crate::emacs_core::jit::feedback::{CallSiteFeedback, CallTarget, SiteShape};
use cranelift_codegen::isa::CallConv;
use std::sync::Arc;

thread_local! {
    /// The source being compiled, when its call sites record (published by
    /// the compile's feedback snapshot, `compile::snapshot`).
    static ACTIVE_CALL_SOURCE: std::cell::RefCell<Option<Arc<RuntimeState>>> =
        const { std::cell::RefCell::new(None) };

    /// Whether the compile in progress may read the recorded targets
    /// (`NEOVM_JIT_FEEDBACK=use`), published with the source.
    static ACTIVE_CALL_TARGETS_READ: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// The `RuntimeState`s the leaf being lowered bakes an address inside
    /// (a site pointer into its own source's table, a speculated target's
    /// state): moved into the leaf, which keeps them alive while its code
    /// can run.
    static ACTIVE_FEEDBACK_HOLDS: std::cell::RefCell<Vec<Arc<RuntimeState>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Restores the call-feedback publication the compile inside it replaced.
pub(crate) struct CallSourceScope {
    prev: Option<Arc<RuntimeState>>,
    prev_read: bool,
}

impl CallSourceScope {
    /// Publish `source` (whose table exists) as the compile's call-feedback
    /// source; `read` lets the compile read the recorded targets.
    pub(crate) fn enter(source: Option<Arc<RuntimeState>>, read: bool) -> Self {
        CallSourceScope {
            prev: ACTIVE_CALL_SOURCE.with(|s| std::mem::replace(&mut *s.borrow_mut(), source)),
            prev_read: ACTIVE_CALL_TARGETS_READ.with(|r| r.replace(read)),
        }
    }
}

impl Drop for CallSourceScope {
    fn drop(&mut self) {
        let prev = self.prev.take();
        ACTIVE_CALL_SOURCE.with(|s| *s.borrow_mut() = prev);
        ACTIVE_CALL_TARGETS_READ.with(|r| r.set(self.prev_read));
    }
}

/// The holds of one leaf build: a build nested inside a lowering keeps its
/// own, and the outer build's are restored when it ends.
pub(crate) struct FeedbackHolds(Option<Vec<Arc<RuntimeState>>>);

impl FeedbackHolds {
    /// Start collecting the holds of a leaf build.
    pub(crate) fn enter() -> Self {
        FeedbackHolds(Some(
            ACTIVE_FEEDBACK_HOLDS.with(|h| std::mem::take(&mut *h.borrow_mut())),
        ))
    }

    /// The holds this build collected (the outer build's restored).
    pub(crate) fn finish(mut self) -> Box<[Arc<RuntimeState>]> {
        let outer = self.0.take().unwrap_or_default();
        ACTIVE_FEEDBACK_HOLDS
            .with(|h| std::mem::replace(&mut *h.borrow_mut(), outer))
            .into_boxed_slice()
    }
}

impl Drop for FeedbackHolds {
    fn drop(&mut self) {
        // A build abandoned on an error: its holds go, the outer's return.
        if let Some(outer) = self.0.take() {
            ACTIVE_FEEDBACK_HOLDS.with(|h| *h.borrow_mut() = outer);
        }
    }
}

/// Keep `state` alive as long as the leaf being built (once per state).
pub(crate) fn hold_for_leaf(state: &Arc<RuntimeState>) {
    ACTIVE_FEEDBACK_HOLDS.with(|h| {
        let mut h = h.borrow_mut();
        if !h.iter().any(|held| Arc::ptr_eq(held, state)) {
            h.push(Arc::clone(state));
        }
    });
}

/// The ORIGINAL-body pc a lowered pc stands for, or `None` inside a region
/// the fuser spliced in (its ops are the callee's).
fn original_pc(pc: usize) -> Option<usize> {
    match inline::active_fused() {
        Some(fused) if fused.region_at(pc).is_some() => None,
        Some(fused) => fused.caller_pc(pc),
        None => Some(pc),
    }
}

/// Source-held scalar array site for the original caller pc. Compiler scope
/// only: callee regions are rejected before mapping, and the source Arc is
/// retained by this leaf before an immutable-address site can be emitted.
pub(crate) fn array_recording_site_at(pc: usize) -> Option<*const ArraySiteFeedback> {
    let pc = original_pc(pc)?;
    ACTIVE_CALL_SOURCE.with(|source| {
        let source = source.borrow();
        let source = source.as_ref()?;
        let array_sites = source.array_sites()?;
        let site = array_sites.site_at(pc)? as *const ArraySiteFeedback;
        hold_for_leaf(source);
        Some(site)
    })
}

/// The recording site of the call at lowered pc `pc`, when compiled sites
/// record (`record`, `census`), or `use` in a profiling T1, and the
/// compiling source has one there. OSR, T2 and knob-off `use` do not record.
/// Keeps the source alive for the leaf.
pub(crate) fn recording_site_at(pc: usize, rt: &RtCtx) -> Option<*const CallSiteFeedback> {
    let mode = crate::emacs_core::jit::feedback::feedback_mode();
    if !mode.records_compiled() && !(mode.uses() && rt.poll.t2.is_some()) {
        return None;
    }
    let pc = original_pc(pc)?;
    ACTIVE_CALL_SOURCE.with(|s| {
        let source = s.borrow();
        let source = source.as_ref()?;
        let site = source.call_sites()?.site_at(pc)? as *const CallSiteFeedback;
        hold_for_leaf(source);
        Some(site)
    })
}

/// The target recorded at the call at lowered pc `pc`, when the compile
/// may read targets (`NEOVM_JIT_FEEDBACK=use`).
pub(crate) fn recorded_target_at(pc: usize) -> Option<(CallTarget, SiteShape)> {
    if !ACTIVE_CALL_TARGETS_READ.with(std::cell::Cell::get) {
        return None;
    }
    let pc = original_pc(pc)?;
    ACTIVE_CALL_SOURCE.with(|s| {
        let source = s.borrow();
        let site = source.as_ref()?.call_sites()?.site_at(pc)?;
        Some((site.target(), site.shape()))
    })
}

// ---------------------------------------------------------------------------
// The recording shims (JIT-only, called by baked address).
// ---------------------------------------------------------------------------

/// [`neovm_jit_call`] at a recording site: count the execution, join its
/// target into the site's lattice, then make the call. Same contract as
/// `neovm_jit_call`, plus `site`, a pointer into the calling leaf's source's
/// call-site table (kept alive by the leaf). `WINDOWED` (every mode but
/// `census`): only inside the site's profiling window; after it the shim
/// is a count test and a tail call.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
pub(crate) extern "C" fn neovm_jit_call_prof<const WINDOWED: bool>(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    site: *const CallSiteFeedback,
) -> i64 {
    // Both arms are tail calls, so the path after the window needs no
    // frame: a load, a compare and a jump.
    // SAFETY: the calling leaf keeps its source's table alive.
    if !WINDOWED || unsafe { &*site }.window_open() {
        return neovm_jit_call_recording(ctx, func_bits, args_ptr, nargs, out, site);
    }
    neovm_jit_call(ctx, func_bits, args_ptr, nargs, out)
}

/// [`neovm_jit_call_prof`]'s recording arm.
#[inline(never)]
extern "C" fn neovm_jit_call_recording(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    site: *const CallSiteFeedback,
) -> i64 {
    record_at(site, func_bits, args_ptr, nargs);
    neovm_jit_call(ctx, func_bits, args_ptr, nargs, out)
}

/// [`neovm_jit_apply`] at a recording site (see [`neovm_jit_call_prof`]).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
pub(crate) extern "C" fn neovm_jit_apply_prof<const WINDOWED: bool>(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    site: *const CallSiteFeedback,
) -> i64 {
    // SAFETY: as above.
    if !WINDOWED || unsafe { &*site }.window_open() {
        return neovm_jit_apply_recording(ctx, func_bits, args_ptr, nargs, out, site);
    }
    neovm_jit_apply(ctx, func_bits, args_ptr, nargs, out)
}

/// [`neovm_jit_apply_prof`]'s recording arm.
#[inline(never)]
extern "C" fn neovm_jit_apply_recording(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
    site: *const CallSiteFeedback,
) -> i64 {
    record_at(site, func_bits, args_ptr, nargs);
    neovm_jit_apply(ctx, func_bits, args_ptr, nargs, out)
}

/// Record `target` at `site` (a speculated mapping-builtin call, whose
/// callback is its argument 0), counting the execution (see
/// [`neovm_jit_call_prof`] for `WINDOWED`).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
pub(crate) extern "C" fn neovm_jit_record_call_target<const WINDOWED: bool>(
    site: *const CallSiteFeedback,
    target: i64,
) {
    // SAFETY: the calling leaf keeps its source's table alive.
    let site = unsafe { &*site };
    if !WINDOWED || site.window_open() {
        site.observe_counted(Value::from_bits(target as usize));
    }
}

/// The recording half of the `_prof` shims. Touches only the site's
/// atomics and, on a transition, Rust-heap `Weak`s: no Lisp allocation, no
/// safepoint, no unwind.
#[inline(always)]
pub(super) fn record_at(
    site: *const CallSiteFeedback,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
) {
    // SAFETY: the calling leaf keeps its source's table alive.
    let site = unsafe { &*site };
    let arg0 = if nargs > 0 {
        // SAFETY: the generated code stored `nargs` words at `args_ptr`.
        Value::from_bits(unsafe { *args_ptr } as usize)
    } else {
        Value::NIL
    };
    site.observe_counted(site.target_of(Value::from_bits(func_bits as usize), arg0));
}

/// The CLIF signature of the two `_prof` call shims: `neovm_jit_call`'s
/// plus the site pointer.
fn prof_call_signature(call_conv: CallConv, ptr_ty: types::Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    for ty in [ptr_ty, types::I64, ptr_ty, types::I64, ptr_ty, ptr_ty] {
        sig.params.push(AbiParam::new(ty));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// Emit the recording call of a generic `Op::Call`/`Op::Apply` site (its
/// shim's arguments, then `site`); returns the call instruction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_prof_call(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    apply: bool,
    site: *const CallSiteFeedback,
    vmctx: ClifValue,
    func_val: ClifValue,
    args_addr: ClifValue,
    n_val: ClifValue,
    out_addr: ClifValue,
) -> cranelift_codegen::ir::Inst {
    let mode = crate::emacs_core::jit::feedback::feedback_mode();
    let windowed = mode.windowed();
    if rt.poll.t2.is_some() && (!apply || mode.uses()) {
        let site_v = fb.ins().iconst(rt.ptr_ty, site as usize as i64);
        if let Some(call) = super::t2_profile::emit_feedback_prof_call(
            fb,
            rt,
            apply,
            windowed,
            &[vmctx, func_val, args_addr, n_val, out_addr, site_v],
        ) {
            return call;
        }
    }
    let sig = fb.import_signature(prof_call_signature(rt.refs.call_conv, rt.ptr_ty));
    let shim = match (apply, windowed) {
        (true, true) => neovm_jit_apply_prof::<true> as *const () as usize,
        (true, false) => neovm_jit_apply_prof::<false> as *const () as usize,
        (false, true) => neovm_jit_call_prof::<true> as *const () as usize,
        (false, false) => neovm_jit_call_prof::<false> as *const () as usize,
    };
    let callee = fb.ins().iconst(rt.ptr_ty, shim as i64);
    let site_v = fb.ins().iconst(rt.ptr_ty, site as usize as i64);
    fb.ins().call_indirect(
        sig,
        callee,
        &[vmctx, func_val, args_addr, n_val, out_addr, site_v],
    )
}

/// Emit the record of a speculated mapping-builtin call's callback `target`
/// at `site`.
pub(crate) fn emit_record_call_target(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    site: *const CallSiteFeedback,
    target: ClifValue,
) {
    if crate::emacs_core::jit::feedback::feedback_mode().uses() {
        let t2 = rt.poll.t2.expect("Use recording requires a profiling T1");
        let shim = rt
            .refs
            .try_get(fb.func, Shim::T2RecordCallUseTarget)
            .expect("profiling refs");
        let site_v = fb.ins().iconst(rt.ptr_ty, site as usize as i64);
        let obs_v = fb.ins().iconst(rt.ptr_ty, t2.obs as i64);
        fb.ins().call(shim, &[site_v, target, obs_v]);
        return;
    }
    let mut sig = Signature::new(rt.refs.call_conv);
    sig.params.push(AbiParam::new(rt.ptr_ty));
    sig.params.push(AbiParam::new(types::I64));
    let sig = fb.import_signature(sig);
    let shim = if crate::emacs_core::jit::feedback::feedback_mode().windowed() {
        neovm_jit_record_call_target::<true> as *const () as usize
    } else {
        neovm_jit_record_call_target::<false> as *const () as usize
    };
    let callee = fb.ins().iconst(rt.ptr_ty, shim as i64);
    let site_v = fb.ins().iconst(rt.ptr_ty, site as usize as i64);
    fb.ins().call_indirect(sig, callee, &[site_v, target]);
}
