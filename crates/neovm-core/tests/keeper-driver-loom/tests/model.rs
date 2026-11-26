// The Loom model the product driver core runs over. `include!`d into one
// module per copy of the core and ring (the real ones and each one-edge
// mutation), next to that module's `control` and `driver_core`.

use self::control::{Mode, Request, RequestId, RingClient, SerialClient, State as Ring};
use self::driver_core::{
    Attempt, Batch, BatchVisitor, Breach, BreachOf, CollectRefusal, CustodyRefused, Driver,
    DriverEnd, GrantMsg, GroupMsg, Head, IntervalEnd, KeeperMsg, KeeperOps, LendDrop, Refusal,
    Settle,
};
use crate::sync::{Signal, mpsc};
use crate::{MutatorKey, RendezvousError};
use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex};
use std::num::NonZeroU64;
use std::time::Duration;

fn key(owner: u64) -> MutatorKey {
    MutatorKey {
        heap: 0,
        world: 0,
        owner,
    }
}

fn generation() -> NonZeroU64 {
    NonZeroU64::new(1).unwrap()
}

/// The Baton group's id in the era's commit and grant records; owners are
/// 1 and up.
const GROUP: u64 = 0;

/// A collection in the era's grant record: what ran, in which stop.
const COLLECTION: u64 = u64::MAX;

/// What the era records: the product ring, the members' modes, the commit
/// order and the grant order.
struct Model {
    ring: Ring,
    members: Vec<(MutatorKey, Mode)>,
    commits: Vec<u64>,
    grants: Vec<(u64, NonZeroU64)>,
}

pub struct ModelEra {
    model: Mutex<Model>,
    /// A member's mode changed, or the ring moved on. Only the keeper waits.
    changed: Signal,
}

impl ModelEra {
    fn new(members: &[(u64, Mode)]) -> Arc<Self> {
        Arc::new(ModelEra {
            model: Mutex::new(Model {
                ring: Ring::new(),
                members: members
                    .iter()
                    .map(|&(owner, mode)| (key(owner), mode))
                    .collect(),
                commits: Vec::new(),
                grants: Vec::new(),
            }),
            changed: Signal::new(),
        })
    }

    /// An owner reaches a poll (or publishes) and stops.
    fn stop_owner(&self, owner: u64) {
        {
            let mut model = self.model.lock().unwrap();
            for member in &mut model.members {
                if member.0 == key(owner) {
                    member.1 = Mode::NativeStopped;
                }
            }
        }
        self.changed.notify();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    owner: u64,
    generation: NonZeroU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    owner: u64,
    epoch: NonZeroU64,
}

pub struct Stop<'a> {
    era: &'a ModelEra,
    epoch: NonZeroU64,
}

pub struct ModelBatch<'a> {
    era: &'a ModelEra,
    epoch: NonZeroU64,
}

pub struct Lent<'a> {
    era: &'a ModelEra,
    ticket: Ticket,
}

/// The Baton group's World turn: it comes back as itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldTurn {
    epoch: NonZeroU64,
}

pub struct WorldLent<'a> {
    era: &'a ModelEra,
    cut: WorldTurn,
}

#[derive(Clone, Copy)]
pub struct Era<'a>(&'a ModelEra);

impl<'a> KeeperOps for Era<'a> {
    type Key = MutatorKey;
    type Receipt = Receipt;
    type Ticket = Ticket;
    type Returned = Ticket;
    type Reason = ();
    type WorldTurn = WorldTurn;
    type WorldReturned = WorldTurn;
    type Error = RendezvousError;
    type Stopped = Stop<'a>;
    type Pending = Vec<MutatorKey>;
    type Report = ();

    fn refusal(error: RendezvousError) -> Refusal {
        match error {
            RendezvousError::Busy => Refusal::Busy,
            RendezvousError::Generation => Refusal::Superseded,
            _ => Refusal::Other,
        }
    }

    fn owner(key: MutatorKey) -> u64 {
        key.owner
    }

    fn generation(receipt: &Receipt) -> NonZeroU64 {
        receipt.generation
    }

    fn enqueue(self, key: MutatorKey, generation: NonZeroU64) -> Result<(), RendezvousError> {
        let mut model = self.0.model.lock().unwrap();
        model.ring.enqueue_serial(key, generation)?;
        model.commits.push(key.owner);
        Ok(())
    }

    fn cancel(self, key: MutatorKey) -> Result<(), RendezvousError> {
        self.0
            .model
            .lock()
            .unwrap()
            .ring
            .cancel_queued(SerialClient::Owner(key))
    }

    fn enqueue_group(self) -> Result<(), RendezvousError> {
        let mut model = self.0.model.lock().unwrap();
        model.ring.enqueue_baton_group()?;
        model.commits.push(GROUP);
        Ok(())
    }

    fn cancel_group(self) -> Result<(), RendezvousError> {
        self.0
            .model
            .lock()
            .unwrap()
            .ring
            .cancel_queued(SerialClient::BatonGroup)
    }

