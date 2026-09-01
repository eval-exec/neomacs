#!/usr/bin/env python3
"""Diff two scripts/posn-parity-audit.el outputs (ledger 201).

  scripts/posn-parity-compare.py GNU.txt NEOMACS.txt [-v]

Breaks the divergence down by query, because the three queries answer different
questions about the SAME probe: `posn' is the divergent call, `pvw' is the call
GNU builds it out of, and a probe where `pvw' agrees while `posn' does not is a
composition defect rather than a geometry one.
"""
import sys, collections
gnu, neo = sys.argv[1], sys.argv[2]

def load(p):
    d, cfg = {}, {}
    for line in open(p):
        line = line.rstrip("\n")
        if not line:
            continue
        if line.startswith("CONFIG"):
            parts = line.split()
            cfg[parts[1]] = " ".join(parts[2:])
            continue
        c, pos, q, val = line.split("|", 3)
        d[(c, pos, q)] = val
    return d, cfg

g, gc = load(gnu)
n, nc = load(neo)
keys = sorted(set(g) | set(n))
div = [k for k in keys if g.get(k) != n.get(k)]
print(f"probes total={len(keys)} divergent={len(div)} agreeing={len(keys)-len(div)}")

print("\nCONFIG geometry (GNU vs NEO):")
for name in gc:
    mark = "  " if gc.get(name) == nc.get(name) else "!!"
    print(f"{mark} {name:28s} GNU {gc.get(name)}")
    if mark == "!!":
        print(f"{'':31s} NEO {nc.get(name)}")

by_q = collections.Counter(k[2] for k in div)
print("\nby query:")
for q in sorted(set(k[2] for k in keys)):
    print(f"  {q:14s} {by_q.get(q,0):4d} / {sum(1 for k in keys if k[2]==q)}")

by_cfg = collections.Counter(k[0] for k in div)
print("\nby config:")
for c in sorted(set(k[0] for k in keys)):
    print(f"  {c:28s} {by_cfg.get(c,0):4d} / {sum(1 for k in keys if k[0]==c)}")

nils = [k for k in div if k[2] == "posn" and n.get(k) == "nil" and g.get(k) != "nil"]
print(f"\nposn: NEO nil where GNU answers = {len(nils)}")
print("  positions:", dict(sorted(collections.Counter(int(k[1]) for k in nils).items())))
print("  configs:  ", dict(sorted(collections.Counter(k[0] for k in nils).items())))

other = [k for k in div if k[2] == "posn" and not (n.get(k) == "nil" and g.get(k) != "nil")]
print(f"posn: divergent but NOT a NEO-nil = {len(other)}")
for k in other:
    print(f"    {k[0]}|{k[1]}  GNU {g.get(k)}  NEO {n.get(k)}")

if len(sys.argv) > 3 and sys.argv[3] == "-v":
    print("\nall divergences (config|pos|query  GNU -> NEO):")
    for k in div:
        print(f"  {k[0]}|{k[1]}|{k[2]}  {g.get(k)} -> {n.get(k)}")
