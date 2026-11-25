//! L6.7a: the keeper driver's protocol, for parallel owners' serial turns.
//!
//! One keeper service thread runs [`Driver::run`]. Owners reach it only by
//! messages: a request (sent after publishing), the returned ticket, a
//! refused ticket, an abandoned lend, and an exit. The keeper grants inside
//! its own stop, one ticket at a time, in ring order, and serves at most the
//! requests queued when that stop began; later requests wait for the next
//! stop, so parallel owners run between stops.
//!
//! The Baton group (main and every Baton thread) is one more client of the
//! same ring (L6.7b). Its head is lent the World instead of a ticket, on the
//! group's own channel; the keeper keeps its stop while the World is out, so
//! no parallel owner runs during the group's interval.
//!
//! Only a proven invariant break poisons the era: an owner whose lend guard
//! dropped armed (a Rust unwind or a leaked turn), a message that cannot
//! belong to the protocol, or a keeper whose stop waits on its own guard
//! (`SelfWait`). A Lisp signal ends a turn normally. A stop its
//! owners have not met within its bound stays requested: the keeper warns,
//! serves its inbox, and waits on (K8 r2). A slow owner poisons nothing.
//!
//! This file holds no era or custody type (a3.1, K9). [`KeeperOps`] is the
//! era and [`Batch`] the keeper between two lends of one stop, so one body
//! runs over the product binding (`keeper_driver.rs`) and over a Loom model.
//! Its only synchronization comes through `super::sync`.
#![forbid(unsafe_code)]

use super::sync::{self, mpsc};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::num::NonZeroU64;
use std::time::Duration;

/// How long one stop attempt waits for its owners before the keeper warns
/// and serves its inbox, and how long a contended gate is retried before the
/// call gives up. Neither poisons.
pub(crate) const CONTENTION_BOUND: Duration = Duration::from_secs(60);

/// One bounded stop attempt, as the era reports it.
pub(crate) enum Attempt<S, P> {
    /// Every owner stopped: the stop is the keeper's.
    Stopped(S),
    /// The bound passed first; the request stands.
    Pending(P),
}

/// What an era refusal means to the driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A gate holder or the keeper's own guard: back off and retry.
    Busy,
    /// The receipt was superseded by a newer publication: send the owner back
    /// to publish.
    Superseded,
    /// Anything else: a breach.
    Other,
}

/// The ring head as the driver routes it (L6.7b b3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Head<K> {
    /// A parallel owner: lend its serial ticket.
    Owner(K),
    /// The Baton group: lend it the World.
    BatonGroup,
}

/// The era and the keeper's custody as the driver uses them.
pub(crate) trait KeeperOps: Copy {
    type Key: Copy + Eq + Debug;
    /// An owner's published request.
    type Receipt;
    /// The ticket a grant lends.
    type Ticket;
    /// A ticket that came back.
    type Returned;
    /// Why an owner refused a ticket.
    type Reason: Copy + Debug + Eq;
    /// The Baton group's turn at the World (L6.7b).
    type WorldTurn;
    /// A World turn that came back.
    type WorldReturned;
    type Error: Copy + Debug + Eq;
    /// The keeper's stop.
    type Stopped;
    /// What a pending attempt waited on.
    type Pending: Debug;
    /// Summary of one completed in-stop collection.
    type Report: Copy + Debug;

    fn refusal(error: Self::Error) -> Refusal;
    fn owner(key: Self::Key) -> u64;
    /// The publication generation a receipt commits its request under.
    fn generation(receipt: &Self::Receipt) -> NonZeroU64;
    fn enqueue(self, key: Self::Key, generation: NonZeroU64) -> Result<(), Self::Error>;
    fn cancel(self, key: Self::Key) -> Result<(), Self::Error>;
    /// Commit the Baton group's one request at the ring's tail.
    fn enqueue_group(self) -> Result<(), Self::Error>;
    /// Cancel the Baton group's queued request.
    fn cancel_group(self) -> Result<(), Self::Error>;
    /// Put the Baton group's request back at the ring's HEAD (L6.7b b5).
    fn requeue_group_at_head(self) -> Result<(), Self::Error>;
    fn stop_within(
        self,
        bound: Duration,
    ) -> Result<Attempt<Self::Stopped, Self::Pending>, Self::Error>;
    /// The registry gate's holder now, for a contention diagnostic.
    fn gate_holder(self) -> Option<NonZeroU64>;
    fn poison(self);
    fn epoch(stopped: &Self::Stopped) -> NonZeroU64;
    fn trace(stopped: &mut Self::Stopped) -> Result<(), Self::Error>;
    /// Whether aggregate allocation debt meets the heap's GC threshold.
    fn should_collect(self, stopped: &mut Self::Stopped) -> Result<bool, Self::Error>;
    /// Run one stopped collection on `stopped`, consuming its single trace.
    fn collect_stopped(self, stopped: &mut Self::Stopped) -> Result<Self::Report, Self::Error>;
    /// Complete a traced stop that has no queued serial turns (GC-only stop).
    fn complete_empty(stopped: Self::Stopped) -> Result<(), Self::Error>;
    /// Serial requests queued or granted under this stop.
    fn ring_len(stopped: &Self::Stopped) -> Result<usize, Self::Error>;
    /// Bind STOPPED as the keeper's custody for one batch and hand it to
    /// VISITOR. The custody types are branded to this call.
    fn with_batch<V: BatchVisitor<Self>>(
        self,
        stopped: Self::Stopped,
        visitor: V,
    ) -> Result<V::Output, BreachOf<Self>>;
}