    fn requeue_group_at_head(self) -> Result<(), RendezvousError> {
        self.0
            .model
            .lock()
            .unwrap()
            .ring
            .requeue_baton_group_at_head()
            .map(drop)
    }

    fn requeue_owner_at_head(
        self,
        key: MutatorKey,
        generation: NonZeroU64,
    ) -> Result<(), RendezvousError> {
        let mut model = self.0.model.lock().unwrap();
        model.ring.requeue_serial_at_head(key, generation)?;
        model.commits.push(key.owner);
        Ok(())
    }

    /// Request the stop. When the members are not all stopped yet, wait for
    /// one model change and report the attempt pending: the request stands,
    /// as the product's condvar-bounded attempt leaves it.
    fn stop_within(
        self,
        _bound: Duration,
    ) -> Result<Attempt<Stop<'a>, Vec<MutatorKey>>, RendezvousError> {
        let mut model = self.0.model.lock().unwrap();
        let members = model.members.clone();
        let epoch = match model.ring.request(&members)? {
            Request::Active(epoch) => epoch,
            Request::Queued => return Err(RendezvousError::Busy),
        };
        if model.ring.ready(&members)? {
            model.ring.begin_trace(epoch, &members)?;
            return Ok(Attempt::Stopped(Stop { era: self.0, epoch }));
        }
        drop(model);
        self.0.changed.wait();
        let model = self.0.model.lock().unwrap();
        let waiting = model
            .members
            .iter()
            .filter(|(_, mode)| *mode == Mode::Running)
            .map(|(key, _)| *key)
            .collect();
        Ok(Attempt::Pending(waiting))
    }

    fn gate_holder(self) -> Option<NonZeroU64> {
        None
    }

    fn poison(self) {
        self.0.model.lock().unwrap().ring.poison();
        self.0.changed.notify();
    }

    fn epoch(stopped: &Stop<'a>) -> NonZeroU64 {
        stopped.epoch
    }

    fn trace(_stopped: &mut Stop<'a>) -> Result<(), RendezvousError> {
        Ok(())
    }

    fn should_collect(self, _stopped: &mut Stop<'a>) -> Result<bool, RendezvousError> {
        Ok(false)
    }

    /// The model collects nothing; it records that the stop collected, so a
    /// row can see where the collection fell among the grants.
    fn collect_stopped(self, stopped: &mut Stop<'a>) -> Result<(), RendezvousError> {
        self.0
            .model
            .lock()
            .unwrap()
            .grants
            .push((COLLECTION, stopped.epoch));
        Ok(())
    }

    fn complete_empty(stopped: Stop<'a>) -> Result<(), RendezvousError> {
        let mut model = stopped.era.model.lock().unwrap();
        let members = model.members.clone();
        model.ring.complete(stopped.epoch, &members)?;
        model.ring.finish_debt(&members)?;
        drop(model);
        stopped.era.changed.notify();
        Ok(())
    }

    fn ring_len(stopped: &Stop<'a>) -> Result<usize, RendezvousError> {
        Ok(stopped.era.model.lock().unwrap().ring.ring_len())
    }

    fn with_batch<V: BatchVisitor<Self>>(
        self,
        stopped: Stop<'a>,
        visitor: V,
    ) -> Result<V::Output, BreachOf<Self>> {
        visitor.visit(ModelBatch {
            era: stopped.era,
            epoch: stopped.epoch,
        })
    }
}

