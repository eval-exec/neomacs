//! Bounded carrier OS-thread pool for the Green runtime (S-track).
//!
//! A Lisp thread still gets a fresh `ThreadState` and handle from
//! `prepare_make_thread`; only the OS executor is reused. A carrier runs one
//! job, then either parks as an idle carrier (bounded by
//! [`MAX_IDLE_CARRIERS`]) or exits. Idle carriers hold no Baton and touch no
//! World state. Parking performs no TLS work: the reset of the previous job's
//! retired records happens on the carrier thread only if this pool hands it
//! another job, so a carrier that is never reused pays nothing.
//!
//! Lifecycle is typed, not boolean:
//! [`CarrierSlot`] tracks each live carrier's `Idle`/`Running`/`Draining`
//! state and [`PoolState`] tracks the pool's `Open`/`Draining`/`Closed` state.
//!
//! This pool is Green-only. Parallel-era owners identify OS threads through
//! `std::thread::ThreadId` for mutator confinement and are never pooled here.

use super::super::world::Baton;
use super::scheduler::{Control, ThreadKey, Wake};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

/// How many idle carriers one World retains. A constant: no env knob.
pub(super) const MAX_IDLE_CARRIERS: usize = 16;

/// Live carrier OS threads, for the shutdown/leak test.
#[cfg(test)]
pub(crate) static LIVE_CARRIERS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Snapshot of [`LIVE_CARRIERS`] for tests.
#[cfg(test)]
pub(crate) fn live_carriers_for_test() -> usize {
    LIVE_CARRIERS.load(std::sync::atomic::Ordering::SeqCst)
}

/// Identity of one OS carrier. This is not a Lisp thread identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct CarrierId(u64);

/// One job handed to a carrier: the Lisp thread's Baton channel and the
/// runtime it runs in.
pub(super) struct CarrierJob {
    pub(super) key: ThreadKey,
    pub(super) receiver: Receiver<Wake>,
    pub(super) control: Arc<Control>,
    /// True when an idle carrier was handed this job: its OS-thread TLS still
    /// holds the previous job's retired state and must be reset before the new
    /// owner's prologue. A fresh carrier starts with pristine TLS.
    pub(super) reused: bool,
}

/// A carrier OS thread could not be created. `dispatch` hands the whole
/// unstarted job back (not just its key): the caller owns the undo of the
/// Lisp-side registration, and the receiver inside the job must close the
/// channel before the scheduler entry is dropped.
pub(super) struct SpawnRefusal {
    pub(super) job: CarrierJob,
    pub(super) error: std::io::Error,
}

impl std::fmt::Debug for SpawnRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnRefusal")
            .field("key", &self.job.key)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// The typed lifecycle of one live carrier.
enum CarrierSlot {
    /// Parked, no Baton, no World state; blocked on its job channel.
    Idle { job_tx: Sender<CarrierJob> },
    /// Executing one Lisp job. The job itself carries the `ThreadKey`; the
    /// pool only needs to know the slot is not reusable.
    Running,
    /// Must not be reused; its thread is exiting.
    Draining,
}

/// The typed lifecycle of the pool itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PoolState {
    Open,
    Draining,
    Closed,
}

struct PoolInner {
    state: PoolState,
    slots: HashMap<CarrierId, CarrierSlot>,
    handles: HashMap<CarrierId, JoinHandle<()>>,
    idle: Vec<CarrierId>,
    next_id: u64,
}

impl PoolInner {
    fn new() -> Self {
        Self {
            state: PoolState::Open,
            slots: HashMap::new(),
            handles: HashMap::new(),
            idle: Vec::new(),
            next_id: 1,
        }
    }
}

/// Shared pool state. Carriers hold an `Arc` while idle so they can park and
/// take their next job without the Baton.
pub(super) struct PoolShared {
    inner: Mutex<PoolInner>,
    /// Test-only: the next fresh spawn fails before any OS thread exists, so
    /// the refusal/undo path is deterministic. Per pool (no global or env
    /// knob), consumed by one spawn.
    #[cfg(test)]
    fail_next_spawn: std::sync::atomic::AtomicBool,
    /// Test-only: the worker key of the last refused spawn.
    #[cfg(test)]
    last_refusal: Mutex<Option<ThreadKey>>,
}

