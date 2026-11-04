//! The cross-thread quit request and the process-wide asynchronous attention
//! word (stage-3b T4.1: moved verbatim from the evaluator).
//!
//! GNU's safe point is `if (!NILP (Vquit_flag) || pending_signals)
//! probably_quit ();` (src/lisp.h:3897-3901): two loads, and everything that
//! can need attention arrives through one of the two words.  This module owns
//! the asynchronous word and the input bridges' quit request; the
//! evaluator's per-`Context` word (`AttentionBit`, `AttentionMask`,
//! `attention_of`) stays in `emacs_core::eval`.
//!
//! The thread-local slot is the ONE definition, moved out of the evaluator's
//! `thread_local!` block: its TLS table entry is re-pathed here, not added to,
//! so PT_TLS size is unchanged.  The evaluator keeps its paths through
//! re-exports (`QuitRequest`, `ASYNC_ATTENTION`, `AsyncSource`,
//! `tls_quit_pending`).

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// A process-wide asynchronous poll source: GNU's `pending_signals`
/// (src/keyboard.c:105) generalized to the things that arrive from other
/// threads or from signal context.  Each owns one bit of [`ASYNC_ATTENTION`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AsyncSource {
    /// The Lisp profiler's watchdog asked for a sample
    /// (`Context::profiler_sample_tick` consumes it).
    ProfilerTick = 1 << 0,
    /// A handled OS signal is pending (`os_signal::drain_pending_os_signals`
    /// consumes it).
    OsSignal = 1 << 1,
}

/// Bits 8..32 of [`ASYNC_ATTENTION`] count the raised [`QuitRequest`]s of the
/// process. Only a lowered-to-raised transition adds a unit and only a
/// raised-to-lowered one removes it, so the count never drifts.
const QUIT_REQUEST_UNIT: u32 = 1 << 8;

/// The word behind [`ASYNC_ATTENTION`], on a cache line of its own: other
/// threads and signal handlers write it, the evaluator reads it at every
/// safe point.
#[repr(C, align(64))]
#[doc(hidden)]
pub struct AsyncAttention {
    word: AtomicU32,
}

/// Nonzero while any asynchronous source may need a safe point's attention.
/// A set bit is a hint: the slow path re-derives the exact answer from each
/// source's own state, so a stale bit costs one cold trip, never a lost event.
pub static ASYNC_ATTENTION: AsyncAttention = AsyncAttention {
    word: AtomicU32::new(0),
};

impl AsyncAttention {
    /// The safe point's fast test: zero means nothing asynchronous is due.
    #[inline(always)]
    pub fn load(&self) -> u32 {
        self.word.load(Ordering::Relaxed)
    }

    /// Raise SOURCE.  One lock-free atomic RMW (`lock or`), so it is
    /// async-signal-safe: `os_signal`'s handler calls it.
    ///
    /// Signed exact-caller row: the profiler watchdog and the OS-signal
    /// bridge.
    #[doc(hidden)]
    #[inline(always)]
    pub fn raise(&self, source: AsyncSource) {
        self.word.fetch_or(source as u32, Ordering::Release);
    }

    /// Lower SOURCE; true when it was raised.
    ///
    /// Signed exact-caller row: the profiler drain and the OS-signal drain.
    #[doc(hidden)]
    #[inline]
    pub fn take(&self, source: AsyncSource) -> bool {
        self.word.fetch_and(!(source as u32), Ordering::AcqRel) & source as u32 != 0
    }

    /// Whether SOURCE is raised, without consuming it.
    #[inline(always)]
    pub fn is_raised(&self, source: AsyncSource) -> bool {
        self.load() & source as u32 != 0
    }

    /// The word's address, for compiled code that inlines the safe point's
    /// fast test.  Only JIT code may bake it (an AOT artifact outlives the
    /// process whose address it would carry).
    ///
    /// Signed exact-caller row: the JIT lowering paths.
    #[doc(hidden)]
    pub fn addr(&self) -> usize {
        std::ptr::from_ref(&self.word) as usize
    }

    /// How many raised quit requests the word counts.
    #[doc(hidden)]
    #[cfg(any(test, feature = "test-support"))]
    pub fn raised_quit_requests_for_test(&self) -> u32 {
        self.load() / QUIT_REQUEST_UNIT
    }
}

/// The cross-thread C-g request.
///
/// GNU's `handle_interrupt` writes `Vquit_flag = Qt` from the signal handler
/// (src/keyboard.c:12719).  Neomacs's input-bridge threads cannot reach the
/// `Context`, so they raise this instead, and the next safe point drains it
/// into `quit-flag` (`Context::maybe_quit_slow`).
///
/// The flag itself is private: [`QuitRequest::request`] is the only way
/// another thread can raise it, and it also counts the raise in
/// [`ASYNC_ATTENTION`], which is the word every safe point tests.
#[derive(Clone, Debug)]
pub struct QuitRequest(Arc<QuitRequestCell>);

#[derive(Debug)]
struct QuitRequestCell {
    raised: AtomicBool,
}

impl QuitRequest {
    /// A lowered request.
    pub fn new() -> Self {
        Self(Arc::new(QuitRequestCell {
            raised: AtomicBool::new(false),
        }))
    }

