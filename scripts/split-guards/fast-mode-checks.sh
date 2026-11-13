#!/usr/bin/env bash
# Split FAST MODE: compile only. No tests, trybuild runners or projectors.
set -euo pipefail
unset RUST_LOG
out=${1:?pass an output directory}
mkdir -p "$out"
run_check() {
    local name=$1
    shift
    printf 'CHECK_BEGIN %s\n' "$name"
    set +e
    "$@" > "$out/$name.log" 2>&1
    local rc=$?
    set -e
    cat "$out/$name.log"
    printf 'CHECK_END %s %s\n' "$name" "$rc"
    printf '%s\n' "$rc" > "$out/$name.rc"
    if [ "$rc" -ne 0 ]; then return "$rc"; fi
}
run_check workspace cargo check --workspace --all-targets
run_check test-support cargo check -p neovm-core -p neovm-value --all-targets --features test-support
run_check perf-fixtures cargo check -p neovm-core --tests --features perf-probes,contract-fixtures
printf 'READY_CHECKS 0 0 0\n'
