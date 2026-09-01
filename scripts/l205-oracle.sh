#!/usr/bin/env bash
# Ledger 205: run the oracle suite against a real GNU Emacs, output to a file.
#
#   scripts/l205-oracle.sh LOGFILE
set -u
log="$1"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
mkdir -p "$(dirname "$log")"
export RUST_LOG=error
export NEOVM_FORCE_ORACLE_PATH="$(command -v emacs)"
echo "oracle emacs = $NEOVM_FORCE_ORACLE_PATH" > "$log"
"$NEOVM_FORCE_ORACLE_PATH" --version | head -1 >> "$log"
cargo nextest run -p neovm-oracle-tests --no-fail-fast >> "$log" 2>&1
status=$?
echo "nextest exit=$status"
grep -E "^ +Summary|tests run:" "$log" | tail -3
exit "$status"
