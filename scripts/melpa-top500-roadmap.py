#!/usr/bin/env python3
"""Regenerate crates/neomacs-melpa-tests/melpa-top500-roadmap.tsv.

Package selection policy: parity corpora are added in descending MELPA
download-count order (https://melpa.org/download_counts.json), not
alphabetically. This script ranks the top N packages, marks which already
have a parity corpus under src/parity_tests/, and rewrites the roadmap TSV.

Usage:
  scripts/melpa-top500-roadmap.py                 # fetch live counts
  scripts/melpa-top500-roadmap.py --counts FILE   # use a cached counts JSON
  scripts/melpa-top500-roadmap.py --top 500       # rank depth (default 500)
"""

import argparse
import json
import os
import re
import sys
import urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATE = os.path.join(REPO, "crates", "neomacs-melpa-tests")
PARITY = os.path.join(CRATE, "src", "parity_tests")
ROADMAP = os.path.join(CRATE, "melpa-top500-roadmap.tsv")
COUNTS_URL = "https://melpa.org/download_counts.json"

# Parity module dirs whose name is not the mechanical munge of the package.
MODULE_ALIASES = {"async": "async1"}


def module_name(package: str) -> str:
    return MODULE_ALIASES.get(package, re.sub(r"[-.+]", "_", package))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--counts", help="path to a cached download_counts.json")
    ap.add_argument("--top", type=int, default=500)
    args = ap.parse_args()

    if args.counts:
        with open(args.counts) as fh:
            counts = json.load(fh)
    else:
        with urllib.request.urlopen(COUNTS_URL, timeout=60) as resp:
            counts = json.load(resp)

    # A package is covered if it has a parity corpus under src/parity_tests/,
    # either as a directory (pkg/mod.rs) or a single-file module (pkg.rs).
    # Counting only directories misses the single-file modules and falsely
    # regresses them to "todo" on regeneration.
    covered_modules = set()
    for entry in os.listdir(PARITY):
        full = os.path.join(PARITY, entry)
        if os.path.isdir(full):
            covered_modules.add(entry)
        elif entry.endswith(".rs") and entry != "mod.rs":
            covered_modules.add(entry[: -len(".rs")])

    ranked = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[: args.top]
    covered = 0
    with open(ROADMAP, "w") as out:
        out.write("rank\tpackage\tdownloads\tstatus\n")
        for rank, (package, downloads) in enumerate(ranked, start=1):
            status = "covered" if module_name(package) in covered_modules else "todo"
            covered += status == "covered"
            out.write(f"{rank}\t{package}\t{downloads}\t{status}\n")

    print(f"{ROADMAP}: {covered} covered / {len(ranked) - covered} todo")
    return 0


if __name__ == "__main__":
    sys.exit(main())
