//! One owner's cons and float cursors.
//!
//! The serial bump stays in [`super::alloc_region`] and writes `JitHeapState`.
//! This value is owned by the owner's `FrameArena` (`!Send` + `!Sync`). Its
//! bump writes only these cells. A second owner has a second value, so the
//! two cursors cannot alias.
//!
//! The hot path does not refill. An exhausted run returns `None`; the cold
//! grant that fills the next run is a later slice and is not called from here.

use super::ConsCell;
use super::alloc_region::{ConsRegionSource, FloatRegionSource};
use super::arena_pages::ArenaRunSource;
use super::jit_state::FLOAT_SLOT_BYTES;
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::size_of;

/// Cursor classes this region can bump. A new class fails this match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorClass {
    Cons,
    Float,
}

fn stride(class: CursorClass) -> usize {
    match class {
        CursorClass::Cons => size_of::<ConsCell>(),
        CursorClass::Float => FLOAT_SLOT_BYTES,
    }
}

/// Cons and float cursors for one owner. Not a copy of `heap.jit`.
///
/// `PhantomData<*const ()>` keeps the value off other threads. Cloning it
/// would hand the same run to two owners, so this type is not `Clone`.
pub struct OwnerAllocRegion {
    cons_cur: Cell<usize>,
    cons_lim: Cell<usize>,
    cons: ConsRegionSource,
    float_cur: Cell<usize>,
    float_lim: Cell<usize>,
    float: FloatRegionSource,
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
    /// The block's bump tail rewinds to `first`. Slice 3's apply re-checks
    /// `next_index` the way `close_cons_region` does, and free-lists the
    /// range if another grant extended the tail past this limit.
    Rewind {
        block: usize,
        first: usize,
        cells: usize,
    },
}

/// What a stop-publication close does with the unhanded float tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum FloatTailRefund {
    None,
    PageFree {
        page: usize,
        first: usize,
        slots: usize,
    },
    PageTail {
        page: usize,
        first: usize,
        slots: usize,
    },
}

/// Both tails of one [`OwnerAllocRegion::close_for_stop`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct OwnerRegionRefund {
    pub cons: ConsTailRefund,
    pub float: FloatTailRefund,
}

impl OwnerAllocRegion {
    pub const fn new() -> Self {
        Self {
            cons_cur: Cell::new(0),
            cons_lim: Cell::new(0),
            cons: ConsRegionSource::Closed,
            float_cur: Cell::new(0),
            float_lim: Cell::new(0),
            float: FloatRegionSource::Closed,
            _thread: PhantomData,
        }
    }

    /// The cons book alone. A float run may stay open.
    pub(in crate::tagged::gc) fn cons_book_closed(&self) -> bool {
        matches!(self.cons, ConsRegionSource::Closed)
    }

    /// The float book alone. A cons run may stay open.
    pub(in crate::tagged::gc) fn float_book_closed(&self) -> bool {
        matches!(self.float, FloatRegionSource::Closed)
    }

    /// Both books closed and both cursor pairs zero.
    pub fn is_closed(&self) -> bool {
        !sources_open(self.cons, self.float)
            && self.cons_cur.get() == 0
            && self.cons_lim.get() == 0
            && self.float_cur.get() == 0
            && self.float_lim.get() == 0
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

    /// Test seam: install a float-page free-list run. Does not read or write `heap.jit`.
    ///
    /// Absent from a product build.
    ///
    /// # Panics
    ///
    /// The float book is already open, or `slots` is 0.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn install_float_page_free(&mut self, first: usize, slots: usize, page: usize) {
        self.install_float(
            first,
            slots,
            FloatRegionSource::Run {
                page,
                source: ArenaRunSource::PageFreeRun,
            },
        );
    }

