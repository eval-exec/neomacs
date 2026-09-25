//! Always-on metering of synchronous JIT compile stalls.
//!
//! The JIT compiles on the eval thread at the first hot call (the cache-miss
//! path in [`super::cache`]), so every compile is a stall the caller feels.
//! This module aggregates how long those stalls are — the evidence base for
//! sizing (or rejecting) background compilation. A compile happens once per
//! function per thread per session (cold path), so the two `Instant` reads per
//! compile are negligible and the metering is unconditionally on.
//!
//! # The report channel
//!
//! Every `[neovm-jit-*]` line goes through [`report_line`]: stderr by
//! default, or the file named by `NEOVM_JIT_STATS_FILE=<path>` (appended).
//! These lines are a measurement REPORT a knob explicitly asked for, not a
//! diagnostic log, so they bypass `tracing` on purpose: under the neomacs
//! subscriber the default filter is `warn`, `LogTarget::Stdout` writes to
//! stdout (which would corrupt batch benchmark output) and `LogTarget::File`
//! is silent — a tracing-only report would vanish or pollute. Nothing is
//! printed unless a knob asks for it, and stdout is never written.

use std::cell::Cell;
use std::io::Write;
use std::sync::OnceLock;
use std::time::Duration;

use super::compile::{CompileError, CompiledLeaf};

/// Upper bounds (exclusive, µs) of the first seven histogram buckets; the
/// eighth bucket is everything >= 10ms.
const BUCKET_LIMITS_US: [u64; 7] = [100, 250, 500, 1_000, 2_500, 5_000, 10_000];

/// Which [`CompileStats::histogram_us`] bucket a stall of `us` lands in.
pub(crate) fn bucket_index(us: u64) -> usize {
    BUCKET_LIMITS_US.partition_point(|&limit| us >= limit)
}

/// Aggregate compile-stall statistics for one thread (compiles are per-thread,
/// like the [`super::cache`] they populate).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CompileStats {
    /// JIT compile attempts — every one a synchronous eval-thread stall,
    /// whether or not it produced native code.
    pub total_compiles: u64,
    pub total_us: u64,
    pub max_us: u64,
    /// `ops.len()` of the function behind `max_us`.
    pub max_fn_len: usize,
    pub compiled_ok: u64,
    /// Times compiled code was actually ENTERED. Compiles alone say nothing
    /// about payoff: on org-editing the JIT costs 3-5% of instructions at every
    /// setting and relaxing its profitability gate moves Ir by 0.006%, which is
    /// only interpretable next to how often the compiled code runs.
    pub native_entries: u64,
    /// Times `dispatch_sized` was consulted at all, and how it answered.
    ///
    /// Compilation is only ever triggered from this decision, so a callee that
    /// never reaches it is never heated, never compiled and never entered. The
    /// counters exist because eliminating every OTHER candidate (threshold,
    /// profitability gate, id cap, arity) left "ordinary calls do not arrive
    /// here" as the only explanation, and that deserved measuring rather than
    /// inferring.
    pub dispatch_consulted: u64,
    pub dispatch_said_compiled: u64,
    pub not_profitable: u64,
    pub not_compilable: u64,
    /// Cache misses served by a pre-compiled AOT leaf ([`super::aot`]) instead
    /// of a JIT compile — no compile ran, so NOT counted in `total_compiles`.
    pub aot_loads: u64,
    /// Leaves rebuilt with the full register allocator after proving hot
    /// (`RuntimeState::RETIER_FACTOR`); each one is a second compile.
    pub retiers: u64,
    /// Stall distribution: `<100µs, <250µs, <500µs, <1ms, <2.5ms, <5ms, <10ms, >=10ms`.
    pub histogram_us: [u64; 8],
    /// MIR-tier funnel: where bodies are lost on the way to the ONLY tier that
    /// inlines, unboxes across op boundaries and elides redundant guards. A
    /// body that does not reach it gets none of those, so knowing which gate
    /// sheds the most is the difference between widening the right one and
    /// widening a gate nothing was waiting behind.
    ///
    /// The three `gate_*` counters are independent (a body can trip several).
    pub mir_gate_optional: u64,
    pub mir_gate_rest: u64,
    pub mir_gate_prefix: u64,
    /// Passed the gates but `build_mir` bailed (unmodelled op, odd CFG).
    pub mir_build_failed: u64,
    /// Passed the tier gate, but `lower_mir_pure` bailed.
    pub mir_lower_failed: u64,
    /// Built (and inlined), but the tier gate sent it to the baseline before
    /// lowering — keyed `gate:*` in the bail census: a Float-feedback site, a
    /// loop with a shim-lowered op, a generic call left after inlining, or a
    /// shim-lowered op in a body that inlined a callee.
    pub mir_tier_rejected: u64,
    /// Actually took the MIR tier.
    pub mir_taken: u64,
    /// Callees successfully spliced in by `inline_pure_single_block_callees`.
    pub mir_inlined_callees: u64,
}