impl<'a> Batch<Era<'a>> for ModelBatch<'a> {
    type Ready = Receipt;
    type Grant = RequestId;
    type Held = Lent<'a>;
    type WorldHeld = WorldLent<'a>;

    fn head(&self) -> Result<Option<Head<MutatorKey>>, RendezvousError> {
        let model = self.era.model.lock().unwrap();
        Ok(model
            .ring
            .ring_head_entry()
            .map(|entry| match entry.client() {
                RingClient::Owner { key, .. } => Head::Owner(key),
                RingClient::BatonGroup => Head::BatonGroup,
            }))
    }

    fn grant_group(&mut self) -> Result<Option<RequestId>, RendezvousError> {
        self.era
            .model
            .lock()
            .unwrap()
            .ring
            .grant_head_in_stop(self.epoch, |entry| entry.client() == RingClient::BatonGroup)
    }

    fn lend_world(self) -> Result<(WorldTurn, WorldLent<'a>), CustodyRefused> {
        let turn = WorldTurn { epoch: self.epoch };
        self.era
            .model
            .lock()
            .unwrap()
            .grants
            .push((GROUP, self.epoch));
        Ok((
            turn,
            WorldLent {
                era: self.era,
                cut: turn,
            },
        ))
    }

    fn settle_world(
        held: WorldLent<'a>,
        returned: WorldTurn,
        grant: RequestId,
    ) -> Result<Self, Settle<RendezvousError>> {
        if returned != held.cut {
            return Err(Settle::NotTheTicketCut);
        }
        held.era
            .model
            .lock()
            .unwrap()
            .ring
            .retire_serial(grant)
            .map_err(Settle::Retire)?;
        Ok(ModelBatch {
            era: held.era,
            epoch: held.cut.epoch,
        })
    }

    fn ready(&self, receipt: Receipt) -> Result<Receipt, RendezvousError> {
        Ok(receipt)
    }

    fn grant_for(&mut self, ready: &Receipt) -> Result<Option<RequestId>, RendezvousError> {
        let owner = ready.owner;
        self.era.model.lock().unwrap().ring.grant_head_in_stop(
            self.epoch,
            |entry| matches!(entry.client(), RingClient::Owner { key, .. } if key.owner == owner),
        )
    }

    fn lend(self, ready: Receipt) -> Result<(Ticket, Lent<'a>), CustodyRefused> {
        let ticket = Ticket {
            owner: ready.owner,
            epoch: self.epoch,
        };
        self.era
            .model
            .lock()
            .unwrap()
            .grants
            .push((ready.owner, self.epoch));
        Ok((
            ticket,
            Lent {
                era: self.era,
                ticket,
            },
        ))
    }

    fn settle(
        held: Lent<'a>,
        returned: Ticket,
        grant: RequestId,
    ) -> Result<Self, Settle<RendezvousError>> {
        if returned != held.ticket {
            return Err(Settle::NotTheTicketCut);
        }
        held.era
            .model
            .lock()
            .unwrap()
            .ring
            .retire_serial(grant)
            .map_err(Settle::Retire)?;
        Ok(ModelBatch {
            era: held.era,
            epoch: held.ticket.epoch,
        })
    }

    fn complete(self) -> Result<(), RendezvousError> {
        let mut model = self.era.model.lock().unwrap();
        let members = model.members.clone();
        model.ring.complete(self.epoch, &members)?;
        model.ring.finish_debt(&members)?;
        drop(model);
        self.era.changed.notify();
        Ok(())
    }
}

type Msg = KeeperMsg<Receipt, Ticket, Ticket, (), WorldTurn, ()>;
type Grant = GrantMsg<Ticket>;
type World = GroupMsg<WorldTurn>;

/// What an owner saw when it waited for its grant.
#[derive(Debug, PartialEq, Eq)]
pub enum Seen {
    Granted(NonZeroU64),
    Republish,
    Poisoned,
    Disconnected,
    /// Its grant channel is still open, with nothing in it.
    StillOpen,
}

/// How an owner ends a turn it was granted.
#[derive(Clone, Copy)]
enum OnGrant {
    GiveBack,
    /// The owner collects (L6.7c (c)): the ticket goes back asking for a
    /// collection, the owner asks again under a new publication, and it
    /// gives back the turn it is lent next.
    Collect,
    /// Its lend guard drops armed (K7).
    Abandon,
    /// The probe's own negative: the ticket goes back while the owner is
    /// still in its turn, the earliest a next lend could start. No driver
    /// copy can lend before the return (a `Batch` lends once; see the
    /// `custody_batch_*` fixtures), so this is how the one-turn-out probe is
    /// shown to fail.
    GiveBackEarly,
}

/// A requester, the shape of `ChannelPort::request_turn` plus a turn: request
/// once, wait for the answer, hold the ticket alone, end the turn, exit. A
/// collecting owner requests a second time, after its collection.
fn requester(
    owner: u64,
    inbox: mpsc::Sender<Msg>,
    grants: mpsc::Receiver<Grant>,
    in_turn: Arc<AtomicUsize>,
    on_grant: OnGrant,
    holding: Option<mpsc::Sender<()>>,
) -> Seen {
    let seen = one_turn(
        owner,
        &inbox,
        &grants,
        &in_turn,
        on_grant,
        holding.as_ref(),
        generation(),
    );
    // A collecting owner asks again, under its next publication, and gives
    // back the turn the keeper lends it after the collection.
    let seen = match (on_grant, seen) {
        (OnGrant::Collect, Seen::Granted(_)) => one_turn(
            owner,
            &inbox,
            &grants,
            &in_turn,
            OnGrant::GiveBack,
            None,
            NonZeroU64::new(2).unwrap(),
        ),
        (_, seen) => seen,
    };
    let _ = inbox.send(KeeperMsg::Exit { owner });
    seen
}

