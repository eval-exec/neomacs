//! The single cold local census/drain/TLS-detach transaction shared by
//! scoped admission and the later consuming source-shell adapter. This is
//! still a local collector loan, never a transport or a stopped interval.

use super::super::{Baton, FrozenLegacyRoots};
use super::factory::AdmissionPlan;
use super::{AdmissionScope, Context, OwnedParallelError, map_rendezvous, map_start};
use neovm_value::tagged::gc::{QuiescentCollector, clear_tagged_heap_root_if_installed};
use std::borrow::Borrow;

#[path = "preparation/policy.rs"]
mod policy;
use policy::{ClosedSource, PreparationPolicy, PreparationResources};
#[path = "preparation/finish_source.rs"]
mod finish_source;
use finish_source::{CensusOnly, FinishSource, WithdrawObservations};

/// Resource recovery work, not a permission to reactivate a Context. The
/// consuming source adapter must separately validate its actual return right.
#[derive(Debug)]
pub(crate) enum SerialRecovery {
    Unchanged,
    RecreateDeadWindows,
}

/// Borrows the actual source Context exclusively for the collector lifetime.
/// No ordinary source access is possible while this local preparation lives.
#[must_use]
pub(super) struct PreparedAdmission<'heap, K = Baton> {
    collector: QuiescentCollector<'heap>,
    keeper: K,
    frozen: FrozenLegacyRoots,
    recovery: SerialRecovery,
}

static_assertions::assert_not_impl_any!(PreparedAdmission<'static>: Send, Sync, Clone, Copy);

impl PreparedAdmission<'_, Baton> {
    /// End the complete local factory/callback frame before returning the
    /// resource recovery plan. No failure temporary can retain a source loan
    /// while the caller reinstalls ordinary Context access.
    pub(super) fn run<P: AdmissionPlan, R>(
        self,
        plan: P,
        body: impl for<'heap> FnOnce(
            P::Projection<'heap>,
            AdmissionScope<'heap>,
        ) -> (QuiescentCollector<'heap>, Baton, R),
    ) -> (SerialRecovery, Result<R, OwnedParallelError>) {
        let Self {
            collector,
            keeper,
            frozen,
            recovery,
        } = self;
        let projection = match plan.admit(collector, keeper, frozen) {
            Ok(projection) => projection,
            Err(failure) => {
                let reason = failure.reason;
                drop(failure.collector);
                drop(failure.keeper);
                return (recovery, Err(reason));
            }
        };
        let (collector, keeper, output) =
            body(projection, AdmissionScope(std::marker::PhantomData));
        drop(collector);
        drop(keeper);
        (recovery, Ok(output))
    }
}

