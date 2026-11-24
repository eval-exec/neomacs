//! Test-support suspension of one Context so another may be constructed.
//!
//! `ParkedContext` has no eval API. Resuming it refuses while a different
//! heap is installed (`HeapInstalled`). The release refusal for a second
//! live Context stays `Context::new`.

use std::fmt;

use super::Context;

/// A Context whose heap slot has been cleared. No method evaluates it.
pub struct ParkedContext {
    context: Context,
}

/// `resume_for_test` found another heap installed on this thread.
pub struct ParkResumeError {
    kind: ParkResumeKind,
}

enum ParkResumeKind {
    HeapInstalled,
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
    /// Clear this Context's heap slot and return a value with no eval API.
    ///
    /// The caller can then construct another Context on this thread. Dropping
    /// the parked value drops the Context; it does not clear a heap installed
    /// by someone else.
    pub fn park_for_test(self) -> ParkedContext {
        neovm_value::tagged::gc::clear_tagged_heap_if_installed(&self.tagged_heap);
        ParkedContext { context: self }
    }

    /// Install `root` until the returned scope drops, and borrow `self` for
    /// that whole interval so this Context cannot be called inside it.
    ///
    /// Drop restores the heap that was installed before the call. A bare heap
    /// with no Context borrow is [`neovm_value::tagged::gc::NestedTaggedHeapInstall::install_bare`].
    ///
    /// # Safety
    ///
    /// `root` must be a live [`neovm_value::tagged::gc::AliasableBox`] root
    /// owned on this thread until the returned scope drops, with no live
    /// references into `root` during installation. Any displaced previous root
    /// must remain live until the returned scope drops, with no live references
    /// into it during restoration.
    ///
    /// ```compile_fail,E0499
    /// fn outer_is_unusable_inside_the_nest(ctx: &mut neovm_eval::emacs_core::Context) {
    ///     let root = std::ptr::NonNull::<neovm_value::tagged::gc::TaggedHeap>::dangling();
    ///     let _scope = unsafe { ctx.nest_foreign_heap_for_test(root) };
    ///     let _ = ctx.eval_str("1");
    /// }
    /// ```
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
    /// Install this Context again.
    ///
    /// Refuses with [`ParkResumeError`] (`HeapInstalled`) when a different
    /// heap is already installed. The armed test-fallback heap is not a
    /// Context and is replaced. The slot being empty is the ordinary path.
    pub fn resume_for_test(self) -> Result<Context, ParkResumeError> {
        if neovm_value::tagged::gc::tagged_heap_is_installed()
            && !neovm_value::tagged::gc::tagged_heap_is_current(&self.context.tagged_heap)
            && !neovm_value::tagged::gc::installed_slot_is_armed_test_fallback()
        {
            return Err(ParkResumeError {
                kind: ParkResumeKind::HeapInstalled,
            });
        }
        let mut context = self.context;
        context.setup_thread_locals();
        Ok(context)
    }
}
