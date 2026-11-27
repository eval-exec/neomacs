//! Loom sync seam for E5.4b (`wait_core.rs` + `driver_core.rs`).
//!
//! Supplies:
//! - `Arc`, `atomic`, `PoisonError`, `Mutex`, `Condvar` for `wait_core.rs`
//!   (`OwnerWake`, `SignalHint`, `ParallelParticipant`, `Participant`).
//! - `mpsc`, `yield_now`, `Instant`, `sleep`, `Signal` for `driver_core.rs`
//!   (same deterministic park/unpark design as `keeper-driver-loom/tests/sync.rs`).
//!
//! `Mutex` + `Condvar` wrap `loom::sync::Mutex` + `loom::thread::park/unpark`
//! directly (instead of `loom::sync::Condvar`) so a parked thread's waiter
//! registration is always taken cleanly on notify without Loom `Condvar`'s
//! stale-waiter bug or `Notify`'s spurious-wake state explosion, while keeping
//! the exact `std::sync::{Mutex, Condvar}` API that `wait_core.rs` calls.

pub use loom::sync::Arc;
pub use loom::sync::atomic;
pub use std::sync::PoisonError;

use loom::thread::Thread;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize as StdAtomicUsize, Ordering as StdOrdering};
use std::time::Duration;

/// Write-only counter of actual `Condvar::wait` parks across executions
/// (never affects scheduling; lets Models (a) and (c) distinguish
/// wake-before-park vs wake-while-parked inside verbatim `OwnerWake::wait`).
static CONDVAR_PARKS: StdAtomicUsize = StdAtomicUsize::new(0);

pub fn condvar_parks() -> usize {
    CONDVAR_PARKS.load(StdOrdering::Relaxed)
}

/// Deterministic `std::sync::Mutex`-shaped wrapper around `loom::sync::Mutex`
/// whose guard retains a reference to its mutex so [`Condvar::wait`] can drop
/// the inner lock, park, and reacquire cleanly.
#[derive(Debug)]
pub struct Mutex<T>(loom::sync::Mutex<T>);

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Mutex<T> {
    pub fn new(value: T) -> Self {
        Self(loom::sync::Mutex::new(value))
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, T>, PoisonError<MutexGuard<'_, T>>> {
        match self.0.lock() {
            Ok(inner) => Ok(MutexGuard {
                mutex: self,
                inner: Some(inner),
            }),
            Err(poisoned) => Err(PoisonError::new(MutexGuard {
                mutex: self,
                inner: Some(poisoned.into_inner()),
            })),
        }
    }
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
    inner: Option<loom::sync::MutexGuard<'a, T>>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.as_ref().expect("MutexGuard holds active lock")
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.as_mut().expect("MutexGuard holds active lock")
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        let _ = self.inner.take();
    }
}

/// Minimal `std::sync::WaitTimeoutResult` stand-in for `OwnerWake::wait_timeout`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitTimeoutResult(bool);

impl WaitTimeoutResult {
    #[allow(dead_code)]
    pub fn timed_out(&self) -> bool {
        self.0
    }
}

/// Deterministic `std::sync::Condvar`-shaped primitive built on
/// `loom::sync::Mutex<VecDeque<Thread>>` + `loom::thread::park/unpark`.
///
/// Lock order: caller's associated `Mutex<T>` -> `Condvar.waiters`.
/// `wait` registers the current thread BEFORE releasing the associated mutex,
/// so a concurrent `notify_one` called under that mutex is strictly serialized
/// with registration and can never drop an unpark or leave a stale waiter.
#[derive(Debug, Default)]
pub struct Condvar {
    waiters: loom::sync::Mutex<VecDeque<Thread>>,
}

impl Condvar {
    pub fn new() -> Self {
        Self {
            waiters: loom::sync::Mutex::new(VecDeque::new()),
        }
    }

    pub fn notify_one(&self) {
        let waiter = self
            .waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front();
        if let Some(waiter) = waiter {
            waiter.unpark();
        }
    }

