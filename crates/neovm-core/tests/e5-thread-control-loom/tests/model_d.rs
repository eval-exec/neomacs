//! Loom model (d): cross-kind wake into an idle Baton group (Q1 ruling)
//! driving the REAL copied `parallel_entry/driver_core.rs`,
//! `rendezvous/control.rs`, and `green/scheduler/wait_core.rs` together with
//! `coalesce_request_locked` (`GroupTurn::{Idle, Requested, Holding}`).

use crate::model_with_bound;
use crate::sync::{self, Arc, Signal, mpsc};
use crate::{MutatorKey, RendezvousError};
use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicUsize as StdAtomicUsize, Ordering as StdOrdering};
use std::time::Duration;

#[allow(dead_code)]
mod control {
    include!(concat!(env!("OUT_DIR"), "/control.rs"));
}

#[allow(dead_code)]
mod driver_core {
    include!(concat!(env!("OUT_DIR"), "/driver_core.rs"));
}

#[allow(dead_code)]
mod wait_core {
    include!(concat!(env!("OUT_DIR"), "/wait_core.rs"));
}

use control::{Mode, Request, RequestId, RingClient, SerialClient, State as Ring};
use driver_core::{
    Attempt, Batch, BatchVisitor, BreachOf, CustodyRefused, Driver, DriverEnd, GrantMsg, GroupMsg,
    Head, IntervalEnd, KeeperMsg, KeeperOps, Refusal, Settle,
};
use wait_core::{ParallelParticipant, Participant};

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

// ---------------------------------------------------------------------------
// Minimal model era for `driver_core::Driver` + `control::State`
// ---------------------------------------------------------------------------

struct EraState {
    ring: Ring,
    members: Vec<(MutatorKey, Mode)>,
    group_enqueues: usize,
    duplicate_group_busy: usize,
    active_turns: usize,
}

struct ModelEra {
    state: loom::sync::Mutex<EraState>,
    changed: Signal,
}

impl ModelEra {
    fn new(members: &[(u64, Mode)]) -> Arc<Self> {
        Arc::new(Self {
            state: loom::sync::Mutex::new(EraState {
                ring: Ring::new(),
                members: members.iter().map(|&(o, m)| (key(o), m)).collect(),
                group_enqueues: 0,
                duplicate_group_busy: 0,
                active_turns: 0,
            }),
            changed: Signal::new(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Receipt {
    owner: u64,
    generation: NonZeroU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ticket {
    owner: u64,
    epoch: NonZeroU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorldTurn {
    epoch: NonZeroU64,
}

struct Stop<'a> {
    era: &'a ModelEra,
    epoch: NonZeroU64,
}

struct ModelBatch<'a> {
    era: &'a ModelEra,
    epoch: NonZeroU64,
}

struct Lent<'a> {
    era: &'a ModelEra,
    ticket: Ticket,
}

struct WorldLent<'a> {
    era: &'a ModelEra,
    cut: WorldTurn,
}

#[derive(Clone, Copy)]
struct Era<'a>(&'a ModelEra);

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
        self.0
            .state
            .lock()
            .unwrap()
            .ring
            .enqueue_serial(key, generation)
            .map(drop)
    }

    fn cancel(self, key: MutatorKey) -> Result<(), RendezvousError> {
        self.0
            .state
            .lock()
            .unwrap()
            .ring
            .cancel_queued(SerialClient::Owner(key))
    }

    fn enqueue_group(self) -> Result<(), RendezvousError> {
        let mut s = self.0.state.lock().unwrap();
        match s.ring.enqueue_baton_group() {
            Ok(_) => {
                s.group_enqueues += 1;
                Ok(())
            }
            Err(RendezvousError::Busy) => {
                s.duplicate_group_busy += 1;
                Err(RendezvousError::Busy)
            }
            Err(e) => Err(e),
        }
    }

    fn cancel_group(self) -> Result<(), RendezvousError> {
        self.0
            .state
            .lock()
            .unwrap()
            .ring
            .cancel_queued(SerialClient::BatonGroup)
    }

    fn requeue_group_at_head(self) -> Result<(), RendezvousError> {
        self.0
            .state
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
        self.0
            .state
            .lock()
            .unwrap()
            .ring
            .requeue_serial_at_head(key, generation)
            .map(drop)
    }

