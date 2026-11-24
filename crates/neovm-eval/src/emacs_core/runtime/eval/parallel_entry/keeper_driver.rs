//! L6.7a: the keeper driver for parallel owners' serial turns.
//!
//! One keeper service thread runs [`Driver::run`]. Owners reach it only by
//! messages: a request (sent after publishing), the returned ticket, a
//! refused ticket, an abandoned lend, and an exit. The keeper grants inside
//! its own stop, one ticket at a time, in ring order, and serves at most the
//! requests queued when that stop began; later requests wait for the next
//! stop, so parallel owners run between stops.
//!
//! Only a proven invariant break poisons the era: an owner whose lend guard
//! dropped armed (a Rust unwind or a leaked turn), or a message that cannot
//! belong to the protocol. A Lisp signal ends a turn normally. A stop its
//! owners have not met within its bound stays requested: the keeper warns,
//! serves its inbox, and waits on (K8 r2). A slow owner poisons nothing.
#![forbid(unsafe_code)]

use super::keeper_custody::{
    KeeperReadyRequest, RestoredKeeper, ReturnedSerialTicket, StoppedKeeper, SuspendedKeeper,
};
use super::owned_source::KeeperEra;
use super::serial_park::{Grant, KeeperPort, LendGuard, PortRefusal};
use super::serial_ticket::{ReceiveReason, StoppedSerialTicket};
use super::sync::mpsc;
use neovm_value::tagged::gc::heap_transfer::{PublishedOwner, with_stop_scope};
use neovm_value::tagged::gc::{
    AllStopped, AllStoppedFailure, Mutator, MutatorKey, RendezvousError, SerialGrant, StopAttempt,
    StopPending,
};
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

/// How long one stop attempt waits for its owners before the keeper warns
/// and serves its inbox, and how long a contended gate is retried before the
/// call gives up. Neither poisons.
const CONTENTION_BOUND: Duration = Duration::from_secs(60);

/// Owner -> keeper. One inbox, so each owner's messages arrive in the order
/// its port sent them.
pub(crate) enum KeeperMsg {
    /// The owner published `receipt` and asks for a serial turn.
    Request { owner: u64, receipt: PublishedOwner },
    /// The lent ticket came back.
    GiveBack {
        owner: u64,
        returned: ReturnedSerialTicket,
    },
    /// The owner could not receive the lent ticket; it comes back unopened.
    Refuse {
        owner: u64,
        ticket: StoppedSerialTicket,
        reason: ReceiveReason,
    },
    /// The owner's lend guard dropped armed: the ticket is lost.
    Abandon { owner: u64, cause: LendDrop },
    /// The owner finished; it requests nothing more.
    Exit { owner: u64 },
}

/// Keeper -> one owner.
pub(crate) enum GrantMsg {
    /// A ticket cut in the stop of `epoch`.
    Grant {
        ticket: StoppedSerialTicket,
        epoch: NonZeroU64,
    },
    /// The request's publication is no longer current: publish again.
    Republish,
    /// The era is poisoned.
    Poisoned,
}

/// Why a lend guard dropped armed. Both lose the ticket, so both poison; the
/// cause is the diagnostic (lead #13898: a Lisp signal never gets here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LendDrop {
    /// Dropped while the owner thread was unwinding from a Rust panic.
    Unwound,
    /// Dropped without `end` or `park` and without a panic.
    Leaked,
}

/// Why the driver stopped serving.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DriverEnd {
    /// Every owner exited and no request is queued.
    Drained,
    /// A proven invariant break poisoned the era.
    Poisoned(Breach),
}

/// A proven invariant break.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Breach {
    /// An owner's lend guard dropped armed.
    Abandoned { owner: u64, cause: LendDrop },
    /// A message the protocol does not allow at this point.
    Unexpected { owner: u64, message: &'static str },
    /// An owner refused the ticket the keeper cut for it. The keeper sends
    /// each ticket to the owner it was cut for, so no reason is recoverable.
    Refused { owner: u64, reason: ReceiveReason },
    /// A message from an owner the keeper did not enroll.
    UnknownOwner { owner: u64 },
    /// The owner's grant channel closed while a ticket was cut for it: the
    /// owner is gone and the ticket is lost, as with an abandoned lend.
    Unreachable { owner: u64 },
    /// Every sender is gone while a ticket is out.
    Disconnected,
    /// A refusal the driver cannot recover from.
    Rendezvous(RendezvousError),
    /// The keeper's custody refused a lend it validated.
    Custody,
}

