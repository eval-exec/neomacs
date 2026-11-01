#!/usr/bin/env bash
# Invoke inside the lane's admitted, pinned environment. Never installs.
set -euo pipefail

harness_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
: "${CARGO_TARGET_DIR:?set the lane-private cargo target root}"
if [[ $# -ne 0 ]]; then
    echo 'usage: run-green-tsan.sh (toolchain is pinned to nightly-2026-07-03)' >&2
    exit 64
fi

# Pin both production code and the cfg(test) files imported by that code.
source_paths=(
    'src/emacs_core/runtime/eval/green/scheduler.rs'
    'src/emacs_core/runtime/eval/green/switch.rs'
    'src/emacs_core/runtime/eval/green/host_service.rs'
    'src/emacs_core/runtime/eval/green/host_service/pump.rs'
    'src/emacs_core/runtime/eval/green/host_service/pending.rs'
    'src/emacs_core/runtime/eval/green/host_service/scheduling.rs'
    'src/emacs_core/runtime/eval/green/host_service/transport.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/runtime.rs'
    'src/emacs_core/system/process/green_select.rs'
    'src/host_effect/native_pending.rs'
    'src/host_effect/native_outcome.rs'
    'src/host_effect/wake.rs'
    'src/host_effect/wake/native_wait.rs'
)
source_hashes=(
    '3e1fb935bb5006de271ce2caaa472ebbf4a062ae102db2f11521f972024183bf'
    '1f10de6b4bac7e3864bb4495e58f9dea5e3b1014de66ae8fddc82c34c1aa06cc'
    '6babe0bdd00b704be321045e3b66b8c12fbfc28558a8cb6be52f6b4589324b09'
    '8f49ea0b24843a24862059202e76a0fe7bffcfba00ca8d591d569e13367c7792'
    '8377e5c9b967498df0fc22016b17a277f9d6cc07c5537975295ffac45081b630'
    '036a0ae104efdcff433efdfe3c3991f8c9d185e9c56e7b99e316edd5cd32ff4a'
    'd39f205e89ecd607d793715f3f7009ceccd136deb85989c830a38657cf050b4b'
    '4096430dcd45f7829e9752bd00635b9241ab8bbd67bd0d16993ceeab31fb5a6d'
    'c87cab72fa3a0d20954b2cb18b2ebc71aff217ea156835e29fd03a748abba36a'
    '7275fe554bb8c5134395012bc6a2c33b813958025d160672ef871be94ec667a1'
    'a2d75c4f9a710d362a983a7d779c45902d1ec6505490a2390fb44a0eac63976f'
    'c95e530e27dc55ec6daf562f65d85d38f6c7a6d36f5eea120315db37ce2a45c2'
    '5872a982be4fa90284916a29e85e2ed10eccd4c91ebd3f39a50dfcb4c2940c4f'
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

export CARGO_TARGET_DIR="$CARGO_TARGET_DIR/host-green-service-tsan"
unset CARGO_ENCODED_RUSTFLAGS RUST_LOG
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Zsanitizer=thread -Cforce-frame-pointers=yes"
export TSAN_OPTIONS="${TSAN_OPTIONS:+$TSAN_OPTIONS:}halt_on_error=1:exitcode=66"

rustup run "$toolchain" rustc -vV
# This executes the ACTUAL main-owned pump, switch and scheduler. The focused
# fixtures disable JIT execution and use OS threads, not stack-switching fibers.
# TSan reserves a large virtual address range; the dedicated config omits the
# ordinary RLIMIT_AS wrapper. Admission and the outer sandbox still bound RSS.
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
export RUSTC_WRAPPER=
for generation in 1 0; do
    NEOVM_GC_GENERATIONAL="$generation" rustup run "$toolchain" cargo nextest run \
        --manifest-path "$harness_dir/../../Cargo.toml" \
        --locked --offline -p neovm-core --lib --target "$target" \
        -Z build-std=std,panic_abort \
        --config-file "$harness_dir/green-tsan-nextest.toml" \
        -E 'test(/host_service::runtime_tests::/)' \
        --test-threads 1 --retries 0 --no-tests fail --no-fail-fast
    printf 'TSAN_REAL_GREEN_PUMP_GEN%s_PASS\n' "$generation"
done
