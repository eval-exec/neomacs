//! The composed pictures of the last two frames, as values someone can hold.
//!
//! This replaces an `offscreen_a` / `offscreen_b` / `current_is_a` flip-flop
//! whose real contract was never written down anywhere: "after the flip, the
//! *other* texture still holds the previous frame's pixels". That contract is
//! exactly what a pool breaks if it hands back an arbitrary free slot, and it
//! is why a transition used to copy the whole previous frame into a third
//! texture on every start — a copy that existed only to pin pixels against
//! the next flip.
//!
//! A ring states the contract as ownership instead. `advance` *moves* the
//! current lease into `previous` and asks for a fresh slot for `current`, so
//! "the previous composition" is a value, and anything that needs it to
//! survive (a running crossfade, a departing pane) holds a clone. A held
//! slot's strong count is above one, so the pool declines to recycle it and
//! transparently allocates beside it: the copy is unnecessary and the
//! invariant cannot be lost by editing an index.

use super::gpu_budget::BudgetExceeded;
use super::snapshot_pool::{SnapshotLease, SnapshotResources, SnapshotSize};

pub struct CompositionRing<R = SnapshotResources> {
    current: SnapshotLease<R>,
    previous: Option<SnapshotLease<R>>,
}

impl<R> CompositionRing<R> {
    pub fn new(current: SnapshotLease<R>) -> Self {
        Self {
            current,
            previous: None,
        }
    }

    /// The slot this frame composes into.
    pub fn current(&self) -> &SnapshotLease<R> {
        &self.current
    }

    /// The picture the last frame composed, once there has been one.
    pub fn previous(&self) -> Option<&SnapshotLease<R>> {
        self.previous.as_ref()
    }

    /// The size every slot in this ring was cut at. A ring whose size no
    /// longer matches the surface has to be rebuilt rather than advanced.
    pub fn size(&self) -> SnapshotSize {
        self.current.size()
    }

    /// Rotate: the picture just composed becomes `previous`, and a fresh slot
    /// becomes `current`.
    pub fn advance(
        &mut self,
        acquire: impl FnOnce() -> Result<SnapshotLease<R>, BudgetExceeded>,
    ) -> Result<(), BudgetExceeded> {
        // Release the two-frames-back slot *before* asking for a new one. Its
        // pixels are already unreachable, and dropping it first is what lets
        // the pool hand that same slot back — which is how a ring at rest
        // costs two full-frame textures rather than three. If a transition is
        // still animating from it, its strong count stays above one, the pool
        // allocates beside it, and the pixels it is fading from survive.
        self.previous = None;
        let next = acquire()?;
        self.previous = Some(std::mem::replace(&mut self.current, next));
        Ok(())
    }
}

#[cfg(test)]
#[path = "composition_ring_test.rs"]
mod tests;
