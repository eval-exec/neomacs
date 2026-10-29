//! Serial owned-Context admission. The body receives a projection of the
//! frozen shell census and the era. It has no Context receiver, no World
//! view, and no legacy heap TLS facade. The GNU default weak window table is
//! retired here, before `supported` runs; that fence stays strict.

use super::Context;

#[path = "parallel_admission/factory.rs"]
mod factory;
use crate::emacs_core::eval::green::GreenStartError;
use crate::emacs_core::value::{HashTableTest, HashTableWeakness, LispHashTable};
use crate::tagged::gc::{
    ParallelHeapEra, QuiescentCollector, RegistrationError, RendezvousError,
    clear_tagged_heap_root_if_installed,
};
use crate::tagged::value::TaggedValue;
use factory::{AdmissionPlan, ProductionPlan};
#[cfg(test)]
use factory::{ProofPlan, WeakProofPlan};

/// Why owned admission refused before any owner ran.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // No public NEOVM_THREADS entry calls this yet.
pub(crate) enum OwnedParallelError {
    Publication,
    Busy,
    Identity,
    Poisoned,
    Start,
    GenerationalRequired,
}

/// The frozen shell plus the era. Finish is the only way back to the Context.
#[allow(dead_code)]
#[must_use]
pub(crate) struct OwnedParallelProjection<'heap> {
    era: ParallelHeapEra<'heap>,
}

impl<'heap> OwnedParallelProjection<'heap> {
    pub(crate) fn era(&self) -> &ParallelHeapEra<'heap> {
        &self.era
    }

    pub(crate) fn finish(
        self,
    ) -> Result<(QuiescentCollector<'heap>, super::Baton), OwnedParallelError> {
        self.era.finish().map_err(|failure| {
            let reason = map_registration(failure.reason);
            drop(failure);
            reason
        })
    }
}

/// Closed proof-only projection; deliberately has no Deref or conversion to
/// the production wrapper. Finish remains the sole serial ownership return.
#[cfg(test)]
#[must_use]
pub(crate) struct OwnedProofProjection<'heap> {
    era: crate::tagged::gc::ProofEra<'heap>,
}

#[cfg(test)]
impl<'heap> OwnedProofProjection<'heap> {
    pub(crate) fn era(&self) -> &crate::tagged::gc::ProofEra<'heap> {
        &self.era
    }

    pub(crate) fn finish(
        self,
    ) -> Result<(QuiescentCollector<'heap>, super::Baton), OwnedParallelError> {
        self.era.finish().map_err(|failure| {
            let reason = map_registration(failure.reason);
            drop(failure);
            reason
        })
    }
}

impl Context {
    /// Keep private keeper minting within the Context module. Fence negatives
    /// borrow the real serial keeper for one scope; no Baton can escape it.
    #[cfg(test)]
    pub(crate) fn with_weak_sweep_keeper_for_test<R>(
        &mut self,
        body: impl FnOnce(&mut Context, &super::Baton) -> R,
    ) -> R {
        let keeper = self.issue_green_baton().expect("weak-fence fixture keeper");
        body(self, &keeper)
    }

    /// Explicit closed-fixture weak sweep admission. Production keeps its
    /// strict weak-table fence and GEN1-only capability boundary.
    #[cfg(test)]
    pub(crate) fn admit_owned_parallel_with_weak_tables_for_test<R>(
        &mut self,
        tables: &[TaggedValue],
        body: impl for<'heap> FnOnce(
            OwnedProofProjection<'heap>,
        ) -> (QuiescentCollector<'heap>, super::Baton, R),
    ) -> Result<R, OwnedParallelError> {
        self.admit_owned_with(WeakProofPlan { tables }, body)
    }

    /// Retained closed proof kernels may exercise either collector mode.
    /// This nominal route cannot produce a production era or JIT admission.
    #[cfg(test)]
    pub(crate) fn admit_owned_proof_parallel<R>(
        &mut self,
        body: impl for<'heap> FnOnce(
            OwnedProofProjection<'heap>,
        ) -> (QuiescentCollector<'heap>, super::Baton, R),
    ) -> Result<R, OwnedParallelError> {
        self.admit_owned_with(ProofPlan, body)
    }

    /// Admit this suspended Context for production owned overlap. GEN0 is
    /// refused by the actual collector capability before any owner can run.
    #[allow(dead_code)]
    pub(crate) fn admit_owned_parallel<R>(
        &mut self,
        body: impl for<'heap> FnOnce(
            OwnedParallelProjection<'heap>,
        ) -> (QuiescentCollector<'heap>, super::Baton, R),
    ) -> Result<R, OwnedParallelError> {
        self.admit_owned_with(ProductionPlan, body)
    }