/// The keeper between two lends of one stop: nothing is out. `lend` and
/// `lend_world` move the batch and only `settle`/`settle_world` give the next
/// one, so an implementation must not be `Copy`: a copy could lend again
/// before the turn came back.
pub(crate) trait Batch<O: KeeperOps>: Sized {
    /// A request validated against its owner's current publication.
    type Ready;
    type Grant;
    /// The custody a lent ticket comes back to.
    type Held;
    /// The custody a lent World turn comes back to.
    type WorldHeld;

    /// The ring head's client.
    fn head(&self) -> Result<Option<Head<O::Key>>, O::Error>;
    fn ready(&self, receipt: O::Receipt) -> Result<Self::Ready, O::Error>;
    fn grant_for(&mut self, ready: &Self::Ready) -> Result<Option<Self::Grant>, O::Error>;
    /// Grant a Baton-group head: the group has no publication to check.
    fn grant_group(&mut self) -> Result<Option<Self::Grant>, O::Error>;
    /// Lend READY's turn: cut its ticket. `Err` means custody refused a lend
    /// it had validated.
    fn lend(self, ready: Self::Ready) -> Result<(O::Ticket, Self::Held), CustodyRefused>;
    /// The ticket came back: accept it and retire its grant.
    fn settle(
        held: Self::Held,
        returned: O::Returned,
        grant: Self::Grant,
    ) -> Result<Self, Settle<O::Error>>;
    /// Lend the World to the Baton group: cut its turn. Nothing is exported;
    /// the keeper keeps the stop while the turn is out.
    fn lend_world(self) -> Result<(O::WorldTurn, Self::WorldHeld), CustodyRefused>;
    /// The World turn came back: accept it and retire its grant.
    fn settle_world(
        held: Self::WorldHeld,
        returned: O::WorldReturned,
        grant: Self::Grant,
    ) -> Result<Self, Settle<O::Error>>;
    fn complete(self) -> Result<(), O::Error>;
}

/// Runs the driver's batch over a branded [`Batch`].
pub(crate) trait BatchVisitor<O: KeeperOps> {
    type Output;
    fn visit<B: Batch<O>>(self, batch: B) -> Result<Self::Output, BreachOf<O>>;
}

/// Custody refused a lend it had validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CustodyRefused;

/// Why a returned ticket could not be settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settle<E> {
    /// It is not the ticket the keeper cut.
    NotTheTicketCut,
    /// Retiring the grant was refused.
    Retire(E),
}

/// Why a collection request was refused (K5 r1 / K8 r2: a slow stop keeps
/// waiting rather than returning `Retry`, so poison is the only refusal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CollectRefusal {
    /// The era is poisoned.
    Poisoned,
}

/// Owner or Baton group -> keeper. One inbox, so each sender's messages
/// arrive in the order it sent them.
pub(crate) enum KeeperMsg<Receipt, Returned, Ticket, Reason, WorldReturned, Report> {
    /// The owner published `receipt` and asks for a serial turn.
    Request { owner: u64, receipt: Receipt },
    /// The lent ticket came back.
    GiveBack { owner: u64, returned: Returned },
    /// The owner could not receive the lent ticket; it comes back unopened.
    Refuse {
        owner: u64,
        ticket: Ticket,
        reason: Reason,
    },
    /// The owner's lend guard dropped armed: the ticket is lost.
    Abandon { owner: u64, cause: LendDrop },
    /// The owner finished; it requests nothing more.
    Exit { owner: u64 },
    /// The Baton group asks for the World (L6.7b). One request stands for
    /// the whole group however many of its threads woke.
    BatonRequest,
    /// The group's World turn came back, and how its interval ended.
    WorldReturn {
        returned: WorldReturned,
        end: IntervalEnd<Report>,
    },
    /// The group's lend guard dropped armed: the World turn is lost.
    GroupAbandon { cause: LendDrop },
    /// The group left the era; it requests nothing more.
    GroupExit,
    /// Explicit collection request (K5): wakes the keeper even when the serial
    /// ring is empty and rides the next stop.
    Collect {
        reply: Option<mpsc::SyncSender<Result<Report, CollectRefusal>>>,
    },
    /// The Baton group found no runnable thread (L6.7b b4, lead #14147 QD2):
    /// may it report a deadlock? REPLY gets `true` only when the keeper is
    /// between stops with no live parallel owner, nothing queued and no
    /// collection asked for. Any other state answers `false`: a missed
    /// verdict is acceptable, a false one never is.
    ///
    /// "Live parallel owner" means an owner enrolled with this driver and
    /// not exited. A mutator registered with the era but never enrolled is
    /// invisible to the query. Accepted limitation (lead #14206): any
    /// enrolled owner blocks the verdict, so a deadlock between Baton
    /// waiters and a live owner that never wakes them is a silent hang, not
    /// a reported deadlock.
    IdleQuery { reply: mpsc::SyncSender<bool> },
}

