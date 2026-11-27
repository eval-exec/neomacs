use super::*;
use wait_core_real::{OwnerWake, ParallelParticipant, Participant, SignalHint};

pub(super) mod api {
    use super::cw_env::{Flow, Value, signal};

    pub(crate) fn mutex_depth_exhausted() -> Flow {
        signal(
            "error",
            vec![Value::string("Mutex recursion depth exhausted")],
        )
    }
}

pub(super) mod cw_env {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct Value(pub(crate) u64);

    impl Value {
        pub(crate) const NIL: Self = Self(0);
        pub(crate) fn string(_s: &str) -> Self {
            Self(1)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct RecursionDepth(pub(crate) u32);

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct CondVarHandle {
        pub(crate) key: u64,
    }

    impl CondVarHandle {
        pub(crate) fn native_key(self) -> u64 {
            self.key
        }
        pub(crate) fn value(self) -> Value {
            Value(self.key)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct MutexHandle {
        pub(crate) key: u64,
    }

    impl MutexHandle {
        pub(crate) fn native_key(self) -> u64 {
            self.key
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum MutexAcquisition {
        Acquired,
        Blocked,
        #[allow(dead_code)]
        DepthExhausted,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum WaitCondvar {
        Condition,
        Mutex,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct Flow {
        pub(crate) symbol: &'static str,
    }

    pub(crate) type EvalResult = Result<Value, Flow>;

    pub(crate) fn signal(symbol: &'static str, _data: Vec<Value>) -> Flow {
        Flow { symbol }
    }

    pub(crate) struct WorldBState {
        pub(crate) turn_holder: Option<ThreadKey>,
        pub(crate) mutex_owner: Option<ThreadKey>,
        pub(crate) mutex_depth: u32,
        pub(crate) blockers: BTreeMap<u64, Value>,
        pub(crate) wait_condvars: BTreeMap<u64, WaitCondvar>,
        pub(crate) pending_signals: BTreeMap<u64, Flow>,
        pub(crate) waits: BTreeMap<ThreadKey, Wait>,
        pub(crate) participants: BTreeMap<ThreadKey, Participant<()>>,
        pub(crate) notified_count: usize,
        pub(crate) signal_sent: bool,
    }

    pub(crate) struct WorldB {
        pub(crate) state: Mutex<WorldBState>,
        pub(crate) turn_cv: Condvar,
    }

    impl WorldB {
        pub(crate) fn new(owner_a: ThreadKey, owner_b: ThreadKey, saved_depth: u32) -> Arc<Self> {
            let mut participants = BTreeMap::new();
            participants.insert(owner_a, Participant::Parallel(ParallelParticipant::new()));
            participants.insert(owner_b, Participant::Parallel(ParallelParticipant::new()));
            let mut waits = BTreeMap::new();
            waits.insert(owner_a, Wait::ParallelActive);
            waits.insert(owner_b, Wait::ParallelActive);
            Arc::new(Self {
                state: Mutex::new(WorldBState {
                    turn_holder: None,
                    mutex_owner: Some(owner_a),
                    mutex_depth: saved_depth,
                    blockers: BTreeMap::new(),
                    wait_condvars: BTreeMap::new(),
                    pending_signals: BTreeMap::new(),
                    waits,
                    participants,
                    notified_count: 0,
                    signal_sent: false,
                }),
                turn_cv: Condvar::new(),
            })
        }

        pub(crate) fn acquire_turn(&self, who: ThreadKey) {
            let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            while s.turn_holder.is_some() {
                s = self.turn_cv.wait(s).unwrap_or_else(PoisonError::into_inner);
            }
            s.turn_holder = Some(who);
        }

        pub(crate) fn release_turn(&self, who: ThreadKey) {
            let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(who));
            s.turn_holder = None;
            self.turn_cv.notify_one();
        }

        pub(crate) fn wake_for_locked(s: &mut WorldBState, target_wait: Wait) -> usize {
            let keys: Vec<ThreadKey> = s
                .waits
                .iter()
                .filter_map(|(&k, &w)| (w == target_wait).then_some(k))
                .collect();
            let mut woken = 0;
            for key in keys {
                if let Some(Participant::Parallel(p)) = s.participants.get(&key) {
                    let wake = p.wake.clone();
                    s.waits.insert(key, Wait::ParallelActive);
                    wake.wake();
                    woken += 1;
                }
            }
            woken
        }

        pub(crate) fn owner_wake(&self, who: ThreadKey) -> OwnerWake {
            let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            s.participants
                .get(&who)
                .and_then(Participant::as_parallel)
                .expect("parallel participant")
                .wake
                .clone()
        }

        pub(crate) fn signal_hint(&self, who: ThreadKey) -> SignalHint {
            let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            s.participants
                .get(&who)
                .and_then(Participant::as_parallel)
                .expect("parallel participant")
                .signal_hint
                .clone()
        }
    }

    pub(crate) struct ThreadManager {
        pub(crate) world: Arc<WorldB>,
        pub(crate) current: ThreadKey,
        pub(crate) cond_key: u64,
        pub(crate) mutex_key: u64,
    }

    impl ThreadManager {
        pub(crate) fn is_condition_variable(&self, cond: CondVarHandle) -> bool {
            cond.key == self.cond_key
        }

        pub(crate) fn condition_variable_mutex(&self, cond: CondVarHandle) -> MutexHandle {
            assert_eq!(cond.key, self.cond_key);
            MutexHandle {
                key: self.mutex_key,
            }
        }

        pub(crate) fn set_thread_blocker(&mut self, id: u64, value: Value) {
            let mut s = self
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(self.current));
            s.blockers.insert(id, value);
        }

        pub(crate) fn clear_thread_blocker(&mut self, id: u64) {
            let mut s = self
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(self.current));
            s.blockers.remove(&id);
        }

        pub(crate) fn mutex_unlock_for_wait(
            &mut self,
            mutex: MutexHandle,
        ) -> Option<RecursionDepth> {
            assert_eq!(mutex.key, self.mutex_key);
            let mut s = self
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(self.current));
            if s.mutex_owner != Some(self.current) || s.mutex_depth == 0 {
                return None;
            }
            let depth = s.mutex_depth;
            s.mutex_owner = None;
            s.mutex_depth = 0;
            Some(RecursionDepth(depth))
        }

        pub(crate) fn mutex_relock_after_wait(
            &mut self,
            mutex: MutexHandle,
            depth: RecursionDepth,
        ) -> MutexAcquisition {
            assert_eq!(mutex.key, self.mutex_key);
            let mut s = self
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(self.current));
            if s.mutex_owner.is_none() {
                s.mutex_owner = Some(self.current);
                s.mutex_depth = depth.0;
                MutexAcquisition::Acquired
            } else {
                MutexAcquisition::Blocked
            }
        }

        pub(crate) fn pending_thread_signal(&self, id: u64) -> Option<Flow> {
            let s = self
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(self.current));
            s.pending_signals.get(&id).cloned()
        }
    }

    // Minimal stubs so `BatonConditionSeat` inside verbatim
    // `condition_wait.rs` compiles unchanged.
    pub(crate) struct ControlStub;
    impl ControlStub {
        pub(crate) fn wake_waiting_for(&self, _wait: Wait) {}
    }

    pub(crate) struct GreenSeatStub {
        pub(crate) condition_wait: super::condition_wait::ConditionWait,
        pub(crate) control: Arc<ControlStub>,
    }

    pub(crate) struct Context {
        pub(crate) threads: ThreadManager,
        pub(crate) owner_key: ThreadKey,
        pub(crate) parallel_condition_wait: super::condition_wait::ConditionWait,
        pub(crate) roots_depth: usize,
        pub(crate) deliveries: usize,
        pub(crate) expected_depth: u32,
        // Coverage observers (write-only, never affect scheduling).
        pub(crate) obs_park_condition: std::sync::Arc<StdAtomicUsize>,
        pub(crate) obs_park_mutex_relock: std::sync::Arc<StdAtomicUsize>,
        pub(crate) obs_signal_after_relock: std::sync::Arc<StdAtomicUsize>,
        pub(crate) obs_notify_before_signal: std::sync::Arc<StdAtomicUsize>,
        pub(crate) obs_signal_woke_wait: std::sync::Arc<StdAtomicUsize>,
    }

    impl Context {
        pub(crate) fn green_key(&mut self) -> Option<ThreadKey> {
            None
        }
        pub(crate) fn green_seat(&mut self) -> Option<&mut GreenSeatStub> {
            None
        }
        pub(crate) fn green_in_wait_condvar<R>(
            &mut self,
            _kind: WaitCondvar,
            f: impl FnOnce(&mut Self) -> Result<R, Flow>,
        ) -> Result<R, Flow> {
            f(self)
        }
        pub(crate) fn green_park(&mut self, _wait: Wait) -> Result<(), Flow> {
            unreachable!("BatonConditionSeat is not used by Model (b)")
        }
        pub(crate) fn green_raise_pending_signal(&mut self, _key: ThreadKey) -> Result<(), Flow> {
            unreachable!("BatonConditionSeat is not used by Model (b)")
        }
    }
}

use condition_wait::{ConditionSeat, ConditionWait, wait_with_seat};
use cw_env::{CondVarHandle, Context, Flow, ThreadManager, WaitCondvar, WorldB};

/// Parallel-owner `ConditionSeat` mirroring product `ParallelConditionSeat`
/// (`thread_control.rs`), driving the REAL product `condition_wait::wait_with_seat`.
struct ParallelModelSeat;

impl ConditionSeat for ParallelModelSeat {
    fn key(&self, ctx: &mut Context) -> Result<ThreadKey, Flow> {
        Ok(ctx.owner_key)
    }

