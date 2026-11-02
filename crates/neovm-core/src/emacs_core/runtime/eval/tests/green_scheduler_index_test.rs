//! Seeded randomized-history differential proof for the scheduler indexes
//! (peer amendment 5): the pre-change scan implementations stay as a
//! reference model over the same logical `waits`/`ready` content, and every
//! generated transition must produce the same pick, the same predicates, the
//! same wake order and the same valid ready FIFO from the indexed production
//! code.
//!
//! The generator follows the single-Baton discipline: `register` is its own
//! transition; `set_wait` is applied only to a key whose own wait is
//! `Runnable` (a running owner) and is immediately followed by the pick that
//! `transfer` performs; `finish` likewise picks at once. A parked key
//! therefore cannot be woken before the pick that drops its stale turn — the
//! corner the prep harness found, where the old retain would resurrect a
//! double turn.

use super::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

/// Mirror of `ThreadKey`: `Main` sorts first, workers by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Main,
    Worker(u64),
}

fn worker(id: u64) -> Key {
    Key::Worker(id)
}

/// Mirror of the `Wait` variants the queries distinguish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotWait {
    Runnable,
    Join(Key),
    Mutex(u64),
    Condition(u64),
    Select { wait_end: Instant },
    HostRequest,
    Exited,
}

/// Mirror of `Peers`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MockPeers {
    Ready,
    Sleeping(Instant),
    Blocked,
}

/// Mirror of the main-thread locations the pick/peers filters exclude.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Loc {
    #[default]
    Running,
    NativeReleased,
    Serving,
}

fn key(thread: ThreadKey) -> Key {
    match thread {
        ThreadKey::Main => Key::Main,
        ThreadKey::Worker(id) => Key::Worker(id.get()),
    }
}

fn thread_key(key: Key) -> ThreadKey {
    match key {
        Key::Main => ThreadKey::Main,
        Key::Worker(id) => {
            ThreadKey::Worker(NonZeroU64::new(id).expect("generated worker ids start at one"))
        }
    }
}

fn slot_wait(wait: Wait) -> SlotWait {
    match wait {
        Wait::Runnable => SlotWait::Runnable,
        Wait::Join(target) => SlotWait::Join(key(target)),
        Wait::Mutex(id) => SlotWait::Mutex(id.0),
        Wait::Condition(id) => SlotWait::Condition(id.0),
        Wait::Select { wait_end } => SlotWait::Select { wait_end },
        Wait::HostRequest(_) => SlotWait::HostRequest,
        Wait::Exited => SlotWait::Exited,
    }
}

fn production_wait(wait: SlotWait) -> Wait {
    match wait {
        SlotWait::Runnable => Wait::Runnable,
        SlotWait::Join(target) => Wait::Join(thread_key(target)),
        SlotWait::Mutex(id) => Wait::Mutex(MutexId::new(id)),
        SlotWait::Condition(id) => Wait::Condition(ConditionId::new(id)),
        SlotWait::Select { wait_end } => Wait::Select { wait_end },
        SlotWait::HostRequest => Wait::HostRequest(HostRequestKey::for_test(NonZeroU64::MIN)),
        SlotWait::Exited => Wait::Exited,
    }
}

fn mock_peers(peers: Peers) -> MockPeers {
    match peers {
        Peers::Ready => MockPeers::Ready,
        Peers::Sleeping(deadline) => MockPeers::Sleeping(deadline),
        Peers::Blocked => MockPeers::Blocked,
    }
}

// ---------------------------------------------------------------------------
// Reference model: the pre-change scans, in behavior identical to the
// scheduler before the index (with the b262/R2 deadline-order wake and the
// Exited retirement in `finish`).
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Old {
    waits: BTreeMap<Key, SlotWait>,
    ready: VecDeque<Key>,
    main_loc: Loc,
    closing: bool,
}

impl Old {
    fn main_excluded(&self) -> bool {
        matches!(self.main_loc, Loc::NativeReleased | Loc::Serving)
    }

