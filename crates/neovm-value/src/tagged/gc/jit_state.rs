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
#[doc(hidden)]
pub struct JitHeapState {
    cons_cur: Cell<usize>,
    cons_lim: Cell<usize>,
    float_cur: Cell<usize>,
    float_lim: Cell<usize>,
    barrier_lo: Cell<usize>,
    barrier_len: Cell<usize>,
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

    #[inline(always)]
    pub(super) fn cons_cur(&self) -> usize {
        self.cons_cur.get()
    }

    #[inline(always)]
    pub(super) fn cons_lim(&self) -> usize {
        self.cons_lim.get()
    }

    #[inline(always)]
    pub(super) fn float_cur(&self) -> usize {
        self.float_cur.get()
    }

    #[inline(always)]
    pub(super) fn float_lim(&self) -> usize {
        self.float_lim.get()
    }

    /// True if any cons or float bump cursor/limit is non-zero.
    #[inline]
    pub(super) fn has_open_cursors(&self) -> bool {
        self.cons_cur.get() != 0
            || self.cons_lim.get() != 0
            || self.float_cur.get() != 0
            || self.float_lim.get() != 0
    }

    /// Bump and return the next reserved cons cell address, or `None` when
    /// the open cons region is exhausted or closed.
    #[inline(always)]
    pub(super) fn take_cons_cell(&self) -> Option<usize> {
        let cur = self.cons_cur.get();
        if cur < self.cons_lim.get() {
            self.cons_cur.set(cur + size_of::<ConsCell>());
            Some(cur)
        } else {
            None
        }
    }

    /// Bump and return the next reserved float slot address, or `None` when
    /// the open float region is exhausted or closed.
    #[inline(always)]
    pub(super) fn take_float_slot(&self) -> Option<usize> {
        let cur = self.float_cur.get();
        if cur < self.float_lim.get() {
            self.float_cur.set(cur + FLOAT_SLOT_BYTES);
            Some(cur)
        } else {
            None
        }
    }

    /// Install an exclusively reserved cons bump region `[cur, lim)`.
    ///
    /// # Safety
    /// `[cur, lim)` must be an exclusively reserved, `ConsCell`-aligned, live
    /// cons cell range of this heap (`cur <= lim`, `(lim - cur) % size_of::<ConsCell>() == 0`)
    /// that remains valid until closed via [`Self::clear_cons_region`] before
    /// any collector phase change, sweep, or heap drop.
    #[inline]
    pub(super) unsafe fn install_cons_region(&self, cur: usize, lim: usize) {
        debug_assert!(cur <= lim && (lim - cur).is_multiple_of(size_of::<ConsCell>()));
        self.cons_cur.set(cur);
        self.cons_lim.set(lim);
    }

    /// Install an exclusively reserved, pre-initialized float bump region `[cur, lim)`.
    ///
    /// # Safety
    /// `[cur, lim)` must be an exclusively reserved, `FloatObj`-aligned, live
    /// float slot range of this heap with initialized `GcHeader`s (`cur <= lim`,
    /// `(lim - cur) % FLOAT_SLOT_BYTES == 0`) that remains valid until closed
    /// via [`Self::clear_float_region`] before any collector phase change,
    /// sweep, or heap drop.
    #[inline]
    pub(super) unsafe fn install_float_region(&self, cur: usize, lim: usize) {
        debug_assert!(cur <= lim && (lim - cur).is_multiple_of(FLOAT_SLOT_BYTES));
        self.float_cur.set(cur);
        self.float_lim.set(lim);
    }

    /// Clear the cons bump cursors to `(0, 0)` and return the previous `(cur, lim)`.
    #[inline]
    pub(super) fn clear_cons_region(&self) -> (usize, usize) {
        let pair = (self.cons_cur.get(), self.cons_lim.get());
        self.cons_cur.set(0);
        self.cons_lim.set(0);
        pair
    }

