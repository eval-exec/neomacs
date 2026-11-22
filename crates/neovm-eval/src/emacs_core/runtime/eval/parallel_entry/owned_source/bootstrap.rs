//! The actual boxed-source split. Only the existing heap-transfer capability
//! travels. Every native, `Rc`, and compiler payload stays on its OS thread.

use super::super::super::world::Baton;
use super::{
    Effect, FailedStep, Keeper, OwnedSourceCompilerClosed, RetainedSourceFailure, SourceFrame,
    SourceWithdrawalFailure, SourceWithdrawalStep,
};
use neovm_value::tagged::collection_reads::SourceObservationsDetached;
use neovm_value::tagged::gc::FrozenLegacyRoots;
use neovm_value::tagged::gc::heap_transfer::{
    PendingHeapReturn, PreparedHeapTransfer, RestoreFailure, RestoredHeapBacking,
    RetainedTransferFailure, ReturnedHeapTransfer,
};
use std::mem::ManuallyDrop;

struct KeeperExported;

trait SplitKeeper {
    type Local: 'static;
    fn split_keeper(self) -> (Self::Local, Baton);
}

impl SplitKeeper for Keeper {
    type Local = KeeperExported;
    fn split_keeper(self) -> (Self::Local, Baton) {
        (KeeperExported, ManuallyDrop::into_inner(self.0))
    }
}

impl<R: SplitKeeper, T: 'static> SplitKeeper for Effect<R, T> {
    type Local = Effect<R::Local, T>;
    fn split_keeper(self) -> (Self::Local, Baton) {
        let (previous, keeper) = self._previous.split_keeper();
        (
            Effect {
                _previous: previous,
                _receipt: self._receipt,
            },
            keeper,
        )
    }
}

struct ObservationExported;

type LocalObservation =
    Effect<<super::ObservationEntry as SplitKeeper>::Local, ObservationExported>;
type LocalCarriers = Effect<LocalObservation, super::super::super::green::SourceWorldCarriers>;
type LocalCache = Effect<LocalCarriers, super::super::source_jit_cache::SourceJitCache>;
type LocalBackend = Effect<LocalCache, super::super::source_jit_backend::SourceJitBackend>;
type LocalCompiler = Effect<LocalBackend, super::super::source_jit_compiler::SourceJitCompiler>;

/// Local-only actual source and its native receipts.
///
/// [`PendingHeapReturn`]'s drop poisons an abandoned prepared right. The
/// Context and the receipts do not restore themselves.
#[must_use]
pub(crate) struct SuspendedMain {
    frame: SourceFrame<LocalCompiler>,
    pending: PendingHeapReturn,
}

/// The transfer already owns the unique baton, the retained allocation, and
/// the rooted opaque census. No collector, era, slab, Context, or native
/// receipt travels with it.
#[must_use]
pub(crate) struct PreparedEraStart {
    transfer: PreparedHeapTransfer,
}

/// Reference-free return. No source activation is justified until it matches
/// the original suspended main.
#[must_use]
pub(crate) struct ReturnedEraEnd {
    transfer: ReturnedHeapTransfer,
}

#[must_use]
pub(crate) struct ReturnedMainBacking {
    frame: SourceFrame<LocalCompiler>,
    backing: RestoredHeapBacking,
}

// Tests move both fields out. The dormant lib build has no reader, so name
// them here without taking the suspended Context.
#[cfg(not(test))]
impl Drop for ReturnedMainBacking {
    fn drop(&mut self) {
        let _retained = (&self.frame, &self.backing);
    }
}

#[must_use]
pub(crate) struct RetainedMainReturn {
    _frame: SourceFrame<LocalCompiler>,
    failure: ManuallyDrop<RestoreFailure>,
}

impl Drop for RetainedMainReturn {
    fn drop(&mut self) {
        // Keep the local shell. Taking it would drop the suspended Context.
        let _retained = &self._frame;
    }
}

impl std::fmt::Debug for RetainedMainReturn {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("RetainedMainReturn")
            .field(&self.failure.reason())
            .finish()
    }
}

static_assertions::assert_impl_all!(PreparedEraStart: Send);
static_assertions::assert_not_impl_any!(PreparedEraStart: Sync, Clone, Copy);
static_assertions::assert_impl_all!(ReturnedEraEnd: Send);
static_assertions::assert_not_impl_any!(ReturnedEraEnd: Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SuspendedMain: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ReturnedMainBacking: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RetainedMainReturn: Send, Sync, Clone, Copy);

const _: fn() = || {
    fn transport<T: Send + 'static>() {}
    transport::<PreparedEraStart>();
    transport::<ReturnedEraEnd>();
};

