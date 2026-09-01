#!/usr/bin/env bash
# R2-D: the AOT v1 VERDICT bench. Measures the REAL neomacs process (startup +
# batch), NOT in-process JIT tiers (that's run-jit-bench.sh). The AOT preload only
# loads in the production FinalRun path, so the only honest measurement is the
# real binary booting with a real `libneomacs-preload.so` beside it.
#
# What it reports (REAL deltas — a null result is the honest verdict):
#   1. PREWARM COVERAGE + real-boot C4 gate — how many of the emitted loadup
#      leaves actually serve native-from-call-1 in a real NEOVM_AOT=force boot.
#   2. STARTUP wall-clock, N=30, NEOVM_AOT=0 vs force, median / p10 / p90.
#   3. BATCH throughput — an elisp workload calling preloaded loadup fns FEWER
#      than HOT_THRESHOLD (10k) times each, so JIT-only runs them interpreted but
#      AOT-prepopulate runs them native from call 1. Timed AOT=0 vs force.
#
# Fingerprint note: the SAME binary both PRODUCES the preload (--temacs=pdump with
# NEOVM_AOT_PRELOAD=1) and BOOTS it, so the manifest fingerprint matches the
# running image by construction (no fresh-build fingerprint-patch needed here).
#
# Usage: crates/neovm-core/scripts/run-aot-bench.sh [N]   (N = startup samples, default 30)
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

ROOT="$(pwd)"
N="${1:-30}"
BIN="$ROOT/target/release/neomacs"
TEMACS="$ROOT/target/release/neomacs-temacs"
RTROOT="$ROOT"
PRELOAD_SO="$ROOT/target/release/libneomacs-preload.so"
PRELOAD_MANIFEST="$ROOT/target/release/libneomacs-preload.manifest"

export NEOMACS_RUNTIME_ROOT="$RTROOT"

if [[ ! -x "$BIN" ]]; then
    echo "error: $BIN not found — build it first: cargo build --release -p neomacs --bin neomacs" >&2
    exit 1
fi

# The role-copy temacs is what --temacs=pdump runs; refresh it from the freshly
# built neomacs so the producer carries the SAME code + fingerprint as the booter.
cp -f "$BIN" "$TEMACS"

echo "=== R2-D AOT bench (binary: $BIN, runtime-root: $RTROOT, N=$N) ==="

# ---------------------------------------------------------------------------
# 0. Produce the preload .so beside the binary (the dump-time producer path).
# ---------------------------------------------------------------------------
echo
echo "--- producing libneomacs-preload.so (dump-time, NEOVM_AOT_PRELOAD=1) ---"
rm -f "$PRELOAD_SO" "$PRELOAD_MANIFEST"
PRODUCE_LOG="$(mktemp)"
RUST_LOG=info NEOVM_AOT_PRELOAD=1 "$TEMACS" --batch -l loadup --temacs=pdump \
    > "$PRODUCE_LOG" 2>&1
grep -iE "aot-preload: emitted" "$PRODUCE_LOG" || {
    echo "error: producer did not emit a preload .so; tail of log:" >&2
    tail -20 "$PRODUCE_LOG" >&2
    exit 1
}
if [[ ! -f "$PRELOAD_SO" || ! -f "$PRELOAD_MANIFEST" ]]; then
    echo "error: preload artifacts missing after produce" >&2
    exit 1
fi
EMITTED="$(grep -oE 'unique_emitted=[0-9]+' "$PRODUCE_LOG" | head -1 | cut -d= -f2)"
echo "preload .so: $(du -h "$PRELOAD_SO" | cut -f1), unique entries emitted: ${EMITTED:-?}"

# ---------------------------------------------------------------------------
# 1. PREWARM COVERAGE + real-boot C4 gate (NEOVM_AOT=force, real boot).
# ---------------------------------------------------------------------------
echo
echo "--- 1. prewarm coverage / real-boot C4 (NEOVM_AOT=force) ---"
# A FinalRun batch boot routes tracing to LogTarget::File (silent unless
# NEOMACS_LOG_FILE is set) — NOT stderr — so point it at a file to read the
# prepopulate line. (stdout still carries the elisp's BOOT-OK.)
COV_LOG="$(mktemp)"
NEOMACS_LOG_FILE="$COV_LOG" RUST_LOG=info NEOVM_AOT=force "$BIN" --batch \
    --eval '(progn (princ "BOOT-OK\n") (kill-emacs 0))' >/dev/null 2>&1
PREPOP_LINE="$(grep -iE "AOT preload: prepopulated" "$COV_LOG" | head -1)"
if [[ -z "$PREPOP_LINE" ]]; then
    echo "FAIL: no prepopulate log line under NEOVM_AOT=force — preload did not load" >&2
    echo "tail of boot log:" >&2; tail -20 "$COV_LOG" >&2
    exit 1