    fn excluded(&self, key: Key) -> bool {
        key == Key::Main && self.main_excluded()
    }

    fn wake_locked(&mut self, key: Key) {
        match self.waits.get(&key) {
            Some(SlotWait::Runnable | SlotWait::HostRequest | SlotWait::Exited) | None => {}
            Some(
                SlotWait::Join(_)
                | SlotWait::Mutex(_)
                | SlotWait::Condition(_)
                | SlotWait::Select { .. },
            ) => {
                self.waits.insert(key, SlotWait::Runnable);
                self.ready.push_back(key);
            }
        }
    }

    fn matching(&self, wait: SlotWait) -> Vec<Key> {
        self.waits
            .iter()
            .filter(|(_, w)| **w == wait)
            .map(|(key, _)| *key)
            .collect()
    }

    fn wake_all(&mut self, wait: SlotWait) -> Vec<Key> {
        let keys = self.matching(wait);
        for key in &keys {
            self.wake_locked(*key);
        }
        keys
    }

    fn wake_one(&mut self, wait: SlotWait) -> Option<Key> {
        let key = self
            .waits
            .iter()
            .find(|(_, w)| **w == wait)
            .map(|(k, _)| *k);
        if let Some(key) = key {
            self.wake_locked(key);
        }
        key
    }

    /// The b262 deadline-order expiry: collect the expired pairs and wake
    /// earliest deadline first; simultaneous deadlines keep key order.
    fn expire(&mut self, now: Instant) -> Vec<Key> {
        let mut expired: Vec<(Key, Instant)> = self
            .waits
            .iter()
            .filter_map(|(key, wait)| match wait {
                SlotWait::Select { wait_end } if *wait_end <= now => Some((*key, *wait_end)),
                _ => None,
            })
            .collect();
        expired.sort_by_key(|&(key, wait_end)| (wait_end, key));
        for (key, _) in &expired {
            self.wake_locked(*key);
        }
        expired.into_iter().map(|(key, _)| key).collect()
    }

    /// The R2 readiness path: expired deadlines first (deadline order), then
    /// the remaining selectors in key order.
    fn wake_selects(&mut self, now: Instant) -> Vec<Key> {
        let mut woken = self.expire(now);
        let remaining: Vec<Key> = self
            .waits
            .iter()
            .filter_map(|(key, wait)| matches!(wait, SlotWait::Select { .. }).then_some(*key))
            .collect();
        for key in &remaining {
            self.wake_locked(*key);
        }
        woken.extend(remaining);
        woken
    }

    /// `register_worker`: a fresh key with one turn.
    fn register(&mut self, key: Key) {
        self.waits.insert(key, SlotWait::Runnable);
        self.ready.push_back(key);
    }

    fn set_wait(&mut self, key: Key, wait: SlotWait) {
        self.waits.insert(key, wait);
    }

    fn prioritize(&mut self, key: Key) {
        if matches!(
            self.waits.get(&key),
            Some(SlotWait::Exited | SlotWait::HostRequest) | None
        ) {
            return;
        }
        self.ready.retain(|ready| *ready != key);
        self.waits.insert(key, SlotWait::Runnable);
        self.ready.push_front(key);
    }

    /// `finish`: publish the end, wake joiners, then retire the record.
    fn finish(&mut self, key: Key) {
        self.waits.insert(key, SlotWait::Exited);
        let joiners = self.matching(SlotWait::Join(key));
        for joiner in &joiners {
            self.wake_locked(*joiner);
        }
        self.waits.remove(&key);
    }

    fn others_could_run(&self, me: Key) -> bool {
        self.waits.iter().any(|(key, wait)| {
            *key != me && matches!(wait, SlotWait::Runnable | SlotWait::Select { .. })
        })
    }