impl Context {
    /// Prepare an owned serial admission by draining the heap and clearing
    /// `self`'s TLS heap slot.
    ///
    /// # Safety
    ///
    /// On `Ok(prepared)`, `self`'s TLS heap slot is cleared while `self` is
    /// still alive. The caller must not re-enter, allocate, collect, or
    /// evaluate on `self` until `self`'s heap is reinstalled via
    /// [`Self::recover_serial_context`] or `self` is dropped.
    pub(super) unsafe fn prepare_owned_admission(
        &mut self,
    ) -> Result<PreparedAdmission<'_>, OwnedParallelError> {
        // SAFETY: caller upholds the dormancy/recovery contract above.
        unsafe { self.prepare_admission_with::<policy::RecoverSerial>() }
    }

    /// Closed, nonrecovering prerequisite for the consuming source adapter.
    /// This borrowed transaction does not own or suspend the source Context.
    /// The adapter must arm its owned fail-closed shell before invoking it.
    /// Dropping a failure is not permission to resume. No product caller
    /// enters this path.
    ///
    /// # Safety
    ///
    /// This policy clears `self`'s TLS heap slot without recovering it on
    /// exit. The caller must not re-enter, allocate, collect, or evaluate on
    /// `self` unless `self`'s heap is explicitly reinstalled via
    /// [`Self::recover_serial_context`] (or `self` is held inside an owned
    /// fail-closed shell / dropped).
    #[allow(dead_code)]
    unsafe fn prepare_closed_source(
        &mut self,
    ) -> Result<PreparedAdmission<'_>, policy::ClosedPreparationFailure<'_>> {
        // SAFETY: caller upholds the fail-closed dormancy contract above.
        unsafe { self.prepare_admission_with::<ClosedSource>() }
    }

    /// Local census and TLS withdrawal for a shell that already holds the keeper.
    /// The shell calls this only after its own frames have returned. The
    /// borrowed keeper is the one the shell retained: this does not issue a
    /// second baton, run Lisp preclean, or recover the source. No production
    /// caller enters it.
    ///
    /// # Safety
    ///
    /// On `Ok(...)`, `self`'s TLS heap slot is cleared without calling
    /// `recover_serial_context` or consuming `self`. The caller must hold
    /// `self` inside a fail-closed consuming shell (or never re-enter,
    /// allocate, collect, or evaluate on `self` without reinstalling its heap).
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn close_owned_source_heap_tls(
        &mut self,
        keeper: &Baton,
    ) -> Result<FrozenLegacyRoots, OwnedParallelError> {
        // SAFETY: caller upholds the fail-closed dormancy contract above.
        unsafe { self.finish_owned_source_heap::<CensusOnly>(keeper) }
    }

    /// Consume the same local collector frame through observation withdrawal.
    /// The collector and the original observation `Rc` do not leave this frame.
    /// No production caller enters it.
    ///
    /// # Safety
    ///
    /// On `Ok(...)` (or `Err(ObservationFailure::Observations)` after drain),
    /// `self`'s TLS heap slot is cleared without calling
    /// `recover_serial_context` or consuming `self`. The caller must hold
    /// `self` inside a fail-closed consuming shell (or never re-enter,
    /// allocate, collect, or evaluate on `self` without reinstalling its heap).
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn withdraw_owned_source_observations(
        &mut self,
        keeper: &Baton,
    ) -> Result<
        (
            FrozenLegacyRoots,
            neovm_value::tagged::collection_reads::SourceObservationsDetached,
        ),
        finish_source::ObservationFailure,
    > {
        // SAFETY: caller upholds the fail-closed dormancy contract above.
        unsafe { self.finish_owned_source_heap::<WithdrawObservations>(keeper) }
    }

    // Only this child module defines finish policies. No caller callback or
    // runtime switch can run inside the source collector frame.
    //
    // # Safety
    //
    // Same fail-closed dormancy contract as `prepare_admission_using`: on
    // drain completion, `self`'s TLS heap slot is cleared without reinstalling
    // it; the caller must not re-enter `self` without reinstalling its heap.
    unsafe fn finish_owned_source_heap<F: FinishSource>(
        &mut self,
        keeper: &Baton,
    ) -> Result<F::Output, F::Failure> {
        // SAFETY: caller upholds the fail-closed dormancy contract above.
        let prepared = match unsafe {
            self.prepare_admission_using::<ClosedSource, _>(SerialRecovery::Unchanged, keeper)
        } {
            Ok(prepared) => prepared,
            Err(failure) => return Err(F::preparation_failure(failure.retain_resources())),
        };
        let PreparedAdmission {
            collector,
            keeper: _,
            frozen,
            recovery: _,
        } = prepared;
        F::finish(collector, frozen)
    }

    /// The one census/drain definition. Only sealed static policies choose
    /// preclean and failure behavior. No runtime mode flag is introduced.
    ///
    /// # Safety
    ///
    /// Same dormancy/recovery contract as [`Self::prepare_admission_using`].
    unsafe fn prepare_admission_with<P: PreparationPolicy>(
        &mut self,
    ) -> Result<PreparedAdmission<'_, Baton>, P::Failure<'_, Baton>> {
        let recovery = P::begin(self);
        let keeper = match self.issue_green_baton() {
            Ok(keeper) => keeper,
            Err(error) => {
                return Err(P::fail(
                    self,
                    recovery,
                    PreparationResources::BeforeKeeper,
                    map_start(error),
                ));
            }
        };
        // SAFETY: caller upholds the dormancy/recovery contract above.
        unsafe { self.prepare_admission_using::<P, _>(recovery, keeper) }
    }

    /// Owned serial preparation passes its baton. The source shell borrows the
    /// baton it already retained. Neither route issues a second keeper.
    ///
    /// # Safety
    ///
    /// On `Ok(prepared)`, this clears `self`'s TLS heap slot while `&mut self`
    /// remains alive and does not itself call `recover_serial_context`. The
    /// caller must not re-enter, allocate, collect, or evaluate on `self`
    /// until `self`'s heap is reinstalled via [`Self::recover_serial_context`]
    /// or `self` is dropped / held inside an owned fail-closed shell.
    unsafe fn prepare_admission_using<P: PreparationPolicy, K: Borrow<Baton>>(
        &mut self,
        recovery: SerialRecovery,
        keeper: K,
    ) -> Result<PreparedAdmission<'_, K>, P::Failure<'_, K>> {
        let frozen = match self.capture_parallel_legacy_roots(keeper.borrow()) {
            Ok(frozen) => frozen,
            Err(error) => {
                return Err(P::fail(
                    self,
                    recovery,
                    PreparationResources::Keeper(keeper),
                    map_rendezvous(error),
                ));
            }
        };
        let heap = self.tagged_heap.heap_root().as_ptr();
        // SAFETY: this thread is the sole serial owner. The keeper excludes
        // concurrent mark and private roots were captured above. The legacy
        // TLS facade stays installed for this drain so its reseed still sees
        // the thread-local census; it is uninstalled before the body runs.
        // The body cannot reach the Context or the heap. Static failure policy
        // does not change this existing local collector loan boundary.
        let drained = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                self.seed_registered_mutator_roots_world_stopped(heap);
            })
        };
        let collector = match drained {
            Ok(collector) => collector,
            Err(_) => {
                return Err(P::fail(
                    self,
                    recovery,
                    PreparationResources::Census { frozen, keeper },
                    OwnedParallelError::Start,
                ));
            }
        };
        // SAFETY: caller upholds `prepare_admission_using`'s `# Safety`
        // contract (will not re-enter `self` without reinstalling its heap via
        // `recover_serial_context` or holding `self` inside a fail-closed
        // consuming shell); `drain_to_quiescent` above already quiesced the
        // heap and detached the store protocol, and no TLS-derived heap loan is
        // live here.
        unsafe { clear_tagged_heap_root_if_installed(self.tagged_heap.heap_root()) };
        Ok(PreparedAdmission {
            collector,
            keeper,
            frozen,
            recovery,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pentry_e2b_shared_preparation_ends_source_loan_before_recovery() {
        let mut source = Context::new_minimal_vm_harness(crate::test_utils::unit_runtime());
        source.setup_thread_locals();
        source.set_variable(
            "window-dead-windows-table",
            neovm_value::tagged::value::TaggedValue::NIL,
        );
        source.gc_collect_exact();
        let heap = source.tagged_heap.heap_identity();
        let generational = source.tagged_heap.tagged_heap_generational_enabled();
        // SAFETY: `source` is not re-entered while cleared; `recover_serial_context`
        // below unconditionally reinstalls its heap before further use.
        let PreparedAdmission {
            collector,
            keeper,
            frozen,
            recovery,
        } = unsafe { source.prepare_owned_admission() }.expect("actual local preparation");
        assert_eq!(collector.heap_identity(), heap);
        assert_eq!(keeper.parallel_heap_identity(), heap);
        assert_eq!(frozen.heap_identity(), heap);
        assert!(keeper.held_here());
        assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
        drop(frozen);
        drop(collector);
        drop(keeper);
        source.recover_serial_context(recovery);
        assert!(neovm_value::tagged::gc::tagged_heap_is_current(
            &source.tagged_heap
        ));
        assert_eq!(
            source.tagged_heap.tagged_heap_generational_enabled(),
            generational
        );
        let pair = neovm_value::tagged::value::TaggedValue::cons(
            neovm_value::tagged::value::TaggedValue::fixnum(47),
            neovm_value::tagged::value::TaggedValue::NIL,
        );
        assert!(source.tagged_heap.owns_heap_value_for_test(pair));
        assert_eq!(
            pair.cons_car(),
            neovm_value::tagged::value::TaggedValue::fixnum(47)
        );
    }
}
