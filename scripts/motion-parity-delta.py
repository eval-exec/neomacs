#!/usr/bin/env python3
"""Score one motion sweep against another: what got FIXED, and what got WORSE.

  scripts/motion-parity-delta.py BEFORE_GNU BEFORE_NEO AFTER_GNU AFTER_NEO

Ledger 211.  A parity count going down is not on its own evidence that a change
was an improvement: it can fix twelve probes and break three, and the headline
`divergent=' will still fall.  The number a behaviour change has to publish is
the one this tool exists to compute -- NEWLY DIVERGENT, probes the two editors
agreed on BEFORE and disagree on AFTER -- and it must be zero.

Every refusal ledger 210 built into scripts/motion-parity-compare.py applies
here and is inherited from it rather than reimplemented, because this tool asks
a STRICTLY harder question than that one:

  * a file with no probes, or fewer than the sweep says it wrote, is refused
    with exit 3 -- two empty files scoring `newly divergent 0' would be the
    same false green in a new place;
  * two files that describe different windows are refused with exit 2;
  * and the BEFORE and AFTER runs must themselves be at the same frame, or the
    delta is a geometry difference wearing a regression's clothes -- which is
    exactly the confusion ledger 210 spent an entry undoing.

Exit status: 0 when nothing became divergent, 4 when something did, and the
refusals above otherwise.  Exit 4 is the point: `newly divergent must be 0' is
a sentence a script can enforce.
"""
import importlib.util
import pathlib
import sys

USAGE = "usage: motion-parity-delta.py BEFORE_GNU BEFORE_NEO AFTER_GNU AFTER_NEO"


def _compare_module():
    """The ledger 210 comparator, imported for its loader and its refusals."""
    path = pathlib.Path(__file__).with_name("motion-parity-compare.py")
    spec = importlib.util.spec_from_file_location("motion_parity_compare", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MPC = _compare_module()


def divergent_set(label, gnu_path, neo_path):
    """(divergent keys, frame, total probes) for one GNU/NEO pair."""
    g, gcfg, gframe = MPC.load(gnu_path)
    n, ncfg, nframe = MPC.load(neo_path)

    short = []
    for path, probes, frame in ((gnu_path, g, gframe), (neo_path, n, nframe)):
        declared = MPC.declared_probes(frame)
        if not probes:
            short.append(f"{path}: 0 probes -- the sweep wrote nothing")
        elif declared is not None and len(probes) != declared:
            short.append(
                f"{path}: {len(probes)} probes, but the sweep says it wrote {declared}"
            )
    if short:
        print(
            f"REFUSING to score the {label} sweep: a sweep that did not write its "
            "probes is a failed sweep, and scoring it would report a perfect "
            "delta from nothing.",
            file=sys.stderr,
        )
        for row in short:
            print(f"  {row}", file=sys.stderr)
        sys.exit(3)

    mismatch = MPC.geometry_rows(gcfg, gframe, ncfg, nframe)
    if mismatch:
        print(
            f"REFUSING to score the {label} sweep: its two editors describe "
            "different windows.",
            file=sys.stderr,
        )
        for name, left, right in mismatch:
            print(f"!! {name:28s} GNU {left}", file=sys.stderr)
            print(f"{'':31s} NEO {right}", file=sys.stderr)
        sys.exit(2)

    keys = set(g) | set(n)
    return {k for k in keys if g.get(k) != n.get(k)}, gframe, len(keys)


def main(argv):
    if len(argv) != 5:
        sys.exit(USAGE)
    before, before_frame, before_total = divergent_set("BEFORE", argv[1], argv[2])
    after, after_frame, after_total = divergent_set("AFTER", argv[3], argv[4])

    if before_frame != after_frame:
        print(
            "REFUSING to compare: the BEFORE and AFTER sweeps ran in different "
            "frames, and a delta taken across two geometries is not a "
            "regression -- it is a geometry difference.",
            file=sys.stderr,
        )
        print(f"!! BEFORE {MPC.frame_label(before_frame)}", file=sys.stderr)
        print(f"!! AFTER  {MPC.frame_label(after_frame)}", file=sys.stderr)
        sys.exit(2)

    fixed = sorted(before - after)
    new = sorted(after - before)
    label = MPC.frame_label(before_frame)
    print(
        f"probes total={before_total} before-divergent={len(before)} "
        f"after-divergent={len(after)}  [{label}]"
    )
    print(f"  fixed            = {len(fixed)}")
    print(f"  NEWLY DIVERGENT  = {len(new)}")
    print(f"  still divergent  = {len(before & after)}")
    for keys, title in ((new, "newly divergent"), (fixed, "fixed")):
        if not keys:
            continue
        print(f"\n{title} (config|pos|motion):")
        for key in keys[:200]:
            print(f"  {key[0]}|{key[1]}|{key[2]}")
        if len(keys) > 200:
            print(f"  ... and {len(keys) - 200} more")
    sys.exit(4 if new else 0)


if __name__ == "__main__":
    main(sys.argv)
