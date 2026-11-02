//! Who holds the Baton next (ported from F4's `Control`, research branch
//! `facade_threads.rs`).
//!
//! Each Lisp thread has a key and a channel; a parked thread blocks in its
//! channel's receive until the Baton arrives. The holder hands the Baton on
//! only at a switch point: it records what it waits for, picks the first
//! ready thread (FIFO, as GNU's runnable queue) and sends it the Baton.
//! A thread waiting for a mutex, a condition, another thread's end or a
//! deadline is not ready until another thread makes it so.

use super::super::world::Baton;
use super::host_service::{HostParkRegistration, HostRequestKey, RealWaitPermit, SettledHostCall};
use crate::emacs_core::process::GreenSelectHandle;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

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
    pub(crate) fn id(self) -> u64 {
        match self {
            Self::Main => 0,
            Self::Worker(id) => id.get(),
        }
    }

    /// The key of the thread with ID in the World's `ThreadManager`.
    pub(crate) fn from_id(id: u64) -> Self {
        NonZeroU64::new(id).map_or(Self::Main, Self::Worker)
    }
}

/// A mutex object id (the Lisp handle's numeric id). Distinct from a
/// condition id at the scheduler boundary; converted once in `api.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MutexId(u64);

impl MutexId {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }
}

/// A condition-variable object id (the Lisp handle's numeric id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ConditionId(u64);

