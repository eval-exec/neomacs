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

/// The write barrier's owner window as compiled code sees it: an owner at
/// address `a` with `a - barrier_lo <u barrier_len` must take the
/// out-of-line barrier (the shim). The heap-side twin of the thread-local
/// `TAGGED_HEAP_BARRIER_WINDOW`, written by the same publisher
/// (`TaggedHeap::publish_barrier_window`), so the two never disagree while
/// the heap is installed.
#[repr(C)]
pub(crate) struct JitHeapState {
    pub(crate) barrier_lo: Cell<usize>,
    pub(crate) barrier_len: Cell<usize>,
}

const _: () = assert!(size_of::<JitHeapState>() == 16);

impl JitHeapState {
    /// A new heap's state: an empty window (no partition, no mark, no
    /// tracking).
    pub(super) const fn new() -> Self {
        Self {
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
        BarrierWindow::span(
            self.barrier_lo.get(),
            self.barrier_lo.get().wrapping_add(self.barrier_len.get()),
        )
    }
}

/// Byte offset, from a `*const TaggedHeap`, of the barrier window's `lo`.
pub(crate) const HEAP_JIT_BARRIER_LO: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, barrier_lo);
/// Byte offset, from a `*const TaggedHeap`, of the barrier window's `len`.
pub(crate) const HEAP_JIT_BARRIER_LEN: usize =
    std::mem::offset_of!(TaggedHeap, jit) + std::mem::offset_of!(JitHeapState, barrier_len);
