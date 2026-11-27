//! L6.7a: the keeper driver over the product era (a3.1).
//!
//! The protocol is [`driver_core`]. This file binds it to the era
//! ([`KeeperEra`]) and to the keeper's custody between two lends of one stop
//! ([`Between`]), and holds the owner's side: [`ChannelPort`] and its lend
//! guard. The names the rows use (`Driver`, `KeeperMsg`, `GrantMsg`,
//! `Breach`, `DriverEnd`, `LendDrop`) are this binding's instances.
#![forbid(unsafe_code)]

use super::driver_core::{
    self, Attempt, Batch, BatchVisitor, CustodyRefused, Head, KeeperOps, Refusal, Settle,
};
pub(crate) use super::driver_core::{CollectRefusal, LendDrop};
use super::keeper_custody::{
    KeeperReadyRequest, RestoredKeeper, ReturnedSerialTicket, StoppedKeeper,
};
use super::owned_source::KeeperEra;
use super::serial_park::{Grant, KeeperPort, LendGuard, PortRefusal};
use super::serial_ticket::{ReceiveReason, SerialLendHeld, StoppedSerialTicket};
use super::sync::{Arc, mpsc};
use super::world_turn::{WorldReturned, WorldTurn, WorldTurnIdentity};
use neovm_value::tagged::gc::heap_transfer::{PublishedOwner, with_stop_scope};
use neovm_value::tagged::gc::{
    AllStopped, CollectionReport, Mutator, MutatorKey, RendezvousError, RingClient, SerialGrant,
    StopAttempt, StopPending,
};
use std::num::NonZeroU64;
use std::time::Duration;

/// Owner or Baton group -> keeper.
pub(crate) type KeeperMsg = driver_core::KeeperMsg<
    PublishedOwner,
    ReturnedSerialTicket,
    StoppedSerialTicket,
    ReceiveReason,
    WorldReturned,
    CollectionReport,
>;
/// Keeper -> one owner.
pub(crate) type GrantMsg = driver_core::GrantMsg<StoppedSerialTicket>;
/// Where a collection's report goes: the caller of `garbage-collect`, or
/// nobody (allocation debt).
pub(crate) type CollectReply = Option<mpsc::SyncSender<Result<CollectionReport, CollectRefusal>>>;
/// Keeper -> the Baton group (L6.7b b3).
pub(crate) type GroupMsg = driver_core::GroupMsg<WorldTurn>;
/// The Baton group's enrollment, for its port.
pub(crate) type GroupLink = driver_core::GroupLink<WorldTurn>;
/// A proven invariant break.
pub(crate) type Breach = driver_core::Breach<ReceiveReason, RendezvousError>;
/// Why the driver stopped serving.
pub(crate) type DriverEnd = driver_core::DriverEnd<ReceiveReason, RendezvousError>;
/// The keeper side of L6.7a over the product era.
pub(crate) type Driver<'k, 'service, 'heap> =
    driver_core::Driver<'k, &'k KeeperEra<'service, 'heap>>;

impl<'k, 'service, 'heap> KeeperOps for &'k KeeperEra<'service, 'heap> {
    type Key = MutatorKey;
    type Receipt = PublishedOwner;
    type Ticket = StoppedSerialTicket;
    type Returned = ReturnedSerialTicket;
    type Reason = ReceiveReason;
    type WorldTurn = WorldTurn;
    type WorldReturned = WorldReturned;
    type Error = RendezvousError;
    type Stopped = AllStopped<'k>;
    type Pending = StopPending;
    type Report = CollectionReport;

    fn refusal(error: RendezvousError) -> Refusal {
        match error {
            RendezvousError::Busy => Refusal::Busy,
            // `PublishedOwner::validate_current`: a newer publication
            // superseded the receipt.
            RendezvousError::Generation => Refusal::Superseded,
            _ => Refusal::Other,
        }
    }

