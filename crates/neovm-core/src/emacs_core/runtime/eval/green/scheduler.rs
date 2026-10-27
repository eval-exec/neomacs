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
use crate::emacs_core::process::GreenSelectHandle;
use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

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

/// What a thread that gave the Baton up waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Nothing: it runs again in its turn.
    Runnable,
    /// Another thread's end (`thread-join`).
    Join(ThreadKey),
    /// A mutex another thread holds (`mutex-lock`).
    Mutex(u64),
    /// A condition variable's notification (`condition-wait`).
    Condition(u64),
    /// A `thread_select` wait, ready on input, a scheduler event, or its
    /// absolute deadline. The scheduler watches I/O while no owner runs.
    Select { wait_end: Instant },
    /// It ended.
    Exited,
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
    NoRunnable(Baton),
}

#[derive(Default)]
struct ControlState {
    // There is only one Baton in the World, so a sender holding it proves
    // every recipient queue empty. A single reusable slot also allows a
    // newborn or parked recipient to receive after the sender has finished.
    senders: BTreeMap<ThreadKey, mpsc::SyncSender<Baton>>,
    waits: BTreeMap<ThreadKey, Wait>,
    ready: VecDeque<ThreadKey>,
    joins: BTreeMap<ThreadKey, std::thread::JoinHandle<()>>,
    closing: bool,
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

/// The scheduler, shared by every thread's seat.
#[derive(Default)]
pub(crate) struct Control {
    state: Mutex<ControlState>,
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
    pub(crate) fn install_select(&self, select: Option<GreenSelectHandle>) {
        assert!(
            self.select.set(select).is_ok(),
            "select handle installed twice"
        );
    }

    fn notify_select(&self) {
        if let Some(Some(select)) = self.select.get() {
            select.notify();
        }
    }

    fn state(&self) -> MutexGuard<'_, ControlState> {
        // Every update is a few map/queue operations that leave the state
        // consistent; a panic while holding the lock cannot tear it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
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
    pub(crate) fn register_main(&self) -> mpsc::Receiver<Baton> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut state = self.state();
        state.senders.insert(ThreadKey::Main, sender);
        state.waits.insert(ThreadKey::Main, Wait::Runnable);
        receiver
    }

