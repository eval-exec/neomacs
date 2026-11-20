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

    /// Both books closed and both cursor pairs zero.
    pub fn is_closed(&self) -> bool {
        !sources_open(self.cons, self.float)
            && self.cons_cur.get() == 0
            && self.cons_lim.get() == 0
            && self.float_cur.get() == 0
            && self.float_lim.get() == 0
    }

    /// Install a free-list cons run. Does not read or write `heap.jit`.
    ///
    /// # Panics
    ///
    /// The cons book is already open, or `cells` is 0. A second grant has
    /// to close first; an empty run is not a region.
    pub fn install_cons_free_list(&mut self, first: usize, cells: usize) {
        self.install_cons(first, cells, ConsRegionSource::FreeListRun);
    }

    /// Install a block-tail cons run. Does not read or write `heap.jit`.
    ///
    /// # Panics
    ///
    /// The cons book is already open, or `cells` is 0.
    pub fn install_cons_block_tail(&mut self, first: usize, cells: usize, block: usize) {
        self.install_cons(first, cells, ConsRegionSource::BlockTail { block });
    }

    /// Install a float-page bump tail. Does not read or write `heap.jit`.
    ///
    /// # Panics
    ///
    /// The float book is already open, or `slots` is 0.
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
