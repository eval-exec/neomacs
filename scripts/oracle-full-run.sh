#!/usr/bin/env bash
# Whole-host oracle run at a batch tip.
#
#   scripts/oracle-full-run.sh [LOGDIR]
#
# Runs the ENTIRE neovm-oracle-tests package against a real, pinned GNU Emacs
# 31.1 on this host, with the release pdump Neomacs binary.  This is the
# reproducible replacement for ad-hoc "cargo nextest run" batches: it fixes
# the binary (release pdump, never debug), the oracle mode (VERIFY, so the
# host's own GNU is spawned instead of comparing against a stored expectation
# recorded on another OS), the frozen-clock interposer, and the two-phase
# scheduling the GC-stress tests need.
#
# Environment:
#   NEOVM_FORCE_ORACLE_PATH  GNU Emacs 31.1 binary to use.  Defaults to
#                            `emacs` on PATH.  The run refuses any other
#                            version: the canonical reference is the
#                            emacs-31.1 tag a360712c9d272d950d8d8255ef74570f7e90b7d9.
#   NEOVM_BINARY_PATH        Release Neomacs binary.  Defaults to
#                            $CARGO_TARGET_DIR/release/neomacs (or
#                            target/release/neomacs).  Must have a .pdump next
#                            to it; debug binaries are rejected.
#   NEOVM_ORACLE_MODE        Oracle mode; defaults to `verify` and may not be
#                            `snapshot` here (see below).
#   NEOVM_LIBFAKETIME_SO     libfaketime shared object for the frozen-clock
#                            oracle tests (the nix devShell exports it).
#   ORACLE_FULL_PHASE        both (default), 1 or 2.  Run one phase at a time
#                            to keep a remote job inside its slot's time cap;
#                            phase 2 reuses the binaries phase 1 built.
#
# Phases:
#   1. Every oracle test except the 15 GC-stress matrix tests, default profile.
#   2. Those 15 tests under the `oracle-slow` nextest profile: same 2400s
#      watchdog as the default-profile override, but a bounded 2-slot
#      reservation (12 concurrent) instead of serializing the whole group.
#      Measured: one such test takes 857s with the release binary in VERIFY
#      mode, so the group is ~3.6 CPU-hours; phase 2 runs it in tens of minutes.
#
# Outputs, in LOGDIR (default target/oracle-full-run/<UTC timestamp>):
#   header.txt, phase1.log, phase2.log, summary.txt
set -u

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
logdir="${1:-$root/target/oracle-full-run/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$logdir" || exit 1

die() {
    echo "oracle-full-run: $*" >&2
    exit 2
}

# --- GNU Emacs 31.1 oracle -------------------------------------------------
oracle="${NEOVM_FORCE_ORACLE_PATH:-$(command -v emacs 2>/dev/null || true)}"
[ -n "$oracle" ] || die "no GNU Emacs oracle found: set NEOVM_FORCE_ORACLE_PATH to the pinned GNU Emacs 31.1 binary"
[ -x "$oracle" ] || die "NEOVM_FORCE_ORACLE_PATH=$oracle is not executable"
oracle_version="$("$oracle" --version 2>/dev/null | head -1)"
case "$oracle_version" in
    *"GNU Emacs 31.1"*) ;;
    *) die "oracle $oracle reports '$oracle_version'; this recipe requires GNU Emacs 31.1 (pinned a360712c9d272d950d8d8255ef74570f7e90b7d9)" ;;
esac