impl PoolShared {
    fn lock(&self) -> MutexGuard<'_, PoolInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

/// The World's carrier pool. Dropping it (with the Green runtime) closes the
/// pool so idle carriers wake and exit; [`Self::drain`] additionally joins
/// every carrier for a deterministic teardown.
pub(super) struct CarrierPool {
    shared: Arc<PoolShared>,
}

impl CarrierPool {
    pub(super) fn new() -> Self {
        Self {
            shared: Arc::new(PoolShared {
                inner: Mutex::new(PoolInner::new()),
                #[cfg(test)]
                fail_next_spawn: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                last_refusal: Mutex::new(None),
            }),
        }
    }

    /// Test-only: make the next fresh carrier spawn fail without creating an
    /// OS thread.
    #[cfg(test)]
    pub(super) fn inject_spawn_failure(&self) {
        self.shared
            .fail_next_spawn
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Test-only: the worker key of the last refused spawn, if any.
    #[cfg(test)]
    pub(super) fn last_refusal_for_test(&self) -> Option<ThreadKey> {
        *self
            .shared
            .last_refusal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Dispatch `job`: reuse an idle carrier, or spawn a fresh one. A refused
    /// OS-thread creation hands the job back; the caller undoes the Lisp-side
    /// registration and signals.
    pub(super) fn dispatch(&self, job: CarrierJob) -> Result<(), SpawnRefusal> {
        let job = match self.take_idle_job_slot(job) {
            Ok(()) => return Ok(()),
            Err(job) => job,
        };
        self.spawn(job)
    }

    fn take_idle_job_slot(&self, mut job: CarrierJob) -> Result<(), CarrierJob> {
        let (id, job_tx) = {
            let mut inner = self.shared.lock();
            if inner.state != PoolState::Open {
                return Err(job);
            }
            let Some(id) = inner.idle.pop() else {
                return Err(job);
            };
            let old = inner.slots.insert(id, CarrierSlot::Running);
            match old {
                Some(CarrierSlot::Idle { job_tx }) => (id, job_tx),
                _ => {
                    // An idle list entry must name an idle slot.
                    debug_assert!(false, "idle carrier {id:?} had a non-idle slot");
                    inner.slots.insert(id, CarrierSlot::Draining);
                    return Err(job);
                }
            }
        };
        job.reused = true;
        match job_tx.send(job) {
            Ok(()) => Ok(()),
            Err(mpsc::SendError(job)) => {
                // The idle carrier went away without parking again. Drop the
                // reservation and let the caller spawn.
                self.shared.lock().slots.insert(id, CarrierSlot::Draining);
                Err(job)
            }
        }
    }

    fn spawn(&self, job: CarrierJob) -> Result<(), SpawnRefusal> {
        let id = {
            let mut inner = self.shared.lock();
            // Only a seated Lisp thread holding the Baton dispatches, and
            // teardown drains only after every thread has ended, so a closed
            // pool here is an invariant violation, not a recoverable state.
            assert!(
                inner.state == PoolState::Open,
                "dispatching a Lisp job on a draining carrier pool"
            );
            let id = CarrierId(inner.next_id);
            inner.next_id += 1;
            inner.slots.insert(id, CarrierSlot::Running);
            id
        };
        #[cfg(test)]
        if self
            .shared
            .fail_next_spawn
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(self.refuse_spawn(
                id,
                job,
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "injected carrier spawn failure",
                ),
            ));
        }
        let shared = Arc::clone(&self.shared);
        // `Builder::spawn` consumes (and drops) its closure on failure, so the
        // job is parked in a cell the new thread takes from; a failed spawn
        // leaves it there for the caller to recover.
        let handoff = Arc::new(Mutex::new(Some(job)));
        let for_carrier = Arc::clone(&handoff);
        let builder = std::thread::Builder::new().name(format!("neovm-carrier-{}", id.0));
        match builder.spawn(move || {
            let job = for_carrier
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .expect("a carrier takes its job exactly once");
            #[cfg(test)]
            LIVE_CARRIERS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            carrier_loop(job, shared, id);
            #[cfg(test)]
            LIVE_CARRIERS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }) {
            Ok(handle) => {
                self.shared.lock().handles.insert(id, handle);
                Ok(())
            }
            Err(error) => {
                let job = handoff
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .expect("an unstarted carrier keeps its job");
                Err(self.refuse_spawn(id, job, error))
            }
        }
    }

