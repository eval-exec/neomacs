//! Dormant local observation effects through the actual exclusive collector.
//! Shared with GEN0 serial preparation; this grants no N-mutator admission.
use super::*;
use crate::tagged::collection_reads::{
    SourceObservationError, SourceObservationRestoreFailure, SourceObservationsDetached,
};

impl QuiescentCollector<'_> {
    // No product caller enters this shell. Tests are the only entry until the
    // owned source transaction is composed.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant until the owned source transaction is composed; observation tests are the caller"
        )
    )]
    #[cold]
    #[inline(never)]
    pub(crate) fn detach_source_observations(
        &mut self,
    ) -> Result<SourceObservationsDetached, SourceObservationError> {
        if tagged_heap_is_installed() {
            return Err(SourceObservationError::InstalledHeap);
        }
        let owner = self
            .heap
            ._satb_layout_reserve
            .existing_observations()
            .ok_or(SourceObservationError::MissingOwner)?;
        owner.detach_source(self.heap_identity())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant until the owned source transaction is composed; observation tests are the caller"
        )
    )]
    #[cold]
    #[inline(never)]
    pub(crate) fn validate_source_observation_export(
        &mut self,
    ) -> Result<(), SourceObservationError> {
        if tagged_heap_is_installed() {
            return Err(SourceObservationError::InstalledHeap);
        }
        let owner = self
            .heap
            ._satb_layout_reserve
            .existing_observations()
            .ok_or(SourceObservationError::MissingOwner)?;
        owner.validate_source_export()
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant until the owned source transaction is composed; observation tests are the caller"
        )
    )]
    #[cold]
    #[inline(never)]
    pub(crate) fn restore_source_observations(
        &mut self,
        receipt: SourceObservationsDetached,
    ) -> Result<(), SourceObservationRestoreFailure> {
        if tagged_heap_is_installed() {
            return Err(SourceObservationRestoreFailure {
                reason: SourceObservationError::InstalledHeap,
                receipt,
            });
        }
        let Some(owner) = self.heap._satb_layout_reserve.existing_observations() else {
            return Err(SourceObservationRestoreFailure {
                reason: SourceObservationError::MissingOwner,
                receipt,
            });
        };
        receipt.restore(self.heap_identity(), owner)
    }
}
