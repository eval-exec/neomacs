//! Nominal entry boundaries over one private authority implementation.
//!
//! A product era always retains the evidence minted from the exclusive GEN1
//! collector. The test-only proof wrapper cannot convert to that evidence.
use super::super::Gen1Admission;
use super::construction::{Gen1Construction, ProductionAdmission};
#[cfg(any(test, feature = "test-support"))]
use super::construction::{ProofAdmission, ProofConstruction};
use super::*;
use crate::tagged::gc::mutator_registration::{
    EraCoreExecutionExt, EraCoreHofExt, EraCoreUnaryExt,
};

/// Production multi-mutator authority, minted only from checked GEN1 inputs.
#[must_use = "finish explicitly; abandonment retains the World and poisons the era"]
pub struct ParallelHeapEra<'heap, K: EraKeeper> {
    pub(super) core: EraCore<'heap, K>,
    witness: Gen1Construction,
}

/// Restricted proof executors retain GEN0 coverage without production admission.
///
/// PEER 10:1xZ Option B': item-level `#[doc(hidden)] pub` inside the private
/// `mutator_registration -> wrappers` chain. The `tagged::gc` re-exports stay
/// `pub(crate)`, so `neovm_value::test_support::authority` is the sole public
/// path; `neovm_core::tagged::gc::ProofEra` keeps its E0432 contract.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[must_use = "finish explicitly; abandonment retains the World and poisons the era"]
pub struct ProofEra<'heap, K: EraKeeper> {
    pub(super) core: EraCore<'heap, K>,
    witness: ProofConstruction,
}

/// Finish refusal preserves the production admission and its exact evidence.
#[must_use]
#[doc(hidden)]
pub struct EraFinishFailure<'heap, K: EraKeeper> {
    pub(crate) reason: RegistrationError,
    pub(crate) era: ParallelHeapEra<'heap, K>,
}
#[cfg(any(test, feature = "test-support"))]
#[must_use]
#[doc(hidden)]
pub struct ProofEraFinishFailure<'heap, K: EraKeeper> {
    pub(crate) reason: RegistrationError,
    pub(crate) era: ProofEra<'heap, K>,
}

impl<'heap, K: EraKeeper> EraFinishFailure<'heap, K> {
    /// SEAL: observe the finish refusal reason (Copy).
    #[inline]
    #[doc(hidden)]
    pub fn reason(&self) -> RegistrationError {
        self.reason
    }
}

#[cfg(any(test, feature = "test-support"))]
impl<'heap, K: EraKeeper> ProofEraFinishFailure<'heap, K> {
    /// SEAL: observe the finish refusal reason (Copy).
    #[inline]
    #[doc(hidden)]
    pub fn reason(&self) -> RegistrationError {
        self.reason
    }

    /// C5 owned-instance move: give the retained proof era back to the test
    /// that owns this refusal. No new authority: the era was already this
    /// failure's, and the caller must still reconcile and finish it
    /// explicitly, exactly as the in-crate fixture does.
    #[inline]
    #[doc(hidden)]
    pub fn into_era(self) -> ProofEra<'heap, K> {
        self.era
    }

    /// SEAL: copied retained-authority facts (C4 copy; the backing objects
    /// never escape). Exactly the three `is_some` facts the lifecycle
    /// refusal tests read, so no keeper/collector/registry handle crosses.
    #[inline]
    #[doc(hidden)]
    pub fn retained_for_test(&self) -> RetainedAuthority {
        RetainedAuthority {
            keeper: self.era.core.keeper.is_some(),
            collector: self.era.core.collector.borrow().is_some(),
            registry: self.era.core.registry.is_some(),
        }
    }

    /// Copied fact for the re-homed leaked-summary-guard test (C4 VIS): the
    /// era registry gate is still held, so `try_lock` would report
    /// `WouldBlock`. The guard and the registry never cross.
    #[inline]
    #[doc(hidden)]
    pub fn registry_gate_held_for_test(&self) -> bool {
        matches!(
            self.era.registry().gate.state.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        )
    }
}

/// Copied retained-authority facts of a finish refusal (peer-signed 18:40Z
/// shape (c)): plain Copy bools only.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct RetainedAuthority {
    pub keeper: bool,
    pub collector: bool,
    pub registry: bool,
}