    fn owner(key: MutatorKey) -> u64 {
        key.owner()
    }

    fn generation(receipt: &PublishedOwner) -> NonZeroU64 {
        let (_, generation, _) = receipt.serial_facts();
        generation
    }

    fn enqueue(self, key: MutatorKey, generation: NonZeroU64) -> Result<(), RendezvousError> {
        self.enqueue_serial(key, generation)
    }

    fn cancel(self, key: MutatorKey) -> Result<(), RendezvousError> {
        self.cancel_serial(key)
    }

    fn enqueue_group(self) -> Result<(), RendezvousError> {
        self.enqueue_baton_group()
    }

    fn cancel_group(self) -> Result<(), RendezvousError> {
        self.cancel_baton_group()
    }

    fn requeue_group_at_head(self) -> Result<(), RendezvousError> {
        self.requeue_baton_group_at_head()
    }

    fn requeue_owner_at_head(
        self,
        key: MutatorKey,
        generation: NonZeroU64,
    ) -> Result<(), RendezvousError> {
        self.requeue_serial_at_head(key, generation)
    }

    fn stop_within(
        self,
        bound: Duration,
    ) -> Result<Attempt<AllStopped<'k>, StopPending>, RendezvousError> {
        Ok(match KeeperEra::stop_within(self, bound)? {
            StopAttempt::Stopped(stopped) => Attempt::Stopped(stopped),
            StopAttempt::Pending(pending) => Attempt::Pending(pending),
        })
    }

    fn gate_holder(self) -> Option<NonZeroU64> {
        KeeperEra::gate_holder(self)
    }

    fn poison(self) {
        let _ = self.poison_era();
    }

    fn epoch(stopped: &AllStopped<'k>) -> NonZeroU64 {
        stopped.epoch()
    }

    fn trace(stopped: &mut AllStopped<'k>) -> Result<(), RendezvousError> {
        stopped.trace_serial()
    }

    fn should_collect(self, stopped: &mut AllStopped<'k>) -> Result<bool, RendezvousError> {
        self.should_collect_stopped(stopped)
    }

    fn collect_stopped(
        self,
        stopped: &mut AllStopped<'k>,
    ) -> Result<CollectionReport, RendezvousError> {
        self.collect_requested(stopped)
    }

    fn complete_empty(stopped: AllStopped<'k>) -> Result<(), RendezvousError> {
        stopped.complete().map_err(|failure| failure.reason)
    }

    fn ring_len(stopped: &AllStopped<'k>) -> Result<usize, RendezvousError> {
        stopped.serial_ring_len()
    }

    fn with_batch<V: BatchVisitor<Self>>(
        self,
        stopped: AllStopped<'k>,
        visitor: V,
    ) -> Result<V::Output, Breach> {
        with_stop_scope(|scope| {
            let keeper = StoppedKeeper::from_scope(scope, stopped)
                .map_err(|(_scope, _stopped, reason)| Breach::Rendezvous(reason))?;
            visitor.visit(Between::Fresh(keeper))
        })
    }
}

/// The keeper between two lends of one stop: nothing is out.
enum Between<'s, 'era> {
    Fresh(StoppedKeeper<'s, 'era>),
    Restored(RestoredKeeper<'s, 'era>),
}

// A `Batch` lends once because `lend`/`lend_world` move it; the
// custody_batch_* compile-fail fixtures pin that only for a batch that is not
// Copy, so the product batch must stay neither Copy nor Clone (lead #14111).
static_assertions::assert_not_impl_any!(Between<'static, 'static>: Copy, Clone);

/// The keeper while the Baton group's World turn is out: the stop stays
/// here (nothing is exported), with the identity of the one turn it cut.
struct WorldLendHeld<'s, 'era> {
    keeper: Between<'s, 'era>,
    cut: WorldTurnIdentity,
}

