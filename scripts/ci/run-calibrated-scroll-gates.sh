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
# Requires glmark2, xvfb-run and timeout (scripts/ci/setup-linux.sh ecosystem
# installs them; the Nix dev shell provides them too). The caller decides the
# CPU pin (the batch gate uses its off-island set).
#
# Evidence the gate is green on the calibrated host (2026-11-18, i9-14900K,
# llvmpipe frames 3.320/2.906/2.798/2.690/2.267 ms, median 2.798 ms,
# SoftwareVulkanFast): plain PASS 5.68 s, rich PASS 15.13 s.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

: "${NEOMACS_GUI_TEST_BINARY:=$repo_root/target/release/neomacs}"
export NEOMACS_GUI_TEST_BINARY
export NEOMACS_GUI_TEST_BACKEND="${NEOMACS_GUI_TEST_BACKEND:-x11}"

for command in glmark2 xvfb-run timeout; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "calibrated scroll gate: $command is missing (see scripts/ci/setup-linux.sh ecosystem)" >&2
        exit 2
    fi
done
if [[ ! -x "$NEOMACS_GUI_TEST_BINARY" ]]; then
    echo "calibrated scroll gate: no executable at $NEOMACS_GUI_TEST_BINARY; run cargo xtask fresh-build --release first" >&2
    exit 2
fi

cd "$repo_root"
exec cargo nextest run -p neomacs-gui-tests \
    -E 'test(continuous_120hz_)' --no-fail-fast
