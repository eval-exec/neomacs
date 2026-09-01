#!/usr/bin/env bash
# run-perf-suite.sh — one-shot orchestrator for the whole neovm-core perf panel.
#
# Runs every in-tree bench + profiler and COLLATES their output into ONE
# timestamped report, so before/after numbers across future work stop being
# hand-assembled. It is a THIN ORCHESTRATOR: it invokes the existing bench
# scripts and the #[ignore] profiler tests UNCHANGED — each already builds in its
# own interleaved-A/B discipline (Hot-vs-Cold, plain-vs-pdump, AOT=0-vs-force),
# so this script re-implements no bench and just sequences + labels them.
#
# What it runs, in order (each a section in the report):
#   1. JIT micro-benches      — scripts/run-jit-bench.sh          [--features jit]
#        interp-vs-JIT steady-state ratios (jit_bench_*, Hot-vs-Cold in one proc).
#   2. GC drain-kinds profile — gc_drain_kinds_profile_{plain,pdump}   [--release]
#        which kinds dominate the STW-termination drain; plain-vs-pdump A/B.
#   3. Alloc round-trip probe — alloc_roundtrip_cost_probe             [--release]
#        per-size-class alloc+free cost (the size-class-arena "before" bound).
#   4. VM subr-mix profile    — vm_subr_mix_{byte_compile,fontlock} [--features vm-profile]
#        per-builtin call ranking on real workloads (Op::Call vs CBSym split).
#        (vm-profile is instrumentation; kept OFF for sections 1-3 so their
#         timings stay honest — hence its own build.)
#   5. AOT verdict bench      — scripts/run-aot-bench.sh    [needs target/release/neomacs]
#        real-boot startup + batch deltas, AOT=0-vs-force A/B.
#
# Sections 1-4 build their own release test binary (three feature configs: jit,
# plain, vm-profile — each compiled once). Section 5 reuses a pre-built release
# neomacs; it is SKIPPED with a note if that binary is absent, or force-skip it
# with SKIP_AOT=1.
#
# The profiler tests "FAIL by design" (they dump numbers via panic!), so nextest
# exits non-zero — that is expected; the numbers are the deliverable. This script
# never aborts on a bench's exit code.
#
# Usage:
#   crates/neovm-core/scripts/run-perf-suite.sh [aot_startup_samples]   # default 30
#   SKIP_AOT=1 crates/neovm-core/scripts/run-perf-suite.sh              # skip heavy AOT stage
#
# DRY RUN — the exact commands this fans out to (copy/paste to run one in
# isolation), from the repo root:
#   scripts/run-jit-bench.sh
#   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
#     --no-fail-fast --no-capture -E 'test(/gc_drain_kinds_profile_(plain|pdump)/)'
#   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
#     --no-fail-fast --no-capture -E 'test(alloc_roundtrip_cost_probe)'
#   cargo nextest run -p neovm-core --features vm-profile --release \
#     --run-ignored ignored-only --no-fail-fast --no-capture \
#     -E 'test(/vm_subr_mix_(byte_compile|fontlock)/)'
#   scripts/run-aot-bench.sh [N]
#
# Output: streamed to stdout AND saved under target/perf-suite/.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

ROOT="$(pwd)"
N="${1:-30}"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_DIR="$ROOT/target/perf-suite"
REPORT="$REPORT_DIR/report-$TS.txt"
mkdir -p "$REPORT_DIR"

# Drop only cargo BUILD spam; every bench/profiler data line is kept verbatim.
denoise() { grep -vE '^[[:space:]]*(Compiling|Finished|Building|Downloading|Updating|Blocking|Locking) '; }

# run_section TITLE -- CMD...   : banner + run + collate (tee to stdout + report).
run_section() {
    local title="$1"; shift
    {
        echo
        echo "############################################################"
        echo "## $title"
        echo "## \$ $*"
        echo "############################################################"
    } | tee -a "$REPORT"
    # `|| true`: a bench that exits non-zero (panic-dump, or the AOT gate) must
    # not sink the pipeline; pipefail would otherwise propagate its status.
    { "$@" 2>&1 || true; } | denoise | tee -a "$REPORT"
}

{
    echo "=== neovm-core perf suite — $TS ==="
    echo "repo: $ROOT"
    echo "report: $REPORT"
    echo "rustc: $(rustc --version 2>/dev/null || echo '?')  |  host cores: $(nproc 2>/dev/null || echo '?')"
} | tee "$REPORT"

# 1. JIT micro-benches (interp-vs-JIT, Hot-vs-Cold interleave; --features jit).
run_section "1. JIT micro-benches (scripts/run-jit-bench.sh)" \
    bash "$ROOT/crates/neovm-core/scripts/run-jit-bench.sh"

# 2. GC drain-kinds profile (plain-vs-pdump A/B; plain --release build).
run_section "2. GC drain-kinds profile (plain + pdump)" \
    cargo nextest run -p neovm-core --release --run-ignored ignored-only \
    --no-fail-fast --no-capture -E 'test(/gc_drain_kinds_profile_(plain|pdump)/)'

# 3. Alloc round-trip cost probe (reuses the section-2 --release build).
run_section "3. Alloc round-trip cost probe (per size class)" \
    cargo nextest run -p neovm-core --release --run-ignored ignored-only \
    --no-fail-fast --no-capture -E 'test(alloc_roundtrip_cost_probe)'

# 4. VM subr-mix profile (Op::Call vs CBSym ranking; own --features vm-profile build).
run_section "4. VM subr-mix profile (byte-compile + font-lock)" \
    cargo nextest run -p neovm-core --features vm-profile --release \
    --run-ignored ignored-only --no-fail-fast --no-capture \
    -E 'test(/vm_subr_mix_(byte_compile|fontlock)/)'

# 5. AOT verdict bench (real boot; needs a pre-built release neomacs).
if [[ "${SKIP_AOT:-0}" == "1" ]]; then
    { echo; echo "## 5. AOT verdict bench — SKIPPED (SKIP_AOT=1)"; } | tee -a "$REPORT"
elif [[ -x "$ROOT/target/release/neomacs" ]]; then
    run_section "5. AOT verdict bench (scripts/run-aot-bench.sh, N=$N)" \
        bash "$ROOT/crates/neovm-core/scripts/run-aot-bench.sh" "$N"
else
    {
        echo
        echo "## 5. AOT verdict bench — SKIPPED"
        echo "##    target/release/neomacs not found. Build it, then re-run (or SKIP_AOT=1):"
        echo "##      cargo build --release -p neomacs --bin neomacs"
    } | tee -a "$REPORT"
fi

{
    echo
    echo "=== perf suite complete — full report: $REPORT ==="
} | tee -a "$REPORT"