    fn condition_wait<'a>(&self, ctx: &'a mut Context) -> Result<&'a ConditionWait, Flow> {
        Ok(&ctx.parallel_condition_wait)
    }

    fn condition_wait_mut<'a>(&self, ctx: &'a mut Context) -> Result<&'a mut ConditionWait, Flow> {
        Ok(&mut ctx.parallel_condition_wait)
    }

    fn push_wait_roots(&self, ctx: &mut Context, _condition: CondVarHandle) {
        ctx.roots_depth += 1;
    }

    fn leave_wait_cleanup(&self, ctx: &mut Context, key: ThreadKey) {
        if std::thread::panicking() {
            return;
        }
        ctx.threads.clear_thread_blocker(key.id());
        ctx.parallel_condition_wait = ConditionWait::Completed;
        assert_eq!(
            ctx.roots_depth, 1,
            "VM root frame must be balanced on leave"
        );
        ctx.roots_depth -= 1;
    }

    fn wake_mutex_waiters(&self, ctx: &mut Context, mutex: MutexId) -> Result<(), Flow> {
        let mut s = ctx
            .threads
            .world
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(s.turn_holder, Some(ctx.owner_key));
        WorldB::wake_for_locked(&mut s, Wait::Mutex(mutex));
        Ok(())
    }

    fn park_condition(
        &self,
        ctx: &mut Context,
        key: ThreadKey,
        condition: ConditionId,
    ) -> Result<(), Flow> {
        assert!(
            ctx.roots_depth > 0,
            "condition must stay rooted while parked"
        );
        ctx.obs_park_condition.fetch_add(1, StdOrdering::Relaxed);
        let wake = ctx.threads.world.owner_wake(key);
        {
            let mut s = ctx
                .threads
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(key));
            s.wait_condvars.insert(key.id(), WaitCondvar::Condition);
            wake.clear();
            s.waits.insert(key, Wait::Condition(condition));
            s.turn_holder = None;
            ctx.threads.world.turn_cv.notify_one();
        }
        wake.wait();
        ctx.threads.world.acquire_turn(key);
        {
            let mut s = ctx
                .threads
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            s.wait_condvars.remove(&key.id());
            if s.signal_sent && s.notified_count == 0 {
                ctx.obs_signal_woke_wait.fetch_add(1, StdOrdering::Relaxed);
            } else if s.notified_count > 0 && !s.signal_sent {
                ctx.obs_notify_before_signal
                    .fetch_add(1, StdOrdering::Relaxed);
            }
        }
        Ok(())
    }

    fn park_mutex_relock(
        &self,
        ctx: &mut Context,
        key: ThreadKey,
        mutex: MutexId,
    ) -> Result<(), Flow> {
        assert!(
            ctx.roots_depth > 0,
            "mutex must stay rooted during relock park"
        );
        ctx.obs_park_mutex_relock.fetch_add(1, StdOrdering::Relaxed);
        let wake = ctx.threads.world.owner_wake(key);
        {
            let mut s = ctx
                .threads
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(key));
            s.wait_condvars.insert(key.id(), WaitCondvar::Mutex);
            wake.clear();
            s.waits.insert(key, Wait::Mutex(mutex));
            s.turn_holder = None;
            ctx.threads.world.turn_cv.notify_one();
        }
        wake.wait();
        ctx.threads.world.acquire_turn(key);
        {
            let mut s = ctx
                .threads
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            s.wait_condvars.remove(&key.id());
        }
        Ok(())
    }

    fn deliver_signal(&self, ctx: &mut Context, key: ThreadKey) -> Result<(), Flow> {
        let hint = ctx.threads.world.signal_hint(key);
        let pending = {
            let mut s = ctx
                .threads
                .world
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            assert_eq!(s.turn_holder, Some(key));
            // GNU src/thread.c:470-475 invariant: recursive mutex
            // ownership MUST be restored at exact saved depth BEFORE
            // any pending signal is delivered.
            assert_eq!(
                (s.mutex_owner, s.mutex_depth),
                (Some(key), ctx.expected_depth),
                "signal delivered before saved recursive mutex depth restored"
            );
            s.pending_signals.remove(&key.id())
        };
        if let Some(flow) = pending {
            assert!(
                hint.check_acquire(),
                "SignalHint must be raised whenever pending_signals has an entry"
            );
            hint.clear_release();
            ctx.deliveries += 1;
            assert_eq!(ctx.deliveries, 1, "signal delivered more than once");
            ctx.obs_signal_after_relock
                .fetch_add(1, StdOrdering::Relaxed);
            return Err(flow);
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub(crate) struct CoverageB {
    pub(crate) executions: std::sync::Arc<StdAtomicUsize>,
    pub(crate) entered_wait_with_seat: std::sync::Arc<StdAtomicUsize>,
    pub(crate) entered_park_condition: std::sync::Arc<StdAtomicUsize>,
    pub(crate) entered_park_mutex_relock: std::sync::Arc<StdAtomicUsize>,
    pub(crate) signal_delivered_after_exact_depth: std::sync::Arc<StdAtomicUsize>,
    pub(crate) notify_before_signal: std::sync::Arc<StdAtomicUsize>,
    pub(crate) signal_woke_wait: std::sync::Arc<StdAtomicUsize>,
}

/// Focused 2-thread probe (A enters `wait_with_seat` while B is
/// already parked on `Wait::Mutex(M)`): asserts `wait_with_seat`
/// wakes mutex waiters after `ValidatedWait::unlock` (proving
/// `condition_wait_drop_wake_mutex.rs` fails loudly with an exact
/// assertion instead of deadlocking).
#[allow(dead_code)]
pub(crate) fn run_mutex_wake_probe() {
    model_with_bound(move || {
        let owner_a = ThreadKey(1);
        let owner_b = ThreadKey(2);
        let cond_key = 100;
        let mutex_key = 200;
        let saved_depth = 2u32;

        let world = WorldB::new(owner_a, owner_b, saved_depth);
        let wake_b = world.owner_wake(owner_b);
        // Pre-register B as waiting on mutex M (contending while A holds M).
        {
            let mut s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
            wake_b.clear();
            s.waits
                .insert(owner_b, Wait::Mutex(MutexId::new(mutex_key)));
        }

        let (b_checked_tx, b_checked_rx) = crate::sync::mpsc::channel::<()>();
        let world_b = Arc::clone(&world);
        let tb = loom::thread::spawn(move || {
            // Wait until A has entered `park_condition` (after unlock).
            b_checked_rx.recv().unwrap();
            assert!(
                wake_b.try_take(),
                "condition-wait failed to wake mutex waiter"
            );
            world_b.acquire_turn(owner_b);
            {
                let mut s = world_b.state.lock().unwrap_or_else(PoisonError::into_inner);
                assert!(s.mutex_owner.is_none());
                s.mutex_owner = Some(owner_b);
                s.mutex_depth = 1;
                WorldB::wake_for_locked(&mut s, Wait::Condition(ConditionId::new(cond_key)));
                s.mutex_owner = None;
                s.mutex_depth = 0;
                WorldB::wake_for_locked(&mut s, Wait::Mutex(MutexId::new(mutex_key)));
            }
            world_b.release_turn(owner_b);
        });

        // Custom seat that notifies B once `park_condition` is reached.
        struct ProbeSeat(crate::sync::mpsc::Sender<()>);
        impl ConditionSeat for ProbeSeat {
            fn key(&self, ctx: &mut Context) -> Result<ThreadKey, Flow> {
                ParallelModelSeat.key(ctx)
            }
            fn condition_wait<'a>(&self, ctx: &'a mut Context) -> Result<&'a ConditionWait, Flow> {
                ParallelModelSeat.condition_wait(ctx)
            }
            fn condition_wait_mut<'a>(
                &self,
                ctx: &'a mut Context,
            ) -> Result<&'a mut ConditionWait, Flow> {
                ParallelModelSeat.condition_wait_mut(ctx)
            }
            fn push_wait_roots(&self, ctx: &mut Context, c: CondVarHandle) {
                ParallelModelSeat.push_wait_roots(ctx, c)
            }
            fn leave_wait_cleanup(&self, ctx: &mut Context, k: ThreadKey) {
                ParallelModelSeat.leave_wait_cleanup(ctx, k)
            }
            fn wake_mutex_waiters(&self, ctx: &mut Context, m: MutexId) -> Result<(), Flow> {
                ParallelModelSeat.wake_mutex_waiters(ctx, m)
            }
            fn park_condition(
                &self,
                ctx: &mut Context,
                key: ThreadKey,
                condition: ConditionId,
            ) -> Result<(), Flow> {
                let wake = ctx.threads.world.owner_wake(key);
                {
                    let mut s = ctx
                        .threads
                        .world
                        .state
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    wake.clear();
                    s.waits.insert(key, Wait::Condition(condition));
                    s.turn_holder = None;
                    ctx.threads.world.turn_cv.notify_one();
                }
                let _ = self.0.send(());
                wake.wait();
                ctx.threads.world.acquire_turn(key);
                Ok(())
            }
            fn park_mutex_relock(
                &self,
                ctx: &mut Context,
                k: ThreadKey,
                m: MutexId,
            ) -> Result<(), Flow> {
                ParallelModelSeat.park_mutex_relock(ctx, k, m)
            }
            fn deliver_signal(&self, ctx: &mut Context, k: ThreadKey) -> Result<(), Flow> {
                ParallelModelSeat.deliver_signal(ctx, k)
            }
        }

        let zero = std::sync::Arc::new(StdAtomicUsize::new(0));
        world.acquire_turn(owner_a);
        let mut ctx = Context {
            threads: ThreadManager {
                world: Arc::clone(&world),
                current: owner_a,
                cond_key,
                mutex_key,
            },
            owner_key: owner_a,
            parallel_condition_wait: ConditionWait::Completed,
            roots_depth: 0,
            deliveries: 0,
            expected_depth: saved_depth,
            obs_park_condition: std::sync::Arc::clone(&zero),
            obs_park_mutex_relock: std::sync::Arc::clone(&zero),
            obs_signal_after_relock: std::sync::Arc::clone(&zero),
            obs_notify_before_signal: std::sync::Arc::clone(&zero),
            obs_signal_woke_wait: std::sync::Arc::clone(&zero),
        };
        let res = wait_with_seat(
            &mut ctx,
            ProbeSeat(b_checked_tx),
            CondVarHandle { key: cond_key },
        );
        assert!(res.is_ok());
        world.release_turn(owner_a);
        tb.join().unwrap();
    });
}

