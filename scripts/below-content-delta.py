#!/usr/bin/env python3
"""Score one below-content audit against another: what got FIXED, and what got WORSE.

  scripts/below-content-delta.py BEFORE_GNU BEFORE_NEO AFTER_GNU AFTER_NEO

Ledger 215.  scripts/below-content-compare.py answers "how many probes differ",
which ledger 211 established is not the number a behaviour change has to
publish: a change can fix twelve probes and break three and still show a
falling total.  The number is NEWLY DIVERGENT -- probes the two editors agreed
on BEFORE and disagree on AFTER -- and it must be zero.  This is the
below-content twin of scripts/motion-parity-delta.py, and it inherits that
tool's refusals rather than re-deriving them:

  * a file with no probes is refused with exit 3, because two EMPTY files
    scoring `newly divergent 0' is the same false green in a new place;
  * BEFORE and AFTER must describe the same CASE geometry -- the audit prints
    one CASE line per widening, and a delta taken across two different window
    or buffer shapes is a geometry difference wearing a regression's clothes
    (exit 2);
  * and the GNU side must be the same in both runs, since the reference is
    pinned and a moving reference invalidates the comparison (exit 2).

Exit status: 0 when nothing became divergent, 4 when something did, and the
refusals above otherwise.
"""
import sys

USAGE = "usage: below-content-delta.py BEFORE_GNU BEFORE_NEO AFTER_GNU AFTER_NEO"


def load(path):
    rows, cases = {}, {}
    for line in open(path):
        line = line.rstrip("\n")
        if not line:
            continue
        if line.startswith("CASE"):
            parts = line.split()
            cases[parts[1]] = " ".join(parts[2:])
            continue
        case, label, question, value = line.split("|", 3)
        rows[(case, label, question)] = value
    return rows, cases


def divergent_set(label, gnu_path, neo_path):
    g, gc = load(gnu_path)
    n, nc = load(neo_path)
    short = [p for p, r in ((gnu_path, g), (neo_path, n)) if not r]
    if short:
        print(
            f"REFUSING to score the {label} audit: an audit that wrote no probes "
            "is a failed audit, and scoring it would report a perfect delta from "
            "nothing.",
            file=sys.stderr,
        )
        for path in short:
            print(f"  {path}: 0 probes", file=sys.stderr)
        sys.exit(3)
    keys = set(g) | set(n)
    div = {k for k in keys if g.get(k) != n.get(k)}
    return div, keys, gc, nc, g, n


def main():
    if len(sys.argv) != 5:
        print(USAGE, file=sys.stderr)
        sys.exit(64)
    bg, bn, ag, an = sys.argv[1:5]
    before, before_keys, bgc, bnc, bgrows, _ = divergent_set("BEFORE", bg, bn)
    after, after_keys, agc, anc, agrows, _ = divergent_set("AFTER", ag, an)

    if bgc != agc:
        print(
            "REFUSING to score this delta: the BEFORE and AFTER runs describe "
            "different CASE geometry, so a difference between them is a shape "
            "difference and not a behaviour one.",
            file=sys.stderr,
        )
        for name in sorted(set(bgc) | set(agc)):
            if bgc.get(name) != agc.get(name):
                print(f"  {name}: BEFORE {bgc.get(name)} / AFTER {agc.get(name)}", file=sys.stderr)
        sys.exit(2)
    if bgrows != agrows:
        differing = [k for k in set(bgrows) | set(agrows) if bgrows.get(k) != agrows.get(k)]
        print(
            "REFUSING to score this delta: the GNU reference answered differently "
            f"in the two runs ({len(differing)} probes). The reference is pinned; "
            "a moving one invalidates the comparison.",
            file=sys.stderr,
        )
        for k in differing[:10]:
            print(f"  {k}: BEFORE {bgrows.get(k)} / AFTER {agrows.get(k)}", file=sys.stderr)
        sys.exit(2)

    fixed = sorted(before - after)
    newly = sorted(after - before)
    print(
        f"probes before={len(before_keys)} after={len(after_keys)} "
        f"divergent {len(before)} -> {len(after)}  fixed={len(fixed)}  "
        f"NEWLY DIVERGENT={len(newly)}"
    )
    for k in newly:
        print(f"  NEWLY DIVERGENT {k[0]}|{k[1]}|{k[2]}")
    for k in fixed[:40]:
        print(f"  fixed           {k[0]}|{k[1]}|{k[2]}")
    if len(fixed) > 40:
        print(f"  ... and {len(fixed) - 40} more fixed")
    sys.exit(4 if newly else 0)


if __name__ == "__main__":
    main()
