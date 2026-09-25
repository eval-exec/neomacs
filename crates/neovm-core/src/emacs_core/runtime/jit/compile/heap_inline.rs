//! Heap writes inline in JIT code (lever P0.7): the stores `setcar` and
//! `setcdr` make, done in place instead of in `neovm_jit_setcar`/`_setcdr`.
//!
//! The write barrier's whole inline decision is one owner-address window
//! (`tagged::gc::BarrierWindow`), which the heap publishes into its
//! `JitHeapState` at every change of collector state. Compiled code reads it
//! through `vmctx -> Context.tagged_heap -> jit` and stores inline only when
//! the owner lies outside it; everything else — a non-cons, an owner in the
//! window (a concurrent mark or owner tracking makes the window ALL, the
//! dump partition makes it the image span) — takes the unchanged shim, which
//! signals, records and stores exactly as before.
//!
//! Why a plain store is sound outside the window: the window is ALL for the
//! whole life of a concurrent mark (`launch_concurrent_mark` publishes it
//! before the GC thread can read anything and `join_concurrent_mark` only
//! clears it after the thread has exited), so an inline store never races a
//! collector read; the next mark's start handshake (a channel send) orders
//! it before any. A cons is never tenured, so outside the window it has
//! nothing to remember.
//!
//! A body containing an inline site dereferences its vmctx: see
//! [`inline_heap_sites`] and `CompiledLeaf::needs_vmctx`.

use super::*;
use crate::emacs_core::eval::runtime_projection::CONTEXT_TAGGED_HEAP_OFFSET;
use crate::tagged::gc::{HEAP_JIT_BARRIER_LEN, HEAP_JIT_BARRIER_LO};

thread_local! {
    /// Inline heap sites emitted in the function being lowered.
    static INLINE_HEAP_SITES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Start the inline-heap-site count for a new function.
pub(crate) fn inline_heap_sites_reset() {
    INLINE_HEAP_SITES.with(|c| c.set(0));
}

/// Inline heap sites emitted since the last [`inline_heap_sites_reset`]: a
/// function with any reads the heap through its vmctx, so it must never be
/// entered with a null one.
pub(crate) fn inline_heap_sites() -> u32 {
    INLINE_HEAP_SITES.with(|c| c.get())
}

/// Inline stores emitted, process-wide, so a test can tell "the inline path
/// answered" from "the emitter declined and the shim answered".
#[cfg(debug_assertions)]
pub(crate) static INLINE_HEAP_STORES_EMITTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn note_inline_site() {
    INLINE_HEAP_SITES.with(|c| c.set(c.get() + 1));
}

/// Load the `*const TaggedHeap` off a `*mut Context`. The box is assigned
/// only by the context's constructors, so the pointer never changes.
pub(crate) fn load_heap_ptr(fb: &mut FunctionBuilder, vmctx: ClifValue) -> ClifValue {
    fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        vmctx,
        CONTEXT_TAGGED_HEAP_OFFSET as i32,
    )
}

/// The running context's heap: the entry block's hoisted load when the body
/// has one, else a load at the site.
pub(crate) fn heap_ptr(fb: &mut FunctionBuilder, rt: &RtCtx) -> ClifValue {
    if let Some(heap) = rt.heap {
        return heap;
    }
    let vmctx = fb.use_var(rt.vmctx_var);
    load_heap_ptr(fb, vmctx)
}

/// Branch to `slow` when the owner at untagged address `owner` lies in the
/// published barrier window; leaves the builder in a fresh sealed block on
/// the outside-the-window path. The window is reloaded at every site: a
/// shim or safe point between two sites may have changed it.
fn emit_barrier_window_check(
    fb: &mut FunctionBuilder,
    heap: ClifValue,
    owner: ClifValue,
    slow: Block,
) {
    let lo = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        heap,
        HEAP_JIT_BARRIER_LO as i32,
    );
    let len = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        heap,
        HEAP_JIT_BARRIER_LEN as i32,
    );
    let offset = fb.ins().isub(owner, lo);
    let inside = fb.ins().icmp(IntCC::UnsignedLessThan, offset, len);
    let outside = fb.create_block();
    fb.ins().brif(inside, slow, &[], outside, &[]);
    fb.switch_to_block(outside);
    fb.seal_block(outside);
}

/// `setcar` (`is_cdr == false`) or `setcdr` of `cell` to `value`, inline —
/// GNU `Bsetcar`/`Bsetcdr`'s `XSETCAR`/`XSETCDR`: a cons outside the barrier
/// window is stored in place, `res` is defined as `value` and control jumps
/// to `merge`; anything else branches to `slow`, where the caller emits the
/// unchanged shim call (the non-cons signal included). Neither op consults
/// the function cell, in GNU or here.
pub(crate) fn emit_inline_cons_store(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    cell: ClifValue,
    value: ClifValue,
    is_cdr: bool,
    slow: Block,
    res: Variable,
    merge: Block,
) {
    let tag = band_imm_p(fb, cell, TAG_MASK as i64);
    let is_cons = icmp_imm_p(fb, IntCC::Equal, tag, TAG_CONS as i64);
    let typed = fb.create_block();
    fb.ins().brif(is_cons, typed, &[], slow, &[]);
    fb.switch_to_block(typed);
    fb.seal_block(typed);
    // The tag is known, so untagging is a subtract (folds into the store's
    // addressing).
    let ptr = iadd_imm_p(fb, cell, -(TAG_CONS as i64));
    let heap = heap_ptr(fb, rt);
    emit_barrier_window_check(fb, heap, ptr, slow);
    let field = if is_cdr {
        core::mem::offset_of!(ConsCell, cdr_or_next)
    } else {
        core::mem::offset_of!(ConsCell, car)
    };
    fb.ins()
        .store(MemFlagsData::trusted(), value, ptr, field as i32);
    fb.def_var(res, value);
    fb.ins().jump(merge, &[]);
    note_inline_site();
    #[cfg(debug_assertions)]
    INLINE_HEAP_STORES_EMITTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Whether a baseline body's inline heap sites justify loading the heap
/// pointer once at entry: two sites, or one inside a loop (the root-window
/// hoisting rule).
pub(crate) fn hoist_heap_ptr(ops: &[Op], has_back_edge: bool) -> bool {
    let sites = ops.iter().filter(|op| op_is_inline_heap_site(op)).count();
    sites >= 2 || (sites == 1 && has_back_edge)
}

/// Whether the baseline lowers `op` with an inline heap site (JIT only).
fn op_is_inline_heap_site(op: &Op) -> bool {
    matches!(op, Op::Setcar | Op::Setcdr) && jit_inline_heap_write_on()
}
