//! Poll sources that send a safe point off its loads-only fast path.
//!
//! GNU's safe point is `if (!NILP (Vquit_flag) || pending_signals)
//! probably_quit ();` (src/lisp.h:3897-3901): two loads, and everything that
//! can need attention arrives through one of the two words.  This module holds
//! neomacs's counterparts.
//!
//! A child module of `eval`, like its siblings, so it keeps the same view of
//! `Context` and the parent's private items (`use super::*`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The cross-thread C-g request.
///
/// GNU's `handle_interrupt` writes `Vquit_flag = Qt` from the signal handler
/// (src/keyboard.c:12719).  Neomacs's input-bridge threads cannot reach the
/// `Context`, so they raise this instead, and the next safe point drains it
/// into `quit-flag` (`Context::maybe_quit_slow`).
///
/// The flag itself is private: [`QuitRequest::request`] is the only way
/// another thread can raise it, which is what lets a safe point trust the
/// raise to be visible wherever it polls.
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
    pub fn request(&self) {
        self.0.raised.store(true, Ordering::Release);
    }

    /// Whether the request is raised, without consuming it.
    #[inline(always)]
    pub(crate) fn is_requested(&self) -> bool {
        self.0.raised.load(Ordering::Relaxed)
    }

    /// Consume the request on the evaluator thread: true when it was raised.
    #[inline]
    pub(crate) fn take(&self) -> bool {
        self.0.raised.swap(false, Ordering::AcqRel)
    }

    /// Lower the request without looking at it.
    #[inline]
    pub(crate) fn clear(&self) {
        let _ = self.take();
    }

    /// How far past the pointer word a request holds its flag, probed on a
    /// live request (the pointer names the shared allocation, whose layout
    /// std does not promise). Compiled code that inlines the poll's fast test
    /// reads the flag at this offset from `Context::quit_requested`'s word.
    /// `None` when the probe cannot find it.
    pub(crate) fn raised_flag_offset() -> Option<usize> {
        const _: () = assert!(std::mem::size_of::<QuitRequest>() == std::mem::size_of::<usize>());
        let request = Self::new();
        // SAFETY: a `QuitRequest` is exactly one `Arc`, which is exactly one
        // non-null pointer word (asserted above).
        let inner: usize = unsafe { std::mem::transmute_copy(&request) };
        let flag = std::ptr::from_ref(&request.0.raised) as usize;
        flag.checked_sub(inner).filter(|&off| off < 64)
    }
}

impl Default for QuitRequest {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/quit_request.rs"]
mod quit_request_tests;
