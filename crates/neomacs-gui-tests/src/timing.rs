//! Structural timing primitives for GUI tests.
//!
//! These types are the only sanctioned way for GUI tests to bound a wait.
//! Their constructors carry the runtime floors that enforce the wall-clock
//! policy; `wall_clock_guard_test` is a static tripwire that keeps raw clock
//! reads out of the rest of the crate.
//!
//! # Observable-time invariant
//!
//! No numeric value a caller can obtain from this module distinguishes
//! elapsed times shorter than [`MIN_HANG_CAP`]:
//!
//! - [`HangCap`] exposes expiry only, and only after at least `MIN_HANG_CAP`.
//!   It does not expose the remaining time.
//! - [`StabilityHold::wait_until_stable`] returns `()` or panics on its cap.
//! - [`NegativeSoakReceipt`] carries the last sample only, not a sample count.
//!   A count over a fixed window measures how fast the sampler runs.
//! - [`unique_nonce`] is not derived from a clock.
//!
//! The diagnostic labels ([`HangCap::diagnostic_elapsed_label`],
//! [`DiagnosticStopwatch`], [`diagnostic_unix_ns_label`]) are text written
//! into post-mortem artifacts. Text is still parseable, so the guard
//! restricts their call sites to an exact `(file, fn)` allowlist.

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

/// Declared hang-avoidance cap for a polling or exit wait.
///
/// Enforces `timeout >= MIN_HANG_CAP` and exposes only expiry queries. Share
/// one cap by reference across the waits it bounds instead of re-deriving a
/// fresh timeout from a remainder.
#[derive(Debug, Clone, Copy)]
pub struct HangCap {
    started: Instant,
    deadline: Instant,
    timeout: Duration,
}

