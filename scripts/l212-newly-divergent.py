#!/usr/bin/env python3
"""Ledger 212: which probes changed SIDE between two runs of a parity harness.

  scripts/l212-newly-divergent.py GNU.txt BEFORE.txt AFTER.txt

A count is not enough.  Ledger 212's first build took ledger 195's warm sweep
from 444 divergent to 94 and made 45 probes newly divergent in the process; the
headline reported that as a 350-probe win.  This prints the SET difference, so a
change that trades one class of divergence for another cannot be published as a
fix.

Reads scripts/motion-parity-audit.el output (CONFIG|POS|MOTION|VALUE); pass
--below for scripts/below-content-audit.el output (CASE|LABEL|QUESTION|VALUE),
whose probe sets can differ between GNU and this port (ledger 209 residual 4),
in which case only the common probes are scored and the asymmetry is printed.

Refuses an empty file, and refuses mismatched probe sets unless --below --
ledger 210's rule: ask what the check reports when the artifact is EMPTY.
"""
import sys

BELOW = "--below" in sys.argv
ARGS = [a for a in sys.argv[1:] if not a.startswith("-")]
if len(ARGS) != 3:
    sys.exit("usage: l212-newly-divergent.py [--below] GNU.txt BEFORE.txt AFTER.txt")


def load(path):
    probes = {}
    for line in open(path):
        line = line.rstrip("\n")
        if not line or line.startswith(("GEOMETRY ", "CONFIG ", "CASE")):
            continue
        a, b, c, val = line.split("|", 3)
        probes[(a, b, c)] = val
    if not probes:
        sys.exit(f"{path}: 0 probes -- refusing to score an empty file")
    return probes

gnu, before, after = (load(p) for p in ARGS)
keys = set(gnu) & set(before) & set(after)
if len(keys) != len(gnu) or len(keys) != len(before) or len(keys) != len(after):
    if not BELOW:
        sys.exit(f"probe sets differ: gnu={len(gnu)} before={len(before)} "
                 f"after={len(after)} common={len(keys)} -- not comparable")
    if set(before) != set(after):
        sys.exit(f"the two PORT runs ask different probes "
                 f"({len(before)} vs {len(after)}) -- not comparable")
    print(f"scoring {len(keys)} common probes; gnu-only={len(set(gnu) - set(before))} "
          f"port-only={len(set(before) - set(gnu))} (ledger 209 residual 4)")
div_before = {k for k in keys if gnu[k] != before[k]}
div_after = {k for k in keys if gnu[k] != after[k]}
fixed = div_before - div_after
newly = div_after - div_before
print(f"probes={len(keys)} divergent before={len(div_before)} after={len(div_after)} "
      f"fixed={len(fixed)} NEWLY-DIVERGENT={len(newly)}")
for k in sorted(newly)[:25]:
    print(f"  NEW {k[0]}|{k[1]}|{k[2]}  GNU {gnu[k]}  before {before[k]}  after {after[k]}")
