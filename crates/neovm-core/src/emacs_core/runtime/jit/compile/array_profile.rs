//! Scalar plain-array observation for selected profiling T1 leaves.
//! Compiler-local APIs use the EXISTING source scope and leaf holds, never a
//! new runtime TLS cache. Array sites are atomic and shared; LeafObs remains
//! mutator-owned as in all current tier shims. Generated code passes a live
//! owning leaf obs plus a source-held immutable-address scalar site.

use super::lowering::RtCtx;
use super::*;
use crate::emacs_core::jit::feedback::arrays::{ArraySiteFeedback, ObservedArrayKind};

/// Every policy selector is compile-time. The runtime helper contains no env
/// reads. This is normal T1 only; OSR/T2/AOT have no active T2 profile pointer.
pub(super) fn selected(aot: bool, t2: Option<crate::emacs_core::jit::tier2::T2Emit>) -> bool {
    !aot && t2.is_some() && jit_opt_mode() == OptMode::Opt && jit_opt_passes().range
}

/// Before original Aref (including intrinsic/leaf/inline/shim variants), append
/// a lazy GC-free observer only inside selected T1. It must run on the original
/// tagged array operand BEFORE semantic error handling; it neither implements
/// Aref nor changes index -> array -> bounds error order.
pub(super) fn emit(fb: &mut FunctionBuilder, rt: &RtCtx, aot: bool, pc: usize, array: ClifValue) {
    if !selected(aot, rt.poll.t2) {
        return;
    }
    let Some(site) = super::call_feedback::array_recording_site_at(pc) else {
        return;
    };
    let t2 = rt.poll.t2.expect("selected T1 profiling context");
    let helper = rt
        .refs
        .try_get(fb.func, Shim::T2RecordArrayUse)
        .expect("selected array profiling refs");
    let site = fb.ins().iconst(rt.ptr_ty, site as usize as i64);
    let obs = fb.ins().iconst(rt.ptr_ty, t2.obs as i64);
    fb.ins().call(helper, &[site, array, obs]);
}

/// Keep every physical-kind inspection inside the open-window continuation.
/// Compiler tests can prove that a closed window never evaluates it, without
/// passing invalid Lisp words or site pointers into a runtime helper.
#[inline]
pub(super) fn when_open(open: bool, record: impl FnOnce()) {
    if open {
        record();
    }
}

/// Separate lazy shim, appended after existing signatures. Window closed means
/// no Value::from_bits/kind/header access at all. Open observation touches only
/// immutable physical tag/header and relaxed scalar atomics: no roots, Lisp,
/// allocation, GC, quit counter, Rust lazy initialization, or error status.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_t2_record_array_use(
    site: *const ArraySiteFeedback,
    array: i64,
    obs: *const LeafObs,
) {
    when_open(super::t2_profile::credit_window_open(obs), || {
        // SAFETY: the native activation owns obs; FeedbackHolds retains source
        // and its once-published site table; array is a live valid tagged Lisp
        // operand. Nothing here can reach a safepoint before its header read.
        unsafe { &*site }.observe(ObservedArrayKind::of(Value::from_bits(array as usize)));
    });
}