impl<'heap, K: EraKeeper> ParallelHeapEra<'heap, K> {
    /// Only a production GEN1 era can authorize bounded parallel execution.
    /// The test-only proof wrapper intentionally has no matching operation.
    pub fn execution_admission(&self) -> ParallelExecutionAdmission<'_> {
        self.core.execution_admission(&self.witness)
    }

    /// # Safety
    /// prepare must return the original retained keeper's epoch and backing,
    /// without executing Lisp, resuming owners, or leaking a manager reference.
    #[cold]
    #[doc(hidden)]
    pub unsafe fn admit_with(
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
    #[doc(hidden)]
    pub unsafe fn admit_rooted_with(
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

#[cfg(any(test, feature = "test-support"))]
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
    #[doc(hidden)]
    pub unsafe fn admit_rooted_with_weak_tables_for_test_inner(
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

    #[doc(hidden)]
    pub fn registry(&self) -> &Registry {
        self.core.registry()
    }

    /// Same park word `Registry::poison` publishes. The flag stays private.
    #[doc(hidden)]
    pub fn registry_poisoned_for_test(&self) -> bool {
        self.registry()
            .park
            .poisoned
            .load(std::sync::atomic::Ordering::Acquire)
    }

    #[doc(hidden)]
    pub fn with_stopped_heap_for_test<R>(
        &self,
        stopped: &AllStopped<'_>,
        body: impl for<'h> FnOnce(&'h super::super::TaggedHeap) -> R,
    ) -> Result<R, CollectionError> {
        self.core.with_stopped_heap_for_test(stopped, body)
    }

    /// Era follow-up row (peer 10:5xZ item 3): read-only keeper accessor for
    /// the re-homed fixtures. No `core` field widening.
    #[doc(hidden)]
    pub fn keeper_for_test(&self) -> Option<&K> {
        self.core.keeper.as_ref()
    }

    /// Copy the frozen legacy root words. The `Option` stays private.
    #[doc(hidden)]
    pub fn append_legacy_root_words_for_test(
        &self,
        out: &mut Vec<crate::tagged::value::TaggedValue>,
    ) {
        self.core
            .legacy_roots
            .as_ref()
            .expect("legacy roots")
            .append_words(out);
    }

    /// Copied registration keys. The registry gate stays private.
    #[doc(hidden)]
    pub fn registered_mutator_keys_for_test(
        &self,
    ) -> Result<Vec<super::MutatorKey>, RegistrationError> {
        let gate = self.registry().lock()?;
        Ok(gate.entries.iter().map(|entry| entry.key()).collect())
    }

    /// Era follow-up row (peer 10:5xZ item 1): the erased-root major
    /// collection entry, forwarded with the original signature. The caller
    /// supplies the complete World root walk, exactly as `EraCore` requires.
    #[doc(hidden)]
    pub fn collect_stopped(
        &self,
        stopped: &mut AllStopped<'_>,
        roots: impl FnOnce(
            &K,
            &mut AllStopped<'_>,
            &mut Vec<crate::tagged::value::TaggedValue>,
        )
            -> Result<Vec<*mut *mut crate::tagged::header::MarkerObj>, RendezvousError>,
    ) -> Result<CollectionReport, CollectionError> {
        self.core.collect_stopped(stopped, roots)
    }

    /// Era follow-up row (peer 10:5xZ item 1): the rooted minor collection
    /// entry, forwarded with the original signature.
    #[doc(hidden)]
    pub fn collect_stopped_minor(
        &self,
        stopped: &mut AllStopped<'_>,
        roots: impl FnOnce(
            &K,
            &mut AllStopped<'_>,
            &mut Vec<crate::tagged::value::TaggedValue>,
        )
            -> Result<Vec<*mut *mut crate::tagged::header::MarkerObj>, RendezvousError>,
    ) -> Result<CollectionReport, MinorCollectionError> {
        self.core.collect_stopped_minor(stopped, roots)
    }

    pub(crate) fn shutdown_admission_stopped_for_test(
        &self,
        stopped: &AllStopped<'_>,
    ) -> Result<Result<(), super::super::MarkFinishError>, RendezvousError> {
        self.core.shutdown_admission_stopped_for_test(stopped)
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn cons_catalog_snapshot_for_test(
        &self,
    ) -> Result<super::super::cons_storage::CatalogSnapshot, RegistrationError> {
        self.core.cons_catalog_snapshot_for_test()
    }

    pub(crate) fn test_root_epoch_retention(&self) -> super::super::FacadeEpochRetention {
        self.core.test_root_epoch_retention()
    }

    #[cfg(any(test, feature = "test-support"))]
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
            #[cfg(any(test, feature = "test-support"))]
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
            #[doc(hidden)]
            pub unsafe fn collect_stopped_with(
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
            #[doc(hidden)]
            pub unsafe fn collect_stopped_minor_with(
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

            pub fn reconcile_functions(
                &self,
                symbols: &mut KeeperSymbols<'_>,
            ) -> Result<(), FunctionError> {
                self.core.reconcile_functions(symbols)
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

            #[doc(hidden)]
            pub fn finish(self) -> Result<(QuiescentCollector<'heap>, K), $failure<'heap, K>> {
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
#[cfg(any(test, feature = "test-support"))]
era_operations!(ProofEra, ProofEraFinishFailure);

// The two preparation entries stay crate-private on the production era and
// become item-level `#[doc(hidden)] pub` on the proof era only (peer 10:5xZ
// era follow-up rows). The proof era's methods are reachable only through
// `neovm_value::test_support::authority::ProofEra`; the production era keeps
// its pre-existing `pub(crate)` surface.
impl<'heap, K: EraKeeper> ParallelHeapEra<'heap, K> {
    pub(crate) fn prepare_hof_builtin(
        &self,
        operation: super::super::function_write_hooks::HofOperation,
        key: &SymbolHandle,
    ) -> Result<symbol_authority::PreparedHofBuiltin, FunctionError> {
        self.core.prepare_hof_builtin(operation, key)
    }

    pub(crate) fn prepare_unary_function_symbol(
        &self,
        key: &SymbolHandle,
    ) -> Result<symbol_authority::PreparedUnaryFunction, FunctionError> {
        self.core.prepare_unary_function_symbol(key)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl<'heap, K: EraKeeper> ProofEra<'heap, K> {
    #[doc(hidden)]
    pub fn prepare_hof_builtin(
        &self,
        operation: super::super::function_write_hooks::HofOperation,
        key: &SymbolHandle,
    ) -> Result<symbol_authority::PreparedHofBuiltin, FunctionError> {
        self.core.prepare_hof_builtin(operation, key)
    }

    #[doc(hidden)]
    pub fn prepare_unary_function_symbol(
        &self,
        key: &SymbolHandle,
    ) -> Result<symbol_authority::PreparedUnaryFunction, FunctionError> {
        self.core.prepare_unary_function_symbol(key)
    }
}
