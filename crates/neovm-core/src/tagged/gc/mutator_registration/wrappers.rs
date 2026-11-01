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
pub struct ParallelHeapEra<'heap, K: EraKeeper> {
    pub(super) core: EraCore<'heap, K>,
    witness: Gen1Construction,
}

/// Restricted proof executors retain GEN0 coverage without production admission.
#[cfg(test)]
#[must_use = "finish explicitly; abandonment retains the World and poisons the era"]
pub(crate) struct ProofEra<'heap, K: EraKeeper> {
    pub(super) core: EraCore<'heap, K>,
    witness: ProofConstruction,
}

/// Finish refusal preserves the production admission and its exact evidence.
#[must_use]
pub(crate) struct EraFinishFailure<'heap, K: EraKeeper> {
    pub(crate) reason: RegistrationError,
    pub(crate) era: ParallelHeapEra<'heap, K>,
}
#[cfg(test)]
#[must_use]
pub(crate) struct ProofEraFinishFailure<'heap, K: EraKeeper> {
    pub(crate) reason: RegistrationError,
    pub(crate) era: ProofEra<'heap, K>,
}

impl<'heap, K: EraKeeper> ParallelHeapEra<'heap, K> {
    /// # Safety
    /// prepare must return the original retained keeper's epoch and backing,
    /// without executing Lisp, resuming owners, or leaking a manager reference.
    #[cold]
    pub(crate) unsafe fn admit_with(
        admission: Gen1Admission<'heap>,
        keeper: K,
        prepare: impl FnOnce(
            &K,
        ) -> Result<
            (
                super::super::FacadeEpochRetention,
                symbol_authority::SymbolBacking,
            ),
            RegistrationError,
        >,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
        EraCore::admit(ProductionAdmission::check(admission, keeper)?, prepare)
            .map(|(core, witness)| Self { core, witness })
    }

    /// # Safety
    /// prepare must return the original retained keeper's epoch and backing,
    /// without executing Lisp, resuming owners, or leaking a manager reference.
    #[cold]
    pub(crate) unsafe fn admit_rooted_with(
        admission: Gen1Admission<'heap>,
        keeper: K,
        roots: FrozenLegacyRoots,
        prepare: impl FnOnce(
            &K,
        ) -> Result<
            (
                super::super::FacadeEpochRetention,
                symbol_authority::SymbolBacking,
            ),
            RegistrationError,
        >,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
        EraCore::admit_rooted(
            ProductionAdmission::check(admission, keeper)?,
            roots,
            prepare,
        )
        .map(|(core, witness)| Self { core, witness })
    }
}

#[cfg(test)]
impl<'heap, K: EraKeeper> ProofEra<'heap, K> {
    /// # Safety
    /// prepare must return the original retained keeper's epoch and backing,
    /// without executing Lisp, resuming owners, or leaking a manager reference.
    #[cold]
    pub(crate) unsafe fn admit_with(
        collector: QuiescentCollector<'heap>,
        keeper: K,
        prepare: impl FnOnce(
            &K,
        ) -> Result<
            (
                super::super::FacadeEpochRetention,
                symbol_authority::SymbolBacking,
            ),
            RegistrationError,
        >,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
        EraCore::admit(ProofAdmission::check(collector, keeper)?, prepare)
            .map(|(core, witness)| Self { core, witness })
    }

    /// # Safety
    /// prepare must return the original retained keeper's epoch and backing,
    /// without executing Lisp, resuming owners, or leaking a manager reference.
    #[cold]
    pub(crate) unsafe fn admit_rooted_with(
        collector: QuiescentCollector<'heap>,
        keeper: K,
        roots: FrozenLegacyRoots,
        prepare: impl FnOnce(
            &K,
        ) -> Result<
            (
                super::super::FacadeEpochRetention,
                symbol_authority::SymbolBacking,
            ),
            RegistrationError,
        >,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
        EraCore::admit_rooted(ProofAdmission::check(collector, keeper)?, roots, prepare)
            .map(|(core, witness)| Self { core, witness })
    }

    /// # Safety
    /// prepare must return the original retained keeper's epoch and backing,
    /// without executing Lisp, resuming owners, or leaking a manager reference.
    #[cold]
    pub(crate) unsafe fn admit_rooted_with_weak_tables_for_test_inner(
        collector: QuiescentCollector<'heap>,
        keeper: K,
        roots: FrozenLegacyRoots,
        tables: &[crate::tagged::value::TaggedValue],
        prepare: impl FnOnce(
            &K,
        ) -> Result<
            (
                super::super::FacadeEpochRetention,
                symbol_authority::SymbolBacking,
            ),
            RegistrationError,
        >,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
        EraCore::admit_rooted_with_weak_tables_for_test(
            ProofAdmission::check(collector, keeper)?,
            roots,
            tables,
            prepare,
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
        impl<'heap, K: EraKeeper> $era<'heap, K> {
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
            /// # Safety
            /// The upper adapter must supply the complete CURRENT World root
            /// walk of the retained keeper, including stable marker head slots.
            /// No root or reference may escape its real stopped authority.
            pub(crate) unsafe fn collect_stopped_with(
                &self,
                stopped: &mut AllStopped<'_>,
                roots: impl FnOnce(
                    &K,
                    &mut AllStopped<'_>,
                    &mut Vec<crate::tagged::value::TaggedValue>,
                ) -> Result<
                    Vec<*mut *mut crate::tagged::header::MarkerObj>,
                    RendezvousError,
                >,
            ) -> Result<CollectionReport, CollectionError> {
                self.core.collect_stopped(stopped, roots)
            }

            #[cold]
            /// # Safety
            /// Same complete CURRENT World census as collect_stopped_with.
            /// GEN0 still refuses before invoking the supplied walk.
            pub(crate) unsafe fn collect_stopped_minor_with(
                &self,
                stopped: &mut AllStopped<'_>,
                roots: impl FnOnce(
                    &K,
                    &mut AllStopped<'_>,
                    &mut Vec<crate::tagged::value::TaggedValue>,
                ) -> Result<
                    Vec<*mut *mut crate::tagged::header::MarkerObj>,
                    RendezvousError,
                >,
            ) -> Result<CollectionReport, MinorCollectionError> {
                self.core.collect_stopped_minor(stopped, roots)
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
            ) -> Result<(QuiescentCollector<'heap>, K), $failure<'heap, K>> {
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

        impl<K: EraKeeper> std::fmt::Debug for $era<'_, K> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($era))
                    .field("core", &self.core)
                    .finish_non_exhaustive()
            }
        }

        impl<K: EraKeeper> std::fmt::Debug for $failure<'_, K> {
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

#[cfg(test)]
static_assertions::assert_not_impl_any!(ParallelHeapEra<'static, super::Baton>: Send, Sync, Clone, Copy);
#[cfg(test)]
static_assertions::assert_not_impl_any!(ProofEra<'static, super::Baton>: Send, Sync, Clone, Copy);
