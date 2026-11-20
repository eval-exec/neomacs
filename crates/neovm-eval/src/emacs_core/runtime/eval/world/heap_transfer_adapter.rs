//! Eval makes the value transfer's source-shell precondition true.
//!
//! Nothing on the production admission path calls these. The value
//! constructor still takes no `Context`. The source callback that uses
//! `prepare_reference_free_transfer` must return that right before any
//! destination frame runs.

use super::{Baton, ParallelHeapEraWorldExt};
use neovm_value::tagged::gc::heap_transfer::{
    HeapTransferCompletion, PendingHeapReturn, PrepareFailure, PreparedHeapTransfer,
    RestoredHeapBacking, ScopedEraError, ScopedEraFailure, SourceRecoveryFailure,
};
use neovm_value::tagged::gc::{
    FrozenLegacyRoots, Gen1Admission, WorldLoan, clear_tagged_heap_root_if_installed,
    tagged_heap_is_installed,
};
use neovm_value::tagged::transport::RootBatchLease;
use std::ptr::NonNull;

/// Uninstall the ordinary heap slot for this provenance root. The collector
/// loan, when one exists, stays with the caller.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "dormant until a later slice admits through the reference-free transfer; the heap-transfer tests are the caller"
    )
)]
pub(crate) fn detach_source_heap(root: NonNull<neovm_value::tagged::gc::TaggedHeap>) {
    clear_tagged_heap_root_if_installed(root);
}

/// # Safety
/// `keeper` is the inaccessible sole serial owner of `admission`'s heap.
/// Every host, projection, and manager view has ended, and ordinary heap TLS
/// for that allocation is uninstalled. `roots` is that shell's fresh census.
/// The returned right must leave every source frame that still names the
/// collector or the era before a destination uses it.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "dormant until a later slice admits through the reference-free transfer; the heap-transfer tests are the caller"
    )
)]
pub(crate) unsafe fn prepare_reference_free_transfer<'heap>(
    mut admission: Gen1Admission<'heap>,
    keeper: Baton,
    roots: FrozenLegacyRoots,
) -> Result<(PendingHeapReturn, PreparedHeapTransfer), PrepareFailure<'heap>> {
    debug_assert!(
        !tagged_heap_is_installed(),
        "source heap TLS must be detached before the reference-free constructor"
    );
    let owner = WorldLoan::from(keeper);
    let observations = match admission.detach_source_observations() {
        Ok(observations) => observations,
        Err(reason) => {
            return Err(PrepareFailure::from_observation_detach(
                reason, admission, owner, roots,
            ));
        }
    };
    // SAFETY: the caller established the constructor's source-shell
    // precondition, including the ended collector frame once this result is
    // returned out of that frame. The observation receipt was detached after
    // those frames and stays out of the Send transfer.
    unsafe { PreparedHeapTransfer::prepare_detached(admission, owner, roots, observations) }
}

/// # Safety
/// `restored` was produced on this source thread from the matching receipt.
/// No other heap or World loan is live. The caller retires `publication`
/// only after the recovered census is rooted in the restored shell.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "dormant until a later slice admits through the reference-free transfer; the heap-transfer tests are the caller"
    )
)]
pub(crate) unsafe fn recover_transferred_source(
    restored: RestoredHeapBacking,
) -> Result<(Baton, FrozenLegacyRoots, RootBatchLease), SourceRecoveryFailure> {
    // SAFETY: the caller is the original source adapter, as required by
    // `into_source_parts`.
    let (owner, heap, words, functions, publication) = unsafe { restored.into_source_parts() }?;
    // SAFETY: `words` and `functions` are the census captured for this heap
    // and pinned by `publication` for the whole reference-free interval.
    // This file lives under eval/world, which is the owner allowed to mint
    // the frozen census.
    let roots = unsafe { FrozenLegacyRoots::new(heap, words, functions) };
    let baton = Baton::from_world_loan(owner);
    Ok((baton, roots, publication))
}

/// Admit the original keeper through the existing World era seam.
///
/// Value keeps the reference-free shell and does not name `Baton`. This is
/// the same split as [`ParallelHeapEraWorldExt`]: the era type is value-layer
/// data, and admitting it needs this World's keeper.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L4 dormant production era admission; no production caller enters it"
    )
)]
pub(crate) trait PreparedHeapTransferProductionEra: Sized {
    fn with_production_era<R: HeapTransferCompletion>(
        self,
        body: impl for<'heap> FnOnce(&neovm_value::tagged::gc::ParallelHeapEra<'heap, Baton>) -> R,
    ) -> Result<(Self, R), ScopedEraFailure>;
}

impl PreparedHeapTransferProductionEra for PreparedHeapTransfer {
    #[cold]
    fn with_production_era<R: HeapTransferCompletion>(
        self,
        body: impl for<'heap> FnOnce(&neovm_value::tagged::gc::ParallelHeapEra<'heap, Baton>) -> R,
    ) -> Result<(Self, R), ScopedEraFailure> {
        self.with_scoped_owner(|admission, owner, roots| {
            let keeper = Baton::from_world_loan(owner);
            // The trait method derives this keeper's retained epoch and
            // symbol backing, and the roots are the sealed census this
            // transfer published.
            let era = match ParallelHeapEraWorldExt::admit_rooted(admission, keeper, roots) {
                Ok(era) => era,
                Err(failure) => {
                    let (reason, collector, keeper) = failure.into_parts();
                    drop(collector);
                    return Err((
                        ScopedEraError::Admission(reason),
                        Some(neovm_value::tagged::gc::WorldLoan::from(keeper)),
                    ));
                }
            };
            let result = body(&era);
            match era.finish() {
                Ok((collector, baton)) => {
                    drop(collector);
                    Ok((neovm_value::tagged::gc::WorldLoan::from(baton), result))
                }
                Err(failure) => {
                    let reason = failure.reason();
                    // Dropping the era poisons its registry and retains the
                    // keeper. The loan must not also be returned.
                    drop(failure);
                    Err((ScopedEraError::Finish(reason), None))
                }
            }
        })
    }
}