impl CompileStats {
    /// The counts accumulated since `base`, an earlier snapshot of the same
    /// thread's stats (the command-loop mark). `max_us`/`max_fn_len` are not
    /// additive; the later values are kept.
    pub(crate) fn since(&self, base: &CompileStats) -> CompileStats {
        let d = |now: u64, then: u64| now.saturating_sub(then);
        let mut histogram_us = [0u64; 8];
        for (i, slot) in histogram_us.iter_mut().enumerate() {
            *slot = d(self.histogram_us[i], base.histogram_us[i]);
        }
        CompileStats {
            total_compiles: d(self.total_compiles, base.total_compiles),
            total_us: d(self.total_us, base.total_us),
            max_us: self.max_us,
            max_fn_len: self.max_fn_len,
            compiled_ok: d(self.compiled_ok, base.compiled_ok),
            native_entries: d(self.native_entries, base.native_entries),
            dispatch_consulted: d(self.dispatch_consulted, base.dispatch_consulted),
            dispatch_said_compiled: d(self.dispatch_said_compiled, base.dispatch_said_compiled),
            not_profitable: d(self.not_profitable, base.not_profitable),
            not_compilable: d(self.not_compilable, base.not_compilable),
            aot_loads: d(self.aot_loads, base.aot_loads),
            retiers: d(self.retiers, base.retiers),
            histogram_us,
            mir_gate_optional: d(self.mir_gate_optional, base.mir_gate_optional),
            mir_gate_rest: d(self.mir_gate_rest, base.mir_gate_rest),
            mir_gate_prefix: d(self.mir_gate_prefix, base.mir_gate_prefix),
            mir_build_failed: d(self.mir_build_failed, base.mir_build_failed),
            mir_lower_failed: d(self.mir_lower_failed, base.mir_lower_failed),
            mir_tier_rejected: d(self.mir_tier_rejected, base.mir_tier_rejected),
            mir_taken: d(self.mir_taken, base.mir_taken),
            mir_inlined_callees: d(self.mir_inlined_callees, base.mir_inlined_callees),
        }
    }
}

