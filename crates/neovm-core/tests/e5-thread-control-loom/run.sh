#!/usr/bin/env bash
# One-line runner for the E5.4b thread-control Loom models (see README.md).
#
#   ./crates/neovm-core/tests/e5-thread-control-loom/run.sh            # from the repo root
#   LOOM_MAX_PREEMPTIONS=3 ./crates/neovm-core/tests/e5-thread-control-loom/run.sh
#
# Extra arguments pass through to `cargo nextest run`. Works from any working
# directory; on this box run it inside the repo shell (`nix develop`).
set -euo pipefail
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec cargo nextest run --manifest-path "$dir/Cargo.toml" "$@"
