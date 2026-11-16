#!/usr/bin/env python3
"""Classify plain-pub ledger rows by their cross-crate consumers.

Companion triage tool to the B3 plain-pub surface guard
(`crates/neovm-core/src/emacs_core/tests/architecture/plain_pub_surface_guard_test.rs`).
The guard freezes each extracted crate's plain-`pub` surface at a reviewed
baseline; this script answers the question the guard cannot: which of those
rows does anything outside the owning crate actually use, so a narrowing pass
knows what is safe to flip to `pub(crate)`.

Method (deliberately lexical, compiler-checked after): a real use of an item
must spell its identifier somewhere in the consuming crate's source -- in a
`use` path, a call, a field access, or a re-export. So the script extracts one
identifier per row and asks, for every other workspace crate (plus every
gitignored harness crate below tmp/ that has its own Cargo.toml, which
`cargo check --workspace` never sees), whether that identifier occurs there at
all, and whether every occurrence is inside a test path (`tests/` component,
`tests.rs`, `*_test.rs`).

Verdicts:

* ZERO -- no other crate spells the identifier. Flip `pub` -> `pub(crate)`.
* TEST-ONLY -- other crates spell it, but only inside test paths (typically a
  re-homed neovm-core test). Flip to `pub(crate)` AND add
  `#[cfg_attr(feature = "test-support", visibility::make(pub))]` (peer ruling
  02:2xZ TEST-SUPPORT-VIS, B3).
* N -- at least one production consumer; leave for re-routing review.

Two blind spots by construction, both caught by the per-commit compile gate:
`$crate::` references inside the owning crate's own `macro_rules!` bodies
expand in the invoking crate, and cfg(windows)-only consumers are invisible on
Linux (the windows-gnu cross-check covers them). Word-boundary matching also
over-counts coincidental spellings and doc-comment mentions -- false consumers,
which only make the script conservative.

Usage:
    python3 -I scripts/plain-pub-consumers.py --crate neovm-eval [--suffix EVAL]
        [--out REPORT.tsv] [--verbose]
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
ARCHITECTURE_DIR = Path(
    "crates/neovm-core/src/emacs_core/tests/architecture"
)
CRATES_DIR = "crates"

# Word-match the exact identifier: -F fixed strings, -w whole word.
GREP_ALL = ["grep", "-rhoIwF", "--include=*.rs"]
GREP_PROD = GREP_ALL + ["--exclude=*_test.rs", "--exclude=tests.rs", "--exclude-dir=tests"]


def identifier_of(kind: str, item: str) -> str | None:
    """The name a consumer would have to spell to use this row's item."""
    if kind == "use":
        # `use a::b` -> b; `use a::b as c` -> c; `use a::*` is a glob row with
        # no leaf identity (narrowing the statement itself is still possible,
        # but consumers never spell the glob, so report no identifier).
        body = item[len("use ") :]
        if body.endswith("::*"):
            return None
        body = body.split(" as ")[-1].strip()
        name = body.split("::")[-1]
    else:
        # Every other row's item ends with the declared name: `... fn foo`,
        # `... fn impl Type::method`, `Type field name`, `... mod name`.
        name = item.split()[-1].split("::")[-1]
    name = name.removeprefix("r#")
    return name if IDENT.match(name) else None


def scan_roots(workspace: Path, owner: str) -> list[Path]:
    """Every crate src root except the owner's, plus tmp/ harness crates."""
    roots = []
    for cargo in sorted(workspace.joinpath(CRATES_DIR).glob("*/Cargo.toml")):
        crate_dir = cargo.parent
        if crate_dir.name != owner and (crate_dir / "src").is_dir():
            roots.append(crate_dir / "src")
    # Gitignored harness crates have their own manifests and are compiled by
    # the nextest lanes, not by `cargo check --workspace`.
    for cargo in sorted(workspace.joinpath("tmp").glob("*/Cargo.toml")):
        crate_dir = cargo.parent
        if crate_dir.name.startswith("wt-"):
            continue
        src = crate_dir / "src"
        if src.is_dir():
            roots.append(src)
    return roots


