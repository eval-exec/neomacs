#!/usr/bin/env bash
# Calibrated-host 120 Hz continuous-scroll gates.
#
# The two `continuous_120hz_scroll_in_large_{plain,rich}_buffer` scenarios are
# performance gates, not functional smoke tests: native_scrolling/budget.rs
# selects a response budget from a recorded software-Vulkan capacity tier and
# deliberately fails on a host outside the recorded tiers.
#
# Placement policy:
#   - Run on a dedicated idle calibration host (e.g. `treeroot`) via:
#       scripts/ci/run-calibrated-scroll-gates.sh --remote treeroot \
#         --remote-runner /path/to/remote-run.sh
#     which dispatches under `REMOTE_RUN_WHOLE_HOST=1` (queuing behind
#     in-flight jobs up to `--queue-timeout`, or failing fast with exit 3 on
#     `--fail-if-busy`).
#   - Local execution is REFUSED with a hard non-zero exit (exit 2) unless
#     `--allow-local` is passed explicitly. When `--allow-local` is used, local
#     1-minute loadavg must be < 2.0 (unless `--allow-loaded-local` is also
#     passed), and both stdout and the JSON provenance artifact are stamped
#     `provenance=local-UNTRUSTED` with `host=<local> load=<1m 5m 15m>` so a
#     local run can never be mistaken for a calibrated host measurement.
#
# This script does NOT change thresholds or the measurement method: it runs the
# same glmark2 capacity probe (cutoffs <= 3.0 ms SoftwareVulkanFast, <= 3.6 ms
# SoftwareVulkanReducedCapacity) and the same nextest filter
# `test(continuous_120hz_)`.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

usage() {
    cat <<'EOF'
Usage: scripts/ci/run-calibrated-scroll-gates.sh [OPTIONS]

Modes (one required):
  --remote HOST          Run on calibrated remote host (e.g. treeroot) via
                         --remote-runner (or SCROLL120_REMOTE_RUN_BIN) with
                         REMOTE_RUN_WHOLE_HOST=1.
  --allow-local          Explicit untrusted escape hatch to run locally; stamps
                         provenance=local-UNTRUSTED in stdout and artifact.

Options:
  --remote-runner PATH   Executable remote dispatch wrapper for --remote
                         (or set SCROLL120_REMOTE_RUN_BIN).
  --allow-loaded-local   With --allow-local, allow running when 1m loadavg >= 2.0.
  --lane KEY             Remote lane key for remote runner (default: scroll120).
  --artifact PATH        JSON provenance artifact path
                         (default: ${TMPDIR:-/tmp}/scroll120-calibrated-provenance.json).
  --queue-timeout SEC    Max seconds to wait for remote lock + run
                         (default: 1800; never silently hangs).
  --fail-if-busy         With --remote, probe remote flock slots first and
                         exit 3 immediately with instructions if busy.
  --on-remote-host HOST  Internal flag used by --remote when executing on HOST
                         (verified against actual hostname -s).
  -h, --help             Show this help.
EOF
}