    fn peers(&self, me: Key, now: Instant) -> MockPeers {
        let mut earliest: Option<Instant> = None;
        for (key, wait) in &self.waits {
            if *key == me {
                continue;
            }
            match *wait {
                SlotWait::Runnable => return MockPeers::Ready,
                SlotWait::Select { wait_end } if wait_end <= now => return MockPeers::Ready,
                SlotWait::Select { wait_end } => {
                    earliest = Some(earliest.map_or(wait_end, |e| e.min(wait_end)));
                }
                SlotWait::Join(_)
                | SlotWait::Mutex(_)
                | SlotWait::Condition(_)
                | SlotWait::HostRequest
                | SlotWait::Exited => {}
            }
        }
        earliest.map_or(MockPeers::Blocked, MockPeers::Sleeping)
    }

    fn ready_run_predicate(&self, me: Key) -> bool {
        self.ready
            .iter()
            .any(|key| *key != me && self.waits.get(key) == Some(&SlotWait::Runnable))
    }

    fn yield_peer_ready(&mut self, me: Key, now: Instant) -> bool {
        if self.closing || self.ready_run_predicate(me) {
            return true;
        }
        self.expire(now);
        self.ready_run_predicate(me)
    }

    fn pick(&mut self, now: Instant) -> Option<(Key, Option<Instant>)> {
        self.expire(now);
        self.ready
            .retain(|key| self.waits.get(key) == Some(&SlotWait::Runnable));
        if let Some(index) = self.ready.iter().position(|key| !self.excluded(*key))
            && let Some(key) = self.ready.remove(index)
        {
            return Some((key, None));
        }
        self.waits
            .iter()
            .filter(|(key, _)| !self.excluded(**key))
            .filter_map(|(key, wait)| match wait {
                SlotWait::Select { wait_end } => Some((*key, *wait_end)),
                _ => None,
            })
            .min_by_key(|(_, deadline)| *deadline)
            .map(|(key, deadline)| (key, Some(deadline)))
    }