impl ConditionId {
    pub(crate) fn new(id: u64) -> Self {
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
        baton: Baton,
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
    fn new(
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
    Resume(Baton),
    ServiceTurn {
        key: HostRequestKey,
        baton: Baton,
    },
    /// Resume the exact published owner step, without a terminal or signal hop.
    OwnerStep {
        key: HostRequestKey,
        baton: Baton,
    },
    /// Continue an admitted semantic operation; do not dispatch its actor again.
    ContinuationTurn {
        key: HostRequestKey,
        baton: Baton,
    },
    /// Reenter the owned native driver without restoring main's Lisp owner.
    NativePump {
        baton: Baton,
    },
}

impl Wake {
    fn baton(&self) -> &Baton {
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
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }

    pub(super) fn requester(&self) -> ThreadKey {
        self.requester
    }
}

static_assertions::assert_not_impl_any!(EndedServiceOwner: Send, Sync, Clone, Copy);

impl ReservedServiceSlice {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }

    pub(super) fn requester(&self) -> ThreadKey {
        self.requester
    }
}

static_assertions::assert_not_impl_any!(ReservedServiceSlice: Send, Sync, Clone, Copy);

/// Prepared under the registry borrow, consumed only after that borrow ends.
#[derive(Debug)]
#[must_use]
pub(super) struct ServiceTransferPlan {
    key: HostRequestKey,
    requester: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}
impl ServiceTransferPlan {
    pub(super) fn requester(&self) -> ThreadKey {
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
    Queued(Baton),
}

#[derive(Debug)]
#[must_use]
pub(super) struct ServiceTransferRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) baton: Baton,
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
    pub(super) baton: Baton,
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
    Idle(Baton),
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
    ResumeMain(Baton),
    ResumeOwnerStep { key: HostRequestKey, baton: Baton },
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
    LocalMain { key: HostRequestKey, baton: Baton },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NextOwner {
    Resume(ThreadKey),
    Native(NativeTurn),
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
    fn insert(&mut self, key: ThreadKey, wait: Wait) -> Option<Wait> {
        let old = self.by_key.insert(key, wait);
        if let Some(old) = old {
            self.unindex(key, old);
        }
        self.index(key, wait);
        old
    }

    fn remove(&mut self, key: &ThreadKey) -> Option<Wait> {
        let old = self.by_key.remove(key)?;
        self.unindex(*key, old);
        Some(old)
    }

    fn runnable_count(&self) -> usize {
        self.runnable_count
    }

    fn select_len(&self) -> usize {
        self.select_index.len()
    }

    fn index(&mut self, key: ThreadKey, wait: Wait) {
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

    fn unindex(&mut self, key: ThreadKey, wait: Wait) {
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
    fn take_condition_index_skip_for_test() -> bool {
        SKIP_NEXT_CONDITION_INDEX_REMOVAL.swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    fn remove_waiter<K: Ord>(
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
    fn verify(&self) -> Result<(), SchedulerIndexInconsistency> {
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
    fn fresh_ticket(&mut self) -> ReadyTicket {
        self.next_ticket += 1;
        ReadyTicket(NonZeroU64::new(self.next_ticket).expect("ready tickets start at one"))
    }

    /// Queue KEY at the back unless it already owns a queued turn.
    fn enqueue_back(&mut self, key: ThreadKey) -> bool {
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
    fn enqueue_front(&mut self, key: ThreadKey) -> bool {
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
    fn move_to_front(&mut self, key: ThreadKey) {
        self.discard(key);
        let ticket = self.fresh_ticket();
        self.entries.push_front((key, ticket));
        self.slots.insert(key, ReadySlot::Queued(ticket));
        self.valid_count += 1;
    }

    /// Remove every entry for KEY and its slot (the old `ready.retain`).
    fn discard(&mut self, key: ThreadKey) {
        self.entries.retain(|(entry, _)| *entry != key);
        if matches!(self.slots.remove(&key), Some(ReadySlot::Queued(_))) {
            self.valid_count -= 1;
        }
    }

    /// KEY's wait stopped being `Runnable`: its queued turn is stale from now
    /// on (lazy: the entry goes when the queue reaches it).
    fn invalidate(&mut self, key: ThreadKey) {
        if let Some(ReadySlot::Queued(_)) = self.slots.get(&key) {
            self.slots.insert(key, ReadySlot::NotQueued);
            self.valid_count -= 1;
        }
    }

    fn is_queued(&self, key: &ThreadKey) -> bool {
        matches!(self.slots.get(key), Some(ReadySlot::Queued(_)))
    }

    fn valid_count(&self) -> usize {
        self.valid_count
    }

    /// The first eligible queued turn, dropping invalid entries met on the
    /// way. An entry is valid only when its ticket still matches the slot AND
    /// its key is `Runnable` (the old `ready.retain` rule); eligible-but-
    /// ineligible entries (main in a native wait) keep their place, exactly
    /// as the old position scan did.
    fn pop_valid(
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
    fn pop_main(&mut self) -> Option<ThreadKey> {
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

    fn valid_excluding(&self, me: &ThreadKey) -> usize {
        self.valid_count - usize::from(self.is_queued(me))
    }

    /// The ready part of the shared invariant predicate: a queued slot has a
    /// matching entry and belongs to a Runnable key; valid_count is exact.
    fn verify(&self, waits: &WaitMap) -> Result<(), SchedulerIndexInconsistency> {
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
    fn push_back(&mut self, key: ThreadKey) {
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
    fn pop_front(&mut self) -> Option<ThreadKey> {
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
    fn front(&self) -> Option<&ThreadKey> {
        self.entries.front().map(|(key, _)| key)
    }

    #[cfg(test)]
    fn iter(&self) -> impl Iterator<Item = &ThreadKey> {
        self.entries.iter().map(|(key, _)| key)
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The valid queued keys in FIFO order (the differential's ready view).
    #[cfg(test)]
    fn valid_keys(&self) -> Vec<ThreadKey> {
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

#[derive(Default)]
struct ControlState {
    // There is only one Baton in the World, so a sender holding it proves
    // every recipient queue empty. A single reusable slot also allows a
    // newborn or parked recipient to receive after the sender has finished.
    senders: BTreeMap<ThreadKey, mpsc::SyncSender<Wake>>,
    waits: WaitMap,
    ready: ReadyQueue,
    closing: bool,
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
    fn verify_invariants(&self) -> Result<(), SchedulerIndexInconsistency> {
        self.waits.verify()?;
        self.ready.verify(&self.waits)?;
        Ok(())
    }
}

#[cfg(test)]
#[derive(Debug, thiserror::Error)]
pub(crate) enum SelectParkObservationError {
    #[error("select park gate lock poisoned")]
    GatePoisoned,
    #[error("MainConnectPending requires the Main owner")]
    NotMain,
    #[error("select park gate prepared twice")]
    AlreadyPrepared,
    #[error(transparent)]
    PendingConnect(#[from] crate::emacs_core::process::PendingConnectObservationError),
}
#[cfg(test)]
static_assertions::assert_impl_all!(SelectParkObservationError: std::error::Error, Send, Sync);

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectParkExpectation {
    Ordinary,
    MainConnectPending(crate::emacs_core::process::ProcessId),
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) enum SelectParkFacts {
    Ordinary,
    MainConnectPending(crate::emacs_core::process::MainConnectPending),
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct SelectParked {
    owner: ThreadKey,
    wait_end: Instant,
    facts: SelectParkFacts,
}

#[cfg(test)]
impl SelectParked {
    pub(crate) fn owner(&self) -> ThreadKey {
        self.owner
    }
    pub(crate) fn wait_end(&self) -> Instant {
        self.wait_end
    }
    pub(crate) fn into_facts(self) -> SelectParkFacts {
        self.facts
    }
}

#[cfg(test)]
static_assertions::assert_impl_all!(SelectParked: Send, std::fmt::Debug);
#[cfg(test)]
static_assertions::assert_not_impl_any!(SelectParked: Clone, Copy);

#[cfg(test)]
type SelectParkCallback = Box<dyn FnOnce(SelectParked) + Send>;

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
static_assertions::assert_impl_all!(SelectParkGate: Send, std::fmt::Debug);

#[cfg(test)]
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
    #[cfg(test)]
    main_native_events: Mutex<Vec<MainNativeWaitEvent>>,
    metadata_changed: Condvar,
    #[cfg(test)]
    after_next_select_park: Mutex<Option<SelectParkGate>>,
    /// An owned I/O handle: the idle scheduler never reads World storage.
    select: OnceLock<Option<GreenSelectHandle>>,
}

impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Control").finish_non_exhaustive()
    }
}

impl Control {
    #[cfg(test)]
    pub(crate) fn record_main_native_event_for_test(&self, event: MainNativeWaitEvent) {
        self.main_native_events.lock().unwrap().push(event);
    }
    #[cfg(test)]
    pub(crate) fn main_native_events_for_test(&self) -> Vec<MainNativeWaitEvent> {
        self.main_native_events.lock().unwrap().clone()
    }

    pub(crate) fn install_select(&self, select: Option<GreenSelectHandle>) {
        assert!(
            self.select.set(select).is_ok(),
            "select handle installed twice"
        );
    }

    fn notify_select(&self) {
        self.metadata_changed.notify_all();
        if let Some(Some(select)) = self.select.get() {
            select.notify();
            // A released native main owns a distinct registration. Its
            // durable scheduler state may become ready before or during the
            // OS wait; the idle-selector epoch alone cannot wake that role.
            // Drop the metadata guard before publishing to the paired source.
            let main_native =
                self.state().physical.get(&ThreadKey::Main) == Some(&OwnerLocation::NativeReleased);
            if main_native {
                select.nudge(
                    crate::host_effect::wake::ControlDestination::NativeMain,
                    crate::host_effect::wake::ControlNudge::SchedulerReady,
                );
            }
        }
    }

    pub(super) fn nudge_main_mailbox(&self) {
        self.nudge_main(crate::host_effect::wake::ControlNudge::MainMailboxReady);
    }

    fn nudge_main(&self, reason: crate::host_effect::wake::ControlNudge) {
        self.metadata_changed.notify_all();
        if let Some(Some(select)) = self.select.get() {
            select.nudge(
                crate::host_effect::wake::ControlDestination::NativeMain,
                reason,
            );
        }
    }

    fn state(&self) -> MutexGuard<'_, ControlState> {
        // Every update is a few map/queue operations that leave the state
        // consistent; a panic while holding the lock cannot tear it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn reserve_service(
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
    pub(super) fn reserve_local_service(
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

    fn transfer_local_metadata(
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
    pub(super) fn transfer_local_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
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
    pub(super) fn transfer_prepared_local_service(
        &self,
        plan: ServiceTransferPlan,
        baton: Baton,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let result = Self::transfer_local_metadata(
            &mut self.state(),
            plan.key,
            plan.requester,
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

    fn intermediate_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        Self::intermediate_metadata_for(state, slice.key, slice.requester)
    }

    fn intermediate_metadata_for(
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
    pub(super) fn return_to_requester_intermediate(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<IntermediateReturn, ServiceTransferRefusal> {
        let plan = match self.prepare_intermediate_return(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.return_prepared_intermediate(plan, baton)
    }

    pub(super) fn prepare_intermediate_return(
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

    pub(super) fn return_prepared_intermediate(
        &self,
        plan: IntermediateReturnPlan,
        baton: Baton,
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

    fn resume_owner_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        Self::resume_owner_metadata_for(state, slice.key, slice.requester)
    }

    fn resume_owner_metadata_for(
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
    pub(super) fn resume_owner_step(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let plan = match self.prepare_owner_resume(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.resume_prepared_owner_step(plan, baton)
    }

    pub(super) fn prepare_owner_resume(
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

    pub(super) fn resume_prepared_owner_step(
        &self,
        plan: OwnerResumePlan,
        baton: Baton,
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

    fn resume_local_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        Self::resume_local_metadata_for(state, slice.key, slice.requester)
    }

    fn resume_local_metadata_for(
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

    pub(super) fn resume_local_owner_step(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let plan = match self.prepare_owner_resume(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.resume_prepared_local_owner_step(plan, baton)
    }

    pub(super) fn resume_prepared_local_owner_step(
        &self,
        plan: OwnerResumePlan,
        baton: Baton,
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
    pub(super) fn return_from_local_service(
        &self,
        slice: ReservedServiceSlice,
        baton: Baton,
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
    pub(super) fn ended_service_owner(
        &self,
        slice: &ReservedServiceSlice,
        baton: &Baton,
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
    pub(super) fn ended_service_owner_for_test(
        key: HostRequestKey,
        requester: ThreadKey,
        baton: &Baton,
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

    pub(super) fn finish_ended_continuation(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
        baton: &Baton,
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

    fn finish_ended_continuation_metadata(
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

    fn validate_ended_continuation(
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
    fn note_continuation_release(state: &mut ControlState, requester: ThreadKey) {
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
    pub(super) fn transfer_to_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let plan = match self.prepare_service_transfer(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.transfer_prepared_service(plan, baton)
    }

    pub(super) fn prepare_service_transfer(
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
            requester: slice.requester,
            _confined: PhantomData,
        })
    }

    pub(super) fn transfer_prepared_service(
        &self,
        slice: ServiceTransferPlan,
        baton: Baton,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let mut state = self.state();
        let valid = state.services.get(&slice.key).is_some_and(|metadata| {
            metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
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
        Self::set_wait(&mut state, slice.requester, Wait::HostRequest(slice.key));
        state.ready.discard(slice.requester);
        Self::check_invariants(&state);
        state
            .physical
            .insert(slice.requester, OwnerLocation::Receiving);
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
    pub(super) fn take_queued_service(&self, baton: Baton) -> Result<Wake, ServiceTransferRefusal> {
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
    pub(super) fn return_from_service(
        &self,
        slice: ReservedServiceSlice,
        baton: Baton,
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
    pub(super) fn release_service(
        &self,
        slice: &ReservedServiceSlice,
        _permit: RealWaitPermit,
        baton: Baton,
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
        drop(state);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                Ok(NativeRelease::Transferred)
            }
            None => Ok(NativeRelease::Idle(baton)),
        }
    }

    /// Called after the native wrapper returns. Main reacquires for native
    /// completion only; its suspended Lisp Wait is neither consumed nor read.
    pub(super) fn reacquire_service(
        &self,
        slice: &ReservedServiceSlice,
        release: NativeRelease,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<Baton, ServiceAcquireRefusal> {
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
                drop(state);
                self.metadata_changed.notify_all();
                self.notify_select();
                match receiver.recv() {
                    Ok(Wake::ServiceTurn { key, baton }) if key == slice.key => {
                        baton.enter();
                        Ok(baton)
                    }
                    Ok(
                        Wake::Resume(baton)
                        | Wake::ServiceTurn { baton, .. }
                        | Wake::OwnerStep { baton, .. }
                        | Wake::ContinuationTurn { baton, .. }
                        | Wake::NativePump { baton },
                    ) => Err(ServiceAcquireRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        release: NativeRelease::Idle(baton),
                    }),
                    // Losing the sole Baton is an ownership failure. Native
                    // request cancellation itself never closes this channel.
                    Err(_) => std::process::abort(),
                }
            }
        }
    }

    /// Preserve this request's linear reservation while the owned driver
    /// serves another request. Deferral alone admits no ordinary Lisp owner.
    pub(super) fn defer_native_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<Baton, ServiceTransferRefusal> {
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

    fn defer_native_metadata(
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
    pub(super) fn enter_native_completion(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<Baton, ServiceTransferRefusal> {
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

    fn enter_native_completion_metadata(
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
    pub(super) fn release_native_pump(
        &self,
        _permit: RealWaitPermit,
        baton: Baton,
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
        drop(state);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                Ok(NativeRelease::Transferred)
            }
            None => Ok(NativeRelease::Idle(baton)),
        }
    }

    /// Physical completion first reacquires the native driver, not whichever
    /// request happened to arm the poll. A service wake for another key is valid.
    pub(super) fn reacquire_native_pump(
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
            NativeRelease::Transferred => {
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                if !state.pending_acquires.contains(&NativeAcquire::Pump) {
                    state.pending_acquires.push_back(NativeAcquire::Pump);
                }
                drop(state);
                self.notify_select();
                match receiver.recv() {
                    Ok(wake) => {
                        wake.baton().enter();
                        Ok(wake)
                    }
                    Err(_) => std::process::abort(),
                }
            }
        }
    }

    /// Service-only choices preserve main's original wait. A still-reserved
    /// PendingNative call cannot admit main's unrelated Lisp continuation.
    pub(super) fn choose_native_pump_turn(
        &self,
        baton: Baton,
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

    fn choose_native_metadata(
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
    pub(super) fn native_pump_source(
        &self,
    ) -> Result<std::sync::Arc<crate::host_effect::wake::ControlWake>, ServiceRefusal> {
        self.select
            .get()
            .and_then(Option::as_ref)
            .map(GreenSelectHandle::control_source)
            .ok_or(ServiceRefusal::MainUnavailable)
    }

    /// Recheck after registration, before committing to an actual OS wait.
    pub(super) fn native_service_queued(&self) -> NativeWork {
        if Self::queued_native_work(&self.state()) == NativeWork::Ready {
            NativeWork::Ready
        } else {
            NativeWork::Idle
        }
    }

    /// The requester keeps the Baton while main finishes an earlier native
    /// release. This metadata-only wait cannot run unrelated Lisp.
    pub(super) fn wait_queued_prepared(
        &self,
        slice: ServiceTransferPlan,
        baton: Baton,
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
                metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
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
    pub(super) fn wait_queued_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
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
    pub(super) fn main_native_acquire(&self, receiver: &mpsc::Receiver<Wake>) -> Wake {
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
    pub(super) fn main_native_released(&self) -> Result<(), ServiceRefusal> {
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
    pub(super) fn release_main_wait(
        &self,
        _permit: RealWaitPermit,
        baton: Baton,
        wait_end: Instant,
    ) -> NativeRelease {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id())
            || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Running)
        {
            std::process::abort();
        }
        #[cfg(test)]
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
        drop(state);
        #[cfg(test)]
        self.observe_select_park_for_test(ThreadKey::Main, wait_end);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                NativeRelease::Transferred
            }
            None => NativeRelease::Idle(baton),
        }
    }

    /// The physical wait has returned. Publish main's readiness before waking
    /// an idle scheduler; owner restoration and all Lisp remain in switch.rs.
    pub(super) fn reacquire_main_wait(
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
        #[cfg(test)]
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
            NativeRelease::Transferred => {
                Self::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                Self::enqueue_ready(&mut state, ThreadKey::Main);
                drop(state);
                self.notify_select();
                match receiver.recv() {
                    Ok(wake) => {
                        wake.baton().enter();
                        wake
                    }
                    Err(_) => std::process::abort(),
                }
            }
        }
    }

    fn finish_idle_main_acquire(state: &mut ControlState) {
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
    pub(super) fn cancel_before_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
    ) -> Result<(), ServiceCancelRefusal> {
        self.cancel_service(slice, settled, CancelPlacement::OrdinaryCompletion)
    }

    /// A refused/queued transfer left the requester holding the Baton. Its
    /// existing restore path resumes directly, without another FIFO turn.
    pub(super) fn cancel_current_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
        _baton: &Baton,
    ) -> Result<(), ServiceCancelRefusal> {
        self.cancel_service(slice, settled, CancelPlacement::CurrentBatonOwner)
    }

    fn cancel_service(
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

    #[cfg(test)]
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
    #[cfg(test)]
    pub(crate) fn prepare_select_park_for_test(
        &self,
        owner: ThreadKey,
        processes: &crate::emacs_core::process::ProcessManager,
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

    #[cfg(test)]
    fn observe_select_park_for_test(&self, owner: ThreadKey, wait_end: Instant) {
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
    pub(super) fn register_main(&self) -> mpsc::Receiver<Wake> {
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
    pub(super) fn register_worker(&self, key: ThreadKey) -> mpsc::Receiver<Wake> {
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
    pub(super) fn unregister_worker(&self, key: ThreadKey) {
        let mut state = self.state();
        Self::set_wait(&mut state, key, Wait::Exited);
        // Whole-key removal goes through the seam; this path no longer writes
        // waits/senders/physical directly beside `set_wait`.
        Self::remove_key(&mut state, key);
    }

    /// Test-only: whether any scheduler record still names KEY.
    #[cfg(test)]
    pub(super) fn worker_registered_for_test(&self, key: ThreadKey) -> bool {
        let state = self.state();
        state.senders.contains_key(&key)
            || state.waits.contains_key(&key)
            || state.physical.contains_key(&key)
    }

    /// Test-only: how many threads the registry currently charges (wait
    /// entries, wake senders).
    #[cfg(test)]
    pub(super) fn registry_sizes_for_test(&self) -> (usize, usize) {
        let state = self.state();
        (state.waits.len(), state.senders.len())
    }

    /// Whether KEY has ended.
    pub(crate) fn exited(&self, key: ThreadKey) -> bool {
        matches!(self.state().waits.get(&key), Some(Wait::Exited) | None)
    }

    /// `Wait::Runnable` entries other than ME's own contribution.
    fn runnable_excluding(state: &ControlState, me: ThreadKey) -> usize {
        state.waits.runnable_count() - usize::from(state.waits.get(&me) == Some(&Wait::Runnable))
    }

    /// Select waits other than ME's own contribution.
    fn selects_excluding(state: &ControlState, me: ThreadKey) -> usize {
        state.waits.select_len()
            - usize::from(matches!(state.waits.get(&me), Some(Wait::Select { .. })))
    }

    /// The earliest Select deadline other than ME's, in `(wait_end, key)`
    /// order. One index entry per key, so peeking at most two entries is the
    /// old skip-`me` scan.
    fn earliest_other_select(state: &ControlState, me: ThreadKey) -> Option<DeadlineKey> {
        let mut index = state.waits.select_index.keys();
        let first = index.next()?;
        if first.key == me {
            index.next().copied()
        } else {
            Some(*first)
        }
    }

    /// Queued ready turns other than ME's own contribution.
    fn ready_valid_excluding(state: &ControlState, me: ThreadKey) -> usize {
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

    fn yield_peer_ready_at(&self, me: ThreadKey, now: Instant) -> bool {
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
            && select.wait_until(now)
        {
            Self::wake_selects_at(&mut state, now);
        }
        Self::ready_valid_excluding(&state, me) > 0
    }

    /// Make KEY the next thread to run, whatever it waits for.
    pub(crate) fn prioritize(&self, key: ThreadKey) {
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
    }

    /// What KEY waits for.
    pub(crate) fn wait_of(&self, key: ThreadKey) -> Option<Wait> {
        self.state().waits.get(&key).copied()
    }

    /// Whether the runtime is shutting down.
    pub(crate) fn lifecycle(&self) -> Lifecycle {
        if self.state().closing {
            Lifecycle::Closing
        } else {
            Lifecycle::Running
        }
    }

    /// Make KEY runnable again if it waits for something.
    pub(crate) fn wake(&self, key: ThreadKey) {
        let mut state = self.state();
        Self::wake_locked(&mut state, key);
        drop(state);
        self.notify_select();
    }

    /// Make every thread waiting for WAIT runnable (a mutex release, a
    /// notification to all, a thread's end).
    pub(crate) fn wake_waiting_for(&self, wait: Wait) -> usize {
        let mut state = self.state();
        match wait {
            Wait::Join(target) => {
                let mut woken = 0;
                while let Some(key) = state
                    .waits
                    .join_waiters
                    .get(&target)
                    .and_then(|set| set.first().copied())
                {
                    Self::wake_locked(&mut state, key);
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
                    Self::wake_locked(&mut state, key);
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
                    Self::wake_locked(&mut state, key);
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
                    Self::wake_locked(&mut state, *key);
                }
                keys.len()
            }
        }
    }

    /// Make the first thread (in key order) waiting for WAIT runnable.
    pub(crate) fn wake_one_waiting_for(&self, wait: Wait) -> bool {
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
            Some(key) => {
                Self::wake_locked(&mut state, key);
                true
            }
            None => false,
        }
    }

    fn wake_locked(state: &mut ControlState, key: ThreadKey) {
        match state.waits.get(&key) {
            Some(Wait::Runnable | Wait::Exited | Wait::HostRequest(_)) | None => {}
            Some(Wait::Join(_) | Wait::Mutex(_) | Wait::Condition(_) | Wait::Select { .. }) => {
                Self::set_wait(state, key, Wait::Runnable);
                Self::enqueue_ready(state, key);
            }
        }
    }

    /// Move threads whose deadline passed into the ready queue, EARLIEST
    /// DEADLINE FIRST. The index is keyed by `(wait_end, ThreadKey)`, so the
    /// expired set comes out in exactly the order the independent per-thread
    /// timers would have produced; simultaneous deadlines keep `ThreadKey`
    /// order (the b262 pin).
    fn expire_deadlines(state: &mut ControlState, now: Instant) {
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
    fn wake_selects(state: &mut ControlState) {
        Self::wake_selects_at(state, Instant::now());
    }

    fn wake_selects_at(state: &mut ControlState, now: Instant) {
        Self::expire_deadlines(state, now);
        while let Some(key) = state.waits.select_keys.first().copied() {
            Self::wake_locked(state, key);
        }
    }

    fn pick_acquire(state: &mut ControlState) -> Option<NativeTurn> {
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

    fn pick_reserved_service(state: &mut ControlState) -> Option<NativeTurn> {
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

    fn continuation_progress(
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

    fn pending_native(state: &ControlState) -> NativeWork {
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

    fn pick_queued_service(state: &mut ControlState) -> Option<NativeTurn> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Receiving) {
            return None;
        }
        Self::pick_acquire(state).or_else(|| Self::pick_reserved_service(state))
    }

    /// Native acquire/service turns never complete main's semantic Wait.
    fn pick_service(state: &mut ControlState) -> Option<NativeTurn> {
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

    fn queued_native_work(state: &ControlState) -> NativeWork {
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

    fn service_ready(state: &ControlState) -> bool {
        state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Receiving)
            && Self::queued_native_work(state) == NativeWork::Ready
    }

    fn native_wake(turn: NativeTurn, baton: Baton) -> Wake {
        match turn {
            NativeTurn::Service(key) => Wake::ServiceTurn { key, baton },
            NativeTurn::Continuation(key) => Wake::ContinuationTurn { key, baton },
            NativeTurn::Pump => Wake::NativePump { baton },
        }
    }

    fn wake_for(next: NextOwner, baton: Baton) -> (ThreadKey, Wake) {
        match next {
            NextOwner::Resume(key) => (key, Wake::Resume(baton)),
            NextOwner::Native(turn) => (ThreadKey::Main, Self::native_wake(turn, baton)),
        }
    }

    fn intercept_ready(state: &mut ControlState, me: ThreadKey) -> Option<NativeTurn> {
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
    pub(super) fn intercept_acquire(
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

    fn send_wake(&self, target: ThreadKey, wake: Wake) {
        let sender = self.state().senders.get(&target).cloned();
        let Some(sender) = sender else {
            std::process::abort()
        };
        // There is exactly one Baton. A closed receiver or occupied slot is
        // an ownership failure, never a Lisp-level request failure.
        if sender.try_send(wake).is_err() {
            std::process::abort()
        }
    }

    /// Choose an owner, blocking only while every owner is parked. The
    /// Baton stays local and no World cell is read during this OS wait.
    fn pick_next(&self) -> PickNext {
        loop {
            // Register BEFORE checking scheduler predicates. A transition
            // between this check and the OS block advances the same snapshot;
            // arming after dropping state would discard that durable nudge.
            let registration = self
                .select
                .get()
                .and_then(Option::as_ref)
                .map(GreenSelectHandle::arm);
            let deadline = {
                let mut state = self.state();
                if state.waits.select_len() > 0
                    && let Some(Some(select)) = self.select.get()
                    && select.wait_until(Instant::now())
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
            let activity = if let Some(Some(select)) = self.select.get() {
                match &registration {
                    Some(Ok(registration)) => select.wait_registered(registration, deadline),
                    Some(Err(_)) => true,
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
    fn set_wait(state: &mut ControlState, key: ThreadKey, wait: Wait) {
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
    fn remove_key(state: &mut ControlState, key: ThreadKey) {
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
    fn enqueue_ready(state: &mut ControlState, key: ThreadKey) {
        state.ready.enqueue_back(key);
        Self::check_invariants(state);
    }

    /// Queue KEY's ready turn at the front if it has none (the native
    /// interception requeue preserves the turn it is about to use).
    fn enqueue_ready_front(state: &mut ControlState, key: ThreadKey) {
        state.ready.enqueue_front(key);
        Self::check_invariants(state);
    }

    fn pop_ready(state: &mut ControlState, selection: ReadySelection) -> Option<ThreadKey> {
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
    fn main_excluded(state: &ControlState) -> bool {
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
    fn pick(state: &mut ControlState) -> Option<(ThreadKey, Option<Instant>)> {
        Self::pick_at(state, Instant::now())
    }

    /// `pick` at an explicit clock instant (the differential harness drives
    /// the same instants through both the reference scans and this code).
    fn pick_at(state: &mut ControlState, now: Instant) -> Option<(ThreadKey, Option<Instant>)> {
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
    fn check_invariants(state: &ControlState) {
        if let Err(error) = state.verify_invariants() {
            panic!("scheduler wait-index invariant violated: {error}");
        }
    }

    #[cfg(not(debug_assertions))]
    fn check_invariants(state: &ControlState) {
        let _ = state;
    }

    /// Cold verdict check: every build runs the same predicate when the
    /// scheduler decides no owner can run. The caller passes the state it
    /// decided on, so the verdict and the predicate share one lock
    /// acquisition; it never repairs the index.
    #[cold]
    #[inline(never)]
    fn verify_invariants(state: &ControlState) -> Result<(), SchedulerIndexInconsistency> {
        state.verify_invariants()
    }

    /// Hand the Baton on from ME, which waits for WAIT, and block until it
    /// comes back. Returns it at once when ME is the next to run.
    pub(super) fn transfer(
        &self,
        me: ThreadKey,
        wait: Wait,
        baton: Baton,
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
        #[cfg(test)]
        if let Wait::Select { wait_end } = wait {
            self.observe_select_park_for_test(me, wait_end);
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

    /// Publish exit and join readiness before entering a scheduler/native wait.
    /// A released native main may be the only owner that can make progress.
    fn publish_owner_exit(&self, me: ThreadKey) {
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
    pub(crate) fn close(&self) {
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

        let mut owner = Context::new_with_threading(ThreadingMode::Green);
        let baton = owner.start_green_runtime().unwrap();
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

        let mut owner = Context::new_with_threading(ThreadingMode::Green);
        let baton = owner.start_green_runtime().unwrap();
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

        let mut owner = Context::new_with_threading(ThreadingMode::Green);
        let baton = owner.start_green_runtime().unwrap();
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

        let mut owner = Context::new_with_threading(ThreadingMode::Green);
        let baton = owner.start_green_runtime().unwrap();
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
        use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWakeOutcome};
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let manager = crate::emacs_core::process::ProcessManager::new();
        let control = Control::default();
        let _main = control.register_main();
        let _worker = control.register_worker(worker);
        control.install_select(manager.green_select_handle());
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
        use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWakeOutcome};
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        for original in [Wait::Runnable, Wait::Join(worker)] {
            let manager = crate::emacs_core::process::ProcessManager::new();
            let control = Control::default();
            let _main = control.register_main();
            let _worker = control.register_worker(worker);
            control.install_select(manager.green_select_handle());
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
}
