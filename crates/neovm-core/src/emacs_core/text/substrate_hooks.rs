//! Host-installed hooks for the text layer's host-free engines (stage-3b C7).
//!
//! The regex compiler and the literal-find/search fast paths run without a
//! `Context` in scope.  The evaluator publishes the hot-path timing service
//! they need once, at construction: the enable switch and the per-operation
//! recorder.  Everything here is static — a bool and plain `fn` pointers,
//! never `dyn` — and no TLS is added.
//!
//! With tracing off, [`time_op`] costs one inlined bool load and a branch; the
//! record indirect call exists only on the enabled path.  Timing stays
//! strictly per-operation: [`time_op`] wraps one whole regex or search
//! operation, never a per-character or per-backtrack step.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// One measured text operation: the text layer's slice of
/// `emacs_core::perf_trace::HotpathOp`.
///
/// Core maps each variant back to its `HotpathOp` counter when it installs the
/// recorder; this enum is the seam's only vocabulary.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub enum TextPerfOp {
    RegexCompileHit,
    RegexCompileMiss,
    RegexLiteralFind,
    RegexMatchDataChars,
    RegexpQuote,
}

/// Whether hot-path timing is enabled.  Set once by the evaluator from
/// `perf_trace::hotpath_timing_enabled()`; false until then.
static PERF_ENABLED: OnceLock<bool> = OnceLock::new();

/// Per-operation recorder, installed once by the evaluator.
static PERF_RECORD: OnceLock<fn(TextPerfOp, Duration)> = OnceLock::new();

#[inline(always)]
fn perf_enabled() -> bool {
    PERF_ENABLED.get().copied().unwrap_or(false)
}

/// Install the hot-path timing pair.
///
/// `enabled` is the evaluator's existing `hotpath_timing_enabled()` answer;
/// `record` is the core adapter that maps [`TextPerfOp`] to the existing
/// `HotpathOp` counters.  Idempotent.
#[doc(hidden)]
pub fn install_perf(enabled: bool, record: fn(TextPerfOp, Duration)) {
    let _ = PERF_ENABLED.set(enabled);
    let _ = PERF_RECORD.set(record);
}

/// Measure one whole text operation, unless hot-path timing is disabled.
///
/// Mirrors `perf_trace::time_op`: the enable test runs before the clock read,
/// so the disabled path is one inlined bool load and a branch.
#[inline]
pub fn time_op<R>(op: TextPerfOp, f: impl FnOnce() -> R) -> R {
    if !perf_enabled() {
        return f();
    }
    let start = Instant::now();
    let result = f();
    if let Some(record) = PERF_RECORD.get() {
        record(op, start.elapsed());
    }
    result
}
