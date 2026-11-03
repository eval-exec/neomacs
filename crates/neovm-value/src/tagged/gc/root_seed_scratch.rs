//! Empty-between-collections storage in the collector's existing cold sidecar.

use crate::tagged::value::TaggedValue;
use std::cell::Cell;
use std::rc::Rc;

/// Only empty capacity is retained by the collector outside a seeding loan.
/// Taking and returning the Vec preserves its previous high-water capacity;
/// draining a source never shrinks it or requires a new reservation.
#[derive(Default)]
pub(super) struct RootSeedScratch(Rc<Cell<Vec<TaggedValue>>>);

/// Owns the temporary Values independently of any borrow of TaggedHeap.
/// Returning or unwinding clears them before publishing the capacity again.
pub(crate) struct RootSeedLoan {
    owner: Rc<Cell<Vec<TaggedValue>>>,
    roots: Vec<TaggedValue>,
}

static_assertions::assert_not_impl_any!(RootSeedLoan: Send, Sync, Clone, Copy);

impl RootSeedScratch {
    pub(super) fn loan(&self) -> RootSeedLoan {
        let roots = self.0.take();
        debug_assert!(
            roots.is_empty(),
            "root seed scratch retained Values between collections"
        );
        RootSeedLoan {
            owner: Rc::clone(&self.0),
            roots,
        }
    }
}

impl RootSeedLoan {
    pub(crate) fn roots(&mut self) -> &mut Vec<TaggedValue> {
        &mut self.roots
    }
}

impl Drop for RootSeedLoan {
    fn drop(&mut self) {
        self.roots.clear();
        debug_assert!(self.roots.is_empty());
        self.owner.set(std::mem::take(&mut self.roots));
    }
}

#[cfg(test)]
#[path = "tests/root_seed_scratch_test.rs"]
mod tests;
