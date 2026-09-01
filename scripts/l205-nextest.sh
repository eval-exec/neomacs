#!/usr/bin/env bash
# Ledger 205: run cargo nextest with RUST_LOG=error and the output in a file.
#
#   scripts/l205-nextest.sh LOGFILE [nextest args...]
set -u
log="$1"
shift
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
mkdir -p "$(dirname "$log")"
export RUST_LOG=error
cargo nextest run "$@" > "$log" 2>&1
status=$?
echo "nextest exit=$status log=$log"
tail -n 40 "$log"
exit "$status"
