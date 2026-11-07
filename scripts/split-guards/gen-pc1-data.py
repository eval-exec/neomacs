#!/usr/bin/env python3
"""Regenerate pc1_data_promotions.tsv from the landed PROMOTION-LEDGER.

This is the canonical projection the split guard already asserts
(`architecture_pc1_data_approvals_match_the_real_promotion_ledger` in
crates/neovm-core/src/emacs_core/tests/architecture/split_shared_guards.rs):
for every ledger row whose reason contains "pc1-d" (case-insensitive), emit

    item  kind  file  reason  PC1-D

where `file` is the ledger's `file:line` field with the final `:line` suffix
removed (the guard uses the same rsplit_once(':')). Rows keep the ledger's
order. The data file is DERIVED data: never hand-edit it; change the ledger,
then rerun this script.

Usage: python3 scripts/split-guards/gen-pc1-data.py [--check]

--check exits 1 when the checked-in file differs from the projection.
"""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ARCH = ROOT / "crates/neovm-core/src/emacs_core/tests/architecture"
LEDGER = ARCH / "PROMOTION-LEDGER.tsv"
DATA = ARCH / "pc1_data_promotions.tsv"

LEDGER_HEADER = "item\tkind\told_vis\tfile:line\tmodule\treason"
DATA_HEADER = "item\tkind\tfile\tprotected_caller\tapproval"
COMMENT = "# PC1-D approvals copied verbatim from the landed ledger; no new permissions."


def split_rows(text: str, header: str, columns: int):
    lines = [line for line in text.splitlines() if line and not line.startswith("#")]
    if not lines or lines[0] != header:
        raise SystemExit(f"unexpected table header: {lines[0] if lines else '<empty>'!r}")
    rows = []
    for line in lines[1:]:
        fields = line.split("\t")
        if len(fields) != columns:
            raise SystemExit(f"expected {columns} columns, got {len(fields)}: {line!r}")
        rows.append(fields)
    return rows


def projection() -> str:
    ledger = split_rows(LEDGER.read_text(), LEDGER_HEADER, 6)
    out = [COMMENT, DATA_HEADER]
    for item, kind, _old_vis, file_line, _module, reason in ledger:
        if "pc1-d" not in reason.lower():
            continue
        file, _line = file_line.rsplit(":", 1)
        out.append("\t".join([item, kind, file, reason, "PC1-D"]))
    return "\n".join(out) + "\n"


def main() -> int:
    text = projection()
    if "--check" in sys.argv:
        current = DATA.read_text()
        if current != text:
            print(f"{DATA}: differs from the ledger projection", file=sys.stderr)
            return 1
        print(f"{DATA}: matches the ledger projection")
        return 0
    DATA.write_text(text)
    print(f"wrote {DATA}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