impl<'s, 'k, 'service, 'heap> Batch<&'k KeeperEra<'service, 'heap>> for Between<'s, 'k> {
    type Ready = KeeperReadyRequest<'s>;
    type Grant = SerialGrant;
    type Held = SerialLendHeld<'s, 'k>;
    type WorldHeld = WorldLendHeld<'s, 'k>;

    fn head(&self) -> Result<Option<Head<MutatorKey>>, RendezvousError> {
        let head = match self {
            Between::Fresh(keeper) => keeper.serial_head()?,
            Between::Restored(keeper) => keeper.serial_head()?,
        };
        Ok(head.map(|client| match client {
            RingClient::Owner { key, .. } => Head::Owner(key),
            RingClient::BatonGroup => Head::BatonGroup,
        }))
    }

    fn ready(&self, receipt: PublishedOwner) -> Result<KeeperReadyRequest<'s>, RendezvousError> {
        match self {
            Between::Fresh(keeper) => keeper.ready_request(receipt),
            Between::Restored(keeper) => keeper.ready_request(receipt),
        }
    }

    fn grant_for(
        &mut self,
        ready: &KeeperReadyRequest<'s>,
    ) -> Result<Option<SerialGrant>, RendezvousError> {
        match self {
            Between::Fresh(keeper) => keeper.grant_for(ready),
            Between::Restored(keeper) => keeper.grant_for(ready),
        }
    }

    fn grant_group(&mut self) -> Result<Option<SerialGrant>, RendezvousError> {
        match self {
            Between::Fresh(keeper) => keeper.grant_group(),
            Between::Restored(keeper) => keeper.grant_group(),
        }
    }

    fn lend(
        self,
        ready: KeeperReadyRequest<'s>,
    ) -> Result<(StoppedSerialTicket, SerialLendHeld<'s, 'k>), CustodyRefused> {
        let lent = match self {
            Between::Fresh(keeper) => keeper.lend_serial(ready),
            Between::Restored(keeper) => keeper.lend_next(ready),
        };
        lent.map(StoppedSerialTicket::from_lend).map_err(|failure| {
            // The failure still holds the stop; finishing it here keeps the
            // epoch from being dropped twice. The era is poisoned anyway.
            let _ = failure.complete_traced();
            CustodyRefused
        })
    }

    fn settle(
        held: SerialLendHeld<'s, 'k>,
        returned: ReturnedSerialTicket,
        grant: SerialGrant,
    ) -> Result<Self, Settle<RendezvousError>> {
        let mut restored = held
            .accept_return(returned)
            .map_err(|_| Settle::NotTheTicketCut)?;
        restored.retire(grant).map_err(Settle::Retire)?;
        Ok(Between::Restored(restored))
    }

    fn lend_world(mut self) -> Result<(WorldTurn, WorldLendHeld<'s, 'k>), CustodyRefused> {
        let (turn, cut) = match &mut self {
            Between::Fresh(keeper) => (keeper.cut_world_turn(), keeper.world_identity()),
            Between::Restored(keeper) => (keeper.cut_world_turn(), keeper.world_identity()),
        };
        Ok((turn, WorldLendHeld { keeper: self, cut }))
    }

    fn settle_world(
        held: WorldLendHeld<'s, 'k>,
        returned: WorldReturned,
        grant: SerialGrant,
    ) -> Result<Self, Settle<RendezvousError>> {
        if returned.identity() != held.cut {
            // `held` drops the stop with the breach the driver reports.
            return Err(Settle::NotTheTicketCut);
        }
        let mut keeper = held.keeper;
        match &mut keeper {
            Between::Fresh(stopped) => stopped.retire(grant),
            Between::Restored(restored) => restored.retire(grant),
        }
        .map_err(Settle::Retire)?;
        Ok(keeper)
    }

    fn complete(self) -> Result<(), RendezvousError> {
        match self {
            Between::Fresh(keeper) => keeper.complete_traced(),
            Between::Restored(keeper) => keeper.complete_traced(),
        }
        .map_err(|failure| failure.reason)
    }
}

