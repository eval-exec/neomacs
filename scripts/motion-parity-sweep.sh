#!/usr/bin/env bash
# Run ledger 195's motion sweep the way its numbers should be published
# (ledger 210): in BOTH editors, at EVERY width in the documented set, under
# BOTH protocols, with each count carrying the frame it was measured in.
#
#   bash scripts/motion-parity-sweep.sh ./target/release/neomacs [GNU-EMACS]
#
# WHY A SET AND NOT A DEFAULT (ledger 210).  Every probe in this sweep is a
# question about a window of a particular width, and which divergences it can
# see depends on where that window's right edge cuts this fixed text.  At 160
# columns -- the pty driver's default, and where ledger 195 section 5.2, ledger
# 204 section 7 and ledger 205 all took their numbers -- the divergent set is a
# STRICT SUBSET of the 80-column one:
#
#   COLD  130 at 160x50   160 at 80x24    160-only 0    80-only 30
#   WARM  352 at 160x50   444 at 80x24    160-only 0    80-only 92
#
# So the documented default was the weaker gate, and one width cannot be the
# whole answer.  80 is kept for coverage, 160 for CONTINUITY: dropping it would
# make every motion number this ledger has already published uncomparable.
#
# 40 AND 60 ARE NOT IN THE SET, AND THE REASON IS THIS SCRIPT'S OWN GUARD.
# Scored across seven widths they look far stronger -- 212 COLD / 506 WARM at
# 40 against 160 / 444 at 80 -- but at 40 and 60 columns GNU's startup message
# wraps the echo area to two rows and the two editors then DISAGREE about the
# window height: COLD, GNU answers `window-body-height' 20 for all nine configs
# where this port answers 21, and one redisplay closes it for all but the first.
# That is a divergence in its own right (see scripts/l210-row-edge-probe.el's
# neighbours and ledger 209's residual 4), and until it is closed a count taken
# there is not a parity number: 32 of the 144 `mtwl-nil' probes diverge at 40
# columns and 0 at 80 or 160, and `mtwl-nil' is precisely the motion that reads
# the window height.  Add 40x24 and 60x24 to the set when that is fixed; the
# comparator refuses them today, which is the intended behaviour.
WIDTH_SET="80x24 160x50"

set -u
neo="${1:-./target/release/neomacs}"
gnu="${2:-emacs}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
out="${MOTION_PARITY_OUT:-./tmp/motion-parity}"
mkdir -p "$out"

# WHICH GNU (ledger 214).  Ledger 210 made every count carry the geometry it
# was measured in, and 210 and 211 made this sweep fail loudly when an editor
# could not be RUN.  Neither said WHICH GNU answered.  A reference that
# VANISHED was caught; a reference that CHANGED -- a rebuild of the shared
# mirror, which is one successful `make' away -- would have been scored in
# silence, and the counts below would have been published as if comparable
# with every earlier one.  So the reference is attested BEFORE any probe is
# taken, the sweep refuses outright on a mismatch, and the stamp is printed
# above the table so the numbers travel with what produced them.
#
# The depth is `exhaustive' because this costs ONCE per sweep -- about 70ms
# against a sweep that runs eight editors for minutes -- so there is no reason
# to take the cheaper check here.
if ! reference="$(bash scripts/parity-reference-attest.sh "$gnu" exhaustive)"; then
  echo "SWEEP REFUSED -- the GNU reference did not attest; see the refusal above" >&2
  exit 1
fi
# A parity number is a statement about a PAIR, so both halves travel with it.
# The port half is a different predicate -- correspondence with this tree, not
# equality with a pin -- and only its unplaceable case refuses; see
# scripts/parity-reference-attest.sh.  This closes what l205-provenance.sh
# cannot say: whether the binary matches the tree being measured.
if ! port="$(bash scripts/parity-reference-attest.sh --port "$neo")"; then
  echo "SWEEP REFUSED -- the port binary did not attest; see the refusal above" >&2
  exit 1
fi
printf 'reference  %s\n' "$reference"
printf 'port       %s\n' "$port"

status=0
printf '%-9s %-5s %s\n' geometry protocol result
for geom in $WIDTH_SET; do
  cols="${geom%x*}"
  rows="${geom#*x}"
  for prot in cold warm; do
    red=1
    [ "$prot" = cold ] && red=0
    for side in gnu neo; do
      editor="$gnu"
      [ "$side" = neo ] && editor="$neo"
      if ! bash scripts/l205-audit-run.sh "$editor" scripts/motion-parity-audit.el \
             L195_OUT "$out/$side-$geom-$prot.txt" L195_REDISPLAY "$red" \
             "$cols" "$rows" > "$out/$side-$geom-$prot.run.log" 2>&1; then
        printf '%-9s %-5s SWEEP FAILED (%s) -- see %s\n' \
               "$geom" "$prot" "$side" "$out/$side-$geom-$prot.run.log"
        status=1
        continue 3
      fi
    done
    # The comparator exits 2 when the two files describe different windows.  A
    # count taken across two geometries is not a parity number, so that is a
    # failure of the sweep and not a result of it.
    line="$(python3 scripts/motion-parity-compare.py \
              "$out/gnu-$geom-$prot.txt" "$out/neo-$geom-$prot.txt" \
              2> "$out/compare-$geom-$prot.err")"
    rc=$?
    if [ "$rc" -ne 0 ]; then
      printf '%-9s %-5s COMPARISON REFUSED (exit %s) -- see %s\n' \
             "$geom" "$prot" "$rc" "$out/compare-$geom-$prot.err"
      status=1
      continue
    fi
    printf '%-9s %-5s %s\n' "$geom" "$prot" "$(printf '%s\n' "$line" | head -1)"
  done
done

if [ "$status" -ne 0 ]; then
  echo "SWEEP INCOMPLETE -- do not publish a partial set" >&2
fi
exit "$status"
