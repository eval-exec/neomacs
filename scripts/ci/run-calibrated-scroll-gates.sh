#!/usr/bin/env bash
# Calibrated-host 120 Hz continuous-scroll gates.
#
# The two `continuous_120hz_scroll_in_large_{plain,rich}_buffer` scenarios are
# performance gates, not functional smoke tests: native_scrolling/budget.rs
# selects a response budget from a recorded software-Vulkan capacity tier and
# deliberately fails on a host outside the recorded tiers. A shared GitHub
# runner is such a host (the same AMD EPYC 9V74 model has measured 2.8 ms and
# 4.0 ms llvmpipe capacity on different runs), so the shared GUI job skips
# them. Run this script on the calibrated batch-gate host instead, after
# `cargo xtask fresh-build --release` in the same worktree.
#
# Requires glmark2, xvfb-run, timeout, and taskset (scripts/ci/setup-linux.sh
# ecosystem installs the first three; the Nix dev shell provides them too).
#
# CPU pin: P-cores only, and off the measurement island. On this i9-14900K,
# /sys/devices/cpu_core/cpus is 0-15 (P-cores, SMT pairs) and
# /sys/devices/cpu_atom/cpus is 16-31 (E-cores). The island is 4-11. The
# off-island P-core threads are 0-3,14-15. The wider off-island set
# 0-3,14-31 includes the E-cores; llvmpipe then lands outside the recorded
# tier (median 6.76 ms, OutsideRecordedCapacity). This script pins the
# capacity probe and the tests to 0-3,14-15. Logical CPUs 12-13 are also
# off-island P-cores and are left unused.
#
# The probe uses the same glmark2 invocation and the same cutoffs as
# budget.rs: median <= 3.0 ms is SoftwareVulkanFast, <= 3.6 ms is
# SoftwareVulkanReducedCapacity, and anything slower is
# OutsideRecordedCapacity. The calibrated host's recorded tier is
# SoftwareVulkanFast. Any other measured tier exits before nextest.
#
# Evidence the gate is green on the calibrated host (2026-11-18, i9-14900K,
# llvmpipe frames 3.320/2.906/2.798/2.690/2.267 ms, median 2.798 ms,
# SoftwareVulkanFast): plain PASS 5.68 s, rich PASS 15.13 s. That sample
# predates this pin; the batch gate must still run in its quiet window.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
pin=0-3,14-15
expected_tier=SoftwareVulkanFast

: "${NEOMACS_GUI_TEST_BINARY:=$repo_root/target/release/neomacs}"
export NEOMACS_GUI_TEST_BINARY
export NEOMACS_GUI_TEST_BACKEND="${NEOMACS_GUI_TEST_BACKEND:-x11}"

for command in glmark2 xvfb-run timeout taskset; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "calibrated scroll gate: $command is missing (see scripts/ci/setup-linux.sh ecosystem)" >&2
        exit 2
    fi
done
if [[ ! -x "$NEOMACS_GUI_TEST_BINARY" ]]; then
    echo "calibrated scroll gate: no executable at $NEOMACS_GUI_TEST_BINARY; run cargo xtask fresh-build --release first" >&2
    exit 2
fi

probe_out="$(mktemp)"
probe_err="$(mktemp)"
trap 'rm -f "$probe_out" "$probe_err"' EXIT

if ! taskset -c "$pin" \
    env LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe LP_NUM_THREADS=4 vblank_mode=0 \
    timeout --kill-after=2s 25s \
    xvfb-run --auto-servernum --server-args='-screen 0 1280x800x24 -nolisten tcp' \
    glmark2 --off-screen --size 1280x800 --frame-end finish --reuse-context \
    --benchmark 'shading:shading=phong:duration=1.0' \
    --benchmark 'shading:shading=phong:duration=1.0' \
    --benchmark 'shading:shading=phong:duration=1.0' \
    --benchmark 'shading:shading=phong:duration=1.0' \
    --benchmark 'shading:shading=phong:duration=1.0' \
    >"$probe_out" 2>"$probe_err"
then
    echo "calibrated scroll gate: capacity probe failed on pin $pin" >&2
    cat "$probe_err" >&2
    exit 2
fi

tier_line="$(awk '
/GL_RENDERER:/ {
    renderer = $0
    sub(/.*GL_RENDERER:[[:space:]]*/, "", renderer)
    if (tolower(renderer) ~ /llvmpipe/) llvmpipe = 1
}
/FrameTime:/ {
    rest = $0
    sub(/.*FrameTime:[[:space:]]*/, "", rest)
    split(rest, fields, /[[:space:]]+/)
    if (fields[1] != "") frames[++n] = fields[1] + 0
}
END {
    if (!llvmpipe || n != 5) {
        printf "invalid %d\n", n
        exit 0
    }
    for (i = 1; i <= 5; i++) sorted[i] = frames[i]
    for (i = 1; i <= 5; i++) {
        for (j = i + 1; j <= 5; j++) {
            if (sorted[j] < sorted[i]) {
                tmp = sorted[i]
                sorted[i] = sorted[j]
                sorted[j] = tmp
            }
        }
    }
    median = sorted[3]
    if (median <= 3.0) tier = "SoftwareVulkanFast"
    else if (median <= 3.6) tier = "SoftwareVulkanReducedCapacity"
    else tier = "OutsideRecordedCapacity"
    printf "%s %.3f", tier, median
    for (i = 1; i <= 5; i++) printf " %.3f", frames[i]
    printf "\n"
}
' "$probe_out")"

measured_tier="${tier_line%% *}"
if [[ "$measured_tier" == "invalid" || "$measured_tier" != "$expected_tier" ]]; then
    echo "calibrated scroll gate: measured host tier ${tier_line:-missing} does not match recorded tier $expected_tier" >&2
    echo "calibrated scroll gate: pin $pin (P-cores 0-15, E-cores 16-31, island 4-11 excluded)" >&2
    exit 2
fi
echo "calibrated scroll gate: pin $pin tier $tier_line"

cd "$repo_root"
exec taskset -c "$pin" cargo nextest run -p neomacs-gui-tests \
    -E 'test(continuous_120hz_)' --no-fail-fast
