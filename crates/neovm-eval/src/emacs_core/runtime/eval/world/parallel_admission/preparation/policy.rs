//! Static choices for the shared cold preparation transaction. These are
//! local borrowed preparation policies, not a source ownership capability.

use super::super::super::{Baton, FrozenLegacyRoots};
use super::super::{Context, OwnedParallelError};
use super::SerialRecovery;

mod sealed {
    pub trait Sealed {}
}

/// Exactly the resources acquired before a failed preparation step. A failed
/// drain has no live collector, but its census and mark exclusion still belong
/// to this failure. Field order releases the census before its keeper.
pub(super) enum PreparationResources<K = Baton> {
    BeforeKeeper,
    Keeper(K),
    Census {
        // Retained so Drop releases the census before the keeper. The closed
        // policy wraps the enum in ManuallyDrop and does not read the fields.
        #[allow(dead_code)]
        frozen: FrozenLegacyRoots,
        #[allow(dead_code)]
        keeper: K,
    },
}

pub(super) trait PreparationPolicy: sealed::Sealed {
    type Failure<'source, K>;

    fn begin(source: &mut Context) -> SerialRecovery;
    fn fail<'source, K>(
        source: &'source mut Context,
        recovery: SerialRecovery,
        resources: PreparationResources<K>,
        reason: OwnedParallelError,
    ) -> Self::Failure<'source, K>;
}

pub(super) struct RecoverSerial;
impl sealed::Sealed for RecoverSerial {}
impl PreparationPolicy for RecoverSerial {
    type Failure<'source, K> = OwnedParallelError;

    fn begin(source: &mut Context) -> SerialRecovery {
        // This may run finalizers and post-gc-hook. It belongs exclusively to
        // the ordinary active serial continuation, before any closed source
        // effects or source-admission preflight are relied upon.
        source.retire_empty_dead_windows_table()
    }

    fn fail<'source, K>(
        source: &'source mut Context,
        recovery: SerialRecovery,
        resources: PreparationResources<K>,
        reason: OwnedParallelError,
    ) -> Self::Failure<'source, K> {
        drop(resources);
        source.recover_serial_context(recovery);
        reason
    }
}

pub(super) struct ClosedSource;
impl sealed::Sealed for ClosedSource {}
impl PreparationPolicy for ClosedSource {
    type Failure<'source, K> = ClosedPreparationFailure<'source, K>;

    fn begin(_source: &mut Context) -> SerialRecovery {
        // Never reclaim the weak table here: exact Context GC runs Lisp.
        // Any ordinary preclean must finish before final entry validation and
        // the first suspension effect. Otherwise strict era admission refuses
        // the retained table. No callback, Context repair or allocation here.
        SerialRecovery::Unchanged
    }

    fn fail<'source, K>(
        source: &'source mut Context,
        recovery: SerialRecovery,
        resources: PreparationResources<K>,
        reason: OwnedParallelError,
    ) -> Self::Failure<'source, K> {
        ClosedPreparationFailure {
            _source: source,
            _recovery: recovery,
            _resources: std::mem::ManuallyDrop::new(resources),
            reason,
        }
    }
}

/// Retains the actual exclusive source borrow and all acquired resources.
/// There is deliberately no resume/recover/conversion method. Dropping it
/// retains these local resources and never refreshes, installs or accesses
/// the Context. This is not the owned poison state: the consuming adapter
/// must separately retain the whole source shell on failure, drop, forget,
/// or panic.
#[must_use]
pub(super) struct ClosedPreparationFailure<'source, K = Baton> {
    _source: &'source mut Context,
    _recovery: SerialRecovery,
    _resources: std::mem::ManuallyDrop<PreparationResources<K>>,
    reason: OwnedParallelError,
}
static_assertions::assert_not_impl_any!(ClosedPreparationFailure<'static>: Send, Sync, Clone, Copy);

impl<K> ClosedPreparationFailure<'_, K> {
    /// End this local source borrow without recovery. Acquired resources stay
    /// retained. The caller's armed shell keeps the original Context and, when
    /// `K` is a borrow, the keeper it already holds.
    pub(super) fn retain_resources(self) -> OwnedParallelError {
        self.reason
    }
}

#[cfg(test)]
#[path = "tests/policy_tests.rs"]
mod tests;