/// A contended gate, for the owner's own retries.
fn busy(error: RendezvousError) -> bool {
    error == RendezvousError::Busy
}

/// The owner's guard over one lent ticket (K7). Armed from the grant until
/// the ticket goes back.
pub(crate) struct ChannelLend {
    owner: u64,
    inbox: mpsc::Sender<KeeperMsg>,
    armed: bool,
}

impl LendGuard for ChannelLend {
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for ChannelLend {
    fn drop(&mut self) {
        if self.armed {
            let cause = if std::thread::panicking() {
                LendDrop::Unwound
            } else {
                LendDrop::Leaked
            };
            let _ = self.inbox.send(KeeperMsg::Abandon {
                owner: self.owner,
                cause,
            });
        }
    }
}

/// The product [`KeeperPort`]: the owner's era mutator, the keeper's inbox,
/// and this owner's grant channel. Each request publishes again: a receipt is
/// single-use, and the request is committed under the new publication.
pub(crate) struct ChannelPort<'era> {
    owner: u64,
    mutator: Mutator<'era>,
    native: bool,
    exited: bool,
    inbox: mpsc::Sender<KeeperMsg>,
    grants: mpsc::Receiver<GrantMsg>,
    publishing_owners: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

struct PublishingGuard<'a> {
    counter: Option<&'a std::sync::atomic::AtomicUsize>,
}

impl<'a> PublishingGuard<'a> {
    fn enter(counter: Option<&'a std::sync::Arc<std::sync::atomic::AtomicUsize>>) -> Self {
        if let Some(counter) = counter {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Self {
                counter: Some(&**counter),
            }
        } else {
            Self { counter: None }
        }
    }
}