    #[allow(dead_code)]
    pub fn notify_all(&self) {
        let waiters: Vec<Thread> = {
            let mut queue = self.waiters.lock().unwrap_or_else(PoisonError::into_inner);
            queue.drain(..).collect()
        };
        for waiter in waiters {
            waiter.unpark();
        }
    }

    pub fn wait<'a, T>(
        &self,
        mut guard: MutexGuard<'a, T>,
    ) -> Result<MutexGuard<'a, T>, PoisonError<MutexGuard<'a, T>>> {
        CONDVAR_PARKS.fetch_add(1, StdOrdering::Relaxed);
        let mutex = guard.mutex;
        {
            let mut queue = self.waiters.lock().unwrap_or_else(PoisonError::into_inner);
            queue.push_back(loom::thread::current());
        }
        // Release the associated mutex before parking.
        let _ = guard.inner.take();
        drop(guard);
        loom::thread::park();
        mutex.lock()
    }

    pub fn wait_timeout<'a, T>(
        &self,
        mut guard: MutexGuard<'a, T>,
        _dur: Duration,
    ) -> Result<
        (MutexGuard<'a, T>, WaitTimeoutResult),
        PoisonError<(MutexGuard<'a, T>, WaitTimeoutResult)>,
    > {
        let mutex = guard.mutex;
        let _ = guard.inner.take();
        drop(guard);
        loom::thread::yield_now();
        match mutex.lock() {
            Ok(reacquired) => Ok((reacquired, WaitTimeoutResult(true))),
            Err(poisoned) => Err(PoisonError::new((
                poisoned.into_inner(),
                WaitTimeoutResult(true),
            ))),
        }
    }
}

/// The driver's backoff yields here.
pub fn yield_now() {
    loom::thread::yield_now();
}

loom::lazy_static! {
    static ref VIRTUAL_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
}

/// The driver backoff's virtual clock.
#[derive(Clone, Copy, Debug)]
pub struct Instant(u64);

impl Instant {
    pub fn now() -> Self {
        Instant(VIRTUAL_NANOS.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn elapsed(&self) -> Duration {
        Duration::from_nanos(
            VIRTUAL_NANOS
                .load(std::sync::atomic::Ordering::Relaxed)
                .saturating_sub(self.0),
        )
    }
}

/// A backoff sleep advances virtual time by `duration` and yields.
pub fn sleep(duration: Duration) {
    let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
    VIRTUAL_NANOS.fetch_add(nanos, std::sync::atomic::Ordering::Relaxed);
    loom::thread::yield_now();
}

fn raw_lock<T>(mutex: &loom::sync::Mutex<T>) -> loom::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn register_single(waiter: &mut Option<Thread>) {
    assert!(
        waiter.is_none(),
        "model: a second waiter (a parked thread resumed unwoken; is the model deterministic?)"
    );
    *waiter = Some(loom::thread::current());
}

/// One-waiter signal for the model era (matching `keeper-driver-loom`).
pub struct Signal(loom::sync::Mutex<SignalState>);

struct SignalState {
    notified: bool,
    waiter: Option<Thread>,
}

impl Signal {
    pub fn new() -> Self {
        Signal(loom::sync::Mutex::new(SignalState {
            notified: false,
            waiter: None,
        }))
    }

    pub fn notify(&self) {
        let waiter = {
            let mut state = raw_lock(&self.0);
            let waiter = state.waiter.take();
            if waiter.is_none() {
                state.notified = true;
            }
            waiter
        };
        if let Some(waiter) = waiter {
            waiter.unpark();
        }
    }

    pub fn wait(&self) {
        {
            let mut state = raw_lock(&self.0);
            if std::mem::take(&mut state.notified) {
                return;
            }
            register_single(&mut state.waiter);
        }
        loom::thread::park();
    }
}

pub mod mpsc {
    use super::{Arc, Thread, VecDeque, raw_lock, register_single};
    pub use std::sync::mpsc::{RecvError, SendError, TryRecvError, TrySendError};

    struct State<T> {
        queue: VecDeque<T>,
        capacity: Option<usize>,
        senders: usize,
        receiver: bool,
        waiter: Option<Thread>,
    }

    type Shared<T> = Arc<loom::sync::Mutex<State<T>>>;

    fn state<T>(shared: &Shared<T>) -> loom::sync::MutexGuard<'_, State<T>> {
        raw_lock(shared)
    }

    fn wake<T>(mut state: loom::sync::MutexGuard<'_, State<T>>) {
        let waiter = state.waiter.take();
        drop(state);
        if let Some(waiter) = waiter {
            waiter.unpark();
        }
    }

    fn shared<T>(capacity: Option<usize>) -> Shared<T> {
        Arc::new(loom::sync::Mutex::new(State {
            queue: VecDeque::new(),
            capacity,
            senders: 1,
            receiver: true,
            waiter: None,
        }))
    }

    pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let shared = shared(None);
        (Sender(shared.clone()), Receiver(shared))
    }

    pub fn sync_channel<T>(capacity: usize) -> (SyncSender<T>, Receiver<T>) {
        let shared = shared(Some(capacity));
        (SyncSender(shared.clone()), Receiver(shared))
    }

    pub struct Sender<T>(Shared<T>);
    pub struct SyncSender<T>(Shared<T>);
    pub struct Receiver<T>(Shared<T>);

    fn add_sender<T>(shared: &Shared<T>) -> Shared<T> {
        state(shared).senders += 1;
        shared.clone()
    }

    fn drop_sender<T>(shared: &Shared<T>) {
        if std::thread::panicking() {
            return;
        }
        let mut state = state(shared);
        state.senders -= 1;
        wake(state);
    }

    fn push<T>(shared: &Shared<T>, message: T) -> Result<(), TrySendError<T>> {
        let mut state = state(shared);
        if !state.receiver {
            return Err(TrySendError::Disconnected(message));
        }
        if state
            .capacity
            .is_some_and(|capacity| state.queue.len() >= capacity)
        {
            return Err(TrySendError::Full(message));
        }
        state.queue.push_back(message);
        wake(state);
        Ok(())
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Sender(add_sender(&self.0))
        }
    }

    impl<T> Clone for SyncSender<T> {
        fn clone(&self) -> Self {
            SyncSender(add_sender(&self.0))
        }
    }

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            drop_sender(&self.0);
        }
    }

    impl<T> Drop for SyncSender<T> {
        fn drop(&mut self) {
            drop_sender(&self.0);
        }
    }

    impl<T> Drop for Receiver<T> {
        fn drop(&mut self) {
            if std::thread::panicking() {
                return;
            }
            let mut state = state(&self.0);
            state.receiver = false;
            state.queue.clear();
        }
    }

    impl<T> Sender<T> {
        pub fn send(&self, message: T) -> Result<(), SendError<T>> {
            push(&self.0, message).map_err(|error| match error {
                TrySendError::Disconnected(message) | TrySendError::Full(message) => {
                    SendError(message)
                }
            })
        }
    }

    impl<T> SyncSender<T> {
        pub fn send(&self, message: T) -> Result<(), SendError<T>> {
            match push(&self.0, message) {
                Ok(()) => Ok(()),
                Err(TrySendError::Disconnected(message)) => Err(SendError(message)),
                Err(TrySendError::Full(_)) => {
                    panic!("model: a blocking send into a full grant channel")
                }
            }
        }

        pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
            push(&self.0, message)
        }
    }

    impl<T> Receiver<T> {
        pub fn recv(&self) -> Result<T, RecvError> {
            loop {
                {
                    let mut state = state(&self.0);
                    if let Some(message) = state.queue.pop_front() {
                        return Ok(message);
                    }
                    if state.senders == 0 {
                        return Err(RecvError);
                    }
                    register_single(&mut state.waiter);
                }
                loom::thread::park();
            }
        }

        pub fn try_recv(&self) -> Result<T, TryRecvError> {
            let mut state = state(&self.0);
            match state.queue.pop_front() {
                Some(message) => Ok(message),
                None if state.senders == 0 => Err(TryRecvError::Disconnected),
                None => Err(TryRecvError::Empty),
            }
        }
    }
}