thread_local! {
    /// `NEOVM_JIT_COMPILE_STATS=1`: WHY the MIR tier bailed, keyed by the
    /// `CompileError::UnsupportedOp` reason — and for the catch-all
    /// `mir-pure-shim-op` (any `Opaque` bytecode op), by the OP itself. This
    /// is the prioritisation data for porting per-op lowerings into the MIR
    /// tier: the ops that bail hot bodies most often go first.
    static MIR_BAIL_REASONS: std::cell::RefCell<std::collections::HashMap<String, u64>> =
        std::cell::RefCell::new(std::collections::HashMap::new());

    /// `NEOVM_JIT_COMPILE_STATS=1`: what the bytecode fuser did at each call
    /// site it looked at — `fused` once per spliced region, and one
    /// `reject:<why>` per site it declined. The reject keys are the widening
    /// worklist: the reason that dominates a real workload is the admission
    /// rule worth relaxing next.
    static INLINE_CENSUS: std::cell::RefCell<std::collections::HashMap<String, u64>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Record one fuser verdict (compile-time only; never on a hot path).
pub(crate) fn record_inline(key: impl Into<String>) {
    if !summary_enabled() {
        return;
    }
    INLINE_CENSUS.with(|m| *m.borrow_mut().entry(key.into()).or_insert(0) += 1);
}

/// Top-N fuser verdicts, most frequent first, for the summary line.
pub(crate) fn inline_census_summary(n: usize) -> String {
    INLINE_CENSUS.with(|m| {
        let m = m.borrow();
        let mut v: Vec<(&String, &u64)> = m.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v.iter()
            .take(n)
            .map(|(k, c)| format!("{k}={c}"))
            .collect::<Vec<_>>()
            .join(",")
    })
}

/// Record one MIR-tier bail reason (compile-time only; never on a hot path).
pub(crate) fn record_mir_bail(reason: String) {
    if !summary_enabled() {
        return;
    }
    MIR_BAIL_REASONS.with(|m| *m.borrow_mut().entry(reason).or_insert(0) += 1);
}

/// Top-N MIR bail reasons, most frequent first, for the summary line.
pub(crate) fn mir_bail_summary(n: usize) -> String {
    MIR_BAIL_REASONS.with(|m| {
        let m = m.borrow();
        let mut v: Vec<(&String, &u64)> = m.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v.iter()
            .take(n)
            .map(|(k, c)| format!("{k}={c}"))
            .collect::<Vec<_>>()
            .join(",")
    })
}

/// Stage at which a body left the MIR funnel (see [`CompileStats`]).
#[derive(Clone, Copy, Debug)]
pub(crate) enum MirFunnel {
    GateOptional,
    GateRest,
    GatePrefix,
    BuildFailed,
    LowerFailed,
    TierRejected,
    Taken,
    InlinedCallees(u64),
}

/// Record one MIR-funnel event. Compile-time only (once per compile attempt),
/// so this is never on a hot path.
pub(crate) fn record_mir(stage: MirFunnel) {
    STATS.with(|c| {
        let mut s = c.get();
        match stage {
            MirFunnel::GateOptional => s.mir_gate_optional += 1,
            MirFunnel::GateRest => s.mir_gate_rest += 1,
            MirFunnel::GatePrefix => s.mir_gate_prefix += 1,
            MirFunnel::BuildFailed => s.mir_build_failed += 1,
            MirFunnel::LowerFailed => s.mir_lower_failed += 1,
            MirFunnel::TierRejected => s.mir_tier_rejected += 1,
            MirFunnel::Taken => s.mir_taken += 1,
            MirFunnel::InlinedCallees(n) => s.mir_inlined_callees += n,
        }
        c.set(s);
    });
}

thread_local! {
    static STATS: Cell<CompileStats> = Cell::new(CompileStats::default());
}

/// Where every `[neovm-jit-*]` report line goes (see the module docs).
pub(crate) enum ReportSink {
    Stderr,
    /// `NEOVM_JIT_STATS_FILE=<path>`, opened for append once.
    File(std::sync::Mutex<std::fs::File>),
}

impl ReportSink {
    /// The sink for `NEOVM_JIT_STATS_FILE`'s value: a file opened for append,
    /// or stderr when the knob is unset or the path cannot be opened (one
    /// `tracing::warn!`, never a panic — a report must not kill a run).
    pub(crate) fn choose(path: Option<&std::ffi::OsStr>) -> ReportSink {
        let Some(path) = path else {
            return ReportSink::Stderr;
        };
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(file) => ReportSink::File(std::sync::Mutex::new(file)),
            Err(err) => {
                tracing::warn!(
                    target: "neovm_jit",
                    path = %std::path::Path::new(path).display(),
                    %err,
                    "NEOVM_JIT_STATS_FILE cannot be opened; reporting to stderr"
                );
                ReportSink::Stderr
            }
        }
    }

    /// Write one `[tag] body` line with a single `write_all`, so concurrent
    /// writers never interleave within a line. Write errors are dropped.
    pub(crate) fn write_line(&self, tag: ReportTag, body: &str) {
        let tag: &'static str = tag.into();
        let line = format!("[{tag}] {body}\n");
        match self {
            ReportSink::Stderr => {
                let _ = std::io::stderr().lock().write_all(line.as_bytes());
            }
            ReportSink::File(file) => {
                let mut file = file.lock().unwrap_or_else(|poison| poison.into_inner());
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}

/// The tag of a report line: the `[...]` prefix tooling greps for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
pub(crate) enum ReportTag {
    /// Periodic compile summary (every 64 compiles).
    #[strum(serialize = "neovm-jit-compile")]
    Compile,
    /// Periodic fuser census.
    #[strum(serialize = "neovm-jit-inline")]
    Inline,
    /// Periodic dispatch-cadence summary (every 50,000 consultations).
    #[strum(serialize = "neovm-jit-dispatch")]
    Dispatch,
    /// Periodic MIR-bail census.
    #[strum(serialize = "neovm-jit-mir-bails")]
    MirBails,
    /// Exit report: the compile aggregates (whole process, then since the
    /// command loop was entered).
    #[strum(serialize = "neovm-jit-final")]
    Final,
    /// Exit report: the MIR-bail census.
    #[strum(serialize = "neovm-jit-final-mir-bails")]
    FinalMirBails,
    /// Exit report: the fuser census.
    #[strum(serialize = "neovm-jit-final-inline")]
    FinalInline,
    /// Exit report: native-run totals.
    #[strum(serialize = "neovm-jit-final-runs")]
    FinalRuns,
}

/// The process-wide report sink, chosen once from `NEOVM_JIT_STATS_FILE`.
fn report_sink() -> &'static ReportSink {
    static SINK: OnceLock<ReportSink> = OnceLock::new();
    SINK.get_or_init(|| ReportSink::choose(std::env::var_os("NEOVM_JIT_STATS_FILE").as_deref()))
}

/// Emit one report line to the configured sink. Callers gate on a knob
/// (normally [`summary_enabled`]); this never checks one itself.
pub(crate) fn report_line(tag: ReportTag, body: &str) {
    report_sink().write_line(tag, body);
}

/// `NEOVM_JIT_COMPILE_STATS=1` (or `NEOVM_JIT_STATS_FILE=<path>`, which
/// implies it): print a one-line running summary every 64 compiles (and on
/// the dispatch cadence) through [`report_line`].
pub(crate) fn summary_enabled() -> bool {
    // Cached in a plain relaxed `u8` rather than a `OnceLock<bool>`: the
    // per-call recorders below sit on the JIT's native->native call seam, and
    // there a `OnceLock` read costs its initialized-flag branch plus an
    // acquire fence on every call, while this costs one load and one compare.
    // `0` = not read yet, `1` = off, `2` = on; the env var cannot change
    // under us, so a racing double read resolves to the same value.
    use std::sync::atomic::{AtomicU8, Ordering};
    static ENABLED: AtomicU8 = AtomicU8::new(0);
    match ENABLED.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = std::env::var("NEOVM_JIT_COMPILE_STATS").as_deref() == Ok("1")
                || std::env::var_os("NEOVM_JIT_STATS_FILE").is_some();
            ENABLED.store(1 + u8::from(on), Ordering::Relaxed);
            on
        }
    }
}

