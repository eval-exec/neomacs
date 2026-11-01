#!/usr/bin/env bash
# One-line runner for the N5 grace model (see README.md in this directory).
#
#   ./crates/neovm-core/tests/n5-grace-loom/run.sh            # from the repo root
#   N5_PREEMPTIONS=3 ./crates/neovm-core/tests/n5-grace-loom/run.sh -- --nocapture
#
# Extra arguments pass through to `cargo test`, so a single target can be
# run with `--test ledger` or `--test measurement`. Works from any working
# directory; on this box run it inside the repo shell (`nix develop`).
set -euo pipefail
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec cargo test --manifest-path "$dir/Cargo.toml" "$@"