/// How the Baton group ended its interval (L6.7b b5).
pub(crate) enum IntervalEnd<Report> {
    /// No Baton thread runs on: the group asks again on its next wake.
    Done,
    /// A Baton thread collects (`garbage-collect`, or allocation debt at the
    /// threshold). The keeper ends this stop, collects first in its next one,
    /// and lends the group the World again at the ring HEAD, so the calling
    /// thread is the next World user with no other client's turn in between,
    /// as GNU's collection runs synchronously under the global lock
    /// (alloc.c:5994, :5779). REPLY, if any, gets that collection's report.
    Collect {
        reply: Option<mpsc::SyncSender<Result<Report, CollectRefusal>>>,
    },
}

/// Keeper -> one owner.
pub(crate) enum GrantMsg<Ticket> {
    /// A ticket cut in the stop of `epoch`.
    Grant { ticket: Ticket, epoch: NonZeroU64 },
    /// The request's publication is no longer current: publish again.
    Republish,
    /// The era is poisoned.
    Poisoned,
}

/// Keeper -> the Baton group, on the group's own channel: an owner can never
/// receive a World turn, nor the group an owner's ticket.
pub(crate) enum GroupMsg<WorldTurn> {
    /// The World, lent in the stop of `epoch`.
    World { turn: WorldTurn, epoch: NonZeroU64 },
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
pub(crate) enum DriverEnd<Reason, Error> {
    /// Every owner exited and no request is queued.
    Drained,
    /// A proven invariant break poisoned the era.
    Poisoned(Breach<Reason, Error>),
}

/// A proven invariant break.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Breach<Reason, Error> {
    /// An owner's lend guard dropped armed.
    Abandoned { owner: u64, cause: LendDrop },
    /// A message the protocol does not allow at this point.
    Unexpected { owner: u64, message: &'static str },
    /// An owner refused the ticket the keeper cut for it. The keeper sends
    /// each ticket to the owner it was cut for, so no reason is recoverable.
    Refused { owner: u64, reason: Reason },
    /// A message from an owner the keeper did not enroll.
    UnknownOwner { owner: u64 },
    /// The owner's grant channel closed while a ticket was cut for it: the
    /// grant bounced back and its ticket is dropped with the stop, so the era
    /// is poisoned as for an abandoned lend.
    Unreachable { owner: u64 },
    /// The keeper's stop refused `Busy` for a whole contention bound with no
    /// gate holder ever seen: the keeper waits on its own guard, which no
    /// retry can release.
    SelfWait,
    /// Every sender is gone while a ticket is out.
    Disconnected,
    /// A refusal the driver cannot recover from.
    Rendezvous(Error),
    /// The keeper's custody refused a lend it validated.
    Custody,
    /// The Baton group's lend guard dropped armed.
    GroupAbandoned { cause: LendDrop },
    /// A Baton-group message the protocol does not allow at this point.
    GroupUnexpected { message: &'static str },
    /// The group's channel closed while its World turn was cut for it.
    GroupUnreachable,
}

pub(crate) type MsgOf<O> = KeeperMsg<
    <O as KeeperOps>::Receipt,
    <O as KeeperOps>::Returned,
    <O as KeeperOps>::Ticket,
    <O as KeeperOps>::Reason,
    <O as KeeperOps>::WorldReturned,
    <O as KeeperOps>::Report,
>;
pub(crate) type GrantOf<O> = GrantMsg<<O as KeeperOps>::Ticket>;
pub(crate) type GroupOf<O> = GroupMsg<<O as KeeperOps>::WorldTurn>;
pub(crate) type GroupLinkOf<O> = GroupLink<<O as KeeperOps>::WorldTurn>;
pub(crate) type IntervalEndOf<O> = IntervalEnd<<O as KeeperOps>::Report>;
pub(crate) type BreachOf<O> = Breach<<O as KeeperOps>::Reason, <O as KeeperOps>::Error>;
pub(crate) type EndOf<O> = DriverEnd<<O as KeeperOps>::Reason, <O as KeeperOps>::Error>;

struct Enrolled<O: KeeperOps> {
    key: O::Key,
    grants: mpsc::SyncSender<GrantOf<O>>,
}

/// The Baton group's seat at the keeper.
struct Group<O: KeeperOps> {
    grants: mpsc::SyncSender<GroupOf<O>>,
    request: GroupRequest,
    waiter: sync::Arc<RingWaiter>,
}

/// Whether the group's one request is in the ring.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupRequest {
    None,
    Queued,
}

/// The Baton group's "the ring has a waiter" word (L6.7b b4.1, QB2). The
/// keeper raises it while the group holds the World and another client (a
/// parallel owner's request or a collection) is waiting, and lowers it when
/// the World comes back. The group reads it at each switch point with one
/// Acquire load: no lock, no syscall.
///
/// The QB2 contract (ratified, lead #14206 R2): the word is RAISED at the
/// lend when a client is already waiting, so an owner committed behind the
/// group is not missed. This overrides the b4 plan's "a grant starts
/// false". Only the keeper writes it (Release). It is published through the
/// channel that carries the lend, and lowered only after the World returns.
pub(crate) struct RingWaiter(sync::atomic::AtomicBool);

/// What `Driver::enroll_group` hands the group's port: the World channel
/// and the waiter word of one enrollment, so a port cannot pair one
/// driver's channel with another's word (lead #14206 nit 4).
pub(crate) struct GroupLink<WorldTurn> {
    world: mpsc::Receiver<GroupMsg<WorldTurn>>,
    waiter: sync::Arc<RingWaiter>,
}

impl<WorldTurn> GroupLink<WorldTurn> {
    /// The World channel and the waiter word, for the group's port.
    pub(crate) fn into_parts(self) -> (mpsc::Receiver<GroupMsg<WorldTurn>>, sync::Arc<RingWaiter>) {
        (self.world, self.waiter)
    }
}

impl RingWaiter {
    fn new() -> Self {
        Self(sync::atomic::AtomicBool::new(false))
    }