#[allow(dead_code)]
pub(crate) fn run() -> CoverageB {
    let executions = std::sync::Arc::new(StdAtomicUsize::new(0));
    let entered_wait_with_seat = std::sync::Arc::new(StdAtomicUsize::new(0));
    let entered_park_condition = std::sync::Arc::new(StdAtomicUsize::new(0));
    let entered_park_mutex_relock = std::sync::Arc::new(StdAtomicUsize::new(0));
    let signal_delivered_after_exact_depth = std::sync::Arc::new(StdAtomicUsize::new(0));
    let notify_before_signal = std::sync::Arc::new(StdAtomicUsize::new(0));
    let signal_woke_wait = std::sync::Arc::new(StdAtomicUsize::new(0));

    let c_exec = std::sync::Arc::clone(&executions);
    let c_wait = std::sync::Arc::clone(&entered_wait_with_seat);
    let c_pcond = std::sync::Arc::clone(&entered_park_condition);
    let c_prelock = std::sync::Arc::clone(&entered_park_mutex_relock);
    let c_sig = std::sync::Arc::clone(&signal_delivered_after_exact_depth);
    let c_nbs = std::sync::Arc::clone(&notify_before_signal);
    let c_sw = std::sync::Arc::clone(&signal_woke_wait);

    model_with_bound(move || {
        c_exec.fetch_add(1, StdOrdering::Relaxed);
        let owner_a = ThreadKey(1);
        let owner_b = ThreadKey(2);
        let owner_c = ThreadKey(3);
        let cond_key = 100;
        let mutex_key = 200;
        let saved_depth = 3u32;

        let world = WorldB::new(owner_a, owner_b, saved_depth);

        // Owner B: acquires mutex M (contending with A if A has not yet
        // entered `wait_with_seat`), notifies cond, yields turn WHILE
        // STILL HOLDING M (forcing A into `park_mutex_relock`), then
        // unlocks M in a second turn and wakes mutex waiters.
        let world_b = Arc::clone(&world);
        let tb = loom::thread::spawn(move || {
            let wake_b = world_b.owner_wake(owner_b);
            world_b.acquire_turn(owner_b);
            loop {
                let acquired = {
                    let mut s = world_b.state.lock().unwrap_or_else(PoisonError::into_inner);
                    assert_eq!(s.turn_holder, Some(owner_b));
                    if s.mutex_owner.is_none() {
                        s.mutex_owner = Some(owner_b);
                        s.mutex_depth = 1;
                        true
                    } else {
                        wake_b.clear();
                        s.waits
                            .insert(owner_b, Wait::Mutex(MutexId::new(mutex_key)));
                        s.turn_holder = None;
                        world_b.turn_cv.notify_one();
                        false
                    }
                };
                if acquired {
                    break;
                }
                wake_b.wait();
                world_b.acquire_turn(owner_b);
            }
            // Notify condition while holding mutex M.
            {
                let mut s = world_b.state.lock().unwrap_or_else(PoisonError::into_inner);
                s.notified_count += 1;
                WorldB::wake_for_locked(&mut s, Wait::Condition(ConditionId::new(cond_key)));
            }
            // Deliberately release the serial turn WHILE STILL HOLDING M so
            // A can wake and hit contended `park_mutex_relock`.
            world_b.release_turn(owner_b);

            // Second turn: unlock M and wake mutex waiters (including A's relock).
            world_b.acquire_turn(owner_b);
            {
                let mut s = world_b.state.lock().unwrap_or_else(PoisonError::into_inner);
                assert_eq!((s.mutex_owner, s.mutex_depth), (Some(owner_b), 1));
                s.mutex_owner = None;
                s.mutex_depth = 0;
                WorldB::wake_for_locked(&mut s, Wait::Mutex(MutexId::new(mutex_key)));
            }
            world_b.release_turn(owner_b);
        });

        // Owner C: sends `thread-signal` to A (stores pending error, raises
        // `SignalHint` Release, wakes A if waiting in Condition or Mutex).
        let world_c = Arc::clone(&world);
        let tc = loom::thread::spawn(move || {
            let hint_a = world_c.signal_hint(owner_a);
            world_c.acquire_turn(owner_c);
            {
                let mut s = world_c.state.lock().unwrap_or_else(PoisonError::into_inner);
                s.pending_signals
                    .insert(owner_a.id(), Flow { symbol: "quit" });
                s.signal_sent = true;
                hint_a.raise();
                if matches!(
                    s.waits.get(&owner_a),
                    Some(Wait::Condition(_) | Wait::Mutex(_) | Wait::Join(_))
                ) {
                    if let Some(Participant::Parallel(p)) = s.participants.get(&owner_a) {
                        let wake = p.wake.clone();
                        s.waits.insert(owner_a, Wait::ParallelActive);
                        wake.wake();
                    }
                }
            }
            world_c.release_turn(owner_c);
        });

        // Owner A: runs the REAL copied product `condition_wait::wait_with_seat`!
        world.acquire_turn(owner_a);
        let mut ctx = Context {
            threads: ThreadManager {
                world: Arc::clone(&world),
                current: owner_a,
                cond_key,
                mutex_key,
            },
            owner_key: owner_a,
            parallel_condition_wait: ConditionWait::Completed,
            roots_depth: 0,
            deliveries: 0,
            expected_depth: saved_depth,
            obs_park_condition: std::sync::Arc::clone(&c_pcond),
            obs_park_mutex_relock: std::sync::Arc::clone(&c_prelock),
            obs_signal_after_relock: std::sync::Arc::clone(&c_sig),
            obs_notify_before_signal: std::sync::Arc::clone(&c_nbs),
            obs_signal_woke_wait: std::sync::Arc::clone(&c_sw),
        };
        c_wait.fetch_add(1, StdOrdering::Relaxed);
        let res = wait_with_seat(&mut ctx, ParallelModelSeat, CondVarHandle { key: cond_key });
        // Post-condition 1: WaitOwner::drop cleaned up blocker, root frame, and state.
        assert_eq!(ctx.roots_depth, 0, "VM root frame leaked");
        assert!(matches!(
            ctx.parallel_condition_wait,
            ConditionWait::Completed
        ));
        // Post-condition 2: Mutex ownership restored at EXACT saved depth 3
        // whether `wait_with_seat` returned Ok or Err(signal).
        {
            let mut s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
            assert_eq!(
                (s.mutex_owner, s.mutex_depth),
                (Some(owner_a), saved_depth),
                "exact saved recursive depth 3 must be held on exit from condition-wait"
            );
            assert!(!s.blockers.contains_key(&owner_a.id()));
            // Release A's 3-deep lock so any remaining turn finishes cleanly.
            s.mutex_owner = None;
            s.mutex_depth = 0;
            WorldB::wake_for_locked(&mut s, Wait::Mutex(MutexId::new(mutex_key)));
        }
        world.release_turn(owner_a);

        tb.join().unwrap();
        tc.join().unwrap();

        // Post-condition 3: Exactly one signal delivery total (either delivered
        // inside `wait_with_seat` after relock, or C ran after A exited
        // `wait_with_seat` and left exactly one pending signal + raised hint).
        let hint_a = world.signal_hint(owner_a);
        let s = world.state.lock().unwrap_or_else(PoisonError::into_inner);
        match res {
            Err(flow) => {
                assert_eq!(flow.symbol, "quit");
                assert_eq!(ctx.deliveries, 1);
                assert!(
                    !s.pending_signals.contains_key(&owner_a.id()),
                    "delivered signal must be drained"
                );
                assert!(
                    !hint_a.peek_relaxed(),
                    "SignalHint must be cleared after drain"
                );
            }
            Ok(_) => {
                assert_eq!(ctx.deliveries, 0);
                assert_eq!(
                    s.pending_signals.get(&owner_a.id()).map(|f| f.symbol),
                    Some("quit"),
                    "if C ran after wait_with_seat, signal must remain pending once"
                );
                assert!(hint_a.check_acquire(), "SignalHint must remain raised");
            }
        }
    });

    CoverageB {
        executions,
        entered_wait_with_seat,
        entered_park_condition,
        entered_park_mutex_relock,
        signal_delivered_after_exact_depth,
        notify_before_signal,
        signal_woke_wait,
    }
}
