//! Who holds the Baton next (ported from F4's `Control`, research branch
//! `facade_threads.rs`).
//!
//! Each Lisp thread has a key and a channel; a parked thread blocks in its
//! channel's receive until the Baton arrives. The holder hands the Baton on
//! only at a switch point: it records what it waits for, picks the first
//! ready thread (FIFO, as GNU's runnable queue) and sends it the Baton.
//! A thread waiting for a mutex, a condition, another thread's end or a
//! deadline is not ready until another thread makes it so.

use super::super::parallel_entry::driver_core::RingWaiter;
use super::super::parallel_entry::keeper_driver::{
    CollectRefusal, GroupPort, GroupRequestSender, WorldLend,
};
use super::super::parallel_entry::serial_park::PortRefusal;
use super::super::parallel_entry::world_seat::prove_lent_world_stowed;
use super::super::parallel_entry::world_turn::{WorldReturned, WorldTurn};
use super::super::world::{Baton, EraWorldHandle, TurnRight};
use super::host_service::{
    HostParkRegistration, HostRequestKey, HostRequester, RealWaitPermit, SettledHostCall,
};
#[cfg(any(test, feature = "test-support"))]
use crate::emacs_core::eval::data::process_state::{
    MainConnectPending, PendingConnectObservationError,
};
#[cfg(any(test, feature = "test-support"))]
use neovm_value::heap_types::ProcessId;
use neovm_value::tagged::gc::CollectionReport;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

mod completion_exit;

/// A Lisp thread's identity in the scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ThreadKey {
    /// The thread the World started on (GNU's `main-thread`).
    Main,
    Worker(NonZeroU64),
}

impl ThreadKey {
    /// The thread's id in the World's `ThreadManager` (0 for main).
    #[doc(hidden)]
    pub fn id(self) -> u64 {
        match self {
            Self::Main => 0,
            Self::Worker(id) => id.get(),
        }
    }

    /// The key of the thread with ID in the World's `ThreadManager`.
    #[doc(hidden)]
    pub fn from_id(id: u64) -> Self {
        NonZeroU64::new(id).map_or(Self::Main, Self::Worker)
    }
}

/// A mutex object id (the Lisp handle's numeric id). Distinct from a
/// condition id at the scheduler boundary; converted once in `api.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MutexId(u64);

impl MutexId {
    #[doc(hidden)]
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// A condition-variable object id (the Lisp handle's numeric id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ConditionId(u64);

impl ConditionId {
    #[doc(hidden)]
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// What a thread that gave the Baton up waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Nothing: it runs again in its turn.
    Runnable,
    /// Another thread's end (`thread-join`).
    Join(ThreadKey),
    /// A mutex another thread holds (`mutex-lock`).
    Mutex(MutexId),
    /// A condition variable's notification (`condition-wait`).
    Condition(ConditionId),
    /// A `thread_select` wait, ready on input, a scheduler event, or its
    /// absolute deadline. The scheduler watches I/O while no owner runs.
    Select { wait_end: Instant },
    /// A rooted host request; only the service lifecycle can complete it.
    HostRequest(HostRequestKey),
    /// It ended.
    Exited,
}

/// One Select wait in the deadline index. Field order is the observable
/// order: earliest deadline first, `ThreadKey` for ties (the b262 pin).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DeadlineKey {
    wait_end: Instant,
    key: ThreadKey,
}

/// Identity of one queued ready turn. A stale entry's ticket never matches
/// its key's slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReadyTicket(NonZeroU64);

/// Whether KEY's current ready turn is queued, and under which ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadySlot {
    Queued(ReadyTicket),
    NotQueued,
}

/// What the other threads are doing, seen by a thread about to wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Peers {
    /// One can run now.
    Ready,
    /// None can run before this deadline, the earliest one sleeps to.
    Sleeping(Instant),
    /// None can run until a running thread wakes it.
    Blocked,
}

/// Whether the runtime still runs threads, or is shutting down because its
/// main thread ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lifecycle {
    Running,
    Closing,
}

/// Why a hand-off did not happen.
#[derive(Debug)]
pub(crate) enum Handoff {
    /// No other thread can run and the caller waits for something only a
    /// running thread could give it: GNU would hang (neomacs signals).
    /// `invariant` is the wait-index predicate evaluated under the same state
    /// lock that decided no owner could run; carrying it out of that critical
    /// section keeps a later transition from rewriting the evidence.
    NoRunnable {
        baton: TurnRight,
        invariant: Result<(), SchedulerIndexInconsistency>,
    },
}

/// One violated wait-index invariant. Cold-path diagnostics only; it never
/// repairs the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WaitIndexKind {
    SelectDeadline,
    SelectKeys,
    JoinWaiters,
    MutexWaiters,
    ConditionWaiters,
    RunnableCount,
    SelectIndexCount,
    SelectKeysCount,
    JoinWaiterCount,
    MutexWaiterCount,
    ConditionWaiterCount,
    ReadySlotCount,
    ReadySlotEntry,
    ReadyKeyNotRunnable,
}

/// The first violated wait-index invariant: the index's view of KEY and the
/// wait map's view of the same key (or the two counts for a count check).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SchedulerIndexInconsistency {
    pub(crate) kind: WaitIndexKind,
    pub(crate) key: Option<ThreadKey>,
    pub(crate) index_view: String,
    pub(crate) wait_view: String,
}

impl SchedulerIndexInconsistency {
    #[doc(hidden)]
    pub fn new(
        kind: WaitIndexKind,
        key: Option<ThreadKey>,
        index_view: String,
        wait_view: String,
    ) -> Self {
        Self {
            kind,
            key,
            index_view,
            wait_view,
        }
    }
}

impl std::fmt::Display for SchedulerIndexInconsistency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)?;
        if let Some(key) = self.key {
            write!(f, " key={key:?}")?;
        }
        write!(f, ": index={} wait-map={}", self.index_view, self.wait_view)
    }
}

/// Test-only fault hook for the release verdict test: skip exactly one
/// Condition index removal, reproducing the released SCHEDIDX bug without a
/// production knob. Absent from non-test builds, and from debug test builds
/// where the per-transition checker would catch the skipped removal first.
#[cfg(all(test, not(debug_assertions)))]
static SKIP_NEXT_CONDITION_INDEX_REMOVAL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(all(test, not(debug_assertions)))]
pub(crate) fn skip_next_condition_index_removal_for_test() {
    SKIP_NEXT_CONDITION_INDEX_REMOVAL.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// A native service turn does not resume main's suspended Lisp continuation.
#[derive(Debug)]
#[must_use]
pub(super) enum Wake {
    Resume(TurnRight),
    ServiceTurn {
        key: HostRequestKey,
        baton: TurnRight,
    },
    /// Resume the exact published owner step, without a terminal or signal hop.
    OwnerStep {
        key: HostRequestKey,
        baton: TurnRight,
    },
    /// Continue an admitted semantic operation; do not dispatch its actor again.
    ContinuationTurn {
        key: HostRequestKey,
        baton: TurnRight,
    },
    /// Reenter the owned native driver without restoring main's Lisp owner.
    NativePump {
        baton: TurnRight,
    },
}

impl Wake {
    pub(crate) fn baton(&self) -> &TurnRight {
        match self {
            Self::Resume(baton)
            | Self::ServiceTurn { baton, .. }
            | Self::OwnerStep { baton, .. }
            | Self::ContinuationTurn { baton, .. }
            | Self::NativePump { baton } => baton,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerLocation {
    Running,
    Receiving,
    NativeReleased,
    NativeDriving,
    Serving(HostRequestKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServicePhase {
    Reserved,
    Serving,
    PendingNative,
    RealWait,
    Reacquiring,
    ServingAfterWait,
    Continuing(ContinuationEligibility),
    Resuming(ContinuationEligibility),
    Ended(EndedContinuationPhase),
}

/// The retirement seam preserves the exact continuation phase it observed.
/// Absence from the indexed wait registry cannot construct this authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndedContinuationPhase {
    Continuing(ContinuationEligibility),
    Resuming(ContinuationEligibility),
}

/// Genuine GNU release history survives any number of intermediate hops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContinuationEligibility {
    Reserved,
    Released,
}

#[derive(Clone, Copy, Debug)]
struct ServiceMetadata {
    requester: ThreadKey,
    phase: ServicePhase,
}

/// Only an explicit exit fact authorizes native-only cleanup. A live or
/// unobserved owner keeps its continuation and resume authority retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContinuationProgress {
    NotContinuing,
    AwaitingOwner,
    OwnerEnded,
}

/// Linear reservation held beside the World-owned host root record.
#[derive(Debug)]
#[must_use]
pub(super) struct ReservedServiceSlice {
    key: HostRequestKey,
    requester: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}

/// Proof that this exact service's requester has ended in its owning World.
/// The held serial Baton prevents another owner transition during cleanup.
#[derive(Debug)]
#[must_use]
pub(super) struct EndedServiceOwner {
    key: HostRequestKey,
    requester: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}

impl EndedServiceOwner {
    #[doc(hidden)]
    pub fn key(&self) -> HostRequestKey {
        self.key
    }

    pub(crate) fn requester(&self) -> ThreadKey {
        self.requester
    }
}

static_assertions::assert_not_impl_any!(EndedServiceOwner: Send, Sync, Clone, Copy);

impl ReservedServiceSlice {
    #[doc(hidden)]
    pub fn key(&self) -> HostRequestKey {
        self.key
    }

    #[doc(hidden)]
    pub fn requester(&self) -> ThreadKey {
        self.requester
    }
}

static_assertions::assert_not_impl_any!(ReservedServiceSlice: Send, Sync, Clone, Copy);

/// Prepared under the registry borrow, consumed only after that borrow ends.
#[derive(Debug)]
#[must_use]
pub(super) struct ServiceTransferPlan {
    key: HostRequestKey,
    requester: HostRequester,
    _confined: PhantomData<Rc<()>>,
}
impl ServiceTransferPlan {
    #[doc(hidden)]
    pub fn requester(&self) -> HostRequester {
        self.requester
    }
}

static_assertions::assert_not_impl_any!(ServiceTransferPlan: Send, Sync, Clone, Copy);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ServiceRefusal {
    Closing,
    MainUnavailable,
    MainIsRequester,
    RequesterUnavailable,
    DuplicateRequest,
    WrongServiceState,
    WrongMainThread,
}

#[derive(Debug)]
#[must_use]
pub(super) enum ServiceTransfer {
    Dispatched,
    /// Main is inside a native release. The requester keeps the Baton in a
    /// metadata-only queued wait until main's native acquire is announced.
    Queued(TurnRight),
}

#[derive(Debug)]
#[must_use]
pub(super) struct ServiceTransferRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) baton: TurnRight,
}

#[derive(Debug)]
#[must_use]
#[allow(
    dead_code,
    reason = "rejection retains linear obligations for recovery or fail-stop"
)]
pub(super) struct ServiceReturnRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) slice: ReservedServiceSlice,
    pub(super) baton: TurnRight,
}

#[derive(Debug)]
#[must_use]
#[allow(
    dead_code,
    reason = "rejection retains linear obligations for recovery or fail-stop"
)]
pub(super) struct ServiceCancelRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) slice: ReservedServiceSlice,
    pub(super) settled: SettledHostCall,
}

/// The actual native wait holds either the idle Baton, inaccessible to Lisp,
/// or an obligation to reacquire it after another eligible owner ran.
#[derive(Debug)]
#[must_use]
pub(super) enum NativeRelease {
    Transferred,
    Idle(TurnRight),
    /// Inside an open era, Case (i) gave the `WorldTurn` back to the keeper
    /// and moved `GroupTurn` to `Idle` before the native OS wait began.
    EraIdle,
}

#[derive(Debug)]
#[must_use]
#[allow(
    dead_code,
    reason = "rejection retains linear obligations for recovery or fail-stop"
)]
pub(super) struct ServiceAcquireRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) release: NativeRelease,
}

#[derive(Debug)]
#[must_use]
pub(super) enum ServiceReturn {
    Sent,
    ResumeMain(TurnRight),
    ResumeOwnerStep {
        key: HostRequestKey,
        baton: TurnRight,
    },
}

/// Metadata prepared while the registry still owns its linear slice. It is
/// revalidated with the held Baton after every World borrow has ended.
pub(super) struct OwnerResumePlan {
    key: HostRequestKey,
    requester: ThreadKey,
    eligibility: ContinuationEligibility,
}

pub(super) struct IntermediateReturnPlan {
    key: HostRequestKey,
    requester: ThreadKey,
    eligibility: ContinuationEligibility,
}

/// A nonterminal return never clears the parent's scheduler reservation.
#[derive(Debug)]
#[must_use]
pub(super) enum IntermediateReturn {
    Sent,
    LocalMain {
        key: HostRequestKey,
        baton: TurnRight,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NextOwner {
    Resume(ThreadKey),
    Native(NativeTurn),
}

/// Outcome of waiting without a `WorldTurn` while `GroupTurn::Idle` (Case i).
enum EraIdleWait {
    Acquired(TurnRight),
    WokenByPeer(Wake),
    Deadlock {
        baton: TurnRight,
        invariant: Result<(), SchedulerIndexInconsistency>,
    },
}

/// The owner-selection loop's outcome. `NoRunnable` carries the wait-index
/// predicate evaluated inside the same critical section that found no owner;
/// a check taken after the lock is dropped could see a state a later
/// transition has already re-synced.
enum PickNext {
    Owner(NextOwner),
    NoRunnable(Result<(), SchedulerIndexInconsistency>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeTurn {
    Service(HostRequestKey),
    Continuation(HostRequestKey),
    Pump,
}

enum NativePumpChoice {
    Native(NativeTurn),
    OriginalMain,
}

/// A driver acquire is the one physical cycle's return, not a request key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeAcquire {
    Service(HostRequestKey),
    Pump,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeWork {
    Ready,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativePumpPurpose {
    Serving,
    Draining,
}

enum ReadySelection {
    NextEligible,
    /// Main already owns the idle Baton: consume only its old ready turn.
    NativeMainAcquire,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "ordinary completion is reserved for later host cancellation delivery"
)]
enum CancelPlacement {
    CurrentBatonOwner,
    OrdinaryCompletion,
}

/// The per-thread wait table plus every wait-derived index. One type owns the
/// table and its indexes so a mutation can never leave them out of step; the
/// cfg(test) module may insert directly (the pinned deadline-order tests do)
/// and the same maintenance runs.
#[derive(Default)]
struct WaitMap {
    by_key: BTreeMap<ThreadKey, Wait>,
    /// One entry per `Wait::Select`, ordered by `(wait_end, ThreadKey)`:
    /// expiry iterates in deadline order for free, ties in key order.
    select_index: BTreeMap<DeadlineKey, ()>,
    /// The same Select keys in `ThreadKey` order, for `wake_selects`'s
    /// remaining key-ordered pass after expiry.
    select_keys: BTreeSet<ThreadKey>,
    join_waiters: BTreeMap<ThreadKey, BTreeSet<ThreadKey>>,
    mutex_waiters: BTreeMap<MutexId, BTreeSet<ThreadKey>>,
    condition_waiters: BTreeMap<ConditionId, BTreeSet<ThreadKey>>,
    /// `Wait::Runnable` entries (peer amendment 1's `runnable_count`).
    runnable_count: usize,
}

impl WaitMap {
    #[doc(hidden)]
    pub fn insert(&mut self, key: ThreadKey, wait: Wait) -> Option<Wait> {
        let old = self.by_key.insert(key, wait);
        if let Some(old) = old {
            self.unindex(key, old);
        }
        self.index(key, wait);
        old
    }

    #[doc(hidden)]
    pub fn remove(&mut self, key: &ThreadKey) -> Option<Wait> {
        let old = self.by_key.remove(key)?;
        self.unindex(*key, old);
        Some(old)
    }

    pub(crate) fn runnable_count(&self) -> usize {
        self.runnable_count
    }

    pub(crate) fn select_len(&self) -> usize {
        self.select_index.len()
    }

    #[doc(hidden)]
    pub fn index(&mut self, key: ThreadKey, wait: Wait) {
        match wait {
            Wait::Runnable => self.runnable_count += 1,
            Wait::Select { wait_end } => {
                self.select_index.insert(DeadlineKey { wait_end, key }, ());
                self.select_keys.insert(key);
            }
            Wait::Join(target) => {
                self.join_waiters.entry(target).or_default().insert(key);
            }
            Wait::Mutex(id) => {
                self.mutex_waiters.entry(id).or_default().insert(key);
            }
            Wait::Condition(id) => {
                self.condition_waiters.entry(id).or_default().insert(key);
            }
            Wait::HostRequest(_) | Wait::Exited => {}
        }
    }

    pub(crate) fn unindex(&mut self, key: ThreadKey, wait: Wait) {
        match wait {
            Wait::Runnable => {
                debug_assert!(self.runnable_count > 0, "runnable_count underflow");
                self.runnable_count -= 1;
            }
            Wait::Select { wait_end } => {
                // The removals run in every build; only the checks are debug.
                // Inside `debug_assert!` they vanished from release builds and
                // left retired waits indexed, which the deadlock verdict then
                // counted as live (false "All threads are blocked").
                let indexed = self.select_index.remove(&DeadlineKey { wait_end, key });
                debug_assert!(indexed.is_some(), "Select wait was not indexed");
                let keyed = self.select_keys.remove(&key);
                debug_assert!(keyed, "Select key was not indexed");
            }
            Wait::Join(target) => Self::remove_waiter(&mut self.join_waiters, target, key),
            Wait::Mutex(id) => Self::remove_waiter(&mut self.mutex_waiters, id, key),
            Wait::Condition(id) => {
                #[cfg(all(test, not(debug_assertions)))]
                if Self::take_condition_index_skip_for_test() {
                    return;
                }
                Self::remove_waiter(&mut self.condition_waiters, id, key)
            }
            Wait::HostRequest(_) | Wait::Exited => {}
        }
    }

    #[cfg(all(test, not(debug_assertions)))]
    pub(crate) fn take_condition_index_skip_for_test() -> bool {
        SKIP_NEXT_CONDITION_INDEX_REMOVAL.swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn remove_waiter<K: Ord>(
        waiters: &mut BTreeMap<K, BTreeSet<ThreadKey>>,
        object: K,
        key: ThreadKey,
    ) {
        let Some(set) = waiters.get_mut(&object) else {
            debug_assert!(false, "waiter object missing from its index");
            return;
        };
        let removed = set.remove(&key);
        debug_assert!(removed, "waiter was not indexed under its object");
        if set.is_empty() {
            waiters.remove(&object);
        }
    }

    /// The single wait-index predicate. Debug builds run it at every
    /// transition seam; every build runs it at the cold deadlock verdict.
    /// It only reports: no mutation, no repair.
    #[doc(hidden)]
    pub fn verify(&self) -> Result<(), SchedulerIndexInconsistency> {
        let mut runnable = 0usize;
        let mut selects = 0usize;
        let mut joiners = 0usize;
        let mut mutexes = 0usize;
        let mut conditions = 0usize;
        for (key, wait) in &self.by_key {
            match *wait {
                Wait::Runnable => runnable += 1,
                Wait::Select { wait_end } => {
                    selects += 1;
                    if !self.select_index.contains_key(&DeadlineKey {
                        wait_end,
                        key: *key,
                    }) {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::SelectDeadline,
                            Some(*key),
                            format!("select_index lacks (deadline={wait_end:?}, key={key:?})"),
                            format!("by_key[{key:?}]=Select {{ wait_end: {wait_end:?} }}"),
                        ));
                    }
                    if !self.select_keys.contains(key) {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::SelectKeys,
                            Some(*key),
                            format!("select_keys lacks {key:?}"),
                            format!("by_key[{key:?}]=Select {{ wait_end: {wait_end:?} }}"),
                        ));
                    }
                }
                Wait::Join(target) => {
                    joiners += 1;
                    if !self
                        .join_waiters
                        .get(&target)
                        .is_some_and(|set| set.contains(key))
                    {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::JoinWaiters,
                            Some(*key),
                            format!("join_waiters[{target:?}] lacks {key:?}"),
                            format!("by_key[{key:?}]=Join({target:?})"),
                        ));
                    }
                }
                Wait::Mutex(id) => {
                    mutexes += 1;
                    if !self
                        .mutex_waiters
                        .get(&id)
                        .is_some_and(|set| set.contains(key))
                    {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::MutexWaiters,
                            Some(*key),
                            format!("mutex_waiters[{id:?}] lacks {key:?}"),
                            format!("by_key[{key:?}]=Mutex({id:?})"),
                        ));
                    }
                }
                Wait::Condition(id) => {
                    conditions += 1;
                    if !self
                        .condition_waiters
                        .get(&id)
                        .is_some_and(|set| set.contains(key))
                    {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::ConditionWaiters,
                            Some(*key),
                            format!("condition_waiters[{id:?}] lacks {key:?}"),
                            format!("by_key[{key:?}]=Condition({id:?})"),
                        ));
                    }
                }
                Wait::HostRequest(_) | Wait::Exited => {}
            }
        }
        // Reverse direction: no index entry may name a key whose current wait
        // is a different kind, Exited, or Runnable.
        for deadline in self.select_index.keys() {
            let DeadlineKey { wait_end, key } = *deadline;
            match self.by_key.get(&key) {
                Some(Wait::Select { wait_end: current }) if *current == wait_end => {}
                other => {
                    return Err(SchedulerIndexInconsistency::new(
                        WaitIndexKind::SelectDeadline,
                        Some(key),
                        format!("select_index has (deadline={wait_end:?}, key={key:?})"),
                        format!("by_key[{key:?}]={other:?}"),
                    ));
                }
            }
        }
        for key in &self.select_keys {
            match self.by_key.get(key) {
                Some(Wait::Select { .. }) => {}
                other => {
                    return Err(SchedulerIndexInconsistency::new(
                        WaitIndexKind::SelectKeys,
                        Some(*key),
                        format!("select_keys contains {key:?}"),
                        format!("by_key[{key:?}]={other:?}"),
                    ));
                }
            }
        }
        for (target, set) in &self.join_waiters {
            for key in set {
                match self.by_key.get(key) {
                    Some(Wait::Join(current)) if *current == *target => {}
                    other => {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::JoinWaiters,
                            Some(*key),
                            format!("join_waiters[{target:?}] contains {key:?}"),
                            format!("by_key[{key:?}]={other:?}"),
                        ));
                    }
                }
            }
        }
        for (id, set) in &self.mutex_waiters {
            for key in set {
                match self.by_key.get(key) {
                    Some(Wait::Mutex(current)) if *current == *id => {}
                    other => {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::MutexWaiters,
                            Some(*key),
                            format!("mutex_waiters[{id:?}] contains {key:?}"),
                            format!("by_key[{key:?}]={other:?}"),
                        ));
                    }
                }
            }
        }
        for (id, set) in &self.condition_waiters {
            for key in set {
                match self.by_key.get(key) {
                    Some(Wait::Condition(current)) if *current == *id => {}
                    other => {
                        return Err(SchedulerIndexInconsistency::new(
                            WaitIndexKind::ConditionWaiters,
                            Some(*key),
                            format!("condition_waiters[{id:?}] contains {key:?}"),
                            format!("by_key[{key:?}]={other:?}"),
                        ));
                    }
                }
            }
        }
        if runnable != self.runnable_count {
            return Err(SchedulerIndexInconsistency::new(
                WaitIndexKind::RunnableCount,
                None,
                format!("runnable_count={}", self.runnable_count),
                format!("Runnable waits={runnable}"),
            ));
        }
        if selects != self.select_index.len() {
            return Err(SchedulerIndexInconsistency::new(
                WaitIndexKind::SelectIndexCount,
                None,
                format!("select_index.len()={}", self.select_index.len()),
                format!("Select waits={selects}"),
            ));
        }
        if selects != self.select_keys.len() {
            return Err(SchedulerIndexInconsistency::new(
                WaitIndexKind::SelectKeysCount,
                None,
                format!("select_keys.len()={}", self.select_keys.len()),
                format!("Select waits={selects}"),
            ));
        }
        let join_entries = self.join_waiters.values().map(BTreeSet::len).sum::<usize>();
        if joiners != join_entries {
            return Err(SchedulerIndexInconsistency::new(
                WaitIndexKind::JoinWaiterCount,
                None,
                format!("join_waiters entries={join_entries}"),
                format!("Join waits={joiners}"),
            ));
        }
        let mutex_entries = self
            .mutex_waiters
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        if mutexes != mutex_entries {
            return Err(SchedulerIndexInconsistency::new(
                WaitIndexKind::MutexWaiterCount,
                None,
                format!("mutex_waiters entries={mutex_entries}"),
                format!("Mutex waits={mutexes}"),
            ));
        }
        let condition_entries = self
            .condition_waiters
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        if conditions != condition_entries {
            return Err(SchedulerIndexInconsistency::new(
                WaitIndexKind::ConditionWaiterCount,
                None,
                format!("condition_waiters entries={condition_entries}"),
                format!("Condition waits={conditions}"),
            ));
        }
        Ok(())
    }
}

impl std::ops::Deref for WaitMap {
    type Target = BTreeMap<ThreadKey, Wait>;

    fn deref(&self) -> &Self::Target {
        &self.by_key
    }
}

/// The ready FIFO plus its per-key invalidation state. One type owns the
/// queue, the slots and the valid count, so a test may push/pop bare keys
/// (the pinned regression tests do) and the pairing stays consistent.
#[derive(Default)]
struct ReadyQueue {
    entries: VecDeque<(ThreadKey, ReadyTicket)>,
    slots: BTreeMap<ThreadKey, ReadySlot>,
    valid_count: usize,
    next_ticket: u64,
}

impl ReadyQueue {
    pub(crate) fn fresh_ticket(&mut self) -> ReadyTicket {
        self.next_ticket += 1;
        ReadyTicket(NonZeroU64::new(self.next_ticket).expect("ready tickets start at one"))
    }

    /// Queue KEY at the back unless it already owns a queued turn.
    pub(crate) fn enqueue_back(&mut self, key: ThreadKey) -> bool {
        if self.is_queued(&key) {
            return false;
        }
        let ticket = self.fresh_ticket();
        self.entries.push_back((key, ticket));
        self.slots.insert(key, ReadySlot::Queued(ticket));
        self.valid_count += 1;
        true
    }

    /// Queue KEY at the front unless it already owns a queued turn.
    pub(crate) fn enqueue_front(&mut self, key: ThreadKey) -> bool {
        if self.is_queued(&key) {
            return false;
        }
        let ticket = self.fresh_ticket();
        self.entries.push_front((key, ticket));
        self.slots.insert(key, ReadySlot::Queued(ticket));
        self.valid_count += 1;
        true
    }