impl Drop for PublishingGuard<'_> {
    fn drop(&mut self) {
        if let Some(counter) = self.counter {
            counter.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// How many times one request republishes before it refuses with `Retry`.
const REPUBLISH_LIMIT: usize = 2;

impl<'era> ChannelPort<'era> {
    pub(crate) fn new(
        owner: u64,
        mutator: Mutator<'era>,
        inbox: mpsc::Sender<KeeperMsg>,
        grants: mpsc::Receiver<GrantMsg>,
    ) -> Self {
        Self {
            owner,
            mutator,
            native: false,
            exited: false,
            inbox,
            grants,
            publishing_owners: None,
        }
    }

    pub(crate) fn with_publishing_owners(
        mut self,
        publishing_owners: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        self.publishing_owners = Some(publishing_owners);
        self
    }

    /// The owner is done: tell the keeper and hand the mutator back for its
    /// retire.
    pub(crate) fn exit(mut self) -> Mutator<'era> {
        if !self.exited {
            self.exited = true;
            let _ = self.inbox.send(KeeperMsg::Exit { owner: self.owner });
        }
        self.mutator
    }

    /// Publish and stay NativeStopped without asking for a turn, so a stop
    /// does not wait on this owner while it waits for something else.
    pub(crate) fn publish_idle(&mut self) -> Result<(), PortRefusal> {
        self.publish().map(drop)
    }

    /// Test-only helper: publish and stay `NativeStopped` while returning the
    /// `PublishedOwner` receipt so a test can later enqueue `KeeperMsg::Request`
    /// while a keeper stop is already held (exercising `await_world`'s live
    /// `RingWaiter` raise during an active Baton-group interval).
    #[cfg(test)]
    pub(crate) fn publish_idle_receipt_for_test(&mut self) -> Result<PublishedOwner, PortRefusal> {
        self.publish()
    }

    /// Test-only helper: send `KeeperMsg::Request` using a pre-published
    /// `PublishedOwner` receipt (falling back to normal `publish()` if the
    /// keeper asks for `Republish`), then wait for `GrantMsg::Grant`.
    #[cfg(test)]
    pub(crate) fn request_turn_with_receipt_for_test(
        &mut self,
        initial_receipt: PublishedOwner,
    ) -> Result<(Grant, ChannelLend), PortRefusal> {
        let mut pending = Some(initial_receipt);
        for _ in 0..=REPUBLISH_LIMIT {
            let receipt = match pending.take() {
                Some(receipt) => receipt,
                None => self.publish()?,
            };
            let (_, generation, _) = receipt.serial_facts();
            if self
                .inbox
                .send(KeeperMsg::Request {
                    owner: self.owner,
                    receipt,
                })
                .is_err()
            {
                return Err(PortRefusal::Poisoned);
            }
            match self.grants.recv() {
                Ok(GrantMsg::Grant { ticket, epoch }) => {
                    return Ok((
                        Grant {
                            ticket,
                            generation,
                            epoch,
                        },
                        ChannelLend {
                            owner: self.owner,
                            inbox: self.inbox.clone(),
                            armed: true,
                        },
                    ));
                }
                Ok(GrantMsg::Republish) => continue,
                Ok(GrantMsg::Poisoned) | Err(mpsc::RecvError) => {
                    return Err(PortRefusal::Poisoned);
                }
            }
        }
        Err(PortRefusal::Retry)
    }

    /// Map a `RendezvousError` from `leave_native` / `publish_native_owner`
    /// into a `PortRefusal`: only `Busy` is transient (`Retry`), `Poisoned`
    /// maps to `Poisoned`, and every permanent non-`Poisoned` error maps to
    /// `Failed` so `resume_ticket_for_collect` fails closed instead of
    /// spinning while `Driver::head_hold` stands (L6.7c (c) F2).
    pub(crate) fn map_publish_error(error: RendezvousError) -> PortRefusal {
        match error {
            RendezvousError::Poisoned => PortRefusal::Poisoned,
            RendezvousError::Busy => PortRefusal::Retry,
            _ => PortRefusal::Failed,
        }
    }

    /// Publish again: leave native first if the last request entered it.
    fn publish(&mut self) -> Result<PublishedOwner, PortRefusal> {
        let mutator = &mut self.mutator;
        let inbox = &self.inbox;
        if self.native {
            // Wake the keeper BEFORE calling `leave_native()` (E6-P.4):
            // `leave_native()` blocks inside `prepare_restore()`'s condvar
            // while a Baton-group `WorldTurn` stop is open rather than
            // returning `Busy`, so without an upfront `Wake` to raise
            // `ring_waiter`, a Baton thread polling `(thread-yield)` would
            // never release its stop and `leave_native()` would deadlock.
            let _ = inbox.send(KeeperMsg::Wake);
            match driver_core::retry_contended(busy, || {
                let res = mutator.leave_native().map(|_| ());
                if matches!(res, Err(RendezvousError::Busy)) {
                    let _ = inbox.send(KeeperMsg::Wake);
                }
                res
            }) {
                Ok(()) => {}
                Err(err) => return Err(Self::map_publish_error(err)),
            }
            self.native = false;
        }
        match driver_core::retry_contended(busy, || {
            let res = mutator.publish_native_owner(&[]);
            if matches!(res, Err(RendezvousError::Busy)) {
                let _ = inbox.send(KeeperMsg::Wake);
            }
            res
        }) {
            Ok(receipt) => {
                self.native = true;
                Ok(receipt)
            }
            Err(err) => Err(Self::map_publish_error(err)),
        }
    }
}

impl KeeperPort for ChannelPort<'_> {
    type LendGuard = ChannelLend;
    type CollectReply = CollectReply;

    fn give_back(&mut self, returned: ReturnedSerialTicket) {
        let _ = self.inbox.send(KeeperMsg::GiveBack {
            owner: self.owner,
            returned,
            end: driver_core::IntervalEnd::Done,
        });
    }

    fn give_back_to_collect(&mut self, returned: ReturnedSerialTicket, reply: CollectReply) {
        let _ = self.inbox.send(KeeperMsg::GiveBack {
            owner: self.owner,
            returned,
            end: driver_core::IntervalEnd::Collect { reply },
        });
    }

    fn request_turn(&mut self) -> Result<(Grant, ChannelLend), PortRefusal> {
        for _ in 0..=REPUBLISH_LIMIT {
            let (generation, sent) = {
                // Dekker pair with `Driver::serve_group`: increment
                // `publishing_owners` (SeqCst) BEFORE `publish()` sends
                // `KeeperMsg::Wake` and enters `leave_native()`, and hold it
                // until `KeeperMsg::Request` is enqueued in `inbox`.
                let counter = self.publishing_owners.clone();
                let _publishing = PublishingGuard::enter(counter.as_ref());
                let receipt = self.publish()?;
                let (_, generation, _) = receipt.serial_facts();
                let sent = self
                    .inbox
                    .send(KeeperMsg::Request {
                        owner: self.owner,
                        receipt,
                    })
                    .is_ok();
                (generation, sent)
            };
            if !sent {
                return Err(PortRefusal::Poisoned);
            }
            match self.grants.recv() {
                Ok(GrantMsg::Grant { ticket, epoch }) => {
                    return Ok((
                        Grant {
                            ticket,
                            generation,
                            epoch,
                        },
                        ChannelLend {
                            owner: self.owner,
                            inbox: self.inbox.clone(),
                            armed: true,
                        },
                    ));
                }
                Ok(GrantMsg::Republish) => continue,
                Ok(GrantMsg::Poisoned) | Err(mpsc::RecvError) => {
                    return Err(PortRefusal::Poisoned);
                }
            }
        }
        Err(PortRefusal::Retry)
    }

    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason) {
        let _ = self.inbox.send(KeeperMsg::Refuse {
            owner: self.owner,
            ticket,
            reason,
        });
    }

    fn abandon_head_hold(&mut self) {
        if !self.exited {
            self.exited = true;
            let _ = self.inbox.send(KeeperMsg::Exit { owner: self.owner });
        }
    }

    fn collect_root_epoch(&self) -> Option<neovm_value::tagged::gc::FacadeEpochRetention> {
        self.mutator.parallel_epoch_retention().ok()
    }

    fn idle_status_for_deadlock(
        &self,
        bound: Duration,
    ) -> Result<driver_core::IdleStatus, PortRefusal> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.inbox
            .send(KeeperMsg::IdleQuery { reply })
            .map_err(|_| PortRefusal::Poisoned)?;
        match answer.recv_timeout(bound) {
            Ok(status) => Ok(status),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(driver_core::IdleStatus::Busy),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(PortRefusal::Poisoned),
        }
    }
}