def identifiers_in(root: Path, patterns: Path, prod_only: bool) -> set[str]:
    command = (GREP_PROD if prod_only else GREP_ALL) + ["-f", str(patterns), str(root)]
    result = subprocess.run(
        command, capture_output=True, text=True, check=False
    )
    # grep exits 1 when nothing matched; that is an empty set, not an error.
    if result.returncode not in (0, 1):
        raise SystemExit(
            f"scan failed under {root}: rc={result.returncode}\n{result.stderr}"
        )
    return {line for line in result.stdout.splitlines() if line}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--crate", required=True, help="owning crate dir name, e.g. neovm-eval")
    parser.add_argument(
        "--suffix",
        help="baseline suffix (default: crate name minus the neovm- prefix, upper-cased)",
    )
    parser.add_argument("--out", help="report path (default: stdout)")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    workspace = Path(__file__).resolve().parent.parent
    suffix = args.suffix or re.sub(r"^neovm[-_]", "", args.crate).upper().replace("-", "_")
    baseline = workspace / ARCHITECTURE_DIR / f"PLAIN-PUB-{suffix}.tsv"
    rows = []
    for line in baseline.read_text().splitlines()[1:]:
        if line.strip():
            file, kind, item = line.split("\t")
            rows.append((file, kind, item))

    by_identifier: dict[str, list[int]] = {}
    globs = 0
    for index, (file, kind, item) in enumerate(rows):
        name = identifier_of(kind, item)
        if name is None:
            globs += 1
            continue
        by_identifier.setdefault(name, []).append(index)

    roots = scan_roots(workspace, args.crate)
    with tempfile.NamedTemporaryFile("w", suffix=".patterns", delete=False) as patterns:
        patterns.write("\n".join(sorted(by_identifier)) + "\n")
        patterns_path = Path(patterns.name)
    try:
        all_sets = {root: identifiers_in(root, patterns_path, prod_only=False) for root in roots}
        prod_sets = {root: identifiers_in(root, patterns_path, prod_only=True) for root in roots}
    finally:
        patterns_path.unlink(missing_ok=True)

    report_lines = [
        "file\tkind\titem\tidentifier\tprod_consumers\ttest_only_consumers\tverdict"
    ]
    verdict_counts: dict[str, int] = {}
    for index, (file, kind, item) in enumerate(rows):
        name = identifier_of(kind, item)
        if name is None:
            verdict = "GLOB"
        else:
            prod = [
                root.parent.name for root in roots if name in prod_sets[root]
            ]
            test_only = [
                root.parent.name
                for root in roots
                if name in all_sets[root] and name not in prod_sets[root]
            ]
            if not prod and not test_only:
                verdict = "ZERO"
            elif not prod:
                verdict = "TEST-ONLY"
            else:
                verdict = "N"
        verdict_counts[verdict] = verdict_counts.get(verdict, 0) + 1
        prod_names = ",".join(
            root.parent.name for root in roots if name and name in prod_sets[root]
        )
        test_names = ",".join(
            root.parent.name
            for root in roots
            if name and name in all_sets[root] and name not in prod_sets[root]
        )
        report_lines.append(
            f"{file}\t{kind}\t{item}\t{name or '-'}\t{prod_names}\t{test_names}\t{verdict}"
        )

    output = "\n".join(report_lines) + "\n"
    if args.out:
        Path(args.out).write_text(output)
    else:
        sys.stdout.write(output)

    summary = " ".join(
        f"{verdict}={count}" for verdict, count in sorted(verdict_counts.items())
    )
    print(
        f"{baseline.name}: {len(rows)} rows ({globs} glob statements) -> {summary}",
        file=sys.stderr,
    )
    if args.verbose:
        for root in roots:
            print(f"scanned {root}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
