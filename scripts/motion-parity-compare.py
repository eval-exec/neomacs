#!/usr/bin/env python3
"""Diff two scripts/motion-parity-audit.el outputs (ledger 195).

  scripts/motion-parity-compare.py GNU.txt NEOMACS.txt [-v] [--allow-geometry-mismatch]

Reports the divergence count broken down by config and by motion.  The two
`posn-*` motions are CONTROLS: they ask the layout engine rather than
`vertical-motion', so a divergence there means the rows themselves differ and
not the motion over them (ledger 184's rule).

THE FRAME IS PART OF THE ANSWER (ledger 210).  Every probe is a question about a
window of a particular width, so the SAME tree answers COLD 130 / WARM 352 at
160 columns and COLD 160 / WARM 444 at 80 -- the 80-column run is a strict
superset, because only there does this text's longest line reach the window edge
where the truncation marker lives.  Ledger 205 published the first pair, ledger
209 the second, and the difference was read as a 30-cold / 92-warm motion
regression that never existed.  So the count is printed WITH the frame it was
measured in: a number pasted into a ledger now carries its own geometry.

The rule is one sentence: a divergence count taken across two geometries is not
a parity number, whatever made them differ.  So the two files must agree about
the FRAME they ran in (`GEOMETRY', from L195_COLS/L195_ROWS) and about every
window they describe (`CONFIG'), or the comparison is REFUSED with exit status
2.  The refusal prints the disagreeing rows marked `!!', the way ledger 201's
and 204's comparators mark theirs, so a real window-geometry divergence is still
visible rather than swallowed by the error; --allow-geometry-mismatch then
scores it anyway, with GEOMETRY MISMATCH standing in the headline.

A file written before ledger 210 has no `GEOMETRY' line.  Two such files still
compare -- their frames are equally unknown, and the headline says
`frame unrecorded' rather than inventing one -- but their `CONFIG' widths still
have to agree, which is what catches a 160-column file diffed against an
80-column one when neither records its frame.
"""
import sys, collections

USAGE = ("usage: motion-parity-compare.py GNU.txt NEOMACS.txt "
         "[-v] [--allow-geometry-mismatch]")
FLAGS = {"-v", "--allow-geometry-mismatch"}


def parse_argv(argv):
    files, flags = [], set()
    for arg in argv[1:]:
        if arg in FLAGS:
            flags.add(arg)
        elif arg.startswith("-"):
            sys.exit(f"{USAGE}\nunknown flag: {arg}")
        else:
            files.append(arg)
    if len(files) != 2:
        sys.exit(USAGE)
    return files[0], files[1], flags


def load(path):
    """Return (probes, configs, frame) for one audit output."""
    probes, configs, frame = {}, {}, None
    with open(path) as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line:
                continue
            if line.startswith("GEOMETRY "):
                frame = line[len("GEOMETRY "):]
                continue
            if line.startswith("CONFIG "):
                fields = line.split()
                configs[fields[1]] = " ".join(fields[2:])
                continue
            cfg, pos, motion, val = line.split("|", 3)
            probes[(cfg, pos, motion)] = val
    return probes, configs, frame


def frame_label(frame):
    """`GEOMETRY frame-width=80 frame-height=23' -> `frame 80x23'."""
    if frame is None:
        return "frame unrecorded"
    width = height = "?"
    for field in frame.split():
        key, _, value = field.partition("=")
        if key == "frame-width":
            width = value
        elif key == "frame-height":
            height = value
    return f"frame {width}x{height}"


def geometry_rows(gcfg, gframe, ncfg, nframe):
    """Every way the two files describe different windows."""
    rows = []
    if gframe != nframe:
        rows.append(("FRAME", frame_label(gframe), frame_label(nframe)))
    for name in sorted(set(gcfg) | set(ncfg)):
        if gcfg.get(name) != ncfg.get(name):
            rows.append((name, gcfg.get(name), ncfg.get(name)))
    return rows

