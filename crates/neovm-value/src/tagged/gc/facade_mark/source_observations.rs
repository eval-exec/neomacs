//! Dormant local observation effects through the actual exclusive collector.
//! Shared with GEN0 serial preparation; this grants no N-mutator admission.
use super::*;
use crate::tagged::collection_reads::{
    SourceObservationError, SourceObservationRestoreFailure, SourceObservationsDetached,
};

impl QuiescentCollector<'_> {
    /// Withdraw this heap's observation owner through the exclusive collector.
    /// Eval's dormant source shell calls this after its own frames return, then
    /// drops the collector before the receipt leaves that frame. Transfer
    /// admission uses the same body. No production caller enters it.
    #[doc(hidden)]
    #[cold]
    #[inline(never)]
    pub fn detach_source_observations(
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
        not(any(test, feature = "test-support")),
        expect(
            dead_code,
            reason = "export revalidation is the transfer test's census; production restore uses the receipt"
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

    /// Displace the detached placeholder by installing this heap's canonical
    /// observation owner. Transfer tests use this to refuse a stale export.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn install_canonical_source_observations(&mut self) {
        self.heap
            ._satb_layout_reserve
            .observations()
            .install(BarrierWindow::NONE);
    }

    /// Retain an extra strong owner of the canonical observation Rc. Transfer
    /// tests use this to refuse an escaped owner. The clone is forgotten.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn leak_canonical_source_observation_owner(&mut self) {
        std::mem::forget(self.heap._satb_layout_reserve.observations().clone());
    }
}

impl TaggedHeap {
    // Called only through the actual retained exclusive heap loan, or after
    // all callback and collector argument frames ended under the local scope.
    #[cold]
    pub(in crate::tagged::gc) fn validate_source_observation_receipt(
        &self,
        receipt: &SourceObservationsDetached,
    ) -> Result<(), SourceObservationError> {
        let owner = self
            ._satb_layout_reserve
            .existing_observations()
            .ok_or(SourceObservationError::MissingOwner)?;
        receipt.validate_interval(self.heap_identity(), owner)
    }

    #[cold]
    pub(in crate::tagged::gc) fn preflight_source_store_retirement(
        &self,
    ) -> Result<(), SourceObservationError> {
        let owner = self
            ._satb_layout_reserve
            .existing_observations()
            .ok_or(SourceObservationError::MissingOwner)?;
        owner.preflight_source_store_retirement()
    }

    #[cold]
    pub(in crate::tagged::gc) fn retire_source_observation_membership(
        &self,
    ) -> Result<(), SourceObservationError> {
        let owner = self
            ._satb_layout_reserve
            .existing_observations()
            .ok_or(SourceObservationError::MissingOwner)?;
        owner.retire_source_membership(self.heap_identity())
    }
}