fi
echo "$PREPOP_LINE"
INSERTED="$(echo "$PREPOP_LINE" | grep -oE 'prepopulated [0-9]+' | grep -oE '[0-9]+')"
TOTAL="$(echo "$PREPOP_LINE" | grep -oE '/ [0-9]+ loadup' | grep -oE '[0-9]+')"
if [[ "${INSERTED:-0}" -gt 0 ]]; then
    echo "C4 GATE PASS: $INSERTED of ${TOTAL:-?} loadup leaves native-from-call-1 (real boot, NEOVM_AOT=force)"
else
    echo "C4 GATE FAIL: 0 leaves prepopulated in a real boot" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 2. STARTUP wall-clock, N samples, AOT=0 vs force, median/p10/p90.
# ---------------------------------------------------------------------------
echo
echo "--- 2. startup wall-clock (N=$N, AOT=0 vs force) ---"
# A minimal real boot: load the final pdump, run a trivial form, exit. The AOT
# prepopulate (force) runs in the FinalRun hook BEFORE this form is dispatched.
STARTUP_FORM='(kill-emacs 0)'

percentile() { # $1=file of ms values, $2=percentile (0..100)
    sort -n "$1" | awk -v p="$2" '{a[NR]=$0} END{if(NR==0){print "NA";exit} idx=int((p/100.0)*(NR-1))+1; if(idx<1)idx=1; if(idx>NR)idx=NR; print a[idx]}'
}
time_boots() { # $1=AOT value, $2=out file (ms per run)
    : > "$2"
    for _ in $(seq 1 "$N"); do
        local start end
        start=$(date +%s%N)
        NEOVM_AOT="$1" "$BIN" --batch --eval "$STARTUP_FORM" >/dev/null 2>&1
        end=$(date +%s%N)
        echo $(( (end - start) / 1000000 )) >> "$2"
    done
}
OFF_MS="$(mktemp)"; FORCE_MS="$(mktemp)"
echo "timing $N boots with NEOVM_AOT=0 ..."
time_boots 0 "$OFF_MS"
echo "timing $N boots with NEOVM_AOT=force ..."
time_boots force "$FORCE_MS"
OFF_MED="$(percentile "$OFF_MS" 50)";   OFF_P10="$(percentile "$OFF_MS" 10)";   OFF_P90="$(percentile "$OFF_MS" 90)"
FOR_MED="$(percentile "$FORCE_MS" 50)"; FOR_P10="$(percentile "$FORCE_MS" 10)"; FOR_P90="$(percentile "$FORCE_MS" 90)"
echo "STARTUP AOT=0    : median ${OFF_MED}ms  p10 ${OFF_P10}ms  p90 ${OFF_P90}ms"
echo "STARTUP AOT=force: median ${FOR_MED}ms  p10 ${FOR_P10}ms  p90 ${FOR_P90}ms"
if [[ "${OFF_MED:-0}" -gt 0 && "${FOR_MED:-0}" -gt 0 ]]; then
    awk -v o="$OFF_MED" -v f="$FOR_MED" 'BEGIN{printf "STARTUP delta (median): %+d ms (%.1f%%)  [force vs off; negative=faster]\n", f-o, 100.0*(f-o)/o}'
fi

# ---------------------------------------------------------------------------
# 3. BATCH throughput — preloaded loadup fns called < HOT_THRESHOLD times.
# ---------------------------------------------------------------------------
echo
echo "--- 3. batch throughput (sub-HOT_THRESHOLD calls of preloaded loadup fns) ---"
# The honest AOT-win surface is code that touches preloaded bytecode fns FEWER
# than HOT_THRESHOLD (10000) times each — JIT never tiers them up (it would catch
# up past 10k), AOT serves them native from call 1. So we make a SINGLE sub-10k
# pass over the accessors; the per-process heat never crosses the threshold, so
# under AOT=0 these run INTERPRETED the whole pass and under AOT=force they run
# NATIVE the whole pass. We repeat the WHOLE PASS only via fresh processes
# (best-of-5 below — each new process resets all heat), never in an in-process
# loop (which would blow past 10k and let the JIT tier up, erasing the contrast).
# NOTE: an honest, modest delta is the expected outcome — the per-call
# native-vs-interp gap is microseconds; AOT's real value is the absence of the
# warmup cliff, not raw steady-state throughput (which the JIT eventually matches).
read -r -d '' BATCH_FORM <<'ELISP'
(let* ((data (let (l) (dotimes (i 64) (push (list i (1+ i) (+ i 2) (+ i 3)) l)) l))
       (reps 140)          ; 140 * 64 = 8960 calls/fn  (< HOT_THRESHOLD 10000)
       (acc 0)
       (t0 (float-time)))
  (dotimes (_ reps)
    (dolist (x data)
      ;; cadr/caddr/cdddr/cddr are preloaded loadup bytecode fns (AOT candidates);
      ;; zerop likewise. car/+/if are core ops (baseline, identical in both modes).
      (setq acc (+ acc
                   (if (zerop (car x)) 0 1)
                   (if (cadr x) 1 0)
                   (if (caddr x) 1 0)
                   (if (cdddr x) 1 0)
                   (if (cddr x) 1 0)))))
  (princ (format "BATCH elapsed %.6f acc %d calls/fn %d\n"
                 (- (float-time) t0) acc (* reps 64)))
  (kill-emacs 0))