# --- release pdump binary --------------------------------------------------
target_dir="${CARGO_TARGET_DIR:-$root/target}"
binary="${NEOVM_BINARY_PATH:-$target_dir/release/neomacs}"
[ -x "$binary" ] || die "Neomacs binary not found at $binary (run 'cargo xtask fresh-build --release' or set NEOVM_BINARY_PATH)"
case "$binary" in
    */debug/*)
        die "NEOVM_BINARY_PATH=$binary is a debug binary. The oracle recipe requires the release pdump binary: debug builds have no pdump, boot through loadup (10-30x slower) and SIGABRT on arm64 deep recursion." ;;
esac
[ -f "$binary.pdump" ] || die "no pdump at $binary.pdump; build with 'cargo xtask fresh-build --release' and point NEOVM_BINARY_PATH at its release/neomacs"

# --- frozen clock ----------------------------------------------------------
[ -n "${NEOVM_LIBFAKETIME_SO:-}" ] || die "NEOVM_LIBFAKETIME_SO is unset; the frozen-clock oracle tests need libfaketime (run inside 'nix develop', or set it to the shared object)"

# --- mode ------------------------------------------------------------------
mode="${NEOVM_ORACLE_MODE:-verify}"
case "$mode" in
    snapshot|snap|expected)
        die "NEOVM_ORACLE_MODE=$mode compares Neomacs against the stored (Linux-recorded) expectation and never spawns this host's GNU. A whole-host oracle run must use verify." ;;
    verify|live|refresh) ;;
    *) die "unknown NEOVM_ORACLE_MODE=$mode; use verify (recommended), live or refresh" ;;
esac

export NEOVM_BINARY_PATH="$binary"
# The oracle package also contains MELPA-support probes: the treesit grammar
# family resolves its Neomacs runtime through neomacs_binary(), which reads
# NEOMACS_BIN and falls back to <workspace>/target/release/neomacs.  The
# recipe's release binary can live in an external CARGO_TARGET_DIR, outside
# the workspace, so export the same artifact under that name as well; without
# it those tests fail to spawn their editor (Launch NotFound) and never reach
# a parity comparison.  NEOMACS_RUNTIME_ROOT mirrors the CI convention.
export NEOMACS_BIN="$binary"
export NEOMACS_RUNTIME_ROOT="$root"
export NEOVM_FORCE_ORACLE_PATH="$oracle"
export NEOVM_ORACLE_MODE="$mode"
export TMPDIR="${TMPDIR:-/tmp}"
export RUST_LOG="${RUST_LOG:-error}"

slow_filter='package(neovm-oracle-tests) and (test(inlined_call_semantics::) or test(backtrace_frame_semantics::oracle_inline_) or test(debug_on_next_call::oracle_inline_))'

# --- fresh-build attestation ----------------------------------------------
# The binary and the Lisp tree it loads must come from the SAME
# `cargo xtask fresh-build --release` run.  A sync can silently strip the
# built .elc files (rsync --delete removes build outputs that are not in the
# source worktree), and the editor then loads Lisp sources instead of the
# compiled product: macroexpansion order, gensym counters, docstring
# references and preloaded state all change.  The preflight verifies the
# binary's sha256 and the whole generated-Lisp manifest against the stamp the
# build wrote beside the binary, and requires a .elc for every .el that
# loadup.el preloads.  The test harnesses enforce the same attestation at
# their startup; this run makes the refusal happen once, before anything is
# measured.  NEOVM_SKIP_FRESH_BUILD_STAMP=<non-empty> is the one override.
echo "# fresh-build preflight"
cargo xtask oracle-preflight --binary "$binary" --lisp-root "$root/lisp" \
    2>&1 | tee "$logdir/preflight.log"
preflight_rc=${PIPESTATUS[0]}
if [ "$preflight_rc" -ne 0 ]; then
    die "fresh-build preflight refused (see $logdir/preflight.log): the binary and the Lisp tree are not from the same fresh build. Run 'cargo xtask fresh-build --release' for this binary."
fi

{
    echo "# oracle-full-run"
    echo "# start   $(date -u +%FT%TZ)"
    echo "# host    $(hostname)"
    echo "# tree    $root"
    if head="$(git -C "$root" rev-parse HEAD 2>/dev/null)"; then
        echo "# head    $head"
    else
        echo "# head    unknown (tree has no .git)"
    fi
    echo "# oracle  $oracle ($oracle_version)"
    echo "# binary  $binary ($(sha256sum "$binary" | cut -d' ' -f1))"
    echo "# pdump   $binary.pdump ($(sha256sum "$binary.pdump" | cut -d' ' -f1))"
    echo "# mode    $mode"
    echo "# slow    $slow_filter"
} | tee "$logdir/header.txt"

phase="${ORACLE_FULL_PHASE:-both}"
case "$phase" in
    both|1|2) ;;
    *) die "unknown ORACLE_FULL_PHASE=$phase; use both, 1 or 2" ;;
esac
rc1=0
rc2=0

if [ "$phase" = "both" ] || [ "$phase" = "1" ]; then
    echo "# phase 1/2: whole oracle package minus the GC-stress slow group"
    cargo nextest run -p neovm-oracle-tests --no-fail-fast \
        -E "not ($slow_filter)" \
        --final-status-level slow 2>&1 | tee "$logdir/phase1.log"
    rc1=${PIPESTATUS[0]}
fi

if [ "$phase" = "both" ] || [ "$phase" = "2" ]; then
    echo "# phase 2/2: GC-stress slow group (profile oracle-slow)"
    cargo nextest run -p neovm-oracle-tests --no-fail-fast --profile oracle-slow \
        -E "$slow_filter" \
        --final-status-level all 2>&1 | tee "$logdir/phase2.log"
    rc2=${PIPESTATUS[0]}
fi

{
    echo "# oracle-full-run summary"
    echo "# end     $(date -u +%FT%TZ)"
    echo "# phase1_rc=$rc1 phase2_rc=$rc2"
    grep -hE "^ +Summary|tests run:" "$logdir"/phase*.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | tail -4
} | tee "$logdir/summary.txt"

[ "$rc1" -eq 0 ] && [ "$rc2" -eq 0 ]