    /// `prioritize`: drop every existing turn for KEY, then queue a fresh one
    /// at the front (the old `ready.retain` + `push_front`).
    pub(crate) fn move_to_front(&mut self, key: ThreadKey) {
        self.discard(key);
        let ticket = self.fresh_ticket();
        self.entries.push_front((key, ticket));
        self.slots.insert(key, ReadySlot::Queued(ticket));
        self.valid_count += 1;
    }

    /// Remove every entry for KEY and its slot (the old `ready.retain`).
    #[doc(hidden)]
    pub fn discard(&mut self, key: ThreadKey) {
        self.entries.retain(|(entry, _)| *entry != key);
        if matches!(self.slots.remove(&key), Some(ReadySlot::Queued(_))) {
            self.valid_count -= 1;
        }
    }

    /// KEY's wait stopped being `Runnable`: its queued turn is stale from now
    /// on (lazy: the entry goes when the queue reaches it).
    #[doc(hidden)]
    pub fn invalidate(&mut self, key: ThreadKey) {
        if let Some(ReadySlot::Queued(_)) = self.slots.get(&key) {
            self.slots.insert(key, ReadySlot::NotQueued);
            self.valid_count -= 1;
        }
    }

    #[doc(hidden)]
    pub fn is_queued(&self, key: &ThreadKey) -> bool {
        matches!(self.slots.get(key), Some(ReadySlot::Queued(_)))
    }

    pub(crate) fn valid_count(&self) -> usize {
        self.valid_count
    }

    /// The first eligible queued turn, dropping invalid entries met on the
    /// way. An entry is valid only when its ticket still matches the slot AND
    /// its key is `Runnable` (the old `ready.retain` rule); eligible-but-
    /// ineligible entries (main in a native wait) keep their place, exactly
    /// as the old position scan did.
    pub(crate) fn pop_valid(
        &mut self,
        mut runnable: impl FnMut(ThreadKey) -> bool,
        mut eligible: impl FnMut(ThreadKey) -> bool,
    ) -> Option<ThreadKey> {
        let mut index = 0;
        while index < self.entries.len() {
            let (key, ticket) = self.entries[index];
            let queued = matches!(
                self.slots.get(&key),
                Some(ReadySlot::Queued(current)) if *current == ticket
            );
            if !queued {
                self.entries.remove(index);
                continue;
            }
            if !runnable(key) {
                // The key's wait left Runnable without the slot being
                // invalidated (a cfg(test) direct push, or a missed seam):
                // the old ready.retain dropped exactly this entry.
                self.entries.remove(index);
                self.slots.insert(key, ReadySlot::NotQueued);
                self.valid_count -= 1;
                continue;
            }
            if !eligible(key) {
                index += 1;
                continue;
            }
            self.entries.remove(index);
            self.slots.insert(key, ReadySlot::NotQueued);
            self.valid_count -= 1;
            return Some(key);
        }
        None
    }

    /// Consume main's turn only (the idle-Baton acquire path). The old code
    /// removed the first Main entry whether or not it was still valid.
    pub(crate) fn pop_main(&mut self) -> Option<ThreadKey> {
        let index = self
            .entries
            .iter()
            .position(|(key, _)| *key == ThreadKey::Main)?;
        let (key, ticket) = self.entries.remove(index)?;
        if matches!(
            self.slots.get(&key),
            Some(ReadySlot::Queued(current)) if *current == ticket
        ) {
            self.slots.insert(key, ReadySlot::NotQueued);
            self.valid_count -= 1;
        }
        Some(key)
    }

    pub(crate) fn valid_excluding(&self, me: &ThreadKey) -> usize {
        self.valid_count - usize::from(self.is_queued(me))
    }

    /// The ready part of the shared invariant predicate: a queued slot has a
    /// matching entry and belongs to a Runnable key; valid_count is exact.
    #[doc(hidden)]
    pub fn verify(&self, waits: &WaitMap) -> Result<(), SchedulerIndexInconsistency> {
        let queued = self
            .slots
            .values()
            .filter(|slot| matches!(slot, ReadySlot::Queued(_)))
            .count();
        if queued != self.valid_count {
            return Err(SchedulerIndexInconsistency::new(
                WaitIndexKind::ReadySlotCount,
                None,
                format!("valid_count={}", self.valid_count),
                format!("queued slots={queued}"),
            ));
        }
        for (key, slot) in &self.slots {
            if let ReadySlot::Queued(ticket) = slot {
                if !self
                    .entries
                    .iter()
                    .any(|(entry, entry_ticket)| entry == key && entry_ticket == ticket)
                {
                    return Err(SchedulerIndexInconsistency::new(
                        WaitIndexKind::ReadySlotEntry,
                        Some(*key),
                        format!("slots[{key:?}]=Queued({ticket:?})"),
                        "ready entries hold no matching ticket".to_owned(),
                    ));
                }
                if waits.get(key) != Some(&Wait::Runnable) {
                    return Err(SchedulerIndexInconsistency::new(
                        WaitIndexKind::ReadyKeyNotRunnable,
                        Some(*key),
                        format!("ready slot Queued({ticket:?})"),
                        format!("by_key[{key:?}]={:?}", waits.get(key)),
                    ));
                }
            }
        }
        Ok(())
    }

    // --- cfg(test) surface for the pinned scheduler tests ----------------
    #[cfg(test)]
    #[doc(hidden)]
    pub fn push_back(&mut self, key: ThreadKey) {
        let ticket = self.fresh_ticket();
        self.entries.push_back((key, ticket));
        if !matches!(
            self.slots.insert(key, ReadySlot::Queued(ticket)),
            Some(ReadySlot::Queued(_))
        ) {
            self.valid_count += 1;
        }
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn pop_front(&mut self) -> Option<ThreadKey> {
        let (key, ticket) = self.entries.pop_front()?;
        if matches!(
            self.slots.get(&key),
            Some(ReadySlot::Queued(current)) if *current == ticket
        ) {
            self.slots.insert(key, ReadySlot::NotQueued);
            self.valid_count -= 1;
        }
        Some(key)
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn front(&self) -> Option<&ThreadKey> {
        self.entries.front().map(|(key, _)| key)
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn iter(&self) -> impl Iterator<Item = &ThreadKey> {
        self.entries.iter().map(|(key, _)| key)
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The valid queued keys in FIFO order (the differential's ready view).
    #[cfg(test)]
    pub(crate) fn valid_keys(&self) -> Vec<ThreadKey> {
        self.entries
            .iter()
            .filter(|(key, ticket)| {
                matches!(
                    self.slots.get(key),
                    Some(ReadySlot::Queued(current)) if current == ticket
                )
            })
            .map(|(key, _)| *key)
            .collect()
    }
}

/// Why a Green `Control` refused to surrender `Solo(Baton)` or open a parallel
/// era (L6.7b r1 §2, QD1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EraOpenError {
    /// A Green thread still holds `TurnRight::Solo(Baton)` (QD1).
    SoloBatonHeld,
    /// `Solo(Baton)` was already surrendered for an era admission.
    AlreadySurrendered,
    /// A parallel era is already open on this `Control`.
    AlreadyOpen,
    /// The Green runtime is closing.
    Closing,
}

/// Why closing a parallel era on `Control` was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EraCloseError {
    /// No parallel era is currently open on this `Control`.
    NotOpen,
}

/// Typed QD1 check failure when a `Wake` carries the wrong `TurnRight` variant
/// for the scheduler's current era state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WakeAuthorityError {
    /// An era is open; every era-time wake must carry `TurnRight::Era` (QD1).
    SoloWakeInOpenEra,
    /// No era is open; every non-era wake must carry `TurnRight::Solo`.
    EraWakeOutsideEra,
    /// `Solo(Baton)` was surrendered for era admission and no era is open yet.
    SoloSurrendered,
}

/// Baton-group turn state inside an open parallel era (L6.7b r1 §3, b4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupTurn {
    Idle,
    Requested,
    Holding,
    /// A Baton thread ended its interval via `GroupPort::collect` and is
    /// waiting for the keeper's ring-HEAD re-lend (b4.4 / b5 QB3). Kept
    /// distinct from `Idle` so concurrently woken sibling Baton threads join
    /// the FIFO without sending a duplicate `BatonRequest`.
    Collecting,
}

/// Era state attached to `ControlState` while a parallel era is open.
pub(crate) struct EraLink {
    world: EraWorldHandle,
    waiter: Arc<RingWaiter>,
    requester: Option<GroupRequestSender>,
    /// Owned by `EraLink` except while one Baton thread is actively blocking
    /// in `GroupPort::await_world()`.
    port: Option<GroupPort>,
    turn: GroupTurn,
}

impl std::fmt::Debug for EraLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EraLink")
            .field("world", &self.world)
            .field("ring_waiter", &self.waiter.raised())
            .field("port_present", &self.port.is_some())
            .field("turn", &self.turn)
            .finish_non_exhaustive()
    }
}

/// Which turn authority governs Green switch points (L6.7b r1 §2, QD1).
#[derive(Debug, Default)]
enum EraState {
    /// Outside an era: Green holds `TurnRight::Solo(Baton)`.
    #[default]
    SoloActive,
    /// `TurnRight::Solo(Baton)` has been linearly surrendered out of Green so
    /// the keeper can escrow the `Baton` in a parallel era.
    SoloSurrendered,
    /// A parallel era is open; every Green turn and wake must carry
    /// `TurnRight::Era` (QD1).
    EraOpen(EraLink),
}

#[derive(Default)]
struct ControlState {
    // There is only one Baton in the World, so a sender holding it proves
    // every recipient queue empty. A single reusable slot also allows a
    // newborn or parked recipient to receive after the sender has finished.
    senders: BTreeMap<ThreadKey, mpsc::SyncSender<Wake>>,
    waits: WaitMap,
    ready: ReadyQueue,
    closing: bool,
    era: EraState,
    physical: BTreeMap<ThreadKey, OwnerLocation>,
    services: BTreeMap<HostRequestKey, ServiceMetadata>,
    pending_services: VecDeque<HostRequestKey>,
    pending_acquires: VecDeque<NativeAcquire>,
    main_os_thread: Option<std::thread::ThreadId>,
}

impl ControlState {
    /// The one wait-index predicate: wait map and ready queue. Debug builds
    /// call it at every transition seam; release builds call it only at the
    /// cold deadlock verdict. It only reports; it never repairs.
    pub(crate) fn verify_invariants(&self) -> Result<(), SchedulerIndexInconsistency> {
        self.waits.verify()?;
        self.ready.verify(&self.waits)?;
        Ok(())
    }
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, thiserror::Error)]
pub(crate) enum SelectParkObservationError {
    #[error("select park gate lock poisoned")]
    GatePoisoned,
    #[error("MainConnectPending requires the Main owner")]
    NotMain,
    #[error("select park gate prepared twice")]
    AlreadyPrepared,
    #[error(transparent)]
    PendingConnect(#[from] PendingConnectObservationError),
}
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(SelectParkObservationError: std::error::Error, Send, Sync);

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectParkExpectation {
    Ordinary,
    MainConnectPending(ProcessId),
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub(crate) enum SelectParkFacts {
    Ordinary,
    MainConnectPending(MainConnectPending),
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub(crate) struct SelectParked {
    owner: ThreadKey,
    wait_end: Instant,
    facts: SelectParkFacts,
}

#[cfg(any(test, feature = "test-support"))]
impl SelectParked {
    #[doc(hidden)]
    pub fn owner(&self) -> ThreadKey {
        self.owner
    }
    pub(crate) fn wait_end(&self) -> Instant {
        self.wait_end
    }
    pub(crate) fn into_facts(self) -> SelectParkFacts {
        self.facts
    }
}

#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(SelectParked: Send, std::fmt::Debug);
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(SelectParked: Clone, Copy);

#[cfg(any(test, feature = "test-support"))]
type SelectParkCallback = Box<dyn FnOnce(SelectParked) + Send>;

#[cfg(any(test, feature = "test-support"))]
enum SelectParkGate {
    Armed {
        owner: ThreadKey,
        expectation: SelectParkExpectation,
        callback: SelectParkCallback,
    },
    Prepared {
        owner: ThreadKey,
        facts: SelectParkFacts,
        callback: SelectParkCallback,
    },
}

#[cfg(any(test, feature = "test-support"))]
impl std::fmt::Debug for SelectParkGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Armed {
                owner,
                expectation,
                callback: _,
            } => f
                .debug_struct("ArmedSelectParkGate")
                .field("owner", owner)
                .field("expectation", expectation)
                .finish_non_exhaustive(),
            Self::Prepared {
                owner,
                facts,
                callback: _,
            } => f
                .debug_struct("PreparedSelectParkGate")
                .field("owner", owner)
                .field("facts", facts)
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(SelectParkGate: Send, std::fmt::Debug);

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainNativeWaitEvent {
    Released,
    PhysicalBlockEntered,
    PhysicalBlockReturned,
    Reacquired,
}

/// The scheduler, shared by every thread's seat.
#[derive(Default)]
pub(crate) struct Control {
    state: Mutex<ControlState>,
    #[cfg(any(test, feature = "test-support"))]
    main_native_events: Mutex<Vec<MainNativeWaitEvent>>,
    metadata_changed: Condvar,
    #[cfg(any(test, feature = "test-support"))]
    after_next_select_park: Mutex<Option<SelectParkGate>>,
    /// An owned I/O handle: the idle scheduler never reads World storage.
    select: OnceLock<Option<crate::emacs_core::eval::process_hooks::SelectHandleToken>>,
    /// Counts Acquire loads of the keeper's ring-waiter word at Green switch
    /// points (QB2 condition: stays 0 on a Baton-only World).
    #[cfg(test)]
    ring_waiter_loads: std::sync::atomic::AtomicUsize,
}

impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Control").finish_non_exhaustive()
    }
}

impl Control {
    #[cfg(test)]
    pub(crate) fn ring_waiter_loads_for_test(&self) -> usize {
        self.ring_waiter_loads
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn reset_ring_waiter_loads_for_test(&self) {
        self.ring_waiter_loads
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn park_worker_for_coalesce_test(&self, key: ThreadKey, wait: Wait) {
        let mut state = self.state();
        Self::set_wait(&mut state, key, wait);
    }

    #[cfg(test)]
    pub(crate) fn register_main_guard_for_test(&self) -> Box<dyn std::any::Any> {
        Box::new(self.register_main())
    }

    #[cfg(test)]
    pub(crate) fn register_worker_guard_for_test(&self, key: ThreadKey) -> Box<dyn std::any::Any> {
        Box::new(self.register_worker(key))
    }

    #[cfg(test)]
    pub(crate) fn open_era_stowed_for_test(
        &self,
        world: EraWorldHandle,
        port: GroupPort,
    ) -> Result<(), (EraWorldHandle, GroupPort, EraOpenError)> {
        {
            let mut state = self.state();
            if matches!(state.era, EraState::SoloActive) {
                state.era = EraState::SoloSurrendered;
            }
        }
        self.open_era(world, port)
    }

    /// Check the keeper's ring-waiter word (L6.7b b4.1, QB2): one Acquire
    /// load, no lock beyond the caller's `ControlState`, no syscall. On a
    /// Baton-only World (`EraState::SoloActive`), returns `false` without
    /// touching any atomic or ring word.
    #[inline]
    fn era_ring_waiter(&self, state: &ControlState) -> bool {
        match &state.era {
            EraState::EraOpen(era) => {
                #[cfg(test)]
                self.ring_waiter_loads
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                era.waiter.raised()
            }
            EraState::SoloActive | EraState::SoloSurrendered => false,
        }
    }
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn record_main_native_event_for_test(&self, event: MainNativeWaitEvent) {
        self.main_native_events.lock().unwrap().push(event);
    }
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn main_native_events_for_test(&self) -> Vec<MainNativeWaitEvent> {
        self.main_native_events.lock().unwrap().clone()
    }

    pub(crate) fn install_select(
        &self,
        select: Option<crate::emacs_core::eval::process_hooks::SelectHandleToken>,
    ) {
        assert!(
            self.select.set(select).is_ok(),
            "select handle installed twice"
        );
    }

    pub(crate) fn notify_select(&self) {
        self.metadata_changed.notify_all();
        if let Some(Some(select)) = self.select.get() {
            let select = *select;
            // `SchedulerReady` interrupts the idle wait so `pick_next` can
            // choose the owner `wake` just published. It is not input:
            // `wait_registered` reports it as a plain timeout so a peer's
            // `thread_select` is not ended (`src/thread.c:974-982`).
            crate::emacs_core::eval::process_hooks::select_nudge(
                select,
                crate::host_effect::wake::ControlDestination::IdleScheduler,
                crate::host_effect::wake::ControlNudge::SchedulerReady,
            );
            // A released native main owns a distinct registration. Its
            // durable scheduler state may become ready before or during the
            // OS wait; the idle-selector epoch alone cannot wake that role.
            // Drop the metadata guard before publishing to the paired source.
            let main_native =
                self.state().physical.get(&ThreadKey::Main) == Some(&OwnerLocation::NativeReleased);
            if main_native {
                crate::emacs_core::eval::process_hooks::select_nudge(
                    select,
                    crate::host_effect::wake::ControlDestination::NativeMain,
                    crate::host_effect::wake::ControlNudge::SchedulerReady,
                );
            }
        }
    }

    pub(crate) fn nudge_main_mailbox(&self) {
        self.nudge_main(crate::host_effect::wake::ControlNudge::MainMailboxReady);
    }

    pub(crate) fn nudge_main(&self, reason: crate::host_effect::wake::ControlNudge) {
        self.metadata_changed.notify_all();
        if let Some(Some(select)) = self.select.get() {
            crate::emacs_core::eval::process_hooks::select_nudge(
                *select,
                crate::host_effect::wake::ControlDestination::NativeMain,
                reason,
            );
        }
    }

    #[doc(hidden)]
    pub fn state(&self) -> MutexGuard<'_, ControlState> {
        // Every update is a few map/queue operations that leave the state
        // consistent; a panic while holding the lock cannot tear it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn reserve_service(
        &self,
        registration: &HostParkRegistration,
    ) -> Result<ReservedServiceSlice, ServiceRefusal> {
        let mut state = self.state();
        if state.closing {
            return Err(ServiceRefusal::Closing);
        }
        if registration.owner() == ThreadKey::Main {
            return Err(ServiceRefusal::MainIsRequester);
        }
        if !state.senders.contains_key(&ThreadKey::Main)
            || matches!(state.waits.get(&ThreadKey::Main), Some(Wait::Exited) | None)
        {
            return Err(ServiceRefusal::MainUnavailable);
        }
        if matches!(
            state.waits.get(&registration.owner()),
            Some(Wait::Exited) | None
        ) {
            return Err(ServiceRefusal::RequesterUnavailable);
        }
        if state.services.contains_key(&registration.key()) {
            return Err(ServiceRefusal::DuplicateRequest);
        }
        state.services.insert(
            registration.key(),
            ServiceMetadata {
                requester: registration.owner(),
                phase: ServicePhase::Reserved,
            },
        );
        Ok(ReservedServiceSlice {
            key: registration.key(),
            requester: registration.owner(),
            _confined: PhantomData,
        })
    }

    /// Sealed local-owner admission. Routed admission still refuses main.
    pub(crate) fn reserve_local_service(
        &self,
        registration: &HostParkRegistration,
    ) -> Result<ReservedServiceSlice, ServiceRefusal> {
        let mut state = self.state();
        if state.closing {
            return Err(ServiceRefusal::Closing);
        }
        if registration.owner() != ThreadKey::Main
            || state.main_os_thread != Some(std::thread::current().id())
        {
            return Err(ServiceRefusal::WrongMainThread);
        }
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Running) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        if state.services.contains_key(&registration.key()) {
            return Err(ServiceRefusal::DuplicateRequest);
        }
        state.services.insert(
            registration.key(),
            ServiceMetadata {
                requester: ThreadKey::Main,
                phase: ServicePhase::Reserved,
            },
        );
        Ok(ReservedServiceSlice {
            key: registration.key(),
            requester: ThreadKey::Main,
            _confined: PhantomData,
        })
    }

    pub(crate) fn transfer_local_metadata(
        state: &mut ControlState,
        key: HostRequestKey,
        requester: ThreadKey,
        main_os: std::thread::ThreadId,
    ) -> Result<(), ServiceRefusal> {
        if state.closing {
            return Err(ServiceRefusal::Closing);
        }
        if state.main_os_thread != Some(main_os) || requester != ThreadKey::Main {
            return Err(ServiceRefusal::WrongMainThread);
        }
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Running) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let Some(metadata) = state.services.get_mut(&key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != ThreadKey::Main || metadata.phase != ServicePhase::Reserved {
            return Err(ServiceRefusal::WrongServiceState);
        }
        metadata.phase = ServicePhase::Serving;
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Serving(key));
        Ok(())
    }

