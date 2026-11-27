//! Self-contained, `sync`-abstracted parallel-owner wait/wake primitives for
//! Green `Control` (E5.3 D1, D2, Q4).
//!
//! All synchronization comes through `super::sync`, so E5.4b's standalone Loom
//! crate can copy this file verbatim and substitute `sync.rs` (zero `cfg(loom)`
//! in `neovm-eval`, matching `keeper-driver-loom`).
#![forbid(unsafe_code)]

use super::sync::{Arc, Condvar, Mutex, PoisonError, atomic, mpsc};
use std::time::Duration;

/// Per-parallel-owner wake slot (E5.3 D1, Q4a).
///
/// Backed by `Arc<(Mutex<bool>, Condvar)>` so wakes are durable (zero
/// spurious-unpark ambiguity) and directly modelable in Loom.
///
/// Lock-ordering invariant: `Control.state` mutex -> `OwnerWake.inner` mutex.
/// `OwnerWake` is always a leaf lock and never acquires `Control.state`.
#[derive(Clone, Default)]
pub(crate) struct OwnerWake {
    inner: Arc<(Mutex<bool>, Condvar)>,
}

impl std::fmt::Debug for OwnerWake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerWake")
            .field("pending", &self.is_pending())
            .finish()
    }
}

impl OwnerWake {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    /// Record a wake and notify any thread blocked in `wait`/`wait_timeout`.
    pub(crate) fn wake(&self) {
        let (lock, cvar) = &*self.inner;
        let mut woken = lock.lock().unwrap_or_else(PoisonError::into_inner);
        *woken = true;
        cvar.notify_one();
    }

    /// Clear any stale wake flag (called under `Control.state` lock right
    /// before registering a new blocking wait so a prior wake cannot fire
    /// spuriously while any subsequent wake sets the flag after registration).
    pub(crate) fn clear(&self) {
        let (lock, _) = &*self.inner;
        let mut woken = lock.lock().unwrap_or_else(PoisonError::into_inner);
        *woken = false;
    }

    /// Consume a pending wake without blocking.
    pub(crate) fn try_take(&self) -> bool {
        let (lock, _) = &*self.inner;
        let mut woken = lock.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::replace(&mut *woken, false)
    }

    /// Whether a wake is currently pending (used by D8's before/after deadlock
    /// check under `Control.state` lock).
    pub(crate) fn is_pending(&self) -> bool {
        let (lock, _) = &*self.inner;
        *lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Block until woken and consume the wake flag.
    #[allow(dead_code)] // Used by E5.4b Loom models and unbounded wait callers.
    pub(crate) fn wait(&self) {
        let (lock, cvar) = &*self.inner;
        let mut woken = lock.lock().unwrap_or_else(PoisonError::into_inner);
        while !*woken {
            woken = cvar.wait(woken).unwrap_or_else(PoisonError::into_inner);
        }
        *woken = false;
    }

    /// Block up to `timeout` for a wake. Returns `true` if a wake was consumed,
    /// `false` if the timeout elapsed without a wake.
    pub(crate) fn wait_timeout(&self, timeout: Duration) -> bool {
        let (lock, cvar) = &*self.inner;
        let mut woken = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if *woken {
            *woken = false;
            return true;
        }
        let (mut guard, _) = cvar
            .wait_timeout(woken, timeout)
            .unwrap_or_else(PoisonError::into_inner);
        if *guard {
            *guard = false;
            true
        } else {
            false
        }
    }
}

/// Per-parallel-owner atomic pending-signal hint (E5.3 D2).
///
/// Written with `Release` by `thread-signal` after storing `error_symbol` and
/// `error_data` in `ThreadManager`. Polled with `Relaxed` on the owner-local
/// fast path (polls and outside-turn `thread-yield`). Confirmed with `Acquire`
/// and cleared with `Release` inside a serial turn after draining `ThreadManager`.
#[derive(Clone, Default)]
pub(crate) struct SignalHint {
    pending: Arc<atomic::AtomicBool>,
}

impl std::fmt::Debug for SignalHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalHint")
            .field("pending", &self.peek_relaxed())
            .finish()
    }
}

impl SignalHint {
    pub(crate) fn new() -> Self {
        Self {
            pending: Arc::new(atomic::AtomicBool::new(false)),
        }
    }

    /// Mark that a signal is pending for this owner (`Release`).
    pub(crate) fn raise(&self) {
        self.pending.store(true, atomic::Ordering::Release);
    }

    /// Owner-local fast-path poll (`Relaxed`).
    #[inline]
    pub(crate) fn peek_relaxed(&self) -> bool {
        self.pending.load(atomic::Ordering::Relaxed)
    }

    /// Serial-turn confirmation (`Acquire`).
    #[inline]
    pub(crate) fn check_acquire(&self) -> bool {
        self.pending.load(atomic::Ordering::Acquire)
    }

    /// Clear the hint after `ThreadManager`'s pending error has been drained
    /// inside a serial turn (`Release`).
    pub(crate) fn clear_release(&self) {
        self.pending.store(false, atomic::Ordering::Release);
    }
}

/// A parallel owner's control-side handles in Green `Control` (E5.3 D1, D2).
#[derive(Clone, Debug)]
pub(crate) struct ParallelParticipant {
    pub(crate) wake: OwnerWake,
    pub(crate) signal_hint: SignalHint,
}

impl ParallelParticipant {
    pub(crate) fn new() -> Self {
        Self {
            wake: OwnerWake::new(),
            signal_hint: SignalHint::new(),
        }
    }
}

/// One participant registered with Green `Control` (E5.3 D1).
pub(crate) enum Participant<W> {
    /// A Baton thread (main or `make-thread` worker) woken by sending a `Wake`
    /// carrying the `TurnRight` over its single-slot channel.
    Baton(mpsc::SyncSender<W>),
    /// An opt-in parallel owner woken via its `OwnerWake` slot, after which it
    /// requests its own serial turn on the L6 ring.
    Parallel(ParallelParticipant),
}

impl<W> Participant<W> {
    #[inline]
    pub(crate) fn is_baton(&self) -> bool {
        matches!(self, Self::Baton(_))
    }

    #[inline]
    pub(crate) fn is_parallel(&self) -> bool {
        matches!(self, Self::Parallel(_))
    }

    #[inline]
    pub(crate) fn as_baton(&self) -> Option<&mpsc::SyncSender<W>> {
        match self {
            Self::Baton(sender) => Some(sender),
            Self::Parallel(_) => None,
        }
    }

    #[inline]
    pub(crate) fn as_parallel(&self) -> Option<&ParallelParticipant> {
        match self {
            Self::Parallel(parallel) => Some(parallel),
            Self::Baton(_) => None,
        }
    }
}