/// The Baton group's guard over its World turn (K7 for the group). Armed
/// from the lend until the turn goes back. `Send`: it travels with the turn
/// between Baton threads, so an unwind on any of them reports the lost turn
/// instead of leaving the keeper waiting.
#[derive(Debug)]
#[must_use = "give the World turn back through the group port"]
pub(crate) struct WorldLend {
    inbox: mpsc::Sender<KeeperMsg>,
    armed: bool,
}

static_assertions::assert_impl_all!(WorldLend: Send);

impl WorldLend {
    #[cfg(test)]
    pub(crate) fn for_test(inbox: mpsc::Sender<KeeperMsg>) -> Self {
        Self { inbox, armed: true }
    }

    /// End the group's interval from whichever Baton OS thread currently holds
    /// the turn: disarms the abandon guard and sends `WorldReturn`.
    pub(crate) fn give_back(mut self, returned: WorldReturned) {
        self.armed = false;
        let _ = self.inbox.send(KeeperMsg::WorldReturn {
            returned,
            end: driver_core::IntervalEnd::Done,
        });
    }
}

impl Drop for WorldLend {
    fn drop(&mut self) {
        if self.armed {
            let cause = if std::thread::panicking() {
                LendDrop::Unwound
            } else {
                LendDrop::Leaked
            };
            let _ = self.inbox.send(KeeperMsg::GroupAbandon { cause });
        }
    }
}

