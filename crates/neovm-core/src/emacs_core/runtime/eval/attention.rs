//! Poll sources that send a safe point off its loads-only fast path.
//!
//! GNU's safe point is `if (!NILP (Vquit_flag) || pending_signals)
//! probably_quit ();` (src/lisp.h:3897-3901): two loads, and everything that
//! can need attention arrives through one of the two words.  This module holds
//! neomacs's counterparts.
//!
//! * [`ASYNC_ATTENTION`], one process-wide word for everything that arrives
//!   from another thread or from signal context: the Lisp profiler's tick, a
//!   handled OS signal, and raised [`QuitRequest`]s.  It is GNU's
//!   `pending_signals` (src/keyboard.c:105), a hint the slow path re-derives.
//!
//! A child module of `eval`, like its siblings, so it keeps the same view of
//! `Context` and the parent's private items (`use super::*`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// A process-wide asynchronous poll source: GNU's `pending_signals`
/// (src/keyboard.c:105) generalized to the things that arrive from other
/// threads or from signal context.  Each owns one bit of [`ASYNC_ATTENTION`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum AsyncSource {
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
pub(crate) struct AsyncAttention {
    word: AtomicU32,
}

/// Nonzero while any asynchronous source may need a safe point's attention.
/// A set bit is a hint: the slow path re-derives the exact answer from each
/// source's own state, so a stale bit costs one cold trip, never a lost event.
pub(crate) static ASYNC_ATTENTION: AsyncAttention = AsyncAttention {
    word: AtomicU32::new(0),
};

impl AsyncAttention {
    /// The safe point's fast test: zero means nothing asynchronous is due.
    #[inline(always)]
    pub(crate) fn load(&self) -> u32 {
        self.word.load(Ordering::Relaxed)
    }

    /// Raise SOURCE.  One lock-free atomic RMW (`lock or`), so it is
    /// async-signal-safe: `os_signal`'s handler calls it.
    #[inline(always)]
    pub(crate) fn raise(&self, source: AsyncSource) {
        self.word.fetch_or(source as u32, Ordering::Release);
    }

    /// Lower SOURCE; true when it was raised.
    #[inline]
    pub(crate) fn take(&self, source: AsyncSource) -> bool {
        self.word.fetch_and(!(source as u32), Ordering::AcqRel) & source as u32 != 0
    }

    /// Whether SOURCE is raised, without consuming it.
    #[inline(always)]
    pub(crate) fn is_raised(&self, source: AsyncSource) -> bool {
        self.load() & source as u32 != 0
    }

    /// The word's address, for compiled code that inlines the safe point's
    /// fast test.  Only JIT code may bake it (an AOT artifact outlives the
    /// process whose address it would carry).
    pub(crate) fn addr(&self) -> usize {
        std::ptr::from_ref(&self.word) as usize
    }

    /// How many raised quit requests the word counts.
    #[cfg(test)]
    pub(crate) fn raised_quit_requests_for_test(&self) -> u32 {
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
    #[inline(always)]
    pub(crate) fn is_requested(&self) -> bool {
        self.0.raised.load(Ordering::Relaxed)
    }

    /// Consume the request on the evaluator thread: true when it was raised.
    #[inline]
    pub(crate) fn take(&self) -> bool {
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
    #[inline]
    pub(crate) fn clear(&self) {
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

#[cfg(test)]
#[path = "tests/quit_request.rs"]
mod quit_request_tests;
