//! L6.1: the dedicated keeper service entry.
//!
//! The keeper owns no Lisp Context. It validates the prepared era start on its
//! own OS thread, admits its local GEN1 era through the existing scoped-era
//! adapter, and runs one service body over that admitted era. The body is the
//! keeper's control for the era, including a real stop. The collector surface
//! lands in L6.5. The serial ring is L6.7. No production spawner exists yet.

use super::super::super::world::{Baton, PreparedHeapTransferProductionEra};
use super::bootstrap::{PreparedEraStart, ReturnedEraEnd};
use neovm_value::tagged::gc::heap_transfer::{
    HeapTransferCompletion, RetainedTransferFailure, ScopedEraError, ScopedEraFailure,
    TransferError,
};
#[cfg(test)]
use std::num::NonZeroU64;

use neovm_value::tagged::gc::{AllStopped, ParallelHeapEra, RendezvousError};
#[cfg(test)]
use neovm_value::tagged::gc::{HeapIdentity, MutatorKey, MutatorPermit, RegistrationError};
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
    // `stop` reads this field, so a non-test `expect(dead_code)` on it is
    // unfulfilled. The expectation stays on `stop`, which the empty body
    // does not call.
    era: &'service ParallelHeapEra<'heap, Baton>,
}

impl<'service, 'heap> KeeperEra<'service, 'heap> {
    fn new(era: &'service ParallelHeapEra<'heap, Baton>) -> Self {
        Self {
            _token: KeeperThreadToken::mint(),
            era,
        }
    }

    /// The real value-side stop requested on the keeper's own service thread.
    ///
    /// `AllStopped` proves every mutator is stopped. The keeper completes the
    /// epoch before this borrow ends; a dropped proof poisons it.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "keeper tests call the real stop; the empty L6.1 body does not"
        )
    )]
    pub(crate) fn stop(&self) -> Result<AllStopped<'_>, RendezvousError> {
        self.era.stop()
    }

    /// Heap the service thread registers a mutator permit against.
    #[cfg(test)]
    pub(crate) fn heap_identity(&self) -> HeapIdentity {
        self.era.heap_identity()
    }

    /// Register one mutator on this service thread. Activation and publication
    /// stay on the owner thread; the permit is what crosses.
    #[cfg(test)]
    pub(crate) fn register(
        &self,
        owner: u64,
        root_capacity: usize,
    ) -> Result<MutatorPermit<'_>, RegistrationError> {
        self.era.register(owner, root_capacity)
    }

    /// Queue one serial request on this era. The product ring is L6.7.
    #[cfg(test)]
    pub(crate) fn enqueue_serial_for_test(
        &self,
        owner: MutatorKey,
        generation: NonZeroU64,
    ) -> Result<u64, RendezvousError> {
        self.era.enqueue_serial_for_test(owner, generation)
    }

    /// `(ring length, head id, grant issued now)` while this stop is open.
    #[cfg(test)]
    pub(crate) fn serial_stop_priority_for_test(
        &self,
    ) -> Result<(usize, Option<u64>, Option<u64>), RendezvousError> {
        self.era.serial_stop_priority_for_test()
    }

    /// Head of the queued ring. Does not grant it.
    #[cfg(test)]
    pub(crate) fn serial_ring_head_for_test(
        &self,
    ) -> Result<(usize, Option<u64>), RendezvousError> {
        self.era.serial_ring_head_for_test()
    }

    /// Drop the probe's head request once the stop has kept it.
    #[cfg(test)]
    pub(crate) fn retire_serial_head_for_test(&self, id: u64) -> Result<(), RendezvousError> {
        self.era.retire_serial_head_for_test(id)
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

/// Run one keeper service body on this thread's admitted era.
///
/// The body owns the keeper's collector/control for the whole admitted era,
/// including a real stop and its completion. The completion bound keeps a
/// Lisp word, a collector loan, or a local borrow from leaving the era.
pub(crate) fn keeper_serve<R: HeapTransferCompletion>(
    prepared: PreparedEraStart,
    body: impl for<'service, 'heap> FnOnce(&KeeperEra<'service, 'heap>) -> R,
) -> Result<(ReturnedEraEnd, R), KeeperStartFailure> {
    let transfer = prepared.into_heap_transfer();
    let (transfer, result) = transfer
        .with_production_era(|era| {
            let keeper = KeeperEra::new(era);
            body(&keeper)
        })
        .map_err(KeeperStartFailure::Scoped)?;
    let end = ReturnedEraEnd::from_transfer(transfer).map_err(KeeperStartFailure::Return)?;
    Ok((end, result))
}

/// Admit and finish one era on this thread with an empty service body.
///
/// L6.1's entry. Stops and lends belong to [`keeper_serve`]'s body. No
/// production spawner exists yet.
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
    keeper_serve(prepared, |_| ()).map(|(end, ())| end)
}
