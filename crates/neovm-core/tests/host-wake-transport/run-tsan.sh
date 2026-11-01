#!/usr/bin/env bash
# Invoke inside the lane's admitted, pinned environment. Never installs.
set -euo pipefail

harness_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
: "${CARGO_TARGET_DIR:?set the lane-private cargo target root}"
if [[ $# -ne 0 ]]; then
    echo 'usage: run-tsan.sh (toolchain is pinned to nightly-2026-07-03)' >&2
    exit 64
fi

# Pin both production code and the cfg(test) files imported by that code.
source_paths=(
    'src/host_effect/wake.rs'
    'src/host_effect/wake/tests/mod.rs'
    'src/host_effect/wake/tests/native.rs'
    'src/host_effect/wake/native_wait.rs'
    'src/host_effect/wake/tests/native_wait.rs'
)
source_hashes=(
    '0fe0a0e5b5f8b418a428a5f57b6349b7275d0b7a2ddb4e256b879a8669a84aea'
    '74b286788d30e9903a6b91048debfd423b13efa9c05ddef28842b76181a54233'
    '366bda39a7ef68e7f0cd9d4107e74b23fb6ce98a0ab614d733ab9a66161014e2'
    '40da6e5f793725c91ac732f69f9b4fe03efd35e6de63c4067200d37633d85fa4'
    '213e1fda27001164d3c6abe21b96f1797269dbcae0e8f61d2dbebec15c422109'
)
for index in "${!source_paths[@]}"; do
    source_file="$harness_dir/../../${source_paths[$index]}"
    actual_hash=$(sha256sum "$source_file" | awk '{print $1}')
    if [[ "$actual_hash" != "${source_hashes[$index]}" ]]; then
        echo "TSAN_SOURCE_MISMATCH: ${source_paths[$index]}" >&2
        echo "expected ${source_hashes[$index]}, found $actual_hash" >&2
        exit 1
    fi
    printf 'TSAN_SOURCE %s  %s\n' "$actual_hash" "${source_paths[$index]}"
done

if ! command -v rustup >/dev/null 2>&1; then
    echo 'TSAN_UNAVAILABLE: rustup is not installed' >&2
    exit 77
fi
if ! command -v cargo-nextest >/dev/null 2>&1; then
    echo 'TSAN_UNAVAILABLE: cargo-nextest is not installed' >&2
    exit 77
fi

requested_toolchain='nightly-2026-07-03'
toolchain=$(rustup toolchain list --quiet | awk -v requested="$requested_toolchain" '
    $1 == requested || index($1, requested "-") == 1 { print $1; exit }
')
if [[ -z "$toolchain" ]]; then
    echo "TSAN_UNAVAILABLE: $requested_toolchain is not already installed" >&2
    exit 77
fi
sysroot=$(rustup run "$toolchain" rustc --print sysroot)
if [[ ! -f "$sysroot/lib/rustlib/src/rust/library/core/src/lib.rs" ]]; then
    echo "TSAN_UNAVAILABLE: installed $toolchain has no rust-src; nothing was installed" >&2
    exit 77
fi
target=$(rustup run "$toolchain" rustc -vV | sed -n 's/^host: //p')
case "$target" in
    x86_64-unknown-linux-gnu | aarch64-unknown-linux-gnu) ;;
    *) echo "TSAN_UNAVAILABLE: runner supports Linux GNU targets, found $target" >&2; exit 77 ;;
esac

export CARGO_TARGET_DIR="$CARGO_TARGET_DIR/host-wake-transport-tsan"
unset CARGO_ENCODED_RUSTFLAGS RUST_LOG
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Zsanitizer=thread -Cforce-frame-pointers=yes"
export TSAN_OPTIONS="${TSAN_OPTIONS:+$TSAN_OPTIONS:}halt_on_error=1:exitcode=66"

rustup run "$toolchain" rustc -vV
# Instrument std as well as the real owner, poller and harness. A partial
# standard-library instrumentation is not a whole-wake sanitizer receipt.
rustup run "$toolchain" cargo nextest run \
    --manifest-path "$harness_dir/Cargo.toml" \
    --locked --offline --test wake --target "$target" \
    -Z build-std=std,panic_abort \
    --test-threads 1 --retries 0 --no-tests fail --no-fail-fast
printf 'TSAN_REAL_WAKE_PASS\n'
