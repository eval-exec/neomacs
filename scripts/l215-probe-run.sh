#!/usr/bin/env bash
# Ledger 215: run one probe .el in one editor under a pty.
#
#   scripts/l215-probe-run.sh EDITOR SCRIPT OUTVAR OUTFILE [COLS] [ROWS]
#
# OUTVAR is the environment variable the probe reads for its output path
# (L205_OUT, L209_OUT, L215_OUT ...), so the same runner drives every probe
# ledger 205, 209 and 215 committed.
set -u
editor="$1"
script="$2"
outvar="$3"
out="$4"
cols="${5:-80}"
rows="${6:-24}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
mkdir -p "$(dirname "$out")"
rm -f "$out"
unset RUST_LOG
export RUST_LOG=error
export L195_COLS="$cols"
export L195_ROWS="$rows"
export L195_TIMEOUT="${L195_TIMEOUT:-180}"
export "$outvar"="$out"
python3 scripts/motion-parity-pty.py "$editor" -nw -Q -l "$script" > "$out.pty.log" 2>&1
status=$?
lines=MISSING
[ -f "$out" ] && lines=$(wc -l < "$out")
echo "pty exit=$status editor=$editor script=$script out=$out lines=$lines"
# An empty or missing artifact is a FAILED probe, not a quiet success.
if [ "$lines" = MISSING ] || [ "$lines" -eq 0 ]; then
  echo "l215: probe produced no lines" >&2
  exit 3
fi
exit "$status"