    /// Mark the reserved slot dead and hand the job back to the caller.
    fn refuse_spawn(&self, id: CarrierId, job: CarrierJob, error: std::io::Error) -> SpawnRefusal {
        self.shared.lock().slots.insert(id, CarrierSlot::Draining);
        #[cfg(test)]
        {
            *self
                .shared
                .last_refusal
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(job.key);
        }
        SpawnRefusal { job, error }
    }

    /// Wake and join every carrier. Called from the main owner's teardown
    /// while the World is still alive; carriers hold no Baton here.
    pub(super) fn drain(&self) {
        let handles = {
            let mut inner = self.shared.lock();
            if inner.state == PoolState::Closed {
                return;
            }
            inner.state = PoolState::Draining;
            // Closing each idle carrier's job channel wakes it with a recv
            // error; the carrier loop then exits.
            inner.slots.clear();
            inner.idle.clear();
            std::mem::take(&mut inner.handles)
        };
        for (_, handle) in handles {
            let _ = handle.join();
        }
        self.shared.lock().state = PoolState::Closed;
    }
}

impl Drop for CarrierPool {
    fn drop(&mut self) {
        // Safety net for a World dropped without `green_teardown`: close the
        // pool so parked carriers stop waiting for a job instead of leaking.
        let mut inner = self.shared.lock();
        if inner.state != PoolState::Closed {
            inner.state = PoolState::Closed;
            inner.slots.clear();
            inner.idle.clear();
            // Handles are detached: those threads exit on their own once
            // their channel closes. `drain` is the deterministic path.
            inner.handles.clear();
        }
    }
}

/// What a carrier does after a job finishes.
pub(super) enum ParkResult {
    /// The carrier is registered idle; wait for the next job on this channel.
    Parked(Receiver<CarrierJob>),
    /// The pool is draining or the idle bound is reached: exit the thread.
    Exit,
}

impl PoolShared {
    /// Register carrier `id` as idle, or tell it to exit.
    pub(super) fn park(&self, id: CarrierId) -> ParkResult {
        let mut inner = self.lock();
        if inner.state == PoolState::Open && inner.idle.len() < MAX_IDLE_CARRIERS {
            let (job_tx, job_rx) = mpsc::channel();
            inner.slots.insert(id, CarrierSlot::Idle { job_tx });
            inner.idle.push(id);
            ParkResult::Parked(job_rx)
        } else {
            inner.slots.insert(id, CarrierSlot::Draining);
            ParkResult::Exit
        }
    }
}

/// A carrier's life: run its first job, park, run the next, until the pool
/// drains.
///
/// The job registers the carrier idle itself, before `Control::finish`
/// publishes the Lisp thread's exit: the carrier holds no Baton between those
/// two points, so no other Lisp thread can dispatch a job in between, and a
/// waiter that observes `exited` is guaranteed to find the carrier parked.
fn carrier_loop(first: CarrierJob, shared: Arc<PoolShared>, id: CarrierId) {
    let mut job = first;
    loop {
        match super::switch::run_carrier_job(job, shared.as_ref(), id) {
            ParkResult::Parked(rx) => match rx.recv() {
                Ok(next) => job = next,
                // The pool dropped this carrier's sender: drain/closed.
                Err(_) => return,
            },
            ParkResult::Exit => return,
        }
    }
}
