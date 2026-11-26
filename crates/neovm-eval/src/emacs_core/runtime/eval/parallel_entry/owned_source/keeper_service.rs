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
use std::num::NonZeroU64;

use neovm_value::tagged::gc::{
    AllStopped, CollectionError, CollectionReport, MutatorKey, ParallelHeapEra, RendezvousError,
    StopAttempt,
};
#[cfg(test)]
use neovm_value::tagged::gc::{HeapIdentity, MutatorPermit, RegistrationError};
use std::marker::PhantomData;
use std::rc::Rc;
use std::thread::ThreadId;
use std::time::Duration;

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

    // The L6.7 driver's methods below carry no `expect(dead_code)`: rustc
    // counts what an item under that expectation calls as used, so the
    // driver's own expectation keeps them live in non-test builds.

    /// L6.7 K8: one stop attempt bounded by BUDGET. A pending attempt leaves
    /// the request standing; the next attempt continues the same epoch.
    pub(crate) fn stop_within(&self, budget: Duration) -> Result<StopAttempt<'_>, RendezvousError> {
        self.era.stop_within(budget)
    }

    /// The registry gate's current holder, for the driver's contention
    /// warning.
    pub(crate) fn gate_holder(&self) -> Option<NonZeroU64> {
        self.era.gate_holder()
    }

    /// L6.7 K4: commit one serial request at the ring's tail.
    pub(crate) fn enqueue_serial(
        &self,
        owner: MutatorKey,
        generation: NonZeroU64,
    ) -> Result<(), RendezvousError> {
        self.era.enqueue_serial(owner, generation)
    }

    /// L6.7 K6: cancel `owner`'s queued request.
    pub(crate) fn cancel_serial(&self, owner: MutatorKey) -> Result<(), RendezvousError> {
        self.era.cancel_serial(owner)
    }

    /// L6.7b b3: commit the Baton group's one request at the ring's tail.
    pub(crate) fn enqueue_baton_group(&self) -> Result<(), RendezvousError> {
        self.era.enqueue_baton_group()
    }

    /// L6.7b b5: put the Baton group's request back at the ring's HEAD.
    pub(crate) fn requeue_baton_group_at_head(&self) -> Result<(), RendezvousError> {
        self.era.requeue_baton_group_at_head()
    }

    /// L6.7c (c): commit a collecting owner's next request at the ring's
    /// HEAD.
    pub(crate) fn requeue_serial_at_head(
        &self,
        owner: MutatorKey,
        generation: NonZeroU64,
    ) -> Result<(), RendezvousError> {
        self.era.requeue_serial_at_head(owner, generation)
    }

    /// L6.7b b3: cancel the Baton group's queued request.
    pub(crate) fn cancel_baton_group(&self) -> Result<(), RendezvousError> {
        self.era.cancel_baton_group()
    }

    /// L6.7 K7: poison the era on a proven invariant break.
    pub(crate) fn poison_era(&self) -> Result<(), RendezvousError> {
        self.era.poison_era()
    }

    /// L6.7 K5: whether aggregate stopped allocation debt meets the heap's GC
    /// threshold (`published + keeper bytes_since_gc >= gc_threshold`).
    pub(crate) fn should_collect_stopped(
        &self,
        stopped: &mut AllStopped<'_>,
    ) -> Result<bool, RendezvousError> {
        self.era.should_collect_stopped(stopped)
    }

    /// L6.7 K5: run one stopped major collection on `stopped`, supplying the
    /// sole canonical Baton World-root walk so callers stay safe.
    pub(crate) fn collect_requested(
        &self,
        stopped: &mut AllStopped<'_>,
    ) -> Result<CollectionReport, CollectionError> {
        // SAFETY: `self.era` is bound to `Baton` and
        // `Baton::collect_parallel_world_roots` is the canonical complete
        // CURRENT World root walk for this stopped era.
        unsafe {
            self.era
                .collect_requested(stopped, Baton::collect_parallel_world_roots)
        }
    }

    #[cfg(test)]
    pub(crate) fn charge_serial_bytes_for_test(
        &self,
        stopped: &AllStopped<'_>,
        bytes: usize,
    ) -> Result<(), CollectionError> {
        self.era.charge_serial_bytes_for_test(stopped, bytes)
    }

    #[cfg(test)]
    pub(crate) fn set_gc_threshold_for_test(
        &self,
        stopped: &AllStopped<'_>,
        threshold: usize,
    ) -> Result<(), CollectionError> {
        self.era.set_gc_threshold_for_test(stopped, threshold)
    }

    #[cfg(test)]
    pub(crate) fn gc_threshold_for_test(
        &self,
        stopped: &AllStopped<'_>,
    ) -> Result<usize, CollectionError> {
        self.era.gc_threshold_for_test(stopped)
    }

    #[cfg(test)]
    pub(crate) fn gc_collections_for_test(
        &self,
        stopped: &AllStopped<'_>,
    ) -> Result<usize, CollectionError> {
        self.era.gc_collections_for_test(stopped)
    }

    #[cfg(test)]
    pub(crate) fn keeper_baton_for_test(&self) -> &Baton {
        self.era
            .keeper_for_test()
            .expect("keeper era retains its Baton")
    }

    #[cfg(test)]
    pub(crate) fn owns_heap_word_for_test(
        &self,
        stopped: &AllStopped<'_>,
        proof: neovm_value::tagged::gc::HeapWord,
    ) -> Result<bool, CollectionError> {
        self.era.owns_heap_word_for_test(stopped, proof)
    }

    #[cfg(test)]
    pub(crate) fn grant_closed_owner_cons_lease_for_test(
        &self,
        stopped: &AllStopped<'_>,
        key: MutatorKey,
    ) -> Result<(), CollectionError> {
        self.era
            .grant_closed_owner_cons_lease_for_test(stopped, key)
    }

    #[cfg(test)]
    pub(crate) fn poison_cons_allocator_for_test(
        &self,
        stopped: &AllStopped<'_>,
    ) -> Result<(), CollectionError> {
        self.era.poison_cons_allocator_for_test(stopped)
    }

    #[cfg(test)]
    pub(crate) fn has_owner_cons_reserves_for_test(
        &self,
        stopped: &AllStopped<'_>,
    ) -> Result<bool, CollectionError> {
        self.era.has_owner_cons_reserves_for_test(stopped)
    }

    #[cfg(test)]
    pub(crate) fn retired_owner_cons_len_for_test(&self) -> usize {
        self.era.retired_owner_cons_len_for_test()
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

    /// Wake parked owners with `Poisoned`. A panicking keeper drop guard
    /// calls this so `thread::scope` joins instead of waiting out the
    /// nextest kill. A dropped `AllStopped` publishes the same word.
    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        self.era.poison_for_test();
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