    /// Test seam: install a float-page bump tail. Does not read or write `heap.jit`.
    ///
    /// Absent from a product build.
    ///
    /// # Panics
    ///
    /// The float book is already open, or `slots` is 0.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn install_float_page_tail(&mut self, first: usize, slots: usize, page: usize) {
        self.install_float(
            first,
            slots,
            FloatRegionSource::Run {
                page,
                source: ArenaRunSource::PageTail,
            },
        );
    }

    /// The next cons cell, or `None` when the run is exhausted or closed.
    ///
    /// The miss is cold. This function does not refill and does not touch
    /// the heap cursors.
    #[inline(always)]
    pub fn take_cons_cell(&self) -> Option<usize> {
        take(&self.cons_cur, &self.cons_lim, stride(CursorClass::Cons))
    }

    /// The next float slot, or `None` when the run is exhausted or closed.
    #[inline(always)]
    pub fn take_float_slot(&self) -> Option<usize> {
        take(&self.float_cur, &self.float_lim, stride(CursorClass::Float))
    }

    /// Refund the unused tails and leave both books closed.
    ///
    /// Only a publication that precedes a stop calls this. A poll that does
    /// not block must not. The returned record is the give-back; this method
    /// does not write the shared free list or `heap.jit`.
    pub fn close_for_stop(&mut self) -> OwnerRegionRefund {
        let cons_step = stride(CursorClass::Cons);
        let float_step = stride(CursorClass::Float);
        let cons_cur = self.cons_cur.get();
        let cons_lim = self.cons_lim.get();
        let float_cur = self.float_cur.get();
        let float_lim = self.float_lim.get();
        let cons_source = self.cons;
        let float_source = self.float;
        self.cons_cur.set(0);
        self.cons_lim.set(0);
        self.float_cur.set(0);
        self.float_lim.set(0);
        self.cons = ConsRegionSource::Closed;
        self.float = FloatRegionSource::Closed;
        OwnerRegionRefund {
            cons: cons_refund(cons_source, cons_cur, cons_lim, cons_step),
            float: float_refund(float_source, float_cur, float_lim, float_step),
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

    /// Install a float run the stopped collector already reserved.
    pub(in crate::tagged::gc) fn install_granted_float(
        &mut self,
        first: usize,
        slots: usize,
        source: FloatRegionSource,
    ) {
        self.install_float(first, slots, source);
    }

    fn install_cons(&mut self, first: usize, cells: usize, source: ConsRegionSource) {
        assert!(
            matches!(self.cons, ConsRegionSource::Closed),
            "cons region is already open"
        );
        assert!(cells >= 1, "a cons region grants at least one cell");
        let step = stride(CursorClass::Cons);
        self.cons_cur.set(first);
        self.cons_lim.set(first + cells * step);
        self.cons = source;
    }

    fn install_float(&mut self, first: usize, slots: usize, source: FloatRegionSource) {
        assert!(
            matches!(self.float, FloatRegionSource::Closed),
            "float region is already open"
        );
        assert!(slots >= 1, "a float region grants at least one slot");
        let step = stride(CursorClass::Float);
        self.float_cur.set(first);
        self.float_lim.set(first + slots * step);
        self.float = source;
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

/// A new [`ConsRegionSource`] or [`FloatRegionSource`] variant fails this
/// match. Closed is the only state that publishes.
fn unused(cur: usize, lim: usize, step: usize) -> usize {
    lim.saturating_sub(cur) / step
}

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

fn float_refund(source: FloatRegionSource, cur: usize, lim: usize, step: usize) -> FloatTailRefund {
    let slots = unused(cur, lim, step);
    match source {
        FloatRegionSource::Closed => {
            debug_assert_eq!((cur, lim), (0, 0));
            FloatTailRefund::None
        }
        FloatRegionSource::Run { page, source } => {
            if slots == 0 {
                return FloatTailRefund::None;
            }
            match source {
                ArenaRunSource::PageFreeRun => FloatTailRefund::PageFree {
                    page,
                    first: cur,
                    slots,
                },
                ArenaRunSource::PageTail => FloatTailRefund::PageTail {
                    page,
                    first: cur,
                    slots,
                },
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
        (self.jit.float_cur.get(), self.jit.float_lim.get())
    }
}

fn sources_open(cons: ConsRegionSource, float: FloatRegionSource) -> bool {
    let cons_open = match cons {
        ConsRegionSource::Closed => false,
        ConsRegionSource::FreeListRun | ConsRegionSource::BlockTail { .. } => true,
    };
    let float_open = match float {
        FloatRegionSource::Closed => false,
        FloatRegionSource::Run { source, .. } => match source {
            ArenaRunSource::PageFreeRun | ArenaRunSource::PageTail => true,
        },
    };
    cons_open || float_open
}