/// One request under GENERATION and the turn it is lent, ended as ON_GRANT
/// says.
fn one_turn(
    owner: u64,
    inbox: &mpsc::Sender<Msg>,
    grants: &mpsc::Receiver<Grant>,
    in_turn: &AtomicUsize,
    on_grant: OnGrant,
    holding: Option<&mpsc::Sender<()>>,
    generation: NonZeroU64,
) -> Seen {
    let _ = inbox.send(KeeperMsg::Request {
        owner,
        receipt: Receipt { owner, generation },
    });
    match grants.recv() {
        Ok(GrantMsg::Grant { ticket, epoch }) => {
            // L7: the grant reaches the owner its ticket was cut for.
            assert_eq!(
                ticket.owner, owner,
                "a grant woke an owner it was not cut for"
            );
            // L3/L4/L5: one ticket out, no lend before the last came back.
            assert_eq!(
                in_turn.fetch_add(1, Ordering::SeqCst),
                0,
                "two serial turns at once"
            );
            if let Some(holding) = holding {
                let _ = holding.send(());
            }
            let early = matches!(on_grant, OnGrant::GiveBackEarly);
            if !early {
                in_turn.fetch_sub(1, Ordering::SeqCst);
            }
            let message = match on_grant {
                OnGrant::GiveBack | OnGrant::GiveBackEarly => KeeperMsg::GiveBack {
                    owner,
                    returned: ticket,
                    end: IntervalEnd::Done,
                },
                OnGrant::Collect => KeeperMsg::GiveBack {
                    owner,
                    returned: ticket,
                    end: IntervalEnd::Collect { reply: None },
                },
                OnGrant::Abandon => KeeperMsg::Abandon {
                    owner,
                    cause: LendDrop::Unwound,
                },
            };
            let _ = inbox.send(message);
            if early {
                in_turn.fetch_sub(1, Ordering::SeqCst);
            }
            Seen::Granted(epoch)
        }
        Ok(GrantMsg::Republish) => Seen::Republish,
        Ok(GrantMsg::Poisoned) => Seen::Poisoned,
        Err(_) => Seen::Disconnected,
    }
}

/// How the Baton group ends the interval it was lent.
#[derive(Clone, Copy)]
enum OnWorld {
    GiveBack,
    /// Its lend guard drops armed (K7).
    Abandon,
    /// A Baton thread collects (b5): the turn goes back for the collection,
    /// and the group waits, without asking again, for the World the keeper
    /// lends it next; then it gives that interval back.
    Collect,
    /// The probe's own negative, as [`OnGrant::GiveBackEarly`]: the World
    /// goes back while the group still holds it.
    GiveBackEarly,
}

/// One interval: hold the World alone, then end it as ON_WORLD says.
fn group_interval(
    inbox: &mpsc::Sender<Msg>,
    in_turn: &AtomicUsize,
    turn: WorldTurn,
    on_world: OnWorld,
    holding: Option<&mpsc::Sender<()>>,
) {
    // One turn out at a time across owners and the group (K3).
    assert_eq!(
        in_turn.fetch_add(1, Ordering::SeqCst),
        0,
        "two serial turns at once"
    );
    if let Some(holding) = holding {
        let _ = holding.send(());
    }
    let early = matches!(on_world, OnWorld::GiveBackEarly);
    if !early {
        in_turn.fetch_sub(1, Ordering::SeqCst);
    }
    let message = match on_world {
        OnWorld::GiveBack | OnWorld::GiveBackEarly => KeeperMsg::WorldReturn {
            returned: turn,
            end: IntervalEnd::Done,
        },
        OnWorld::Abandon => KeeperMsg::GroupAbandon {
            cause: LendDrop::Unwound,
        },
        OnWorld::Collect => KeeperMsg::WorldReturn {
            returned: turn,
            end: IntervalEnd::Collect { reply: None },
        },
    };
    let _ = inbox.send(message);
    if early {
        in_turn.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The Baton group's runner, the shape of the seat's request: ask once,
/// wait for the World, hold it alone, end the interval, leave the era.
fn group_runner(
    inbox: mpsc::Sender<Msg>,
    world: mpsc::Receiver<World>,
    in_turn: Arc<AtomicUsize>,
    on_world: OnWorld,
    holding: Option<mpsc::Sender<()>>,
) -> Seen {
    let _ = inbox.send(KeeperMsg::BatonRequest);
    let seen = match world.recv() {
        Ok(GroupMsg::World { turn, epoch }) => {
            group_interval(&inbox, &in_turn, turn, on_world, holding.as_ref());
            if let OnWorld::Collect = on_world {
                match world.recv() {
                    Ok(GroupMsg::World { turn, .. }) => {
                        group_interval(&inbox, &in_turn, turn, OnWorld::GiveBack, None);
                    }
                    Ok(GroupMsg::Poisoned) => return Seen::Poisoned,
                    Err(_) => return Seen::Disconnected,
                }
            }
            Seen::Granted(epoch)
        }
        Ok(GroupMsg::Poisoned) => Seen::Poisoned,
        Err(_) => Seen::Disconnected,
    };
    let _ = inbox.send(KeeperMsg::GroupExit);
    seen
}

/// `Builder::new` reads `LOOM_MAX_PREEMPTIONS`; without it the bound is 2.
/// The env read is explicit so `scripts/ci/run-loom-models.sh` classes this
/// crate as bounded and exports its BOUND here.
fn model(body: impl Fn() + Sync + Send + 'static) {
    let mut builder = loom::model::Builder::new();
    if std::env::var("LOOM_MAX_PREEMPTIONS").is_err() {
        builder.preemption_bound = Some(2);
    }
    builder.check(body);
}

/// L1 + L3/L4/L5 + L7. Two owners request; each is woken only by its own
/// grant, the grants follow commit order, and one ticket is out at a time.
pub fn two_owners_one_turn_each_in_commit_order() {
    two_owners(OnGrant::GiveBack);
}

/// The one-turn-out probe's negative (L3/L4/L5): each owner sends its ticket
/// back while still in its turn, so the next lend can overlap it.
pub fn an_early_give_back_overlaps_two_turns() {
    two_owners(OnGrant::GiveBackEarly);
}

fn two_owners(on_grant: OnGrant) {
    model(move || {
        let era = ModelEra::new(&[(1, Mode::NativeStopped), (2, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let mut owners = Vec::new();
        for owner in [1, 2] {
            let (grants_tx, grants) = mpsc::sync_channel(1);
            driver.enroll(owner, key(owner), grants_tx);
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            owners.push(loom::thread::spawn(move || {
                requester(owner, inbox, grants, in_turn, on_grant, None)
            }));
        }
        drop(inbox);
        let end = driver.run();
        let seen: Vec<Seen> = owners
            .into_iter()
            .map(|owner| owner.join().unwrap())
            .collect();
        assert_eq!(end, DriverEnd::Drained);
        assert!(
            seen.iter().all(|seen| matches!(seen, Seen::Granted(_))),
            "each owner is woken only by its grant: {seen:?}"
        );
        let model = era.model.lock().unwrap();
        let granted: Vec<u64> = model.grants.iter().map(|(owner, _)| *owner).collect();
        assert_eq!(granted, model.commits, "grants follow commit order");
    });
}

/// L8. Owner 1's lend guard drops armed while it holds the ticket. The keeper
/// poisons, and owner 2, enrolled and requesting, reads Poisoned.
pub fn an_abandoned_lend_poisons_and_wakes_the_rest() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped), (2, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (one_tx, one) = mpsc::sync_channel(1);
        let (two_tx, two) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), one_tx);
        driver.enroll(2, key(2), two_tx);
        let (holding_tx, holding) = mpsc::channel::<()>();
        let first = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                requester(1, inbox, one, in_turn, OnGrant::Abandon, Some(holding_tx))
            })
        };
        let second = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                // Owner 2 asks only once owner 1 holds the ticket.
                let _ = holding.recv();
                requester(2, inbox, two, in_turn, OnGrant::GiveBack, None)
            })
        };
        drop(inbox);
        let end = driver.run();
        assert_eq!(
            first.join().unwrap(),
            Seen::Granted(NonZeroU64::new(1).unwrap())
        );
        assert_eq!(
            end,
            DriverEnd::Poisoned(Breach::Abandoned {
                owner: 1,
                cause: LendDrop::Unwound,
            })
        );
        assert_eq!(
            second.join().unwrap(),
            Seen::Poisoned,
            "the queued owner reads Poisoned"
        );
    });
}

