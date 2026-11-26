//! Structural timing primitives for TUI tests.
//!
//! These types are the only sanctioned way for TUI tests to bound a wait.
//! Their constructors carry the runtime floors that enforce the wall-clock
//! policy; `wall_clock_guard_test` is a static tripwire that keeps raw clock
//! reads out of the rest of the crate.
//!
//! # Observable-time invariant
//!
//! No value a caller can obtain from this module distinguishes elapsed times
//! shorter than `MIN_HANG_CAP - MAX_POLL_SLICE` (4.5 s):
//!
//! - [`HangCap::is_expired`] flips only after the cap, which is at least
//!   [`MIN_HANG_CAP`].
//! - [`HangCap::poll_slice`] returns at most `max_slice <= MAX_POLL_SLICE`, so
//!   the strongest assertion it supports is "at least `MAX_POLL_SLICE`
//!   remains". The remaining time itself is never exposed.
//! - [`NegativeSoakReceipt`] carries the last sample only, not a sample count.
//!   A count over a fixed window measures how fast the sampler runs.
//! - [`unique_nonce`] is not derived from a clock.

use std::collections::hash_map::RandomState;
use std::fmt::Display;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Minimum allowed duration for a hang-avoidance cap.
///
/// Anything shorter risks turning a polling loop into a load-sensitive speed
/// gate on a busy host.
pub const MIN_HANG_CAP: Duration = Duration::from_secs(5);

/// Minimum allowed observation window for a negative soak.
pub const MIN_NEGATIVE_SOAK: Duration = Duration::from_secs(1);

/// Largest slice [`HangCap::poll_slice`] hands out.
pub const MAX_POLL_SLICE: Duration = Duration::from_millis(500);

const _: () = assert!(
    MAX_POLL_SLICE.as_nanos() < MIN_HANG_CAP.as_nanos(),
    "a poll slice must never be long enough to seed another HangCap"
);

/// Declared hang-avoidance cap for a polling or exit wait.
///
/// Enforces `timeout >= MIN_HANG_CAP`. A cap is shared by reference across
/// every wait it bounds; the remaining time is never exposed. A helper that
/// takes a fresh `Duration` and constructs its own cap re-floors the
/// remainder and panics early. That was the shape of the `HangCap::new(cap.remaining())`
/// regression.
#[derive(Debug, Clone, Copy)]
pub struct HangCap {
    deadline: Instant,
    timeout: Duration,
}

impl HangCap {
    #[track_caller]
    pub fn new(timeout: Duration) -> Self {
        assert!(
            timeout >= MIN_HANG_CAP,
            "HangCap requires timeout >= {MIN_HANG_CAP:?}, got {timeout:?}; \
             assert state or protocol receipts instead of a short wall-clock deadline"
        );
        Self {
            deadline: Instant::now() + timeout,
            timeout,
        }
    }

