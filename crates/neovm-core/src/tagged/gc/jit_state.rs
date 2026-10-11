//! Heap state that compiled code reads and writes in place.
//!
//! `JitHeapState` lives inside `TaggedHeap` (which `Context` boxes), so its
//! address is fixed for the heap's life, and JIT code reaches it as
//! `vmctx -> Context.tagged_heap -> jit`. Its fields are `Cell`s: compiled
//! code mutates them behind Rust's back, and the mutator is the only thread
//! that touches them.
//!
//! Every offset compiled code bakes is a compile-time constant here
//! (`HEAP_JIT_*`), so a layout change moves the constant with it.

use super::*;

/// The allocation cursors and the write barrier's owner window.
///
/// **Cons region** (`cons_cur`, `cons_lim`): the open cons allocation region
/// (`alloc_region.rs`) — `cons_cur` is the next cell to hand out, `cons_lim`
/// one past the region's last cell; both are 0 when no region is open.
/// Regions are whole cells, so `cur < lim` iff a cell is left. Rust
/// (`TaggedHeap::take_cons_cell`) and compiled code bump the same cursor.
///
/// **Float region** (`float_cur`, `float_lim`): the same for float slots
/// (stride `FloatObj::SLOT_BYTES`, 32), whose headers were written when
/// the region was granted (`TaggedHeap::alloc_float_inline`).
///
/// **Barrier window** (`barrier_lo`, `barrier_len`): an owner at address `a`
/// with `a - barrier_lo <u barrier_len` must take the out-of-line barrier
/// (the shim). With GEN0 observed journaling this encloses the ordinary GC
/// window and the executing mutator's observed-owner envelope. Window hits
/// still check the exact sticky mark before journaling. The Rust TLS window
/// remains the ordinary GC window. GEN1 retains that ordinary native window.
/// Only the executing mutator publishes these Cells; Context installation
/// re-derives the window from that mutator's persistent collection history.
#[repr(C)]
pub(crate) struct JitHeapState {
    pub(crate) cons_cur: Cell<usize>,
    pub(crate) cons_lim: Cell<usize>,
    pub(crate) float_cur: Cell<usize>,
    pub(crate) float_lim: Cell<usize>,
    pub(crate) barrier_lo: Cell<usize>,
    pub(crate) barrier_len: Cell<usize>,
}

const _: () = assert!(size_of::<JitHeapState>() == 48);

impl JitHeapState {
    /// A new heap's state: no open region, an empty window (no partition,
    /// no mark, no tracking).
    pub(super) const fn new() -> Self {
        Self {
            cons_cur: Cell::new(0),
            cons_lim: Cell::new(0),
            float_cur: Cell::new(0),
            float_lim: Cell::new(0),
            barrier_lo: Cell::new(0),
            barrier_len: Cell::new(0),
        }
    }

    /// Store `window` for compiled code.
    #[inline]
    pub(super) fn set_barrier_window(&self, window: BarrierWindow) {
        self.barrier_lo.set(window.lo());
        self.barrier_len.set(window.len());
    }

    /// The window compiled code currently tests.
    #[cfg(test)]
    pub(crate) fn barrier_window(&self) -> BarrierWindow {
        BarrierWindow::from_lo_len(self.barrier_lo.get(), self.barrier_len.get())
    }
}

/// Byte offset, from a `*const TaggedHeap`, of the open cons region's cursor.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const HEAP_JIT_CONS_CUR: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, cons_cur);
/// Byte offset, from a `*const TaggedHeap`, of the open cons region's limit.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const HEAP_JIT_CONS_LIM: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, cons_lim);
/// Byte offset, from a `*const TaggedHeap`, of the open float region's
/// cursor.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const HEAP_JIT_FLOAT_CUR: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, float_cur);
/// Byte offset, from a `*const TaggedHeap`, of the open float region's
/// limit.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const HEAP_JIT_FLOAT_LIM: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, float_lim);
/// A float slot's stride, what compiled code bumps the float cursor by.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const FLOAT_SLOT_BYTES: usize = <FloatObj as PagedObject>::SLOT_BYTES;
/// Byte offset, from a `*const TaggedHeap`, of the barrier window's `lo`.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const HEAP_JIT_BARRIER_LO: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, barrier_lo);
/// Byte offset, from a `*const TaggedHeap`, of the barrier window's `len`.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const HEAP_JIT_BARRIER_LEN: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, barrier_len);

// ABI 32's measured production layout. Deriving offsets above keeps generated
// code consistent with this build; these independent pins also detect drift
// from the existing ABI. Test-only heap fields and unmeasured configurations
// have their own layout probes rather than inheriting these numeric values.
#[cfg(all(
    not(test),
    target_arch = "x86_64",
    target_os = "linux",
    target_pointer_width = "64",
    feature = "jit",
    feature = "flow-word",
    not(feature = "gc-memory-telemetry")
))]
const _: () = {
    assert!(HEAP_JIT_CONS_CUR == 0xe18);
    assert!(HEAP_JIT_CONS_LIM == 0xe20);
    assert!(HEAP_JIT_FLOAT_CUR == 0xe28);
    assert!(HEAP_JIT_FLOAT_LIM == 0xe30);
    assert!(HEAP_JIT_BARRIER_LO == 0xe38);
    assert!(HEAP_JIT_BARRIER_LEN == 0xe40);
};