/// Non-blocking Baton-group request handle (L6.7b r1 §3, QD3).
///
/// Sends `KeeperMsg::BatonRequest` at the ring TAIL, visibly distinct from
/// `GroupPort::collect` (b5's synchronous-GC ring HEAD re-insert).
#[derive(Clone, Debug)]
pub(crate) struct GroupRequestSender {
    inbox: mpsc::Sender<KeeperMsg>,
}

impl GroupRequestSender {
    /// Enqueue the group's coalesced request at the ring tail (QD3).
    pub(crate) fn request_tail(&self) -> Result<(), PortRefusal> {
        self.inbox
            .send(KeeperMsg::BatonRequest)
            .map_err(|_| PortRefusal::Poisoned)
    }

    /// D8 / QD2 ring-idleness query (`KeeperMsg::IdleQuery`): answers
    /// [`driver_core::IdleStatus::Idle`] with the keeper's `enrolled_owners`
    /// count when between stops with an empty ring and no collect/head-hold,
    /// or [`driver_core::IdleStatus::Busy`] conservatively on timeout or activity.
    pub(crate) fn idle_status_for_deadlock(
        &self,
        bound: Duration,
    ) -> Result<driver_core::IdleStatus, PortRefusal> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.inbox
            .send(KeeperMsg::IdleQuery { reply })
            .map_err(|_| PortRefusal::Poisoned)?;
        match answer.recv_timeout(bound) {
            Ok(status) => Ok(status),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(driver_core::IdleStatus::Busy),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(PortRefusal::Poisoned),
        }
    }
}

/// The Baton group's side of the keeper protocol (L6.7b b3): one request
/// for the whole group, the World turn it is lent, and the guard over it.
/// The group's runner (pentry's seat) activates the World host under the
/// turn.
pub(crate) struct GroupPort {
    inbox: mpsc::Sender<KeeperMsg>,
    world: mpsc::Receiver<GroupMsg>,
    waiter: Arc<driver_core::RingWaiter>,
}

impl std::fmt::Debug for GroupPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupPort")
            .field("ring_waiter", &self.waiter.raised())
            .finish_non_exhaustive()
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "L6.7b b4 (Green switch points) is the caller")
)]
impl GroupPort {
    /// LINK is what `Driver::enroll_group` returned for this group.
    pub(crate) fn new(inbox: mpsc::Sender<KeeperMsg>, link: GroupLink) -> Self {
        let (world, waiter) = link.into_parts();
        Self {
            inbox,
            world,
            waiter,
        }
    }

    /// Test-only constructor for synthetic channel unit tests (without a live
    /// `Driver`). Production callers use [`Self::new`] with `Driver::enroll_group()`.
    #[cfg(test)]
    pub(crate) fn from_raw_parts_for_test(
        inbox: mpsc::Sender<KeeperMsg>,
        world: mpsc::Receiver<GroupMsg>,
        waiter: Arc<driver_core::RingWaiter>,
    ) -> Self {
        Self {
            inbox,
            world,
            waiter,
        }
    }

    /// Clone the lock-free waiter word so Green switch points can read it with
    /// one Acquire load even while another thread holds `&mut GroupPort` in
    /// `await_world()`.
    pub(crate) fn waiter_handle(&self) -> Arc<driver_core::RingWaiter> {
        Arc::clone(&self.waiter)
    }

    /// Clone the non-blocking ring-tail request sender for coalesced wakes and
    /// Case-(ii) re-requests (r1 §3, QD3).
    pub(crate) fn request_sender(&self) -> GroupRequestSender {
        GroupRequestSender {
            inbox: self.inbox.clone(),
        }
    }