    #[track_caller]
    pub fn from_secs(secs: u64) -> Self {
        Self::new(Duration::from_secs(secs))
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// The next bounded wait slice: `None` once the cap has expired,
    /// otherwise `min(remaining, max_slice)`.
    ///
    /// `max_slice` must be non-zero and at most [`MAX_POLL_SLICE`], so the
    /// result can neither seed a new [`HangCap`] nor reveal more than
    /// "`max_slice` still remains".
    #[track_caller]
    pub fn poll_slice(&self, max_slice: Duration) -> Option<Duration> {
        assert!(
            !max_slice.is_zero() && max_slice <= MAX_POLL_SLICE,
            "HangCap::poll_slice requires 0 < max_slice <= {MAX_POLL_SLICE:?}, got {max_slice:?}"
        );
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        (!remaining.is_zero()).then(|| remaining.min(max_slice))
    }

    /// [`Self::poll_slice`], panicking with `context` once the cap expired.
    #[track_caller]
    pub fn expect_poll_slice(&self, max_slice: Duration, context: impl Display) -> Duration {
        match self.poll_slice(max_slice) {
            Some(slice) => slice,
            None => panic!("hang-avoidance cap ({:?}) expired: {context}", self.timeout),
        }
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub(crate) fn deadline_instant(&self) -> Instant {
        self.deadline
    }

    #[track_caller]
    pub fn assert_not_expired(&self, context: impl Display) {
        assert!(
            !self.is_expired(),
            "hang-avoidance cap ({:?}) expired: {context}",
            self.timeout
        );
    }
}

/// Receipt produced by a completed [`NegativeSoak`] observation window.
///
/// Carries only the last observed state receipt. Every receipt implies at
/// least one sample. It carries no sample count, deadline predicate or
/// elapsed duration, because each of those measures speed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegativeSoakReceipt<R> {
    last_receipt: R,
}

impl<R> NegativeSoakReceipt<R> {
    pub fn last_receipt(&self) -> &R {
        &self.last_receipt
    }
}

/// Non-failing observation window confirming that a forbidden state does NOT
/// appear during a bounded soak of at least [`MIN_NEGATIVE_SOAK`] (1 s).
///
/// Structurally non-invertible: exposes no deadline boolean and no elapsed
/// duration, so it cannot be repurposed as a wall-clock completion-speed
/// assertion.
pub struct NegativeSoak;

impl NegativeSoak {
    /// Repeatedly invoke `sample` across at least `window` (>= 1 s), sleeping
    /// `poll_interval` between samples, and return a [`NegativeSoakReceipt`].
    #[track_caller]
    pub fn observe<R>(
        window: Duration,
        poll_interval: Duration,
        mut sample: impl FnMut() -> R,
    ) -> NegativeSoakReceipt<R> {
        assert!(
            window >= MIN_NEGATIVE_SOAK,
            "NegativeSoak requires window >= {MIN_NEGATIVE_SOAK:?}, got {window:?}"
        );
        let deadline = Instant::now() + window;
        loop {
            let receipt = sample();
            if Instant::now() >= deadline {
                return NegativeSoakReceipt {
                    last_receipt: receipt,
                };
            }
            std::thread::sleep(poll_interval);
        }
    }
}

/// Unique nonce for isolated artifact directory names.
///
/// Combines the process id, a process-local counter and per-process OS
/// randomness (the `RandomState` seed). It is deliberately not derived from a
/// clock, so two nonces cannot be subtracted into an elapsed time.
pub fn unique_nonce() -> u128 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let entropy = RandomState::new().hash_one((std::process::id(), sequence));
    (u128::from(entropy) << 64)
        | (u128::from(std::process::id()) << 32)
        | u128::from(sequence as u32)
}

