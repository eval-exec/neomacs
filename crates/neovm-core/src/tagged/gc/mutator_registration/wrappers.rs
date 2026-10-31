//! Nominal entry boundaries over one private authority implementation.
//!
//! A product era always retains the evidence minted from the exclusive GEN1
//! collector. The test-only proof wrapper cannot convert to that evidence.
use super::super::Gen1Admission;
use super::construction::{Gen1Construction, ProductionAdmission};
#[cfg(test)]
use super::construction::{ProofAdmission, ProofConstruction};
use super::*;

/// Production multi-mutator authority, minted only from checked GEN1 inputs.
#[must_use = "finish explicitly; abandonment retains the World and poisons the era"]
pub struct ParallelHeapEra<'heap> {
    pub(super) core: EraCore<'heap>,
    witness: Gen1Construction,
}

/// Restricted proof executors retain GEN0 coverage without production admission.
#[cfg(test)]
#[must_use = "finish explicitly; abandonment retains the World and poisons the era"]
pub(crate) struct ProofEra<'heap> {
    pub(super) core: EraCore<'heap>,
    witness: ProofConstruction,
}

/// Finish refusal preserves the production admission and its exact evidence.
#[must_use]
pub(crate) struct EraFinishFailure<'heap> {
    pub(crate) reason: RegistrationError,
    pub(crate) era: ParallelHeapEra<'heap>,
}
#[cfg(test)]
#[must_use]
pub(crate) struct ProofEraFinishFailure<'heap> {
    pub(crate) reason: RegistrationError,
    pub(crate) era: ProofEra<'heap>,
}

impl<'heap> ParallelHeapEra<'heap> {
    #[cold]
    pub(crate) fn admit(
        admission: Gen1Admission<'heap>,
        keeper: Baton,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        EraCore::admit(ProductionAdmission::check(admission, keeper)?)
            .map(|(core, witness)| Self { core, witness })
    }

    #[cold]
    pub(crate) fn admit_rooted(
        admission: Gen1Admission<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        EraCore::admit_rooted(ProductionAdmission::check(admission, keeper)?, roots)
            .map(|(core, witness)| Self { core, witness })
    }
}

#[cfg(test)]
impl<'heap> ProofEra<'heap> {
    #[cold]
    pub(crate) fn admit(
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        EraCore::admit(ProofAdmission::check(collector, keeper)?)
            .map(|(core, witness)| Self { core, witness })
    }

    #[cold]
    pub(crate) fn admit_rooted(
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        EraCore::admit_rooted(ProofAdmission::check(collector, keeper)?, roots)
            .map(|(core, witness)| Self { core, witness })
    }

    #[cold]
    pub(crate) fn admit_rooted_with_weak_tables_for_test(
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
        tables: &[crate::tagged::value::TaggedValue],
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        EraCore::admit_rooted_with_weak_tables_for_test(
            ProofAdmission::check(collector, keeper)?,
            roots,
            tables,
        )
        .map(|(core, witness)| Self { core, witness })
    }

    pub(super) fn registry(&self) -> &Registry {
        self.core.registry()
    }

    pub(crate) fn with_stopped_heap_for_test<R>(
        &self,
        stopped: &AllStopped<'_>,
        body: impl for<'h> FnOnce(&'h super::super::TaggedHeap) -> R,
    ) -> Result<R, CollectionError> {
        self.core.with_stopped_heap_for_test(stopped, body)
    }

    pub(crate) fn shutdown_admission_stopped_for_test(
        &self,
        stopped: &AllStopped<'_>,
    ) -> Result<Result<(), super::super::MarkFinishError>, RendezvousError> {
        self.core.shutdown_admission_stopped_for_test(stopped)
    }

    pub(super) fn cons_catalog_snapshot_for_test(
        &self,
    ) -> Result<super::super::cons_storage::CatalogSnapshot, RegistrationError> {
        self.core.cons_catalog_snapshot_for_test()
    }

    pub(crate) fn test_root_epoch_retention(&self) -> super::super::FacadeEpochRetention {
        self.core.test_root_epoch_retention()
    }

    pub(super) fn prepare_name_function_symbol(
        &self,
        key: &SymbolHandle,
    ) -> Result<symbol_authority::PreparedNameFunction, FunctionError> {
        self.core.prepare_name_function_symbol(key)
    }
}