    /// Register the new thread KEY, ready to run in its turn: its channel.
    pub(crate) fn register_worker(&self, key: ThreadKey) -> mpsc::Receiver<Baton> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut state = self.state();
        state.senders.insert(key, sender);
        state.waits.insert(key, Wait::Runnable);
        state.ready.push_back(key);
        receiver
    }

    /// Keep the OS thread KEY runs on, to join at shutdown.
    pub(crate) fn note_join_handle(&self, key: ThreadKey, handle: std::thread::JoinHandle<()>) {
        self.state().joins.insert(key, handle);
    }

    /// Whether KEY has ended.
    pub(crate) fn exited(&self, key: ThreadKey) -> bool {
        matches!(self.state().waits.get(&key), Some(Wait::Exited) | None)
    }

    /// Whether a thread other than ME is ready to run, or will be at a
    /// deadline.
    pub(crate) fn others_could_run(&self, me: ThreadKey) -> bool {
        self.state()
            .waits
            .iter()
            .any(|(key, wait)| *key != me && matches!(wait, Wait::Runnable | Wait::Select { .. }))
    }

    /// What the threads other than ME are doing at NOW.
    pub(crate) fn peers(&self, me: ThreadKey, now: Instant) -> Peers {
        let state = self.state();
        let mut earliest: Option<Instant> = None;
        for (key, wait) in &state.waits {
            if *key == me {
                continue;
            }
            match *wait {
                Wait::Runnable => return Peers::Ready,
                Wait::Select { wait_end, .. } if wait_end <= now => return Peers::Ready,
                Wait::Select {
                    wait_end: deadline, ..
                } => {
                    earliest = Some(earliest.map_or(deadline, |e| e.min(deadline)));
                }
                _ => {}
            }
        }
        earliest.map_or(Peers::Blocked, Peers::Sleeping)
    }

    /// Poll the selectors at a yield safepoint without taking an owner
    /// off the CPU. Preserve FIFO order: only readiness changes queue state.
    pub(crate) fn yield_peer_ready(&self, me: ThreadKey) -> bool {
        let mut state = self.state();
        if state.closing
            || state
                .ready
                .iter()
                .any(|key| *key != me && state.waits.get(key) == Some(&Wait::Runnable))
        {
            // The ensuing transfer handles I/O/deadline readiness. Avoid
            // scanning sleeping owners twice on an actual handoff.
            return true;
        }
        Self::expire_deadlines(&mut state, Instant::now());
        if state
            .waits
            .values()
            .any(|wait| matches!(wait, Wait::Select { .. }))
            && let Some(Some(select)) = self.select.get()
            && select.wait_until(Instant::now())
        {
            Self::wake_selects(&mut state);
        }
        state
            .ready
            .iter()
            .any(|key| *key != me && state.waits.get(key) == Some(&Wait::Runnable))
    }

    /// Make KEY the next thread to run, whatever it waits for.
    pub(crate) fn prioritize(&self, key: ThreadKey) {
        let mut state = self.state();
        if matches!(state.waits.get(&key), Some(Wait::Exited) | None) {
            return;
        }
        state.ready.retain(|ready| *ready != key);
        state.waits.insert(key, Wait::Runnable);
        state.ready.push_front(key);
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

    /// Make the first thread (in key order) waiting for WAIT runnable.
    pub(crate) fn wake_one_waiting_for(&self, wait: Wait) -> bool {
        let mut state = self.state();
        let key = state
            .waits
            .iter()
            .find(|(_, w)| **w == wait)
            .map(|(key, _)| *key);
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
            Some(Wait::Runnable | Wait::Exited) | None => {}
            Some(_) => {
                state.waits.insert(key, Wait::Runnable);
                state.ready.push_back(key);
            }
        }
    }

    /// Move threads whose deadline passed into the ready queue.
    fn expire_deadlines(state: &mut ControlState, now: Instant) {
        let expired: Vec<ThreadKey> = state
            .waits
            .iter()
            .filter(|(_, wait)| matches!(wait, Wait::Select { wait_end } if *wait_end <= now))
            .map(|(key, _)| *key)
            .collect();
        for key in expired {
            Self::wake_locked(state, key);
        }
    }

    /// Readiness wakes all selecting owners. They inspect their own wait
    /// policy and poll live descriptors only after reacquiring the Baton.
    fn wake_selects(state: &mut ControlState) {
        let selecting: Vec<_> = state
            .waits
            .iter()
            .filter_map(|(key, wait)| matches!(wait, Wait::Select { .. }).then_some(*key))
            .collect();
        for key in selecting {
            Self::wake_locked(state, key);
        }
    }

    /// Choose an owner, blocking only while every owner is parked. The
    /// Baton stays local and no World cell is read during this OS wait.
    fn pick_next(&self) -> Option<ThreadKey> {
        loop {
            let deadline = {
                let mut state = self.state();
                if state
                    .waits
                    .values()
                    .any(|wait| matches!(wait, Wait::Select { .. }))
                    && let Some(Some(select)) = self.select.get()
                    && select.wait_until(Instant::now())
                {
                    Self::wake_selects(&mut state);
                }
                match Self::pick(&mut state) {
                    Some((key, None)) => return Some(key),
                    Some((_, Some(deadline))) => deadline,
                    None => return None,
                }
            };
            let activity = if let Some(Some(select)) = self.select.get() {
                select.wait_until(deadline)
            } else {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                false
            };
            if activity {
                Self::wake_selects(&mut self.state());
            }
            // An eventless/spurious return never expires a deadline early.
            // pick's monotonic check is the sole clock-based wake.
        }
    }

    /// The next thread to hold the Baton, if any can run; with no ready
    /// thread but one waiting for a deadline, the earliest deadline.
    fn pick(state: &mut ControlState) -> Option<(ThreadKey, Option<Instant>)> {
        Self::expire_deadlines(state, Instant::now());
        while let Some(key) = state.ready.pop_front() {
            if state.waits.get(&key) == Some(&Wait::Runnable) {
                return Some((key, None));
            }
        }
        state
            .waits
            .iter()
            .filter_map(|(key, wait)| match wait {
                Wait::Select { wait_end } => Some((*key, *wait_end)),
                _ => None,
            })
            .min_by_key(|(_, deadline)| *deadline)
            .map(|(key, deadline)| (key, Some(deadline)))
    }

    /// Hand the Baton on from ME, which waits for WAIT, and block until it
    /// comes back. Returns it at once when ME is the next to run.
    pub(crate) fn transfer(
        &self,
        me: ThreadKey,
        wait: Wait,
        baton: Baton,
        receiver: &mpsc::Receiver<Baton>,
    ) -> Result<Baton, Handoff> {
        {
            let mut state = self.state();
            state.waits.insert(me, wait);
            if wait == Wait::Runnable {
                state.ready.push_back(me);
            }
        }
        #[cfg(test)]
        if let Wait::Select { wait_end } = wait {
            self.observe_select_park_for_test(me, wait_end);
        }
        let Some(next) = self.pick_next() else {
            self.state().waits.insert(me, Wait::Runnable);
            return Err(Handoff::NoRunnable(baton));
        };
        if next == me {
            self.state().waits.insert(me, Wait::Runnable);
            return Ok(baton);
        }
        let sender = self
            .state()
            .senders
            .get(&next)
            .cloned()
            .expect("every registered thread has a channel");
        if sender.try_send(baton).is_err() {
            // Disconnection loses the next holder. A full slot would mean
            // the sole-Baton invariant was broken. Neither can recover, and
            // try_send keeps an invariant failure from parking this sender.
            std::process::abort();
        }
        match receiver.recv() {
            Ok(baton) => {
                baton.enter();
                Ok(baton)
            }
            Err(_) => std::process::abort(),
        }
    }

    /// ME ended: hand the Baton to the next thread, wake its joiners, and
    /// return without waiting. With no thread able to run, the runtime is
    /// closing: every waiting thread is woken (it signals) and gets the
    /// Baton in turn.
    pub(crate) fn finish(&self, me: ThreadKey, baton: Baton) {
        {
            let mut state = self.state();
            state.waits.insert(me, Wait::Exited);
            let joiners: Vec<ThreadKey> = state
                .waits
                .iter()
                .filter(|(_, wait)| **wait == Wait::Join(me))
                .map(|(key, _)| *key)
                .collect();
            for key in joiners {
                Self::wake_locked(&mut state, key);
            }
        }
        let next = self.pick_next().or_else(|| {
            let mut state = self.state();
            state.closing = true;
            let blocked: Vec<ThreadKey> = state
                .waits
                .iter()
                .filter(|(_, wait)| !matches!(wait, Wait::Exited))
                .map(|(key, _)| *key)
                .collect();
            for key in blocked {
                Self::wake_locked(&mut state, key);
            }
            Self::pick(&mut state).map(|(next, _)| next)
        });
        let sender = next.and_then(|next| self.state().senders.get(&next).cloned());
        match sender {
            Some(sender) => {
                if sender.try_send(baton).is_err() {
                    std::process::abort();
                }
            }
            // Nothing else lives: the Baton (and the World's facade epoch)
            // goes with the last thread.
            None => drop(baton),
        }
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
    }

    /// Take KEY's join handle, once KEY has ended.
    pub(crate) fn take_join_handle(&self, key: ThreadKey) -> Option<std::thread::JoinHandle<()>> {
        let mut state = self.state();
        if !matches!(state.waits.get(&key), Some(Wait::Exited)) {
            return None;
        }
        state.joins.remove(&key)
    }

    /// Take every worker's join handle (shutdown).
    pub(crate) fn take_join_handles(&self) -> Vec<(ThreadKey, std::thread::JoinHandle<()>)> {
        std::mem::take(&mut self.state().joins)
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
