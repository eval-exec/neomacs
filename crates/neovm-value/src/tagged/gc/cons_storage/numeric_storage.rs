//! Complete numeric arenas carried by the existing stopped catalog transfer.
//!
//! No numeric allocation or descriptor borrow leaves the catalog in N1. The
//! bank preserves allocated-slot state, partial chains, retired pages, spare
//! storage, and chunk-map entries by moving each arena without rebuilding it.

use super::super::TaggedHeap;
use super::super::arena_pages::ObjectArena;
use crate::tagged::header::{BignumObj, FloatObj};

pub(super) struct NumericStorage {
    floats: ObjectArena<FloatObj>,
    bignums: ObjectArena<BignumObj>,
}

impl NumericStorage {
    /// The caller's catalog transaction excludes admission and retains the
    /// original heap loan. No owner can run until that loan is suspended.
    pub(super) fn detach(heap: &mut TaggedHeap) -> Self {
        let floats = heap.float_arena.empty_for_transfer();
        let bignums = heap.bignum_arena.empty_for_transfer();
        Self {
            floats: std::mem::replace(&mut heap.float_arena, floats),
            bignums: std::mem::replace(&mut heap.bignum_arena, bignums),
        }
    }

    /// Check both arenas before the transaction moves any catalog storage.
    pub(super) fn placeholders_intact(&self, heap: &TaggedHeap) -> bool {
        heap.float_arena.is_transfer_placeholder_for(&self.floats)
            && heap.bignum_arena.is_transfer_placeholder_for(&self.bignums)
    }

    /// The existing transaction validated the complete shell before taking
    /// the catalog state. Only pristine, storage-free placeholders are dropped.
    pub(super) fn restore(self, heap: &mut TaggedHeap) {
        heap.float_arena = self.floats;
        heap.bignum_arena = self.bignums;
    }
}

static_assertions::assert_not_impl_any!(NumericStorage: Clone, Copy);