impl OwnedSourceCompilerClosed {
    /// Consume exactly this concrete closed source chain.
    ///
    /// No Context or heap field is read after the split. The resulting
    /// reference-free right leaves this frame before a caller can send it.
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant source era preparation; no production caller enters it"
        )
    )]
    pub(crate) fn prepare_era(
        self,
    ) -> Result<(SuspendedMain, PreparedEraStart), SourceWithdrawalFailure> {
        let SourceFrame { source, effects } = self._frame;
        let Effect {
            _previous: backend_chain,
            _receipt: compiler,
        } = effects;
        let Effect {
            _previous: cache_chain,
            _receipt: backend,
        } = backend_chain;
        let Effect {
            _previous: carriers_chain,
            _receipt: cache,
        } = cache_chain;
        let Effect {
            _previous: observation_chain,
            _receipt: carriers,
        } = carriers_chain;
        let Effect {
            _previous: prefix,
            _receipt: observation,
        } = observation_chain;
        let (prefix, keeper) = prefix.split_keeper();
        let (roots, observations): (FrozenLegacyRoots, SourceObservationsDetached) =
            ManuallyDrop::into_inner(observation);
        // The local shell is concrete before the first fallible hook. It is
        // not an erased successful state.
        let frame: SourceFrame<LocalCompiler> = SourceFrame {
            source,
            effects: Effect {
                _previous: Effect {
                    _previous: Effect {
                        _previous: Effect {
                            _previous: Effect {
                                _previous: prefix,
                                _receipt: ManuallyDrop::new(ObservationExported),
                            },
                            _receipt: carriers,
                        },
                        _receipt: cache,
                    },
                    _receipt: backend,
                },
                _receipt: compiler,
            },
        };
        #[cfg(test)]
        super::super::super::lifecycle::record_source_shell_trace(
            SourceWithdrawalStep::HeapTransfer.trace_label(),
        );
        // SAFETY: the original unsafe capture consumed the actual sole source
        // after every execution, collector, and store argument frame had
        // returned. This chain then took the final census and withdrew
        // observation ownership, and the later steps were cold native and TLS
        // effects with no heap mutation. The Context and host and JIT backing
        // stay in `frame` on success, error, and unwind. The original keeper,
        // census, and receipt move once. They are not rebuilt from identity
        // words. This frame returns before `PreparedEraStart` can be used or
        // sent.
        let prepared = unsafe {
            PreparedHeapTransfer::prepare_closed_source(keeper.into(), roots, observations)
        };
        match prepared {
            Ok((pending, transfer)) => Ok((
                SuspendedMain { frame, pending },
                PreparedEraStart { transfer },
            )),
            Err(error) => Err(SourceWithdrawalFailure::Retained(RetainedSourceFailure {
                step: SourceWithdrawalStep::HeapTransfer,
                _state: Box::new(FailedStep {
                    _source: frame,
                    _error: ManuallyDrop::new(error),
                }),
            })),
        }
    }
}

impl ReturnedEraEnd {
    /// Build the reference-free end from a finished scoped-era transfer.
    ///
    /// The keeper service returns the transfer only after its local era,
    /// collector and callback frames have ended.
    pub(crate) fn from_transfer(
        transfer: PreparedHeapTransfer,
    ) -> Result<Self, RetainedTransferFailure> {
        transfer
            .return_to_source()
            .map(|transfer| Self { transfer })
    }
}

impl PreparedEraStart {
    /// Hand the affine transfer to a later scoped-era adapter.
    ///
    /// The transfer retains its backing. This does not expose a collector or
    /// a manager view.
    pub(crate) fn into_heap_transfer(self) -> PreparedHeapTransfer {
        self.transfer
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant era return has no production caller yet"
        )
    )]
    pub(crate) fn return_to_main(self) -> Result<ReturnedEraEnd, RetainedTransferFailure> {
        self.transfer
            .return_to_source()
            .map(|transfer| ReturnedEraEnd { transfer })
    }
}

impl SuspendedMain {
    #[cold]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant suspended main has no production caller yet"
        )
    )]
    pub(crate) fn accept_return(
        self,
        end: ReturnedEraEnd,
    ) -> Result<ReturnedMainBacking, RetainedMainReturn> {
        let Self { frame, pending } = self;
        match pending.restore(end.transfer) {
            Ok(backing) => Ok(ReturnedMainBacking { frame, backing }),
            Err(failure) => Err(RetainedMainReturn {
                _frame: frame,
                failure: ManuallyDrop::new(failure),
            }),
        }
    }
}

// No Context accessor and no serial recovery on these intermediates.
// Typed effect restoration is a later transaction.

#[cfg(test)]
#[path = "../tests/bootstrap_test.rs"]
mod tests;