/// L9. The keeper dies before it answers. Once it is gone, the owner's
/// grant channel reads disconnected, which `ChannelPort::request_turn` maps
/// to Poisoned, so no owner waits for a keeper that will never answer. With
/// LEAK, one grant sender outlives the keeper: the one edge the row depends
/// on. The owner checks with `try_recv` after a "keeper gone" signal, which
/// reads the same channel state as the port's blocking `recv`, so the
/// negative is an exact assertion rather than a Loom deadlock. The keeper is
/// dropped, as a panic's unwind drops it: an unwind cannot run inside a Loom
/// model, whose threads are coroutines on one OS thread, so the real panic is
/// the eval row `l67a_a_keeper_panic_answers_the_waiting_owner_poisoned`.
pub fn a_dead_keeper_disconnects_every_owner(leak: bool) {
    model(move || {
        let era = ModelEra::new(&[(1, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let (grants_tx, grants) = mpsc::sync_channel(1);
        let leaked = leak.then(|| grants_tx.clone());
        driver.enroll(1, key(1), grants_tx);
        let (gone_tx, gone) = mpsc::channel::<()>();
        let owner = loom::thread::spawn(move || {
            let _ = inbox.send(KeeperMsg::Request {
                owner: 1,
                receipt: Receipt {
                    owner: 1,
                    generation: generation(),
                },
            });
            let _ = gone.recv();
            match grants.try_recv() {
                Ok(GrantMsg::Grant { epoch, .. }) => Seen::Granted(epoch),
                Ok(GrantMsg::Republish) => Seen::Republish,
                Ok(GrantMsg::Poisoned) => Seen::Poisoned,
                Err(mpsc::TryRecvError::Disconnected) => Seen::Disconnected,
                Err(mpsc::TryRecvError::Empty) => Seen::StillOpen,
            }
        });
        // The keeper dies without serving.
        drop(driver);
        let _ = gone_tx.send(());
        assert_eq!(
            owner.join().unwrap(),
            Seen::Disconnected,
            "the owner learns the keeper is gone"
        );
        drop(leaked);
    });
}

/// K8 r2. Owner 2 is running and has not met the stop owner 1's request
/// started. The attempt comes back pending and the request stands. Once
/// owner 2 stops, owner 1 is lent its turn: a slow owner delays the turn and
/// poisons nothing.
pub fn a_slow_owner_delays_the_turn_and_poisons_nothing() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped), (2, Mode::Running)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let (grants_tx, grants) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), grants_tx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let requesting = {
            let inbox = inbox.clone();
            loom::thread::spawn(move || {
                requester(1, inbox, grants, in_turn, OnGrant::GiveBack, None)
            })
        };
        let slow = {
            let era = era.clone();
            loom::thread::spawn(move || era.stop_owner(2))
        };
        drop(inbox);
        let end = driver.run();
        slow.join().unwrap();
        assert_eq!(end, DriverEnd::Drained, "a slow owner poisons nothing");
        assert!(matches!(requesting.join().unwrap(), Seen::Granted(_)));
    });
}