/// Record one JIT compile attempt at the cache-miss seam: `elapsed` wall time
/// of the compile call only, `ops_len` instruction count, and the `result`.
pub(super) fn record_compile(
    elapsed: Duration,
    ops_len: usize,
    result: &Result<CompiledLeaf, CompileError>,
) {
    let compile_us = elapsed.as_micros() as u64;
    let outcome = match result {
        Ok(_) => "ok",
        Err(CompileError::NotProfitable) => "not_profitable",
        Err(_) => "not_compilable",
    };
    // Phase-0 tier residency: which tier produced the leaf. "-" = no leaf.
    let tier = result
        .as_ref()
        .map(|leaf| leaf.tier().name())
        .unwrap_or("-");
    let (clif_insts, clif_blocks, deopt_sites, deopt_slots) =
        super::compile::LAST_IR_STATS.with(|c| c.get());
    tracing::debug!(
        target: "neovm_jit",
        compile_us,
        ops_len,
        outcome,
        tier,
        clif_insts,
        clif_blocks,
        deopt_sites,
        deopt_slots,
        "compile"
    );
    let stats = STATS.with(|s| {
        let mut stats = s.get();
        stats.total_compiles += 1;
        stats.total_us += compile_us;
        if compile_us >= stats.max_us {
            stats.max_us = compile_us;
            stats.max_fn_len = ops_len;
        }
        stats.histogram_us[bucket_index(compile_us)] += 1;
        match result {
            Ok(_) => stats.compiled_ok += 1,
            Err(CompileError::NotProfitable) => stats.not_profitable += 1,
            Err(_) => stats.not_compilable += 1,
        }
        s.set(stats);
        stats
    });
    if summary_enabled() && stats.total_compiles.is_multiple_of(64) {
        report_line(ReportTag::Compile, &format_summary(&stats));
        // The fuser records at compile time, so its census belongs with the
        // compile summary: the dispatch-cadence print below can miss a body
        // whose calls the fuser removed, since it then stops dispatching.
        let inline = inline_census_summary(16);
        if !inline.is_empty() {
            report_line(ReportTag::Inline, &inline);
        }
    }
}