    /// The valid ready FIFO as the old scans observe it.
    fn ready_runnable(&self) -> Vec<Key> {
        self.ready
            .iter()
            .copied()
            .filter(|key| self.waits.get(key) == Some(&SlotWait::Runnable))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Production side: the indexed scheduler, driven through its seams.
// ---------------------------------------------------------------------------

struct Prod {
    control: Control,
}

impl Prod {
    fn new() -> Self {
        Self {
            control: Control::default(),
        }
    }

    fn register(&self, key: Key) {
        let mut state = self.control.state();
        Control::set_wait(&mut state, thread_key(key), Wait::Runnable);
        Control::enqueue_ready(&mut state, thread_key(key));
    }

    fn set_wait(&self, key: Key, wait: SlotWait) {
        let mut state = self.control.state();
        Control::set_wait(&mut state, thread_key(key), production_wait(wait));
    }

    fn wake_locked(&self, key: Key) {
        let mut state = self.control.state();
        Control::wake_locked(&mut state, thread_key(key));
    }

    fn wake_all(&self, wait: SlotWait) -> usize {
        self.control.wake_waiting_for(production_wait(wait))
    }

    fn wake_one(&self, wait: SlotWait) -> bool {
        self.control.wake_one_waiting_for(production_wait(wait))
    }

    fn prioritize(&self, key: Key) {
        self.control.prioritize(thread_key(key));
    }

    fn expire(&self, now: Instant) {
        let mut state = self.control.state();
        Control::expire_deadlines(&mut state, now);
    }

    fn wake_selects(&self, now: Instant) {
        let mut state = self.control.state();
        Control::wake_selects_at(&mut state, now);
    }

    fn finish(&self, key: Key) {
        self.control.publish_owner_exit(thread_key(key));
        let mut state = self.control.state();
        Control::remove_key(&mut state, thread_key(key));
    }

    fn set_main_loc(&self, loc: Loc) {
        let mut state = self.control.state();
        let location = match loc {
            Loc::Running => OwnerLocation::Running,
            Loc::NativeReleased => OwnerLocation::NativeReleased,
            Loc::Serving => OwnerLocation::Serving(HostRequestKey::for_test(NonZeroU64::MIN)),
        };
        state.physical.insert(ThreadKey::Main, location);
    }

    fn pick(&self, now: Instant) -> Option<(Key, Option<Instant>)> {
        let mut state = self.control.state();
        Control::pick_at(&mut state, now).map(|(key, deadline)| (key_of(key), deadline))
    }

    fn others_could_run(&self, me: Key) -> bool {
        self.control.others_could_run(thread_key(me))
    }

    fn peers(&self, me: Key, now: Instant) -> MockPeers {
        mock_peers(self.control.peers(thread_key(me), now))
    }

    fn yield_peer_ready(&self, me: Key, now: Instant) -> bool {
        self.control.yield_peer_ready_at(thread_key(me), now)
    }

    fn snapshot(&self) -> (BTreeMap<Key, SlotWait>, Vec<Key>) {
        let state = self.control.state();
        Control::check_invariants(&state);
        let waits = state
            .waits
            .iter()
            .map(|(key, wait)| (key_of(*key), slot_wait(*wait)))
            .collect();
        let ready = state.ready.valid_keys().into_iter().map(key_of).collect();
        (waits, ready)
    }
}

fn key_of(thread: ThreadKey) -> Key {
    key(thread)
}

// ---------------------------------------------------------------------------
// Seeded randomized-history differential driver.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    /// `register_worker`: a fresh key becomes Runnable with one turn.
    Register(Key),
    /// `transfer`: the running thread gives up the Baton for `wait`; the
    /// caller then runs `pick_next` immediately.
    SetWait(Key, SlotWait),
    Wake(Key),
    WakeAll(SlotWait),
    WakeOne(SlotWait),
    Prioritize(Key),
    Expire(Duration),
    Pick,
    /// `finish`: the running thread ends; `pick_next` runs immediately.
    Finish(Key),
    /// The readiness path: expired deadlines first, then the remaining
    /// selectors in key order.
    WakeSelects,
    MainLoc(Loc),
}

struct Driver {
    old: Old,
    prod: Prod,
    rng: Rng,
    keys: Vec<Key>,
    now: Instant,
}

impl Driver {
    fn new(seed: u64) -> Self {
        let keys: Vec<Key> = (1..=40).map(worker).collect();
        Self {
            old: Old::default(),
            prod: Prod::new(),
            rng: Rng(seed),
            keys,
            now: Instant::now(),
        }
    }

    fn pick_key(&mut self) -> Key {
        if self.rng.below(8) == 0 {
            Key::Main
        } else {
            self.keys[self.rng.below(self.keys.len())]
        }
    }

    fn random_target(&mut self) -> SlotWait {
        match self.rng.below(3) {
            0 => SlotWait::Join(self.pick_key()),
            1 => SlotWait::Mutex(self.rng.below(4) as u64),
            _ => SlotWait::Condition(self.rng.below(4) as u64),
        }
    }

    fn random_wait(&mut self) -> SlotWait {
        match self.rng.below(5) {
            0 => self.random_target(),
            1 | 2 => SlotWait::Select {
                wait_end: self.now + Duration::from_micros(self.rng.below(8000) as u64),
            },
            3 => SlotWait::HostRequest,
            _ => self.random_target(),
        }
    }

    fn random_op(&mut self) -> Op {
        match self.rng.below(12) {
            0 => Op::Register(self.pick_key()),
            1 | 2 => Op::SetWait(self.pick_key(), self.random_wait()),
            3 => Op::Wake(self.pick_key()),
            4 => Op::WakeAll(self.random_target()),
            5 => Op::WakeOne(self.random_target()),
            6 => Op::Prioritize(self.pick_key()),
            7 => Op::Expire(Duration::from_micros(self.rng.below(4000) as u64)),
            8 => Op::Pick,
            9 => Op::Finish(self.pick_key()),
            10 => Op::WakeSelects,
            _ => Op::MainLoc(match self.rng.below(3) {
                0 => Loc::Running,
                1 => Loc::NativeReleased,
                _ => Loc::Serving,
            }),
        }
    }