struct Enrolled {
    key: MutatorKey,
    grants: mpsc::SyncSender<GrantMsg>,
}

/// The keeper between two lends of one stop: nothing is out.
enum Between<'s, 'era> {
    Fresh(StoppedKeeper<'s, 'era>),
    Restored(RestoredKeeper<'s, 'era>),
}

impl<'s, 'era> Between<'s, 'era> {
    fn serial_head(&self) -> Result<Option<(MutatorKey, NonZeroU64)>, RendezvousError> {
        match self {
            Between::Fresh(keeper) => keeper.serial_head(),
            Between::Restored(keeper) => keeper.serial_head(),
        }
    }

    fn ready_request(
        &self,
        receipt: PublishedOwner,
    ) -> Result<KeeperReadyRequest<'s>, RendezvousError> {
        match self {
            Between::Fresh(keeper) => keeper.ready_request(receipt),
            Between::Restored(keeper) => keeper.ready_request(receipt),
        }
    }

    fn grant_for(
        &mut self,
        request: &KeeperReadyRequest<'s>,
    ) -> Result<Option<SerialGrant>, RendezvousError> {
        match self {
            Between::Fresh(keeper) => keeper.grant_for(request),
            Between::Restored(keeper) => keeper.grant_for(request),
        }
    }

    fn lend(self, request: KeeperReadyRequest<'s>) -> Result<SuspendedKeeper<'s, 'era>, Breach> {
        let lent = match self {
            Between::Fresh(keeper) => keeper.lend_serial(request),
            Between::Restored(keeper) => keeper.lend_next(request),
        };
        lent.map_err(|failure| {
            // The failure still holds the stop; finishing it here keeps the
            // epoch from being dropped twice. The era is poisoned anyway.
            let _ = failure.complete_traced();
            Breach::Custody
        })
    }

    fn complete(self) -> Result<(), AllStoppedFailure<'era>> {
        match self {
            Between::Fresh(keeper) => keeper.complete_traced(),
            Between::Restored(keeper) => keeper.complete_traced(),
        }
    }
}

/// Called after each pending stop attempt, once the keeper has served its
/// inbox, with what the attempt waited on and how many requests are queued.
type PendingObserver<'k> = Box<dyn FnMut(&StopPending, usize) + 'k>;

/// The keeper side of L6.7a.
pub(crate) struct Driver<'k, 'service, 'heap> {
    era: &'k KeeperEra<'service, 'heap>,
    inbox: mpsc::Receiver<KeeperMsg>,
    owners: HashMap<u64, Enrolled>,
    /// The receipt each queued request was committed under.
    queued: HashMap<u64, PublishedOwner>,
    /// One stop attempt's bound.
    stop_bound: Duration,
    pending_observer: Option<PendingObserver<'k>>,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "no production spawner until E6-P admission")
)]
impl<'k, 'service, 'heap> Driver<'k, 'service, 'heap> {
    pub(crate) fn new(
        era: &'k KeeperEra<'service, 'heap>,
        inbox: mpsc::Receiver<KeeperMsg>,
    ) -> Self {
        Self {
            era,
            inbox,
            owners: HashMap::new(),
            queued: HashMap::new(),
            stop_bound: CONTENTION_BOUND,
            pending_observer: None,
        }
    }

    /// Shorten each stop attempt to BOUND and hand every pending attempt to
    /// OBSERVER. The driver's loop is the same with or without one.
    #[cfg(test)]
    pub(crate) fn observe_pending(
        mut self,
        bound: Duration,
        observer: impl FnMut(&StopPending, usize) + 'k,
    ) -> Self {
        self.stop_bound = bound;
        self.pending_observer = Some(Box::new(observer));
        self
    }

