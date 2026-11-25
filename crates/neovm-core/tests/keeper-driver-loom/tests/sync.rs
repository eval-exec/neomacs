//! The model's side of the driver's sync seam (`parallel_entry/sync.rs`).
//!
//! The product core takes its channels and its backoff yield/sleep from
//! `super::sync`. Loom's own `mpsc` cannot stand in for them: it has no
//! `sync_channel`, and dropping every sender does not disconnect its
//! receiver, while the driver ends `Drained` on exactly that disconnect and
//! an owner learns of a dead keeper from it (K9 L9). So this seam builds
//! std-shaped channels on Loom's `Mutex` and `park`/`unpark`.
//!
//! Each channel, and each [`Signal`], has one waiter. The waiter registers
//! its thread under the lock and parks; the waker takes that registration
//! under the same lock and unparks it, so every unpark meets a thread that
//! is parked or about to park. Loom's `Condvar` can leave a stale waiter
//! whose later wake resumes a thread blocked on a mutex, and its `Notify`
//! explores a spurious wake at every wait, which multiplies the state space
//! without testing anything the driver relies on. A send into a full
//! `sync_channel` panics rather than block: the keeper sends at most one
//! answer per request, plus the poison fan-out's `try_send`, so a blocking
//! send on a full grant channel would itself be a protocol break.

use loom::sync::{Arc, Mutex, MutexGuard};
use loom::thread::Thread;
use std::collections::VecDeque;
use std::time::Duration;

/// The driver's backoff yields here.
pub fn yield_now() {
    loom::thread::yield_now();
}

loom::lazy_static! {
    /// Virtual nanoseconds, advanced only by the model's backoff sleeps, so
    /// the contention bound is measured on the model's own time, never the
    /// host's. A Loom lazy static lives for one execution, so every
    /// execution's clock starts at zero and no count crosses executions or
    /// tests (lead #14111). One OS thread runs every model thread, so the
    /// order of the advances is the schedule's.
    static ref VIRTUAL_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
}

/// The backoff's clock on the model's virtual time.
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

/// A backoff sleep advances virtual time by DURATION and yields.
pub fn sleep(duration: Duration) {
    let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
    VIRTUAL_NANOS.fetch_add(nanos, std::sync::atomic::Ordering::Relaxed);
    loom::thread::yield_now();
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Register the current thread as the one waiter. A registration that is
/// still standing means a parked thread resumed with no wake. Loom replays a
/// recorded schedule without checking that its thread can run, so this is
/// how a nondeterministic model (one that took another path on replay) shows
/// up here.
fn register(waiter: &mut Option<Thread>) {
    assert!(
        waiter.is_none(),
        "model: a second waiter (a parked thread resumed unwoken; is the model deterministic?)"
    );
    *waiter = Some(loom::thread::current());
}

/// A one-waiter wakeup that keeps a notification nobody waited for, as
/// Loom's `Notify` does, without its spurious wakes.
pub struct Signal(Mutex<SignalState>);

struct SignalState {
    notified: bool,
    waiter: Option<Thread>,
}

impl Signal {
    pub fn new() -> Self {
        Signal(Mutex::new(SignalState {
            notified: false,
            waiter: None,
        }))
    }

    pub fn notify(&self) {
        let waiter = {
            let mut state = lock(&self.0);
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
            let mut state = lock(&self.0);
            if std::mem::take(&mut state.notified) {
                return;
            }
            register(&mut state.waiter);
        }
        loom::thread::park();
    }
}

pub mod mpsc {
    use super::{Arc, Mutex, MutexGuard, Thread, VecDeque, lock, register};
    pub use std::sync::mpsc::{RecvError, SendError, TryRecvError, TrySendError};

    struct State<T> {
        queue: VecDeque<T>,
        /// `None` for an unbounded channel.
        capacity: Option<usize>,
        senders: usize,
        receiver: bool,
        /// The receiver, parked until a message arrives or a sender leaves.
        waiter: Option<Thread>,
    }

    type Shared<T> = Arc<Mutex<State<T>>>;

    fn state<T>(shared: &Shared<T>) -> MutexGuard<'_, State<T>> {
        lock(shared)
    }

    /// Release the lock, then wake the receiver if it was parked.
    fn wake<T>(mut state: MutexGuard<'_, State<T>>) {
        let waiter = state.waiter.take();
        drop(state);
        if let Some(waiter) = waiter {
            waiter.unpark();
        }
    }

    fn shared<T>(capacity: Option<usize>) -> Shared<T> {
        Arc::new(Mutex::new(State {
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
        // While unwinding (a Loom failure report) touch nothing, so the
        // report is not turned into an abort by a second panic.
        if std::thread::panicking() {
            return;
        }
        let mut state = state(shared);
        state.senders -= 1;
        wake(state);
    }

    /// Push MESSAGE, or hand it back when the receiver is gone. A full
    /// bounded channel is `Full`.
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
        /// std's send blocks while full; in this protocol it never may.
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
                    register(&mut state.waiter);
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