    fn step(&mut self) -> Result<(), String> {
        let op = self.random_op();
        self.apply(op)?;
        self.compare()
    }

    /// `transfer` ends with `pick_next`, and only the Baton holder acts
    /// between the wait insertion and that pick; mirror the immediate pick.
    fn pick_both(&mut self) -> Result<(), String> {
        let a = self.old.pick(self.now);
        let b = self.prod.pick(self.now);
        if a != b {
            return Err(format!("pick {a:?} != {b:?}"));
        }
        Ok(())
    }

    fn apply(&mut self, op: Op) -> Result<(), String> {
        match op {
            Op::Register(key) => {
                if self.old.waits.contains_key(&key) {
                    return Ok(());
                }
                self.old.register(key);
                self.prod.register(key);
            }
            Op::SetWait(key, wait) => {
                // Only a running thread transfers: its own wait is Runnable.
                if self.old.waits.get(&key) != Some(&SlotWait::Runnable) {
                    return Ok(());
                }
                self.old.set_wait(key, wait);
                self.prod.set_wait(key, wait);
                self.pick_both()?;
            }
            Op::Wake(key) => {
                self.old.wake_locked(key);
                self.prod.wake_locked(key);
            }
            Op::WakeAll(wait) => {
                let a = self.old.wake_all(wait);
                let b = self.prod.wake_all(wait);
                if a.len() != b {
                    return Err(format!("wake_all({wait:?}) {} != {b}", a.len()));
                }
            }
            Op::WakeOne(wait) => {
                let a = self.old.wake_one(wait);
                let b = self.prod.wake_one(wait);
                if a.is_some() != b {
                    return Err(format!("wake_one({wait:?}) {a:?} != {b}"));
                }
            }
            Op::Prioritize(key) => {
                self.old.prioritize(key);
                self.prod.prioritize(key);
            }
            Op::Expire(delta) => {
                self.now += delta;
                self.old.expire(self.now);
                self.prod.expire(self.now);
            }
            Op::Pick => {
                self.pick_both()?;
            }
            Op::Finish(key) => {
                // `finish` belongs to the running thread.
                if self.old.waits.get(&key) != Some(&SlotWait::Runnable) {
                    return Ok(());
                }
                self.old.finish(key);
                self.prod.finish(key);
                self.pick_both()?;
            }
            Op::WakeSelects => {
                self.old.wake_selects(self.now);
                self.prod.wake_selects(self.now);
            }
            Op::MainLoc(loc) => {
                self.old.main_loc = loc;
                self.prod.set_main_loc(loc);
            }
        }
        Ok(())
    }

    fn compare(&mut self) -> Result<(), String> {
        let (waits, ready) = self.prod.snapshot();
        if waits != self.old.waits {
            return Err(format!(
                "waits differ: prod {waits:?} reference {:?}",
                self.old.waits
            ));
        }
        let reference_ready = self.old.ready_runnable();
        if ready != reference_ready {
            return Err(format!("ready FIFO {ready:?} != {reference_ready:?}"));
        }
        for me in self.keys.iter().copied().chain(std::iter::once(Key::Main)) {
            if self.old.others_could_run(me) != self.prod.others_could_run(me) {
                return Err(format!(
                    "others_could_run({me:?}) {} != {}",
                    self.old.others_could_run(me),
                    self.prod.others_could_run(me)
                ));
            }
            let a = self.old.peers(me, self.now);
            let b = self.prod.peers(me, self.now);
            if a != b {
                return Err(format!("peers({me:?}) {a:?} != {b:?}"));
            }
            let a = self.old.yield_peer_ready(me, self.now);
            let b = self.prod.yield_peer_ready(me, self.now);
            if a != b {
                return Err(format!("yield_peer_ready({me:?}) {a} != {b}"));
            }
        }
        // `yield_peer_ready` above may expire deadlines; both sides must
        // still agree on the resulting content.
        let (waits, ready) = self.prod.snapshot();
        if waits != self.old.waits {
            return Err(format!(
                "waits differ after yields: prod {waits:?} reference {:?}",
                self.old.waits
            ));
        }
        let reference_ready = self.old.ready_runnable();
        if ready != reference_ready {
            return Err(format!(
                "ready FIFO after yields {ready:?} != {reference_ready:?}"
            ));
        }
        Ok(())
    }