    /// Enroll one owner before it can request. `grants` is that owner's own
    /// one-slot channel.
    pub(crate) fn enroll(
        &mut self,
        owner: u64,
        key: MutatorKey,
        grants: mpsc::SyncSender<GrantMsg>,
    ) {
        self.owners.insert(owner, Enrolled { key, grants });
    }

    /// Serve until every owner has exited, or until a proven invariant
    /// break poisons the era.
    pub(crate) fn run(mut self) -> DriverEnd {
        loop {
            if self.queued.is_empty() {
                match self.inbox.recv() {
                    Ok(message) => {
                        if let Err(breach) = self.between_stops(message) {
                            return self.poison(breach);
                        }
                    }
                    Err(mpsc::RecvError) => return DriverEnd::Drained,
                }
            }
            while let Ok(message) = self.inbox.try_recv() {
                if let Err(breach) = self.between_stops(message) {
                    return self.poison(breach);
                }
            }
            if !self.queued.is_empty()
                && let Err(breach) = self.serve_stop()
            {
                return self.poison(breach);
            }
        }
    }

    fn key(&self, owner: u64) -> Result<MutatorKey, Breach> {
        self.owners
            .get(&owner)
            .map(|enrolled| enrolled.key)
            .ok_or(Breach::UnknownOwner { owner })
    }

    /// A message while no ticket is out.
    fn between_stops(&mut self, message: KeeperMsg) -> Result<(), Breach> {
        match message {
            KeeperMsg::Request { owner, receipt } => self.commit(owner, receipt),
            KeeperMsg::Exit { owner } => self.exit(owner),
            KeeperMsg::GiveBack { owner, .. } => Err(Breach::Unexpected {
                owner,
                message: "a returned ticket with no lend out",
            }),
            KeeperMsg::Refuse {
                owner,
                ticket,
                reason,
            } => {
                drop(ticket);
                Err(Breach::Refused { owner, reason })
            }
            KeeperMsg::Abandon { owner, cause } => Err(Breach::Abandoned { owner, cause }),
        }
    }

    /// Commit a request at the ring's tail: commit order is receive order.
    fn commit(&mut self, owner: u64, receipt: PublishedOwner) -> Result<(), Breach> {
        let key = self.key(owner)?;
        if self.queued.contains_key(&owner) {
            return Err(Breach::Unexpected {
                owner,
                message: "a second request while one is queued",
            });
        }
        let (_, generation, _) = receipt.serial_facts();
        retry_contended(|| self.era.enqueue_serial(key, generation)).map_err(Breach::Rendezvous)?;
        self.queued.insert(owner, receipt);
        Ok(())
    }

    /// A normal exit: cancel the owner's queued request, if any.
    fn exit(&mut self, owner: u64) -> Result<(), Breach> {
        let key = self.key(owner)?;
        if self.queued.remove(&owner).is_some() {
            retry_contended(|| self.era.cancel_serial(key)).map_err(Breach::Rendezvous)?;
        }
        self.owners.remove(&owner);
        Ok(())
    }

    /// Send MESSAGE to OWNER. A closed grant channel hands it back: that
    /// owner is gone.
    fn send(&self, owner: u64, message: GrantMsg) -> Result<(), GrantMsg> {
        match self.owners.get(&owner) {
            Some(enrolled) => enrolled
                .grants
                .send(message)
                .map_err(|mpsc::SendError(message)| message),
            None => Err(message),
        }
    }