impl HangCap {
    #[track_caller]
    pub fn new(timeout: Duration) -> Self {
        assert!(
            timeout >= MIN_HANG_CAP,
            "HangCap requires timeout >= {MIN_HANG_CAP:?}, got {timeout:?}; \
             assert state or presentation receipts instead of a short wall-clock deadline"
        );
        let started = Instant::now();
        Self {
            started,
            deadline: started + timeout,
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

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Human-readable elapsed label for post-mortem artifact JSON only.
    /// Returns a formatted string rather than a numeric duration so it cannot
    /// be compared against a latency threshold.
    pub fn diagnostic_elapsed_label(&self) -> String {
        format!("{:.3}ms", self.started.elapsed().as_secs_f64() * 1000.0)
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

/// Minimum continuous-hold wait required before a state is considered settled
/// (e.g. a window manager holding steady geometry for >= 350 ms).
///
/// Structurally non-invertible: returns `()` only once `condition_met()` has
/// held continuously for at least `hold`, or panics via the caller's `>= 5s`
/// [`HangCap`]. Exposes no boolean predicate and no elapsed duration, so it
/// cannot be inverted into a sub-second completion-speed assertion.
pub struct StabilityHold;

impl StabilityHold {
    #[track_caller]
    pub fn wait_until_stable(
        cap: &HangCap,
        hold: Duration,
        poll_interval: Duration,
        context: impl Display,
        mut condition_met: impl FnMut() -> bool,
    ) {
        let mut since: Option<Instant> = None;
        loop {
            if condition_met() {
                let start = since.get_or_insert_with(Instant::now);
                if start.elapsed() >= hold {
                    return;
                }
            } else {
                since = None;
            }
            cap.assert_not_expired(&context);
            std::thread::sleep(poll_interval);
        }
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
/// Structurally non-invertible: exposes no deadline boolean, no early-exit
/// predicate, and no elapsed duration, so it cannot be repurposed as a
/// wall-clock completion-speed assertion.
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

/// Wall-clock nanoseconds since the Unix epoch, as text, for post-mortem
/// artifact metadata only. Call sites are restricted by the wall-clock guard.
pub fn diagnostic_unix_ns_label() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_nanos().to_string())
        .unwrap_or_else(|_| "before-unix-epoch".to_owned())
}

/// Diagnostic-only stopwatch that formats a log line for post-mortem artifacts
/// without exposing any numeric duration or timestamp to test assertions.
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticStopwatch {
    started_epoch_secs: f64,
    started: Instant,
}

impl DiagnosticStopwatch {
    pub fn start() -> Self {
        Self {
            started_epoch_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
            started: Instant::now(),
        }
    }

    pub fn format_log_entry(&self, label: &str) -> String {
        format!(
            "{:.6} {:.3} {label}",
            self.started_epoch_secs,
            self.started.elapsed().as_secs_f64()
        )
    }
}

/// Assert that a presentation receipt contains a valid monotonic
/// `input_to_present_ns` reading (presence only; never compares against a
/// wall-clock threshold).
#[track_caller]
pub fn assert_present_receipt_has_monotonic_clock(
    sample: &serde_json::Value,
    context: impl Display,
) {
    assert!(
        sample["input_to_present_ns"].as_u64().is_some(),
        "clock mismatch: {context}"
    );
}

/// Assert the load-invariant causal ordering `projected_present < present` on
/// a presentation receipt without exposing raw latency-key indexing to tests.
#[track_caller]
pub fn assert_projected_precedes_present(sample: &serde_json::Value, context: impl Display) {
    let projected = sample["input_to_projected_present_ns"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing input_to_projected_present_ns: {context}"));
    let presented = sample["input_to_present_ns"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing input_to_present_ns: {context}"));
    assert!(
        projected < presented,
        "projected presentation ({projected}ns) did not precede actual presentation ({presented}ns): {context}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hang_cap_accepts_minimum_and_above() {
        let cap = HangCap::new(MIN_HANG_CAP);
        assert_eq!(cap.timeout(), MIN_HANG_CAP);
        assert!(!cap.is_expired());
        assert!(cap.diagnostic_elapsed_label().ends_with("ms"));
        cap.assert_not_expired("fresh cap must not expire");
    }

    #[test]
    #[should_panic(expected = "HangCap requires timeout >=")]
    fn hang_cap_rejects_short_speed_deadline() {
        let _ = HangCap::new(Duration::from_millis(500));
    }

    #[test]
    fn stability_hold_waits_for_continuous_condition() {
        let cap = HangCap::new(MIN_HANG_CAP);
        let mut ticks = 0usize;
        StabilityHold::wait_until_stable(
            &cap,
            Duration::from_millis(5),
            Duration::from_millis(1),
            "synthetic hold",
            || {
                ticks += 1;
                ticks >= 2
            },
        );
        assert!(ticks >= 2);
    }

    #[test]
    #[should_panic(expected = "NegativeSoak requires window >=")]
    fn negative_soak_rejects_short_window() {
        let _ = NegativeSoak::observe(Duration::from_millis(200), Duration::from_millis(10), || ());
    }

    #[test]
    #[should_panic(expected = "HangCap requires timeout >=")]
    fn run_input_tool_floors_its_deadline() {
        let _ = crate::run_input_tool(
            std::process::Command::new("true"),
            Duration::from_millis(200),
        );
    }

    #[test]
    #[should_panic(expected = "GuiRunOptions::with_timeout requires timeout >=")]
    fn gui_run_options_rejects_short_timeout() {
        let _ = crate::GuiRunOptions::with_timeout(Duration::from_millis(200));
    }

    #[test]
    fn unique_nonce_is_distinct_and_stopwatch_formats_string() {
        let first = unique_nonce();
        assert_ne!(first, 0);
        assert_ne!(first, unique_nonce());
        assert!(diagnostic_unix_ns_label().parse::<u128>().is_ok());
        let sw = DiagnosticStopwatch::start();
        assert!(sw.format_log_entry("probe.png").ends_with(" probe.png"));
    }
}
