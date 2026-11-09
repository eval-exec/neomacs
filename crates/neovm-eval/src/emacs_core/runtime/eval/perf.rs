//! Hot-path perf pair and its carrier hooks (stage-4 W4/E8, ruled C7 shape).
//!
//! The evaluator's per-operation timing wrappers (`string-match`, `substring`,
//! `concat`, `format`, `assoc`, …) check one installed bool and branch out
//! when tracing is off — exactly one inlined bool + branch, zero indirect
//! calls on the disabled path. When tracing is on they time the operation and
//! record it through a cold `&'static fn` table installed by core at
//! `Context::new_inner` / `Context::from_dump`.
//!
//! The counters and their thread-local state stay in
//! `system/perf_trace` (E7: core/system TLS never crosses); eval reaches them
//! only through this pair. The carrier hooks (green carrier reset and the
//! debug-only TLS fingerprint) are part of the same installed descriptor.
//!
//! Frequency: per timed operation when enabled; the disabled path is the
//! performance-critical one and performs no indirect call.

use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The timed hot-path operations (mirrors the C7 counter table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfOp {
    StringMatch,
    RegexCompileHit,
    RegexCompileMiss,
    RegexLiteralFind,
    RegexMatchDataChars,
    MatchBeginning,
    MatchEnd,
    RegexpQuote,
    Substring,
    Concat,
    Format,
    Assoc,
}

/// Record one measured operation in the core counters.
pub type PerfRecord = fn(PerfOp, Duration);
/// Drop the core hotpath tracing state on a green carrier park.
pub type PerfCarrierReset = fn();
/// Debug-only TLS fingerprint for the core hotpath records.
pub type PerfCarrierFingerprint = fn(&mut u64);

/// The cold descriptor core publishes at the construction seams.
#[derive(Clone, Copy)]
pub struct PerfHooks {
    record: PerfRecord,
    reset: PerfCarrierReset,
    fingerprint: Option<PerfCarrierFingerprint>,
}

impl PerfHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        record: PerfRecord,
        reset: PerfCarrierReset,
        fingerprint: Option<PerfCarrierFingerprint>,
    ) -> Self {
        Self {
            record,
            reset,
            fingerprint,
        }
    }

    /// Record one operation.
    pub fn record(&self) -> PerfRecord {
        self.record
    }

    /// Drop the core tracing state on a carrier park.
    pub fn reset(&self) -> PerfCarrierReset {
        self.reset
    }

    /// Debug-only fingerprint of the core tracing state.
    pub fn fingerprint(&self) -> Option<PerfCarrierFingerprint> {
        self.fingerprint
    }
}

static PERF_HOOKS: OnceLock<PerfHooks> = OnceLock::new();
static PERF_HOOKS_IDENTITY: OnceLock<&'static PerfHooks> = OnceLock::new();
static PERF_ENABLED: OnceLock<bool> = OnceLock::new();

/// Publish the perf pair and carrier hooks.
///
/// Hooks are published before the enable bool, so an enabled read always
/// implies a published descriptor. Idempotent for the same descriptor and
/// enable answer; a different descriptor (or a different enable answer) is a
/// typed refusal, never a silent overwrite. Called only by the one eval
/// `install_eval_hooks` entry (E3a); core no longer calls this per-table
/// installer (peer follow-up F2).
#[must_use = "handle a conflicting perf publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_perf_hooks(
    enabled: bool,
    hooks: &'static PerfHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    let published = neovm_value::publish_hook_table(&PERF_HOOKS, &PERF_HOOKS_IDENTITY, hooks)?;
    match PERF_ENABLED.set(enabled) {
        Ok(()) => Ok(published),
        Err(_same_answer) if *PERF_ENABLED.get().unwrap_or(&false) == enabled => Ok(published),
        Err(_different_answer) => Err(HookInstallError::DifferentTable),
    }
}

/// The installed descriptor, if core has run one of the two pinned seams.
pub fn perf_hooks() -> Option<&'static PerfHooks> {
    PERF_HOOKS.get()
}

/// Whether hot-path tracing is on. This is the disabled fast-path gate: one
/// installed bool and one branch.
#[inline]
pub fn enabled() -> bool {
    *PERF_ENABLED.get().unwrap_or(&false)
}

/// Time one hot-path operation through the installed pair.
#[inline]
pub fn time_op<R>(op: PerfOp, f: impl FnOnce() -> R) -> R {
    if !enabled() {
        return f();
    }
    let start = Instant::now();
    let result = f();
    if let Some(hooks) = PERF_HOOKS.get() {
        (hooks.record())(op, start.elapsed());
    }
    result
}

/// Record an operation that was already timed by the caller (the text layer's
/// C7 seam checks the enabled bool on its side).
pub fn record_op(op: PerfOp, elapsed: Duration) {
    if let Some(hooks) = PERF_HOOKS.get() {
        (hooks.record())(op, elapsed);
    }
}

/// Drop the core tracing state on a green carrier park.
pub fn reset_carrier_tls() {
    if let Some(hooks) = PERF_HOOKS.get() {
        (hooks.reset())();
    }
}

/// Debug-only fingerprint of the core tracing records for the carrier check.
#[cfg(debug_assertions)]
pub fn carrier_tls_fingerprint(h: &mut u64) {
    if let Some(fingerprint) = PERF_HOOKS.get().and_then(|hooks| hooks.fingerprint()) {
        fingerprint(h);
    }
}
