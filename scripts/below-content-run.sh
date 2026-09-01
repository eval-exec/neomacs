#!/usr/bin/env bash
# Ledger 205: run scripts/below-content-audit.el in one editor under a pty.
#
#   scripts/below-content-run.sh EDITOR REDISPLAYS OUTFILE [COLS] [ROWS]
#
# EDITOR is an absolute path or a name on PATH.  REDISPLAYS is L205_REDISPLAY
# (0 = COLD, 1 = WARM).  The pty geometry defaults to 80x24, which is the
# geometry ledger 204's residual 1 was measured in.
set -u
editor="$1"
redisplays="$2"
out="$3"
cols="${4:-80}"
rows="${5:-24}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
mkdir -p "$(dirname "$out")"
rm -f "$out"
# WHICH GNU (ledger 214).  This runner is handed either peer and cannot know
# the role, so it asks whether the editor IS a GNU Emacs and, if it is,
# requires it to be the pinned one.  Ledger 210 and 211 made this script fail
# loudly when an editor could not be RUN; a reference that CHANGED instead of
# vanishing used to pass straight through here into a published count.  The
# depth is `fingerprint': one 48-byte read, because this runs once per editor
# per probe file and several ledgers drive it in loops.
if ! reference="$(bash scripts/parity-reference-attest.sh --if-gnu "$editor" fingerprint)"; then
  echo "$(basename "$0"): the GNU reference did not attest -- refusing to take probes" >&2
  exit 3
fi
echo "reference $reference"
unset RUST_LOG
export RUST_LOG=error
export L195_COLS="$cols"
export L195_ROWS="$rows"
export L195_TIMEOUT="${L195_TIMEOUT:-240}"
export L205_REDISPLAY="$redisplays"
export L205_OUT="$out"
python3 scripts/motion-parity-pty.py "$editor" -nw -Q -l scripts/below-content-audit.el \
  > "$out.pty.log" 2>&1
status=$?
lines=$( [ -f "$out" ] && wc -l < "$out" || echo MISSING )
echo "pty exit=$status out=$out lines=$lines"
# Ledger 211 section 10.1: an editor that could not be RUN is a different
# failure from one that ran and wrote nothing.
if [ "$status" -eq 127 ]; then
  echo "$(basename "$0"): the EDITOR could not be RUN: $editor" >&2
  echo "  -- not found, not executable, or a broken symlink (exit 127)." >&2
  echo "  -- this is a fact about the EDITOR, not an audit result." >&2
fi
# Ledger 210: a run that wrote nothing is a failed run, and ledger 214 found
# that guard had only ever been added to scripts/l205-audit-run.sh -- these two
# siblings still exited 0 on an EMPTY artifact.  This is a HARNESS DEFECT of
# ledger 210's own class, not a divergence.
if [ "$lines" = MISSING ] || [ "$lines" -eq 0 ]; then
  echo "$(basename "$0"): $editor produced no probes -- see $out.pty.log" >&2
  [ "$status" -eq 0 ] && status=1
fi
exit "$status"
