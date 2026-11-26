#!/usr/bin/env bash
# One-line runner for the L6.7 keeper-driver model (see README.md in this directory).
#
#   ./crates/neovm-core/tests/keeper-driver-loom/run.sh            # from the repo root
#   LOOM_MAX_PREEMPTIONS=3 ./crates/neovm-core/tests/keeper-driver-loom/run.sh
#
# Extra arguments pass through to `cargo nextest run`. Works from any working directory; on this box run it
# inside the repo shell (`nix develop`).
set -euo pipefail
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec cargo nextest run --manifest-path "$dir/Cargo.toml" "$@"