    fn run_ok(&mut self, steps: usize) {
        for step in 0..steps {
            self.step()
                .unwrap_or_else(|err| panic!("seed step {step}: {err}"));
        }
    }
}

#[test]
fn randomized_histories_match_the_reference_scans() {
    for seed in [1u64, 2, 3, 0x5EED, 0xC0FFEE] {
        Driver::new(seed).run_ok(1500);
    }
}

/// The lazy ready invalidation preserves FIFO: a thread whose queued turn
/// went stale is appended at the back when it is woken again, exactly as the
/// old `ready.retain` + `push_back` did.
#[test]
fn ready_lazy_invalidation_preserves_fifo() {
    let prod = Prod::new();
    let a = worker(1);
    let b = worker(2);
    prod.register(a);
    prod.register(b);
    prod.set_wait(a, SlotWait::Join(Key::Main));
    prod.wake_locked(a);
    let (_, ready) = prod.snapshot();
    assert_eq!(ready, vec![b, a], "the re-woken key joins at the back");
}

/// The deadline index is keyed by `(wait_end, ThreadKey)`: expiry admits the
/// earliest deadline first and keeps key order for ties.
#[test]
fn expire_index_iterates_in_deadline_then_key_order() {
    let now = Instant::now();
    let early = now + Duration::from_micros(5);
    let middle = now + Duration::from_micros(20);
    let late = now + Duration::from_micros(30);
    let worker2 = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
    let worker5 = ThreadKey::Worker(NonZeroU64::new(5).expect("nonzero"));
    let mut state = ControlState::default();
    state
        .waits
        .insert(ThreadKey::Main, Wait::Select { wait_end: late });
    state
        .waits
        .insert(worker2, Wait::Select { wait_end: middle });
    state
        .waits
        .insert(worker5, Wait::Select { wait_end: early });
    // The same deadline keeps ThreadKey order.
    state.waits.insert(
        ThreadKey::Worker(NonZeroU64::new(6).expect("nonzero")),
        Wait::Select { wait_end: early },
    );
    Control::expire_deadlines(&mut state, late);
    assert_eq!(state.ready.pop_front(), Some(worker5));
    assert_eq!(
        state.ready.pop_front(),
        Some(ThreadKey::Worker(NonZeroU64::new(6).expect("nonzero")))
    );
    assert_eq!(state.ready.pop_front(), Some(worker2));
    assert_eq!(state.ready.pop_front(), Some(ThreadKey::Main));
    assert!(state.ready.is_empty());
}

/// The R2 readiness path: expired deadlines admit first in deadline order,
/// then the remaining selectors wake in `ThreadKey` order.
#[test]
fn wake_selects_expires_deadline_first_then_keys_in_order() {
    let now = Instant::now();
    // The three expired deadlines are already in the past at NOW; the index
    // emits them earliest first. The two future selectors stay key-ordered.
    let early = now - Duration::from_micros(30);
    let middle = now - Duration::from_micros(20);
    let late = now - Duration::from_micros(5);
    let future = now + Duration::from_secs(1);
    let worker2 = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
    let worker5 = ThreadKey::Worker(NonZeroU64::new(5).expect("nonzero"));
    let worker7 = ThreadKey::Worker(NonZeroU64::new(7).expect("nonzero"));
    let worker8 = ThreadKey::Worker(NonZeroU64::new(8).expect("nonzero"));
    let mut state = ControlState::default();
    state
        .waits
        .insert(ThreadKey::Main, Wait::Select { wait_end: late });
    state
        .waits
        .insert(worker2, Wait::Select { wait_end: middle });
    state
        .waits
        .insert(worker5, Wait::Select { wait_end: early });
    state
        .waits
        .insert(worker8, Wait::Select { wait_end: future });
    state
        .waits
        .insert(worker7, Wait::Select { wait_end: future });
    Control::wake_selects_at(&mut state, now);
    assert_eq!(state.ready.pop_front(), Some(worker5));
    assert_eq!(state.ready.pop_front(), Some(worker2));
    assert_eq!(state.ready.pop_front(), Some(ThreadKey::Main));
    assert_eq!(state.ready.pop_front(), Some(worker7));
    assert_eq!(state.ready.pop_front(), Some(worker8));
    assert!(state.ready.is_empty());
}

/// Wake-one picks the smallest waiting key per object, not the first woken.
#[test]
fn wake_one_picks_the_smallest_waiting_key() {
    let prod = Prod::new();
    let low = worker(3);
    let high = worker(9);
    prod.register(low);
    prod.register(high);
    prod.set_wait(high, SlotWait::Mutex(7));
    prod.set_wait(low, SlotWait::Mutex(7));
    assert!(prod.wake_one(SlotWait::Mutex(7)), "the object had a waiter");
    assert_eq!(
        prod.control.wait_of(thread_key(low)),
        Some(Wait::Runnable),
        "the smallest key waiting on the object is woken"
    );
    assert_eq!(
        prod.control.wait_of(thread_key(high)),
        Some(Wait::Mutex(MutexId::new(7)))
    );
}

/// The production scheduler must not grow direct `waits.insert` or raw
/// `ready` queue writes again (peer amendment 4; the seams are the only
/// writers). The cfg(test) module is exempt.
#[test]
fn production_wait_writes_go_through_the_seams() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/emacs_core/runtime/eval/green/scheduler.rs"
    );
    let source = std::fs::read_to_string(path).expect("read scheduler.rs");
    let marker = "#[cfg(test)]\nmod tests {";
    let production = source
        .split_once(marker)
        .map(|(production, _)| production)
        .expect("scheduler.rs cfg(test) module marker");

    let inserts: Vec<usize> = production
        .match_indices("waits.insert(")
        .map(|(offset, _)| offset)
        .collect();
    assert_eq!(
        inserts.len(),
        1,
        "production may write the wait map only inside set_wait"
    );
    let set_wait = production.find("fn set_wait(").expect("set_wait seam");
    let set_wait_end = set_wait
        + production[set_wait..]
            .find("\n    }")
            .expect("set_wait end");
    assert!(
        inserts[0] > set_wait && inserts[0] < set_wait_end,
        "the only wait-map write must sit inside fn set_wait"
    );
    for raw in [
        "ready.push_back(",
        "ready.push_front(",
        "ready.retain(",
        "ready.pop_front(",
        "ready.remove(",
    ] {
        assert!(
            !production.contains(raw),
            "production ready-queue writes must go through the seams, found {raw}"
        );
    }

    fn span<'a>(source: &'a str, name: &str) -> &'a str {
        let start = source
            .find(&format!("fn {name}("))
            .unwrap_or_else(|| panic!("missing fn {name}"));
        let end = start + source[start..].find("\n    }").expect("function end");
        &source[start..end]
    }

    let wake_locked = span(production, "wake_locked");
    assert!(
        wake_locked.contains("Self::set_wait(") && wake_locked.contains("Self::enqueue_ready("),
        "wake_locked must route its transition through set_wait/enqueue_ready"
    );
    assert!(!wake_locked.contains("waits.insert("));
    let wake_selects = span(production, "wake_selects");
    assert!(
        wake_selects.contains("Self::wake_selects_at("),
        "wake_selects must run the readiness path through its clock-parameterized seam"
    );
    for seam in ["expire_deadlines", "wake_selects_at"] {
        let body = span(production, seam);
        assert!(
            body.contains("Self::wake_locked("),
            "{seam} must wake through wake_locked's seams"
        );
        assert!(!body.contains("waits.insert("));
    }
}