ELISP
batch_once() { # $1=AOT value -> prints elapsed seconds
    NEOVM_AOT="$1" "$BIN" --batch --eval "$BATCH_FORM" 2>/dev/null \
        | grep -oE 'BATCH elapsed [0-9.]+' | grep -oE '[0-9.]+'
}
# Best-of-5 (min cancels scheduler/thermal noise), each AOT mode.
best_of() { # $1=AOT value
    local best="" v
    for _ in 1 2 3 4 5; do
        v="$(batch_once "$1")"
        [[ -z "$v" ]] && continue
        if [[ -z "$best" ]] || awk -v a="$v" -v b="$best" 'BEGIN{exit !(a<b)}'; then best="$v"; fi
    done
    echo "$best"
}
echo "running batch workload (best-of-5) AOT=0 ..."
B_OFF="$(best_of 0)"
echo "running batch workload (best-of-5) AOT=force ..."
B_FORCE="$(best_of force)"
echo "BATCH AOT=0    : ${B_OFF:-NA} s"
echo "BATCH AOT=force: ${B_FORCE:-NA} s"
if [[ -n "${B_OFF:-}" && -n "${B_FORCE:-}" ]]; then
    awk -v o="$B_OFF" -v f="$B_FORCE" 'BEGIN{printf "BATCH-trivial speedup (off/force): %.2fx  [>1 = AOT faster]\n", o/f}'
fi
echo "  NOTE: ~1x is EXPECTED here — trivial accessors are exactly where our JIT"
echo "  has no win to inherit (JIT is ~1x on call-dominated/trivial, 12-19x on compute)."

# ---------------------------------------------------------------------------
# 4. BATCH-COMPUTE (the AOT SWEET SPOT): a compute-heavy AOT-candidate body served
#    NATIVE FROM CALL 1 vs the interpreter (the body never reaches HOT_THRESHOLD,
#    so the JIT never tiers it → interp baseline). This is the decisive measurement
#    — AOT native code == the JIT's MIR codegen, so it inherits the JIT's compute
#    win FROM CALL 1. Run as the in-tree `aot_bench_compute_loop` (#[ignore] release
#    bench, BENCH-panic), the same harness as run-jit-bench.sh.
# ---------------------------------------------------------------------------
echo
echo "--- 4. batch-compute (AOT-native-from-call-1 vs interp, the sweet spot) ---"
cargo nextest run -p neovm-core --release --run-ignored ignored-only \
    -E 'test(/aot_bench_compute_loop/)' --no-fail-fast --no-capture 2>&1 \
    | grep -oE 'BENCH aot-compute-loop.*' || echo "  (compute bench did not report — check the build)"

# ---------------------------------------------------------------------------
# 5. REAL-ALGORITHM (R2-E E2): a RECOGNIZABLE pure-fixnum algorithm — Collatz
#    step-count, whose hot loop byte-compiles to ZERO CallBuiltin(Sym) (only the
#    dedicated arith ops Gtr/Rem/Eqlsign/Div/Mul/Add1 — VERIFIED via the
#    byte-compiler) — served AOT-native-from-call-1 vs the interpreter. Reports
#    TWO regimes to be honest about WHERE the win lands: (A) realistic many-short
#    -calls (dispatch-bound, ~1x) and (B) inner-loop-bound long orbits (the real
#    compute win, ~4-4.5x). This is the NARROW pure-fixnum sweet spot — most real
#    elisp is shim-bound (~1x); we show where AOT helps, not that it helps
#    everywhere.
# ---------------------------------------------------------------------------
echo
echo "--- 5. real-algorithm collatz (pure-fixnum, verified zero-CallBuiltinSym; A=short B=long-orbit) ---"
cargo nextest run -p neovm-core --release --run-ignored ignored-only \
    -E 'test(/aot_bench_real_algorithm/)' --no-fail-fast --no-capture 2>&1 \
    | grep -oE 'BENCH aot-real-collatz.*' || echo "  (real-algorithm bench did not report — check the build)"

echo
echo "=== R2-D/E bench complete ==="