    /// Whether a client is waiting behind the group's interval.
    pub(crate) fn raised(&self) -> bool {
        self.0.load(sync::atomic::Ordering::Acquire)
    }

    fn set(&self, raised: bool) {
        self.0.store(raised, sync::atomic::Ordering::Release);
    }
}

/// Called after each pending stop attempt, once the keeper has served its
/// inbox, with what the attempt waited on and how many requests are queued.
type PendingObserver<'o, P> = Box<dyn FnMut(&P, usize) + 'o>;

/// The keeper side of L6.7a.
pub(crate) struct Driver<'o, O: KeeperOps> {
    ops: O,
    inbox: mpsc::Receiver<MsgOf<O>>,
    /// Ordered by owner, so the poison fan-out tells owners in one fixed
    /// order on every run (and a Loom replay sees the same sends).
    owners: BTreeMap<u64, Enrolled<O>>,
    /// The receipt each queued request was committed under.
    queued: BTreeMap<u64, O::Receipt>,
    /// The Baton group, once enrolled (L6.7b).
    group: Option<Group<O>>,
    /// Explicit collection requested for the next stop (K5).
    collect_requested: bool,
    /// Waiters expecting the next in-stop collection's report (or Poisoned).
    collect_waiters: Vec<mpsc::SyncSender<Result<O::Report, CollectRefusal>>>,
    /// One stop attempt's bound.
    stop_bound: Duration,
    /// How long a refused stop may stay `Busy` before the keeper warns, or,
    /// with no gate holder seen, ends as a self-wait.
    contention_bound: Duration,
    pending_observer: Option<PendingObserver<'o, O::Pending>>,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "no production spawner until E6-P admission")
)]
impl<'o, O: KeeperOps> Driver<'o, O> {
    pub(crate) fn new(ops: O, inbox: mpsc::Receiver<MsgOf<O>>) -> Self {
        Self {
            ops,
            inbox,
            owners: BTreeMap::new(),
            queued: BTreeMap::new(),
            group: None,
            collect_requested: false,
            collect_waiters: Vec::new(),
            stop_bound: CONTENTION_BOUND,
            contention_bound: CONTENTION_BOUND,
            pending_observer: None,
        }
    }

    /// Shorten the contention bound to BOUND.
    #[cfg(test)]
    pub(crate) fn contention_bound(mut self, bound: Duration) -> Self {
        self.contention_bound = bound;
        self
    }

    /// Shorten each stop attempt to BOUND and hand every pending attempt to
    /// OBSERVER. The driver's loop is the same with or without one.
    #[cfg(test)]
    pub(crate) fn observe_pending(
        mut self,
        bound: Duration,
        observer: impl FnMut(&O::Pending, usize) + 'o,
    ) -> Self {
        self.stop_bound = bound;
        self.pending_observer = Some(Box::new(observer));
        self
    }

    /// Enroll one owner before it can request. `grants` is that owner's own
    /// one-slot channel.
    pub(crate) fn enroll(&mut self, owner: u64, key: O::Key, grants: mpsc::SyncSender<GrantOf<O>>) {
        self.owners.insert(owner, Enrolled { key, grants });
    }

    /// Enroll the Baton group before it can request. The returned link
    /// carries the group's own one-slot World channel and its waiter word
    /// (b4.1), minted together for this driver.
    pub(crate) fn enroll_group(&mut self) -> GroupLinkOf<O> {
        let (grants, world) = mpsc::sync_channel(1);
        let waiter = sync::Arc::new(RingWaiter::new());
        self.group = Some(Group {
            grants,
            request: GroupRequest::None,
            waiter: waiter.clone(),
        });
        GroupLink { world, waiter }
    }