    fn stop_within(
        self,
        _bound: Duration,
    ) -> Result<Attempt<Stop<'a>, Vec<MutatorKey>>, RendezvousError> {
        let mut s = self.0.state.lock().unwrap();
        let members = s.members.clone();
        let epoch = match s.ring.request(&members)? {
            Request::Active(epoch) => epoch,
            Request::Queued => return Err(RendezvousError::Busy),
        };
        if s.ring.ready(&members)? {
            s.ring.begin_trace(epoch, &members)?;
            return Ok(Attempt::Stopped(Stop { era: self.0, epoch }));
        }
        drop(s);
        self.0.changed.wait();
        let s = self.0.state.lock().unwrap();
        let waiting = s
            .members
            .iter()
            .filter(|(_, m)| *m == Mode::Running)
            .map(|(k, _)| *k)
            .collect();
        Ok(Attempt::Pending(waiting))
    }

    fn gate_holder(self) -> Option<NonZeroU64> {
        None
    }

    fn poison(self) {
        self.0.state.lock().unwrap().ring.poison();
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

    fn collect_stopped(self, _stopped: &mut Stop<'a>) -> Result<(), RendezvousError> {
        Ok(())
    }

    fn complete_empty(stopped: Stop<'a>) -> Result<(), RendezvousError> {
        let mut s = stopped.era.state.lock().unwrap();
        let members = s.members.clone();
        s.ring.complete(stopped.epoch, &members)?;
        s.ring.finish_debt(&members)?;
        drop(s);
        stopped.era.changed.notify();
        Ok(())
    }

    fn ring_len(stopped: &Stop<'a>) -> Result<usize, RendezvousError> {
        Ok(stopped.era.state.lock().unwrap().ring.ring_len())
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
        let s = self.era.state.lock().unwrap();
        Ok(s.ring.ring_head_entry().map(|entry| match entry.client() {
            RingClient::Owner { key, .. } => Head::Owner(key),
            RingClient::BatonGroup => Head::BatonGroup,
        }))
    }

    fn grant_group(&mut self) -> Result<Option<RequestId>, RendezvousError> {
        self.era
            .state
            .lock()
            .unwrap()
            .ring
            .grant_head_in_stop(self.epoch, |e| e.client() == RingClient::BatonGroup)
    }

    fn lend_world(self) -> Result<(WorldTurn, WorldLent<'a>), CustodyRefused> {
        let mut s = self.era.state.lock().unwrap();
        s.active_turns += 1;
        assert_eq!(s.active_turns, 1, "two serial turns at once");
        let turn = WorldTurn { epoch: self.epoch };
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
        let mut s = held.era.state.lock().unwrap();
        assert_eq!(s.active_turns, 1);
        s.active_turns = 0;
        s.ring.retire_serial(grant).map_err(Settle::Retire)?;
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
        self.era.state.lock().unwrap().ring.grant_head_in_stop(
            self.epoch,
            |e| matches!(e.client(), RingClient::Owner { key, .. } if key.owner == owner),
        )
    }

    fn lend(self, ready: Receipt) -> Result<(Ticket, Lent<'a>), CustodyRefused> {
        let mut s = self.era.state.lock().unwrap();
        s.active_turns += 1;
        assert_eq!(s.active_turns, 1, "two serial turns at once");
        let ticket = Ticket {
            owner: ready.owner,
            epoch: self.epoch,
        };
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
        let mut s = held.era.state.lock().unwrap();
        assert_eq!(s.active_turns, 1);
        s.active_turns = 0;
        s.ring.retire_serial(grant).map_err(Settle::Retire)?;
        Ok(ModelBatch {
            era: held.era,
            epoch: held.ticket.epoch,
        })
    }

    fn complete(self) -> Result<(), RendezvousError> {
        let mut s = self.era.state.lock().unwrap();
        let members = s.members.clone();
        s.ring.complete(self.epoch, &members)?;
        s.ring.finish_debt(&members)?;
        drop(s);
        self.era.changed.notify();
        Ok(())
    }
}

type Msg = KeeperMsg<Receipt, Ticket, Ticket, (), WorldTurn, ()>;