/// Deterministic file modification timestamp relative to `UNIX_EPOCH` for
/// filesystem mtime fixtures. Never reads the host clock.
pub fn fixed_file_mtime(secs_since_epoch: u64, subsec_nanos: u32) -> SystemTime {
    UNIX_EPOCH + Duration::new(secs_since_epoch, subsec_nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hang_cap_accepts_minimum_and_above() {
        let cap = HangCap::new(MIN_HANG_CAP);
        assert_eq!(cap.timeout(), MIN_HANG_CAP);
        assert!(!cap.is_expired());
        assert_eq!(cap.poll_slice(MAX_POLL_SLICE), Some(MAX_POLL_SLICE));
        cap.assert_not_expired("fresh cap must not expire");
    }

    #[test]
    #[should_panic(expected = "HangCap requires timeout >=")]
    fn hang_cap_rejects_short_speed_deadline() {
        let _ = HangCap::new(Duration::from_millis(250));
    }

    /// The leuven harness regression: one 8 s input budget was shared across
    /// two waits by re-wrapping `remaining()` in `HangCap::new`, which
    /// panicked as soon as less than `MIN_HANG_CAP` remained (about 3 s in).
    /// A shared cap whose remainder is below the floor must keep serving
    /// slices until the cap itself expires.
    #[test]
    fn shared_cap_keeps_serving_slices_below_the_floor() {
        let cap = HangCap::new(MIN_HANG_CAP + Duration::from_millis(20));
        std::thread::sleep(Duration::from_millis(60));
        assert!(!cap.is_expired());
        let slice = cap.expect_poll_slice(MAX_POLL_SLICE, "shared cap below the floor");
        assert!(slice <= MAX_POLL_SLICE);
    }

    #[test]
    #[should_panic(expected = "HangCap requires timeout >=")]
    fn poll_slice_cannot_seed_another_cap() {
        let cap = HangCap::new(MIN_HANG_CAP);
        let _ = HangCap::new(cap.expect_poll_slice(MAX_POLL_SLICE, "fresh cap"));
    }

    #[test]
    #[should_panic(expected = "HangCap::poll_slice requires 0 < max_slice <=")]
    fn poll_slice_rejects_slices_above_the_maximum() {
        let cap = HangCap::new(MIN_HANG_CAP);
        let _ = cap.poll_slice(MAX_POLL_SLICE + Duration::from_millis(1));
    }

    #[test]
    #[should_panic(expected = "HangCap::poll_slice requires 0 < max_slice <=")]
    fn poll_slice_rejects_zero_slices() {
        let cap = HangCap::new(MIN_HANG_CAP);
        let _ = cap.poll_slice(Duration::ZERO);
    }

    #[test]
    #[should_panic(expected = "NegativeSoak requires window >=")]
    fn negative_soak_rejects_short_window() {
        let _ = NegativeSoak::observe(Duration::from_millis(250), Duration::from_millis(10), || ());
    }

    /// Every session wait derives its deadline through `HangCap::new`, so
    /// a short bound panics before any I/O whatever expression built it.
    mod session_floors {
        use super::*;
        use crate::{TuiLaunch, TuiSession, pair};

        fn idle_shell(name: &str) -> TuiSession {
            TuiSession::spawn_launch(TuiLaunch::new("sh").args(["-c", "sleep 30"]), name)
        }

        const SHORT: Duration = Duration::from_millis(200);

        #[test]
        #[should_panic(expected = "HangCap requires timeout >=")]
        fn read_until() {
            idle_shell("FLOOR-READ-UNTIL").read_until(SHORT, |_| true);
        }

        #[test]
        #[should_panic(expected = "HangCap requires timeout >=")]
        fn read_until_screen() {
            idle_shell("FLOOR-READ-UNTIL-SCREEN").read_until_screen(SHORT, |_, _| true);
        }

        #[test]
        #[should_panic(expected = "HangCap requires timeout >=")]
        fn run_to_completion() {
            let _ = idle_shell("FLOOR-RUN-TO-COMPLETION").run_to_completion(SHORT);
        }

        #[test]
        #[should_panic(expected = "HangCap requires timeout >=")]
        fn wait_for_raw_output() {
            let _ = idle_shell("FLOOR-RAW-OUTPUT").wait_for_raw_output(b"marker", SHORT);
        }

        #[test]
        #[should_panic(expected = "HangCap requires timeout >=")]
        fn eval_expression_one_within() {
            pair::eval_expression_one_within(&mut idle_shell("FLOOR-EVAL-ONE"), "nil", SHORT);
        }

        #[test]
        #[should_panic(expected = "PairTimeout::same requires a hang-cap >= MIN_HANG_CAP")]
        fn pair_timeout_same() {
            let _ = crate::package_scenario::PairTimeout::same(SHORT);
        }

        #[test]
        #[should_panic(expected = "PairTimeout::per_editor requires hang-caps >= MIN_HANG_CAP")]
        fn pair_timeout_per_editor() {
            let _ = crate::package_scenario::PairTimeout::per_editor(MIN_HANG_CAP, SHORT);
        }

        #[test]
        #[should_panic(expected = "HangCap requires timeout >=")]
        fn wait_for_both() {
            let mut gnu = idle_shell("FLOOR-BOTH-A");
            let mut neo = idle_shell("FLOOR-BOTH-B");
            pair::wait_for_both(&mut gnu, &mut neo, SHORT, |_| true);
        }
    }

    #[test]
    fn unique_nonces_are_distinct_within_a_process() {
        let first = unique_nonce();
        let second = unique_nonce();
        assert_ne!(first, 0);
        assert_ne!(first, second);
    }
}