/// Record a cache miss served from the AOT store — a pre-warmed leaf, no JIT
/// compile (and so no stall timed into the compile aggregates).
pub(super) fn record_aot_load(ops_len: usize) {
    tracing::debug!(target: "neovm_jit", ops_len, "aot leaf served");
    STATS.with(|s| {
        let mut stats = s.get();
        stats.aot_loads += 1;
        s.set(stats);
    });
}

/// One-line human-readable rendering of a stats snapshot (the periodic
/// `NEOVM_JIT_COMPILE_STATS` summary and the profiling driver's report).
pub(crate) fn format_summary(s: &CompileStats) -> String {
    let mean_us = s.total_us.checked_div(s.total_compiles).unwrap_or(0);
    format!(
        "compiles={} ok={} native_entries={} dispatch={}/{} not_profitable={} not_compilable={} aot_loads={} retiers={} \
         total_us={} mean_us={mean_us} max_us={} max_fn_len={} \
         mir[taken={} tier_rej={} lower_fail={} build_fail={} gate_opt={} gate_rest={} gate_prefix={} inlined={}] \
         hist[<100us,<250us,<500us,<1ms,<2.5ms,<5ms,<10ms,>=10ms]={:?}",
        s.total_compiles,
        s.compiled_ok,
        s.native_entries,
        s.dispatch_said_compiled,
        s.dispatch_consulted,
        s.not_profitable,
        s.not_compilable,
        s.aot_loads,
        s.retiers,
        s.total_us,
        s.max_us,
        s.max_fn_len,
        s.mir_taken,
        s.mir_tier_rejected,
        s.mir_lower_failed,
        s.mir_build_failed,
        s.mir_gate_optional,
        s.mir_gate_rest,
        s.mir_gate_prefix,
        s.mir_inlined_callees,
        s.histogram_us,
    )
}

/// Record one `dispatch_sized` consultation and its verdict.
///
/// Split gate/body: the gate is called once per JIT call on the direct-entry
/// seam, so it must inline into the caller as a load and a branch instead of
/// costing a call frame to discover the counters are off.
#[inline]
pub(crate) fn record_dispatch(said_compiled: bool) {
    if summary_enabled() {
        record_dispatch_enabled(said_compiled);
    }
}

#[cold]
#[inline(never)]
fn record_dispatch_enabled(said_compiled: bool) {
    STATS.with(|cell| {
        let mut stats = cell.get();
        stats.dispatch_consulted += 1;
        if said_compiled {
            stats.dispatch_said_compiled += 1;
        }
        cell.set(stats);
        // Also report on a dispatch cadence, not only every 64 compiles. A
        // workload that consults the JIT tens of thousands of times and
        // compiles once would otherwise print nothing at all -- which is
        // precisely the case worth seeing, since it means the surface is a
        // coverage question rather than a codegen one.
        if stats.dispatch_consulted.is_multiple_of(50_000) {
            report_line(ReportTag::Dispatch, &format_summary(&stats));
            report_line(ReportTag::MirBails, &mir_bail_summary(16));
            let inline = inline_census_summary(16);
            if !inline.is_empty() {
                report_line(ReportTag::Inline, &inline);
            }
        }
    });
}

