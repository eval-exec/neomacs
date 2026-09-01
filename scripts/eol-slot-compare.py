#!/usr/bin/env python3
"""Diff two scripts/eol-slot-audit.el outputs (ledger 204).

  scripts/eol-slot-compare.py GNU.txt NEOMACS.txt [-v]

Breaks the divergence down by QUESTION, because the six questions are about the
same defect seen from different sides:

  posn / posn-actual  the divergent call, asked AT the end-of-line position
  pvw                 the call GNU builds posn-at-point out of
  xmap                which buffer position owns each screen column of the row
  vmgoal / vmgoal1    the same question put to the MOTION engine
  wend / vmot         the two neighbours ledger 201 named as the blast radius
                      of any fix, so a fix that moves them is caught here
"""
import sys, collections

gnu, neo = sys.argv[1], sys.argv[2]
verbose = len(sys.argv) > 3 and sys.argv[3] == "-v"


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


g, gc = load(gnu)
n, nc = load(neo)
keys = sorted(set(g) | set(n))
div = [k for k in keys if g.get(k) != n.get(k)]
print(f"probes total={len(keys)} divergent={len(div)} agreeing={len(keys) - len(div)}")

print("\nCASE geometry (GNU vs NEO):")
for name in gc:
    mark = "  " if gc.get(name) == nc.get(name) else "!!"
    print(f"{mark} {name:16s} GNU {gc.get(name)}")
    if mark == "!!":
        print(f"{'':19s} NEO {nc.get(name)}")

by_q = collections.Counter(k[2] for k in div)
print("\nby question:")
for q in sorted(set(k[2] for k in keys)):
    print(f"  {q:14s} {by_q.get(q, 0):4d} / {sum(1 for k in keys if k[2] == q)}")

by_case = collections.Counter(k[0] for k in div)
print("\nby case:")
for c in gc:
    print(f"  {c:16s} {by_case.get(c, 0):4d} / {sum(1 for k in keys if k[0] == c)}")

nils = [k for k in div if k[2] in ("posn", "posn-actual", "pvw") and n.get(k) == "nil"
        and g.get(k) != "nil"]
print(f"\nNEO nil where GNU answers = {len(nils)}")
for k in nils:
    print(f"    {k[0]}|{k[1]}|{k[2]}  GNU {g.get(k)}")

motion = [k for k in div if k[2] in ("vmgoal", "vmgoal1", "vmot", "vmot2", "wend")]
print(f"\nmotion / window-end divergences = {len(motion)}")
for k in motion:
    print(f"    {k[0]}|{k[1]}|{k[2]}  GNU {g.get(k)}  NEO {n.get(k)}")

if verbose:
    print("\nall divergences (case|label|question  GNU -> NEO):")
    for k in div:
        print(f"  {k[0]}|{k[1]}|{k[2]}\n      GNU {g.get(k)}\n      NEO {n.get(k)}")
