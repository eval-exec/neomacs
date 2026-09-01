#!/usr/bin/env bash
# Ledger 205: run one parity audit .el in one editor under a pty.
#
#   scripts/l205-audit-run.sh EDITOR AUDIT.el OUT_ENV OUTFILE REDISPLAY_ENV N COLS ROWS
#
# e.g.  scripts/l205-audit-run.sh emacs scripts/posn-parity-audit.el \
#         L201_OUT ./tmp/l205/p201-gnu-warm.txt L201_REDISPLAY 1 160 50
set -u
editor="$1"
audit="$2"
out_env="$3"
out="$4"
red_env="$5"
red="$6"
cols="${7:-160}"
rows="${8:-50}"
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
export "$out_env=$out"
export "$red_env=$red"
python3 scripts/motion-parity-pty.py "$editor" -nw -Q -l "$audit" > "$out.pty.log" 2>&1
status=$?
lines=$( [ -f "$out" ] && wc -l < "$out" || echo MISSING )
echo "pty exit=$status out=$out lines=$lines"
# Ledger 211: the runner already KNOWS the editor's exit status; it should also
# interpret it.  127 is the shell's answer for "not found or not executable",
# and scripts/motion-parity-pty.py exits with it deliberately.  An editor that
# could not be RUN is a different failure from an editor that ran and wrote
# nothing, and only the second is a fact about the sweep -- but both used to
# print the same generic "produced no probes" below.  Found when a rebuild in
# the SHARED GNU mirror deleted src/emacs mid-session: the GNU side failed with
# an EMPTY pty log, and a reader who did not go looking would have had no way to
# tell that from a port problem.
if [ "$status" -eq 127 ]; then
  echo "l205-audit-run: the EDITOR could not be RUN: $editor" >&2
  echo "  -- not found, not executable, or a broken symlink (exit 127)." >&2
  echo "  -- this is a fact about the EDITOR, not a sweep result." >&2
fi
# Ledger 210: a sweep that wrote nothing is a failed sweep.  Ask what this
# check reports when the artifact is EMPTY, not only when it is absent.
if [ "$lines" = MISSING ] || [ "$lines" -eq 0 ]; then
  echo "l205-audit-run: $editor produced no probes -- see $out.pty.log" >&2
  [ "$status" -eq 0 ] && status=1
fi
exit "$status"