    /// Raise or lower the group's waiter word.
    fn set_waiter(&self, raised: bool) {
        if let Some(group) = &self.group {
            group.waiter.set(raised);
        }
    }

    /// Requests in the ring: queued owners, and the group's if it has one.
    fn queued_len(&self) -> usize {
        let group = self
            .group
            .as_ref()
            .is_some_and(|group| group.request == GroupRequest::Queued);
        self.queued.len() + usize::from(group)
    }

    /// Serve until every owner has exited, or until a proven invariant
    /// break poisons the era.
    pub(crate) fn run(mut self) -> EndOf<O> {
        loop {
            if self.queued_len() == 0 && !self.collect_requested {
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
            if (self.queued_len() != 0 || self.collect_requested)
                && let Err(breach) = self.serve_stop()
            {
                return self.poison(breach);
            }
        }
    }

    fn queue_collect(
        &mut self,
        reply: Option<mpsc::SyncSender<Result<O::Report, CollectRefusal>>>,
    ) {
        self.collect_requested = true;
        if let Some(reply) = reply {
            self.collect_waiters.push(reply);
        }
    }

    fn key(&self, owner: u64) -> Result<O::Key, BreachOf<O>> {
        self.owners
            .get(&owner)
            .map(|enrolled| enrolled.key)
            .ok_or(Breach::UnknownOwner { owner })
    }

    /// A message while no ticket is out.
    fn between_stops(&mut self, message: MsgOf<O>) -> Result<(), BreachOf<O>> {
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
            KeeperMsg::BatonRequest => self.commit_group(),
            KeeperMsg::GroupExit => self.group_exit(),
            KeeperMsg::WorldReturn { .. } => Err(Breach::GroupUnexpected {
                message: "a returned World with no turn out",
            }),
            KeeperMsg::GroupAbandon { cause } => Err(Breach::GroupAbandoned { cause }),
            KeeperMsg::Collect { reply } => {
                self.queue_collect(reply);
                Ok(())
            }
            KeeperMsg::IdleQuery { reply } => {
                let idle =
                    self.owners.is_empty() && self.queued_len() == 0 && !self.collect_requested;
                let _ = reply.try_send(idle);
                Ok(())
            }
        }
    }

    /// Commit the group's one request at the ring's tail.
    fn commit_group(&mut self) -> Result<(), BreachOf<O>> {
        let Some(group) = &self.group else {
            return Err(Breach::GroupUnexpected {
                message: "a request from a group the keeper did not enroll",
            });
        };
        if group.request == GroupRequest::Queued {
            return Err(Breach::GroupUnexpected {
                message: "a second group request while one is queued",
            });
        }
        let ops = self.ops;
        retry_busy::<O, _>(|| ops.enqueue_group()).map_err(Breach::Rendezvous)?;
        if let Some(group) = &mut self.group {
            group.request = GroupRequest::Queued;
        }
        Ok(())
    }

    /// The group left: cancel its queued request, if any.
    fn group_exit(&mut self) -> Result<(), BreachOf<O>> {
        match self.group.take() {
            None => Err(Breach::GroupUnexpected {
                message: "an exit from a group the keeper did not enroll",
            }),
            Some(group) if group.request == GroupRequest::Queued => {
                let ops = self.ops;
                retry_busy::<O, _>(|| ops.cancel_group()).map_err(Breach::Rendezvous)
            }
            Some(_) => Ok(()),
        }
    }

    /// Send MESSAGE to the group. A closed channel hands it back: the group
    /// is gone.
    fn send_group(&self, message: GroupOf<O>) -> Result<(), GroupOf<O>> {
        match &self.group {
            Some(group) => group
                .grants
                .send(message)
                .map_err(|mpsc::SendError(message)| message),
            None => Err(message),
        }
    }

    /// Commit a request at the ring's tail: commit order is receive order.
    fn commit(&mut self, owner: u64, receipt: O::Receipt) -> Result<(), BreachOf<O>> {
        let key = self.key(owner)?;
        if self.queued.contains_key(&owner) {
            return Err(Breach::Unexpected {
                owner,
                message: "a second request while one is queued",
            });
        }
        let generation = O::generation(&receipt);
        let ops = self.ops;
        retry_busy::<O, _>(|| ops.enqueue(key, generation)).map_err(Breach::Rendezvous)?;
        self.queued.insert(owner, receipt);
        Ok(())
    }

    /// A normal exit: cancel the owner's queued request, if any.
    fn exit(&mut self, owner: u64) -> Result<(), BreachOf<O>> {
        let key = self.key(owner)?;
        if self.queued.remove(&owner).is_some() {
            let ops = self.ops;
            retry_busy::<O, _>(|| ops.cancel(key)).map_err(Breach::Rendezvous)?;
        }
        self.owners.remove(&owner);
        Ok(())
    }

