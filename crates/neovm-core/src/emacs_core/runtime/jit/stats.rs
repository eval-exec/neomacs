//! Always-on metering of synchronous JIT compile stalls.
//!
//! The JIT compiles on the eval thread at the first hot call (the cache-miss
//! path in [`super::cache`]), so every compile is a stall the caller feels.
//! This module aggregates how long those stalls are — the evidence base for
//! sizing (or rejecting) background compilation. A compile happens once per
//! function per thread per session (cold path), so the two `Instant` reads per
//! compile are negligible and the metering is unconditionally on.

use std::cell::Cell;
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
    /// Stall distribution: `<100µs, <250µs, <500µs, <1ms, <2.5ms, <5ms, <10ms, >=10ms`.
    pub histogram_us: [u64; 8],
}

thread_local! {
    static STATS: Cell<CompileStats> = Cell::new(CompileStats::default());
}

/// `NEOVM_JIT_COMPILE_STATS=1`: eprintln a one-line running summary every 64
/// compiles. There is no end-of-process dump — thread_locals have no clean
/// exit hook — so the periodic line is the record.
fn summary_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NEOVM_JIT_COMPILE_STATS").as_deref() == Ok("1"))
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
        eprintln!("[neovm-jit-compile] {}", format_summary(&stats));
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
        "compiles={} ok={} native_entries={} dispatch={}/{} not_profitable={} not_compilable={} aot_loads={} \
         total_us={} mean_us={mean_us} max_us={} max_fn_len={} \
         hist[<100us,<250us,<500us,<1ms,<2.5ms,<5ms,<10ms,>=10ms]={:?}",
        s.total_compiles,
        s.compiled_ok,
        s.native_entries,
        s.dispatch_said_compiled,
        s.dispatch_consulted,
        s.not_profitable,
        s.not_compilable,
        s.aot_loads,
        s.total_us,
        s.max_us,
        s.max_fn_len,
        s.histogram_us,
    )
}

/// Record one `dispatch_sized` consultation and its verdict.
pub(crate) fn record_dispatch(said_compiled: bool) {
    if !summary_enabled() {
        return;
    }
    STATS.with(|cell| {
        let mut stats = cell.get();
        stats.dispatch_consulted += 1;
        if said_compiled {
            stats.dispatch_said_compiled += 1;
        }
        cell.set(stats);
    });
}

/// Record one ENTRY into compiled code.
///
/// Separate from `record_compile` on purpose: a compile is a cost, an entry is
/// the only thing that can repay it, and the two had no relationship in the
/// stats until this existed.
pub(crate) fn record_native_entry() {
    if !summary_enabled() {
        return;
    }
    STATS.with(|cell| {
        let mut stats = cell.get();
        stats.native_entries += 1;
        cell.set(stats);
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jit_stats_bucket_index_boundaries() {
        assert_eq!(bucket_index(0), 0);
        assert_eq!(bucket_index(99), 0);
        assert_eq!(bucket_index(100), 1);
        assert_eq!(bucket_index(249), 1);
        assert_eq!(bucket_index(250), 2);
        assert_eq!(bucket_index(500), 3);
        assert_eq!(bucket_index(999), 3);
        assert_eq!(bucket_index(1_000), 4);
        assert_eq!(bucket_index(2_500), 5);
        assert_eq!(bucket_index(5_000), 6);
        assert_eq!(bucket_index(9_999), 6);
        assert_eq!(bucket_index(10_000), 7);
        assert_eq!(bucket_index(u64::MAX), 7);
    }
}
