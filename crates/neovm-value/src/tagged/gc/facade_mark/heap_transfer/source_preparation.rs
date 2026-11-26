//! Reconstruct only the already-quiescent source heap, after every source
//! argument frame has returned. This performs no second census, no collecting
//! drain, and it does not name a `Context`.

use super::super::QuiescentCollector;
use super::{PendingHeapReturn, PreparedHeapTransfer, RetainedBootstrapParts, TransferError};
use crate::tagged::collection_reads::SourceObservationsDetached;
use crate::tagged::gc::{
    EraWorldLoan, FrozenLegacyRoots, Gen1Admission, Gen1AdmissionError, WorldLoan,
    tagged_heap_is_installed,
};

/// `prepare_closed_source` refused before publishing a right.
///
/// The original owner, census, and observation receipt stay inside. Drop does
/// not recover them: a refusal keeps the inputs the caller already suspended.
#[must_use]
#[doc(hidden)]
pub struct ClosedSourcePrepareFailure {
    reason: TransferError,
    parts: RetainedBootstrapParts,
}

impl ClosedSourcePrepareFailure {
    #[doc(hidden)]
    pub fn reason(&self) -> TransferError {
        self.reason
    }

    /// Heap identity of the owner retained by this refusal.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn retained_owner_heap_identity(&self) -> crate::tagged::gc::HeapIdentity {
        self.parts.owner.parallel_heap_identity()
    }

    /// Census length retained by this refusal. The words stay private.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn retained_root_word_count(&self) -> usize {
        self.parts.roots.words().len()
    }
}

impl Drop for ClosedSourcePrepareFailure {
    fn drop(&mut self) {
        // Name the retained inputs. Taking the ManuallyDrop would free the
        // owner, census, and receipt this refusal is required to keep.
        let _retained = (&self.parts, self.reason);
    }
}

impl std::fmt::Debug for ClosedSourcePrepareFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("ClosedSourcePrepareFailure")
            .field(&self.reason)
            .finish()
    }
}

static_assertions::assert_not_impl_any!(ClosedSourcePrepareFailure: Send, Sync, Clone, Copy);

impl PreparedHeapTransfer {
    /// # Safety
    /// `owner`, `roots`, and `observations` are the actual sole suspended
    /// source's original loan, final census, and observation-withdrawal
    /// receipt. Its completed collector drained this same heap and returned
    /// before this call. Every Context, collector, store, native, and compiler
    /// argument frame has returned. Host, JIT, and projection activations, and
    /// non-atomic observation owners, are withdrawn and retained in the
    /// inaccessible source shell.
    ///
    /// No heap mutation, collection, or source execution has occurred since
    /// that quiescent point, and none may occur until this constructor's
    /// reference-free result returns through every frame that still names it.
    /// The owner's retained epoch excludes a concurrent marker throughout.
    /// Metadata or `Rc` counts alone cannot establish these source-frame facts.
    ///
    /// This function supplies no serial recovery on failure, unwind, or forget.
    /// The reconstructed collector frame must return before any resulting right
    /// is used or sent.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub unsafe fn prepare_closed_source(
        owner: WorldLoan,
        roots: FrozenLegacyRoots,
        observations: SourceObservationsDetached,
    ) -> Result<(PendingHeapReturn, Self), ClosedSourcePrepareFailure> {
        let parts = RetainedBootstrapParts::new(owner, roots, observations);
        // Identity checks only. None of these dereference the heap allocation.
        let refusal = if tagged_heap_is_installed() {
            Some(TransferError::InstalledTls)
        } else if !parts.owner.held_here() {
            Some(TransferError::WrongThread)
        } else if parts.roots.heap_identity() != parts.owner.parallel_heap_identity() {
            Some(TransferError::WrongHeap)
        } else {
            None
        };
        if let Some(reason) = refusal {
            return Err(ClosedSourcePrepareFailure { reason, parts });
        }
        let root = parts.owner.heap_root_designator();
        // SAFETY: `root` is the original retained allocation root, not a
        // pointer derived from a collector reference. The previous collector
        // frame ended, and the closed source has not touched the heap since.
        // Reconstruct the quiescent loan without a collecting operation.
        let collector =
            unsafe { QuiescentCollector::from_retained_root(&mut *root.as_non_null().as_ptr()) };
        let admission = match Gen1Admission::check(collector) {
            Ok(admission) => admission,
            Err(failure) => {
                let (reason, collector) = failure.into_parts();
                drop(collector);
                let reason = match reason {
                    Gen1AdmissionError::GenerationalRequired => TransferError::GenerationalRequired,
                };
                return Err(ClosedSourcePrepareFailure { reason, parts });
            }
        };
        let (owner, roots, observations) = parts.into_parts();
        // SAFETY: the caller met this constructor's source-frame contract. The
        // inner constructor checks the receipt and publishes the original
        // census. Its argument frame returns here, before any resulting right
        // is used or sent. No local cleanup touches the heap.
        match unsafe { Self::prepare_detached(admission, owner, roots, observations) } {
            Ok(prepared) => Ok(prepared),
            Err(failure) => {
                let (reason, admission, owner, roots, observations) = failure.into_parts();
                drop(admission);
                let observations =
                    observations.expect("prepare_detached stores the detached receipt");
                Err(ClosedSourcePrepareFailure {
                    reason,
                    parts: RetainedBootstrapParts::new(owner, roots, observations),
                })
            }
        }
    }
}