    /// Send MESSAGE to OWNER. A closed grant channel hands it back: that
    /// owner is gone.
    fn send(&self, owner: u64, message: GrantOf<O>) -> Result<(), GrantOf<O>> {
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
    fn take_stop(&mut self) -> Result<O::Stopped, BreachOf<O>> {
        let mut contended: Option<Backoff> = None;
        loop {
            match self.ops.stop_within(self.stop_bound) {
                Ok(Attempt::Stopped(stopped)) => return Ok(stopped),
                Ok(Attempt::Pending(pending)) => {
                    contended = None;
                    tracing::warn!(?pending, "keeper stop overdue");
                    while let Ok(message) = self.inbox.try_recv() {
                        self.between_stops(message)?;
                    }
                    let queued = self.queued_len();
                    if let Some(observe) = &mut self.pending_observer {
                        observe(&pending, queued);
                    }
                }
                // A gate holder is brief by construction. Back off rather
                // than spin, and name the holder if the hold lasts. Busy with
                // the gate free for a whole bound is the keeper waiting on
                // its own guard (`stop`'s self-wait refusals), not contention.
                Err(error) if O::refusal(error) == Refusal::Busy => {
                    let backoff =
                        contended.get_or_insert_with(|| Backoff::new(self.contention_bound));
                    backoff.note_holder(self.ops.gate_holder());
                    backoff.wait();
                    match backoff.window_end() {
                        None => {}
                        Some(None) => return Err(Breach::SelfWait),
                        Some(Some(holder)) => tracing::warn!(
                            holder,
                            waited = ?backoff.waited(),
                            "keeper stop: the registry gate stays held"
                        ),
                    }
                }
                Err(error) => return Err(Breach::Rendezvous(error)),
            }
        }
    }

    /// Run collection on `stopped` when `forced` is true or `should_collect`
    /// reports debt at threshold; otherwise run the single empty serial trace.
    /// Shared by `serve_stop` and future callers (such as b5 group GC) that
    /// force a collection before binding a batch.
    fn collect_or_trace(
        &mut self,
        stopped: &mut O::Stopped,
        forced: bool,
    ) -> Result<(), BreachOf<O>> {
        let ops = self.ops;
        let due = forced
            || retry_busy::<O, _>(|| ops.should_collect(stopped)).map_err(Breach::Rendezvous)?;
        if due {
            let report =
                retry_busy::<O, _>(|| ops.collect_stopped(stopped)).map_err(Breach::Rendezvous)?;
            for reply in std::mem::take(&mut self.collect_waiters) {
                let _ = reply.send(Ok(report));
            }
        } else {
            retry_busy::<O, _>(|| O::trace(stopped)).map_err(Breach::Rendezvous)?;
        }
        Ok(())
    }

    /// One stop: collect instead of trace when collection is due (K5), then
    /// lend the ring's head in order, each after the previous ticket came back,
    /// at most the requests queued when the stop began. An empty ring completes
    /// directly without binding a serial batch.
    fn serve_stop(&mut self) -> Result<(), BreachOf<O>> {
        let mut stopped = self.take_stop()?;
        let epoch = O::epoch(&stopped);
        let forced = std::mem::take(&mut self.collect_requested);
        self.collect_or_trace(&mut stopped, forced)?;
        let bound = O::ring_len(&stopped).map_err(Breach::Rendezvous)?;
        if bound == 0 {
            return O::complete_empty(stopped).map_err(Breach::Rendezvous);
        }
        let ops = self.ops;
        ops.with_batch(
            stopped,
            ServeBatch {
                driver: self,
                epoch,
                bound,
            },
        )
    }

    fn serve_batch<B: Batch<O>>(
        &mut self,
        mut batch: B,
        epoch: NonZeroU64,
        bound: usize,
    ) -> Result<(), BreachOf<O>> {
        for _ in 0..bound {
            let Some(head) = batch.head().map_err(Breach::Rendezvous)? else {
                break;
            };
            let step = match head {
                Head::Owner(key) => self.serve_owner(batch, key, epoch)?,
                Head::BatonGroup => self.serve_group(batch, epoch)?,
            };
            batch = match step {
                Step::Next(batch) => batch,
                Step::End(batch) => return batch.complete().map_err(Breach::Rendezvous),
            };
        }
        batch.complete().map_err(Breach::Rendezvous)
    }

    /// Lend an owner head its ticket and settle its return.
    fn serve_owner<B: Batch<O>>(
        &mut self,
        mut batch: B,
        head: O::Key,
        epoch: NonZeroU64,
    ) -> Result<Step<B>, BreachOf<O>> {
        let owner = O::owner(head);
        let Some(receipt) = self.queued.remove(&owner) else {
            return Err(Breach::Unexpected {
                owner,
                message: "a ring entry with no committed request",
            });
        };
        let ready = match batch.ready(receipt) {
            Ok(ready) => ready,
            // The owner published again after its request, so the receipt is
            // superseded. Send it back to publish and requeue at the tail.
            // FIFO is not skipped, so this stop lends nothing more. Any other
            // refusal is a breach.
            Err(error) if O::refusal(error) == Refusal::Superseded => {
                let ops = self.ops;
                retry_busy::<O, _>(|| ops.cancel(head)).map_err(Breach::Rendezvous)?;
                if self.send(owner, GrantMsg::Republish).is_err() {
                    // Gone while only queued: like an exit.
                    self.owners.remove(&owner);
                }
                return Ok(Step::End(batch));
            }
            Err(error) => return Err(Breach::Rendezvous(error)),
        };
        let Some(grant) = batch.grant_for(&ready).map_err(Breach::Rendezvous)? else {
            return Err(Breach::Unexpected {
                owner,
                message: "a ready head the ring did not grant",
            });
        };
        let (ticket, held) = batch
            .lend(ready)
            .map_err(|CustodyRefused| Breach::Custody)?;
        if self.send(owner, GrantMsg::Grant { ticket, epoch }).is_err() {
            // The bounced message drops the ticket, and `held` drops the
            // stop with the breach.
            return Err(Breach::Unreachable { owner });
        }
        let returned = self.await_return(owner)?;
        B::settle(held, returned, grant)
            .map(Step::Next)
            .map_err(|settle| match settle {
                Settle::NotTheTicketCut => Breach::Unexpected {
                    owner,
                    message: "a returned ticket that is not the one cut",
                },
                Settle::Retire(error) => Breach::Rendezvous(error),
            })
    }

    /// Lend a Baton-group head the World and settle its return (L6.7b b3).
    /// The keeper keeps the stop while the turn is out, so no parallel owner
    /// runs during the group's interval (K3).
    fn serve_group<B: Batch<O>>(
        &mut self,
        mut batch: B,
        epoch: NonZeroU64,
    ) -> Result<Step<B>, BreachOf<O>> {
        match &mut self.group {
            Some(group) if group.request == GroupRequest::Queued => {
                group.request = GroupRequest::None;
            }
            _ => {
                return Err(Breach::GroupUnexpected {
                    message: "a group ring entry with no committed request",
                });
            }
        }
        let Some(grant) = batch.grant_group().map_err(Breach::Rendezvous)? else {
            return Err(Breach::GroupUnexpected {
                message: "a group head the ring did not grant",
            });
        };
        let (turn, held) = batch
            .lend_world()
            .map_err(|CustodyRefused| Breach::Custody)?;
        // A client already waiting behind the group's grant ends the
        // interval at its first switch point (QB2).
        self.set_waiter(!self.queued.is_empty() || self.collect_requested);
        if self.send_group(GroupMsg::World { turn, epoch }).is_err() {
            // The bounced message drops the turn, and `held` drops the stop
            // with the breach.
            return Err(Breach::GroupUnreachable);
        }
        let (returned, end) = self.await_world()?;
        self.set_waiter(false);
        let batch = B::settle_world(held, returned, grant).map_err(|settle| match settle {
            Settle::NotTheTicketCut => Breach::GroupUnexpected {
                message: "a returned World turn that is not the one cut",
            },
            Settle::Retire(error) => Breach::Rendezvous(error),
        })?;
        match end {
            IntervalEnd::Done => Ok(Step::Next(batch)),
            // b5 (QB3): the group's turn is retired, so it goes back at the
            // HEAD; this stop lends nothing more, and the next one collects
            // before it grants anyone.
            IntervalEnd::Collect { reply } => {
                let ops = self.ops;
                retry_busy::<O, _>(|| ops.requeue_group_at_head()).map_err(Breach::Rendezvous)?;
                if let Some(group) = &mut self.group {
                    group.request = GroupRequest::Queued;
                }
                self.queue_collect(reply);
                Ok(Step::End(batch))
            }
        }
    }

    /// Wait for the group's World turn. Owners' requests and exits are
    /// committed as they come; anything else is a breach.
    fn await_world(&mut self) -> Result<(O::WorldReturned, IntervalEndOf<O>), BreachOf<O>> {
        loop {
            let message = self
                .inbox
                .recv()
                .map_err(|mpsc::RecvError| Breach::Disconnected)?;
            match message {
                KeeperMsg::WorldReturn { returned, end } => return Ok((returned, end)),
                KeeperMsg::GroupAbandon { cause } => {
                    return Err(Breach::GroupAbandoned { cause });
                }
                KeeperMsg::BatonRequest => {
                    return Err(Breach::GroupUnexpected {
                        message: "a request while its own turn is out",
                    });
                }
                KeeperMsg::GroupExit => {
                    return Err(Breach::GroupUnexpected {
                        message: "an exit while its turn is out",
                    });
                }
                KeeperMsg::Request { owner, receipt } => {
                    self.commit(owner, receipt)?;
                    self.set_waiter(true);
                }
                KeeperMsg::Exit { owner } => self.exit(owner)?,
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
                    drop(ticket);
                    return Err(Breach::Refused { owner, reason });
                }
                KeeperMsg::Abandon { owner, cause } => {
                    return Err(Breach::Abandoned { owner, cause });
                }
                KeeperMsg::Collect { reply } => {
                    self.queue_collect(reply);
                    self.set_waiter(true);
                }
                // A turn is out, so the group cannot be idle (QD2).
                KeeperMsg::IdleQuery { reply } => {
                    let _ = reply.try_send(false);
                }
            }
        }
    }

