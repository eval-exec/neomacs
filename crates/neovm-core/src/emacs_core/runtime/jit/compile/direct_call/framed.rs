//! Contained framed direct calls (P1.1 Stage 2c).
//!
//! The generated site owns only the original backtrace push and depth
//! increment. This trampoline keeps `NativeRun`'s boxed precise-deopt
//! payload intact until the reference framed finish. A pending panic keeps
//! the reference path's depth/frame residue for caller healing, detaching
//! its reads of the caller's argument slot before that storage can die.
//! Threading: it borrows the current mutator's dormant Context and live
//! entered leaf; it adds no shared Lisp state. The diagnostic counter is
//! process-wide and uses relaxed atomics, as other engagement counters do.

use super::super::dispatch::{FastRun, call_spec_finish, call_spec_framed_run};
use super::super::{CompiledLeaf, NativeRun, STATUS_OK, STATUS_SIGNAL, shim_panic_pending};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[cfg(any(test, debug_assertions))]
pub(crate) static DIRECT_FRAMED_CALLS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Enter an exact-arity framed JIT leaf with the call's backtrace entry
/// already pushed and its depth already counted. The result slot is
/// written on `STATUS_OK`; all other statuses leave it unchanged.
///
/// The entered leaf is captured before running Lisp: recursive re-arming
/// or redefinition must not change the precise-deopt subject. An object
/// call's original frame roots its callee directly. A named call records
/// the symbol; `cache::pin_redefined_function` retains the old object on a
/// function-cell write, and GC's active-backtrace tracing roots that pin
/// while this symbol frame remains live. No generated depth decrement or pop follows this
/// trampoline. A contained panic leaves the reference path's counted
/// depth and frame residue for the caller's healing boundary.
///
/// SAFETY: the shim vmctx contract; `leaf_bits` names the live entered
/// leaf, `args_ptr` addresses `nargs` original call words for this extent,
/// `bt_count` is its frame's specpdl index, and `const_base` belongs to the
/// executing callee object. The slot's framed tag proves the memory ABI,
/// exact required-only argument shape, and absence of an AOT sidecar.
#[allow(clippy::too_many_arguments, clippy::not_unsafe_ptr_arg_deref)]
#[inline(never)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_direct_framed(
    ctx: *mut u8,
    callee_bits: i64,
    leaf_bits: i64,
    const_base: i64,
    args_ptr: *const i64,
    nargs: i64,
    bt_count: i64,
    out: *mut i64,
) -> i64 {
    #[cfg(any(test, debug_assertions))]
    DIRECT_FRAMED_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    {
        // SAFETY: the generated site's captured leaf and dormant Context.
        let leaf = unsafe { &*(leaf_bits as usize as *const CompiledLeaf) };
        let nargs = nargs as usize;
        let bt_count = bt_count as usize;
        debug_assert!(super::super::spec_slot::framed_direct_eligible(leaf, nargs));
        let outcome = call_spec_framed_run(ctx, leaf, const_base as u64, args_ptr);
        // No mutable Context reference spans the native/Lisp execution
        // above. Its dormant seam borrow is reconstructed only for cleanup.
        let ctx_ref = unsafe { &mut *(ctx as *mut Context) };
        // Never fold the marker through finish_framed_run: the caller's
        // healing boundary must see it first. As the spec shim's framed
        // branch does, keep its depth/frame residue and detach all reads of
        // the dying argument slot before returning to that boundary.
        if shim_panic_pending() {
            // SAFETY: the caller's argument slot is still live here.
            unsafe { ctx_ref.detach_native_frames_into(args_ptr) };
            return STATUS_SIGNAL;
        }
        let run = match outcome {
            NativeRun::Ok(bits) => {
                if ctx_ref.pop_native_backtrace_frame(bt_count) {
                    ctx_ref.depth -= 1;
                    // SAFETY: the generated code's result stack slot.
                    unsafe { *out = bits as i64 };
                    return STATUS_OK;
                }
                // A promoted/debug-on-exit frame needs the reference pop,
                // with GNU's decrement-before-exit-debugger ordering.
                FastRun::Done(Value::from_bits(bits))
            }
            other => FastRun::Framed(other),
        };
        // This owns depth/frame cleanup, signal propagation and a boxed
        // DeoptAt resume. In particular, a framed run is never FastRun::Raw.
        call_spec_finish(
            ctx,
            Value::from_bits(callee_bits as usize),
            leaf,
            args_ptr,
            nargs,
            out,
            bt_count,
            run,
        )
    }
}
