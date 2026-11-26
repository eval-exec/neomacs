//! One owner's cons-only allocation cursor.
//!
//! The serial bump stays in [`super::alloc_region`] and writes `JitHeapState`
//! (both cons and serial/JIT float bump windows). Parallel float and bignum
//! allocation belongs solely to N2 (`NumericStorage`), which mints owner pages
//! with zero allocation bits and publishes each slot via
//! `RunCore::commit_initialized_slot` only after payload initialization.
//!
//! This value is owned by the owner's `FrameArena` (`!Send` + `!Sync`). Its
//! bump writes only these cons cursor cells. A second owner has a second value,
//! so two owners' cursors cannot alias.
//!
//! The hot path does not refill. An exhausted run returns `None`; the cold
//! grant that fills the next run is carved from [`super::OwnerEraReserve`].

use super::ConsCell;
use super::alloc_region::ConsRegionSource;
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::size_of;

const CONS_STRIDE: usize = size_of::<ConsCell>();

/// Cons-only allocation cursor for one parallel owner. Not a copy of `heap.jit`.
///
/// `PhantomData<*const ()>` keeps the value off other threads. Cloning it
/// would hand the same run to two owners, so this type is not `Clone`.
pub struct OwnerAllocRegion {
    cons_cur: Cell<usize>,
    cons_lim: Cell<usize>,
    cons: ConsRegionSource,
    _thread: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(OwnerAllocRegion: Send, Sync, Clone, Copy);

/// What a stop-publication close does with the unhanded cons tail.
///
/// This is the decision. The era reserve's `close_for_stop` applies it and
/// re-checks a block tail's `next_index` before rewinding. A cursor installed
/// from a raw address has no reserve behind it, so applying that decision
/// would write the shared free list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ConsTailRefund {
    None,
    /// `[first, first + cells)` goes back on the block free list.
    FreeList {
        first: usize,
        cells: usize,
    },
    /// The block's bump tail rewinds to `first`. The reserve's apply re-checks
    /// `next_index` the way `close_cons_region` does, and free-lists the
    /// range if another grant extended the tail past this limit.
    Rewind {
        block: usize,
        first: usize,
        cells: usize,
    },
}

/// Refund record returned by [`OwnerAllocRegion::close_for_stop`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct OwnerRegionRefund {
    pub cons: ConsTailRefund,
}

impl OwnerAllocRegion {
    pub const fn new() -> Self {
        Self {
            cons_cur: Cell::new(0),
            cons_lim: Cell::new(0),
            cons: ConsRegionSource::Closed,
            _thread: PhantomData,
        }
    }

    /// Read-only close preflight for one cons reserve's exact carved range.
    /// The source, limit, and cursor must all belong to that run.
    pub(in crate::tagged::gc) fn matches_granted_cons(
        &self,
        first: usize,
        cells: usize,
        block: usize,
    ) -> bool {
        let Some(limit) = cells
            .checked_mul(CONS_STRIDE)
            .and_then(|bytes| first.checked_add(bytes))
        else {
            return false;
        };
        let current = self.cons_cur.get();
        matches!(self.cons, ConsRegionSource::BlockTail { block: actual } if actual == block)
            && self.cons_lim.get() == limit
            && first <= current
            && current <= limit
            && (current - first).is_multiple_of(CONS_STRIDE)
    }

    /// Cons book closed and cursor pair zero.
    pub fn is_closed(&self) -> bool {
        !cons_source_open(self.cons) && self.cons_cur.get() == 0 && self.cons_lim.get() == 0
    }

