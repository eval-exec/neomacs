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
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

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
/// # Safety
/// `out` points to a writable `i64` slot owned by the calling frame.
/// the shim vmctx contract; `leaf_bits` names the live entered
/// leaf, `args_ptr` addresses `nargs` original call words for this extent,
/// `bt_count` is its frame's specpdl index, and `const_base` belongs to the
/// executing callee object. The slot's framed tag proves the memory ABI,
/// exact required-only argument shape, and absence of an AOT sidecar.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn neovm_jit_direct_framed(
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
        let callee = unsafe {
            /* SAFETY: JIT constant-pool word */
            Value::from_bits(callee_bits as usize)
        };
        // GNU's bytecode stack (`setup_frame`, src/bytecode.c:496-516): the
        // trampoline enters the leaf without the Rust entry, so it charges
        // the callee's DECLARED stack depth here, by the same rule. Over
        // budget raises GNU's error through the reference finish, which
        // unwinds the frame and depth the site already pushed.
        let saved_budget = match callee.bytecode_data_if_materialized() {
            Some(bc) => {
                // SAFETY: the seam's dormant Context (the shim contract).
                let ctx_ref = unsafe { &mut *(ctx as *mut Context) };
                match crate::emacs_core::bytecode::vm::charge_bytecode_frame(
                    ctx_ref,
                    bc.max_stack.get(),
                ) {
                    Some(saved_budget) => saved_budget,
                    None => {
                        super::super::shims::stash_pending_flow(neovm_value::error::signal(
                            "error",
                            vec![Value::string("Bytecode stack overflow")],
                        ));
                        return call_spec_finish(
                            ctx,
                            callee,
                            leaf,
                            args_ptr,
                            nargs,
                            out,
                            bt_count,
                            FastRun::Framed(NativeRun::Signal),
                            crate::emacs_core::bytecode::vm::BC_STACK_POSITION_UNCHARGED,
                        );
                    }
                }
            }
            // A stub is never armed as a framed direct leaf; it never
            // charged, so a release must be a no-op.
            None => crate::emacs_core::bytecode::vm::BC_STACK_POSITION_UNCHARGED,
        };
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
            if saved_budget != crate::emacs_core::bytecode::vm::BC_STACK_POSITION_UNCHARGED {
                crate::emacs_core::bytecode::vm::release_bytecode_frame(ctx_ref, saved_budget);
            }
            return STATUS_SIGNAL;
        }
        let run = match outcome {
            NativeRun::Ok(bits) => {
                if ctx_ref.pop_native_backtrace_frame(bt_count) {
                    ctx_ref.depth -= 1;
                    if saved_budget != crate::emacs_core::bytecode::vm::BC_STACK_POSITION_UNCHARGED
                    {
                        crate::emacs_core::bytecode::vm::release_bytecode_frame(
                            ctx_ref,
                            saved_budget,
                        );
                    }
                    // SAFETY: the generated code's result stack slot.
                    unsafe { *out = bits as i64 };
                    return STATUS_OK;
                }
                // A promoted/debug-on-exit frame needs the reference pop,
                // with GNU's decrement-before-exit-debugger ordering.
                FastRun::Done(unsafe {
                    /* SAFETY: JIT constant-pool word */
                    Value::from_bits(bits)
                })
            }
            other => FastRun::Framed(other),
        };
        // This owns depth/frame cleanup, signal propagation and a boxed
        // DeoptAt resume. In particular, a framed run is never FastRun::Raw.
        call_spec_finish(
            ctx,
            unsafe {
                /* SAFETY: JIT constant-pool word */
                Value::from_bits(callee_bits as usize)
            },
            leaf,
            args_ptr,
            nargs,
            out,
            bt_count,
            run,
            saved_budget,
        )
    }
}
