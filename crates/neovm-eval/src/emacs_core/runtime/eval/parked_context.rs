//! Consuming suspension of one `Context` so another may be constructed or
//! loaded on the same thread without leaving a dormant active `Context`.
//!
//! `ParkedContext` has no eval API. Resuming it refuses while a different
//! heap is installed (`HeapInstalled`) and returns the parked `Context` inside
//! [`ParkResumeError`] so callers can retry after parking or dropping the
//! blocking `Context`.

use std::fmt;

use super::Context;

/// A `Context` whose heap slot has been cleared. No method evaluates it.
#[must_use]
pub struct ParkedContext {
    context: Context,
}

/// [`ParkedContext::resume`] found another heap installed on this thread.
pub struct ParkResumeError {
    kind: ParkResumeKind,
    parked: ParkedContext,
}

enum ParkResumeKind {
    HeapInstalled,
}

impl ParkResumeError {
    /// Recover the still-parked `Context` so the caller can retry after
    /// parking or dropping the blocking `Context`.
    pub fn into_parked(self) -> ParkedContext {
        self.parked
    }
}

impl fmt::Debug for ParkResumeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ParkResumeKind::HeapInstalled => f.write_str("HeapInstalled"),
        }
    }
}

impl fmt::Display for ParkResumeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ParkResumeKind::HeapInstalled => f.write_str("HeapInstalled"),
        }
    }
}

impl std::error::Error for ParkResumeError {}

impl Context {
    /// Clear this `Context`'s heap slot and return a parked handle with no
    /// eval API.
    ///
    /// Dropping the parked value drops the `Context`; it does not clear a heap
    /// installed by someone else.
    pub(crate) fn park(self) -> ParkedContext {
        // SAFETY: `self` is consumed by value into `ParkedContext`, which has
        // no eval API until explicitly resumed on an empty slot.
        unsafe { neovm_value::tagged::gc::clear_tagged_heap_if_installed(&self.tagged_heap) };
        ParkedContext { context: self }
    }

    /// Clear this `Context`'s heap slot and return a value with no eval API.
    ///
    /// The caller can then construct another `Context` on this thread. Dropping
    /// the parked value drops the `Context`; it does not clear a heap installed
    /// by someone else.
    #[cfg(any(test, feature = "test-support"))]
    pub fn park_for_test(self) -> ParkedContext {
        self.park()
    }

    /// Install `root` until the returned scope drops, and borrow `self` for
    /// that whole interval so this `Context` cannot be called inside it.
    ///
    /// Drop restores the heap that was installed before the call. A bare heap
    /// with no `Context` borrow is [`neovm_value::tagged::gc::NestedTaggedHeapInstall::install_bare`].
    /// The borrow is the trybuild fixture `nest_foreign_heap_borrows_the_context`
    /// (E0499). nextest does not run doctests.
    ///
    /// # Safety
    ///
    /// `root` must be a live [`neovm_value::tagged::gc::AliasableBox`] root
    /// owned on this thread until the returned scope drops, with no live
    /// references into `root` during installation. Forgetting the scope
    /// extends that liveness obligation indefinitely. Any displaced previous
    /// root must remain live until the returned scope drops, with no live
    /// references into it during restoration.
    #[cfg(any(test, feature = "test-support"))]
    pub unsafe fn nest_foreign_heap_for_test<'a>(
        &'a mut self,
        root: std::ptr::NonNull<neovm_value::tagged::gc::TaggedHeap>,
    ) -> neovm_value::tagged::gc::NestedTaggedHeapInstall<'a> {
        // The signature's `'a` is the borrow that keeps `self` unusable for
        // the scope. Touching `self` here would end that borrow early.
        let _ = std::ptr::from_mut(self);
        // SAFETY: caller upholds `NestedTaggedHeapInstall::install`'s contract.
        unsafe { neovm_value::tagged::gc::NestedTaggedHeapInstall::install(root) }
    }
}

#[cfg(test)]
impl ParkedContext {
    /// The suspended heap. Reads of collector state only; this does not
    /// install the heap and it is not an eval entry.
    pub(crate) fn tagged_heap(
        &self,
    ) -> &super::world::WorldView<neovm_value::tagged::gc::TaggedHeap> {
        self.context.tagged_heap()
    }

    /// Join this suspended heap's marker without installing it. Not an eval entry.
    pub(crate) fn finish_concurrent_mark(
        &mut self,
    ) -> Result<(), neovm_value::tagged::gc::MarkFinishError> {
        self.context.finish_concurrent_mark()
    }
}

impl ParkedContext {
    /// Install this `Context` again.
    ///
    /// Refuses with [`ParkResumeError`] (`HeapInstalled`) when a different
    /// heap is already installed; the error retains the parked `Context` via
    /// [`ParkResumeError::into_parked`]. The armed test-fallback heap is not a
    /// `Context` and is replaced. The slot being empty is the ordinary path.
    pub fn resume(self) -> Result<Context, ParkResumeError> {
        if neovm_value::tagged::gc::tagged_heap_is_installed()
            && !neovm_value::tagged::gc::tagged_heap_is_current(&self.context.tagged_heap)
            && !neovm_value::tagged::gc::installed_slot_is_armed_test_fallback()
        {
            return Err(ParkResumeError {
                kind: ParkResumeKind::HeapInstalled,
                parked: self,
            });
        }
        let mut context = self.context;
        context.setup_thread_locals();
        Ok(context)
    }

    /// Test-support alias for [`Self::resume`].
    #[cfg(any(test, feature = "test-support"))]
    pub fn resume_for_test(self) -> Result<Context, ParkResumeError> {
        self.resume()
    }
}
