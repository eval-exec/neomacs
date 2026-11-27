//! Loom models (a), (b), and (c) over copied product `wait_core.rs` and
//! `condition_wait.rs` (E5.4b).

use crate::model_with_bound;
use crate::sync::{self, Arc, Condvar, Mutex, PoisonError};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize as StdAtomicUsize, Ordering as StdOrdering};

// ---------------------------------------------------------------------------
// Shared minimal domain types matching `green/scheduler.rs` & `condition_wait.rs`
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ThreadKey(pub(crate) u64);

impl ThreadKey {
    pub(crate) fn id(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MutexId(u64);

impl MutexId {
    pub(crate) fn new(key: u64) -> Self {
        Self(key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ConditionId(u64);

impl ConditionId {
    pub(crate) fn new(key: u64) -> Self {
        Self(key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wait {
    ParallelActive,
    Join(ThreadKey),
    Mutex(MutexId),
    Condition(ConditionId),
    Exited,
}

// ---------------------------------------------------------------------------
// Copied product `wait_core.rs` (real + negative)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
mod wait_core_real {
    include!(concat!(env!("OUT_DIR"), "/wait_core.rs"));
}

#[allow(dead_code)]
mod wait_core_drop_wake_flag {
    include!(concat!(env!("OUT_DIR"), "/wait_core_drop_wake_flag.rs"));
}

// ---------------------------------------------------------------------------
// Model (a): notify against wait registration
// ---------------------------------------------------------------------------

pub(crate) mod model_a_real {
    use super::wait_core_real as wait_core;
    include!("model_a_body.rs");
}

pub(crate) mod model_a_drop_wake_flag {
    use super::wait_core_drop_wake_flag as wait_core;
    include!("model_a_body.rs");
}

// ---------------------------------------------------------------------------
// Model (b): signal against wake and relock using REAL `green/condition_wait.rs`
// ---------------------------------------------------------------------------

pub(crate) mod model_b_real {
    #[allow(dead_code, unused_imports)]
    mod condition_wait {
        include!(concat!(env!("OUT_DIR"), "/condition_wait.rs"));
    }
    include!("model_b_body.rs");
}

pub(crate) mod model_b_signal_before_relock {
    #[allow(dead_code, unused_imports)]
    mod condition_wait {
        include!(concat!(
            env!("OUT_DIR"),
            "/condition_wait_signal_before_relock.rs"
        ));
    }
    include!("model_b_body.rs");
}

pub(crate) mod model_b_drop_wake_mutex {
    #[allow(dead_code, unused_imports)]
    mod condition_wait {
        include!(concat!(
            env!("OUT_DIR"),
            "/condition_wait_drop_wake_mutex.rs"
        ));
    }
    include!("model_b_body.rs");
}

// ---------------------------------------------------------------------------
// Model (c): join against exit + pre-wait signal snapshot
// ---------------------------------------------------------------------------

pub(crate) mod model_c {
    use super::*;
    use wait_core_real::{OwnerWake, ParallelParticipant, Participant};

    struct StateC {
        turn_holder: Option<ThreadKey>,
        results: BTreeMap<ThreadKey, i64>,
        pending_signals: BTreeMap<ThreadKey, &'static str>,
        waits: BTreeMap<ThreadKey, Wait>,
        join_waiters: BTreeMap<ThreadKey, BTreeSet<ThreadKey>>,
        participants: BTreeMap<ThreadKey, Participant<()>>,
    }

    struct WorldC {
        state: Mutex<StateC>,
        turn_cv: Condvar,
    }

    impl WorldC {
        fn new(
            joiner: ThreadKey,
            target: ThreadKey,
            target_initial_signal: Option<&'static str>,
        ) -> Arc<Self> {
            let mut participants = BTreeMap::new();
            participants.insert(joiner, Participant::Parallel(ParallelParticipant::new()));
            participants.insert(target, Participant::Parallel(ParallelParticipant::new()));
            let mut waits = BTreeMap::new();
            waits.insert(joiner, Wait::ParallelActive);
            waits.insert(target, Wait::ParallelActive);
            let mut pending_signals = BTreeMap::new();
            if let Some(sig) = target_initial_signal {
                pending_signals.insert(target, sig);
            }
            Arc::new(Self {
                state: Mutex::new(StateC {
                    turn_holder: None,
                    results: BTreeMap::new(),
                    pending_signals,
                    waits,
                    join_waiters: BTreeMap::new(),
                    participants,
                }),
                turn_cv: Condvar::new(),
            })
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

        fn owner_wake(&self, who: ThreadKey) -> OwnerWake {
            let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            s.participants
                .get(&who)
                .and_then(Participant::as_parallel)
                .expect("parallel participant")
                .wake
                .clone()
        }
    }

    pub(crate) struct CoverageC {
        pub(crate) executions: std::sync::Arc<StdAtomicUsize>,
        pub(crate) joined_after_exit: std::sync::Arc<StdAtomicUsize>,
        pub(crate) joined_while_parked: std::sync::Arc<StdAtomicUsize>,
        pub(crate) wake_before_park_join: std::sync::Arc<StdAtomicUsize>,
    }

    pub(crate) fn run_join_against_exit(skip_join_wake: bool) -> CoverageC {
        let executions = std::sync::Arc::new(StdAtomicUsize::new(0));
        let joined_after_exit = std::sync::Arc::new(StdAtomicUsize::new(0));
        let joined_while_parked = std::sync::Arc::new(StdAtomicUsize::new(0));
        let wake_before_park_join = std::sync::Arc::new(StdAtomicUsize::new(0));

        let c_exec = std::sync::Arc::clone(&executions);
        let c_after = std::sync::Arc::clone(&joined_after_exit);
        let c_parked = std::sync::Arc::clone(&joined_while_parked);
        let c_wbp = std::sync::Arc::clone(&wake_before_park_join);

        model_with_bound(move || {
            c_exec.fetch_add(1, StdOrdering::Relaxed);
            let owner_a = ThreadKey(1);
            let owner_b = ThreadKey(2);
            let world = WorldC::new(owner_a, owner_b, None);

            // Owner B: records result 42, publishes `Wait::Exited`, wakes joiners.
            let world_b = Arc::clone(&world);
            let tb = loom::thread::spawn(move || {
                world_b.acquire_turn(owner_b);
                {
                    let mut s = world_b.state.lock().unwrap_or_else(PoisonError::into_inner);
                    s.results.insert(owner_b, 42);
                    s.waits.insert(owner_b, Wait::Exited);
                    if !skip_join_wake {
                        if let Some(waiters) = s.join_waiters.remove(&owner_b) {
                            for waiter in waiters {
                                if let Some(Participant::Parallel(p)) = s.participants.get(&waiter)
                                {
                                    let wake = p.wake.clone();
                                    s.waits.insert(waiter, Wait::ParallelActive);
                                    wake.wake();
                                }
                            }
                        }
                    }
                }
                world_b.release_turn(owner_b);
            });

            // Owner A: joins Owner B.
            let wake_a = world.owner_wake(owner_a);
            if skip_join_wake {
                // Negative probe: register Wait::Join first, let B exit, and
                // assert joiner's OwnerWake was woken (exact assertion, no
                // deadlock unwind).
                world.acquire_turn(owner_a);
                let registered = {
                    let mut s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
                    if s.waits.get(&owner_b) == Some(&Wait::Exited) {
                        s.turn_holder = None;
                        world.turn_cv.notify_one();
                        false
                    } else {
                        wake_a.clear();
                        s.waits.insert(owner_a, Wait::Join(owner_b));
                        s.join_waiters.entry(owner_b).or_default().insert(owner_a);
                        s.turn_holder = None;
                        world.turn_cv.notify_one();
                        true
                    }
                };
                tb.join().unwrap();
                if registered {
                    assert!(wake_a.try_take(), "exit failed to wake parked joiner");
                }
                return;
            }

            world.acquire_turn(owner_a);
            let already_exited = {
                let mut s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
                if s.waits.get(&owner_b) == Some(&Wait::Exited) {
                    true
                } else {
                    wake_a.clear();
                    s.waits.insert(owner_a, Wait::Join(owner_b));
                    s.join_waiters.entry(owner_b).or_default().insert(owner_a);
                    s.turn_holder = None;
                    world.turn_cv.notify_one();
                    false
                }
            };

            if already_exited {
                c_after.fetch_add(1, StdOrdering::Relaxed);
            } else {
                let parks_before = crate::sync::condvar_parks();
                wake_a.wait();
                let parks_after = crate::sync::condvar_parks();
                if parks_after == parks_before {
                    c_wbp.fetch_add(1, StdOrdering::Relaxed);
                } else {
                    c_parked.fetch_add(1, StdOrdering::Relaxed);
                }
                world.acquire_turn(owner_a);
            }

            {
                let s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
                assert_eq!(s.waits.get(&owner_b), Some(&Wait::Exited));
                assert_eq!(
                    s.results.get(&owner_b).copied(),
                    Some(42),
                    "joiner must read target's published result"
                );
            }
            world.release_turn(owner_a);
            tb.join().unwrap();
        });

        CoverageC {
            executions,
            joined_after_exit,
            joined_while_parked,
            wake_before_park_join,
        }
    }

    pub(crate) struct CoverageSnapshot {
        pub(crate) executions: std::sync::Arc<StdAtomicUsize>,
        pub(crate) snapshot_saw_pre_wait_signal: std::sync::Arc<StdAtomicUsize>,
        pub(crate) snapshot_ignored_post_wait_signal: std::sync::Arc<StdAtomicUsize>,
    }

    /// GNU `Fthread_join` (`src/thread.c:1134-1141`): target's pending
    /// `thread-signal` is snapshotted BEFORE the join wait; signals sent to the
    /// target AFTER the snapshot do not retroactively alter the joiner's
    /// snapshot.
    pub(crate) fn run_join_pre_wait_signal_snapshot() -> CoverageSnapshot {
        let executions = std::sync::Arc::new(StdAtomicUsize::new(0));
        let snapshot_saw_pre_wait_signal = std::sync::Arc::new(StdAtomicUsize::new(0));
        let snapshot_ignored_post_wait_signal = std::sync::Arc::new(StdAtomicUsize::new(0));

        let c_exec = std::sync::Arc::clone(&executions);
        let c_pre = std::sync::Arc::clone(&snapshot_saw_pre_wait_signal);
        let c_post = std::sync::Arc::clone(&snapshot_ignored_post_wait_signal);

        model_with_bound(move || {
            c_exec.fetch_add(1, StdOrdering::Relaxed);
            let owner_a = ThreadKey(1);
            let owner_b = ThreadKey(2);
            let owner_c = ThreadKey(3);
            let world = WorldC::new(owner_a, owner_b, None);

            // Owner C: injects `thread-signal` into target B (racing with A's
            // pre-wait snapshot!).
            let world_c = Arc::clone(&world);
            let tc = loom::thread::spawn(move || {
                world_c.acquire_turn(owner_c);
                {
                    let mut s = world_c.state.lock().unwrap_or_else(PoisonError::into_inner);
                    if s.waits.get(&owner_b) != Some(&Wait::Exited) {
                        s.pending_signals.insert(owner_b, "injected-err");
                    }
                }
                world_c.release_turn(owner_c);
            });

            // Owner B: exits and wakes joiners (consuming/retiring as GNU does,
            // recording result 99).
            let world_b = Arc::clone(&world);
            let tb = loom::thread::spawn(move || {
                world_b.acquire_turn(owner_b);
                {
                    let mut s = world_b.state.lock().unwrap_or_else(PoisonError::into_inner);
                    s.results.insert(owner_b, 99);
                    s.waits.insert(owner_b, Wait::Exited);
                    if let Some(waiters) = s.join_waiters.remove(&owner_b) {
                        for waiter in waiters {
                            if let Some(Participant::Parallel(p)) = s.participants.get(&waiter) {
                                let wake = p.wake.clone();
                                s.waits.insert(waiter, Wait::ParallelActive);
                                wake.wake();
                            }
                        }
                    }
                }
                world_b.release_turn(owner_b);
            });

            // Owner A: snapshots target B's pending signal BEFORE parking, waits
            // for exit, then verifies the pre-wait snapshot rule.
            let wake_a = world.owner_wake(owner_a);
            world.acquire_turn(owner_a);
            let (snapshot, need_park) = {
                let mut s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
                let snap = s.pending_signals.get(&owner_b).copied();
                if s.waits.get(&owner_b) == Some(&Wait::Exited) {
                    (snap, false)
                } else {
                    wake_a.clear();
                    s.waits.insert(owner_a, Wait::Join(owner_b));
                    s.join_waiters.entry(owner_b).or_default().insert(owner_a);
                    s.turn_holder = None;
                    world.turn_cv.notify_one();
                    (snap, true)
                }
            };
            if need_park {
                wake_a.wait();
                world.acquire_turn(owner_a);
            }
            {
                let s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
                assert_eq!(s.waits.get(&owner_b), Some(&Wait::Exited));
                assert_eq!(s.results.get(&owner_b).copied(), Some(99));
                let post_signal = s.pending_signals.get(&owner_b).copied();
                match snapshot {
                    Some("injected-err") => {
                        c_pre.fetch_add(1, StdOrdering::Relaxed);
                    }
                    None => {
                        if post_signal == Some("injected-err") {
                            c_post.fetch_add(1, StdOrdering::Relaxed);
                        }
                    }
                    other => panic!("unexpected snapshot {other:?}"),
                }
            }
            world.release_turn(owner_a);

            tb.join().unwrap();
            tc.join().unwrap();
        });

        CoverageSnapshot {
            executions,
            snapshot_saw_pre_wait_signal,
            snapshot_ignored_post_wait_signal,
        }
    }
}