    /// Test seam: install a free-list cons run. Does not read or write `heap.jit`.
    ///
    /// Absent from a product build. A real run is installed by a sealed
    /// reserve grant, not by a caller-supplied address.
    ///
    /// # Panics
    ///
    /// The cons book is already open, or `cells` is 0. A second grant has
    /// to close first; an empty run is not a region.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn install_cons_free_list(&mut self, first: usize, cells: usize) {
        self.install_cons(first, cells, ConsRegionSource::FreeListRun);
    }

    /// Test seam: install a block-tail cons run. Does not read or write `heap.jit`.
    ///
    /// Absent from a product build.
    ///
    /// # Panics
    ///
    /// The cons book is already open, or `cells` is 0.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn install_cons_block_tail(&mut self, first: usize, cells: usize, block: usize) {
        self.install_cons(first, cells, ConsRegionSource::BlockTail { block });
    }

    /// The next cons cell, or `None` when the run is exhausted or closed.
    ///
    /// The miss is cold. This function does not refill and does not touch
    /// the heap cursors.
    #[inline(always)]
    pub fn take_cons_cell(&self) -> Option<usize> {
        take(&self.cons_cur, &self.cons_lim, CONS_STRIDE)
    }

    /// Refund the unused cons tail and leave the book closed.
    ///
    /// Only a publication that precedes a stop calls this. A poll that does
    /// not block must not. The returned record is the give-back; this method
    /// does not write the shared free list or `heap.jit`.
    ///
    /// A run carved from an [`super::OwnerEraReserve`] must instead use
    /// [`super::OwnerEraReserve::close_for_stop`] so its book records the
    /// refund. Closing that run here alone strands the book and leaves
    /// collection refused.
    pub fn close_for_stop(&mut self) -> OwnerRegionRefund {
        let cons_cur = self.cons_cur.get();
        let cons_lim = self.cons_lim.get();
        let cons_source = self.cons;
        self.cons_cur.set(0);
        self.cons_lim.set(0);
        self.cons = ConsRegionSource::Closed;
        OwnerRegionRefund {
            cons: cons_refund(cons_source, cons_cur, cons_lim, CONS_STRIDE),
        }
    }

    /// Install a run the stopped collector already reserved.
    ///
    /// Not a raw-address seam. The era reserve is the only caller.
    pub(in crate::tagged::gc) fn install_granted_cons(
        &mut self,
        first: usize,
        cells: usize,
        source: ConsRegionSource,
    ) {
        self.install_cons(first, cells, source);
    }

    fn install_cons(&mut self, first: usize, cells: usize, source: ConsRegionSource) {
        assert!(
            matches!(self.cons, ConsRegionSource::Closed),
            "cons region is already open"
        );
        assert!(cells >= 1, "a cons region grants at least one cell");
        self.cons_cur.set(first);
        self.cons_lim.set(first + cells * CONS_STRIDE);
        self.cons = source;
    }
}

#[inline(always)]
fn take(cur: &Cell<usize>, lim: &Cell<usize>, step: usize) -> Option<usize> {
    let here = cur.get();
    if here < lim.get() {
        cur.set(here + step);
        Some(here)
    } else {
        None
    }
}

fn unused(cur: usize, lim: usize, step: usize) -> usize {
    lim.saturating_sub(cur) / step
}

/// A new [`ConsRegionSource`] variant fails this match. Closed is the only
/// state that publishes.
fn cons_refund(source: ConsRegionSource, cur: usize, lim: usize, step: usize) -> ConsTailRefund {
    let cells = unused(cur, lim, step);
    match source {
        ConsRegionSource::Closed => {
            debug_assert_eq!((cur, lim), (0, 0));
            ConsTailRefund::None
        }
        ConsRegionSource::FreeListRun => {
            if cells == 0 {
                ConsTailRefund::None
            } else {
                ConsTailRefund::FreeList { first: cur, cells }
            }
        }
        ConsRegionSource::BlockTail { block } => {
            if cells == 0 {
                ConsTailRefund::None
            } else {
                ConsTailRefund::Rewind {
                    block,
                    first: cur,
                    cells,
                }
            }
        }
    }
}

impl super::TaggedHeap {
    /// Copied read of the heap JIT float window.
    ///
    /// Lives here so the heap root stays under its line ceiling. The owner
    /// bump does not call it.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn jit_float_window_for_test(&self) -> (usize, usize) {
        (self.jit.float_cur(), self.jit.float_lim())
    }
}

fn cons_source_open(cons: ConsRegionSource) -> bool {
    match cons {
        ConsRegionSource::Closed => false,
        ConsRegionSource::FreeListRun | ConsRegionSource::BlockTail { .. } => true,
    }
}
