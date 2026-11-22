#!/usr/bin/env bash
# Scaffold TSan for the three real-thread doubles. Not the E2 gate.
# Same nightly and build-std shape as crates/neovm-core/tests/host-wake-transport/run-tsan.sh.
set -euo pipefail
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$dir/target-tsan}"
unset CARGO_ENCODED_RUSTFLAGS RUST_LOG
export RUSTFLAGS="-Zsanitizer=thread -Cforce-frame-pointers=yes"
export RUSTDOCFLAGS="-Zsanitizer=thread"
export TSAN_OPTIONS="${TSAN_OPTIONS:+$TSAN_OPTIONS:}halt_on_error=1:exitcode=66"
target="x86_64-unknown-linux-gnu"
exec taskset -c 0-3,14-31 \
  cargo +nightly-2026-07-03 nextest run \
  --manifest-path "$dir/Cargo.toml" \
  --test tsan_scaffold \
  --target "$target" \
  -Z build-std=std,panic_abort \
  --test-threads 1 \
  --success-output final