def declared_probes(frame):
    """The probe count the sweep itself says it wrote, if it says."""
    if frame is None:
        return None
    for field in frame.split():
        key, _, value = field.partition("=")
        if key == "probes":
            return int(value) if value.isdigit() else None
    return None

# Ledger 211: the driver lives in `main' so that the loader and the refusals
# above can be IMPORTED.  scripts/motion-parity-delta.py asks a strictly harder
# question of the same files -- what became divergent -- and reimplementing
# either of them there would give the new tool its own way to false-green.
def main():
    gnu_path, neo_path, flags = parse_argv(sys.argv)
    g, gcfg, gframe = load(gnu_path)
    n, ncfg, nframe = load(neo_path)

    # A sweep that wrote no probes, or fewer than it says it wrote, is a FAILED
    # sweep -- and the answer it used to give was the most dangerous one available:
    # `divergent=0', exit 0, a perfect parity score taken from nothing (ledger 210).
    short = []
    for path, probes, frame in ((gnu_path, g, gframe), (neo_path, n, nframe)):
        declared = declared_probes(frame)
        if not probes:
            short.append(f"{path}: 0 probes -- the sweep wrote nothing")
        elif declared is not None and len(probes) != declared:
            short.append(
                f"{path}: {len(probes)} probes, but the sweep says it wrote {declared}"
            )
    if short:
        print(
            "REFUSING to compare: a sweep that did not write its probes is a failed "
            "sweep, and scoring it would report perfect parity from nothing.",
            file=sys.stderr,
        )
        for row in short:
            print(f"  {row}", file=sys.stderr)
        sys.exit(3)

    mismatch = geometry_rows(gcfg, gframe, ncfg, nframe)
    if mismatch and "--allow-geometry-mismatch" not in flags:
        print(
            "REFUSING to compare: these two sweeps describe different windows, and "
            "a divergence count taken across two geometries is not a parity number.",
            file=sys.stderr,
        )
        for name, left, right in mismatch:
            print(f"!! {name:28s} GNU {left}", file=sys.stderr)
            print(f"{'':31s} NEO {right}", file=sys.stderr)
        print(
            "Re-run both editors at the same L195_COLS/L195_ROWS, or pass "
            "--allow-geometry-mismatch if the geometry difference is itself what "
            "you are studying.",
            file=sys.stderr,
        )
        sys.exit(2)

    keys = sorted(set(g) | set(n))
    div = [k for k in keys if g.get(k) != n.get(k)]
    if mismatch:
        print(f"GEOMETRY MISMATCH ({len(mismatch)} rows) -- this count is not a parity number")
        label = f"{frame_label(gframe)} vs {frame_label(nframe)}"
    else:
        label = frame_label(gframe)
    print(
        f"probes total={len(keys)} divergent={len(div)} agreeing={len(keys)-len(div)}"
        f"  [{label}]"
    )

    print("\nCONFIG geometry (GNU vs NEO):")
    for name in sorted(set(gcfg) | set(ncfg)):
        mark = "  " if gcfg.get(name) == ncfg.get(name) else "!!"
        print(f"{mark} {name:28s} GNU {gcfg.get(name)}")
        if mark == "!!":
            print(f"{'':31s} NEO {ncfg.get(name)}")

    by_cfg = collections.Counter(k[0] for k in div)
    by_mot = collections.Counter(k[2] for k in div)
    print("\nby config:")
    for cfg, _ in sorted(collections.Counter(k[0] for k in keys).items()):
        print(f"  {cfg:28s} {by_cfg.get(cfg,0):4d} / {sum(1 for k in keys if k[0]==cfg)}")
    print("\nby motion:")
    for mot in sorted(set(k[2] for k in keys)):
        print(f"  {mot:14s} {by_mot.get(mot,0):4d} / {sum(1 for k in keys if k[2]==mot)}")
    if "-v" in flags:
        print("\nfirst 60 divergences (config|pos|motion  GNU -> NEO):")
        for k in div[:60]:
            print(f"  {k[0]}|{k[1]}|{k[2]}  {g.get(k)!r} -> {n.get(k)!r}")


if __name__ == "__main__":
    main()
