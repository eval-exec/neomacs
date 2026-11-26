//! Closed source compiler scopes. Observing that a scope is absent does not
//! grant permission to transport a Context, execute code, or construct an
//! N-mutator JIT admission.

use super::bg::source_suspension::DetachedBgMetadata;
use super::compile::shared::source_detachment::DetachedSourceBackend;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceScopeSlot {
    FusedBody,
    TierBuild,
    CallSource,
    FeedbackHolds,
    DeferredCode,
    PendingIds,
    EpochDiscards,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceScopeRefusal {
    HeapInstalled,
    CompileEnvironment,
    FusedBody,
    TierBuild,
    CallSource,
    FeedbackHolds,
    DeferredScope,
    DeferredCode,
    PendingJobs,
    Borrowed(SourceScopeSlot),
}

/// Retains the actual background metadata after every owning pointer scope
/// has been observed absent. The original frame-return precondition forbids
/// later source execution from reactivating any of those scopes.
#[must_use]
pub(crate) struct SourceCompilerClosed {
    _background: ManuallyDrop<DetachedBgMetadata>,
    _local: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for SourceCompilerClosed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceCompilerClosed")
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(SourceCompilerClosed: Send, Sync, Clone, Copy);

impl Drop for SourceCompilerClosed {
    fn drop(&mut self) {
        // Retain the background id and epoch allocation. Taking the
        // ManuallyDrop would free metadata a later return protocol still names.
        let _retained = (&self._background, &self._local);
    }
}

impl SourceCompilerClosed {
    /// # Safety
    ///
    /// `backend` is the actual preceding source receipt. The same sole source
    /// and all prior cache, root, and native backing remain retained and
    /// inaccessible. Every complete Lisp, native, compiler, collector, and
    /// store frame has returned. No source execution or GC may occur before
    /// the complete publication or return protocol. Slot observations cannot
    /// prove that an unrepresented stack borrow has ended.
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn close(
        _backend: &DetachedSourceBackend,
    ) -> Result<Self, SourceScopeRefusal> {
        if neovm_value::tagged::gc::tagged_heap_is_installed() {
            return Err(SourceScopeRefusal::HeapInstalled);
        }
        // Every read-only refusal precedes the sole mutating effect. Inactive
        // scalar compile policies are not heap or World authority.
        super::compile::inline_vars::validate_source_compile_scope()?;
        super::inline::validate_source_compile_scope()?;
        super::tier2::validate_source_compile_scope()?;
        super::compile::call_feedback::validate_source_compile_scope()?;
        Ok(Self {
            _background: ManuallyDrop::new(DetachedBgMetadata::withdraw()?),
            _local: PhantomData,
        })
    }
}