    /// Whether another client waits behind this group's interval (L6.7b
    /// b4.1, QB2): one Acquire load, for the Green switch points.
    pub(crate) fn ring_waiter(&self) -> bool {
        self.waiter.raised()
    }

    /// Ask for the World and wait for it. The era being poisoned, or the
    /// keeper being gone, refuses with `Poisoned`.
    pub(crate) fn request_world(
        &mut self,
    ) -> Result<(WorldTurn, NonZeroU64, WorldLend), PortRefusal> {
        self.request()?;
        self.await_world()
    }

    /// Ask for the World without waiting. The group's coalesced request
    /// (b4.3): the Baton thread that asks need not be the one that receives
    /// the World.
    pub(crate) fn request(&self) -> Result<(), PortRefusal> {
        self.inbox
            .send(KeeperMsg::BatonRequest)
            .map_err(|_| PortRefusal::Poisoned)
    }

    /// The next World turn the keeper lends this group.
    pub(crate) fn await_world(
        &mut self,
    ) -> Result<(WorldTurn, NonZeroU64, WorldLend), PortRefusal> {
        match self.world.recv() {
            Ok(driver_core::GroupMsg::World { turn, epoch }) => Ok((
                turn,
                epoch,
                WorldLend {
                    inbox: self.inbox.clone(),
                    armed: true,
                },
            )),
            Ok(driver_core::GroupMsg::Poisoned) | Err(mpsc::RecvError) => {
                Err(PortRefusal::Poisoned)
            }
        }
    }

    /// End the group's interval: the turn goes back and its guard disarms.
    pub(crate) fn give_back(&mut self, lend: WorldLend, returned: WorldReturned) {
        lend.give_back(returned);
    }

    /// A Baton thread collects (L6.7b b5): end the interval so the keeper
    /// collects first in its next stop, then wait for the World it lends
    /// this group again at the ring HEAD. No new request: the keeper keeps
    /// the group's place. REPLY, if any, gets the collection's report.
    pub(crate) fn collect(
        &mut self,
        mut lend: WorldLend,
        returned: WorldReturned,
        reply: CollectReply,
    ) -> Result<(WorldTurn, NonZeroU64, WorldLend), PortRefusal> {
        lend.armed = false;
        if self
            .inbox
            .send(KeeperMsg::WorldReturn {
                returned,
                end: driver_core::IntervalEnd::Collect { reply },
            })
            .is_err()
        {
            return Err(PortRefusal::Poisoned);
        }
        self.await_world()
    }

    /// Whether the group may report a deadlock (L6.7b b4, lead #14147 QD2):
    /// asked when no Baton thread is runnable and the group holds no World.
    /// `Ok(true)` only when the keeper, between stops, has no live parallel
    /// owner, nothing queued and no collection asked for. Conservative: any
    /// doubt answers `false`, and so does a keeper that has not answered
    /// within BOUND (lead #14206: a caller may hold scheduler state, so
    /// the wait is bounded). The era being gone refuses.
    ///
    /// On the group's port, not `KeeperEra` (ratified, lead #14206 R1):
    /// `KeeperEra` is keeper-confined, and this port is Green's handle on
    /// the keeper.
    pub(crate) fn idle_for_deadlock(&self, bound: Duration) -> Result<bool, PortRefusal> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.inbox
            .send(KeeperMsg::IdleQuery { reply })
            .map_err(|_| PortRefusal::Poisoned)?;
        match answer.recv_timeout(bound) {
            Ok(driver_core::IdleStatus::Idle { enrolled_owners: 0 }) => Ok(true),
            Ok(driver_core::IdleStatus::Idle { .. } | driver_core::IdleStatus::Busy) => Ok(false),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(false),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(PortRefusal::Poisoned),
        }
    }

    /// The group leaves the era.
    pub(crate) fn exit(self) {
        let _ = self.inbox.send(KeeperMsg::GroupExit);
    }
}

// The driver's rows live with E5.2's live rows (C3): tests/serial_park_live_test.rs.