/// L10 (L6.7b b3). The Baton group and an owner each request once. Each is
/// lent its turn in commit order across the two kinds, and one turn is out
/// at a time: the World turn excludes an owner's ticket.
pub fn a_group_and_an_owner_alternate_in_commit_order() {
    a_group_and_an_owner(OnWorld::GiveBack);
}

/// The plan's b3 negative: the World goes back while the group still holds
/// it, so an owner lent next overlaps the group's interval.
pub fn an_early_world_return_overlaps_the_owner() {
    a_group_and_an_owner(OnWorld::GiveBackEarly);
}

fn a_group_and_an_owner(on_world: OnWorld) {
    model(move || {
        let era = ModelEra::new(&[(1, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (grants_tx, grants) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), grants_tx);
        let (world, _waiter) = driver.enroll_group().into_parts();
        let owner = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                requester(1, inbox, grants, in_turn, OnGrant::GiveBack, None)
            })
        };
        let group = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || group_runner(inbox, world, in_turn, on_world, None))
        };
        drop(inbox);
        let end = driver.run();
        let owner = owner.join().unwrap();
        let group = group.join().unwrap();
        assert_eq!(end, DriverEnd::Drained);
        assert!(matches!(owner, Seen::Granted(_)), "the owner: {owner:?}");
        assert!(matches!(group, Seen::Granted(_)), "the group: {group:?}");
        let model = era.model.lock().unwrap();
        let granted: Vec<u64> = model.grants.iter().map(|(client, _)| *client).collect();
        assert_eq!(granted, model.commits, "grants follow commit order");
    });
}

/// L11 (K7 for the group). The group's lend guard drops armed while it
/// holds the World. The keeper poisons, and the owner, enrolled and
/// requesting, reads Poisoned.
pub fn an_abandoned_world_turn_poisons_the_owners() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (grants_tx, grants) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), grants_tx);
        let (world, _waiter) = driver.enroll_group().into_parts();
        let (holding_tx, holding) = mpsc::channel::<()>();
        let group = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                group_runner(inbox, world, in_turn, OnWorld::Abandon, Some(holding_tx))
            })
        };
        let owner = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                // The owner asks only once the group holds the World.
                let _ = holding.recv();
                requester(1, inbox, grants, in_turn, OnGrant::GiveBack, None)
            })
        };
        drop(inbox);
        let end = driver.run();
        assert_eq!(
            group.join().unwrap(),
            Seen::Granted(NonZeroU64::new(1).unwrap())
        );
        assert_eq!(
            end,
            DriverEnd::Poisoned(Breach::GroupAbandoned {
                cause: LendDrop::Unwound,
            })
        );
        assert_eq!(
            owner.join().unwrap(),
            Seen::Poisoned,
            "the queued owner reads Poisoned"
        );
    });
}

/// L12. An owner's lend guard drops armed while the group's request is
/// queued behind it. The keeper poisons, and the group reads Poisoned on its
/// own channel.
pub fn an_abandoned_lend_poisons_the_queued_group() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (grants_tx, grants) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), grants_tx);
        let (world, _waiter) = driver.enroll_group().into_parts();
        let (holding_tx, holding) = mpsc::channel::<()>();
        let owner = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                requester(
                    1,
                    inbox,
                    grants,
                    in_turn,
                    OnGrant::Abandon,
                    Some(holding_tx),
                )
            })
        };
        let group = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                // The group asks only once the owner holds its ticket.
                let _ = holding.recv();
                group_runner(inbox, world, in_turn, OnWorld::GiveBack, None)
            })
        };
        drop(inbox);
        let end = driver.run();
        assert_eq!(
            owner.join().unwrap(),
            Seen::Granted(NonZeroU64::new(1).unwrap())
        );
        assert_eq!(
            end,
            DriverEnd::Poisoned(Breach::Abandoned {
                owner: 1,
                cause: LendDrop::Unwound,
            })
        );
        assert_eq!(
            group.join().unwrap(),
            Seen::Poisoned,
            "the queued group reads Poisoned"
        );
    });
}