    /// Wait for `owner`'s ticket. Other owners' requests and exits are
    /// handled as they come; anything else is a breach.
    fn await_return(&mut self, lent: u64) -> Result<O::Returned, BreachOf<O>> {
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
                // The group holds no turn while an owner's ticket is out:
                // it may request or leave, and nothing else.
                KeeperMsg::BatonRequest => self.commit_group()?,
                KeeperMsg::GroupExit => self.group_exit()?,
                KeeperMsg::WorldReturn { .. } => {
                    return Err(Breach::GroupUnexpected {
                        message: "a returned World with no turn out",
                    });
                }
                KeeperMsg::GroupAbandon { cause } => {
                    return Err(Breach::GroupAbandoned { cause });
                }
                KeeperMsg::Collect { reply } => self.queue_collect(reply),
                // An owner's turn is out, so the group cannot be idle (QD2).
                KeeperMsg::IdleQuery { reply } => {
                    let _ = reply.try_send(false);
                }
            }
        }
    }

    /// Poison the era and tell every owner. Dropping a held stop has already
    /// poisoned it when the breach came from inside one.
    fn poison(mut self, breach: BreachOf<O>) -> EndOf<O> {
        self.ops.poison();
        for reply in std::mem::take(&mut self.collect_waiters) {
            let _ = reply.send(Err(CollectRefusal::Poisoned));
        }
        // The ring dies with the era (a poisoned gate refuses every ring
        // call), so queued owners are only told.
        for (owner, _receipt) in std::mem::take(&mut self.queued) {
            let _ = self.send(owner, GrantMsg::Poisoned);
        }
        for enrolled in self.owners.values() {
            let _ = enrolled.grants.try_send(GrantMsg::Poisoned);
        }
        if let Some(group) = &self.group {
            let _ = group.grants.try_send(GroupMsg::Poisoned);
        }
        DriverEnd::Poisoned(breach)
    }
}