/// Count a fast-allocator leaf rebuilt with the full allocator (see
/// `cache::try_run_compiled`).
pub(crate) fn record_retier() {
    STATS.with(|s| {
        let mut stats = s.get();
        stats.retiers += 1;
        s.set(stats);
    });
}

/// Record one ENTRY into compiled code.
///
/// Separate from `record_compile` on purpose: a compile is a cost, an entry is
/// the only thing that can repay it, and the two had no relationship in the
/// stats until this existed.
#[inline]
pub(crate) fn record_native_entry() {
    if summary_enabled() {
        record_native_entry_enabled();
    }
}

#[cold]
#[inline(never)]
fn record_native_entry_enabled() {
    STATS.with(|cell| {
        let mut stats = cell.get();
        stats.native_entries += 1;
        cell.set(stats);
    });
}

/// Snapshot taken when the outer command loop is entered, so the exit report
/// can separate startup (loadup, `after-pdump-load-hook`) from the session.
struct LoopMark {
    at: std::time::Instant,
    compile: CompileStats,
}

thread_local! {
    static LOOP_MARK: std::cell::RefCell<Option<LoopMark>> =
        const { std::cell::RefCell::new(None) };
}

/// Snapshot the counters the exit report prints as `since_command_loop`
/// deltas. Called once, right before the outer `recursive_edit`, on the eval
/// thread. A no-op unless a report knob is set.
pub fn mark_command_loop_entry() {
    if !report_requested() {
        return;
    }
    let compile = STATS.with(Cell::get);
    LOOP_MARK.with(|m| {
        *m.borrow_mut() = Some(LoopMark {
            at: std::time::Instant::now(),
            compile,
        })
    });
}

/// Whether any knob asks for the exit report: `NEOVM_JIT_COMPILE_STATS=1`,
/// `NEOVM_JIT_STATS_FILE` or `NEOVM_JIT_PROFILE`.
fn report_requested() -> bool {
    summary_enabled() || super::compile::jit_profile_path().is_some()
}

/// The final report, printed once when the command loop has returned
/// (`kill-emacs`, the end of `--batch`, a batch error's exit 255). No-op
/// unless a report knob is set. MUST run on the eval thread: it reads the
/// thread-local compile aggregates and caches. Read-only on `ctx`: no
/// interning, no Lisp allocation, no safepoint.
pub fn report_at_exit(_ctx: &crate::emacs_core::eval::Context) {
    if !report_requested() {
        return;
    }
    let report = collect_final_report();
    if summary_enabled() {
        for (tag, line) in report.render() {
            report_line(tag, &line);
        }
    }
    tracing::debug!(
        target: "neovm_jit",
        compiles = report.compile.total_compiles,
        native_entries = report.compile.native_entries,
        osr_transfers = report.osr_transfers,
        "final jit report"
    );
}

/// Gather this thread's aggregates and the process-wide JIT counters.
fn collect_final_report() -> report::FinalReport {
    use std::sync::atomic::Ordering;
    let compile = STATS.with(Cell::get);
    let (since_command_loop_ms, compile_since_loop) = LOOP_MARK.with(|m| match &*m.borrow() {
        Some(mark) => (
            Some(mark.at.elapsed().as_millis() as u64),
            Some(compile.since(&mark.compile)),
        ),
        None => (None, None),
    });
    report::FinalReport {
        pid: std::process::id(),
        since_command_loop_ms,
        compile,
        compile_since_loop,
        mir_bails: mir_bail_summary(32),
        inline: inline_census_summary(32),
        osr_transfers: super::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed),
        seam_fallbacks: super::cache::SEAM_INTERP_FALLBACK_COUNT.load(Ordering::Relaxed),
    }
}

/// Test-only: this thread's current compile-stall aggregate.
#[cfg(test)]
pub(crate) fn compile_stats_snapshot() -> CompileStats {
    STATS.with(Cell::get)
}

/// Test-only: zero this thread's compile-stall aggregate.
#[cfg(test)]
pub(crate) fn reset_compile_stats() {
    STATS.with(|s| s.set(CompileStats::default()));
}

mod report;

#[cfg(test)]
#[path = "stats/tests/stats_test.rs"]
mod tests;

#[cfg(test)]
#[path = "stats/tests/report_test.rs"]
mod report_tests;
