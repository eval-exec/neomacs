#!/usr/bin/env bash
# One-line runner for the stopped serial keeper model (see README.md).
#
#   ./tests/pentry-keeper-loom/run.sh
#   ./tests/pentry-keeper-loom/run.sh --test lifetime_contracts
#
# With no arguments this runs keeper at bounds 2 and 3, then the lifetime
# contracts. Extra arguments replace that plan and pass through to one
# `cargo nextest run`.
set -euo pipefail
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest="$dir/Cargo.toml"
if [[ $# -gt 0 ]]; then
  exec taskset -c 0-3,14-31 cargo nextest run --manifest-path "$manifest" "$@"
fi
taskset -c 0-3,14-31 env -u RUST_LOG PENTRY_PREEMPTIONS=2 \
  cargo nextest run --manifest-path "$manifest" --test keeper --test-threads 1 --success-output final
taskset -c 0-3,14-31 env -u RUST_LOG PENTRY_PREEMPTIONS=3 \
  cargo nextest run --manifest-path "$manifest" --test keeper --test-threads 1 --success-output final
exec taskset -c 0-3,14-31 env -u RUST_LOG \
  cargo nextest run --manifest-path "$manifest" --test lifetime_contracts --test-threads 1 --success-output final
