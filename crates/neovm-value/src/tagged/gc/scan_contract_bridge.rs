//! Production young-log bridge for `SingleMutatorWorld`.
//!
//! Kept out of `scan_contract.rs` (which the compile-fail fixtures
//! `#[path]`-include): the fixture scratch crate does not declare the
//! `test-support` feature, so the `cfg_attr(not(any(test, feature)))`
//! dead-code expectations below would add `unexpected_cfgs` notes to their
//! stderr (CG-5 mechanism (b)).  The bridge is compiled in every build; the
//! 200-character fixture scan never sees this file.

use super::scan_contract::{SingleMutatorWorld, StoppedMutatorsProof};
use std::marker::PhantomData;

impl SingleMutatorWorld<'_> {
    /// Split the allocator and the canonical cold registry under the same
    /// closed-world proof used for SATB folding. No callbacks into Lisp or GC,
    /// allocation safepoints, or mutator resumes are allowed inside `body`.
    ///
    /// The bridge itself never drains logs or clears flags. C2/C3 use its
    /// resolver for capture, then ConsStopAll clears flags only after promotion
    /// and detaches sweep work before releasing this physical stop-all.
    #[cfg_attr(
        not(any(test, feature = "test-support")),
        expect(
            dead_code,
            reason = "Typed split bridge retained for worker integration; current collectors use with_stopped_heap."
        )
    )]
    pub(in crate::tagged::gc) fn with_stopped_young_logs<R>(
        &mut self,
        body: impl FnOnce(
            &mut super::cons_allocator::ConsAllocatorOwner,
            &mut super::young_registry::StoppedYoungLogs<'_>,
            &[super::ConsBlock],
        ) -> R,
    ) -> Result<R, super::young_registry::YoungRegistryError> {
        assert!(
            !self.heap.concurrent_mark_running(),
            "young capture requires an explicit successful marker join"
        );
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        let heap = &mut *self.heap;
        let mut logs =
            super::young_registry::StoppedYoungLogs::new(&mut heap._satb_layout_reserve, &proof)?;
        Ok(body(&mut heap.cons_allocator, &mut logs, &heap.cons_blocks))
    }

    /// Transfer only a young payload to a worker; SATB remains owner-confined.
    /// The caller must return the loan before collection or owner retirement.
    #[cfg_attr(
        not(any(test, feature = "test-support")),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(in crate::tagged::gc) fn take_young_log_loan(
        &mut self,
        id: super::young_registry::YoungRegistrationId,
    ) -> Result<super::young_registry::YoungLogLoan, super::young_registry::YoungRegistryError>
    {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap._satb_layout_reserve.take_young_loan(id, &proof)
    }

    #[cfg_attr(
        not(any(test, feature = "test-support")),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(in crate::tagged::gc) fn return_young_log_loan(
        &mut self,
        loan: super::young_registry::YoungLogLoan,
    ) -> Result<(), super::young_registry::YoungLogReturnError> {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap
            ._satb_layout_reserve
            .return_young_loan(loan, &proof)
    }

    #[cfg_attr(
        not(any(test, feature = "test-support")),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(in crate::tagged::gc) fn current_young_registration_id(
        &self,
    ) -> super::young_registry::YoungRegistrationId {
        self.heap._satb_layout_reserve.current_young_id()
    }
}
