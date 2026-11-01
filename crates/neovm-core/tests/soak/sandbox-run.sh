#!/usr/bin/env bash
# Portable P11 soak sandbox launcher (tracked; the soak driver invokes it).
#
# usage: sandbox-run.sh [taskset -c <cpus>] [perf stat -e instructions:u -x, -o <file>] <command...>
# env:  SANDBOX_MEM_GB             memory cap for the systemd scope (default 8)
#       SOAK_MODE                  smoke|reference|arm; an arm refuses an uncapped launcher
#       SOAK_ALLOW_UNSANDBOXED_ARM=1  allow an arm run without the full scope
#       SOAK_SANDBOX_MODE         scope|scope-nopidns|unshare|direct forces a mode
#                                  (test hook; default auto)
#
# Isolation modes, best first. Linux only; other platforms use direct exec.
#   scope          systemd user scope: MemoryMax=${SANDBOX_MEM_GB}G, no swap, PID
#                  namespace (unshare --user --pid --kill-child), no core dumps.
#   scope-nopidns  the same memory-capped scope and core limit without the PID
#                  namespace, for hosts that deny unprivileged user namespaces
#                  (GitHub's Ubuntu 24.04 runners restrict them through AppArmor:
#                  unshare dies with "write failed /proc/self/uid_map: Operation
#                  not permitted"). The driver's timeout kills the process tree.
#   unshare        PID namespace + no core dumps, but NO memory cap: thread stacks
#                  are virtual reservations, so an RLIMIT_AS cap makes
#                  pthread_create fail with EAGAIN at a few hundred threads; the
#                  driver's wall-clock timeout is the bound.
#   direct         plain exec (macOS and other hosts, or when every mode is denied).
# Long arm runs require the full scope.
#
# Each mode is probed by running `true` under the exact prefix it will exec, so
# a host where systemd-run works but the namespace hop inside it is denied
# never reports a scope it cannot use.
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

scope_prefix=(systemd-run --user --scope --quiet -p "MemoryMax=${MEM}G"
              -p MemorySwapMax=0 --)
pidns_prefix=(unshare --user --map-current-user --pid --fork --mount-proc
              --kill-child --)
nocore_prefix=(prlimit --core=0 --)

mode=direct
prefix=()
forced=${SOAK_SANDBOX_MODE:-}
if [ "$(uname -s)" = "Linux" ]; then
    for candidate in scope scope-nopidns unshare; do
        if [ -n "$forced" ] && [ "$forced" != "$candidate" ]; then
            continue
        fi
        case "$candidate" in
            scope)
                try=("${scope_prefix[@]}" "${pidns_prefix[@]}" "${nocore_prefix[@]}") ;;
            scope-nopidns)
                try=("${scope_prefix[@]}" "${nocore_prefix[@]}") ;;
            unshare)
                try=("${pidns_prefix[@]}" "${nocore_prefix[@]}") ;;
        esac
        if "${try[@]}" true >/dev/null 2>&1; then
            mode=$candidate
            prefix=("${try[@]}")
            break
        fi
    done
fi

if [ "${SOAK_MODE:-smoke}" = "arm" ] && [ "$mode" != "scope" ] &&
   [ "${SOAK_ALLOW_UNSANDBOXED_ARM:-0}" != "1" ]; then
    echo "soak-sandbox: refusing SOAK_MODE=arm without the memory-capped systemd scope with a PID namespace (mode=$mode); set SOAK_ALLOW_UNSANDBOXED_ARM=1 to override" >&2
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

exec "${prefix[@]}" "${inner[@]}"
