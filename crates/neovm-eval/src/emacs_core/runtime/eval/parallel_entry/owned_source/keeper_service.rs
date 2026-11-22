//! L6.1: the dedicated keeper service entry.
//!
//! The keeper owns no Lisp Context. It validates the prepared era start on its
//! own OS thread, admits its local GEN1 era through the existing scoped-era
//! adapter, runs the keeper-side service step, and returns the reference-free
//! end to main. L6.1 has no stop/queue/lend loop yet; L6.2/L6.7 replace the
//! body with those transitions.

use super::super::super::world::{Baton, PreparedHeapTransferProductionEra};
use super::bootstrap::{PreparedEraStart, ReturnedEraEnd};
use neovm_value::tagged::gc::ParallelHeapEra;
use neovm_value::tagged::gc::heap_transfer::{
    RetainedTransferFailure, ScopedEraError, ScopedEraFailure, TransferError,
};
use std::marker::PhantomData;
use std::rc::Rc;
use std::thread::ThreadId;

/// Private proof that the current thread is the keeper's service thread.
#[must_use]
pub(crate) struct KeeperThreadToken {
    // Recorded for L6.2 diagnostics; the proof itself is minting on the
    // service thread plus `!Send`.
    _thread: ThreadId,
    _local: PhantomData<Rc<()>>,
}

impl KeeperThreadToken {
    fn mint() -> Self {
        Self {
            _thread: std::thread::current().id(),
            _local: PhantomData,
        }
    }
}

static_assertions::assert_not_impl_any!(KeeperThreadToken: Send, Sync, Clone, Copy);

/// The keeper-local era state for one service step. It borrows the era that
/// `with_production_era` admits and finishes around the body, so it never
/// crosses the thread or outlives that era.
pub(crate) struct KeeperEra<'service, 'heap> {
    _token: KeeperThreadToken,
    _era: &'service ParallelHeapEra<'heap, Baton>,
}

impl<'service, 'heap> KeeperEra<'service, 'heap> {
    fn new(era: &'service ParallelHeapEra<'heap, Baton>) -> Self {
        Self {
            _token: KeeperThreadToken::mint(),
            _era: era,
        }
    }
}

static_assertions::assert_not_impl_any!(KeeperEra<'static, 'static>: Send, Sync, Clone, Copy);

/// Why the keeper service could not start or finish. The confined failure
/// stays on the service thread; only [`KeeperStartNotice`] crosses to main.
#[must_use]
pub(crate) enum KeeperStartFailure {
    Scoped(ScopedEraFailure),
    Return(RetainedTransferFailure),
}

impl KeeperStartFailure {
    /// Send metadata for main; this is not a recovery capability.
    pub(crate) fn notice(&self) -> KeeperStartNotice {
        match self {
            Self::Scoped(failure) => KeeperStartNotice::Scoped(failure.reason()),
            Self::Return(failure) => KeeperStartNotice::Return(failure.reason()),
        }
    }
}

impl std::fmt::Debug for KeeperStartFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeeperStartFailure")
            .field("notice", &self.notice())
            .finish()
    }
}

static_assertions::assert_not_impl_any!(KeeperStartFailure: Send, Sync, Clone, Copy);

/// Send metadata about a refused keeper start. It is not a recovery capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeeperStartNotice {
    Scoped(ScopedEraError),
    Return(TransferError),
}

static_assertions::assert_impl_all!(KeeperStartNotice: Send);

/// Run the keeper service on the calling (dedicated service) OS thread.
///
/// L6.1 admits and finishes one local era; L6.2/L6.7 replace the body with the
/// stop/queue/lend loop. No production spawner exists yet.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "L6.1 keeper service has no production spawner; the keeper tests are the caller"
    )
)]
pub(crate) fn keeper_start(
    prepared: PreparedEraStart,
) -> Result<ReturnedEraEnd, KeeperStartFailure> {
    let transfer = prepared.into_heap_transfer();
    let (transfer, ()) = transfer
        .with_production_era(|era| {
            // The token is minted on the calling service thread and dropped
            // before the era finishes, so no keeper state outlives it. The
            // service-thread identity itself is asserted by the keeper test.
            let _keeper = KeeperEra::new(era);
        })
        .map_err(KeeperStartFailure::Scoped)?;
    ReturnedEraEnd::from_transfer(transfer).map_err(KeeperStartFailure::Return)
}