/// L13 (L6.7b b5, QB3). The group's turn comes back asking for a
/// collection while an owner is queued behind it. The keeper ends that stop,
/// collects first in the next one, and lends the group the World again
/// before the owner: the collecting group is the next World user, and no
/// other client's turn comes between its collection and its resume.
pub fn a_collecting_group_resumes_before_any_other_turn() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (grants_tx, grants) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), grants_tx);
        let (world, _waiter) = driver.enroll_group().into_parts();
        let (holding_tx, holding) = mpsc::channel::<()>();
        let group = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                group_runner(inbox, world, in_turn, OnWorld::Collect, Some(holding_tx))
            })
        };
        let owner = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                // The owner asks only once the group holds the World.
                let _ = holding.recv();
                requester(1, inbox, grants, in_turn, OnGrant::GiveBack, None)
            })
        };
        drop(inbox);
        let end = driver.run();
        assert!(matches!(group.join().unwrap(), Seen::Granted(_)));
        assert!(matches!(owner.join().unwrap(), Seen::Granted(_)));
        assert_eq!(end, DriverEnd::Drained);
        let model = era.model.lock().unwrap();
        let clients: Vec<u64> = model.grants.iter().map(|(client, _)| *client).collect();
        assert_eq!(
            clients,
            [GROUP, COLLECTION, GROUP, 1],
            "the collecting group resumes next"
        );
        assert_eq!(
            model.grants[1].1, model.grants[2].1,
            "the collection and the group's resume ride one stop"
        );
    });
}

/// How many switch points the group's runner polls the waiter word at in
/// L14 before it calls the word never raised.
const WAIT_YIELDS: usize = 8;

/// L14 (L6.7b b4.1, QB2). The group holds the World when the owner asks.
/// The keeper raises the group's waiter word; the group, polling it at its
/// switch points, ends the interval; the owner is lent next. Nobody waited
/// when the World was lent, so the interval starts with the word lowered.
pub fn an_owner_request_during_the_world_raises_the_waiter_word() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (grants_tx, grants) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), grants_tx);
        let (world, waiter) = driver.enroll_group().into_parts();
        let (holding_tx, holding) = mpsc::channel::<()>();
        let group = {
            let inbox = inbox.clone();
            loom::thread::spawn(move || {
                let _ = inbox.send(KeeperMsg::BatonRequest);
                let seen = match world.recv() {
                    Ok(GroupMsg::World { turn, epoch }) => {
                        assert!(
                            !waiter.raised(),
                            "an interval nobody waits behind starts lowered"
                        );
                        let _ = holding_tx.send(());
                        let raised = (0..WAIT_YIELDS).any(|_| {
                            let raised = waiter.raised();
                            if !raised {
                                loom::thread::yield_now();
                            }
                            raised
                        });
                        assert!(raised, "the waiter word was never raised");
                        let _ = inbox.send(KeeperMsg::WorldReturn {
                            returned: turn,
                            end: IntervalEnd::Done,
                        });
                        Seen::Granted(epoch)
                    }
                    Ok(GroupMsg::Poisoned) => Seen::Poisoned,
                    Err(_) => Seen::Disconnected,
                };
                let _ = inbox.send(KeeperMsg::GroupExit);
                seen
            })
        };
        let owner = {
            let inbox = inbox.clone();
            loom::thread::spawn(move || {
                // The owner asks only once the group holds the World.
                let _ = holding.recv();
                requester(1, inbox, grants, in_turn, OnGrant::GiveBack, None)
            })
        };
        drop(inbox);
        let end = driver.run();
        let group = group.join().unwrap();
        let owner = owner.join().unwrap();
        assert_eq!(end, DriverEnd::Drained);
        assert!(matches!(group, Seen::Granted(_)), "the group: {group:?}");
        assert!(matches!(owner, Seen::Granted(_)), "the owner: {owner:?}");
        let model = era.model.lock().unwrap();
        let granted: Vec<u64> = model.grants.iter().map(|(client, _)| *client).collect();
        assert_eq!(granted, [GROUP, 1], "the owner is lent after the group's interval");
    });
}

