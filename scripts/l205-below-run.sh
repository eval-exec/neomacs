#!/usr/bin/env bash
# Ledger 205: run a below-content audit .el in one editor under a pty.
#
#   scripts/l205-below-run.sh EDITOR AUDIT.el REDISPLAYS OUTFILE [COLS] [ROWS]
#
# Same as scripts/below-content-run.sh but with the audit file as an argument,
# so the 14-case script this ledger's RED commit shipped can be re-run against
# the post-fix binary on exactly the basis the pre-fix numbers were taken on.
set -u
editor="$1"
audit="$2"
redisplays="$3"
out="$4"
cols="${5:-80}"
rows="${6:-24}"
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
export RUST_LOG=error
export L195_COLS="$cols"
export L195_ROWS="$rows"
export L195_TIMEOUT="${L195_TIMEOUT:-300}"
export L205_REDISPLAY="$redisplays"
export L205_OUT="$out"
python3 scripts/motion-parity-pty.py "$editor" -nw -Q -l "$audit" > "$out.pty.log" 2>&1
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