    /// Raise the request (the input-bridge threads, on a C-g).
    ///
    /// The flag goes up before the count, so a safe point that sees the
    /// count finds the flag; one that polls in between notices the request
    /// at its next poll, which is GNU's own latency ("the next maybe_quit").
    pub fn request(&self) {
        if !self.0.raised.swap(true, Ordering::AcqRel) {
            ASYNC_ATTENTION
                .word
                .fetch_add(QUIT_REQUEST_UNIT, Ordering::Release);
        }
    }

    /// Whether the request is raised, without consuming it.
    ///
    /// Signed exact-caller rows besides the poll reader: the evaluator's
    /// quit-request tests.
    #[doc(hidden)]
    #[inline(always)]
    pub fn is_requested(&self) -> bool {
        self.0.raised.load(Ordering::Relaxed)
    }

    /// Consume the request on the evaluator thread: true when it was raised.
    ///
    /// Signed exact-caller row: `Context::maybe_quit_slow`.
    #[doc(hidden)]
    #[inline]
    pub fn take(&self) -> bool {
        if self.0.raised.swap(false, Ordering::AcqRel) {
            ASYNC_ATTENTION
                .word
                .fetch_sub(QUIT_REQUEST_UNIT, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Lower the request without looking at it.
    ///
    /// Signed exact-caller row: `Context::maybe_quit_slow`'s clear path.
    #[doc(hidden)]
    #[inline]
    pub fn clear(&self) {
        let _ = self.take();
    }
}

impl Default for QuitRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for QuitRequestCell {
    /// The last handle is gone while the request is raised: nobody can take
    /// it any more, so release its unit of [`ASYNC_ATTENTION`].
    fn drop(&mut self) {
        if *self.raised.get_mut() {
            ASYNC_ATTENTION
                .word
                .fetch_sub(QUIT_REQUEST_UNIT, Ordering::Release);
        }
    }
}

thread_local! {
    /// Thread-local handle to the active `Context::quit_requested`
    /// atomic. Installed by `Context::setup_thread_locals`, read by
    /// leaf functions (e.g. the regex matcher) that need a cheap quit
    /// check without threading `&mut Context` through their signature.
    /// Mirrors the call site shape of GNU's `maybe_quit()` — reachable
    /// from anywhere without an explicit context pointer.
    ///
    /// This is the one definition of the slot (stage-3b T4.1); the
    /// evaluator's TLS block no longer declares it.
    static QUIT_REQUESTED_TLS: RefCell<Option<QuitRequest>> = const { RefCell::new(None) };
}

/// Check whether a quit is pending without needing `&mut Context`.
/// The regex matcher calls this at jump/fail sites, mirroring GNU's
/// `regex-emacs.c:4901,5236`. When it returns `true`, the caller
/// should unwind its work so the next `maybe_quit()` poll can promote
/// the pending flag to a `quit` signal.
#[inline]
pub fn quit_pending() -> bool {
    QUIT_REQUESTED_TLS.with(|cell| {
        cell.borrow()
            .as_ref()
            .is_some_and(QuitRequest::is_requested)
    })
}

/// Install the active quit-request flag on this thread.
///
/// Signed exact-caller row: `Context::setup_thread_locals_inner`
/// (`gc_pacing.rs`).  Same slot and body as the evaluator-local write this
/// replaces.
#[doc(hidden)]
pub fn install_quit_requested_tls(flag: QuitRequest) {
    QUIT_REQUESTED_TLS.with(|cell| *cell.borrow_mut() = Some(flag));
}

/// Forget the thread's quit-request flag.
///
/// Signed exact-caller row: `carrier park` (`eval::park_carrier_tls`).
#[doc(hidden)]
pub fn clear_quit_requested_tls() {
    QUIT_REQUESTED_TLS.with(|slot| *slot.borrow_mut() = None);
}

/// Mix the slot's installed-ness into the carrier TLS fingerprint.
///
/// Signed exact-caller row: `eval::carrier_tls_fingerprint`.  The slot is the
/// same one the evaluator fingerprinted before the move.
#[doc(hidden)]
pub fn mix_quit_requested_tls_fingerprint(hash: &mut u64) {
    crate::tls_fingerprint::mix(
        hash,
        QUIT_REQUESTED_TLS.with(|slot| slot.borrow().is_some() as u64),
    );
}

/// Install a quit-request flag for the current thread. CCL polls this the
/// way GNU's driver polls `Vquit_flag`. The flag is not cleared by CCL.
///
/// Engine tests (CCL, DFA) use this without a `Context`; the crate-boundary
/// rule gates the helper on `test-support` because plain `cfg(test)` is off
/// for a dependency crate.
#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub fn install_quit_requested_for_test(pending: bool) -> QuitRequest {
    let flag = QuitRequest::new();
    if pending {
        flag.request();
    }
    QUIT_REQUESTED_TLS.with(|cell| *cell.borrow_mut() = Some(flag.clone()));
    flag
}

#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub fn clear_quit_requested_for_test() {
    QUIT_REQUESTED_TLS.with(|cell| *cell.borrow_mut() = None);
}
