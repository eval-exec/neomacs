//! Wait boundary for hosts without OS subprocesses.
//!
//! Without a process poller there is nothing to `pselect` on except the host
//! input channel, so every wait here is GNU's `wait_reading_process_output`
//! reduced to its keyboard-and-timer half: service staged host input and due
//! timers, then BLOCK on the input channel for the shortest of the caller's
//! deadline and the next timer, and repeat. Blocking goes through
//! `Context::wait_for_next_host_input_event`, which sleeps when the host has
//! not attached an input channel, so a portable build never busy-spins.
//!
//! Hosts whose main thread cannot block at all (a browser tab) must drive the
//! evaluator from their asynchronous event adapter instead of calling these
//! waits; on `wasm32-unknown-unknown` the standard library's `Instant` and
//! `thread::sleep` abort loudly rather than returning fake progress, which is
//! the intended failure mode until that adapter exists.

#[path = "host_input.rs"]
mod host_input;
pub use host_input::{HostInputWaitBackend, HostInputWaitError};

use std::time::{Duration, Instant};

use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{Context, GnuTimerTimestamp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandInputWaitOutcome {
    InputPending,
    Interrupted,
    DeadlineElapsed,
}

/// Longest single block when neither a deadline nor a timer bounds the wait.
/// The loop re-evaluates timers after every slice, so the bound only caps how
/// late a timer created by another thread can be noticed.
const UNBOUNDED_WAIT_SLICE: Duration = Duration::from_secs(1);

impl Context {
    fn service_portable_input(&mut self, timers: bool, redisplay: bool) -> Result<bool, Flow> {
        let _ = self.stage_next_host_input_event_if_available()?;
        let special = self.service_wait_request_special_input_events()?;
        if timers {
            let _ = self.service_pending_timers_with_wait_policy(redisplay)?;
        }
        if redisplay && special.has_any_activity() {
            self.redisplay();
        }
        Ok(special.has_any_activity())
    }

    /// The timeout GNU's `timer_check` would impose on one wait iteration.
    fn next_portable_timer_timeout(
        &self,
        timer_deadline: Option<GnuTimerTimestamp>,
    ) -> Option<Duration> {
        let ordinary = self.next_ordinary_gnu_timer_timeout_before(timer_deadline);
        let idle = self.next_idle_gnu_timer_timeout();
        match (ordinary, idle) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(timeout), None) | (None, Some(timeout)) => Some(timeout),
            (None, None) => None,
        }
    }

    /// Block once, for at most `limit`, shortened by the next due timer.
    /// Returns whether a host input event arrived.
    fn block_for_portable_input(
        &mut self,
        limit: Option<Duration>,
        timer_deadline: Option<GnuTimerTimestamp>,
        waiting_for_user_input: bool,
    ) -> Result<bool, Flow> {
        let mut timeout = limit.unwrap_or(UNBOUNDED_WAIT_SLICE);
        if let Some(next_timer) = self.next_portable_timer_timeout(timer_deadline) {
            timeout = timeout.min(next_timer);
        }
        self.wait_for_next_host_input_event(timeout, waiting_for_user_input)
    }

    pub(crate) fn service_input_pending_without_timers(&mut self) -> Result<(), Flow> {
        self.service_portable_input(false, false).map(drop)
    }

    pub(crate) fn service_input_pending_with_timers(&mut self) -> Result<(), Flow> {
        self.service_portable_input(true, false).map(drop)
    }

    pub(crate) fn service_input_wait_with_redisplay(&mut self) -> Result<(), Flow> {
        self.service_portable_input(true, true).map(drop)
    }

    pub(crate) fn service_timers_with_redisplay(&mut self) -> Result<(), Flow> {
        let _ = self.service_pending_timers_with_wait_policy(true)?;
        Ok(())
    }

    pub(crate) fn service_timers_without_redisplay(&mut self) -> Result<(), Flow> {
        let _ = self.service_pending_timers_with_wait_policy(false)?;
        Ok(())
    }

    /// GNU `Fsleep_for`: keyboard input does not end the sleep (`read_kbd`
    /// is 0), timers still run, and the full duration elapses.
    pub(crate) fn wait_for_duration_until_timer_deadline(
        &mut self,
        duration: Duration,
        timer_deadline: GnuTimerTimestamp,
    ) -> Result<(), Flow> {
        let deadline = Instant::now() + duration;
        loop {
            let _ = self.service_portable_input(true, false)?;
            let now = Instant::now();
            if now >= deadline {
                return Ok(());
            }
            let _ =
                self.block_for_portable_input(Some(deadline - now), Some(timer_deadline), false)?;
        }
    }

    pub(crate) fn wait_for_resize_ack_until(&mut self, deadline: Instant) -> Result<bool, Flow> {
        loop {
            let _ = self.stage_next_host_input_event_if_available()?;
            if self
                .service_wait_request_special_input_events()?
                .has_resize_activity()
            {
                return Ok(true);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            let _ = self.block_for_portable_input(Some(deadline - now), None, false)?;
        }
    }

    /// GNU `read_char`'s wait: return as soon as command input is staged,
    /// when the deadline passes, or when display maintenance (resize, focus)
    /// needs the command loop's attention.
    pub(crate) fn wait_for_command_input(
        &mut self,
        deadline: Option<Instant>,
    ) -> Result<CommandInputWaitOutcome, Flow> {
        loop {
            let _ = self.stage_next_host_input_event_if_available()?;
            let special_activity = self.service_portable_input(true, false)?;
            if self.stage_pending_command_input_for_wait_request()? {
                return Ok(CommandInputWaitOutcome::InputPending);
            }
            if special_activity {
                return Ok(CommandInputWaitOutcome::Interrupted);
            }
            let now = Instant::now();
            let limit = match deadline {
                Some(deadline) if deadline <= now => {
                    return Ok(CommandInputWaitOutcome::DeadlineElapsed);
                }
                Some(deadline) => Some(deadline - now),
                None => None,
            };
            let _ = self.block_for_portable_input(limit, None, true)?;
        }
    }
}
