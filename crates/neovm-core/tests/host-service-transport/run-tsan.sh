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

source_paths=(
    'src/host_effect/contracts.rs'
    'src/host_effect/native_outcome.rs'
    'src/host_effect/native_pending.rs'
    'src/host_effect/tests/native_pending_test.rs'
    'src/host_effect/wake.rs'
    'src/host_effect/wake/native_wait.rs'
    'src/host_effect/wake/tests/mod.rs'
    'src/host_effect/wake/tests/native.rs'
    'src/host_effect/wake/tests/native_wait.rs'
    'src/emacs_core/runtime/eval/green/host_service/transport.rs'
    'src/emacs_core/runtime/eval/green/host_service/pending.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/pending.rs'
    'tests/host-service-transport/tests/channel.rs'
    'tests/host-service-transport/tests/emacs_core/eval/green/mod.rs'
    'tests/host-service-transport/tests/emacs_core/eval/green/actor_tests.rs'
)
source_hashes=(
    'c67b61fe6f6e99503065423a42ce2a63c7d5ce0c5dac6767ab0024249a88a33f'
    'a2d75c4f9a710d362a983a7d779c45902d1ec6505490a2390fb44a0eac63976f'
    '7275fe554bb8c5134395012bc6a2c33b813958025d160672ef871be94ec667a1'
    '91307d41809362d42dde8a93b322eef388e382e082db8cd8b3ac12476769a512'
    'c95e530e27dc55ec6daf562f65d85d38f6c7a6d36f5eea120315db37ce2a45c2'
    '5872a982be4fa90284916a29e85e2ed10eccd4c91ebd3f39a50dfcb4c2940c4f'
    '74b286788d30e9903a6b91048debfd423b13efa9c05ddef28842b76181a54233'
    '366bda39a7ef68e7f0cd9d4107e74b23fb6ce98a0ab614d733ab9a66161014e2'
    '213e1fda27001164d3c6abe21b96f1797269dbcae0e8f61d2dbebec15c422109'
    'd39f205e89ecd607d793715f3f7009ceccd136deb85989c830a38657cf050b4b'
    '8377e5c9b967498df0fc22016b17a277f9d6cc07c5537975295ffac45081b630'
    '1aba556cfe94afcea45695fd1a65bdcef3d1e87f8de4febc7894ce7517fb048c'
    'dddcfc101a3fd59c637a7d268a6eacb0ae9455614c62f6d78c0657e76378114f'
    '9f6e505335d84f9aae2ee214eb6036701923ae8f39009c4af72daec4b52ed085'
    'ce40f14969f7fdb1bf40c22dd2819d1acda658a3625001493f64d933a727f7a3'
)
for index in "${!source_paths[@]}"; do
    source_file="$harness_dir/../../${source_paths[$index]}"
    actual_hash=$(sha256sum "$source_file" | awk '{print $1}')
    if [[ "$actual_hash" != "${source_hashes[$index]}" ]]; then
        echo "TSAN_SOURCE_MISMATCH: ${source_paths[$index]}" >&2
        exit 1
    fi
    printf 'TSAN_SOURCE %s  %s\n' "$actual_hash" "${source_paths[$index]}"
done
rustup run "$toolchain" rustc -vV
# Rebuild std as well as this harness/crossbeam; don't mistake a partially
# instrumented standard library for a whole-channel sanitizer receipt.
rustup run "$toolchain" cargo nextest run \
    --manifest-path "$harness_dir/Cargo.toml" \
    --locked --offline --test channel --target "$target" \
    -Z build-std=std,panic_abort \
    --test-threads 1 --retries 0 --no-tests fail --no-fail-fast
printf 'TSAN_REAL_CHANNEL_PASS\n'