// ---------------------------------------------------------------------------
// Scheduler `Control` model mirroring `green/scheduler.rs:3850-3884, 4415-4429`
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupTurn {
    Idle,
    Requested,
    Holding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoalesceMode {
    /// Real product `coalesce_request_locked`: send ONE `BatonRequest` only
    /// on `GroupTurn::Idle -> GroupTurn::Requested`.
    Real,
    /// Negative 1: omit `GroupTurn::Idle` guard, sending a duplicate
    /// `BatonRequest` on subsequent wakes while `Requested`.
    MissingIdleGuard,
    /// Negative 2: omit `coalesce_request_locked`, leaving woken Baton threads
    /// stranded in the ready queue with `GroupTurn::Idle`.
    DroppedCoalesce,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BatonWait {
    Condition(u64),
    Runnable,
    Done,
}

struct ControlModelState {
    participants: BTreeMap<u64, Participant<()>>,
    waits: BTreeMap<u64, BatonWait>,
    ready: VecDeque<u64>,
    turn: GroupTurn,
    baton_requests_sent: usize,
    duplicate_suppressed: usize,
    requester: Option<mpsc::Sender<Msg>>,
    mode: CoalesceMode,
}

struct ControlModel(loom::sync::Mutex<ControlModelState>);

impl ControlModel {
    fn new(
        baton_threads: &[u64],
        parallel_owner: u64,
        requester: mpsc::Sender<Msg>,
        mode: CoalesceMode,
    ) -> Arc<Self> {
        let mut participants = BTreeMap::new();
        let mut waits = BTreeMap::new();
        for &tid in baton_threads {
            let (tx, _rx) = mpsc::sync_channel(1);
            participants.insert(tid, Participant::Baton(tx));
            waits.insert(tid, BatonWait::Condition(1));
        }
        participants.insert(
            parallel_owner,
            Participant::Parallel(ParallelParticipant::new()),
        );
        Arc::new(Self(loom::sync::Mutex::new(ControlModelState {
            participants,
            waits,
            ready: VecDeque::new(),
            turn: GroupTurn::Idle,
            baton_requests_sent: 0,
            duplicate_suppressed: 0,
            requester: Some(requester),
            mode,
        })))
    }

    fn close_requester(&self) {
        self.0.lock().unwrap().requester = None;
    }

    /// Exact mirror of `Control::coalesce_request_locked` (`green/scheduler.rs:4415-4429`).
    fn coalesce_request_locked(s: &mut ControlModelState) -> bool {
        match s.mode {
            CoalesceMode::Real => {
                if s.turn == GroupTurn::Idle {
                    if let Some(requester) = &s.requester {
                        if requester.send(KeeperMsg::BatonRequest).is_ok() {
                            s.turn = GroupTurn::Requested;
                            s.baton_requests_sent += 1;
                            return true;
                        }
                    }
                } else {
                    s.duplicate_suppressed += 1;
                }
                false
            }
            CoalesceMode::MissingIdleGuard => {
                if let Some(requester) = &s.requester {
                    if requester.send(KeeperMsg::BatonRequest).is_ok() {
                        s.turn = GroupTurn::Requested;
                        s.baton_requests_sent += 1;
                        return true;
                    }
                }
                false
            }
            CoalesceMode::DroppedCoalesce => false,
        }
    }

    /// Exact mirror of `Control::wake_waiting_for(Wait::Condition(cond))`
    /// waking every Baton thread parked on `cond` into `ready` FIFO and
    /// coalescing into ONE `BatonRequest`.
    fn wake_condition_all(&self, cond: u64) -> usize {
        let mut s = self.0.lock().unwrap();
        let targets: Vec<u64> = s
            .waits
            .iter()
            .filter_map(|(&tid, &w)| (w == BatonWait::Condition(cond)).then_some(tid))
            .collect();
        let mut woken = 0;
        for tid in targets {
            if s.participants.get(&tid).is_some_and(Participant::is_baton) {
                s.waits.insert(tid, BatonWait::Runnable);
                s.ready.push_back(tid);
                Self::coalesce_request_locked(&mut s);
                woken += 1;
            }
        }
        woken
    }

    fn enter_group_holding_and_drain(&self) -> Vec<u64> {
        let mut s = self.0.lock().unwrap();
        assert_eq!(
            s.turn,
            GroupTurn::Requested,
            "Baton group granted World while GroupTurn was not Requested"
        );
        s.turn = GroupTurn::Holding;
        let mut ran = Vec::new();
        while let Some(tid) = s.ready.pop_front() {
            assert_eq!(s.waits.get(&tid), Some(&BatonWait::Runnable));
            s.waits.insert(tid, BatonWait::Done);
            ran.push(tid);
        }
        s.turn = GroupTurn::Idle;
        ran
    }
}

pub(crate) struct CoverageD {
    pub(crate) executions: std::sync::Arc<StdAtomicUsize>,
    pub(crate) parallel_turn_granted: std::sync::Arc<StdAtomicUsize>,
    pub(crate) group_turn_granted: std::sync::Arc<StdAtomicUsize>,
    pub(crate) coalesced_idle_to_requested: std::sync::Arc<StdAtomicUsize>,
    pub(crate) coalesced_suppressed_duplicate: std::sync::Arc<StdAtomicUsize>,
    pub(crate) baton_threads_completed: std::sync::Arc<StdAtomicUsize>,
}

fn run_model_d_impl(mode: CoalesceMode) -> CoverageD {
    let executions = std::sync::Arc::new(StdAtomicUsize::new(0));
    let parallel_turn_granted = std::sync::Arc::new(StdAtomicUsize::new(0));
    let group_turn_granted = std::sync::Arc::new(StdAtomicUsize::new(0));
    let coalesced_idle_to_requested = std::sync::Arc::new(StdAtomicUsize::new(0));
    let coalesced_suppressed_duplicate = std::sync::Arc::new(StdAtomicUsize::new(0));
    let baton_threads_completed = std::sync::Arc::new(StdAtomicUsize::new(0));

    let c_exec = std::sync::Arc::clone(&executions);
    let c_pturn = std::sync::Arc::clone(&parallel_turn_granted);
    let c_gturn = std::sync::Arc::clone(&group_turn_granted);
    let c_idle = std::sync::Arc::clone(&coalesced_idle_to_requested);
    let c_dup = std::sync::Arc::clone(&coalesced_suppressed_duplicate);
    let c_done = std::sync::Arc::clone(&baton_threads_completed);

    model_with_bound(move || {
        c_exec.fetch_add(1, StdOrdering::Relaxed);
        let parallel_owner = 1u64;
        let baton_t1 = 10u64;
        let baton_t2 = 11u64;

        let era = ModelEra::new(&[(parallel_owner, Mode::NativeStopped)]);
        let (inbox_tx, inbox_rx) = mpsc::channel::<Msg>();
        let (owner_grant_tx, owner_grant_rx) = mpsc::sync_channel::<GrantMsg<Ticket>>(1);

        let control = ControlModel::new(
            &[baton_t1, baton_t2],
            parallel_owner,
            inbox_tx.clone(),
            mode,
        );

        let mut driver: Driver<'_, Era<'_>> = Driver::new(Era(&*era), inbox_rx);
        driver.enroll(parallel_owner, key(parallel_owner), owner_grant_tx);
        let (group_grant_rx, _group_waiter) = driver.enroll_group().into_parts();

        // Parallel owner P: requests a serial turn on real `driver_core`, receives
        // its `Ticket`, wakes TWO parked Baton threads (T1 & T2) while holding
        // its ticket (so `driver_core::await_return` receives the coalesced
        // `BatonRequest` while P's ticket is out!), then returns its ticket and exits.
        let p_inbox = inbox_tx.clone();
        let p_control = Arc::clone(&control);
        let c_pturn_p = std::sync::Arc::clone(&c_pturn);
        let c_idle_p = std::sync::Arc::clone(&c_idle);
        let c_dup_p = std::sync::Arc::clone(&c_dup);
        let tp = loom::thread::spawn(move || {
            p_inbox
                .send(KeeperMsg::Request {
                    owner: parallel_owner,
                    receipt: Receipt {
                        owner: parallel_owner,
                        generation: generation(),
                    },
                })
                .unwrap();
            let ticket = match owner_grant_rx.recv().unwrap() {
                GrantMsg::Grant { ticket, .. } => ticket,
                GrantMsg::Republish => panic!("unexpected Republish for parallel owner"),
                GrantMsg::Poisoned => panic!("unexpected Poisoned for parallel owner"),
            };
            c_pturn_p.fetch_add(1, StdOrdering::Relaxed);

            // Cross-kind wake into idle Baton group: wakes BOTH T1 and T2.
            let woken = p_control.wake_condition_all(1);
            assert_eq!(woken, 2, "both parked Baton threads must be woken");
            {
                let s = p_control.0.lock().unwrap();
                if s.baton_requests_sent >= 1 {
                    c_idle_p.fetch_add(1, StdOrdering::Relaxed);
                }
                if s.duplicate_suppressed >= 1 {
                    c_dup_p.fetch_add(1, StdOrdering::Relaxed);
                }
                assert!(
                    s.baton_requests_sent <= 1,
                    "duplicate BatonRequest sent ({}) while GroupTurn was already Requested",
                    s.baton_requests_sent
                );
            }

            p_inbox
                .send(KeeperMsg::GiveBack {
                    owner: parallel_owner,
                    returned: ticket,
                    end: IntervalEnd::Done,
                })
                .unwrap();
            p_inbox
                .send(KeeperMsg::Exit {
                    owner: parallel_owner,
                })
                .unwrap();
            p_control.close_requester();
            if mode == CoalesceMode::DroppedCoalesce {
                // In the negative row, P finished without any BatonRequest
                // being sent; send GroupExit so keeper drains cleanly and
                // trips the exact post-condition witness instead of a Loom
                // deadlock unwind.
                let _ = p_inbox.send(KeeperMsg::GroupExit);
            }
        });

        // Baton group thread: starts `GroupTurn::Idle`, waits on real
        // `driver_core`'s `GroupMsg::World`, drains T1 & T2, returns World, exits.
        let g_inbox = inbox_tx.clone();
        let g_control = Arc::clone(&control);
        let c_gturn_g = std::sync::Arc::clone(&c_gturn);
        let c_done_g = std::sync::Arc::clone(&c_done);
        let tg = loom::thread::spawn(move || {
            match group_grant_rx.recv() {
                Ok(GroupMsg::World {
                    turn: world_turn, ..
                }) => {
                    c_gturn_g.fetch_add(1, StdOrdering::Relaxed);
                    let ran = g_control.enter_group_holding_and_drain();
                    assert_eq!(
                        ran,
                        vec![baton_t1, baton_t2],
                        "both woken Baton threads must run in FIFO order"
                    );
                    c_done_g.fetch_add(ran.len(), StdOrdering::Relaxed);
                    g_inbox
                        .send(KeeperMsg::WorldReturn {
                            returned: world_turn,
                            end: IntervalEnd::Done,
                        })
                        .unwrap();
                }
                Ok(GroupMsg::Poisoned) => panic!("Baton group poisoned unexpectedly"),
                Err(_) => {
                    // Keeper drained without ever granting the Baton group.
                }
            }
            if mode != CoalesceMode::DroppedCoalesce {
                let _ = g_inbox.send(KeeperMsg::GroupExit);
            }
            drop(g_inbox);
        });

        drop(inbox_tx);

        // Run real `driver_core::Driver` on the current thread (3 Loom threads total:
        // keeper + parallel owner + Baton group).
        let end = driver.run();

        tp.join().unwrap();
        tg.join().unwrap();

        assert_eq!(end, DriverEnd::Drained, "keeper must drain cleanly");
        let s = control.0.lock().unwrap();
        assert_eq!(
            s.waits.get(&baton_t1),
            Some(&BatonWait::Done),
            "woken Baton thread never ran"
        );
        assert_eq!(
            s.waits.get(&baton_t2),
            Some(&BatonWait::Done),
            "woken Baton thread never ran"
        );
        assert_eq!(
            s.baton_requests_sent, 1,
            "exactly one coalesced BatonRequest"
        );
        assert_eq!(
            s.duplicate_suppressed, 1,
            "second wake coalesced without duplicate request"
        );
        let era_s = era.state.lock().unwrap();
        assert_eq!(
            era_s.group_enqueues, 1,
            "control ring must enqueue BatonGroup exactly once"
        );
        assert_eq!(
            era_s.duplicate_group_busy, 0,
            "control ring must see zero Busy duplicates"
        );
    });

    CoverageD {
        executions,
        parallel_turn_granted,
        group_turn_granted,
        coalesced_idle_to_requested,
        coalesced_suppressed_duplicate,
        baton_threads_completed,
    }
}

pub(crate) fn run_real() -> CoverageD {
    run_model_d_impl(CoalesceMode::Real)
}

pub(crate) fn run_missing_idle_guard() -> CoverageD {
    run_model_d_impl(CoalesceMode::MissingIdleGuard)
}

pub(crate) fn run_dropped_coalesce() -> CoverageD {
    run_model_d_impl(CoalesceMode::DroppedCoalesce)
}

/// Companion Model (d) stress: TWO concurrent parallel owners P1 and P2 race
/// under `Control` lock to wake two Baton threads T1 and T2 while `GroupTurn`
/// is `Idle` -> proves `coalesce_request_locked` sends EXACTLY ONE
/// `BatonRequest` across concurrent wakers (zero duplicate entries on the ring).
pub(crate) struct CoverageDConcurrent {
    pub(crate) executions: std::sync::Arc<StdAtomicUsize>,
    pub(crate) p1_won_idle: std::sync::Arc<StdAtomicUsize>,
    pub(crate) p2_won_idle: std::sync::Arc<StdAtomicUsize>,
}

pub(crate) fn run_concurrent_wakers_coalesce() -> CoverageDConcurrent {
    let executions = std::sync::Arc::new(StdAtomicUsize::new(0));
    let p1_won_idle = std::sync::Arc::new(StdAtomicUsize::new(0));
    let p2_won_idle = std::sync::Arc::new(StdAtomicUsize::new(0));

    let c_exec = std::sync::Arc::clone(&executions);
    let c_p1 = std::sync::Arc::clone(&p1_won_idle);
    let c_p2 = std::sync::Arc::clone(&p2_won_idle);

    model_with_bound(move || {
        c_exec.fetch_add(1, StdOrdering::Relaxed);
        let (tx, rx) = mpsc::channel::<Msg>();
        let control = ControlModel::new(&[10, 11], 1, tx, CoalesceMode::Real);
        {
            let mut s = control.0.lock().unwrap();
            // T1 waits on condition 1, T2 waits on condition 2.
            s.waits.insert(10, BatonWait::Condition(1));
            s.waits.insert(11, BatonWait::Condition(2));
        }

        let ctrl1 = Arc::clone(&control);
        let c_p1_t = std::sync::Arc::clone(&c_p1);
        let t1 = loom::thread::spawn(move || {
            let mut s = ctrl1.0.lock().unwrap();
            s.waits.insert(10, BatonWait::Runnable);
            s.ready.push_back(10);
            if ControlModel::coalesce_request_locked(&mut s) {
                c_p1_t.fetch_add(1, StdOrdering::Relaxed);
            }
        });

        let ctrl2 = Arc::clone(&control);
        let c_p2_t = std::sync::Arc::clone(&c_p2);
        let t2 = loom::thread::spawn(move || {
            let mut s = ctrl2.0.lock().unwrap();
            s.waits.insert(11, BatonWait::Runnable);
            s.ready.push_back(11);
            if ControlModel::coalesce_request_locked(&mut s) {
                c_p2_t.fetch_add(1, StdOrdering::Relaxed);
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Drain channel: MUST contain EXACTLY ONE `BatonRequest`.
        let mut count = 0usize;
        while let Ok(msg) = rx.try_recv() {
            assert!(matches!(msg, KeeperMsg::BatonRequest));
            count += 1;
        }
        assert_eq!(
            count, 1,
            "two concurrent wakes must coalesce to 1 BatonRequest"
        );

        let ran = control.enter_group_holding_and_drain();
        assert_eq!(ran.len(), 2, "both Baton threads must run");
        assert!(ran.contains(&10) && ran.contains(&11));
    });

    CoverageDConcurrent {
        executions,
        p1_won_idle,
        p2_won_idle,
    }
}