    /// One preparation/recovery transaction for sealed static factories. The
    /// plan chooses a nominal construction boundary at compile time; no mode
    /// flag or heap-mode branch is carried into execution.
    fn admit_owned_with<P: AdmissionPlan, R>(
        &mut self,
        plan: P,
        body: impl for<'heap> FnOnce(
            P::Projection<'heap>,
        ) -> (QuiescentCollector<'heap>, super::Baton, R),
    ) -> Result<R, OwnedParallelError> {
        let retired = self.retire_empty_dead_windows_table();
        let keeper = match self.issue_green_baton() {
            Ok(keeper) => keeper,
            Err(error) => {
                self.recover_serial_context(retired);
                return Err(map_start(error));
            }
        };
        let frozen = match self.capture_parallel_legacy_roots(&keeper) {
            Ok(frozen) => frozen,
            Err(error) => {
                drop(keeper);
                self.recover_serial_context(retired);
                return Err(map_rendezvous(error));
            }
        };
        let heap = self.tagged_heap.heap_root().as_ptr();
        // SAFETY: this thread is the sole serial owner. The keeper excludes
        // concurrent mark and private roots were captured above. The legacy
        // TLS facade stays installed for this drain so its reseed still sees
        // the thread-local census; it is uninstalled before the body runs.
        // The body cannot reach the Context or the heap.
        let drained = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                self.seed_registered_mutator_roots_world_stopped(heap);
            })
        };
        let collector = match drained {
            Ok(collector) => collector,
            Err(_) => {
                drop(frozen);
                drop(keeper);
                self.recover_serial_context(retired);
                return Err(OwnedParallelError::Start);
            }
        };
        clear_tagged_heap_root_if_installed(self.tagged_heap.heap_root());
        let projection = match plan.admit(collector, keeper, frozen) {
            Ok(projection) => projection,
            Err(failure) => {
                let reason = failure.reason;
                drop(failure.collector);
                drop(failure.keeper);
                self.recover_serial_context(retired);
                return Err(reason);
            }
        };
        let (collector, keeper, output) = body(projection);
        drop(collector);
        drop(keeper);
        self.recover_serial_context(retired);
        Ok(output)
    }

    /// Reclaim GNU's empty `window-dead-windows-table` so the strict admission
    /// fence can run. A non-empty or differently shaped table is left in place
    /// and admission refuses it. Returns whether the original object was
    /// actually reclaimed; only then does serial recovery install a fresh
    /// empty weak-value table.
    fn retire_empty_dead_windows_table(&mut self) -> bool {
        // The canonical variable read, not the evaluator: admission must not
        // run Lisp, and GNU stores this DEFVAR_LISP as one global value.
        let Some(value) = self.variable_value_by_name("window-dead-windows-table") else {
            return false;
        };
        if !is_empty_weak_value_eql(value) {
            return false;
        }
        let saved = value;
        self.set_variable("window-dead-windows-table", TaggedValue::NIL);
        self.gc_collect_exact();
        if self.tagged_heap.retains_veclike(saved) {
            self.set_variable("window-dead-windows-table", saved);
            return false;
        }
        true
    }

    fn recover_serial_context(&mut self, retired_dead_windows: bool) {
        // The original returning continuation owns the current receiver.
        // Retire parallel function publication before any serial allocation
        // or entry; never retain or refresh a parked admission-header pointer.
        self.obarray.refresh_admission();
        self.tagged_heap.install_tagged_heap();
        if retired_dead_windows {
            self.install_empty_dead_windows_table();
        }
    }

    fn install_empty_dead_windows_table(&mut self) {
        let table = self
            .tagged_heap
            .alloc_hash_table(LispHashTable::new_with_options(
                HashTableTest::Eql,
                0,
                Some(HashTableWeakness::Value),
                1.5,
                0.8125,
            ));
        self.set_variable("window-dead-windows-table", table);
    }
}

fn is_empty_weak_value_eql(value: TaggedValue) -> bool {
    let Some(table) = value.as_hash_table() else {
        return false;
    };
    table.test == HashTableTest::Eql
        && table.weakness == Some(HashTableWeakness::Value)
        && table.data.len() == 0
        && table
            .data
            .pending_entries()
            .is_none_or(|entries| entries.is_empty())
}

fn map_registration(error: RegistrationError) -> OwnedParallelError {
    match error {
        RegistrationError::Busy => OwnedParallelError::Busy,
        RegistrationError::WrongHeap => OwnedParallelError::Identity,
        RegistrationError::Poisoned => OwnedParallelError::Poisoned,
        RegistrationError::WrongOwner
        | RegistrationError::StaleRegistration
        | RegistrationError::DuplicateOwner
        | RegistrationError::WrongThread
        | RegistrationError::Exhausted
        | RegistrationError::Capacity
        | RegistrationError::OutstandingLoan
        | RegistrationError::OpenRegion
        | RegistrationError::ResidualSatb
        | RegistrationError::Publication
        | RegistrationError::PendingRoots
        | RegistrationError::PendingYoung => OwnedParallelError::Publication,
    }
}

fn map_rendezvous(error: RendezvousError) -> OwnedParallelError {
    match error {
        RendezvousError::Busy => OwnedParallelError::Busy,
        RendezvousError::Identity => OwnedParallelError::Identity,
        RendezvousError::Poisoned => OwnedParallelError::Poisoned,
        RendezvousError::Registration(error) => map_registration(error),
        RendezvousError::Loan
        | RendezvousError::Publication
        | RendezvousError::Generation
        | RendezvousError::Readers
        | RendezvousError::Capacity
        | RendezvousError::Root => OwnedParallelError::Publication,
    }
}

fn map_start(error: GreenStartError) -> OwnedParallelError {
    match error {
        GreenStartError::Collector(_) | GreenStartError::Admission(_) => OwnedParallelError::Start,
        GreenStartError::MarkExclusion(_) => OwnedParallelError::Busy,
        GreenStartError::NotGreen
        | GreenStartError::ThreadAffineHost
        | GreenStartError::GcInhibited => OwnedParallelError::Start,
    }
}

#[cfg(test)]
mod heapborrow_tests {
    use super::*;

    #[test]
    fn heapborrow_r4_serial_recovery_preserves_tls_root() {
        let mut context = Context::new_minimal_vm_harness();
        // Exercise the production installer and the immediate view-derived
        // allocation that used to invalidate its borrow-derived TLS pointer.
        context.recover_serial_context(true);
        let value = TaggedValue::cons(TaggedValue::fixnum(41), TaggedValue::NIL);
        assert_eq!(value.cons_car(), TaggedValue::fixnum(41));
        assert!(
            context
                .variable_value_by_name("window-dead-windows-table")
                .is_some()
        );
    }
}
