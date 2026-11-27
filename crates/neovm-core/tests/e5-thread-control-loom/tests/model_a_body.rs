use self::wait_core::{OwnerWake, ParallelParticipant, Participant};
use super::*;

struct StateA {
    turn_holder: Option<ThreadKey>,
    ready: bool,
    waits: BTreeMap<ThreadKey, Wait>,
    participants: BTreeMap<ThreadKey, Participant<()>>,
}

struct WorldA {
    state: Mutex<StateA>,
    turn_cv: Condvar,
}

impl WorldA {
    fn new(owner_a: ThreadKey) -> (Arc<Self>, OwnerWake) {
        let participant = ParallelParticipant::new();
        let wake = participant.wake.clone();
        let mut participants = BTreeMap::new();
        participants.insert(owner_a, Participant::Parallel(participant));
        let mut waits = BTreeMap::new();
        waits.insert(owner_a, Wait::ParallelActive);
        let world = Arc::new(Self {
            state: Mutex::new(StateA {
                turn_holder: None,
                ready: false,
                waits,
                participants,
            }),
            turn_cv: Condvar::new(),
        });
        (world, wake)
    }

    fn acquire_turn(&self, who: ThreadKey) {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while s.turn_holder.is_some() {
            s = self.turn_cv.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
        s.turn_holder = Some(who);
    }

    fn release_turn(&self, who: ThreadKey) {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(s.turn_holder, Some(who));
        s.turn_holder = None;
        self.turn_cv.notify_one();
    }

    /// Atomically register `Wait::Condition(cond)` + `wake.clear()`
    /// inside turn T1 and return the turn, OR report already ready.
    fn register_or_ready(&self, who: ThreadKey, cond: ConditionId) -> bool {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(s.turn_holder, Some(who));
        if s.ready {
            s.turn_holder = None;
            self.turn_cv.notify_one();
            return true;
        }
        if let Some(Participant::Parallel(p)) = s.participants.get(&who) {
            p.wake.clear();
        }
        s.waits.insert(who, Wait::Condition(cond));
        s.turn_holder = None;
        self.turn_cv.notify_one();
        false
    }

    fn notify_condition(&self, who: ThreadKey, cond: ConditionId) -> bool {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(s.turn_holder, Some(who));
        s.ready = true;
        let waiters: Vec<ThreadKey> = s
            .waits
            .iter()
            .filter_map(|(&k, &w)| (w == Wait::Condition(cond)).then_some(k))
            .collect();
        let mut woke_any = false;
        for key in waiters {
            if let Some(Participant::Parallel(p)) = s.participants.get(&key) {
                let wake = p.wake.clone();
                s.waits.insert(key, Wait::ParallelActive);
                wake.wake();
                woke_any = true;
            }
        }
        woke_any
    }

    fn verify_woken_state(&self, who: ThreadKey) {
        let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(s.turn_holder, Some(who));
        assert!(s.ready, "Owner A resumed without condition being ready");
        assert_eq!(
            s.waits.get(&who).copied(),
            Some(Wait::ParallelActive),
            "Owner A resumed while wait was not cleared to ParallelActive"
        );
    }
}

#[allow(dead_code)]
pub(crate) struct CoverageA {
    pub(crate) executions: std::sync::Arc<StdAtomicUsize>,
    pub(crate) skipped_already_ready: std::sync::Arc<StdAtomicUsize>,
    pub(crate) woke_before_park: std::sync::Arc<StdAtomicUsize>,
    pub(crate) woke_while_parked: std::sync::Arc<StdAtomicUsize>,
}

pub(crate) fn run(probe_wake_before_park_only: bool) -> CoverageA {
    let executions = std::sync::Arc::new(StdAtomicUsize::new(0));
    let skipped_already_ready = std::sync::Arc::new(StdAtomicUsize::new(0));
    let woke_before_park = std::sync::Arc::new(StdAtomicUsize::new(0));
    let woke_while_parked = std::sync::Arc::new(StdAtomicUsize::new(0));

    let c_exec = std::sync::Arc::clone(&executions);
    let c_skip = std::sync::Arc::clone(&skipped_already_ready);
    let c_before = std::sync::Arc::clone(&woke_before_park);
    let c_parked = std::sync::Arc::clone(&woke_while_parked);

    model_with_bound(move || {
        c_exec.fetch_add(1, StdOrdering::Relaxed);
        let owner_a = ThreadKey(1);
        let owner_b = ThreadKey(2);
        let cond = ConditionId::new(10);
        let (world, wake_a) = WorldA::new(owner_a);

        if probe_wake_before_park_only {
            // Negative probe: A registers Wait::Condition in T1 and returns
            // T1; B notifies in T2 BEFORE A parks; then A checks via
            // `OwnerWake::wait_timeout` that the wake-before-park flag
            // survived (exact assertion, no deadlock unwind).
            world.acquire_turn(owner_a);
            assert!(!world.register_or_ready(owner_a, cond));
            let world_b = Arc::clone(&world);
            let tb = loom::thread::spawn(move || {
                world_b.acquire_turn(owner_b);
                assert!(world_b.notify_condition(owner_b, cond));
                world_b.release_turn(owner_b);
            });
            tb.join().unwrap();
            assert!(
                wake_a.wait_timeout(std::time::Duration::from_millis(1)),
                "OwnerWake lost wake-before-park"
            );
            world.acquire_turn(owner_a);
            world.verify_woken_state(owner_a);
            world.release_turn(owner_a);
            return;
        }

        let world_b = Arc::clone(&world);
        let tb = loom::thread::spawn(move || {
            world_b.acquire_turn(owner_b);
            let _ = world_b.notify_condition(owner_b, cond);
            world_b.release_turn(owner_b);
        });

        world.acquire_turn(owner_a);
        let already_ready = world.register_or_ready(owner_a, cond);

        if already_ready {
            c_skip.fetch_add(1, StdOrdering::Relaxed);
        } else {
            let parks_before = crate::sync::condvar_parks();
            // Real product `OwnerWake::wait()` outside serial turn!
            wake_a.wait();
            let parks_after = crate::sync::condvar_parks();
            if parks_after == parks_before {
                c_before.fetch_add(1, StdOrdering::Relaxed);
            } else {
                c_parked.fetch_add(1, StdOrdering::Relaxed);
            }
            world.acquire_turn(owner_a);
            world.verify_woken_state(owner_a);
            world.release_turn(owner_a);
        }

        tb.join().unwrap();
    });

    CoverageA {
        executions,
        skipped_already_ready,
        woke_before_park,
        woke_while_parked,
    }
}
