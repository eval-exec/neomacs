//! L6.7a: the keeper driver over the product era (a3.1).
//!
//! The protocol is [`driver_core`]. This file binds it to the era
//! ([`KeeperEra`]) and to the keeper's custody between two lends of one stop
//! ([`Between`]), and holds the owner's side: [`ChannelPort`] and its lend
//! guard. The names the rows use (`Driver`, `KeeperMsg`, `GrantMsg`,
//! `Breach`, `DriverEnd`, `LendDrop`) are this binding's instances.
#![forbid(unsafe_code)]

pub(crate) use super::driver_core::LendDrop;
use super::driver_core::{
    self, Attempt, Batch, BatchVisitor, CustodyRefused, KeeperOps, Refusal, Settle,
};
use super::keeper_custody::{
    KeeperReadyRequest, RestoredKeeper, ReturnedSerialTicket, StoppedKeeper,
};
use super::owned_source::KeeperEra;
use super::serial_park::{Grant, KeeperPort, LendGuard, PortRefusal};
use super::serial_ticket::{ReceiveReason, SerialLendHeld, StoppedSerialTicket};
use super::sync::mpsc;
use neovm_value::tagged::gc::heap_transfer::{PublishedOwner, with_stop_scope};
use neovm_value::tagged::gc::{
    AllStopped, Mutator, MutatorKey, RendezvousError, SerialGrant, StopAttempt, StopPending,
};
use std::num::NonZeroU64;
use std::time::Duration;

/// Owner -> keeper.
pub(crate) type KeeperMsg = driver_core::KeeperMsg<
    PublishedOwner,
    ReturnedSerialTicket,
    StoppedSerialTicket,
    ReceiveReason,
>;
/// Keeper -> one owner.
pub(crate) type GrantMsg = driver_core::GrantMsg<StoppedSerialTicket>;
/// A proven invariant break.
pub(crate) type Breach = driver_core::Breach<ReceiveReason, RendezvousError>;
/// Why the driver stopped serving.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "no production spawner until E6-P admission")
)]
pub(crate) type DriverEnd = driver_core::DriverEnd<ReceiveReason, RendezvousError>;
/// The keeper side of L6.7a over the product era.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "no production spawner until E6-P admission")
)]
pub(crate) type Driver<'k, 'service, 'heap> =
    driver_core::Driver<'k, &'k KeeperEra<'service, 'heap>>;

impl<'k, 'service, 'heap> KeeperOps for &'k KeeperEra<'service, 'heap> {
    type Key = MutatorKey;
    type Receipt = PublishedOwner;
    type Ticket = StoppedSerialTicket;
    type Returned = ReturnedSerialTicket;
    type Reason = ReceiveReason;
    type Error = RendezvousError;
    type Stopped = AllStopped<'k>;
    type Pending = StopPending;

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

impl<'s, 'k, 'service, 'heap> Batch<&'k KeeperEra<'service, 'heap>> for Between<'s, 'k> {
    type Ready = KeeperReadyRequest<'s>;
    type Grant = SerialGrant;
    type Held = SerialLendHeld<'s, 'k>;

    fn head(&self) -> Result<Option<MutatorKey>, RendezvousError> {
        let head = match self {
            Between::Fresh(keeper) => keeper.serial_head()?,
            Between::Restored(keeper) => keeper.serial_head()?,
        };
        Ok(head.map(|(key, _generation)| key))
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
    inbox: mpsc::Sender<KeeperMsg>,
    grants: mpsc::Receiver<GrantMsg>,
}

/// How many times one request republishes before it refuses with `Retry`.
const REPUBLISH_LIMIT: usize = 2;

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "no production spawner until E6-P admission")
)]
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
            inbox,
            grants,
        }
    }

    /// The owner is done: tell the keeper and hand the mutator back for its
    /// retire.
    pub(crate) fn exit(self) -> Mutator<'era> {
        let _ = self.inbox.send(KeeperMsg::Exit { owner: self.owner });
        self.mutator
    }

    /// Publish and stay NativeStopped without asking for a turn, so a stop
    /// does not wait on this owner while it waits for something else.
    pub(crate) fn publish_idle(&mut self) -> Result<(), PortRefusal> {
        self.publish().map(drop)
    }

    /// Publish again: leave native first if the last request entered it.
    fn publish(&mut self) -> Result<PublishedOwner, PortRefusal> {
        let mutator = &mut self.mutator;
        if self.native {
            match driver_core::retry_contended(busy, || mutator.leave_native().map(|_| ())) {
                Ok(()) => {}
                Err(RendezvousError::Poisoned) => return Err(PortRefusal::Poisoned),
                Err(_) => return Err(PortRefusal::Retry),
            }
            self.native = false;
        }
        match driver_core::retry_contended(busy, || mutator.publish_native_owner(&[])) {
            Ok(receipt) => {
                self.native = true;
                Ok(receipt)
            }
            Err(RendezvousError::Poisoned) => Err(PortRefusal::Poisoned),
            Err(_) => Err(PortRefusal::Retry),
        }
    }
}

impl KeeperPort for ChannelPort<'_> {
    type LendGuard = ChannelLend;

    fn give_back(&mut self, returned: ReturnedSerialTicket) {
        let _ = self.inbox.send(KeeperMsg::GiveBack {
            owner: self.owner,
            returned,
        });
    }

    fn request_turn(&mut self) -> Result<(Grant, ChannelLend), PortRefusal> {
        for _ in 0..=REPUBLISH_LIMIT {
            let receipt = self.publish()?;
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

    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason) {
        let _ = self.inbox.send(KeeperMsg::Refuse {
            owner: self.owner,
            ticket,
            reason,
        });
    }
}

// The driver's rows live with E5.2's live rows (C3): tests/serial_park_live_test.rs.