    /// Take the stop (K8 r2). Each attempt is bounded. A pending attempt
    /// leaves the request standing, so the owners that stopped stay stopped
    /// and a requester keeps its place: the keeper warns, commits what its
    /// inbox holds, and continues the same epoch. As with GNU's global lock,
    /// a slow owner delays the turn and poisons nothing.
    fn take_stop(&mut self) -> Result<AllStopped<'k>, Breach> {
        let mut contended: Option<Backoff> = None;
        loop {
            match self.era.stop_within(self.stop_bound) {
                Ok(StopAttempt::Stopped(stopped)) => return Ok(stopped),
                Ok(StopAttempt::Pending(pending)) => {
                    contended = None;
                    tracing::warn!(
                        epoch = ?pending.epoch(),
                        waiting_on = ?pending.waiting_on(),
                        "keeper stop overdue"
                    );
                    while let Ok(message) = self.inbox.try_recv() {
                        self.between_stops(message)?;
                    }
                    let queued = self.queued.len();
                    if let Some(observe) = &mut self.pending_observer {
                        observe(&pending, queued);
                    }
                }
                // A gate holder is brief by construction. Back off rather
                // than spin, and name the holder if the hold lasts.
                Err(RendezvousError::Busy) => {
                    let backoff = contended.get_or_insert_with(Backoff::new);
                    backoff.wait();
                    if backoff.warning_due() {
                        tracing::warn!(
                            holder = ?self.era.gate_holder(),
                            waited = ?backoff.waited(),
                            "keeper stop: the registry gate stays held"
                        );
                    }
                }
                Err(reason) => return Err(Breach::Rendezvous(reason)),
            }
        }
    }

    /// One stop: lend the ring's head in order, each after the previous
    /// ticket came back, at most the requests queued when the stop began.
    fn serve_stop(&mut self) -> Result<(), Breach> {
        let mut stopped = self.take_stop()?;
        let epoch = stopped.epoch();
        if let Err(reason) = retry_contended(|| stopped.trace_serial()) {
            return Err(Breach::Rendezvous(reason));
        }
        let bound = match stopped.serial_ring_len() {
            Ok(len) => len,
            Err(reason) => return Err(Breach::Rendezvous(reason)),
        };
        with_stop_scope(|scope| {
            let keeper = StoppedKeeper::from_scope(scope, stopped)
                .map_err(|(_scope, _stopped, reason)| Breach::Rendezvous(reason))?;
            let mut between = Between::Fresh(keeper);
            for _ in 0..bound {
                let Some((head, _generation)) =
                    between.serial_head().map_err(Breach::Rendezvous)?
                else {
                    break;
                };
                let owner = head.owner();
                let Some(receipt) = self.queued.remove(&owner) else {
                    return Err(Breach::Unexpected {
                        owner,
                        message: "a ring entry with no committed request",
                    });
                };
                let request = match between.ready_request(receipt) {
                    Ok(request) => request,
                    // The receipt is not the owner's current publication:
                    // it published again after its request (Generation), or
                    // holds no publication now (Publication). Send it back
                    // to publish and requeue at the tail. FIFO is not
                    // skipped, so this stop lends nothing more.
                    Err(RendezvousError::Generation | RendezvousError::Publication) => {
                        retry_contended(|| self.era.cancel_serial(head))
                            .map_err(Breach::Rendezvous)?;
                        if self.send(owner, GrantMsg::Republish).is_err() {
                            // Gone while only queued: like an exit.
                            self.owners.remove(&owner);
                        }
                        break;
                    }
                    Err(reason) => return Err(Breach::Rendezvous(reason)),
                };
                let Some(grant) = between.grant_for(&request).map_err(Breach::Rendezvous)? else {
                    return Err(Breach::Unexpected {
                        owner,
                        message: "a ready head the ring did not grant",
                    });
                };
                let lent = between.lend(request)?;
                let (ticket, held) = StoppedSerialTicket::from_lend(lent);
                if self.send(owner, GrantMsg::Grant { ticket, epoch }).is_err() {
                    // The bounced message drops the ticket, and `held`
                    // drops the stop with the breach.
                    return Err(Breach::Unreachable { owner });
                }
                let returned = self.await_return(owner)?;
                let mut restored =
                    held.accept_return(returned)
                        .map_err(|_| Breach::Unexpected {
                            owner,
                            message: "a returned ticket that is not the one cut",
                        })?;
                restored.retire(grant).map_err(Breach::Rendezvous)?;
                between = Between::Restored(restored);
            }
            between
                .complete()
                .map_err(|failure| Breach::Rendezvous(failure.reason))
        })
    }

    /// Wait for `owner`'s ticket. Other owners' requests and exits are
    /// handled as they come; anything else is a breach.
    fn await_return(&mut self, lent: u64) -> Result<ReturnedSerialTicket, Breach> {
        loop {
            let message = self
                .inbox
                .recv()
                .map_err(|mpsc::RecvError| Breach::Disconnected)?;
            match message {
                KeeperMsg::GiveBack { owner, returned } if owner == lent => return Ok(returned),
                KeeperMsg::Abandon { owner, cause } => {
                    return Err(Breach::Abandoned { owner, cause });
                }
                KeeperMsg::Request { owner, receipt } if owner != lent => {
                    self.commit(owner, receipt)?;
                }
                KeeperMsg::Exit { owner } if owner != lent => self.exit(owner)?,
                KeeperMsg::Request { owner, .. } => {
                    return Err(Breach::Unexpected {
                        owner,
                        message: "a request while its own ticket is out",
                    });
                }
                KeeperMsg::Exit { owner } => {
                    return Err(Breach::Unexpected {
                        owner,
                        message: "an exit while its ticket is out",
                    });
                }
                KeeperMsg::GiveBack { owner, .. } => {
                    return Err(Breach::Unexpected {
                        owner,
                        message: "a returned ticket from an owner with no lend",
                    });
                }
                KeeperMsg::Refuse {
                    owner,
                    ticket,
                    reason,
                } => {
                    // The ticket is dropped with the stop the breach ends.
                    drop(ticket);
                    return Err(Breach::Refused { owner, reason });
                }
            }
        }
    }

    /// Poison the era and tell every owner. Dropping a held stop has already
    /// poisoned it when the breach came from inside one.
    fn poison(mut self, breach: Breach) -> DriverEnd {
        let _ = self.era.poison_era();
        // The ring dies with the era (a poisoned gate refuses every ring
        // call), so queued owners are only told.
        for (owner, _receipt) in std::mem::take(&mut self.queued) {
            let _ = self.send(owner, GrantMsg::Poisoned);
        }
        for enrolled in self.owners.values() {
            let _ = enrolled.grants.try_send(GrantMsg::Poisoned);
        }
        DriverEnd::Poisoned(breach)
    }
}

