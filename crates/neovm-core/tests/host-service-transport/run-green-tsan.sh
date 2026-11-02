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
    'src/host_effect/contracts.rs'
    'src/host_effect/owner_step.rs'
    'src/host_effect/tests/owner_step_test.rs'
    'src/emacs_core/runtime/eval/green/host_service/owner_steps.rs'
    'src/emacs_core/runtime/eval/green/host_service/registered_payload.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/owner_steps.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/registered_payload.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/owner_runtime.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/semantic_pump.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/local_main.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/unstarted.rs'
    'src/emacs_core/runtime/eval/green/host_service/tests/prepublication.rs'
    'src/emacs_core/system/process/green_select.rs'
    'src/host_effect/native_pending.rs'
    'src/host_effect/native_outcome.rs'
    'src/host_effect/wake.rs'
    'src/host_effect/wake/native_wait.rs'
)
source_hashes=(
    '90d3e40a8e4d68ae02f8bae113e20367713b194e6cf8e6febb9be95ea3e4fdc6'
    '5deed51fd440f3e040097e7ba92409302d79c3a3d90de2981119c09c325fb65d'
    'faf39b34a0666a4f4653afcccbca35c86071fdaa03b649bf75a6d1b5c49f5b5a'
    '13e7132bc4d802f2c14a083ee8a02370fed937a0db5aefe91c08c6719693d74f'
    'fd5cc4e3d1ddee97e0436416ba14d3f2eb5f0b130a3d50004952650f14a88bc3'
    'ddecb8d2ff95559f9341ee4955d4718a10b86c4f05bd27a5da25c475d59f643a'
    'd39f205e89ecd607d793715f3f7009ceccd136deb85989c830a38657cf050b4b'
    '4096430dcd45f7829e9752bd00635b9241ab8bbd67bd0d16993ceeab31fb5a6d'
    '068aade6166a7958f7a5108af031090cad7f3e24a0ce953acbb6031d12063a53'
    '67ba28ff49ebef50616b3771e6ac290722658255902de638e24764eaca4e3c6f'
    '701d52d9853e342d81b4de728ab2fe6a45f39e91b715abd15fc2cf0782aa5d31'
    '567d037a520352cb20538235fb27837559ae68692de0683181a44083059ce3a4'
    '71dcf98847b4c4446b27234eaec5b98cd507e4ff22bd57070aad96ba3da6c0ed'
    '6272b058278b244415f864429c768de9535f311ebc8738e73399923f023bd0da'
    '8f898e06a66c6f9f29f7b6b15ec135b4a632e628714c12acf2120bf24c5e95d9'
    '4a8d093ecfeb53b73572be0eb054f9a61b84d65baf67fcc65554f05c19ecad71'
    '8d310914cf93f20491a1f4b46a4f5abf10f3e9c355ba3c5ea68122f6b94738d7'
    'dafd04870e81e92bf108061346f43cce51b09f7f05787a06c01d342503d4f431'
    'ece4e38efd0a7aa17cec1d2ac8a32ae9b739dde0f6298f384f320e75da6dcfda'
    '05a097f4fbef1ecc9437f9b9f3bdf90992ecc9dad8dc319f2505d120de5190e7'
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
        -E 'test(/host_service::/)' \
        --test-threads 1 --retries 0 --no-tests fail --no-fail-fast
    printf 'TSAN_REAL_GREEN_PUMP_GEN%s_PASS\n' "$generation"
done