remote_host="${SCROLL120_REMOTE_HOST:-}"
remote_runner="${SCROLL120_REMOTE_RUN_BIN:-}"
on_remote_host=""
allow_local=0
allow_loaded_local=0
lane="${SCROLL120_LANE:-scroll120}"
artifact_path="${SCROLL120_ARTIFACT:-${TMPDIR:-/tmp}/scroll120-calibrated-provenance.json}"
queue_timeout="${SCROLL120_QUEUE_TIMEOUT_SEC:-1800}"
fail_if_busy="${SCROLL120_FAIL_IF_BUSY:-0}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --remote)
            [[ $# -ge 2 ]] || { echo "calibrated scroll gate: --remote requires HOST" >&2; exit 2; }
            remote_host="$2"
            shift 2
            ;;
        --remote-runner)
            [[ $# -ge 2 ]] || { echo "calibrated scroll gate: --remote-runner requires PATH" >&2; exit 2; }
            remote_runner="$2"
            shift 2
            ;;
        --on-remote-host)
            [[ $# -ge 2 ]] || { echo "calibrated scroll gate: --on-remote-host requires HOST" >&2; exit 2; }
            on_remote_host="$2"
            shift 2
            ;;
        --allow-local)
            allow_local=1
            shift
            ;;
        --allow-loaded-local)
            allow_loaded_local=1
            shift
            ;;
        --lane)
            [[ $# -ge 2 ]] || { echo "calibrated scroll gate: --lane requires KEY" >&2; exit 2; }
            lane="$2"
            shift 2
            ;;
        --artifact)
            [[ $# -ge 2 ]] || { echo "calibrated scroll gate: --artifact requires PATH" >&2; exit 2; }
            artifact_path="$2"
            shift 2
            ;;
        --queue-timeout)
            [[ $# -ge 2 ]] || { echo "calibrated scroll gate: --queue-timeout requires SEC" >&2; exit 2; }
            queue_timeout="$2"
            shift 2
            ;;
        --fail-if-busy)
            fail_if_busy=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "calibrated scroll gate: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [[ -n "$remote_host" && "$allow_local" -eq 1 ]]; then
    echo "calibrated scroll gate: --remote and --allow-local are mutually exclusive" >&2
    exit 2
fi
if [[ "$allow_loaded_local" -eq 1 && "$allow_local" -ne 1 ]]; then
    echo "calibrated scroll gate: --allow-loaded-local requires --allow-local" >&2
    exit 2
fi
if ! [[ "$queue_timeout" =~ ^[0-9]+$ ]] || [[ "$queue_timeout" -le 0 ]]; then
    echo "calibrated scroll gate: --queue-timeout must be a positive integer, got '$queue_timeout'" >&2
    exit 2
fi

ssh_bin="${SCROLL120_SSH_BIN:-ssh}"
scp_bin="${SCROLL120_SCP_BIN:-scp}"
loadavg_file="${SCROLL120_LOADAVG_FILE:-/proc/loadavg}"
governor_file="${SCROLL120_GOVERNOR_FILE:-/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor}"
stream_inputs="${NEOMACS_GUI_SCROLL_STREAM_INPUTS:-480}"
if ! [[ "$stream_inputs" =~ ^[0-9]+$ ]] || [[ "$stream_inputs" -le 0 ]]; then
    echo "calibrated scroll gate: NEOMACS_GUI_SCROLL_STREAM_INPUTS must be positive, got '$stream_inputs'" >&2
    exit 2
fi

read_loadavg() {
    if [[ -r "$loadavg_file" ]]; then
        cut -d' ' -f1-3 "$loadavg_file"
    else
        echo "unavailable"
    fi
}

read_governor() {
    if [[ -r "$governor_file" ]]; then
        cat "$governor_file"
    else
        echo "unavailable"
    fi
}

read_power_profile() {
    if command -v powerprofilesctl >/dev/null 2>&1; then
        powerprofilesctl get 2>/dev/null || echo "unavailable"
    else
        echo "unavailable"
    fi
}

read_actual_hostname() {
    if [[ -n "${SCROLL120_TEST_ACTUAL_HOSTNAME:-}" ]]; then
        echo "$SCROLL120_TEST_ACTUAL_HOSTNAME"
    else
        hostname -s 2>/dev/null || hostname
    fi
}

remote_lock_list_for_host() {
    local host_lower="${1,,}"
    if [[ -n "${SCROLL120_REMOTE_LOCK_PATHS:-}" ]]; then
        echo "$SCROLL120_REMOTE_LOCK_PATHS"
        return 0
    fi
    case "$host_lower" in
        treeroot)
            echo "~/.treeroot-lock.gate ~/.treeroot-lock ~/.treeroot-lock.2 ~/.treeroot-lock.3"
            ;;
        macmini)
            echo "~/.macmini-lock"
            ;;
        *)
            echo "~/.${host_lower}-lock"
            ;;
    esac
}

write_provenance_json() {
    local out_file="$1"
    mkdir -p "$(dirname -- "$out_file")"
    python3 -I - "$out_file" \
        "$prov_tag" \
        "$prov_host" \
        "$prov_tip_sha" \
        "$prov_governor" \
        "$prov_power_profile" \
        "$prov_load_start" \
        "$prov_load_end" \
        "$prov_cpu_pin" \
        "$prov_tier" \
        "$prov_tier_line" \
        "$prov_binary" \
        "$prov_binary_sha256" \
        "$prov_oracle_profile" \
        "$prov_stream_inputs" \
        "$prov_probe_samples" \
        "$prov_scenario_count" \
        "$prov_utc_start" \
        "$prov_utc_end" \
        "$prov_exit_code" <<'PYEOF'
import json, pathlib, sys
(
    out_file, prov_tag, host, tip_sha, governor, power_profile,
    load_start, load_end, cpu_pin, tier, tier_line, binary_path,
    binary_sha256, oracle_profile, stream_inputs, probe_samples,
    scenario_count, utc_start, utc_end, exit_code,
) = sys.argv[1:21]
doc = {
    "schema_version": 2,
    "provenance": prov_tag,
    "host": host,
    "tip_sha": tip_sha,
    "governor": governor,
    "power_profile": power_profile,
    "loadavg_start": load_start,
    "loadavg_end": load_end,
    "cpu_pin": cpu_pin,
    "n": {
        "stream_inputs": int(stream_inputs),
        "probe_samples": int(probe_samples),
        "scenarios": int(scenario_count),
    },
    "capacity_tier": tier,
    "capacity_probe": tier_line,
    "binary_path": binary_path,
    "binary_sha256": binary_sha256,
    "oracle_profile": oracle_profile if oracle_profile else None,
    "utc_start": utc_start,
    "utc_end": utc_end,
    "exit_code": int(exit_code),
}
pathlib.Path(out_file).write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
PYEOF
}

# --- Remote dispatch path (--remote HOST) ------------------------------------
if [[ -n "$remote_host" ]]; then
    if [[ -z "$remote_runner" || ! -x "$remote_runner" ]]; then
        echo "calibrated scroll gate: --remote requires an executable --remote-runner PATH (or SCROLL120_REMOTE_RUN_BIN)" >&2
        exit 2
    fi
    tip_sha="${SCROLL120_TIP_SHA:-$(git -C "$repo_root" rev-parse HEAD 2>/dev/null || echo unknown)}"
    if [[ "$fail_if_busy" -eq 1 ]]; then
        lock_list="$(remote_lock_list_for_host "$remote_host")"
        set +e
        "$ssh_bin" -o BatchMode=yes "$remote_host" "bash -lc '
            for lk in $lock_list; do
                flock -n -E 254 -o \"\$lk\" true || exit 254
            done
        '"
        ssh_rc=$?
        set -e
        if [[ "$ssh_rc" -eq 254 ]]; then
            echo "calibrated scroll gate: $remote_host is busy (host flock held); re-run without --fail-if-busy to queue up to ${queue_timeout}s" >&2
            exit 3
        elif [[ "$ssh_rc" -ne 0 ]]; then
            echo "calibrated scroll gate: SSH busy-probe to $remote_host failed (exit $ssh_rc); cannot verify host lock state" >&2
            exit "$ssh_rc"
        fi
    else
        echo "calibrated scroll gate: queueing on $remote_host whole-host lock (lane=$lane, timeout=${queue_timeout}s; pass --fail-if-busy to fail fast)" >&2
    fi

    run_nonce="$(date -u +%Y%m%dT%H%M%SZ)-$$"
    remote_artifact="/tmp/scroll120-${lane}-${run_nonce}-provenance.json"
    # Remove any prior local artifact so a failed/timed-out remote run can
    # never appear to produce a stale provenance file from an earlier run.
    rm -f "$artifact_path"

    remote_env_prefix="SCROLL120_TIP_SHA=$(printf '%q' "$tip_sha")"
    remote_env_prefix+=" SCROLL120_ORACLE_BIN=$(printf '%q' "${SCROLL120_ORACLE_BIN:-}")"
    remote_env_prefix+=" SCROLL120_ORACLE_MANIFEST=$(printf '%q' "${SCROLL120_ORACLE_MANIFEST:-}")"
    remote_env_prefix+=" NEOMACS_GUI_SCROLL_STREAM_INPUTS=$(printf '%q' "$stream_inputs")"
    if [[ -n "${SCROLL120_CPU_PIN:-}" ]]; then
        remote_env_prefix+=" SCROLL120_CPU_PIN=$(printf '%q' "$SCROLL120_CPU_PIN")"
    fi
    if [[ -n "${NEOMACS_GUI_TEST_BACKEND:-}" ]]; then
        remote_env_prefix+=" NEOMACS_GUI_TEST_BACKEND=$(printf '%q' "$NEOMACS_GUI_TEST_BACKEND")"
    fi
    if [[ -n "${SCROLL120_USE_PREBUILT_BINARY:-}" ]]; then
        remote_env_prefix+=" SCROLL120_USE_PREBUILT_BINARY=$(printf '%q' "$SCROLL120_USE_PREBUILT_BINARY")"
    fi
    remote_cmd="rm -f $(printf '%q' "$remote_artifact") && $remote_env_prefix bash scripts/ci/run-calibrated-scroll-gates.sh --on-remote-host $(printf '%q' "$remote_host") --artifact $(printf '%q' "$remote_artifact")"

    set +e
    exec 3< <(sleep "$queue_timeout")
    keepopen_pid=$!
    TMPDIR=/tmp REMOTE_RUN_WHOLE_HOST=1 \
        timeout --kill-after=10s "${queue_timeout}s" \
        "$remote_runner" "$remote_host" "$lane" "$repo_root" "$remote_cmd" <&3
    remote_rc=$?
    exec 3<&-
    kill "$keepopen_pid" 2>/dev/null || true
    wait "$keepopen_pid" 2>/dev/null || true
    set -e

    mkdir -p "$(dirname -- "$artifact_path")"
    "$scp_bin" -q -o BatchMode=yes "$remote_host:$remote_artifact" "$artifact_path" 2>/dev/null || true
    "$ssh_bin" -o BatchMode=yes "$remote_host" "rm -f $(printf '%q' "$remote_artifact")" 2>/dev/null || true

    if [[ "$remote_rc" -eq 124 ]]; then
        echo "calibrated scroll gate: timed out after ${queue_timeout}s waiting for/running on $remote_host (lane=$lane)" >&2
    fi
    exit "$remote_rc"
fi

# --- Local refusal & hostname verification guard -----------------------------
actual_host="$(read_actual_hostname)"
if [[ -z "$on_remote_host" && "$allow_local" -ne 1 ]]; then
    local_load="$(read_loadavg)"
    echo "calibrated scroll gate: REFUSING local execution (host=$actual_host load=$local_load)." >&2
    echo "calibrated scroll gate: run on the calibrated idle host instead: scripts/ci/run-calibrated-scroll-gates.sh --remote treeroot --remote-runner <path>" >&2
    echo "calibrated scroll gate: or pass --allow-local explicitly (stamps provenance=local-UNTRUSTED)." >&2
    exit 2
fi
if [[ -n "$on_remote_host" ]]; then
    if [[ "${actual_host,,}" != "${on_remote_host,,}" ]]; then
        echo "calibrated scroll gate: REFUSING --on-remote-host $on_remote_host on mismatched host $actual_host" >&2
        exit 2
    fi
fi

prov_utc_start="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
prov_host="$actual_host"
prov_tip_sha="${SCROLL120_TIP_SHA:-$(git -C "$repo_root" rev-parse HEAD 2>/dev/null || echo unknown)}"
prov_governor="$(read_governor)"
prov_power_profile="$(read_power_profile)"
prov_load_start="$(read_loadavg)"
prov_load_end="$prov_load_start"
prov_cpu_pin=""
prov_tier="unmeasured"
prov_tier_line=""
prov_binary=""
prov_binary_sha256=""
prov_oracle_profile=""
prov_stream_inputs="$stream_inputs"
prov_probe_samples=5
prov_scenario_count=2
prov_exit_code=2
cleanup_done=0
probe_out=""
probe_err=""

if [[ -n "$on_remote_host" ]]; then
    prov_tag="${on_remote_host,,}-calibrated"
else
    prov_tag="local-UNTRUSTED"
    load_1m="${prov_load_start%% *}"
    if [[ "$allow_loaded_local" -ne 1 && "$load_1m" != "unavailable" ]]; then
        if ! awk -v l="$load_1m" 'BEGIN { exit !(l < 2.0) }'; then
            echo "calibrated scroll gate: REFUSING loaded local host (provenance=$prov_tag host=$prov_host load=$prov_load_start; requires load_1m < 2.0 or --allow-loaded-local)" >&2
            prov_utc_end="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
            prov_load_end="$(read_loadavg)"
            write_provenance_json "$artifact_path"
            exit 2
        fi
    fi
    echo "calibrated scroll gate: provenance=$prov_tag host=$prov_host load=$prov_load_start" >&2
fi

finalize_provenance() {
    local rc="$1"
    if [[ "$cleanup_done" -eq 1 ]]; then
        return 0
    fi
    cleanup_done=1
    [[ -n "$probe_out" ]] && rm -f "$probe_out"
    [[ -n "$probe_err" ]] && rm -f "$probe_err"
    prov_exit_code="$rc"
    prov_load_end="$(read_loadavg)"
    prov_utc_end="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    write_provenance_json "$artifact_path"
}

cleanup_exit() {
    local rc=$?
    finalize_provenance "$rc"
}

cleanup_signal() {
    local sig_rc="$1"
    finalize_provenance "$sig_rc"
    exit "$sig_rc"
}

# Install EXIT/TERM/INT traps BEFORE remote build and oracle attestation so
# early failures or timeouts always emit provenance with accurate exit codes.
trap cleanup_exit EXIT
trap 'cleanup_signal 143' TERM
trap 'cleanup_signal 130' INT

# CPU pin selection: preserve hybrid P-core off-island pin (0-3,14-15) on hosts
# with hybrid topology and >= 16 CPUs (i9-14900K), and use homogeneous 0-(N-1)
# on treeroot (12-thread i7-9750H).
if [[ -n "${SCROLL120_CPU_PIN:-}" ]]; then
    pin="$SCROLL120_CPU_PIN"
elif [[ -r /sys/devices/cpu_core/cpus && "$(nproc)" -ge 16 ]]; then
    pin="0-3,14-15"
else
    pin="0-$(( $(nproc) - 1 ))"
fi
prov_cpu_pin="$pin"

expected_tier="SoftwareVulkanFast"
: "${NEOMACS_GUI_TEST_BINARY:=$repo_root/target/release/neomacs}"
export NEOMACS_GUI_TEST_BINARY
export NEOMACS_GUI_TEST_BACKEND="${NEOMACS_GUI_TEST_BACKEND:-x11}"
export NEOMACS_GUI_SCROLL_STREAM_INPUTS="$stream_inputs"
prov_binary="$NEOMACS_GUI_TEST_BINARY"

# On a remote host, ALWAYS run `cargo xtask fresh-build --release` so the
# measured binary, `.pdump`, and `.elc` Lisp tree always correspond to
# `tip_sha`, unless the caller explicitly opts into a prebuilt binary whose
# `.fresh-build-stamp` SHA-256 matches the executable.
if [[ -n "$on_remote_host" ]]; then
    cd "$repo_root"
    if [[ "${SCROLL120_USE_PREBUILT_BINARY:-0}" == "1" ]]; then
        stamp_file="${NEOMACS_GUI_TEST_BINARY}.fresh-build-stamp"
        if [[ ! -x "$NEOMACS_GUI_TEST_BINARY" || ! -r "$stamp_file" ]]; then
            echo "calibrated scroll gate: SCROLL120_USE_PREBUILT_BINARY=1 requires executable $NEOMACS_GUI_TEST_BINARY and readable $stamp_file" >&2
            exit 2
        fi
        actual_sha="$(sha256sum "$NEOMACS_GUI_TEST_BINARY" | awk '{print $1}')"
        if ! grep -Fq "$actual_sha" "$stamp_file"; then
            echo "calibrated scroll gate: prebuilt binary SHA-256 $actual_sha does not match $stamp_file" >&2
            exit 2
        fi
    else
        cargo xtask fresh-build --release
    fi
fi

if [[ -x "$NEOMACS_GUI_TEST_BINARY" ]]; then
    prov_binary_sha256="$(sha256sum "$NEOMACS_GUI_TEST_BINARY" | awk '{print $1}')"
fi

# Optional GNU oracle attestation if GNU is measured in the same run.
if [[ -n "${SCROLL120_ORACLE_BIN:-}" && -n "${SCROLL120_ORACLE_MANIFEST:-}" ]]; then
    mc="$(awk -F'"' '/^mirror_commit = / { print $2 }' "$repo_root/parity-reference.toml")"
    ev="$(awk -F'"' '/^emacs_version = / { print $2 }' "$repo_root/parity-reference.toml")"
    prov_oracle_profile="$("$repo_root/scripts/ci/check-gnu-oracle.sh" \
        "$SCROLL120_ORACLE_BIN" "$ev" "$SCROLL120_ORACLE_MANIFEST" "$mc" | tr '\n' '; ')"
fi

for command in sway glmark2 xvfb-run timeout taskset; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "calibrated scroll gate: $command is missing on $prov_host (see scripts/ci/setup-linux.sh ecosystem; on Arch/CachyOS install sway and glmark2)" >&2
        exit 2
    fi
done
if [[ ! -x "$NEOMACS_GUI_TEST_BINARY" ]]; then
    echo "calibrated scroll gate: no executable at $NEOMACS_GUI_TEST_BINARY; run cargo xtask fresh-build --release first" >&2
    exit 2
fi

probe_out="$(mktemp)"
probe_err="$(mktemp)"

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
    echo "calibrated scroll gate: capacity probe failed on $prov_host pin $pin" >&2
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
prov_tier="${measured_tier:-invalid}"
prov_tier_line="$tier_line"
if [[ "$measured_tier" == "invalid" || "$measured_tier" != "$expected_tier" ]]; then
    echo "calibrated scroll gate: measured host tier ${tier_line:-missing} does not match recorded tier $expected_tier (host=$prov_host pin=$pin)" >&2
    exit 2
fi
echo "calibrated scroll gate: provenance=$prov_tag host=$prov_host pin=$pin gov=$prov_governor load_start=\"$prov_load_start\" tier $tier_line"

cd "$repo_root"
taskset -c "$pin" cargo nextest run -p neomacs-gui-tests \
    -E 'test(continuous_120hz_)' --no-fail-fast