/// Waits on a contended registry gate (lead #13966 LOW-1). The first waits
/// only yield; later ones sleep, doubling up to a millisecond, so a long
/// hold never spins.
struct Backoff {
    attempts: u32,
    start: Instant,
    next_warning: Duration,
}

impl Backoff {
    /// Waits that only yield before the sleeps start.
    const YIELDS: u32 = 64;

    fn new() -> Self {
        Self {
            attempts: 0,
            start: Instant::now(),
            next_warning: CONTENTION_BOUND,
        }
    }

    fn wait(&mut self) {
        self.attempts = self.attempts.saturating_add(1);
        match self.attempts.checked_sub(Self::YIELDS) {
            None | Some(0) => std::thread::yield_now(),
            Some(extra) => std::thread::sleep(Duration::from_micros(1 << extra.min(10))),
        }
    }

    fn waited(&self) -> Duration {
        self.start.elapsed()
    }

    /// True once per [`CONTENTION_BOUND`] of waiting.
    fn warning_due(&mut self) -> bool {
        let due = self.waited() >= self.next_warning;
        if due {
            self.next_warning += CONTENTION_BOUND;
        }
        due
    }
}

/// Retry a contended gate until [`CONTENTION_BOUND`]; any other result,
/// and `Busy` past the bound, passes through.
fn retry_contended<T>(
    mut attempt: impl FnMut() -> Result<T, RendezvousError>,
) -> Result<T, RendezvousError> {
    let mut backoff = Backoff::new();
    loop {
        match attempt() {
            Err(RendezvousError::Busy) if backoff.waited() < CONTENTION_BOUND => backoff.wait(),
            other => return other,
        }
    }
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
            match retry_contended(|| mutator.leave_native().map(|_| ())) {
                Ok(()) => {}
                Err(RendezvousError::Poisoned) => return Err(PortRefusal::Poisoned),
                Err(_) => return Err(PortRefusal::Retry),
            }
            self.native = false;
        }
        match retry_contended(|| mutator.publish_native_owner(&[])) {
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
