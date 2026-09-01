#!/usr/bin/env bash
# ThreadSanitizer pass over neovm-core's CONCURRENT GC (perf-roadmap task 11).
#
# The concurrent collector marks on a background thread while the mutator runs
# (parity mark bits `GcHeader.marked`/`is_marked_at`/`mark_claim_at`, the
# concurrent claim dispatcher `concurrent_try_mark_owned`, the string
# `intervals: AtomicPtr<TextPropertyTable>` word, per-class page-base snapshots,
# the immediate-join wake, size-class ObjectArenas). This landed with ZERO TSan
# coverage; this script is the repeatable gate that pays that verification debt.
#
# WHY NOT `cargo nextest`:
#   TSan reserves a huge (~tens of TB) shadow-memory VA region and REQUIRES an
#   UNLIMITED virtual address space (RLIMIT_AS). This repo's `.config/nextest.toml`
#   runs every test under a `prlimit --as=8G` wrapper (so unit-test OOMs abort
#   with a backtrace). 8G < TSan's shadow map, so a nextest-run TSan binary
#   re-execs and dies with "ThreadSanitizer setrlimit() failed 22". We therefore
#   build the instrumented libtest binary with cargo and drive it DIRECTLY,
#   one process per test (== nextest's isolation), in parallel across all cores.
#
# WHY build-std + nightly + --no-default-features:
#   `-Zsanitizer=thread` instruments user code; `-Zbuild-std` rebuilds std/core
#   with the same instrumentation (both need nightly, side-installed —
#   `rust-toolchain.toml` stays pinned at stable). `--no-default-features` drops
#   the cranelift/JIT stack (not exercised by the GC, and a big compile saving);
#   the concurrent GC tests do not need the `jit` feature.
#
# Prereqs (side-install, does NOT touch the repo pin):
#   rustup toolchain install nightly --component rust-src
#
# Usage:
#   crates/neovm-core/scripts/run-gc-tsan.sh                 # full concurrent surface
#   crates/neovm-core/scripts/run-gc-tsan.sh 'concurrent_mark_races'   # substring filter
#
# Exit status: 0 iff every selected test passed with NO ThreadSanitizer report.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

TARGET="x86_64-unknown-linux-gnu"
TOOLCHAIN="nightly"
OUTDIR="${TMPDIR:-./tmp}/gc-tsan-logs"
FILTER="${1:-}"

# The concurrent-GC + seqlock surface. Module-scoped (auto-picks up new
# concurrent_* / parity_* / finalizer_* tests) plus the eval/symbol concurrent
# tests. Single-threaded unit tests in these modules also run (they just pass).
SURFACE_RE='^tagged::gc::(ownership|float_arena|bytecode_arena|arena_promotion)_tests::'
SURFACE_RE+='|^emacs_core::symbol::tests::seqlock'
SURFACE_RE+='|^emacs_core::eval::tests::gc_concurrent'
SURFACE_RE+='|^emacs_core::eval::tests::gc_safe_point_runs_concurrent'

# --- TSan needs an unlimited virtual address space -------------------------
ulimit -v unlimited 2>/dev/null || true
if [ "$(ulimit -v)" != "unlimited" ]; then
  echo "ERROR: RLIMIT_AS (ulimit -v) is $(ulimit -v), not unlimited." >&2
  echo "       ThreadSanitizer cannot map its shadow memory. Run this OUTSIDE" >&2
  echo "       any prlimit/cgroup AS cap (in particular NOT via cargo nextest)." >&2
  exit 2
fi

echo ">>> Building TSan-instrumented neovm-core test binary (build-std, --no-default-features)..."
BUILD_LOG="$(mktemp)"
# -fuse-ld=mold: the GNU-ld link of the TSan-instrumented binary OOM-SIGKILLs
# under memory contention (sibling builds); mold links it comfortably.
if ! RUSTFLAGS="-Zsanitizer=thread -Clink-arg=-fuse-ld=mold" \
      cargo "+$TOOLCHAIN" test -p neovm-core --lib --no-run \
      -Zbuild-std --target "$TARGET" --no-default-features >"$BUILD_LOG" 2>&1; then
  echo "BUILD FAILED:" >&2; cat "$BUILD_LOG" >&2; exit 1
fi
# The lib unittest binary carries all tagged::/emacs_core:: tests.
BIN="$(sed -n 's/.*Executable unittests[^(]*(\(.*\))/\1/p' "$BUILD_LOG" | head -1)"
rm -f "$BUILD_LOG"
if [ -z "${BIN:-}" ] || [ ! -x "$BIN" ]; then
  echo "ERROR: could not locate the built test binary." >&2; exit 1
fi
echo ">>> Test binary: $BIN"

# --- Select tests ----------------------------------------------------------
mkdir -p "$OUTDIR"
LIST="$OUTDIR/selected.txt"
"$BIN" --list 2>/dev/null | sed 's/: test$//' | grep -E "$SURFACE_RE" \
  | { [ -n "$FILTER" ] && grep -F "$FILTER" || cat; } | sort -u > "$LIST"
N="$(wc -l < "$LIST")"
if [ "$N" -eq 0 ]; then echo "ERROR: no tests selected." >&2; exit 1; fi
echo ">>> Selected $N tests. Running one process per test across $(nproc) cores..."

# --- Run: process-per-test, in parallel ------------------------------------
# RUST_MIN_STACK mirrors .cargo/config.toml [env] (128M) — the conservative GC
# scans the main-thread stack, so tests need the large stack; running the binary
# directly (off-cargo) does NOT inherit that env, so set it here.
export RUST_MIN_STACK=134217728
export TSAN_OPTIONS="halt_on_error=1 exitcode=66 history_size=7 second_deadlock_stack=1"
export BIN OUTDIR

run_one() {
  local name="$1" safe log rc
  safe="${name//:/_}"; log="$OUTDIR/$safe.log"
  "$BIN" --exact "$name" --nocapture --test-threads=1 >"$log" 2>&1; rc=$?
  if [ "$rc" -eq 0 ]; then echo "PASS $name"
  elif grep -qiE "ThreadSanitizer:|data race" "$log"; then echo "RACE(rc=$rc) $name"
  else echo "FAIL(rc=$rc) $name"; fi
}
export -f run_one

RESULTS="$OUTDIR/results.txt"
xargs -P "$(nproc)" -I{} bash -c 'run_one "$@"' _ {} < "$LIST" | tee "$RESULTS"

# --- Summary ---------------------------------------------------------------
PASS=$(grep -c '^PASS ' "$RESULTS" || true)
RACE=$(grep -c '^RACE' "$RESULTS" || true)
FAIL=$(grep -c '^FAIL' "$RESULTS" || true)
echo "-------------------------------------------------------------"
echo "TSan GC pass: $PASS passed, $RACE races, $FAIL other failures (of $N)."
echo "Per-test logs: $OUTDIR"
if [ "$RACE" -ne 0 ] || [ "$FAIL" -ne 0 ]; then
  echo "NON-CLEAN — inspect the RACE/FAIL logs above." >&2
  exit 1
fi
echo "CLEAN — no ThreadSanitizer reports across the concurrent GC surface."