/// What one served head leaves the batch with.
enum Step<B> {
    /// The turn came back: serve the next head.
    Next(B),
    /// This stop lends nothing more: an owner was sent back to publish (FIFO
    /// is not skipped), or the group asked for a collection that must run
    /// before any other turn.
    End(B),
}

/// The batch of one stop, run inside the custody `with_batch` binds.
struct ServeBatch<'d, 'o, O: KeeperOps> {
    driver: &'d mut Driver<'o, O>,
    epoch: NonZeroU64,
    bound: usize,
}

impl<O: KeeperOps> BatchVisitor<O> for ServeBatch<'_, '_, O> {
    type Output = ();

    fn visit<B: Batch<O>>(self, batch: B) -> Result<(), BreachOf<O>> {
        self.driver.serve_batch(batch, self.epoch, self.bound)
    }
}

/// Waits on a contended registry gate (lead #13966 LOW-1). The first waits
/// only yield; later ones sleep, doubling up to a millisecond, so a long
/// hold never spins. Both, and the clock that bounds them, go through the
/// sync seam, so the Loom copy schedules them on its own time.
struct Backoff {
    attempts: u32,
    start: sync::Instant,
    bound: Duration,
    window_end: Duration,
    /// The last gate holder seen in the current window.
    holder: Option<NonZeroU64>,
}

impl Backoff {
    /// Waits that only yield before the sleeps start.
    const YIELDS: u32 = 64;

    fn new(bound: Duration) -> Self {
        Self {
            attempts: 0,
            start: sync::Instant::now(),
            bound,
            window_end: bound,
            holder: None,
        }
    }

    fn wait(&mut self) {
        self.attempts = self.attempts.saturating_add(1);
        match self.attempts.checked_sub(Self::YIELDS) {
            None | Some(0) => sync::yield_now(),
            Some(extra) => sync::sleep(Duration::from_micros(1 << extra.min(10))),
        }
    }

    fn waited(&self) -> Duration {
        self.start.elapsed()
    }

    fn note_holder(&mut self, holder: Option<NonZeroU64>) {
        if holder.is_some() {
            self.holder = holder;
        }
    }

    /// Once per bound of waiting: `Some` with the last holder seen in that
    /// window (`None` if the gate was never seen held), then a new window.
    fn window_end(&mut self) -> Option<Option<NonZeroU64>> {
        if self.waited() < self.window_end {
            return None;
        }
        self.window_end += self.bound;
        Some(self.holder.take())
    }
}

/// Retry a refusal that BUSY classifies as contention until
/// [`CONTENTION_BOUND`]; any other result, and contention past the bound,
/// passes through.
pub(crate) fn retry_contended<T, E: Copy>(
    busy: impl Fn(E) -> bool,
    mut attempt: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut backoff = Backoff::new(CONTENTION_BOUND);
    loop {
        match attempt() {
            Err(error) if busy(error) && backoff.waited() < CONTENTION_BOUND => backoff.wait(),
            other => return other,
        }
    }
}

fn retry_busy<O: KeeperOps, T>(
    attempt: impl FnMut() -> Result<T, O::Error>,
) -> Result<T, O::Error> {
    retry_contended(|error| O::refusal(error) == Refusal::Busy, attempt)
}