    /// Clear the float bump cursors to `(0, 0)` and return the previous `(cur, lim)`.
    #[inline]
    pub(super) fn clear_float_region(&self) -> (usize, usize) {
        let pair = (self.float_cur.get(), self.float_lim.get());
        self.float_cur.set(0);
        self.float_lim.set(0);
        pair
    }

    /// Store `window` for compiled code.
    #[inline]
    pub(super) fn set_barrier_window(&self, window: BarrierWindow) {
        self.barrier_lo.set(window.lo());
        self.barrier_len.set(window.len());
    }

    /// Store `window` through a raw `*const JitHeapState` without creating a
    /// `&TaggedHeap` over sibling fields (value-gc C4).
    ///
    /// # Safety
    /// `this` must point to a live `JitHeapState` belonging to the executing
    /// mutator's installed heap, with no concurrent reader/writer of
    /// `barrier_lo` or `barrier_len`.
    #[inline]
    pub(super) unsafe fn set_barrier_window_raw(this: *const Self, window: BarrierWindow) {
        // SAFETY: caller guarantees `this` points to a live `JitHeapState` on
        // the executing mutator's thread; projecting only `barrier_lo` and
        // `barrier_len` avoids retagging sibling fields.
        unsafe {
            (*std::ptr::addr_of!((*this).barrier_lo)).set(window.lo());
            (*std::ptr::addr_of!((*this).barrier_len)).set(window.len());
        }
    }

    /// Test-only raw cursor injection for `image_tests` open-region refusal checks.
    ///
    /// # Safety
    /// Callers must never allocate from this `JitHeapState` while synthetic
    /// non-zero cursors are installed, and must reset them to zero before use.
    #[cfg(test)]
    pub(super) unsafe fn set_raw_cursors_for_test(
        &self,
        cons_cur: usize,
        cons_lim: usize,
        float_cur: usize,
        float_lim: usize,
    ) {
        self.cons_cur.set(cons_cur);
        self.cons_lim.set(cons_lim);
        self.float_cur.set(float_cur);
        self.float_lim.set(float_lim);
    }

    /// The window compiled code currently tests.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn barrier_window(&self) -> BarrierWindow {
        BarrierWindow::from_lo_len(self.barrier_lo.get(), self.barrier_len.get())
    }
}

/// Byte offset, from a `*const TaggedHeap`, of the open cons region's cursor.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[doc(hidden)]
pub const HEAP_JIT_CONS_CUR: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, cons_cur);
/// Byte offset, from a `*const TaggedHeap`, of the open cons region's limit.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[doc(hidden)]
pub const HEAP_JIT_CONS_LIM: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, cons_lim);
/// Byte offset, from a `*const TaggedHeap`, of the open float region's
/// cursor.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[doc(hidden)]
pub const HEAP_JIT_FLOAT_CUR: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, float_cur);
/// Byte offset, from a `*const TaggedHeap`, of the open float region's
/// limit.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[doc(hidden)]
pub const HEAP_JIT_FLOAT_LIM: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, float_lim);
/// A float slot's stride, what compiled code bumps the float cursor by.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[doc(hidden)]
pub const FLOAT_SLOT_BYTES: usize = <FloatObj as PagedObject>::SLOT_BYTES;
/// Byte offset, from a `*const TaggedHeap`, of the barrier window's `lo`.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[doc(hidden)]
pub const HEAP_JIT_BARRIER_LO: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, barrier_lo);
/// Byte offset, from a `*const TaggedHeap`, of the barrier window's `len`.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[doc(hidden)]
pub const HEAP_JIT_BARRIER_LEN: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, barrier_len);

// ABI 32's measured production layout. Deriving offsets above keeps generated
// code consistent with this build; these independent pins also detect drift
// from the existing ABI. Test-only heap fields and unmeasured configurations
// have their own layout probes rather than inheriting these numeric values.
#[cfg(all(
    not(any(test, feature = "test-support")),
    target_arch = "x86_64",
    target_os = "linux",
    target_pointer_width = "64",
    feature = "jit",
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
