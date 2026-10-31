#!/usr/bin/env bash
# Invoke inside the lane's admitted, pinned build environment. Never installs.
set -euo pipefail

harness_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
: "${CARGO_TARGET_DIR:?set the lane-private cargo target directory}"
if ! command -v rustup >/dev/null 2>&1; then
    echo 'TSAN_UNAVAILABLE: rustup is not installed' >&2
    exit 77
fi
if ! command -v cargo-nextest >/dev/null 2>&1; then
    echo 'TSAN_UNAVAILABLE: cargo-nextest is not installed' >&2
    exit 77
fi

toolchain=${1:-}
if [[ -z "$toolchain" ]]; then
    toolchain=$(rustup toolchain list --quiet | awk '$1 ~ /^nightly-/ {print $1}' | sort -r | head -n 1)
fi
if [[ -z "$toolchain" ]]; then
    echo 'TSAN_UNAVAILABLE: no installed nightly toolchain' >&2
    exit 77
fi
if ! rustup toolchain list --quiet | awk '{print $1}' | grep -Fx -- "$toolchain" >/dev/null; then
    echo "TSAN_UNAVAILABLE: requested toolchain is not already installed: $toolchain" >&2
    exit 77
fi
case "$toolchain" in
    nightly-*) ;;
    *) echo 'TSAN_UNAVAILABLE: select an installed nightly toolchain' >&2; exit 77 ;;
esac

sysroot=$(rustup run "$toolchain" rustc --print sysroot)
if [[ ! -f "$sysroot/lib/rustlib/src/rust/library/core/src/lib.rs" ]]; then
    echo "TSAN_UNAVAILABLE: installed $toolchain has no rust-src; nothing was installed" >&2
    exit 77
fi
target=$(rustup run "$toolchain" rustc -vV | sed -n 's/^host: //p')
case "$target" in
    x86_64-unknown-linux-gnu | aarch64-unknown-linux-gnu) ;;
    *) echo "TSAN_UNAVAILABLE: this runner supports Linux GNU targets, found $target" >&2; exit 77 ;;
esac

export CARGO_TARGET_DIR="$CARGO_TARGET_DIR/host-service-transport-tsan"
unset CARGO_ENCODED_RUSTFLAGS RUST_LOG
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Zsanitizer=thread -Cforce-frame-pointers=yes"
export TSAN_OPTIONS="${TSAN_OPTIONS:+$TSAN_OPTIONS:}halt_on_error=1:exitcode=66"

printf 'TSAN_SOURCE '
sha256sum "$harness_dir/../../src/emacs_core/runtime/eval/green/host_service/transport.rs"
rustup run "$toolchain" rustc -vV
# Rebuild std as well as this harness/crossbeam; don't mistake a partially
# instrumented standard library for a whole-channel sanitizer receipt.
rustup run "$toolchain" cargo nextest run \
    --manifest-path "$harness_dir/Cargo.toml" \
    --test channel --target "$target" \
    -Z build-std=std,panic_abort \
    --test-threads 1 --retries 0 --no-tests fail --no-fail-fast
printf 'TSAN_REAL_CHANNEL_PASS\n'