    /// Enter local native service after canonical owner/root publication.
    pub(crate) fn transfer_local_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let result = Self::transfer_local_metadata(
            &mut self.state(),
            slice.key,
            slice.requester,
            std::thread::current().id(),
        );
        match result {
            Ok(()) => Ok(Wake::ServiceTurn {
                key: slice.key,
                baton,
            }),
            Err(reason) => Err(ServiceTransferRefusal { reason, baton }),
        }
    }

    /// Consume the plan only after all registry/actor borrows have ended.
    /// Local service never sends to or receives from main's own channel.
    pub(crate) fn transfer_prepared_local_service(
        &self,
        plan: ServiceTransferPlan,
        baton: TurnRight,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let result = Self::transfer_local_metadata(
            &mut self.state(),
            plan.key,
            plan.requester.green_owner(),
            std::thread::current().id(),
        );
        match result {
            Ok(()) => Ok(Wake::ServiceTurn {
                key: plan.key,
                baton,
            }),
            Err(reason) => Err(ServiceTransferRefusal { reason, baton }),
        }
    }

    pub(crate) fn intermediate_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        Self::intermediate_metadata_for(state, slice.key, slice.requester)
    }

    pub(crate) fn intermediate_metadata_for(
        state: &mut ControlState,
        key: HostRequestKey,
        requester: ThreadKey,
    ) -> Result<(), ServiceRefusal> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Serving(key)) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        if requester != ThreadKey::Main
            && state.waits.get(&requester) != Some(&Wait::HostRequest(key))
        {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let Some(metadata) = state.services.get_mut(&key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let eligibility = match metadata.phase {
            ServicePhase::Serving => ContinuationEligibility::Reserved,
            ServicePhase::ServingAfterWait => ContinuationEligibility::Released,
            ServicePhase::Reserved
            | ServicePhase::PendingNative
            | ServicePhase::RealWait
            | ServicePhase::Reacquiring
            | ServicePhase::Continuing(_)
            | ServicePhase::Resuming(_)
            | ServicePhase::Ended(_) => {
                return Err(ServiceRefusal::WrongServiceState);
            }
        };
        metadata.phase = ServicePhase::Continuing(eligibility);
        if requester != ThreadKey::Main {
            Self::set_wait(state, requester, Wait::Runnable);
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Receiving);
        }
        state.physical.insert(requester, OwnerLocation::Running);
        Ok(())
    }

    /// The caller retains the linear slice beside its published owner step.
    /// Neither this hop nor its wake creates FIFO or signal eligibility.
    pub(crate) fn return_to_requester_intermediate(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<IntermediateReturn, ServiceTransferRefusal> {
        let plan = match self.prepare_intermediate_return(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.return_prepared_intermediate(plan, baton)
    }

    pub(crate) fn prepare_intermediate_return(
        &self,
        slice: &ReservedServiceSlice,
    ) -> Result<IntermediateReturnPlan, ServiceRefusal> {
        let state = self.state();
        let Some(metadata) = state.services.get(&slice.key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != slice.requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let eligibility = match metadata.phase {
            ServicePhase::Serving => ContinuationEligibility::Reserved,
            ServicePhase::ServingAfterWait => ContinuationEligibility::Released,
            ServicePhase::Reserved
            | ServicePhase::PendingNative
            | ServicePhase::RealWait
            | ServicePhase::Reacquiring
            | ServicePhase::Continuing(_)
            | ServicePhase::Resuming(_)
            | ServicePhase::Ended(_) => return Err(ServiceRefusal::WrongServiceState),
        };
        Ok(IntermediateReturnPlan {
            key: slice.key,
            requester: slice.requester,
            eligibility,
        })
    }

    pub(crate) fn return_prepared_intermediate(
        &self,
        plan: IntermediateReturnPlan,
        baton: TurnRight,
    ) -> Result<IntermediateReturn, ServiceTransferRefusal> {
        let mut state = self.state();
        let result = if state.main_os_thread != Some(std::thread::current().id()) {
            Err(ServiceRefusal::WrongMainThread)
        } else if !state.services.get(&plan.key).is_some_and(|metadata| {
            metadata.requester == plan.requester
                && metadata.phase
                    == match plan.eligibility {
                        ContinuationEligibility::Reserved => ServicePhase::Serving,
                        ContinuationEligibility::Released => ServicePhase::ServingAfterWait,
                    }
        }) {
            Err(ServiceRefusal::WrongServiceState)
        } else {
            Self::intermediate_metadata_for(&mut state, plan.key, plan.requester)
        };
        if let Err(reason) = result {
            return Err(ServiceTransferRefusal { reason, baton });
        }
        drop(state);
        if plan.requester == ThreadKey::Main {
            Ok(IntermediateReturn::LocalMain {
                key: plan.key,
                baton,
            })
        } else {
            self.send_wake(
                plan.requester,
                Wake::OwnerStep {
                    key: plan.key,
                    baton,
                },
            );
            Ok(IntermediateReturn::Sent)
        }
    }

    pub(crate) fn resume_owner_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        Self::resume_owner_metadata_for(state, slice.key, slice.requester)
    }

    pub(crate) fn resume_owner_metadata_for(
        state: &mut ControlState,
        key: HostRequestKey,
        requester: ThreadKey,
    ) -> Result<(), ServiceRefusal> {
        if state.physical.get(&requester) != Some(&OwnerLocation::Running)
            || state.waits.get(&requester) != Some(&Wait::Runnable)
        {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let Some(metadata) = state.services.get_mut(&key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let eligibility = match metadata.phase {
            ServicePhase::Continuing(eligibility) => eligibility,
            ServicePhase::Reserved
            | ServicePhase::Serving
            | ServicePhase::ServingAfterWait
            | ServicePhase::PendingNative
            | ServicePhase::RealWait
            | ServicePhase::Reacquiring
            | ServicePhase::Resuming(_)
            | ServicePhase::Ended(_) => return Err(ServiceRefusal::WrongServiceState),
        };
        metadata.phase = ServicePhase::Resuming(eligibility);
        if requester != ThreadKey::Main {
            Self::set_wait(state, requester, Wait::HostRequest(key));
            state.physical.insert(requester, OwnerLocation::Receiving);
        }
        Ok(())
    }

    /// Continue an existing parent even after close; no new actor publication.
    /// A native main first reacquires physically before accepting this Baton.
    pub(crate) fn resume_owner_step(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let plan = match self.prepare_owner_resume(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.resume_prepared_owner_step(plan, baton)
    }

    pub(crate) fn prepare_owner_resume(
        &self,
        slice: &ReservedServiceSlice,
    ) -> Result<OwnerResumePlan, ServiceRefusal> {
        let state = self.state();
        let Some(metadata) = state.services.get(&slice.key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != slice.requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let ServicePhase::Continuing(eligibility) = metadata.phase else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        Ok(OwnerResumePlan {
            key: slice.key,
            requester: slice.requester,
            eligibility,
        })
    }

    pub(crate) fn resume_prepared_owner_step(
        &self,
        plan: OwnerResumePlan,
        baton: TurnRight,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let mut state = self.state();
        if plan.requester == ThreadKey::Main {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::MainIsRequester,
                baton,
            });
        }
        if !state.services.get(&plan.key).is_some_and(|metadata| {
            metadata.requester == plan.requester
                && metadata.phase == ServicePhase::Continuing(plan.eligibility)
        }) {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::WrongServiceState,
                baton,
            });
        }
        if !matches!(
            state.physical.get(&ThreadKey::Main),
            Some(OwnerLocation::Receiving | OwnerLocation::NativeReleased)
        ) {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::MainUnavailable,
                baton,
            });
        }
        if let Err(reason) = Self::resume_owner_metadata_for(&mut state, plan.key, plan.requester) {
            return Err(ServiceTransferRefusal { reason, baton });
        }
        state.pending_services.push_back(plan.key);
        drop(state);
        self.nudge_main(crate::host_effect::wake::ControlNudge::HostServiceReady);
        let mut state = self.state();
        loop {
            if let Some(turn) = Self::pick_service(&mut state) {
                drop(state);
                self.send_wake(ThreadKey::Main, Self::native_wake(turn, baton));
                return Ok(ServiceTransfer::Dispatched);
            }
            state = self
                .metadata_changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub(crate) fn resume_local_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        Self::resume_local_metadata_for(state, slice.key, slice.requester)
    }

    pub(crate) fn resume_local_metadata_for(
        state: &mut ControlState,
        key: HostRequestKey,
        requester: ThreadKey,
    ) -> Result<(), ServiceRefusal> {
        if requester != ThreadKey::Main {
            return Err(ServiceRefusal::MainIsRequester);
        }
        Self::resume_owner_metadata_for(state, key, requester)?;
        let Some(metadata) = state.services.get_mut(&key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        let eligibility = match metadata.phase {
            ServicePhase::Resuming(eligibility) => eligibility,
            ServicePhase::Reserved
            | ServicePhase::Serving
            | ServicePhase::ServingAfterWait
            | ServicePhase::PendingNative
            | ServicePhase::RealWait
            | ServicePhase::Reacquiring
            | ServicePhase::Continuing(_)
            | ServicePhase::Ended(_) => return Err(ServiceRefusal::WrongServiceState),
        };
        metadata.phase = match eligibility {
            ContinuationEligibility::Reserved => ServicePhase::Serving,
            ContinuationEligibility::Released => ServicePhase::ServingAfterWait,
        };
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Serving(key));
        Ok(())
    }

    pub(crate) fn resume_local_owner_step(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let plan = match self.prepare_owner_resume(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.resume_prepared_local_owner_step(plan, baton)
    }

    pub(crate) fn resume_prepared_local_owner_step(
        &self,
        plan: OwnerResumePlan,
        baton: TurnRight,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let mut state = self.state();
        if plan.requester != ThreadKey::Main
            || state.main_os_thread != Some(std::thread::current().id())
        {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::WrongMainThread,
                baton,
            });
        }
        if !state.services.get(&plan.key).is_some_and(|metadata| {
            metadata.requester == plan.requester
                && metadata.phase == ServicePhase::Continuing(plan.eligibility)
        }) {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::WrongServiceState,
                baton,
            });
        }
        if let Err(reason) = Self::resume_local_metadata_for(&mut state, plan.key, plan.requester) {
            return Err(ServiceTransferRefusal { reason, baton });
        }
        Ok(Wake::ContinuationTurn {
            key: plan.key,
            baton,
        })
    }

    /// Only a semantically final local result clears the parent's reservation.
    pub(crate) fn return_from_local_service(
        &self,
        slice: ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<ServiceReturn, ServiceReturnRefusal> {
        let mut state = self.state();
        if slice.requester != ThreadKey::Main
            || state.main_os_thread != Some(std::thread::current().id())
            || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Serving(slice.key))
            || !state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == ThreadKey::Main
                    && matches!(
                        metadata.phase,
                        ServicePhase::Serving | ServicePhase::ServingAfterWait
                    )
            })
        {
            return Err(ServiceReturnRefusal {
                reason: ServiceRefusal::WrongServiceState,
                slice,
                baton,
            });
        }
        state.services.remove(&slice.key);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
        Ok(ServiceReturn::ResumeMain(baton))
    }

    /// Native settlement does not acknowledge a dead owner. It only removes
    /// the exact ended continuation's scheduler reservation before quiescent drain.
    pub(crate) fn ended_service_owner(
        &self,
        slice: &ReservedServiceSlice,
        baton: &TurnRight,
    ) -> Result<EndedServiceOwner, ServiceRefusal> {
        let runtime = baton.green_runtime();
        if !baton.held_here()
            || !runtime
                .as_ref()
                .is_some_and(|runtime| std::ptr::eq(self, std::sync::Arc::as_ptr(&runtime.control)))
        {
            return Err(ServiceRefusal::WrongServiceState);
        }
        Self::validate_ended_continuation(&self.state(), slice)?;
        Ok(EndedServiceOwner {
            key: slice.key,
            requester: slice.requester,
            _confined: PhantomData,
        })
    }

    #[cfg(test)]
    pub(crate) fn ended_service_owner_for_test(
        key: HostRequestKey,
        requester: ThreadKey,
        baton: &TurnRight,
    ) -> EndedServiceOwner {
        let runtime = baton.green_runtime();
        let control = &runtime.as_ref().unwrap().control;
        {
            let mut state = control.state();
            Self::set_wait(&mut state, requester, Wait::Exited);
            state.services.insert(
                key,
                ServiceMetadata {
                    requester,
                    phase: ServicePhase::Continuing(ContinuationEligibility::Released),
                },
            );
        }
        let slice = ReservedServiceSlice {
            key,
            requester,
            _confined: PhantomData,
        };
        control.ended_service_owner(&slice, baton).unwrap()
    }

    pub(crate) fn finish_ended_continuation(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
        baton: &TurnRight,
    ) -> Result<(), ServiceCancelRefusal> {
        let mut state = self.state();
        let result = if !baton.held_here()
            || settled.key() != slice.key
            || settled.owner() != slice.requester
        {
            Err(ServiceRefusal::WrongServiceState)
        } else {
            Self::finish_ended_continuation_metadata(&mut state, &slice)
        };
        if let Err(reason) = result {
            return Err(ServiceCancelRefusal {
                reason,
                slice,
                settled,
            });
        }
        drop(state);
        self.notify_select();
        Ok(())
    }

    pub(crate) fn finish_ended_continuation_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        Self::validate_ended_continuation(state, slice)?;
        state.services.remove(&slice.key);
        // Queue entries are metadata, not roots; removing this exact key cannot
        // consume a different parent's continuation or ordinary ready ticket.
        state.pending_services.retain(|key| *key != slice.key);
        Ok(())
    }

    pub(crate) fn validate_ended_continuation(
        state: &ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        if !state.services.get(&slice.key).is_some_and(|metadata| {
            metadata.requester == slice.requester
                && Self::continuation_progress(&state.waits, metadata)
                    == ContinuationProgress::OwnerEnded
        }) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        Ok(())
    }

    /// GNU-real waits release every active parent in this owner's nested
    /// continuation ancestry. Artificial service/owner hops never call this.
    pub(crate) fn note_continuation_release(state: &mut ControlState, requester: ThreadKey) {
        for metadata in state.services.values_mut() {
            if metadata.requester == requester {
                if let ServicePhase::Continuing(eligibility) = &mut metadata.phase {
                    *eligibility = ContinuationEligibility::Released;
                }
            }
        }
    }

    /// Only the rooted registration, never a generic wake, starts a service.
    /// Main's original wait and its ordinary FIFO position remain untouched.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(crate) fn transfer_to_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let plan = match self.prepare_service_transfer(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.transfer_prepared_service(plan, baton)
    }

    pub(crate) fn prepare_service_transfer(
        &self,
        slice: &ReservedServiceSlice,
    ) -> Result<ServiceTransferPlan, ServiceRefusal> {
        let state = self.state();
        if state.closing {
            return Err(ServiceRefusal::Closing);
        }
        if !state.services.get(&slice.key).is_some_and(|metadata| {
            metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
        }) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        Ok(ServiceTransferPlan {
            key: slice.key,
            requester: HostRequester::green(slice.requester),
            _confined: PhantomData,
        })
    }

    pub(crate) fn transfer_prepared_service(
        &self,
        slice: ServiceTransferPlan,
        baton: TurnRight,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let mut state = self.state();
        let valid = state.services.get(&slice.key).is_some_and(|metadata| {
            metadata.requester == slice.requester.green_owner()
                && metadata.phase == ServicePhase::Reserved
        });
        if !valid || state.closing {
            return Err(ServiceTransferRefusal {
                reason: if state.closing {
                    ServiceRefusal::Closing
                } else {
                    ServiceRefusal::WrongServiceState
                },
                baton,
            });
        }
        let main_location = state.physical.get(&ThreadKey::Main).copied();
        if !matches!(
            main_location,
            Some(OwnerLocation::Receiving | OwnerLocation::NativeReleased)
        ) {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::MainUnavailable,
                baton,
            });
        }
        Self::set_wait(
            &mut state,
            slice.requester.green_owner(),
            Wait::HostRequest(slice.key),
        );
        state.ready.discard(slice.requester.green_owner());
        Self::check_invariants(&state);
        state
            .physical
            .insert(slice.requester.green_owner(), OwnerLocation::Receiving);
        match main_location {
            Some(OwnerLocation::Receiving) => {
                if !state.pending_services.contains(&slice.key) {
                    state.pending_services.push_back(slice.key);
                }
                let Some(turn) = Self::pick_service(&mut state) else {
                    return Err(ServiceTransferRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        baton,
                    });
                };
                drop(state);
                self.send_wake(ThreadKey::Main, Self::native_wake(turn, baton));
                Ok(ServiceTransfer::Dispatched)
            }
            Some(OwnerLocation::NativeReleased) => {
                state.pending_services.push_back(slice.key);
                drop(state);
                self.nudge_main(crate::host_effect::wake::ControlNudge::HostServiceReady);
                Ok(ServiceTransfer::Queued(baton))
            }
            Some(
                OwnerLocation::Running | OwnerLocation::NativeDriving | OwnerLocation::Serving(_),
            )
            | None => {
                // The sole-Baton protocol makes a simultaneously running main
                // impossible. Preserve the Baton and reservation for diagnosis.
                Err(ServiceTransferRefusal {
                    reason: ServiceRefusal::MainUnavailable,
                    baton,
                })
            }
        }
    }

    /// Called on main's acquire path before its Lisp continuation is restored.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(crate) fn take_queued_service(
        &self,
        baton: TurnRight,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id()) {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::WrongMainThread,
                baton,
            });
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        if let Some(turn) = Self::pick_service(&mut state) {
            return Ok(Self::native_wake(turn, baton));
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
        Ok(Wake::Resume(baton))
    }

    /// The service result was committed to the registry first. An immediate
    /// service returns its reserved slice; a real release completes through
    /// ordinary FIFO eligibility. Main's original Wait stays intact.
    pub(crate) fn return_from_service(
        &self,
        slice: ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<ServiceReturn, ServiceReturnRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if slice.requester == ThreadKey::Main {
            Some(ServiceRefusal::MainIsRequester)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Serving(slice.key))
            || state.waits.get(&slice.requester) != Some(&Wait::HostRequest(slice.key))
            || !state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester
                    && matches!(
                        metadata.phase,
                        ServicePhase::Serving | ServicePhase::ServingAfterWait
                    )
            })
        {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceReturnRefusal {
                reason,
                slice,
                baton,
            });
        }
        let completion_phase = state
            .services
            .remove(&slice.key)
            .map(|metadata| metadata.phase);
        Self::set_wait(&mut state, slice.requester, Wait::Runnable);
        // An acquire may have selected main's ordinary turn before noticing
        // the queued service. Preserve that eligibility, but the reserved
        // requester still receives the Baton first.
        if state.waits.get(&ThreadKey::Main) == Some(&Wait::Runnable) {
            Self::enqueue_ready_front(&mut state, ThreadKey::Main);
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        match completion_phase {
            Some(ServicePhase::Serving) => {
                state
                    .physical
                    .insert(slice.requester, OwnerLocation::Running);
                drop(state);
                self.send_wake(slice.requester, Wake::Resume(baton));
                Ok(ServiceReturn::Sent)
            }
            Some(ServicePhase::ServingAfterWait) => {
                Self::enqueue_ready(&mut state, slice.requester);
                drop(state);
                let next = match self.pick_next() {
                    PickNext::Owner(next) => next,
                    PickNext::NoRunnable(_) => {
                        return Err(ServiceReturnRefusal {
                            reason: ServiceRefusal::WrongServiceState,
                            slice,
                            baton,
                        });
                    }
                };
                let (target, wake) = Self::wake_for(next, baton);
                match wake {
                    Wake::Resume(baton) if target == ThreadKey::Main => {
                        self.state()
                            .physical
                            .insert(ThreadKey::Main, OwnerLocation::Running);
                        Ok(ServiceReturn::ResumeMain(baton))
                    }
                    Wake::Resume(baton) => {
                        self.send_wake(target, Wake::Resume(baton));
                        Ok(ServiceReturn::Sent)
                    }
                    Wake::ServiceTurn { key, baton } => {
                        self.send_wake(target, Wake::ServiceTurn { key, baton });
                        Ok(ServiceReturn::Sent)
                    }
                    Wake::NativePump { baton } => {
                        self.send_wake(target, Wake::NativePump { baton });
                        Ok(ServiceReturn::Sent)
                    }
                    Wake::ContinuationTurn { key, baton } => {
                        self.send_wake(target, Wake::ContinuationTurn { key, baton });
                        Ok(ServiceReturn::Sent)
                    }
                    Wake::OwnerStep { .. } => std::process::abort(),
                }
            }
            Some(
                ServicePhase::Reserved
                | ServicePhase::PendingNative
                | ServicePhase::RealWait
                | ServicePhase::Reacquiring
                | ServicePhase::Continuing(_)
                | ServicePhase::Resuming(_)
                | ServicePhase::Ended(_),
            )
            | None => std::process::abort(),
        }
    }

    /// The owned native wrapper has reached a genuine GNU release point.
    /// The requester remains parked; only this permit admits ordinary peers.
    pub(crate) fn release_service(
        &self,
        slice: &ReservedServiceSlice,
        _permit: RealWaitPermit,
        baton: TurnRight,
    ) -> Result<NativeRelease, ServiceTransferRefusal> {
        let mut state = self.state();
        let valid = state.main_os_thread == Some(std::thread::current().id())
            && state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Serving(slice.key))
            && state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester
                    && matches!(
                        metadata.phase,
                        ServicePhase::Serving | ServicePhase::ServingAfterWait
                    )
            });
        if !valid {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::WrongServiceState,
                baton,
            });
        }
        Self::note_continuation_release(&mut state, slice.requester);
        if let Some(metadata) = state.services.get_mut(&slice.key) {
            metadata.phase = ServicePhase::RealWait;
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        let next = match Self::pick(&mut state) {
            Some((key, None)) => Some(key),
            Some((_, Some(_))) | None => None,
        };
        let end_era_to_idle = next.is_none() && baton.is_era();
        if end_era_to_idle {
            let EraState::EraOpen(era) = &mut state.era else {
                std::process::abort();
            };
            era.turn = GroupTurn::Idle;
        }
        drop(state);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                Ok(NativeRelease::Transferred)
            }
            None if end_era_to_idle => {
                Self::give_back_era_turn(baton);
                Ok(NativeRelease::EraIdle)
            }
            None => Ok(NativeRelease::Idle(baton)),
        }
    }

    /// Called after the native wrapper returns. Main reacquires for native
    /// completion only; its suspended Lisp Wait is neither consumed nor read.
    pub(crate) fn reacquire_service(
        &self,
        slice: &ReservedServiceSlice,
        release: NativeRelease,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<TurnRight, ServiceAcquireRefusal> {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id())
            || !state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester && metadata.phase == ServicePhase::RealWait
            })
        {
            return Err(ServiceAcquireRefusal {
                reason: ServiceRefusal::WrongServiceState,
                release,
            });
        }
        match release {
            NativeRelease::Idle(baton) => {
                if let Some(metadata) = state.services.get_mut(&slice.key) {
                    metadata.phase = ServicePhase::ServingAfterWait;
                }
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Serving(slice.key));
                drop(state);
                baton.enter();
                Ok(baton)
            }
            NativeRelease::EraIdle => {
                if let Some(metadata) = state.services.get_mut(&slice.key) {
                    metadata.phase = ServicePhase::Reacquiring;
                }
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                state
                    .pending_acquires
                    .push_back(NativeAcquire::Service(slice.key));
                Self::coalesce_request_locked(&mut state);
                let port = Self::take_era_port_if_requested(&mut state);
                drop(state);
                self.metadata_changed.notify_all();
                let wake = self.reacquire_era_wake(ThreadKey::Main, port, receiver);
                match wake {
                    Wake::ServiceTurn { key, baton } if key == slice.key => {
                        baton.enter();
                        Ok(baton)
                    }
                    Wake::Resume(baton)
                    | Wake::ServiceTurn { baton, .. }
                    | Wake::OwnerStep { baton, .. }
                    | Wake::ContinuationTurn { baton, .. }
                    | Wake::NativePump { baton } => Err(ServiceAcquireRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        release: NativeRelease::Idle(baton),
                    }),
                }
            }
            NativeRelease::Transferred => {
                if let Some(metadata) = state.services.get_mut(&slice.key) {
                    metadata.phase = ServicePhase::Reacquiring;
                }
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                state
                    .pending_acquires
                    .push_back(NativeAcquire::Service(slice.key));
                Self::coalesce_request_locked(&mut state);
                let port = Self::take_era_port_if_requested(&mut state);
                drop(state);
                self.metadata_changed.notify_all();
                let wake = if port.is_some() {
                    self.reacquire_era_wake(ThreadKey::Main, port, receiver)
                } else {
                    self.notify_select();
                    Self::recv_wake_and_enter(receiver)
                };
                match wake {
                    Wake::ServiceTurn { key, baton } if key == slice.key => {
                        baton.enter();
                        Ok(baton)
                    }
                    Wake::Resume(baton)
                    | Wake::ServiceTurn { baton, .. }
                    | Wake::OwnerStep { baton, .. }
                    | Wake::ContinuationTurn { baton, .. }
                    | Wake::NativePump { baton } => Err(ServiceAcquireRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        release: NativeRelease::Idle(baton),
                    }),
                }
            }
        }
    }

    /// Preserve this request's linear reservation while the owned driver
    /// serves another request. Deferral alone admits no ordinary Lisp owner.
    pub(crate) fn defer_native_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<TurnRight, ServiceTransferRefusal> {
        let mut state = self.state();
        let result = if state.main_os_thread != Some(std::thread::current().id()) {
            Err(ServiceRefusal::WrongMainThread)
        } else {
            Self::defer_native_metadata(&mut state, slice)
        };
        match result {
            Ok(()) => Ok(baton),
            Err(reason) => Err(ServiceTransferRefusal { reason, baton }),
        }
    }

    pub(crate) fn defer_native_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Serving(slice.key)) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let Some(metadata) = state.services.get_mut(&slice.key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != slice.requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        metadata.phase = match metadata.phase {
            ServicePhase::Serving => ServicePhase::PendingNative,
            ServicePhase::ServingAfterWait => ServicePhase::RealWait,
            ServicePhase::Reserved
            | ServicePhase::PendingNative
            | ServicePhase::RealWait
            | ServicePhase::Reacquiring
            | ServicePhase::Continuing(_)
            | ServicePhase::Resuming(_)
            | ServicePhase::Ended(_) => return Err(ServiceRefusal::WrongServiceState),
        };
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        Ok(())
    }

    /// Called only after the owned native record has an actual completion.
    /// The record retains the request's dispatch and cancellation obligations.
    pub(crate) fn enter_native_completion(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<TurnRight, ServiceTransferRefusal> {
        let mut state = self.state();
        let result = if state.main_os_thread != Some(std::thread::current().id()) {
            Err(ServiceRefusal::WrongMainThread)
        } else {
            Self::enter_native_completion_metadata(&mut state, slice)
        };
        match result {
            Ok(()) => Ok(baton),
            Err(reason) => Err(ServiceTransferRefusal { reason, baton }),
        }
    }

    pub(crate) fn enter_native_completion_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeDriving) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let Some(metadata) = state.services.get_mut(&slice.key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != slice.requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        metadata.phase = match metadata.phase {
            ServicePhase::PendingNative => ServicePhase::Serving,
            ServicePhase::RealWait => ServicePhase::ServingAfterWait,
            ServicePhase::Reserved
            | ServicePhase::Serving
            | ServicePhase::Reacquiring
            | ServicePhase::ServingAfterWait
            | ServicePhase::Continuing(_)
            | ServicePhase::Resuming(_)
            | ServicePhase::Ended(_) => return Err(ServiceRefusal::WrongServiceState),
        };
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Serving(slice.key));
        Ok(())
    }

    /// The sole physical executor lends ordinary eligibility to every newly
    /// pending call together. The driver owns one release, never one per key.
    pub(crate) fn release_native_pump(
        &self,
        _permit: RealWaitPermit,
        baton: TurnRight,
    ) -> Result<NativeRelease, ServiceTransferRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeDriving)
            || Self::pending_native(&state) == NativeWork::Idle
        {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceTransferRefusal { reason, baton });
        }
        let releasing: Vec<_> = state
            .services
            .values()
            .filter_map(|metadata| {
                (metadata.phase == ServicePhase::PendingNative).then_some(metadata.requester)
            })
            .collect();
        for requester in releasing {
            Self::note_continuation_release(&mut state, requester);
        }
        for metadata in state.services.values_mut() {
            if metadata.phase == ServicePhase::PendingNative {
                metadata.phase = ServicePhase::RealWait;
            }
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        let next = match Self::pick(&mut state) {
            Some((key, None)) => Some(key),
            Some((_, Some(_))) | None => None,
        };
        let end_era_to_idle = next.is_none() && baton.is_era();
        if end_era_to_idle {
            let EraState::EraOpen(era) = &mut state.era else {
                std::process::abort();
            };
            era.turn = GroupTurn::Idle;
        }
        drop(state);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                Ok(NativeRelease::Transferred)
            }
            None if end_era_to_idle => {
                Self::give_back_era_turn(baton);
                Ok(NativeRelease::EraIdle)
            }
            None => Ok(NativeRelease::Idle(baton)),
        }
    }

    /// Physical completion first reacquires the native driver, not whichever
    /// request happened to arm the poll. A service wake for another key is valid.
    pub(crate) fn reacquire_native_pump(
        &self,
        release: NativeRelease,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<Wake, ServiceAcquireRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeReleased) {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceAcquireRefusal { reason, release });
        }
        match release {
            NativeRelease::Idle(baton) => {
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
                drop(state);
                baton.enter();
                Ok(Wake::NativePump { baton })
            }
            NativeRelease::EraIdle => {
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                if !state.pending_acquires.contains(&NativeAcquire::Pump) {
                    state.pending_acquires.push_back(NativeAcquire::Pump);
                }
                Self::coalesce_request_locked(&mut state);
                let port = Self::take_era_port_if_requested(&mut state);
                drop(state);
                Ok(self.reacquire_era_wake(ThreadKey::Main, port, receiver))
            }
            NativeRelease::Transferred => {
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                if !state.pending_acquires.contains(&NativeAcquire::Pump) {
                    state.pending_acquires.push_back(NativeAcquire::Pump);
                }
                Self::coalesce_request_locked(&mut state);
                let port = Self::take_era_port_if_requested(&mut state);
                drop(state);
                if port.is_some() {
                    Ok(self.reacquire_era_wake(ThreadKey::Main, port, receiver))
                } else {
                    self.notify_select();
                    Ok(Self::recv_wake_and_enter(receiver))
                }
            }
        }
    }

    /// Service-only choices preserve main's original wait. A still-reserved
    /// PendingNative call cannot admit main's unrelated Lisp continuation.
    pub(crate) fn choose_native_pump_turn(
        &self,
        baton: TurnRight,
        purpose: NativePumpPurpose,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeDriving) {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceTransferRefusal { reason, baton });
        }
        match Self::choose_native_metadata(&mut state, purpose) {
            NativePumpChoice::Native(turn) => Ok(Self::native_wake(turn, baton)),
            NativePumpChoice::OriginalMain => Ok(Wake::Resume(baton)),
        }
    }

    pub(crate) fn choose_native_metadata(
        state: &mut ControlState,
        purpose: NativePumpPurpose,
    ) -> NativePumpChoice {
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        if let Some(turn) = Self::pick_queued_service(state) {
            return NativePumpChoice::Native(turn);
        }
        if state.services.values().any(|metadata| {
            Self::continuation_progress(&state.waits, metadata) == ContinuationProgress::OwnerEnded
        }) {
            // A different request's genuine release cannot admit main's Lisp
            // ahead of an ended requester's retained native cleanup.
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
            return NativePumpChoice::Native(NativeTurn::Pump);
        }
        let released = state
            .services
            .values()
            .any(|metadata| metadata.phase == ServicePhase::RealWait);
        let reserved = state
            .services
            .values()
            .any(|metadata| metadata.phase == ServicePhase::PendingNative);
        let local_native = state.services.values().any(|metadata| {
            metadata.requester == ThreadKey::Main
                && !matches!(metadata.phase, ServicePhase::Continuing(_))
        });
        let main_eligible = match purpose {
            NativePumpPurpose::Serving => released || state.services.is_empty(),
            NativePumpPurpose::Draining => state.services.is_empty(),
        };
        if main_eligible
            && !reserved
            && !local_native
            && state.waits.get(&ThreadKey::Main) == Some(&Wait::Runnable)
        {
            let _ = Self::pop_ready(state, ReadySelection::NativeMainAcquire);
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Running);
            return NativePumpChoice::OriginalMain;
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        NativePumpChoice::Native(NativeTurn::Pump)
    }

    /// Existing owned process source; no host object or World borrow escapes.
    pub(crate) fn native_pump_source(
        &self,
    ) -> Result<std::sync::Arc<crate::host_effect::wake::ControlWake>, ServiceRefusal> {
        self.select
            .get()
            .and_then(Option::as_ref)
            .and_then(|select| {
                crate::emacs_core::eval::process_hooks::select_control_source(*select)
            })
            .ok_or(ServiceRefusal::MainUnavailable)
    }

    /// Recheck after registration, before committing to an actual OS wait.
    pub(crate) fn native_service_queued(&self) -> NativeWork {
        if Self::queued_native_work(&self.state()) == NativeWork::Ready {
            NativeWork::Ready
        } else {
            NativeWork::Idle
        }
    }

    /// The requester keeps the Baton while main finishes an earlier native
    /// release. This metadata-only wait cannot run unrelated Lisp.
    pub(crate) fn wait_queued_prepared(
        &self,
        slice: ServiceTransferPlan,
        baton: TurnRight,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let mut state = self.state();
        loop {
            if state.closing {
                return Err(ServiceTransferRefusal {
                    reason: ServiceRefusal::Closing,
                    baton,
                });
            }
            if !state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester.green_owner()
                    && metadata.phase == ServicePhase::Reserved
            }) {
                return Err(ServiceTransferRefusal {
                    reason: ServiceRefusal::WrongServiceState,
                    baton,
                });
            }
            if let Some(turn) = Self::pick_service(&mut state) {
                drop(state);
                self.send_wake(ThreadKey::Main, Self::native_wake(turn, baton));
                return Ok(ServiceTransfer::Dispatched);
            }
            state = self
                .metadata_changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(crate) fn wait_queued_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: TurnRight,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let plan = match self.prepare_service_transfer(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.wait_queued_prepared(plan, baton)
    }

    /// A main native wait returning without the Baton announces that its
    /// receive loop can now accept service turns, preserving the original Wait.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(crate) fn main_native_acquire(&self, receiver: &mpsc::Receiver<Wake>) -> Wake {
        {
            let mut state = self.state();
            if state.main_os_thread != Some(std::thread::current().id())
                || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeReleased)
            {
                std::process::abort()
            }
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Receiving);
        }
        self.metadata_changed.notify_all();
        self.notify_select();
        match receiver.recv() {
            Ok(wake) => {
                wake.baton().enter();
                wake
            }
            Err(_) => std::process::abort(),
        }
    }

    /// The existing native blocking wrapper announces its physical release;
    /// this does not alter the owner's semantic Wait or make a peer runnable.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(crate) fn main_native_released(&self) -> Result<(), ServiceRefusal> {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id()) {
            return Err(ServiceRefusal::WrongMainThread);
        }
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Running) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Ok(())
    }

    /// Main has published its canonical parked roots and reached an actual
    /// native wait. Its Select remains distinct from its physical location:
    /// peers may run, but cannot resume main before the native observer returns.
    pub(crate) fn release_main_wait(
        &self,
        _permit: RealWaitPermit,
        baton: TurnRight,
        wait_end: Instant,
    ) -> NativeRelease {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id())
            || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Running)
        {
            std::process::abort();
        }
        #[cfg(any(test, feature = "test-support"))]
        self.main_native_events
            .lock()
            .unwrap()
            .push(MainNativeWaitEvent::Released);
        Self::note_continuation_release(&mut state, ThreadKey::Main);
        Self::set_wait(&mut state, ThreadKey::Main, Wait::Select { wait_end });
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        let next = match Self::pick(&mut state) {
            Some((key, None)) => Some(key),
            Some((_, Some(_))) | None => None,
        };
        let end_era_to_idle = next.is_none() && baton.is_era();
        if end_era_to_idle {
            let EraState::EraOpen(era) = &mut state.era else {
                std::process::abort();
            };
            era.turn = GroupTurn::Idle;
        }
        drop(state);
        #[cfg(any(test, feature = "test-support"))]
        self.observe_select_park_for_test(ThreadKey::Main, wait_end);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                NativeRelease::Transferred
            }
            None if end_era_to_idle => {
                Self::give_back_era_turn(baton);
                NativeRelease::EraIdle
            }
            None => NativeRelease::Idle(baton),
        }
    }

    /// The physical wait has returned. Publish main's readiness before waking
    /// an idle scheduler; owner restoration and all Lisp remain in switch.rs.
    pub(crate) fn reacquire_main_wait(
        &self,
        release: NativeRelease,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Wake {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id())
            || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeReleased)
        {
            std::process::abort();
        }
        #[cfg(any(test, feature = "test-support"))]
        self.main_native_events
            .lock()
            .unwrap()
            .push(MainNativeWaitEvent::Reacquired);
        match release {
            NativeRelease::Idle(baton) => {
                Self::finish_idle_main_acquire(&mut state);
                drop(state);
                baton.enter();
                Wake::Resume(baton)
            }
            NativeRelease::EraIdle => {
                Self::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                Self::enqueue_ready(&mut state, ThreadKey::Main);
                Self::coalesce_request_locked(&mut state);
                let port = Self::take_era_port_if_requested(&mut state);
                drop(state);
                self.reacquire_era_wake(ThreadKey::Main, port, receiver)
            }
            NativeRelease::Transferred => {
                Self::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                Self::enqueue_ready(&mut state, ThreadKey::Main);
                Self::coalesce_request_locked(&mut state);
                let port = Self::take_era_port_if_requested(&mut state);
                drop(state);
                if port.is_some() {
                    self.reacquire_era_wake(ThreadKey::Main, port, receiver)
                } else {
                    self.notify_select();
                    Self::recv_wake_and_enter(receiver)
                }
            }
        }
    }

    pub(crate) fn finish_idle_main_acquire(state: &mut ControlState) {
        Self::set_wait(state, ThreadKey::Main, Wait::Runnable);
        // Readiness may have queued main while the physical wait excluded it.
        // Returning its idle Baton consumes that turn without selecting a peer.
        let _ = Self::pop_ready(state, ReadySelection::NativeMainAcquire);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
    }

    /// Cancellation before dispatch must first commit a durable terminal
    /// result in the World registry. In-flight native service keeps its
    /// reserved return path until main has finished with the owned request.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(crate) fn cancel_before_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
    ) -> Result<(), ServiceCancelRefusal> {
        self.cancel_service(slice, settled, CancelPlacement::OrdinaryCompletion)
    }

    /// A refused/queued transfer left the requester holding the Baton. Its
    /// existing restore path resumes directly, without another FIFO turn.
    pub(crate) fn cancel_current_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
        _baton: &TurnRight,
    ) -> Result<(), ServiceCancelRefusal> {
        self.cancel_service(slice, settled, CancelPlacement::CurrentBatonOwner)
    }

    pub(crate) fn cancel_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
        placement: CancelPlacement,
    ) -> Result<(), ServiceCancelRefusal> {
        let mut state = self.state();
        let matches = settled.key() == slice.key
            && settled.owner() == slice.requester
            && state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
            });
        if !matches {
            return Err(ServiceCancelRefusal {
                reason: ServiceRefusal::WrongServiceState,
                slice,
                settled,
            });
        }
        let was_parked = match state.waits.get(&slice.requester).copied() {
            Some(Wait::HostRequest(key)) if key == slice.key => true,
            Some(Wait::Runnable) => false,
            Some(
                Wait::HostRequest(_)
                | Wait::Join(_)
                | Wait::Mutex(_)
                | Wait::Condition(_)
                | Wait::Select { .. }
                | Wait::Exited,
            )
            | None => {
                return Err(ServiceCancelRefusal {
                    reason: ServiceRefusal::WrongServiceState,
                    slice,
                    settled,
                });
            }
        };
        Self::set_wait(&mut state, slice.requester, Wait::Runnable);
        match placement {
            CancelPlacement::CurrentBatonOwner => {
                state.ready.discard(slice.requester);
                state
                    .physical
                    .insert(slice.requester, OwnerLocation::Running);
            }
            CancelPlacement::OrdinaryCompletion if was_parked => {
                Self::enqueue_ready(&mut state, slice.requester);
            }
            CancelPlacement::OrdinaryCompletion => {}
        }
        Self::check_invariants(&state);
        state.services.remove(&slice.key);
        state.pending_services.retain(|key| *key != slice.key);
        drop(state);
        self.notify_select();
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn after_next_select_park_for_test(
        &self,
        owner: ThreadKey,
        expectation: SelectParkExpectation,
        callback: impl FnOnce(SelectParked) + Send + 'static,
    ) {
        let mut slot = self
            .after_next_select_park
            .lock()
            .expect("select park gate lock");
        assert!(slot.is_none(), "select park gate already installed");
        *slot = Some(SelectParkGate::Armed {
            owner,
            expectation,
            callback: Box::new(callback),
        });
    }

    /// The live owner prepares only native facts immediately before its
    /// parked-root/Baton transfer. No process operation occurs during that
    /// transfer; the token itself is minted only after Select is committed.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn prepare_select_park_for_test(
        &self,
        owner: ThreadKey,
        processes: &crate::emacs_core::eval::data::process_state::ProcessManager,
    ) -> Result<(), SelectParkObservationError> {
        let mut slot = self
            .after_next_select_park
            .lock()
            .map_err(|_| SelectParkObservationError::GatePoisoned)?;
        let Some(gate) = slot.take() else {
            return Ok(());
        };
        match gate {
            SelectParkGate::Armed {
                owner: expected,
                expectation,
                callback,
            } => {
                if owner != expected {
                    *slot = Some(SelectParkGate::Armed {
                        owner: expected,
                        expectation,
                        callback,
                    });
                    return Ok(());
                }
                let facts = match expectation {
                    SelectParkExpectation::Ordinary => Ok(SelectParkFacts::Ordinary),
                    SelectParkExpectation::MainConnectPending(id) => match owner {
                        ThreadKey::Main => processes
                            .main_connect_pending_for_test(id)
                            .map(SelectParkFacts::MainConnectPending)
                            .map_err(SelectParkObservationError::PendingConnect),
                        ThreadKey::Worker(_) => Err(SelectParkObservationError::NotMain),
                    },
                };
                match facts {
                    Ok(facts) => {
                        *slot = Some(SelectParkGate::Prepared {
                            owner,
                            facts,
                            callback,
                        })
                    }
                    Err(error) => {
                        *slot = Some(SelectParkGate::Armed {
                            owner: expected,
                            expectation,
                            callback,
                        });
                        return Err(error);
                    }
                }
            }
            SelectParkGate::Prepared {
                owner: expected,
                facts,
                callback,
            } => {
                *slot = Some(SelectParkGate::Prepared {
                    owner: expected,
                    facts,
                    callback,
                });
                if owner == expected {
                    return Err(SelectParkObservationError::AlreadyPrepared);
                }
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn observe_select_park_for_test(&self, owner: ThreadKey, wait_end: Instant) {
        let callback = {
            let mut slot = self
                .after_next_select_park
                .lock()
                .expect("select park gate lock");
            match slot.take() {
                Some(SelectParkGate::Prepared {
                    owner: expected,
                    facts,
                    callback,
                }) => {
                    if owner == expected {
                        Some((
                            callback,
                            SelectParked {
                                owner,
                                wait_end,
                                facts,
                            },
                        ))
                    } else {
                        *slot = Some(SelectParkGate::Prepared {
                            owner: expected,
                            facts,
                            callback,
                        });
                        None
                    }
                }
                Some(SelectParkGate::Armed {
                    owner,
                    expectation,
                    callback,
                }) => {
                    *slot = Some(SelectParkGate::Armed {
                        owner,
                        expectation,
                        callback,
                    });
                    None
                }
                None => None,
            }
        };
        // Never hold Control.state or the gate mutex while waiting for the
        // native writer's consumed acknowledgement. The Baton stays local.
        if let Some((callback, witness)) = callback {
            callback(witness);
        }
    }

    /// Register the main thread, which holds the Baton: its channel.
    pub(crate) fn register_main(&self) -> mpsc::Receiver<Wake> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut state = self.state();
        state.senders.insert(ThreadKey::Main, sender);
        Self::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
        state.main_os_thread = Some(std::thread::current().id());
        receiver
    }

    /// Register the new thread KEY, ready to run in its turn: its channel.
    pub(crate) fn register_worker(&self, key: ThreadKey) -> mpsc::Receiver<Wake> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut state = self.state();
        state.senders.insert(key, sender);
        Self::set_wait(&mut state, key, Wait::Runnable);
        state.physical.insert(key, OwnerLocation::Receiving);
        Self::enqueue_ready(&mut state, key);
        receiver
    }

    /// Undo `register_worker` for a worker whose carrier OS thread never
    /// started (`make-thread` spawn refusal). The worker never held the
    /// Baton and its Lisp handle was never returned, so no joiner, mutex,
    /// condition, host request, or service can name it.
    ///
    /// The ready turn is invalidated through the seams: `set_wait` publishes
    /// the typed exit (releasing the wait's index entries and making its
    /// queued turn stale) and `remove_key` releases the whole record, so the
    /// stale turn is never selected. A stale Runnable turn would be picked
    /// and block the scheduler on a carrier that does not exist.
    pub(crate) fn unregister_worker(&self, key: ThreadKey) {
        let mut state = self.state();
        Self::set_wait(&mut state, key, Wait::Exited);
        // Whole-key removal goes through the seam; this path no longer writes
        // waits/senders/physical directly beside `set_wait`.
        Self::remove_key(&mut state, key);
    }

    /// Test-only: whether any scheduler record still names KEY.
    #[cfg(test)]
    pub(crate) fn worker_registered_for_test(&self, key: ThreadKey) -> bool {
        let state = self.state();
        state.senders.contains_key(&key)
            || state.waits.contains_key(&key)
            || state.physical.contains_key(&key)
    }

    /// Test-only: how many threads the registry currently charges (wait
    /// entries, wake senders).
    #[cfg(test)]
    pub(crate) fn registry_sizes_for_test(&self) -> (usize, usize) {
        let state = self.state();
        (state.waits.len(), state.senders.len())
    }

    /// Whether KEY has ended.
    pub(crate) fn exited(&self, key: ThreadKey) -> bool {
        matches!(self.state().waits.get(&key), Some(Wait::Exited) | None)
    }

    /// `Wait::Runnable` entries other than ME's own contribution.
    pub(crate) fn runnable_excluding(state: &ControlState, me: ThreadKey) -> usize {
        state.waits.runnable_count() - usize::from(state.waits.get(&me) == Some(&Wait::Runnable))
    }

    /// Select waits other than ME's own contribution.
    pub(crate) fn selects_excluding(state: &ControlState, me: ThreadKey) -> usize {
        state.waits.select_len()
            - usize::from(matches!(state.waits.get(&me), Some(Wait::Select { .. })))
    }

    /// The earliest Select deadline other than ME's, in `(wait_end, key)`
    /// order. One index entry per key, so peeking at most two entries is the
    /// old skip-`me` scan.
    pub(crate) fn earliest_other_select(
        state: &ControlState,
        me: ThreadKey,
    ) -> Option<DeadlineKey> {
        let mut index = state.waits.select_index.keys();
        let first = index.next()?;
        if first.key == me {
            index.next().copied()
        } else {
            Some(*first)
        }
    }

    /// Queued ready turns other than ME's own contribution.
    pub(crate) fn ready_valid_excluding(state: &ControlState, me: ThreadKey) -> usize {
        state.ready.valid_excluding(&me)
    }

    /// Whether a thread other than ME is ready to run, or will be at a
    /// deadline.
    pub(crate) fn others_could_run(&self, me: ThreadKey) -> bool {
        let state = self.state();
        Self::service_ready(&state)
            || Self::runnable_excluding(&state, me) > 0
            || Self::selects_excluding(&state, me) > 0
    }

    /// What the threads other than ME are doing at NOW.
    pub(crate) fn peers(&self, me: ThreadKey, now: Instant) -> Peers {
        let state = self.state();
        if Self::service_ready(&state) {
            return Peers::Ready;
        }
        if Self::runnable_excluding(&state, me) > 0 {
            return Peers::Ready;
        }
        match Self::earliest_other_select(&state, me) {
            Some(deadline) if deadline.wait_end <= now => Peers::Ready,
            Some(deadline) => Peers::Sleeping(deadline.wait_end),
            None => Peers::Blocked,
        }
    }

    /// Poll the selectors at a yield safepoint without taking an owner
    /// off the CPU. Preserve FIFO order: only readiness changes queue state.
    pub(crate) fn yield_peer_ready(&self, me: ThreadKey) -> bool {
        self.yield_peer_ready_at(me, Instant::now())
    }

    pub(crate) fn yield_peer_ready_at(&self, me: ThreadKey, now: Instant) -> bool {
        let mut state = self.state();
        if state.closing
            || Self::service_ready(&state)
            || Self::ready_valid_excluding(&state, me) > 0
        {
            // The ensuing transfer handles I/O/deadline readiness. Avoid
            // scanning sleeping owners twice on an actual handoff.
            return true;
        }
        Self::expire_deadlines(&mut state, now);
        if state.waits.select_len() > 0
            && let Some(Some(select)) = self.select.get()
            && crate::emacs_core::eval::process_hooks::select_wait_until(*select, now)
        {
            Self::wake_selects_at(&mut state, now);
        }
        Self::ready_valid_excluding(&state, me) > 0 || self.era_ring_waiter(&state)
    }

    /// Make KEY the next thread to run, whatever it waits for.
    pub(crate) fn prioritize(&self, key: ThreadKey) {
        let notify = {
            let mut state = self.state();
            if matches!(
                state.waits.get(&key),
                Some(Wait::Exited | Wait::HostRequest(_)) | None
            ) {
                return;
            }
            Self::set_wait(&mut state, key, Wait::Runnable);
            state.ready.move_to_front(key);
            Self::check_invariants(&state);
            Self::coalesce_request_locked(&mut state)
        };
        if notify {
            self.notify_select();
        }
    }

    /// What KEY waits for.
    pub(crate) fn wait_of(&self, key: ThreadKey) -> Option<Wait> {
        self.state().waits.get(&key).copied()
    }

    /// Whether the runtime is shutting down.
    #[doc(hidden)]
    pub fn lifecycle(&self) -> Lifecycle {
        if self.state().closing {
            Lifecycle::Closing
        } else {
            Lifecycle::Running
        }
    }

    /// Make KEY runnable again if it waits for something.
    #[doc(hidden)]
    pub fn wake(&self, key: ThreadKey) {
        let mut state = self.state();
        Self::wake_locked(&mut state, key);
        drop(state);
        self.notify_select();
    }

    /// Make every thread waiting for WAIT runnable (a mutex release, a
    /// notification to all, a thread's end).
    pub(crate) fn wake_waiting_for(&self, wait: Wait) -> usize {
        let (woken, notify) = {
            let mut state = self.state();
            let mut notify = false;
            let woken = match wait {
                Wait::Join(target) => {
                    let mut woken = 0;
                    while let Some(key) = state
                        .waits
                        .join_waiters
                        .get(&target)
                        .and_then(|set| set.first().copied())
                    {
                        notify |= Self::wake_locked(&mut state, key);
                        woken += 1;
                    }
                    woken
                }
                Wait::Mutex(id) => {
                    let mut woken = 0;
                    while let Some(key) = state
                        .waits
                        .mutex_waiters
                        .get(&id)
                        .and_then(|set| set.first().copied())
                    {
                        notify |= Self::wake_locked(&mut state, key);
                        woken += 1;
                    }
                    woken
                }
                Wait::Condition(id) => {
                    let mut woken = 0;
                    while let Some(key) = state
                        .waits
                        .condition_waiters
                        .get(&id)
                        .and_then(|set| set.first().copied())
                    {
                        notify |= Self::wake_locked(&mut state, key);
                        woken += 1;
                    }
                    woken
                }
                _ => {
                    // A wait with no object index (host requests): bounded by
                    // in-flight requests, so it stays a linear scan (cut 2 note).
                    let keys: Vec<ThreadKey> = state
                        .waits
                        .iter()
                        .filter(|(_, w)| **w == wait)
                        .map(|(key, _)| *key)
                        .collect();
                    for key in &keys {
                        notify |= Self::wake_locked(&mut state, *key);
                    }
                    keys.len()
                }
            };
            (woken, notify)
        };
        if notify {
            self.notify_select();
        }
        woken
    }

    /// Make the first thread (in key order) waiting for WAIT runnable.
    pub(crate) fn wake_one_waiting_for(&self, wait: Wait) -> bool {
        let (woken, notify) = {
            let mut state = self.state();
            let key = match wait {
                Wait::Join(target) => state
                    .waits
                    .join_waiters
                    .get(&target)
                    .and_then(|set| set.first().copied()),
                Wait::Mutex(id) => state
                    .waits
                    .mutex_waiters
                    .get(&id)
                    .and_then(|set| set.first().copied()),
                Wait::Condition(id) => state
                    .waits
                    .condition_waiters
                    .get(&id)
                    .and_then(|set| set.first().copied()),
                _ => state
                    .waits
                    .iter()
                    .find(|(_, w)| **w == wait)
                    .map(|(key, _)| *key),
            };
            match key {
                Some(key) => (true, Self::wake_locked(&mut state, key)),
                None => (false, false),
            }
        };
        if notify {
            self.notify_select();
        }
        woken
    }

    /// Returns `true` if waking `key` in an open era transitioned `GroupTurn`
    /// from `Idle -> Requested` (so the caller should notify waiters/select).
    pub(crate) fn wake_locked(state: &mut ControlState, key: ThreadKey) -> bool {
        match state.waits.get(&key) {
            Some(Wait::Runnable | Wait::Exited | Wait::HostRequest(_)) | None => false,
            Some(Wait::Join(_) | Wait::Mutex(_) | Wait::Condition(_) | Wait::Select { .. }) => {
                Self::set_wait(state, key, Wait::Runnable);
                Self::enqueue_ready(state, key);
                if key == ThreadKey::Main && Self::main_excluded(state) {
                    false
                } else {
                    Self::coalesce_request_locked(state)
                }
            }
        }
    }

    /// Move threads whose deadline passed into the ready queue, EARLIEST
    /// DEADLINE FIRST. The index is keyed by `(wait_end, ThreadKey)`, so the
    /// expired set comes out in exactly the order the independent per-thread
    /// timers would have produced; simultaneous deadlines keep `ThreadKey`
    /// order (the b262 pin).
    pub(crate) fn expire_deadlines(state: &mut ControlState, now: Instant) {
        while let Some(deadline) = state.waits.select_index.keys().next().copied() {
            if deadline.wait_end > now {
                break;
            }
            Self::wake_locked(state, deadline.key);
        }
    }

    /// Readiness wakes every selecting owner. Expired deadlines admit first
    /// in deadline order — the next-owner scan and the post-wait path run
    /// before `pick`'s own expiry — and the remaining selectors then wake in
    /// `ThreadKey` order, as the independent host waits would. They inspect
    /// their own wait policy and poll live descriptors only after
    /// reacquiring the Baton.
    pub(crate) fn wake_selects(state: &mut ControlState) {
        Self::wake_selects_at(state, Instant::now());
    }

    pub(crate) fn wake_selects_at(state: &mut ControlState, now: Instant) {
        Self::expire_deadlines(state, now);
        while let Some(key) = state.waits.select_keys.first().copied() {
            Self::wake_locked(state, key);
        }
    }

    pub(crate) fn pick_acquire(state: &mut ControlState) -> Option<NativeTurn> {
        while let Some(acquire) = state.pending_acquires.pop_front() {
            match acquire {
                NativeAcquire::Pump => {
                    if Self::pending_native(state) == NativeWork::Idle {
                        continue;
                    }
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
                    return Some(NativeTurn::Pump);
                }
                NativeAcquire::Service(key) => {
                    let Some(metadata) = state.services.get_mut(&key) else {
                        continue;
                    };
                    match metadata.phase {
                        ServicePhase::Reacquiring => {
                            metadata.phase = ServicePhase::ServingAfterWait;
                            state
                                .physical
                                .insert(ThreadKey::Main, OwnerLocation::Serving(key));
                            return Some(NativeTurn::Service(key));
                        }
                        ServicePhase::Reserved
                        | ServicePhase::Serving
                        | ServicePhase::PendingNative
                        | ServicePhase::RealWait
                        | ServicePhase::ServingAfterWait
                        | ServicePhase::Continuing(_)
                        | ServicePhase::Resuming(_)
                        | ServicePhase::Ended(_) => {}
                    }
                }
            }
        }
        None
    }

    pub(crate) fn pick_reserved_service(state: &mut ControlState) -> Option<NativeTurn> {
        while let Some(key) = state.pending_services.pop_front() {
            let Some(metadata) = state.services.get_mut(&key) else {
                continue;
            };
            match metadata.phase {
                ServicePhase::Reserved => {
                    metadata.phase = ServicePhase::Serving;
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::Serving(key));
                    return Some(NativeTurn::Service(key));
                }
                ServicePhase::Resuming(eligibility) => {
                    if Self::continuation_progress(&state.waits, metadata)
                        == ContinuationProgress::OwnerEnded
                    {
                        // Preserve the exact phase until actual native cleanup
                        // consumes it. Never send an owner wake to an ended stack.
                        state
                            .physical
                            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
                        return Some(NativeTurn::Pump);
                    }
                    metadata.phase = match eligibility {
                        ContinuationEligibility::Reserved => ServicePhase::Serving,
                        ContinuationEligibility::Released => ServicePhase::ServingAfterWait,
                    };
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::Serving(key));
                    return Some(NativeTurn::Continuation(key));
                }
                ServicePhase::Ended(_) => {
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
                    return Some(NativeTurn::Pump);
                }
                ServicePhase::Serving
                | ServicePhase::Continuing(_)
                | ServicePhase::PendingNative
                | ServicePhase::RealWait
                | ServicePhase::Reacquiring
                | ServicePhase::ServingAfterWait => {}
            }
        }
        None
    }

    pub(crate) fn continuation_progress(
        waits: &BTreeMap<ThreadKey, Wait>,
        metadata: &ServiceMetadata,
    ) -> ContinuationProgress {
        match metadata.phase {
            ServicePhase::Ended(_) => ContinuationProgress::OwnerEnded,
            ServicePhase::Continuing(_) | ServicePhase::Resuming(_) => {
                match waits.get(&metadata.requester) {
                    Some(Wait::Exited) => ContinuationProgress::OwnerEnded,
                    Some(
                        Wait::Runnable
                        | Wait::Join(_)
                        | Wait::Mutex(_)
                        | Wait::Condition(_)
                        | Wait::Select { .. }
                        | Wait::HostRequest(_),
                    )
                    | None => ContinuationProgress::AwaitingOwner,
                }
            }
            ServicePhase::Reserved
            | ServicePhase::Serving
            | ServicePhase::PendingNative
            | ServicePhase::RealWait
            | ServicePhase::Reacquiring
            | ServicePhase::ServingAfterWait => ContinuationProgress::NotContinuing,
        }
    }

    pub(crate) fn pending_native(state: &ControlState) -> NativeWork {
        if state.services.values().any(|metadata| {
            matches!(
                metadata.phase,
                ServicePhase::PendingNative | ServicePhase::RealWait
            ) || Self::continuation_progress(&state.waits, metadata)
                == ContinuationProgress::OwnerEnded
        }) {
            NativeWork::Ready
        } else {
            NativeWork::Idle
        }
    }

    pub(crate) fn pick_queued_service(state: &mut ControlState) -> Option<NativeTurn> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Receiving) {
            return None;
        }
        Self::pick_acquire(state).or_else(|| Self::pick_reserved_service(state))
    }

    /// Native acquire/service turns never complete main's semantic Wait.
    pub(crate) fn pick_service(state: &mut ControlState) -> Option<NativeTurn> {
        if let Some(turn) = Self::pick_queued_service(state) {
            return Some(turn);
        }
        if state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Receiving)
            && Self::pending_native(state) == NativeWork::Ready
        {
            // Pending native work remains a progress source even when no Lisp
            // owner is runnable. Reenter only the driver, never main's Lisp.
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
            return Some(NativeTurn::Pump);
        }
        None
    }

    pub(crate) fn queued_native_work(state: &ControlState) -> NativeWork {
        if state.pending_acquires.iter().any(|acquire| match acquire {
            NativeAcquire::Pump => Self::pending_native(state) == NativeWork::Ready,
            NativeAcquire::Service(key) => state
                .services
                .get(key)
                .is_some_and(|metadata| metadata.phase == ServicePhase::Reacquiring),
        }) || state.pending_services.iter().any(|key| {
            state.services.get(key).is_some_and(|metadata| {
                matches!(
                    metadata.phase,
                    ServicePhase::Reserved | ServicePhase::Resuming(_) | ServicePhase::Ended(_)
                )
            })
        }) {
            NativeWork::Ready
        } else {
            NativeWork::Idle
        }
    }

    pub(crate) fn service_ready(state: &ControlState) -> bool {
        state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Receiving)
            && Self::queued_native_work(state) == NativeWork::Ready
    }

    pub(crate) fn native_wake(turn: NativeTurn, baton: TurnRight) -> Wake {
        match turn {
            NativeTurn::Service(key) => Wake::ServiceTurn { key, baton },
            NativeTurn::Continuation(key) => Wake::ContinuationTurn { key, baton },
            NativeTurn::Pump => Wake::NativePump { baton },
        }
    }

    pub(crate) fn wake_for(next: NextOwner, baton: TurnRight) -> (ThreadKey, Wake) {
        match next {
            NextOwner::Resume(key) => (key, Wake::Resume(baton)),
            NextOwner::Native(turn) => (ThreadKey::Main, Self::native_wake(turn, baton)),
        }
    }

    pub(crate) fn intercept_ready(state: &mut ControlState, me: ThreadKey) -> Option<NativeTurn> {
        if me == ThreadKey::Main {
            state.physical.insert(me, OwnerLocation::Receiving);
        }
        // A pending-only driver is a scheduling fallback, not authority to
        // intercept a terminal's direct return to its reserved requester.
        if let Some(turn) = Self::pick_queued_service(state) {
            if me != ThreadKey::Main {
                state.physical.insert(me, OwnerLocation::Receiving);
                if state.waits.get(&me) == Some(&Wait::Runnable) {
                    // Its selected turn was already removed from FIFO. A
                    // native-only interception must not forfeit that turn.
                    Self::enqueue_ready_front(state, me);
                }
            }
            Some(turn)
        } else {
            state.physical.insert(me, OwnerLocation::Running);
            None
        }
    }

    /// A Resume may already be in flight when main announces its native
    /// acquire. Run that service first, before restoring any Context/JIT state.
    /// The loop holds scheduler metadata only while forwarding the Baton.
    pub(crate) fn intercept_acquire(
        &self,
        me: ThreadKey,
        mut wake: Wake,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Wake {
        loop {
            match wake {
                Wake::ServiceTurn { key, baton } => {
                    return Wake::ServiceTurn { key, baton };
                }
                Wake::NativePump { baton } => return Wake::NativePump { baton },
                Wake::OwnerStep { key, baton } => return Wake::OwnerStep { key, baton },
                Wake::ContinuationTurn { key, baton } => {
                    return Wake::ContinuationTurn { key, baton };
                }
                Wake::Resume(baton) => {
                    let key = Self::intercept_ready(&mut self.state(), me);
                    match key {
                        None => return Wake::Resume(baton),
                        Some(turn) if me == ThreadKey::Main => {
                            return Self::native_wake(turn, baton);
                        }
                        Some(turn) => {
                            self.send_wake(ThreadKey::Main, Self::native_wake(turn, baton));
                            wake = match receiver.recv() {
                                Ok(wake) => {
                                    wake.baton().enter();
                                    wake
                                }
                                Err(_) => std::process::abort(),
                            };
                        }
                    }
                }
            }
        }
    }

    /// Linearly surrender Green's unique `TurnRight::Solo(Baton)` so the caller
    /// can escrow the `Baton` in a parallel era (QD1). Refused if the runtime
    /// is closing, an era is already open, or `Solo` was already surrendered.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "L6.7b b4.3 Slice 2 era admission is the caller")
    )]
    pub(crate) fn surrender_solo_for_era(
        &self,
        right: TurnRight,
    ) -> Result<Baton, (TurnRight, EraOpenError)> {
        let mut state = self.state();
        if state.closing {
            return Err((right, EraOpenError::Closing));
        }
        match state.era {
            EraState::SoloActive => {}
            EraState::SoloSurrendered => return Err((right, EraOpenError::AlreadySurrendered)),
            EraState::EraOpen(_) => return Err((right, EraOpenError::AlreadyOpen)),
        }
        let baton = match right.into_solo() {
            Ok(baton) => baton,
            Err(right) => return Err((right, EraOpenError::AlreadyOpen)),
        };
        state.era = EraState::SoloSurrendered;
        Ok(baton)
    }

    /// Restore a previously surrendered `Baton` back into Green if era
    /// admission fails before [`Control::open_era`].
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "L6.7b b4.3 Slice 2 era admission rollback is the caller"
        )
    )]
    pub(crate) fn restore_surrendered_solo(&self, baton: Baton) -> Result<TurnRight, Baton> {
        let mut state = self.state();
        if !matches!(state.era, EraState::SoloSurrendered) {
            return Err(baton);
        }
        state.era = EraState::SoloActive;
        Ok(TurnRight::Solo(baton))
    }

    /// Open a parallel era on this `Control` (QD1 guard).
    ///
    /// Refused with [`EraOpenError::SoloBatonHeld`] if Green has not linearly
    /// surrendered its unique `TurnRight::Solo(Baton)` via
    /// [`Control::surrender_solo_for_era`].
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "L6.7b b4.3 era admission is the caller")
    )]
    pub(crate) fn open_era(
        &self,
        world: EraWorldHandle,
        port: GroupPort,
    ) -> Result<(), (EraWorldHandle, GroupPort, EraOpenError)> {
        let mut state = self.state();
        if state.closing {
            return Err((world, port, EraOpenError::Closing));
        }
        match state.era {
            EraState::SoloActive => return Err((world, port, EraOpenError::SoloBatonHeld)),
            EraState::EraOpen(_) => return Err((world, port, EraOpenError::AlreadyOpen)),
            EraState::SoloSurrendered => {}
        }
        let waiter = port.waiter_handle();
        let requester = port.request_sender();
        state.era = EraState::EraOpen(EraLink {
            world,
            waiter,
            requester: Some(requester),
            port: Some(port),
            turn: GroupTurn::Idle,
        });
        Ok(())
    }

    /// Acquire the Baton group's initial `WorldTurn` right after [`Self::open_era`].
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "L6.7b b4.3 era admission is the caller")
    )]
    pub(crate) fn acquire_initial_era_turn(&self) -> Result<TurnRight, PortRefusal> {
        let (requester, port) = {
            let mut state = self.state();
            let EraState::EraOpen(era) = &mut state.era else {
                return Err(PortRefusal::Poisoned);
            };
            if era.turn != GroupTurn::Idle {
                return Err(PortRefusal::Retry);
            }
            let Some(requester) = era.requester.clone() else {
                return Err(PortRefusal::Poisoned);
            };
            let Some(port) = era.port.take() else {
                return Err(PortRefusal::Retry);
            };
            era.turn = GroupTurn::Requested;
            (requester, port)
        };
        if let Err(err) = requester.request_tail() {
            let mut state = self.state();
            if let EraState::EraOpen(era) = &mut state.era {
                era.port = Some(port);
                era.turn = GroupTurn::Idle;
            }
            return Err(err);
        }
        self.await_era_world_with_port(port)
    }

    /// End the group's final active interval (`Holding -> Idle`) before
    /// leaving the era.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "L6.7b b4.3 era teardown is the caller")
    )]
    pub(crate) fn release_era_turn_to_idle(&self, right: TurnRight) {
        {
            let mut state = self.state();
            let EraState::EraOpen(era) = &mut state.era else {
                std::process::abort();
            };
            era.turn = GroupTurn::Idle;
        }
        Self::give_back_era_turn(right);
    }

    /// Remove the `GroupPort` (and drop the cloned inbox request sender) from
    /// an idle open era so the caller can invoke `GroupPort::exit()` and allow
    /// the keeper's `Driver::run()` to observe channel disconnect and drain.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "L6.7b b4.3 era teardown is the caller")
    )]
    pub(crate) fn take_era_port_for_exit(&self) -> Option<GroupPort> {
        let mut state = self.state();
        let EraState::EraOpen(era) = &mut state.era else {
            return None;
        };
        era.requester = None;
        era.port.take()
    }

    /// Close the active parallel era, restoring the keeper's escrowed `Baton`
    /// back into Green as `TurnRight::Solo(Baton)`.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "L6.7b b4.3 era teardown is the caller")
    )]
    pub(crate) fn close_era(
        &self,
        baton: Baton,
    ) -> Result<(TurnRight, EraWorldHandle, Option<GroupPort>), (Baton, EraCloseError)> {
        let mut state = self.state();
        if !matches!(state.era, EraState::EraOpen(_)) {
            return Err((baton, EraCloseError::NotOpen));
        }
        let EraState::EraOpen(link) = std::mem::replace(&mut state.era, EraState::SoloActive)
        else {
            unreachable!("checked EraOpen above");
        };
        Ok((TurnRight::Solo(baton), link.world, link.port))
    }

    /// Return a held `TurnRight::Era` back to the keeper (Case i / Case ii).
    /// Alloc regions must be closed, parked roots published, World carriers
    /// `CarriedHolder::Stowed`, and carrier TLS parked before calling (Nit 6).
    fn give_back_era_turn(right: TurnRight) {
        let era = match right.into_era() {
            Ok(era) => era,
            Err(_) => {
                debug_assert!(
                    false,
                    "QD1 violation: non-Era TurnRight passed to give_back_era_turn"
                );
                std::process::abort();
            }
        };
        let (turn, lend, _world, stowed) = era.into_parts_with_stowed();
        let proof = stowed.expect(
            "Nit 6 violation: EraTurnRight must carry StowedForReturn before returning to keeper",
        );
        lend.give_back(WorldReturned::from_stowed_seat(turn, proof));
    }

    /// Synchronous collection inside an open parallel era (L6.7b b4.4 / b5 QB3):
    /// keeps `EraLink.turn` in [`GroupTurn::Collecting`] (NOT `Idle`) while
    /// `Control.state` is unlocked across `GroupPort::collect`, so concurrently
    /// woken sibling Baton threads join the FIFO and never emit a duplicate
    /// `BatonRequest` (`Identity` refusal).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "L6.7b b4.4 gc_collect_from_current_roots_impl_flow is the caller"
        )
    )]
    pub(crate) fn collect_era_interval(
        &self,
        lend: WorldLend,
        returned: WorldReturned,
        reply: Option<mpsc::SyncSender<Result<CollectionReport, CollectRefusal>>>,
    ) -> Result<(WorldTurn, NonZeroU64, WorldLend), PortRefusal> {
        let mut port = {
            let mut state = self.state();
            let EraState::EraOpen(era) = &mut state.era else {
                return Err(PortRefusal::Poisoned);
            };
            if era.turn != GroupTurn::Holding {
                return Err(PortRefusal::Retry);
            }
            let Some(port) = era.port.take() else {
                return Err(PortRefusal::Retry);
            };
            era.turn = GroupTurn::Collecting;
            port
        };
        let result = port.collect(lend, returned, reply);
        let mut state = self.state();
        let EraState::EraOpen(era) = &mut state.era else {
            std::process::abort();
        };
        debug_assert!(
            era.port.is_none(),
            "GroupPort duplicated while collecting in open era"
        );
        era.port = Some(port);
        era.turn = if result.is_ok() {
            GroupTurn::Holding
        } else {
            GroupTurn::Idle
        };
        result
    }

    /// Coalesced re-request on wake (L6.7b r1 §3): if an era is open and
    /// `GroupTurn` is `Idle`, send ONE `BatonRequest` at the ring tail and
    /// transition `Idle -> Requested` only if the send succeeds (NIT-1). Later
    /// wakes only join the FIFO.
    fn coalesce_request_locked(state: &mut ControlState) -> bool {
        let EraState::EraOpen(era) = &mut state.era else {
            return false;
        };
        if era.turn == GroupTurn::Idle {
            if let Some(requester) = &era.requester {
                if requester.request_tail().is_ok() {
                    era.turn = GroupTurn::Requested;
                    return true;
                }
            }
        }
        false
    }

    /// Number of `Wait::Runnable` threads currently eligible to run, excluding
    /// `ThreadKey::Main` while `Self::main_excluded(state)` is true (HIGH-2a).
    pub(super) fn eligible_runnable_count(state: &ControlState) -> usize {
        let excluded_main = usize::from(
            Self::main_excluded(state)
                && state.waits.get(&ThreadKey::Main) == Some(&Wait::Runnable),
        );
        state.waits.runnable_count().saturating_sub(excluded_main)
    }

    /// Non-blocking check for whether any Baton owner or native service turn is
    /// immediately eligible to run right now (after polling non-blocking select
    /// readiness and expiring elapsed deadlines).
    pub(super) fn has_eligible_owner_now(&self, state: &mut ControlState) -> bool {
        if state.waits.select_len() > 0
            && let Some(Some(select)) = self.select.get()
            && crate::emacs_core::eval::process_hooks::select_wait_until(*select, Instant::now())
        {
            Self::wake_selects(state);
        }
        Self::expire_deadlines(state, Instant::now());
        let main_receiving =
            state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Receiving);
        let service_eligible = main_receiving
            && (Self::queued_native_work(state) == NativeWork::Ready
                || Self::pending_native(state) == NativeWork::Ready);
        service_eligible || Self::eligible_runnable_count(state) > 0
    }

    #[cfg(test)]
    pub(crate) fn mark_main_native_released_for_test(&self, wait: Wait) {
        let mut state = self.state();
        Self::set_wait(&mut state, ThreadKey::Main, wait);
        if wait == Wait::Runnable {
            Self::enqueue_ready(&mut state, ThreadKey::Main);
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
    }

    #[cfg(test)]
    pub(crate) fn eligible_runnable_count_for_test(&self) -> usize {
        Self::eligible_runnable_count(&self.state())
    }

    /// Claim the `GroupPort` receiver if `GroupTurn::Requested` and no other
    /// Baton thread is currently blocked in `await_world()`.
    fn take_era_port_if_requested(state: &mut ControlState) -> Option<GroupPort> {
        let EraState::EraOpen(era) = &mut state.era else {
            return None;
        };
        if era.turn == GroupTurn::Requested {
            era.port.take()
        } else {
            None
        }
    }

    fn recv_wake_and_enter(receiver: &mpsc::Receiver<Wake>) -> Wake {
        match receiver.recv() {
            Ok(wake) => {
                wake.baton().enter();
                wake
            }
            Err(_) => std::process::abort(),
        }
    }

    /// Block in `GroupPort::await_world()` without holding `Control.state`,
    /// then reinstall the port, set `GroupTurn::Holding`, and build
    /// `TurnRight::Era`.
    fn await_era_world_with_port(&self, mut port: GroupPort) -> Result<TurnRight, PortRefusal> {
        let result = port.await_world();
        let mut state = self.state();
        let EraState::EraOpen(era) = &mut state.era else {
            std::process::abort();
        };
        debug_assert!(
            era.port.is_none(),
            "GroupPort duplicated while awaiting World"
        );
        era.port = Some(port);
        match result {
            Ok((turn, _epoch, lend)) => {
                era.turn = GroupTurn::Holding;
                let world = era
                    .world
                    .try_clone()
                    .expect("era WorldHandle retains facade epoch while era is open");
                let proof = prove_lent_world_stowed(&turn, &world);
                let mut right = match TurnRight::from_era(turn, lend, world) {
                    Ok(right) => right,
                    Err(_) => std::process::abort(),
                };
                right.set_stowed_for_return(proof);
                Ok(right)
            }
            Err(refusal) => {
                era.turn = GroupTurn::Idle;
                Err(refusal)
            }
        }
    }

    /// Reacquire path for `NativeRelease::EraIdle` (and `Transferred` when a
    /// wake transitioned `Idle -> Requested` and left `port` unclaimed).
    fn reacquire_era_wake(
        &self,
        me: ThreadKey,
        port: Option<GroupPort>,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Wake {
        let Some(port) = port else {
            self.notify_select();
            return Self::recv_wake_and_enter(receiver);
        };
        let baton = match self.await_era_world_with_port(port) {
            Ok(right) => right,
            Err(_) => std::process::abort(),
        };
        let next = {
            let mut state = self.state();
            if let Some(turn) = Self::pick_service(&mut state) {
                Some(NextOwner::Native(turn))
            } else {
                match Self::pick(&mut state) {
                    Some((key, None)) => Some(NextOwner::Resume(key)),
                    Some((_, Some(_))) | None => None,
                }
            }
        };
        let Some(next) = next else {
            std::process::abort();
        };
        let (target, wake) = Self::wake_for(next, baton);
        if target == me {
            if matches!(&wake, Wake::Resume(_)) {
                let mut state = self.state();
                Self::set_wait(&mut state, me, Wait::Runnable);
                state.physical.insert(me, OwnerLocation::Running);
            }
            wake.baton().enter();
            return wake;
        }
        self.send_wake(target, wake);
        Self::recv_wake_and_enter(receiver)
    }

    /// Conservative QD2 deadlock-idle check: queries the keeper via
    /// `KeeperMsg::IdleQuery` (matching `GroupPort::idle_for_deadlock`) outside
    /// `Control.state` with a bounded wait (lead #14206). Returns `true` ONLY
    /// when the keeper confirms no live parallel owner exists and the ring is
    /// empty; any timeout, refusal, or doubt returns `false` conservatively.
    fn query_era_idle_for_deadlock(&self) -> bool {
        const IDLE_QUERY_BOUND: Duration = Duration::from_millis(50);
        let requester = {
            let state = self.state();
            match &state.era {
                EraState::EraOpen(era) => era.requester.clone(),
                _ => return false,
            }
        };
        let Some(requester) = requester else {
            return false;
        };
        matches!(requester.idle_for_deadlock(IDLE_QUERY_BOUND), Ok(true))
    }

    /// QD1 typed check: validate that `right` matches the scheduler's current
    /// era state (`EraOpen` requires `TurnRight::Era`; `SoloActive` requires
    /// `TurnRight::Solo`).
    fn check_turn_authority(
        state: &ControlState,
        right: &TurnRight,
    ) -> Result<(), WakeAuthorityError> {
        match (&state.era, right.is_solo()) {
            (EraState::SoloActive, true) | (EraState::EraOpen(_), false) => Ok(()),
            (EraState::EraOpen(_), true) => Err(WakeAuthorityError::SoloWakeInOpenEra),
            (EraState::SoloActive, false) => Err(WakeAuthorityError::EraWakeOutsideEra),
            (EraState::SoloSurrendered, _) => Err(WakeAuthorityError::SoloSurrendered),
        }
    }

    /// Debug-only QD1 guard for turn entry and seat restoration. Compiles to
    /// nothing in release builds so the Baton-only hot path adds zero loads.
    #[inline]
    pub(crate) fn debug_assert_turn_authority(&self, right: &TurnRight) {
        #[cfg(debug_assertions)]
        {
            let state = self.state();
            let check = Self::check_turn_authority(&state, right);
            debug_assert!(
                check.is_ok(),
                "QD1 violation: Green thread holds wrong TurnRight authority for era state: {check:?}"
            );
        }
        #[cfg(not(debug_assertions))]
        let _ = right;
    }

    /// Validate `wake` against the current era state (QD1) and send it to
    /// `target`. Returns `Err((wake, error))` without sending if the `Wake`
    /// carries the wrong `TurnRight` variant (e.g. `TurnRight::Solo` while an
    /// era is open).
    pub(crate) fn try_send_wake(
        &self,
        target: ThreadKey,
        wake: Wake,
    ) -> Result<(), (Wake, WakeAuthorityError)> {
        let sender = {
            let state = self.state();
            if let Err(error) = Self::check_turn_authority(&state, wake.baton()) {
                return Err((wake, error));
            }
            state.senders.get(&target).cloned()
        };
        let Some(sender) = sender else {
            std::process::abort()
        };
        // There is exactly one active TurnRight. A closed receiver or occupied
        // slot is an ownership failure, never a Lisp-level request failure.
        if sender.try_send(wake).is_err() {
            std::process::abort()
        }
        Ok(())
    }

    pub(crate) fn send_wake(&self, target: ThreadKey, wake: Wake) {
        if let Err((_wake, error)) = self.try_send_wake(target, wake) {
            debug_assert!(
                false,
                "QD1 violation: send_wake received wrong TurnRight authority: {error:?}"
            );
            tracing::error!(
                ?target,
                ?error,
                "QD1 violation: send_wake received wrong TurnRight authority"
            );
            std::process::abort();
        }
    }

    /// Choose an owner, blocking only while every owner is parked. The
    /// Baton stays local and no World cell is read during this OS wait.
    pub(crate) fn pick_next(&self) -> PickNext {
        loop {
            // Register BEFORE checking scheduler predicates. A transition
            // between this check and the OS block advances the same snapshot;
            // arming after dropping state would discard that durable nudge.
            let select = self.select.get().and_then(Option::as_ref).copied();
            let mut registration =
                select.map(crate::emacs_core::eval::process_hooks::SelectArmGuard::arm);
            let deadline = {
                let mut state = self.state();
                if state.waits.select_len() > 0
                    && let Some(Some(select)) = self.select.get()
                    && crate::emacs_core::eval::process_hooks::select_wait_until(
                        *select,
                        Instant::now(),
                    )
                {
                    Self::wake_selects(&mut state);
                }
                if let Some(turn) = Self::pick_service(&mut state) {
                    return PickNext::Owner(NextOwner::Native(turn));
                }
                match Self::pick(&mut state) {
                    Some((key, None)) => return PickNext::Owner(NextOwner::Resume(key)),
                    Some((_, Some(deadline))) => deadline,
                    None => {
                        if state.physical.get(&ThreadKey::Main)
                            == Some(&OwnerLocation::NativeReleased)
                            && (!state.closing || Self::pending_native(&state) == NativeWork::Ready)
                        {
                            // No ordinary owner can run, but main's actual
                            // native wait can still complete. Closing cannot
                            // drop the last Baton while native settlements and
                            // their parked roots remain live. Keep it out of
                            // Lisp until the driver's acquire is announced.
                            drop(
                                self.metadata_changed
                                    .wait(state)
                                    .unwrap_or_else(PoisonError::into_inner),
                            );
                            continue;
                        }
                        return PickNext::NoRunnable(Self::verify_invariants(&state));
                    }
                }
            };
            let activity = if let Some(select) = select {
                match &mut registration {
                    Some(guard) => guard.wait(select, deadline),
                    None => false,
                }
            } else {
                let mut state = self.state();
                // Recheck under the same lock used by the notifier: an acquire
                // between the initial scan and this wait must not be lost.
                if let Some(turn) = Self::pick_service(&mut state) {
                    return PickNext::Owner(NextOwner::Native(turn));
                }
                if let Some((key, None)) = Self::pick(&mut state) {
                    return PickNext::Owner(NextOwner::Resume(key));
                }
                drop(
                    self.metadata_changed
                        .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                        .unwrap_or_else(PoisonError::into_inner),
                );
                false
            };
            if activity {
                Self::wake_selects(&mut self.state());
            }
            // An eventless/spurious return never expires a deadline early.
            // Activity wakes may expire deadlines (`wake_selects`), but only
            // ones a monotonic now shows already passed; pick's monotonic
            // check remains the sole clock-based wake.
        }
    }

    /// Maintenance seams shared with the scheduler-index implementation.
    /// New transitions must not mutate the wait map or ready queue elsewhere.
    ///
    /// `set_wait` moves KEY's wait and every wait-derived index together. A
    /// transition away from `Runnable` also makes KEY's queued turn stale;
    /// a later `Runnable` transition queues a fresh turn via `enqueue_ready`.
    pub(crate) fn set_wait(state: &mut ControlState, key: ThreadKey, wait: Wait) {
        state.waits.insert(key, wait);
        if !matches!(wait, Wait::Runnable) {
            state.ready.invalidate(key);
        }
        Self::check_invariants(state);
    }

    /// Remove every scheduler record that names an ended thread. This is the
    /// single whole-key removal seam: `unregister_worker` and `finish` call it
    /// only after publishing `Wait::Exited` through [`Self::set_wait`], so the
    /// ended fact stays typed (`exited` reads an absent wait as ended) and
    /// joiners keep observing it. No production code removes a key piecemeal
    /// beside `set_wait`: this seam releases the wait, its index entries and
    /// counts, the sender and the physical location together. Before discarding
    /// an observed exit, retain in-flight continuation authority in its service
    /// record. Arbitrary absence cannot authorize native cleanup.
    pub(crate) fn remove_key(state: &mut ControlState, key: ThreadKey) {
        debug_assert!(
            matches!(state.waits.get(&key), Some(Wait::Exited) | None),
            "remove_key follows the exit publication, not a pause"
        );
        if state.waits.get(&key) == Some(&Wait::Exited) {
            for metadata in state.services.values_mut() {
                if metadata.requester == key {
                    metadata.phase = match metadata.phase {
                        ServicePhase::Continuing(eligibility) => {
                            ServicePhase::Ended(EndedContinuationPhase::Continuing(eligibility))
                        }
                        ServicePhase::Resuming(eligibility) => {
                            ServicePhase::Ended(EndedContinuationPhase::Resuming(eligibility))
                        }
                        ServicePhase::Reserved
                        | ServicePhase::Serving
                        | ServicePhase::PendingNative
                        | ServicePhase::RealWait
                        | ServicePhase::Reacquiring
                        | ServicePhase::ServingAfterWait
                        | ServicePhase::Ended(_) => metadata.phase,
                    };
                }
            }
        }
        state.waits.remove(&key);
        state.senders.remove(&key);
        state.physical.remove(&key);
        state.ready.invalidate(key);
        Self::check_invariants(state);
    }

    /// Queue KEY's ready turn at the back if it has none. The caller already
    /// made KEY `Runnable` through [`Self::set_wait`].
    pub(crate) fn enqueue_ready(state: &mut ControlState, key: ThreadKey) {
        state.ready.enqueue_back(key);
        Self::check_invariants(state);
    }

    /// Queue KEY's ready turn at the front if it has none (the native
    /// interception requeue preserves the turn it is about to use).
    pub(crate) fn enqueue_ready_front(state: &mut ControlState, key: ThreadKey) {
        state.ready.enqueue_front(key);
        Self::check_invariants(state);
    }

    pub(crate) fn pop_ready(
        state: &mut ControlState,
        selection: ReadySelection,
    ) -> Option<ThreadKey> {
        match selection {
            ReadySelection::NativeMainAcquire => return state.ready.pop_main(),
            ReadySelection::NextEligible => {}
        }
        // Keep a suspended main's exact FIFO position while selecting peers.
        // Its ordinary turn becomes eligible only after native completion.
        let main_excluded = Self::main_excluded(state);
        let ControlState { waits, ready, .. } = state;
        let waits = &*waits;
        let key = ready.pop_valid(
            |key| waits.get(&key) == Some(&Wait::Runnable),
            |key| key != ThreadKey::Main || !main_excluded,
        );
        Self::check_invariants(state);
        key
    }

    /// Whether main's ordinary ready turn is ineligible: its physical wait
    /// owns the Baton until native completion.
    pub(crate) fn main_excluded(state: &ControlState) -> bool {
        matches!(
            state.physical.get(&ThreadKey::Main),
            Some(
                OwnerLocation::NativeReleased
                    | OwnerLocation::NativeDriving
                    | OwnerLocation::Serving(_)
            )
        )
    }

    /// The next thread to hold the Baton, if any can run; with no ready
    /// thread but one waiting for a deadline, the earliest deadline.
    pub(crate) fn pick(state: &mut ControlState) -> Option<(ThreadKey, Option<Instant>)> {
        Self::pick_at(state, Instant::now())
    }

    /// `pick` at an explicit clock instant (the differential harness drives
    /// the same instants through both the reference scans and this code).
    pub(crate) fn pick_at(
        state: &mut ControlState,
        now: Instant,
    ) -> Option<(ThreadKey, Option<Instant>)> {
        Self::expire_deadlines(state, now);
        if let Some(key) = Self::pop_ready(state, ReadySelection::NextEligible) {
            return Some((key, None));
        }
        let main_excluded = Self::main_excluded(state);
        for deadline in state.waits.select_index.keys() {
            if deadline.key == ThreadKey::Main && main_excluded {
                continue;
            }
            return Some((deadline.key, Some(deadline.wait_end)));
        }
        None
    }

    /// Debug per-transition contract: the same predicate the release deadlock
    /// verdict runs, so both builds agree on what an inconsistent index is.
    #[cfg(debug_assertions)]
    pub(crate) fn check_invariants(state: &ControlState) {
        if let Err(error) = state.verify_invariants() {
            panic!("scheduler wait-index invariant violated: {error}");
        }
    }

    #[cfg(not(debug_assertions))]
    pub(crate) fn check_invariants(state: &ControlState) {
        let _ = state;
    }

    /// Cold verdict check: every build runs the same predicate when the
    /// scheduler decides no owner can run. The caller passes the state it
    /// decided on, so the verdict and the predicate share one lock
    /// acquisition; it never repairs the index.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn verify_invariants(state: &ControlState) -> Result<(), SchedulerIndexInconsistency> {
        state.verify_invariants()
    }

    /// Hand the Baton on from ME, which waits for WAIT, and block until it
    /// comes back. Returns it at once when ME is the next to run.
    #[doc(hidden)]
    pub fn transfer(
        &self,
        me: ThreadKey,
        wait: Wait,
        baton: TurnRight,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<Wake, Handoff> {
        if matches!(wait, Wait::HostRequest(_)) {
            // Host parks require their linear reservation and publication
            // plan. In particular, NoRunnable must not revive a host waiter.
            // This refusal is itself a deadlock verdict, so check the index
            // under the state that produces it.
            let invariant = {
                let state = self.state();
                Self::verify_invariants(&state)
            };
            return Err(Handoff::NoRunnable { baton, invariant });
        }
        {
            let mut state = self.state();
            Self::note_continuation_release(&mut state, me);
            Self::set_wait(&mut state, me, wait);
            state.physical.insert(me, OwnerLocation::Receiving);
            if wait == Wait::Runnable {
                Self::enqueue_ready(&mut state, me);
            }
        }
        #[cfg(any(test, feature = "test-support"))]
        if let Wait::Select { wait_end } = wait {
            self.observe_select_park_for_test(me, wait_end);
        }
        if baton.is_era() {
            return self.transfer_era(me, baton, receiver);
        }
        let next = match self.pick_next() {
            PickNext::Owner(next) => next,
            PickNext::NoRunnable(invariant) => {
                let mut state = self.state();
                Self::set_wait(&mut state, me, Wait::Runnable);
                state.physical.insert(me, OwnerLocation::Running);
                return Err(Handoff::NoRunnable { baton, invariant });
            }
        };
        let (target, wake) = Self::wake_for(next, baton);
        if target == me {
            if matches!(&wake, Wake::Resume(_)) {
                let mut state = self.state();
                Self::set_wait(&mut state, me, Wait::Runnable);
                state.physical.insert(me, OwnerLocation::Running);
            }
            return Ok(wake);
        }
        self.send_wake(target, wake);
        match receiver.recv() {
            Ok(wake) => {
                wake.baton().enter();
                if matches!(&wake, Wake::Resume(_)) {
                    self.state().physical.insert(me, OwnerLocation::Running);
                }
                Ok(wake)
            }
            Err(_) => std::process::abort(),
        }
    }

    /// Era-mode switch point for [`Self::transfer`] (L6.7b b4.3):
    /// - Case (ii): `ring_waiter()` is true before `pick_next` -> end the
    ///   interval, re-request at the ring TAIL (QD3), await the next
    ///   `WorldTurn`, then dispatch to the FIFO head.
    /// - Case (i): no runnable Baton owner right now -> end the interval
    ///   (`GroupTurn::Idle`), give back `WorldTurn`, wait without holding the
    ///   World until a wake/deadline arrives or conservative QD2 confirms
    ///   deadlock.
    fn transfer_era(
        &self,
        me: ThreadKey,
        mut baton: TurnRight,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<Wake, Handoff> {
        loop {
            enum EraTransferStep {
                CaseIi {
                    requester: GroupRequestSender,
                    port: Option<GroupPort>,
                },
                Owner(NextOwner),
                EndIntervalToIdle,
            }
            // Check eligibility before choosing Case (ii) vs Case (i) so when
            // the last runnable Baton thread blocks while `ring_waiter()` is
            // raised, we enter Case (i) directly without emitting a spurious
            // empty `BatonRequest` round-trip (MEDIUM-2).
            let step = {
                let mut state = self.state();
                let eligible_now = self.has_eligible_owner_now(&mut state);
                if self.era_ring_waiter(&state) {
                    if eligible_now {
                        let EraState::EraOpen(era) = &mut state.era else {
                            std::process::abort();
                        };
                        let Some(requester) = era.requester.clone() else {
                            std::process::abort();
                        };
                        era.turn = GroupTurn::Requested;
                        let port = era.port.take();
                        EraTransferStep::CaseIi { requester, port }
                    } else {
                        let EraState::EraOpen(era) = &mut state.era else {
                            std::process::abort();
                        };
                        era.turn = GroupTurn::Idle;
                        EraTransferStep::EndIntervalToIdle
                    }
                } else if let Some(turn) = Self::pick_service(&mut state) {
                    EraTransferStep::Owner(NextOwner::Native(turn))
                } else {
                    match Self::pick(&mut state) {
                        Some((key, None)) => EraTransferStep::Owner(NextOwner::Resume(key)),
                        Some((_, Some(_))) | None => {
                            let EraState::EraOpen(era) = &mut state.era else {
                                std::process::abort();
                            };
                            era.turn = GroupTurn::Idle;
                            EraTransferStep::EndIntervalToIdle
                        }
                    }
                }
            };
            match step {
                EraTransferStep::CaseIi { requester, port } => {
                    Self::give_back_era_turn(baton);
                    if requester.request_tail().is_err() {
                        std::process::abort();
                    }
                    let Some(port) = port else {
                        let wake = Self::recv_wake_and_enter(receiver);
                        if matches!(&wake, Wake::Resume(_)) {
                            self.state().physical.insert(me, OwnerLocation::Running);
                        }
                        return Ok(wake);
                    };
                    baton = match self.await_era_world_with_port(port) {
                        Ok(right) => right,
                        Err(_) => std::process::abort(),
                    };
                    continue;
                }
                EraTransferStep::Owner(next) => {
                    let (target, wake) = Self::wake_for(next, baton);
                    if target == me {
                        if matches!(&wake, Wake::Resume(_)) {
                            let mut state = self.state();
                            Self::set_wait(&mut state, me, Wait::Runnable);
                            state.physical.insert(me, OwnerLocation::Running);
                        }
                        wake.baton().enter();
                        return Ok(wake);
                    }
                    self.send_wake(target, wake);
                    let wake = Self::recv_wake_and_enter(receiver);
                    if matches!(&wake, Wake::Resume(_)) {
                        self.state().physical.insert(me, OwnerLocation::Running);
                    }
                    return Ok(wake);
                }
                EraTransferStep::EndIntervalToIdle => {
                    // Case (i): give back WorldTurn before blocking so parallel
                    // owners run freely while Baton threads sleep/wait.
                    Self::give_back_era_turn(baton);
                    match self.wait_while_era_idle(receiver) {
                        EraIdleWait::Acquired(new_baton) => {
                            baton = new_baton;
                            continue;
                        }
                        EraIdleWait::WokenByPeer(wake) => {
                            if matches!(&wake, Wake::Resume(_)) {
                                self.state().physical.insert(me, OwnerLocation::Running);
                            }
                            return Ok(wake);
                        }
                        EraIdleWait::Deadlock { baton, invariant } => {
                            let mut state = self.state();
                            Self::set_wait(&mut state, me, Wait::Runnable);
                            state.physical.insert(me, OwnerLocation::Running);
                            return Err(Handoff::NoRunnable { baton, invariant });
                        }
                    }
                }
            }
        }
    }

    /// Wait without holding a `WorldTurn` while `GroupTurn` is `Idle` (Case i).
    fn wait_while_era_idle(&self, receiver: &mpsc::Receiver<Wake>) -> EraIdleWait {
        const DEADLOCK_POLL: Duration = Duration::from_millis(5);
        loop {
            let select = self.select.get().and_then(Option::as_ref).copied();
            let mut registration =
                select.map(crate::emacs_core::eval::process_hooks::SelectArmGuard::arm);
            enum Action {
                AwaitWorld(GroupPort),
                BlockOnChannel,
                WaitDeadline(Instant),
                WaitNativeMain,
                CheckDeadlock,
            }
            let action = {
                let mut state = self.state();
                if state.waits.select_len() > 0
                    && let Some(Some(select)) = self.select.get()
                    && crate::emacs_core::eval::process_hooks::select_wait_until(
                        *select,
                        Instant::now(),
                    )
                {
                    Self::wake_selects(&mut state);
                }
                let turn = match &state.era {
                    EraState::EraOpen(era) => era.turn,
                    _ => std::process::abort(),
                };
                match turn {
                    GroupTurn::Holding | GroupTurn::Collecting => Action::BlockOnChannel,
                    GroupTurn::Requested => match Self::take_era_port_if_requested(&mut state) {
                        Some(port) => Action::AwaitWorld(port),
                        None => Action::BlockOnChannel,
                    },
                    GroupTurn::Idle => {
                        Self::expire_deadlines(&mut state, Instant::now());
                        let now_requested = matches!(
                            &state.era,
                            EraState::EraOpen(era) if era.turn == GroupTurn::Requested
                        );
                        if now_requested {
                            match Self::take_era_port_if_requested(&mut state) {
                                Some(port) => Action::AwaitWorld(port),
                                None => Action::BlockOnChannel,
                            }
                        } else if Self::queued_native_work(&state) == NativeWork::Ready
                            || Self::eligible_runnable_count(&state) > 0
                        {
                            Self::coalesce_request_locked(&mut state);
                            match Self::take_era_port_if_requested(&mut state) {
                                Some(port) => Action::AwaitWorld(port),
                                None => Action::BlockOnChannel,
                            }
                        } else {
                            let main_excluded = Self::main_excluded(&state);
                            let next_deadline = state
                                .waits
                                .select_index
                                .keys()
                                .find(|d| !(d.key == ThreadKey::Main && main_excluded))
                                .map(|d| d.wait_end);
                            if let Some(deadline) = next_deadline {
                                Action::WaitDeadline(deadline)
                            } else if state.physical.get(&ThreadKey::Main)
                                == Some(&OwnerLocation::NativeReleased)
                                && (!state.closing
                                    || Self::pending_native(&state) == NativeWork::Ready)
                            {
                                Action::WaitNativeMain
                            } else {
                                Action::CheckDeadlock
                            }
                        }
                    }
                }
            };
            match action {
                Action::AwaitWorld(port) => {
                    let baton = match self.await_era_world_with_port(port) {
                        Ok(right) => right,
                        Err(_) => std::process::abort(),
                    };
                    return EraIdleWait::Acquired(baton);
                }
                Action::BlockOnChannel => {
                    return EraIdleWait::WokenByPeer(Self::recv_wake_and_enter(receiver));
                }
                Action::WaitDeadline(deadline) => {
                    let activity = if let Some(select) = select {
                        match &mut registration {
                            Some(guard) => guard.wait(select, deadline),
                            None => false,
                        }
                    } else {
                        let state = self.state();
                        if !matches!(&state.era, EraState::EraOpen(era) if era.turn == GroupTurn::Idle)
                        {
                            continue;
                        }
                        drop(
                            self.metadata_changed
                                .wait_timeout(
                                    state,
                                    deadline.saturating_duration_since(Instant::now()),
                                )
                                .unwrap_or_else(PoisonError::into_inner),
                        );
                        false
                    };
                    if activity {
                        Self::wake_selects(&mut self.state());
                    }
                }
                Action::WaitNativeMain => {
                    let state = self.state();
                    if !matches!(&state.era, EraState::EraOpen(era) if era.turn == GroupTurn::Idle)
                    {
                        continue;
                    }
                    drop(
                        self.metadata_changed
                            .wait(state)
                            .unwrap_or_else(PoisonError::into_inner),
                    );
                }
                Action::CheckDeadlock => {
                    // QD2: query keeper outside Control.state lock. Only report
                    // deadlock when keeper confirms no live parallel owner AND
                    // empty ring AND re-check under lock still shows Idle with
                    // every Baton thread in a non-select releasing wait.
                    if self.query_era_idle_for_deadlock() {
                        let confirmed = {
                            let mut state = self.state();
                            let still_idle = matches!(
                                &state.era,
                                EraState::EraOpen(era) if era.turn == GroupTurn::Idle
                            );
                            let any_runnable = Self::eligible_runnable_count(&state) > 0
                                || Self::queued_native_work(&state) == NativeWork::Ready
                                || state.waits.select_len() > 0
                                || state.physical.get(&ThreadKey::Main)
                                    == Some(&OwnerLocation::NativeReleased);
                            if still_idle && !any_runnable {
                                let invariant = Self::verify_invariants(&state);
                                if !Self::coalesce_request_locked(&mut state) {
                                    // Requester channel closed / keeper poisoned: fail closed
                                    // instead of spinning in CheckDeadlock (LOW-3).
                                    std::process::abort();
                                }
                                let port = Self::take_era_port_if_requested(&mut state);
                                Some((port, invariant))
                            } else {
                                None
                            }
                        };
                        if let Some((Some(port), invariant)) = confirmed {
                            let baton = match self.await_era_world_with_port(port) {
                                Ok(right) => right,
                                Err(_) => std::process::abort(),
                            };
                            baton.enter();
                            return EraIdleWait::Deadlock { baton, invariant };
                        }
                        // Fall through to the 5 ms `metadata_changed` wait below rather
                        // than spinning immediately when `confirmed` is `None` (e.g.
                        // `closing` with `Main == NativeReleased`) or `Some((None, _))`
                        // (another waiter took the port) (LOW-3).
                    }
                    let state = self.state();
                    if !matches!(&state.era, EraState::EraOpen(era) if era.turn == GroupTurn::Idle)
                    {
                        continue;
                    }
                    drop(
                        self.metadata_changed
                            .wait_timeout(state, DEADLOCK_POLL)
                            .unwrap_or_else(PoisonError::into_inner),
                    );
                }
            }
        }
    }

    /// Publish exit and join readiness before entering a scheduler/native wait.
    /// A released native main may be the only owner that can make progress.
    pub(crate) fn publish_owner_exit(&self, me: ThreadKey) {
        {
            let mut state = self.state();
            Self::set_wait(&mut state, me, Wait::Exited);
            state.physical.remove(&me);
            let joiners: Vec<ThreadKey> = state
                .waits
                .join_waiters
                .get(&me)
                .map(|set| set.iter().copied().collect())
                .unwrap_or_default();
            for key in joiners {
                Self::wake_locked(&mut state, key);
            }
        }
        // NativeMain observes a different arm from the idle selector. Wake it
        // before pick_next can wait for that owner's physical reacquisition.
        self.notify_select();
    }

    /// Start closing: every waiting thread is woken, to signal and end.
    #[doc(hidden)]
    pub fn close(&self) {
        let mut state = self.state();
        state.closing = true;
        let blocked: Vec<ThreadKey> = state
            .waits
            .iter()
            .filter(|(_, wait)| !matches!(wait, Wait::Exited | Wait::Runnable))
            .map(|(key, _)| *key)
            .collect();
        for key in blocked {
            Self::wake_locked(&mut state, key);
        }
        drop(state);
        self.notify_select();
    }
}

#[cfg(test)]
#[path = "../tests/green_scheduler_index_test.rs"]
mod index_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn host_key() -> HostRequestKey {
        HostRequestKey::for_test(NonZeroU64::MIN)
    }

    fn continuation_slice(key: HostRequestKey, requester: ThreadKey) -> ReservedServiceSlice {
        ReservedServiceSlice {
            key,
            requester,
            _confined: PhantomData,
        }
    }

    #[test]
    fn ended_service_owner_mint_accepts_exact_world_and_preserves_sibling_reservations() {
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = TurnRight::from(owner.start_green_runtime().unwrap());
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        let sibling = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let slice = continuation_slice(key, requester);
        let sibling_phase = ServicePhase::Continuing(ContinuationEligibility::Reserved);
        for phase in [
            ServicePhase::Continuing(ContinuationEligibility::Reserved),
            ServicePhase::Continuing(ContinuationEligibility::Released),
            ServicePhase::Resuming(ContinuationEligibility::Reserved),
            ServicePhase::Resuming(ContinuationEligibility::Released),
        ] {
            {
                let mut state = control.state();
                Control::set_wait(&mut state, requester, Wait::Exited);
                state
                    .services
                    .insert(key, ServiceMetadata { requester, phase });
                state.services.insert(
                    sibling,
                    ServiceMetadata {
                        requester,
                        phase: sibling_phase,
                    },
                );
                state.pending_services = VecDeque::from([sibling, key]);
            }
            assert!(baton.held_here());
            let ended = control.ended_service_owner(&slice, &baton).unwrap();
            assert_eq!(ended.key(), key);
            assert_eq!(ended.requester(), requester);
            let state = control.state();
            assert_eq!(state.services.len(), 2);
            assert_eq!(state.services[&key].requester, requester);
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(state.services[&sibling].requester, requester);
            assert_eq!(state.services[&sibling].phase, sibling_phase);
            assert_eq!(state.pending_services, VecDeque::from([sibling, key]));
            assert_eq!(state.waits[&requester], Wait::Exited);
        }
    }

    #[test]
    fn ended_continuation_authority_survives_indexed_whole_key_retirement() {
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = TurnRight::from(owner.start_green_runtime().unwrap());
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let other = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let key = host_key();
        let nested = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let sibling = HostRequestKey::for_test(NonZeroU64::new(3).unwrap());
        let original = Wait::Join(other);
        let nested_phase = ServicePhase::Continuing(ContinuationEligibility::Reserved);
        let sibling_phase = ServicePhase::Reserved;
        let slice = continuation_slice(key, requester);
        for (phase, ended_phase) in [
            (
                ServicePhase::Continuing(ContinuationEligibility::Reserved),
                EndedContinuationPhase::Continuing(ContinuationEligibility::Reserved),
            ),
            (
                ServicePhase::Continuing(ContinuationEligibility::Released),
                EndedContinuationPhase::Continuing(ContinuationEligibility::Released),
            ),
            (
                ServicePhase::Resuming(ContinuationEligibility::Reserved),
                EndedContinuationPhase::Resuming(ContinuationEligibility::Reserved),
            ),
            (
                ServicePhase::Resuming(ContinuationEligibility::Released),
                EndedContinuationPhase::Resuming(ContinuationEligibility::Released),
            ),
        ] {
            {
                let mut state = control.state();
                Control::set_wait(&mut state, ThreadKey::Main, original);
                Control::set_wait(&mut state, requester, Wait::Exited);
                Control::set_wait(&mut state, other, Wait::HostRequest(sibling));
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                state
                    .services
                    .insert(key, ServiceMetadata { requester, phase });
                state.services.insert(
                    nested,
                    ServiceMetadata {
                        requester,
                        phase: nested_phase,
                    },
                );
                state.services.insert(
                    sibling,
                    ServiceMetadata {
                        requester: other,
                        phase: sibling_phase,
                    },
                );
                state.pending_services = VecDeque::from([key, sibling]);
                Control::remove_key(&mut state, requester);
                assert!(!state.waits.contains_key(&requester));
                assert!(!state.senders.contains_key(&requester));
                assert!(!state.physical.contains_key(&requester));
                assert_eq!(state.services[&key].phase, ServicePhase::Ended(ended_phase));
                assert_eq!(
                    state.services[&nested].phase,
                    ServicePhase::Ended(EndedContinuationPhase::Continuing(
                        ContinuationEligibility::Reserved
                    ))
                );
                assert_eq!(state.services[&sibling].phase, sibling_phase);
                assert_eq!(Control::pending_native(&state), NativeWork::Ready);
                assert_eq!(Control::queued_native_work(&state), NativeWork::Ready);
                assert_eq!(Control::pick_service(&mut state), Some(NativeTurn::Pump));
                assert_eq!(state.waits[&ThreadKey::Main], original);
                assert!(state.ready.is_empty());
            }
            let ended = control.ended_service_owner(&slice, &baton).unwrap();
            assert_eq!(ended.key(), key);
            assert_eq!(ended.requester(), requester);
            let mut state = control.state();
            // Test the metadata removal without manufacturing a native settlement
            // receipt. Production can remove it only after the actor settles.
            Control::finish_ended_continuation_metadata(&mut state, &slice).unwrap();
            assert!(!state.services.contains_key(&key));
            assert!(state.services.contains_key(&nested));
            assert_eq!(state.services[&sibling].phase, sibling_phase);
            assert_eq!(state.waits[&other], Wait::HostRequest(sibling));
            assert!(!state.waits.contains_key(&requester));
            assert_eq!(state.waits[&ThreadKey::Main], original);
        }
    }

    #[test]
    fn indexed_removal_without_observed_exit_cannot_mint_continuation_authority() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        for phase in [
            ServicePhase::Continuing(ContinuationEligibility::Reserved),
            ServicePhase::Continuing(ContinuationEligibility::Released),
            ServicePhase::Resuming(ContinuationEligibility::Reserved),
            ServicePhase::Resuming(ContinuationEligibility::Released),
        ] {
            let mut state = ControlState::default();
            state
                .services
                .insert(key, ServiceMetadata { requester, phase });
            Control::remove_key(&mut state, requester);
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(Control::pending_native(&state), NativeWork::Idle);
            assert_eq!(
                Control::validate_ended_continuation(&state, &continuation_slice(key, requester)),
                Err(ServiceRefusal::WrongServiceState)
            );
        }
    }

    #[test]
    fn ended_service_owner_mint_rejects_live_requester_without_consuming_reservation() {
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = TurnRight::from(owner.start_green_runtime().unwrap());
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        let slice = continuation_slice(key, requester);
        let phase = ServicePhase::Continuing(ContinuationEligibility::Released);
        for wait in [Wait::Runnable, Wait::HostRequest(key)] {
            {
                let mut state = control.state();
                Control::set_wait(&mut state, requester, wait);
                state
                    .services
                    .insert(key, ServiceMetadata { requester, phase });
                state.pending_services = VecDeque::from([key]);
            }
            assert_eq!(
                control.ended_service_owner(&slice, &baton).unwrap_err(),
                ServiceRefusal::WrongServiceState
            );
            let state = control.state();
            assert_eq!(state.services.len(), 1);
            assert_eq!(state.services[&key].requester, requester);
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(state.pending_services, VecDeque::from([key]));
            assert_eq!(state.waits[&requester], wait);
        }
    }

    #[test]
    fn ended_service_owner_mint_rejects_foreign_control_with_matching_ended_metadata() {
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = TurnRight::from(owner.start_green_runtime().unwrap());
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let foreign = Control::default();
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        let slice = continuation_slice(key, requester);
        let phase = ServicePhase::Resuming(ContinuationEligibility::Released);
        for scheduler in [control.as_ref(), &foreign] {
            let mut state = scheduler.state();
            Control::set_wait(&mut state, requester, Wait::Exited);
            state
                .services
                .insert(key, ServiceMetadata { requester, phase });
            state.pending_services = VecDeque::from([key]);
        }
        assert!(baton.held_here());
        assert_eq!(
            foreign.ended_service_owner(&slice, &baton).unwrap_err(),
            ServiceRefusal::WrongServiceState
        );
        let ended = control.ended_service_owner(&slice, &baton).unwrap();
        assert_eq!(ended.key(), key);
        assert_eq!(ended.requester(), requester);
        for scheduler in [control.as_ref(), &foreign] {
            let state = scheduler.state();
            assert_eq!(state.services.len(), 1);
            assert_eq!(state.services[&key].requester, requester);
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(state.pending_services, VecDeque::from([key]));
            assert_eq!(state.waits[&requester], Wait::Exited);
        }
    }

    #[test]
    fn ended_owner_continuation_selects_native_cleanup_driver_without_resuming_main() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        let original = Wait::Join(requester);
        for phase in [
            ServicePhase::Continuing(ContinuationEligibility::Reserved),
            ServicePhase::Continuing(ContinuationEligibility::Released),
            ServicePhase::Resuming(ContinuationEligibility::Reserved),
            ServicePhase::Resuming(ContinuationEligibility::Released),
        ] {
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, original);
            Control::set_wait(&mut state, requester, Wait::Runnable);
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Receiving);
            state
                .services
                .insert(key, ServiceMetadata { requester, phase });
            assert_eq!(Control::pending_native(&state), NativeWork::Idle);
            assert_eq!(
                Control::pick_service(&mut state),
                None,
                "a live continuation must not become runnable native cleanup"
            );
            Control::set_wait(&mut state, requester, Wait::Exited);
            assert_eq!(Control::pending_native(&state), NativeWork::Ready);
            assert_eq!(Control::pick_service(&mut state), Some(NativeTurn::Pump));
            assert_eq!(
                state.services[&key].phase, phase,
                "native cleanup needs the exact ended-phase proof"
            );
            assert_eq!(
                Control::validate_ended_continuation(&state, &continuation_slice(key, requester)),
                Ok(())
            );
            assert_eq!(state.waits[&ThreadKey::Main], original);
            assert_eq!(state.waits[&requester], Wait::Exited);
            assert_eq!(
                state.physical[&ThreadKey::Main],
                OwnerLocation::NativeDriving
            );
            assert!(state.ready.is_empty());
        }
        let mut unknown = ControlState::default();
        unknown.services.insert(
            key,
            ServiceMetadata {
                requester,
                phase: ServicePhase::Continuing(ContinuationEligibility::Released),
            },
        );
        assert_eq!(
            Control::pending_native(&unknown),
            NativeWork::Idle,
            "an absent owner record is not an explicit exit proof"
        );
    }

    #[test]
    fn ended_owner_queued_resume_keeps_phase_and_other_service_reservation() {
        let ended = ThreadKey::Worker(NonZeroU64::MIN);
        let live = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let key = host_key();
        let sibling = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let phase = ServicePhase::Resuming(ContinuationEligibility::Released);
        let original = Wait::Join(live);
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, original);
        Control::set_wait(&mut state, ended, Wait::Exited);
        Control::set_wait(&mut state, live, Wait::HostRequest(sibling));
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.services.insert(
            key,
            ServiceMetadata {
                requester: ended,
                phase,
            },
        );
        state.services.insert(
            sibling,
            ServiceMetadata {
                requester: live,
                phase: ServicePhase::Reserved,
            },
        );
        state.pending_services = VecDeque::from([key, sibling]);
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Pump),
            "an ended resume must never select a requester ContinuationTurn"
        );
        assert_eq!(state.services[&key].phase, phase);
        assert_eq!(
            Control::validate_ended_continuation(&state, &continuation_slice(key, ended)),
            Ok(())
        );
        assert_eq!(state.services[&sibling].phase, ServicePhase::Reserved);
        assert_eq!(state.pending_services, VecDeque::from([sibling]));
        assert_eq!(state.waits[&ThreadKey::Main], original);
        assert_eq!(state.waits[&live], Wait::HostRequest(sibling));
        assert!(state.ready.is_empty());
        // This private metadata transition is tested independently of the
        // production settlement mint; it cannot acknowledge or retire roots.
        assert_eq!(
            Control::finish_ended_continuation_metadata(
                &mut state,
                &continuation_slice(key, ended)
            ),
            Ok(())
        );
        assert!(!state.services.contains_key(&key));
        assert_eq!(state.services[&sibling].phase, ServicePhase::Reserved);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Service(sibling))
        );
        assert_eq!(state.waits[&ThreadKey::Main], original);
    }

    #[test]
    fn ended_owner_cleanup_precedes_unrelated_real_wait_main_eligibility() {
        let ended = ThreadKey::Worker(NonZeroU64::MIN);
        let other = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let key = host_key();
        let unrelated = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        for phase in [
            ServicePhase::Continuing(ContinuationEligibility::Reserved),
            ServicePhase::Continuing(ContinuationEligibility::Released),
            ServicePhase::Resuming(ContinuationEligibility::Reserved),
            ServicePhase::Resuming(ContinuationEligibility::Released),
        ] {
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
            Control::set_wait(&mut state, ended, Wait::Exited);
            Control::set_wait(&mut state, other, Wait::HostRequest(unrelated));
            Control::enqueue_ready(&mut state, ThreadKey::Main);
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ended,
                    phase,
                },
            );
            state.services.insert(
                unrelated,
                ServiceMetadata {
                    requester: other,
                    phase: ServicePhase::RealWait,
                },
            );
            assert!(matches!(
                Control::choose_native_metadata(&mut state, NativePumpPurpose::Serving),
                NativePumpChoice::Native(NativeTurn::Pump)
            ));
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(state.services[&unrelated].phase, ServicePhase::RealWait);
            assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
            assert_eq!(
                state.ready.front(),
                Some(&ThreadKey::Main),
                "cleanup cannot consume main's ordinary FIFO eligibility"
            );
            Control::finish_ended_continuation_metadata(
                &mut state,
                &continuation_slice(key, ended),
            )
            .unwrap();
            assert!(
                matches!(
                    Control::choose_native_metadata(&mut state, NativePumpPurpose::Serving),
                    NativePumpChoice::OriginalMain
                ),
                "the unrelated real wait keeps its original eligibility after exact cleanup"
            );
            assert!(state.ready.is_empty());
        }
    }

    #[test]
    fn settled_last_ended_continuation_returns_main_its_existing_ready_turn() {
        let ended = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        for purpose in [NativePumpPurpose::Serving, NativePumpPurpose::Draining] {
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
            Control::enqueue_ready(&mut state, ThreadKey::Main);
            Control::set_wait(&mut state, ended, Wait::Exited);
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ended,
                    phase: ServicePhase::Continuing(ContinuationEligibility::Reserved),
                },
            );
            Control::remove_key(&mut state, ended);
            assert!(matches!(
                Control::choose_native_metadata(&mut state, purpose),
                NativePumpChoice::Native(NativeTurn::Pump)
            ));
            assert_eq!(state.ready.front(), Some(&ThreadKey::Main));
            // This tests only the scheduler transition, not a manufactured actor
            // receipt. Production removes it through finish_ended_continuation.
            Control::finish_ended_continuation_metadata(
                &mut state,
                &continuation_slice(key, ended),
            )
            .unwrap();
            assert!(matches!(
                Control::choose_native_metadata(&mut state, purpose),
                NativePumpChoice::OriginalMain
            ));
            assert!(state.services.is_empty());
            assert!(state.ready.is_empty());
            assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
            assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Running);
            assert!(!state.waits.contains_key(&ended));
        }
    }

    #[test]
    fn ended_owner_continuation_exit_notifies_registered_native_source() {
        let _harness = crate::test_utils::unit_harness();
        use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWakeOutcome};
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let manager = crate::emacs_core::eval::data::process_state::ProcessManager::new();
        let control = Control::default();
        let _main = control.register_main();
        let _worker = control.register_worker(worker);
        control.install_select(
            crate::emacs_core::eval::process_hooks::select::register_manager(&manager),
        );
        let key = host_key();
        let phase = ServicePhase::Continuing(ContinuationEligibility::Released);
        {
            let mut state = control.state();
            Control::set_wait(&mut state, ThreadKey::Main, Wait::Join(worker));
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: worker,
                    phase,
                },
            );
            assert_eq!(Control::pending_native(&state), NativeWork::Idle);
        }
        let source = control.native_pump_source().unwrap();
        let armed = source.arm(ControlDestination::NativeMain).unwrap();
        assert_eq!(armed.poll(), None);
        control.publish_owner_exit(worker);
        assert_eq!(
            armed.poll(),
            Some(ControlWakeOutcome::ControlReady(
                ControlNudge::SchedulerReady
            ))
        );
        let state = control.state();
        assert_eq!(
            Control::pending_native(&state),
            NativeWork::Ready,
            "the exit fact must be durable before native readiness is observed"
        );
        assert_eq!(state.services[&key].phase, phase);
        assert_eq!(state.waits[&worker], Wait::Exited);
        assert_eq!(
            state.physical[&ThreadKey::Main],
            OwnerLocation::NativeReleased,
            "the notification cannot fabricate physical reacquisition"
        );
    }

    #[test]
    fn intermediate_return_retains_parent_and_preserves_main_wait_and_fifo() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let waiting_for = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let queued = ThreadKey::Worker(NonZeroU64::new(3).unwrap());
        let key = host_key();
        let slice = continuation_slice(key, requester);
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Join(waiting_for));
        Control::set_wait(&mut state, requester, Wait::HostRequest(key));
        Control::set_wait(&mut state, queued, Wait::Runnable);
        Control::enqueue_ready(&mut state, queued);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Serving(key));
        state.services.insert(
            key,
            ServiceMetadata {
                requester,
                phase: ServicePhase::Serving,
            },
        );
        assert_eq!(Control::intermediate_metadata(&mut state, &slice), Ok(()));
        assert_eq!(
            state.services[&key].phase,
            ServicePhase::Continuing(ContinuationEligibility::Reserved)
        );
        assert_eq!(state.waits[&ThreadKey::Main], Wait::Join(waiting_for));
        assert_eq!(state.waits[&requester], Wait::Runnable);
        assert_eq!(state.physical[&requester], OwnerLocation::Running);
        assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Receiving);
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![queued]
        );
        assert!(!Control::service_ready(&state));
        assert_eq!(Control::pending_native(&state), NativeWork::Idle);
    }

    #[test]
    fn continuing_release_marks_nested_ancestry_but_not_another_owner() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let other = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let parent = host_key();
        let child = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let foreign = HostRequestKey::for_test(NonZeroU64::new(3).unwrap());
        let mut state = ControlState::default();
        for (key, owner) in [(parent, requester), (child, requester), (foreign, other)] {
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: owner,
                    phase: ServicePhase::Continuing(ContinuationEligibility::Reserved),
                },
            );
        }
        Control::note_continuation_release(&mut state, requester);
        for key in [parent, child] {
            assert_eq!(
                state.services[&key].phase,
                ServicePhase::Continuing(ContinuationEligibility::Released)
            );
        }
        assert_eq!(
            state.services[&foreign].phase,
            ServicePhase::Continuing(ContinuationEligibility::Reserved)
        );
        assert_eq!(Control::pending_native(&state), NativeWork::Idle);
    }

    #[test]
    fn continuation_resume_retains_real_release_across_close_and_queued_native_turn() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        let slice = continuation_slice(key, requester);
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Join(requester));
        Control::set_wait(&mut state, requester, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.physical.insert(requester, OwnerLocation::Running);
        state.services.insert(
            key,
            ServiceMetadata {
                requester,
                phase: ServicePhase::Continuing(ContinuationEligibility::Released),
            },
        );
        state.closing = true;
        assert_eq!(Control::resume_owner_metadata(&mut state, &slice), Ok(()));
        state.pending_services.push_back(key);
        assert_eq!(
            state.services[&key].phase,
            ServicePhase::Resuming(ContinuationEligibility::Released)
        );
        assert_eq!(state.waits[&requester], Wait::HostRequest(key));
        assert!(state.ready.is_empty());
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Continuation(key))
        );
        assert_eq!(state.services[&key].phase, ServicePhase::ServingAfterWait);
        assert_eq!(state.waits[&ThreadKey::Main], Wait::Join(requester));
    }

    #[test]
    fn prepared_local_transfer_preserves_main_wait_and_ordinary_fifo() {
        let key = host_key();
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        state.main_os_thread = Some(std::thread::current().id());
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        Control::set_wait(&mut state, worker, Wait::Runnable);
        Control::enqueue_ready(&mut state, worker);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
        state.services.insert(
            key,
            ServiceMetadata {
                requester: ThreadKey::Main,
                phase: ServicePhase::Reserved,
            },
        );
        assert_eq!(
            Control::transfer_local_metadata(
                &mut state,
                key,
                ThreadKey::Main,
                std::thread::current().id()
            ),
            Ok(())
        );
        assert_eq!(state.services[&key].phase, ServicePhase::Serving);
        assert_eq!(
            state.physical[&ThreadKey::Main],
            OwnerLocation::Serving(key)
        );
        assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![worker]
        );
    }

    #[test]
    fn local_intermediate_round_trip_never_changes_main_wait_or_enqueues_it() {
        let key = host_key();
        let slice = continuation_slice(key, ThreadKey::Main);
        for phase in [ServicePhase::Serving, ServicePhase::ServingAfterWait] {
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Serving(key));
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ThreadKey::Main,
                    phase,
                },
            );
            assert_eq!(Control::intermediate_metadata(&mut state, &slice), Ok(()));
            assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
            assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Running);
            assert!(state.ready.is_empty());
            assert_eq!(Control::resume_local_metadata(&mut state, &slice), Ok(()));
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(
                state.physical[&ThreadKey::Main],
                OwnerLocation::Serving(key)
            );
            assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
            assert!(state.ready.is_empty());
        }
    }

    #[test]
    fn pending_local_native_request_cannot_resume_unrelated_main_before_done() {
        let key = host_key();
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        state.services.insert(
            key,
            ServiceMetadata {
                requester: ThreadKey::Main,
                phase: ServicePhase::RealWait,
            },
        );
        assert!(matches!(
            Control::choose_native_metadata(&mut state, NativePumpPurpose::Serving),
            NativePumpChoice::Native(NativeTurn::Pump)
        ));
        assert_eq!(state.ready.front(), Some(&ThreadKey::Main));
        assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
    }

    #[test]
    fn ended_continuation_cleanup_refuses_live_owner_and_keeps_nested_parent() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let key = host_key();
        let parent = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let slice = continuation_slice(key, requester);
        let mut state = ControlState::default();
        Control::set_wait(&mut state, requester, Wait::Runnable);
        for key in [key, parent] {
            state.services.insert(
                key,
                ServiceMetadata {
                    requester,
                    phase: ServicePhase::Continuing(ContinuationEligibility::Released),
                },
            );
        }
        assert_eq!(
            Control::finish_ended_continuation_metadata(&mut state, &slice),
            Err(ServiceRefusal::WrongServiceState)
        );
        assert!(state.services.contains_key(&key));
        Control::set_wait(&mut state, requester, Wait::Exited);
        assert_eq!(
            Control::finish_ended_continuation_metadata(&mut state, &slice),
            Ok(())
        );
        assert!(!state.services.contains_key(&key));
        assert!(state.services.contains_key(&parent));
        assert_eq!(state.waits[&requester], Wait::Exited);
    }

    #[test]
    fn wrong_intermediate_slice_does_not_consume_the_rightful_reservation() {
        let key = host_key();
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let foreign = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let mut state = ControlState::default();
        Control::set_wait(&mut state, foreign, Wait::HostRequest(key));
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Serving(key));
        state.services.insert(
            key,
            ServiceMetadata {
                requester,
                phase: ServicePhase::Serving,
            },
        );
        let wrong = continuation_slice(key, foreign);
        assert_eq!(
            Control::intermediate_metadata(&mut state, &wrong),
            Err(ServiceRefusal::WrongServiceState)
        );
        assert_eq!(state.services[&key].phase, ServicePhase::Serving);
        assert_eq!(
            state.physical[&ThreadKey::Main],
            OwnerLocation::Serving(key)
        );
        assert_eq!(state.waits[&foreign], Wait::HostRequest(key));
    }

    #[test]
    fn in_flight_resume_is_intercepted_before_worker_restore() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let later = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let original_main_wait = Wait::Join(worker);
        let mut state = ControlState::default();
        state.waits.insert(ThreadKey::Main, original_main_wait);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.waits.insert(worker, Wait::Runnable);
        state.physical.insert(worker, OwnerLocation::Running);
        state.waits.insert(later, Wait::Runnable);
        state.ready.push_back(later);
        state.services.insert(
            host_key(),
            ServiceMetadata {
                requester: later,
                phase: ServicePhase::Reacquiring,
            },
        );
        state
            .pending_acquires
            .push_back(NativeAcquire::Service(host_key()));
        assert_eq!(
            Control::intercept_ready(&mut state, worker),
            Some(NativeTurn::Service(host_key()))
        );
        assert_eq!(state.physical.get(&worker), Some(&OwnerLocation::Receiving));
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![worker, later]
        );
        assert_eq!(state.waits.get(&ThreadKey::Main), Some(&original_main_wait));
    }

    #[test]
    fn ordinary_acquire_without_a_service_does_not_enqueue_itself() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        state.waits.insert(worker, Wait::Runnable);
        state.physical.insert(worker, OwnerLocation::Receiving);
        assert_eq!(Control::intercept_ready(&mut state, worker), None);
        assert_eq!(state.physical.get(&worker), Some(&OwnerLocation::Running));
        assert!(state.ready.is_empty());
    }

    #[test]
    fn native_completion_acquire_precedes_fifo_without_completing_main_wait() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let unrelated = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let original = Wait::Join(requester);
        let mut state = ControlState::default();
        state.waits.insert(ThreadKey::Main, original);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.waits.insert(unrelated, Wait::Runnable);
        state.ready.push_back(unrelated);
        state.services.insert(
            host_key(),
            ServiceMetadata {
                requester,
                phase: ServicePhase::Reacquiring,
            },
        );
        state
            .pending_acquires
            .push_back(NativeAcquire::Service(host_key()));
        assert!(Control::service_ready(&state));
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Service(host_key()))
        );
        assert_eq!(state.waits.get(&ThreadKey::Main), Some(&original));
        assert_eq!(state.ready.front(), Some(&unrelated));
        assert_eq!(
            state
                .services
                .get(&host_key())
                .map(|metadata| metadata.phase),
            Some(ServicePhase::ServingAfterWait)
        );
    }

    #[test]
    fn suspended_native_main_retains_its_exact_fifo_place() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let later = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let mut state = ControlState::default();
        for key in [ThreadKey::Main, worker, later] {
            state.waits.insert(key, Wait::Runnable);
            state.ready.push_back(key);
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        assert_eq!(Control::pick(&mut state), Some((worker, None)));
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![ThreadKey::Main, later]
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(Control::pick(&mut state), Some((ThreadKey::Main, None)));
    }

    #[test]
    fn native_reacquire_precedes_a_later_reserved_host_request() {
        let key = host_key();
        let later = HostRequestKey::for_test(NonZeroU64::new(2).expect("nonzero"));
        let mut state = ControlState::default();
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        for (key, phase) in [
            (key, ServicePhase::Reacquiring),
            (later, ServicePhase::Reserved),
        ] {
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ThreadKey::Worker(NonZeroU64::MIN),
                    phase,
                },
            );
        }
        state
            .pending_acquires
            .push_back(NativeAcquire::Service(key));
        state.pending_services.push_back(later);
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Service(key))
        );
        assert_eq!(state.pending_services.front(), Some(&later));
        assert_eq!(
            Control::pick_service(&mut state),
            None,
            "one native main service at a time"
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Service(later))
        );
    }

    #[test]
    fn native_pump_serves_b_without_completing_pending_a_or_main_wait() {
        let a = host_key();
        let b = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let owner_a = ThreadKey::Worker(NonZeroU64::MIN);
        let owner_b = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let original_wait = Wait::Join(owner_a);
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, original_wait);
        Control::set_wait(&mut state, owner_a, Wait::HostRequest(a));
        Control::set_wait(&mut state, owner_b, Wait::HostRequest(b));
        for (key, requester, phase) in [
            (a, owner_a, ServicePhase::RealWait),
            (b, owner_b, ServicePhase::Reserved),
        ] {
            state
                .services
                .insert(key, ServiceMetadata { requester, phase });
        }
        state.pending_services.push_back(b);
        state.pending_acquires.push_back(NativeAcquire::Pump);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(Control::pick_service(&mut state), Some(NativeTurn::Pump));
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert!(
            matches!(Control::choose_native_metadata(&mut state, NativePumpPurpose::Serving), NativePumpChoice::Native(NativeTurn::Service(key)) if key == b)
        );
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert_eq!(state.services[&b].phase, ServicePhase::Serving);
        assert_eq!(state.waits[&ThreadKey::Main], original_wait);

        let slice_b = ReservedServiceSlice {
            key: b,
            requester: owner_b,
            _confined: PhantomData,
        };
        assert_eq!(Control::defer_native_metadata(&mut state, &slice_b), Ok(()));
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert_eq!(state.services[&b].phase, ServicePhase::PendingNative);
        let slice_a = ReservedServiceSlice {
            key: a,
            requester: owner_a,
            _confined: PhantomData,
        };
        assert_eq!(
            Control::enter_native_completion_metadata(&mut state, &slice_a),
            Ok(())
        );
        assert_eq!(state.services[&a].phase, ServicePhase::ServingAfterWait);
        assert_eq!(state.services[&b].phase, ServicePhase::PendingNative);
        assert_eq!(state.waits[&ThreadKey::Main], original_wait);
        assert!(
            state.ready.is_empty(),
            "native hops cannot enqueue a Lisp continuation"
        );
    }

    #[test]
    fn pending_native_record_keeps_no_runnable_scheduler_in_the_driver() {
        let key = host_key();
        let owner = ThreadKey::Worker(NonZeroU64::MIN);
        let original_wait = Wait::Join(owner);
        for phase in [ServicePhase::PendingNative, ServicePhase::RealWait] {
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, original_wait);
            Control::set_wait(&mut state, owner, Wait::HostRequest(key));
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: owner,
                    phase,
                },
            );
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Receiving);
            assert_eq!(Control::pick(&mut state), None);
            assert_eq!(Control::queued_native_work(&state), NativeWork::Idle);
            assert!(!Control::service_ready(&state));
            assert_eq!(Control::pick_service(&mut state), Some(NativeTurn::Pump));
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(
                state.physical[&ThreadKey::Main],
                OwnerLocation::NativeDriving
            );
            assert_eq!(state.waits[&ThreadKey::Main], original_wait);
            assert!(state.ready.is_empty());
        }
    }

    #[test]
    fn stale_native_pump_acquire_does_not_revive_a_drained_record() {
        let owner = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        let original = Wait::Join(owner);
        Control::set_wait(&mut state, ThreadKey::Main, original);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.pending_acquires.push_back(NativeAcquire::Pump);
        assert_eq!(Control::queued_native_work(&state), NativeWork::Idle);
        assert!(!Control::service_ready(&state));
        assert_eq!(Control::pick_service(&mut state), None);
        assert!(state.pending_acquires.is_empty());
        assert_eq!(state.waits[&ThreadKey::Main], original);
        assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Receiving);
    }

    #[test]
    fn pending_only_driver_cannot_intercept_a_reserved_requester_return() {
        let owner_a = ThreadKey::Worker(NonZeroU64::MIN);
        let returning_b = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Join(owner_a));
        Control::set_wait(&mut state, returning_b, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.physical.insert(returning_b, OwnerLocation::Receiving);
        state.services.insert(
            host_key(),
            ServiceMetadata {
                requester: owner_a,
                phase: ServicePhase::RealWait,
            },
        );
        assert!(!Control::service_ready(&state));
        assert_eq!(Control::intercept_ready(&mut state, returning_b), None);
        assert_eq!(state.physical[&returning_b], OwnerLocation::Running);
        assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Receiving);
        assert_eq!(state.services[&host_key()].phase, ServicePhase::RealWait);
        assert!(state.ready.is_empty());
    }

    #[test]
    fn a_new_pending_native_reservation_cannot_resume_main_on_an_older_release() {
        let a = host_key();
        let b = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        for (key, phase) in [
            (a, ServicePhase::RealWait),
            (b, ServicePhase::PendingNative),
        ] {
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ThreadKey::Worker(NonZeroU64::MIN),
                    phase,
                },
            );
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        assert!(matches!(
            Control::choose_native_metadata(&mut state, NativePumpPurpose::Serving),
            NativePumpChoice::Native(NativeTurn::Pump)
        ));
        assert_eq!(state.ready.front(), Some(&ThreadKey::Main));
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert_eq!(state.services[&b].phase, ServicePhase::PendingNative);
    }

    #[test]
    fn explicit_native_drain_does_not_resume_main_before_settlement() {
        for purpose in [NativePumpPurpose::Serving, NativePumpPurpose::Draining] {
            let key = host_key();
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
            Control::enqueue_ready(&mut state, ThreadKey::Main);
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ThreadKey::Worker(NonZeroU64::MIN),
                    phase: ServicePhase::RealWait,
                },
            );
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
            let choice = Control::choose_native_metadata(&mut state, purpose);
            match purpose {
                NativePumpPurpose::Serving => {
                    assert!(matches!(choice, NativePumpChoice::OriginalMain));
                    assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Running);
                    assert!(state.ready.is_empty());
                }
                NativePumpPurpose::Draining => {
                    assert!(matches!(choice, NativePumpChoice::Native(NativeTurn::Pump)));
                    assert_eq!(
                        state.physical[&ThreadKey::Main],
                        OwnerLocation::NativeDriving
                    );
                    assert_eq!(state.ready.front(), Some(&ThreadKey::Main));
                }
            }
            assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
            assert_eq!(state.services[&key].phase, ServicePhase::RealWait);
        }
    }

    #[test]
    fn pending_native_completion_refuses_another_owner_without_changing_any_phase() {
        let key = host_key();
        let owner = ThreadKey::Worker(NonZeroU64::MIN);
        let other = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let mut state = ControlState::default();
        state.services.insert(
            key,
            ServiceMetadata {
                requester: owner,
                phase: ServicePhase::RealWait,
            },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        let wrong = ReservedServiceSlice {
            key,
            requester: other,
            _confined: PhantomData,
        };
        assert_eq!(
            Control::enter_native_completion_metadata(&mut state, &wrong),
            Err(ServiceRefusal::WrongServiceState)
        );
        assert_eq!(state.services[&key].phase, ServicePhase::RealWait);
        assert_eq!(
            state.physical[&ThreadKey::Main],
            OwnerLocation::NativeDriving
        );
    }

    #[test]
    fn ordinary_wakeup_cannot_complete_a_host_request() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        state.waits.insert(requester, Wait::HostRequest(host_key()));
        Control::wake_locked(&mut state, requester);
        assert_eq!(
            state.waits.get(&requester),
            Some(&Wait::HostRequest(host_key()))
        );
        assert!(state.ready.is_empty());
    }

    #[test]
    fn signal_priority_cannot_turn_a_host_park_into_a_lisp_resume() {
        let control = Control::default();
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        control
            .state()
            .waits
            .insert(requester, Wait::HostRequest(host_key()));
        control.prioritize(requester);
        assert_eq!(
            control.wait_of(requester),
            Some(Wait::HostRequest(host_key()))
        );
        assert!(control.state().ready.is_empty());
    }

    #[test]
    fn host_wait_has_no_clock_or_fifo_readiness() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        state.waits.insert(requester, Wait::HostRequest(host_key()));
        // A stale FIFO entry is not a completion certificate.
        state.ready.push_back(requester);
        assert_eq!(Control::pick(&mut state), None);
        assert_eq!(
            state.waits.get(&requester),
            Some(&Wait::HostRequest(host_key()))
        );
    }

    #[test]
    fn pending_native_owner_exit_publishes_readiness_before_native_reacquire() {
        let _harness = crate::test_utils::unit_harness();
        use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWakeOutcome};
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        for original in [Wait::Runnable, Wait::Join(worker)] {
            let manager = crate::emacs_core::eval::data::process_state::ProcessManager::new();
            let control = Control::default();
            let _main = control.register_main();
            let _worker = control.register_worker(worker);
            control.install_select(
                crate::emacs_core::eval::process_hooks::select::register_manager(&manager),
            );
            {
                let mut state = control.state();
                Control::set_wait(&mut state, ThreadKey::Main, original);
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
            }
            let source = control.native_pump_source().unwrap();
            let armed = source.arm(ControlDestination::NativeMain).unwrap();
            assert_eq!(armed.poll(), None);
            control.publish_owner_exit(worker);
            assert_eq!(control.wait_of(worker), Some(Wait::Exited));
            assert_eq!(control.wait_of(ThreadKey::Main), Some(Wait::Runnable));
            assert_eq!(
                armed.poll(),
                Some(ControlWakeOutcome::ControlReady(
                    ControlNudge::SchedulerReady
                )),
                "worker exit must wake main before waiting for its native acquire"
            );
            assert_eq!(
                control.state().physical.get(&ThreadKey::Main),
                Some(&OwnerLocation::NativeReleased),
                "a wake hint does not fabricate native reacquisition"
            );
        }
    }

    #[test]
    fn native_release_preserves_the_semantic_wait_and_requires_the_running_main() {
        let control = Control::default();
        let _receiver = control.register_main();
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let original = Wait::Join(requester);
        control.state().waits.insert(ThreadKey::Main, original);
        assert_eq!(control.main_native_released(), Ok(()));
        assert_eq!(control.wait_of(ThreadKey::Main), Some(original));
        assert_eq!(
            control.main_native_released(),
            Err(ServiceRefusal::WrongServiceState)
        );
        assert!(control.state().ready.is_empty());
    }

    #[test]
    fn native_release_cannot_be_announced_from_a_worker_os_thread() {
        let control = std::sync::Arc::new(Control::default());
        let _receiver = control.register_main();
        let worker_control = std::sync::Arc::clone(&control);
        let refusal = std::thread::spawn(move || worker_control.main_native_released())
            .join()
            .expect("worker finished");
        assert_eq!(refusal, Err(ServiceRefusal::WrongMainThread));
        assert_eq!(
            control.state().physical.get(&ThreadKey::Main),
            Some(&OwnerLocation::Running)
        );
    }

    #[test]
    fn native_main_readiness_retains_one_fifo_turn_until_physical_acquire() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        let deadline = Instant::now();
        Control::set_wait(
            &mut state,
            ThreadKey::Main,
            Wait::Select { wait_end: deadline },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Control::expire_deadlines(&mut state, deadline);
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            None
        );
        Control::set_wait(&mut state, worker, Wait::Runnable);
        Control::enqueue_ready(&mut state, worker);
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            Some(ThreadKey::Main)
        );
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            Some(worker)
        );
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            None
        );
    }

    #[test]
    fn idle_main_acquire_consumes_its_woken_turn_before_the_next_yield() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        Control::set_wait(
            &mut state,
            ThreadKey::Main,
            Wait::Select {
                wait_end: Instant::now() + std::time::Duration::from_secs(30),
            },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Control::wake_locked(&mut state, ThreadKey::Main);
        Control::finish_idle_main_acquire(&mut state);
        assert_eq!(Control::intercept_ready(&mut state, ThreadKey::Main), None);

        Control::set_wait(&mut state, worker, Wait::Runnable);
        Control::enqueue_ready(&mut state, worker);
        // The metadata prelude of main's next Runnable transfer.
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        assert_eq!(Control::pick(&mut state), Some((worker, None)));
        assert_eq!(Control::pick(&mut state), Some((ThreadKey::Main, None)));
        assert_eq!(Control::pick(&mut state), None);
    }

    #[test]
    fn idle_main_acquire_does_not_consume_an_earlier_peer_turn() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        Control::set_wait(
            &mut state,
            ThreadKey::Main,
            Wait::Select {
                wait_end: Instant::now() + std::time::Duration::from_secs(30),
            },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Control::set_wait(&mut state, worker, Wait::Runnable);
        Control::enqueue_ready(&mut state, worker);
        Control::wake_locked(&mut state, ThreadKey::Main);
        Control::finish_idle_main_acquire(&mut state);
        assert_eq!(Control::intercept_ready(&mut state, ThreadKey::Main), None);
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![worker]
        );

        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        assert_eq!(Control::pick(&mut state), Some((worker, None)));
        assert_eq!(Control::pick(&mut state), Some((ThreadKey::Main, None)));
        assert_eq!(Control::pick(&mut state), None);
    }

    #[test]
    fn idle_native_wait_must_use_earlier_peer_select_deadline() {
        let control = Control::default();
        let _main_receiver = control.register_main();
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let _worker_receiver = control.register_worker(worker);
        let now = Instant::now();
        let peer_deadline = now + std::time::Duration::from_secs(1);
        let main_deadline = now + std::time::Duration::from_secs(30);
        {
            let mut state = control.state();
            Control::set_wait(
                &mut state,
                worker,
                Wait::Select {
                    wait_end: peer_deadline,
                },
            );
        }
        assert_eq!(
            control.peers(ThreadKey::Main, now),
            Peers::Sleeping(peer_deadline),
            "the executor must cap its physical wait while retaining main's semantic deadline"
        );
        assert!(peer_deadline < main_deadline);
    }

    #[test]
    fn select_deadline_becomes_ready_at_its_exact_fine_instant() {
        let now = Instant::now();
        let deadline = now + std::time::Duration::from_millis(50);
        let mut state = ControlState::default();
        state
            .waits
            .insert(ThreadKey::Main, Wait::Select { wait_end: deadline });
        Control::expire_deadlines(&mut state, deadline - std::time::Duration::from_nanos(1));
        assert_eq!(
            state.waits.get(&ThreadKey::Main),
            Some(&Wait::Select { wait_end: deadline })
        );
        assert!(state.ready.is_empty());
        Control::expire_deadlines(&mut state, deadline);
        assert_eq!(state.waits.get(&ThreadKey::Main), Some(&Wait::Runnable));
        assert_eq!(state.ready.pop_front(), Some(ThreadKey::Main));
    }

    /// Several deadlines expired by one coarse host wakeup admit in
    /// DEADLINE order, not `ThreadKey` order: `waits` is a BTreeMap and
    /// `Main` orders before every worker, so map order resumed Main's
    /// later `sleep-for` before a peer whose earlier timer had already
    /// passed (macOS arm64, CI run 41283917816: the green release-point
    /// tests lost the redefinition that way). `pick` runs the earliest
    /// deadline while exactly one is expired; expiry must not invert
    /// that order when several expire together.
    #[test]
    fn expired_select_waits_admit_in_deadline_order_not_key_order() {
        let now = Instant::now();
        let early = now + std::time::Duration::from_micros(5);
        let middle = now + std::time::Duration::from_micros(20);
        let late = now + std::time::Duration::from_micros(30);
        // Key order is Main, Worker(2), Worker(5); deadline order is the
        // opposite, so a key-ordered expiry and a deadline-ordered one
        // differ at every position.
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
        Control::expire_deadlines(&mut state, late);
        assert_eq!(
            state.ready.pop_front(),
            Some(worker5),
            "the earliest deadline admits first"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(worker2),
            "the middle deadline admits second"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(ThreadKey::Main),
            "the latest deadline admits last, key order notwithstanding"
        );
        assert!(state.ready.is_empty());
    }

    /// The readiness path admits expired timers in deadline order too.
    /// `wake_selects` runs before `pick` both in the next-owner scan and
    /// after a physical wait with activity, so fd/loopback readiness
    /// arriving while several deadlines have already passed must not let
    /// key order (Main first) resume the later deadline first: the
    /// readiness twin of the timer regression above.
    #[test]
    fn readiness_wake_admits_expired_selects_in_deadline_order() {
        // `wake_selects` samples its own `Instant::now()`, so the expired
        // three are PAST offsets — however many microseconds the prologue
        // takes, every sample expires them, and only the order can vary.
        // (Future microsecond offsets made the test racy on slower hosts:
        // a sample between `early` and `middle` expired one deadline and
        // let key order in behind it.)
        let now = Instant::now();
        let early = now - std::time::Duration::from_micros(30);
        let middle = now - std::time::Duration::from_micros(20);
        let late = now - std::time::Duration::from_micros(5);
        let unexpired = now + std::time::Duration::from_secs(3600);
        // Key order is Main, Worker(2), Worker(5), Worker(7); deadline
        // order is the opposite for the expired three, and Worker(7) has
        // not expired at all.
        let worker2 = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let worker5 = ThreadKey::Worker(NonZeroU64::new(5).expect("nonzero"));
        let worker7 = ThreadKey::Worker(NonZeroU64::new(7).expect("nonzero"));
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
        state.waits.insert(
            worker7,
            Wait::Select {
                wait_end: unexpired,
            },
        );
        Control::wake_selects(&mut state);
        assert_eq!(
            state.ready.pop_front(),
            Some(worker5),
            "the earliest expired deadline admits first"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(worker2),
            "the middle expired deadline admits second"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(ThreadKey::Main),
            "the latest expired deadline admits after the earlier ones"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(worker7),
            "the unexpired selector admits last, key order among the waiting"
        );
        assert!(state.ready.is_empty());
    }

    #[test]
    fn a_retired_worker_is_exited_and_leaves_no_record_or_turn() {
        let control = Control::default();
        let _main_receiver = control.register_main();
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let _worker_receiver = control.register_worker(worker);
        {
            let mut state = control.state();
            Control::set_wait(&mut state, worker, Wait::Exited);
            Control::remove_key(&mut state, worker);
        }
        assert!(control.exited(worker), "absence keeps the ended fact typed");
        assert!(!control.worker_registered_for_test(worker));
        assert_eq!(
            control.registry_sizes_for_test(),
            (1, 1),
            "only main remains charged after retirement"
        );
        assert_eq!(
            Control::pick(&mut control.state()),
            None,
            "the retired worker's stale Runnable turn must not be selected"
        );
        assert!(
            control.state().ready.is_empty(),
            "pop_ready dropped the retired worker's stale turn"
        );
    }

    #[test]
    fn turn_right_era_accepts_matching_heap_and_rejects_foreign_heap() {
        use crate::emacs_core::eval::parallel_entry::keeper_driver::{KeeperMsg, WorldLend};
        use crate::emacs_core::eval::parallel_entry::world_turn::{
            WorldReturned, WorldTurn, WorldTurnIdentity,
        };
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let foreign_heap = {
            let other = Context::new_with_threading(
                ThreadingMode::Green,
                crate::test_utils::unit_runtime(),
            );
            other.tagged_heap.heap_identity()
        };
        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = owner.start_green_runtime().unwrap();
        let expected_heap = baton.parallel_heap_identity();
        let world_handle = baton.era_world_handle().unwrap();
        assert_ne!(expected_heap, foreign_heap);

        let (tx, rx) = mpsc::channel();
        let foreign_turn = WorldTurn::cut_for_test(WorldTurnIdentity {
            heap: foreign_heap,
            epoch: NonZeroU64::MIN,
        });
        let foreign_lend = WorldLend::for_test(tx.clone());
        let (returned_turn, returned_lend, world_handle) =
            TurnRight::from_era(foreign_turn, foreign_lend, world_handle)
                .expect_err("A-3: foreign heap WorldTurn must be refused");
        // Disarm the returned lend cleanly (no GroupAbandon sent).
        returned_lend.give_back(WorldReturned::from_end(returned_turn));
        assert!(matches!(rx.try_recv(), Ok(KeeperMsg::WorldReturn { .. })));

        let matching_turn = WorldTurn::cut_for_test(WorldTurnIdentity {
            heap: expected_heap,
            epoch: NonZeroU64::MIN,
        });
        let matching_lend = WorldLend::for_test(tx);
        let right = TurnRight::from_era(matching_turn, matching_lend, world_handle)
            .expect("matching heap constructs TurnRight::Era");
        assert!(right.is_era());
        assert!(!right.is_solo());
        right.enter();
        assert!(right.held_here());
        assert_eq!(right.parallel_heap_identity(), expected_heap);
        assert!(right.green_runtime().is_some());

        let (turn, lend, _world_handle) = right
            .into_era()
            .expect("TurnRight::Era decomposes into parts")
            .into_parts();
        lend.give_back(WorldReturned::from_end(turn));
        assert!(matches!(rx.try_recv(), Ok(KeeperMsg::WorldReturn { .. })));
    }

    fn test_group_port() -> (
        GroupPort,
        mpsc::Receiver<crate::emacs_core::eval::parallel_entry::keeper_driver::KeeperMsg>,
        mpsc::SyncSender<crate::emacs_core::eval::parallel_entry::keeper_driver::GroupMsg>,
    ) {
        let (inbox_tx, inbox_rx) = mpsc::channel();
        let (world_tx, world_rx) = mpsc::sync_channel(1);
        let waiter = Arc::new(RingWaiter::new_for_test());
        (
            GroupPort::from_raw_parts_for_test(inbox_tx, world_rx, waiter),
            inbox_rx,
            world_tx,
        )
    }

    #[test]
    fn qd1_opening_era_while_green_holds_solo_is_refused() {
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = owner.start_green_runtime().unwrap();
        let world_handle = baton.era_world_handle().unwrap();
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let _main_rx = control.register_main();
        let (port, _inbox_rx, _world_tx) = test_group_port();

        // Main still holds TurnRight::Solo(baton): opening an era must be refused.
        let (world_handle, port, err) = control
            .open_era(world_handle, port)
            .expect_err("QD1: opening an era while Green holds Solo must be refused");
        assert_eq!(err, EraOpenError::SoloBatonHeld);

        // Linearly surrender Solo(baton) out of Green, then opening an era succeeds.
        let escrowed_baton = control
            .surrender_solo_for_era(TurnRight::Solo(baton))
            .expect("surrender Solo(Baton) before opening era");
        let second_handle = escrowed_baton.era_world_handle().unwrap();
        control
            .open_era(world_handle, port)
            .expect("opening era after surrendering Solo succeeds");

        // Opening a second era while one is open is refused.
        let (second_port, _inbox_rx2, _world_tx2) = test_group_port();
        let (_second_handle, _second_port, err) = control
            .open_era(second_handle, second_port)
            .expect_err("opening a second era must be refused");
        assert_eq!(err, EraOpenError::AlreadyOpen);

        // Closing the era restores Solo(baton) and re-arms the SoloBatonHeld guard.
        let (restored, returned_handle, returned_port) = control
            .close_era(escrowed_baton)
            .expect("closing open era restores Solo(Baton)");
        assert!(restored.is_solo());
        let (_returned_handle, _returned_port, err) = control
            .open_era(returned_handle, returned_port.expect("port returned"))
            .expect_err("QD1: after closing era, Solo is held again so open_era refuses");
        assert_eq!(err, EraOpenError::SoloBatonHeld);
    }

    #[test]
    fn qd1_era_time_wake_carrying_solo_fails_typed_check() {
        use crate::emacs_core::eval::parallel_entry::keeper_driver::WorldLend;
        use crate::emacs_core::eval::parallel_entry::world_turn::{
            WorldReturned, WorldTurn, WorldTurnIdentity,
        };
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = owner.start_green_runtime().unwrap();
        let heap = baton.parallel_heap_identity();
        let world_handle = baton.era_world_handle().unwrap();
        let era_turn_handle = baton.era_world_handle().unwrap();
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let main_rx = control.register_main();

        let (tx, _rx) = mpsc::channel();
        let make_era_right = |handle| {
            let turn = WorldTurn::cut_for_test(WorldTurnIdentity {
                heap,
                epoch: NonZeroU64::MIN,
            });
            TurnRight::from_era(turn, WorldLend::for_test(tx.clone()), handle).unwrap()
        };

        // Outside an era (SoloActive), an Era wake fails the typed check.
        let era_right = make_era_right(era_turn_handle);
        let (rejected_era_wake, err) = control
            .try_send_wake(ThreadKey::Main, Wake::Resume(era_right))
            .expect_err("Era wake outside an open era must fail typed check");
        assert_eq!(err, WakeAuthorityError::EraWakeOutsideEra);
        assert!(matches!(main_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let Wake::Resume(era_right) = rejected_era_wake else {
            panic!("expected returned Wake::Resume");
        };

        // Surrender Solo and open the era.
        let escrowed_baton = control
            .surrender_solo_for_era(TurnRight::Solo(baton))
            .expect("surrender Solo");
        let (port, _inbox_rx, _world_tx) = test_group_port();
        control.open_era(world_handle, port).expect("open era");

        // QD1: an era-time wake that would produce Solo fails the typed check
        // and does not enqueue anything on the receiver.
        let (rejected_solo_wake, err) = control
            .try_send_wake(
                ThreadKey::Main,
                Wake::Resume(TurnRight::Solo(escrowed_baton)),
            )
            .expect_err("QD1: era-time Solo wake must fail typed check");
        assert_eq!(err, WakeAuthorityError::SoloWakeInOpenEra);
        assert!(matches!(main_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let Wake::Resume(solo_right) = rejected_solo_wake else {
            panic!("expected returned Wake::Resume");
        };
        let escrowed_baton = solo_right.into_solo().expect("returned Solo Baton");

        // An era-time wake carrying TurnRight::Era succeeds and arrives intact.
        control
            .try_send_wake(ThreadKey::Main, Wake::Resume(era_right))
            .expect("era-time Era wake succeeds");
        let Wake::Resume(received) = main_rx.try_recv().expect("Era wake delivered") else {
            panic!("expected delivered Wake::Resume");
        };
        assert!(received.is_era());
        let (turn, lend, _world_handle) = received.into_era().unwrap().into_parts();
        lend.give_back(WorldReturned::from_end(turn));

        let (_restored, _handle, _port) = control.close_era(escrowed_baton).expect("close era");
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "QD1 violation")]
    fn qd1_debug_assert_catches_solo_authority_in_open_era() {
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = owner.start_green_runtime().unwrap();
        let world_handle = baton.era_world_handle().unwrap();
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let escrowed_baton = control
            .surrender_solo_for_era(TurnRight::Solo(baton))
            .expect("surrender Solo");
        let (port, _inbox_rx, _world_tx) = test_group_port();
        control.open_era(world_handle, port).expect("open era");

        // Presenting TurnRight::Solo while an era is open trips the QD1 debug assert.
        let bogus_solo = TurnRight::Solo(escrowed_baton);
        control.debug_assert_turn_authority(&bogus_solo);
    }

    /// MEDIUM-2: when the last runnable Baton thread parks on a blocking wait
    /// while `ring_waiter()` is raised, `transfer_era` enters Case (i) (`Idle`)
    /// directly without emitting a spurious empty `BatonRequest` round-trip.
    #[test]
    fn l67b_b43_ring_waiter_with_no_eligible_baton_thread_enters_idle_without_spurious_request() {
        use crate::emacs_core::eval::parallel_entry::driver_core::{GroupMsg, IntervalEnd};
        use crate::emacs_core::eval::parallel_entry::keeper_driver::KeeperMsg;
        use crate::emacs_core::eval::parallel_entry::world_turn::{WorldTurn, WorldTurnIdentity};
        use crate::emacs_core::eval::{Context, ThreadingMode};

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = owner.start_green_runtime().unwrap();
        let heap = baton.parallel_heap_identity();
        let world_handle = baton.era_world_handle().unwrap();
        let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
        let main_rx = control.register_main();
        let escrowed = control
            .surrender_solo_for_era(TurnRight::Solo(baton))
            .expect("surrender Solo");

        let (inbox_tx, inbox_rx) = mpsc::channel();
        let (world_tx, world_rx) = mpsc::sync_channel(1);
        let waiter = Arc::new(RingWaiter::new_for_test());
        let port = GroupPort::from_raw_parts_for_test(inbox_tx, world_rx, Arc::clone(&waiter));
        control.open_era(world_handle, port).expect("open era");

        let prev_holder = owner.with_green_runtime(|runtime| {
            runtime
                .carried_holder
                .replace(crate::emacs_core::eval::green::CarriedHolder::Stowed)
        });
        let epoch1 = NonZeroU64::new(1).unwrap();
        let epoch2 = NonZeroU64::new(2).unwrap();
        let id1 = WorldTurnIdentity {
            heap,
            epoch: epoch1,
        };
        let id2 = WorldTurnIdentity {
            heap,
            epoch: epoch2,
        };

        let mx = MutexId::new(77);
        std::thread::scope(|scope| {
            let ctrl = Arc::clone(&control);
            let waiter_t = Arc::clone(&waiter);
            let main_thread = scope.spawn(move || {
                let right = ctrl.acquire_initial_era_turn().expect("initial turn");
                // Raise ring_waiter while holding the WorldTurn, then park on a
                // blocking mutex with ZERO other runnable Baton threads.
                waiter_t.set_for_test(true);
                let wake = ctrl
                    .transfer(ThreadKey::Main, Wait::Mutex(mx), right, &main_rx)
                    .expect("woken after mutex unlock");
                let Wake::Resume(right2) = wake else {
                    panic!("expected Wake::Resume");
                };
                ctrl.release_era_turn_to_idle(right2);
            });

            // 1. Satisfy initial acquire_initial_era_turn.
            assert!(matches!(
                inbox_rx.recv_timeout(Duration::from_secs(5)),
                Ok(KeeperMsg::BatonRequest)
            ));
            world_tx
                .send(GroupMsg::World {
                    turn: WorldTurn::cut_for_test(id1),
                    epoch: epoch1,
                })
                .expect("lend initial World");

            // 2. Main calls transfer(Wait::Mutex) while ring_waiter == true:
            // must send WorldReturn { Done } and ZERO spurious BatonRequest!
            assert!(matches!(
                inbox_rx.recv_timeout(Duration::from_secs(5)),
                Ok(KeeperMsg::WorldReturn {
                    end: IntervalEnd::Done,
                    ..
                })
            ));
            // Give wait_while_era_idle a brief window; reply false to any QD2
            // IdleQuery and assert NO BatonRequest arrives before we wake mx.
            let deadline = Instant::now() + Duration::from_millis(25);
            while Instant::now() < deadline {
                match inbox_rx.recv_timeout(Duration::from_millis(5)) {
                    Ok(KeeperMsg::IdleQuery { reply }) => {
                        let _ = reply.try_send(false);
                    }
                    Ok(KeeperMsg::BatonRequest) => {
                        panic!(
                            "MEDIUM-2 regression: spurious BatonRequest emitted when no Baton thread is eligible"
                        );
                    }
                    Ok(_) => panic!("unexpected keeper msg while idle"),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => panic!("inbox closed"),
                }
            }

            // 3. Now wake Main -> ONE coalesced BatonRequest arrives, lend epoch2.
            waiter.set_for_test(false);
            assert_eq!(control.wake_waiting_for(Wait::Mutex(mx)), 1);
            loop {
                match inbox_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(KeeperMsg::IdleQuery { reply }) => {
                        let _ = reply.try_send(false);
                    }
                    Ok(KeeperMsg::BatonRequest) => break,
                    Ok(_) => panic!("expected BatonRequest after wake"),
                    Err(err) => panic!("inbox error waiting for BatonRequest: {err:?}"),
                }
            }
            world_tx
                .send(GroupMsg::World {
                    turn: WorldTurn::cut_for_test(id2),
                    epoch: epoch2,
                })
                .expect("lend second World");
            assert!(matches!(
                inbox_rx.recv_timeout(Duration::from_secs(5)),
                Ok(KeeperMsg::WorldReturn {
                    end: IntervalEnd::Done,
                    ..
                })
            ));
            main_thread.join().expect("main thread");
        });

        owner.with_green_runtime(|runtime| {
            runtime.carried_holder.set(prev_holder);
        });
        let (_solo, _wh, _port) = control.close_era(escrowed).expect("close era");
    }
}