// These are direct forwards, not a Deref exposing the private core. The sole
// register mint receives the wrapper's sealed, nominal construction witness.
macro_rules! era_operations {
    ($era:ident, $failure:ident) => {
        impl<'heap> $era<'heap> {
            pub fn heap_identity(&self) -> HeapIdentity {
                self.core.heap_identity()
            }

            /// Exact value charged to owner 0's image by checked admission.
            /// Later table growth is intentionally not part of this receipt.
            #[cfg(test)]
            pub(crate) fn symbol_table_admission_charge_for_test(
                &self,
            ) -> super::super::symbol_table::publication::RetainedTableBytes {
                self.core.symbol_table_admission_charge
            }

            #[cold]
            pub fn register(
                &self,
                owner: u64,
                root_capacity: usize,
            ) -> Result<MutatorPermit<'_>, RegistrationError> {
                self.core.register(&self.witness, owner, root_capacity)
            }

            pub fn stopped_summaries(&self) -> Result<StoppedSummaries<'_>, RegistrationError> {
                self.core.stopped_summaries()
            }

            #[cold]
            pub fn start_cons_storage(&self) -> Result<(), RegistrationError> {
                self.core.start_cons_storage()
            }

            #[cold]
            pub fn return_cons_storage(
                &self,
                stopped: &AllStopped<'_>,
            ) -> Result<(), RegistrationError> {
                self.core.return_cons_storage(stopped)
            }

            #[cold]
            pub fn resume_cons_storage(
                &self,
                stopped: &AllStopped<'_>,
            ) -> Result<(), RegistrationError> {
                self.core.resume_cons_storage(stopped)
            }

            #[cold]
            pub fn collect_stopped(
                &self,
                stopped: &mut AllStopped<'_>,
            ) -> Result<CollectionReport, CollectionError> {
                self.core.collect_stopped(stopped)
            }

            #[cold]
            pub fn collect_stopped_minor(
                &self,
                stopped: &mut AllStopped<'_>,
            ) -> Result<CollectionReport, MinorCollectionError> {
                self.core.collect_stopped_minor(stopped)
            }

            pub fn prepare_function_symbol(&self, key: &SymbolHandle) -> Result<(), FunctionError> {
                self.core.prepare_function_symbol(key)
            }

            pub fn try_stop(&self) -> Result<Option<AllStopped<'_>>, RendezvousError> {
                self.core.try_stop()
            }

            pub fn stop(&self) -> Result<AllStopped<'_>, RendezvousError> {
                self.core.stop()
            }

            pub fn rendezvous(&self) -> Rendezvous<'_> {
                self.core.rendezvous()
            }

            pub(crate) fn finish(
                self,
            ) -> Result<(QuiescentCollector<'heap>, Baton), $failure<'heap>> {
                let Self { core, witness } = self;
                core.finish().map_err(|failure| $failure {
                    reason: failure.reason,
                    era: Self {
                        core: failure.core,
                        witness,
                    },
                })
            }
        }

        impl std::fmt::Debug for $era<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($era))
                    .field("core", &self.core)
                    .finish_non_exhaustive()
            }
        }

        impl std::fmt::Debug for $failure<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple(stringify!($failure))
                    .field(&self.reason)
                    .finish()
            }
        }
    };
}
era_operations!(ParallelHeapEra, EraFinishFailure);
#[cfg(test)]
era_operations!(ProofEra, ProofEraFinishFailure);

static_assertions::assert_not_impl_any!(ParallelHeapEra<'static>: Send, Sync, Clone, Copy);
#[cfg(test)]
static_assertions::assert_not_impl_any!(ProofEra<'static>: Send, Sync, Clone, Copy);