/// L15 (L6.7c (c)). Owner 1's turn comes back asking for a collection while
/// owner 2 is queued behind it. The keeper holds the HEAD for owner 1's next
/// request and runs no stop until it comes; that stop collects first and
/// lends owner 1 its turn before owner 2's: the collecting owner resumes
/// next, and its collection traces the publication it made after its turn.
pub fn a_collecting_owner_resumes_before_any_other_turn() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped), (2, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (one_tx, one) = mpsc::sync_channel(1);
        let (two_tx, two) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), one_tx);
        driver.enroll(2, key(2), two_tx);
        let (holding_tx, holding) = mpsc::channel::<()>();
        let first = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                requester(1, inbox, one, in_turn, OnGrant::Collect, Some(holding_tx))
            })
        };
        let second = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                // Owner 2 asks only once owner 1 holds its first ticket.
                let _ = holding.recv();
                requester(2, inbox, two, in_turn, OnGrant::GiveBack, None)
            })
        };
        drop(inbox);
        let end = driver.run();
        // The order is the witness: a keeper that lends owner 2 first either
        // shows it here or refuses owner 1's HEAD commit behind owner 2's
        // granted turn and poisons, which shows here too.
        let grants = era.model.lock().unwrap().grants.clone();
        let clients: Vec<u64> = grants.iter().map(|(client, _)| *client).collect();
        assert_eq!(
            clients,
            [1, COLLECTION, 1, 2],
            "the collecting owner resumes next"
        );
        assert_eq!(
            grants[1].1, grants[2].1,
            "the collection and the owner's resume ride one stop"
        );
        assert_eq!(end, DriverEnd::Drained);
        assert!(matches!(first.join().unwrap(), Seen::Granted(_)));
        assert!(matches!(second.join().unwrap(), Seen::Granted(_)));
    });
}

/// L16 (L6.7c (c)). Owner 1's turn comes back asking for a collection, and
/// owner 1 exits instead of asking again. Its exit gives up the HEAD; the
/// collection it asked for still runs, before owner 2's turn, and the era
/// drains.
pub fn a_collecting_owner_that_exits_gives_up_the_head() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped), (2, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let in_turn = Arc::new(AtomicUsize::new(0));
        let (one_tx, one) = mpsc::sync_channel(1);
        let (two_tx, two) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), one_tx);
        driver.enroll(2, key(2), two_tx);
        let (holding_tx, holding) = mpsc::channel::<()>();
        let first = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                let seen = one_turn(
                    1,
                    &inbox,
                    &one,
                    &in_turn,
                    OnGrant::Collect,
                    Some(&holding_tx),
                    generation(),
                );
                let _ = inbox.send(KeeperMsg::Exit { owner: 1 });
                seen
            })
        };
        let second = {
            let inbox = inbox.clone();
            let in_turn = in_turn.clone();
            loom::thread::spawn(move || {
                let _ = holding.recv();
                requester(2, inbox, two, in_turn, OnGrant::GiveBack, None)
            })
        };
        drop(inbox);
        let end = driver.run();
        assert!(matches!(first.join().unwrap(), Seen::Granted(_)));
        assert!(matches!(second.join().unwrap(), Seen::Granted(_)));
        assert_eq!(end, DriverEnd::Drained, "an exit gives up the HEAD");
        let model = era.model.lock().unwrap();
        let clients: Vec<u64> = model.grants.iter().map(|(client, _)| *client).collect();
        assert_eq!(
            clients,
            [1, COLLECTION, 2],
            "the collection still runs, before owner 2's turn"
        );
    });
}

/// L17 (L6.7c (c) F2). A collecting owner whose re-request is refused by the
/// ring must still get its held collect reply answered `Poisoned`, not
/// silently dropped: `Driver::commit` takes the head_hold, and when
/// `requeue_owner_at_head` fails it answers the reply before the breach.
pub fn a_failed_owner_requeue_answers_the_collect_reply() {
    model(|| {
        let era = ModelEra::new(&[(1, Mode::NativeStopped)]);
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(Era(&era), inbox_rx);
        let (grants_tx, grants) = mpsc::sync_channel(1);
        driver.enroll(1, key(1), grants_tx);
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let owner_inbox = inbox.clone();
        let owner = loom::thread::spawn(move || {
            let _ = owner_inbox.send(KeeperMsg::Request {
                owner: 1,
                receipt: Receipt { owner: 1, generation: generation() },
            });
            let Ok(GrantMsg::Grant { ticket, .. }) = grants.recv() else {
                panic!("owner 1 is granted");
            };
            // Collect carrying the reply, then re-request. The re-request's
            // HEAD commit is refused by the ring (N-L17), so the driver must
            // answer the reply Poisoned.
            let _ = owner_inbox.send(KeeperMsg::GiveBack {
                owner: 1,
                returned: ticket,
                end: IntervalEnd::Collect { reply: Some(reply_tx) },
            });
            let _ = owner_inbox.send(KeeperMsg::Request {
                owner: 1,
                receipt: Receipt { owner: 1, generation: NonZeroU64::new(2).unwrap() },
            });
            let seen = match reply_rx.recv() {
                Ok(Err(CollectRefusal::Poisoned)) => Seen::Poisoned,
                other => panic!("the collect reply must be answered Poisoned: {other:?}"),
            };
            let _ = owner_inbox.send(KeeperMsg::Exit { owner: 1 });
            seen
        });
        drop(inbox);
        let end = driver.run();
        assert_eq!(
            owner.join().unwrap(),
            Seen::Poisoned,
            "the collect reply reaches the owner as Poisoned"
        );
        assert!(
            matches!(end, DriverEnd::Poisoned(Breach::Rendezvous(RendezvousError::Capacity))),
            "a refused re-commit poisons the era: {end:?}"
        );
    });
}
