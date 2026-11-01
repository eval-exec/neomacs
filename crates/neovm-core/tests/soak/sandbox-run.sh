#!/usr/bin/env bash
# Portable P11 soak sandbox launcher (tracked; the soak driver invokes it).
#
# usage: sandbox-run.sh [taskset -c <cpus>] [perf stat -e instructions:u -x, -o <file>] <command...>
# env:  SANDBOX_MEM_GB             memory cap for the systemd scope (default 8)
#       SOAK_MODE                  smoke|reference|arm; an arm refuses an uncapped launcher
#       SOAK_ALLOW_UNSANDBOXED_ARM=1  allow an arm run without a memory-capped scope
#       SOAK_SANDBOX_MODE         scope|unshare|direct forces a mode (test hook; default auto)
#
# Isolation modes, best first. Linux only; other platforms use direct exec.
#   scope    systemd user scope: MemoryMax=${SANDBOX_MEM_GB}G, no swap, PID namespace
#            (unshare --user --pid --kill-child), no core dumps.
#   unshare  PID namespace + no core dumps, but NO memory cap: thread stacks are
#            virtual reservations, so an RLIMIT_AS cap makes pthread_create fail
#            with EAGAIN at a few hundred threads; the driver's wall-clock
#            timeout is the bound. Long arm runs require the scope.
#   direct   plain exec (macOS and other hosts, or when namespaces are denied).
#
# The optional taskset and perf prefixes are dropped when unavailable or broken
# (e.g. a CI runner without a PMU, or a CPU set the runner does not have), so
# the smoke still runs; the driver tolerates a missing instruction count.
#
# Every generated or seeded Lisp form runs through this launcher (or the soak
# lane's own wrapper), never directly.

MEM=${SANDBOX_MEM_GB:-8}
args=("$@")

use_taskset=0
if [ "${args[0]:-}" = "taskset" ]; then
    cpus=${args[2]:-}
    if [ -n "$cpus" ] && command -v taskset >/dev/null 2>&1 &&
       taskset -c "$cpus" true >/dev/null 2>&1; then
        use_taskset=1
    fi
    args=("${args[@]:3}")
fi

use_perf=0
perf_out=""
if [ "${args[0]:-}" = "perf" ]; then
    perf_out=${args[6]:-}
    if command -v perf >/dev/null 2>&1 &&
       perf stat -e instructions:u -x, -o /dev/null -- true >/dev/null 2>&1; then
        use_perf=1
    fi
    args=("${args[@]:7}")
fi

mode=direct
forced=${SOAK_SANDBOX_MODE:-}
if [ "$(uname -s)" = "Linux" ]; then
    if [ -z "$forced" ] || [ "$forced" = "scope" ]; then
        if command -v systemd-run >/dev/null 2>&1 &&
           systemd-run --user --scope --quiet -p "MemoryMax=${MEM}G" \
               -p MemorySwapMax=0 -- true >/dev/null 2>&1; then
            mode=scope
        fi
    fi
    if [ "$mode" = "direct" ] && { [ -z "$forced" ] || [ "$forced" = "unshare" ]; }; then
        if command -v unshare >/dev/null 2>&1 &&
           unshare --user --map-current-user --pid --fork --mount-proc \
               true >/dev/null 2>&1; then
            mode=unshare
        fi
    fi
fi

if [ "${SOAK_MODE:-smoke}" = "arm" ] && [ "$mode" != "scope" ] &&
   [ "${SOAK_ALLOW_UNSANDBOXED_ARM:-0}" != "1" ]; then
    echo "soak-sandbox: refusing SOAK_MODE=arm without a memory-capped systemd scope (mode=$mode); set SOAK_ALLOW_UNSANDBOXED_ARM=1 to override" >&2
    exit 97
fi

echo "soak-sandbox: mode=$mode taskset=$use_taskset perf=$use_perf" >&2

inner=()
if [ "$use_taskset" = "1" ]; then
    inner=(taskset -c "$cpus")
fi
if [ "$use_perf" = "1" ]; then
    inner=("${inner[@]}" perf stat -e instructions:u -x, -o "$perf_out")
fi
inner=("${inner[@]}" "${args[@]}")

case "$mode" in
    scope)
        exec systemd-run --user --scope --quiet -p "MemoryMax=${MEM}G" \
            -p MemorySwapMax=0 -- \
            unshare --user --map-current-user --pid --fork --mount-proc \
                --kill-child -- \
            prlimit --core=0 -- "${inner[@]}"
        ;;
    unshare)
        exec unshare --user --map-current-user --pid --fork --mount-proc \
            --kill-child -- \
            prlimit --core=0 -- "${inner[@]}"
        ;;
    *)
        exec "${inner[@]}"
        ;;
esac
