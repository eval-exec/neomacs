#!/usr/bin/env python3
"""Diff two scripts/below-content-audit.el outputs (ledger 205).

  scripts/below-content-compare.py GNU.txt NEOMACS.txt [-v]

Breaks the divergence down three ways, because "below the last row with
content" is a claim about WHICH rows, not just how many probes differ:

  by question   xy is the divergent call; wend / vmot / pmax-posn are the
                neighbours ledgers 201 and 204 named as the blast radius of
                any fix here, so a fix that moves them is caught in the same
                run that measures the defect.
  NEO nil       the headline: probes where GNU answers and this port does not.
  point-max     of those, how many of GNU's answers are the buffer's own
                point-max -- ledger 204's residual 1 claims all of them are.
"""
import sys, collections, re

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


def pmax_of(caseline):
    m = re.search(r"pmax=(\d+)", caseline or "")
    return int(m.group(1)) if m else None


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

nils = [k for k in div if n.get(k) == "nil" and g.get(k) != "nil"]
print(f"\nNEO nil where GNU answers = {len(nils)}")
at_pmax = 0
for k in nils:
    pm = pmax_of(gc.get(k[0]))
    m = re.match(r"^\((\d+) ", g.get(k) or "")
    if m and pm is not None and int(m.group(1)) == pm:
        at_pmax += 1
print(f"    of which GNU's answer is the buffer's own point-max = {at_pmax}")
seen = set()
for k in nils:
    if k[0] not in seen:
        seen.add(k[0])
        print(f"    {k[0]:16s} pmax={pmax_of(gc.get(k[0]))}")
if verbose:
    for k in nils:
        print(f"      {k[0]}|{k[1]}|{k[2]}  GNU {g.get(k)}")

gnils = [k for k in div if g.get(k) == "nil" and n.get(k) != "nil"]
print(f"\nGNU nil where NEO answers = {len(gnils)}")
for k in gnils:
    print(f"    {k[0]}|{k[1]}|{k[2]}  NEO {n.get(k)}")

other = [k for k in div if k not in set(nils) and k not in set(gnils)]
print(f"\nboth answer, differently = {len(other)}")
for k in other[: (10_000 if verbose else 40)]:
    print(f"    {k[0]}|{k[1]}|{k[2]}\n        GNU {g.get(k)}\n        NEO {n.get(k)}")

neigh = [k for k in div if k[2] in ("wend", "vmot", "pmax-posn", "geom")]
print(f"\nneighbour (wend / vmot / pmax-posn / geom) divergences = {len(neigh)}")
for k in neigh:
    print(f"    {k[0]}|{k[1]}|{k[2]}  GNU {g.get(k)}  NEO {n.get(k)}")
