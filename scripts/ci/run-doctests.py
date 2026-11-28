#!/usr/bin/env python3
"""Run and validate Rust doctests for the core NeoVM crates.

`cargo nextest` does not execute doctests, and a plain multi-package
`cargo test --doc -p neovm-value -p neovm-eval -p neovm-core` has three blind
spots:

1. Cargo feature unification: `neovm-core` and `neovm-eval` enable
   `neovm-value/test-support` in `[dev-dependencies]`. Running `-p neovm-value`
   in the same Cargo invocation as either upper crate widens `neovm-value`'s
   `#[cfg_attr(feature = "test-support", visibility::make(pub))]` items,
   breaking production visibility contracts. Each crate is therefore run in its
   own isolated `cargo test --doc` invocation.
2. Stable `rustdoc` ignores `,E0xxx` on `compile_fail` fences: a stale
   `compile_fail,E0616` snippet that fails with `E0433` (unresolved crate) still
   reports `... ok`. Running with `--nocapture --test-threads=1` exposes the
   compiler diagnostics so this runner verifies that every `compile_fail`
   doctest declares an explicit `E0xxx` code AND actually emitted it.
3. Silent skips and zero-test runs: `cargo test --doc` exits 0 when every
   doctest is `ignored` or when no `test result:` line is emitted. This runner
   fails closed on any `ignored` doctest, missing summary, or count below the
   per-crate floor.

Usage:
    python3 -I scripts/ci/run-doctests.py
    python3 -I scripts/ci/run-doctests.py --crate neovm-value
    python3 -I scripts/ci/run-doctests.py --check-log neovm-value path/to/log
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import re
import subprocess
import sys

# (crate_name, minimum passing doctests required)
DEFAULT_CRATES: tuple[tuple[str, int], ...] = (
    ("neovm-value", 17),
    ("neovm-eval", 4),
    ("neovm-core", 0),
)

ANSI_RE = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")
TEST_START_RE = re.compile(
    r"^test (?P<path>.+?) - (?P<item>.+?) \(line (?P<line>\d+)\)"
    r"(?P<compile_fail> - compile fail)? \.\.\. (?P<rest>.*)$"
)
SUMMARY_RE = re.compile(
    r"^test result: (?P<outcome>ok|FAILED)\. "
    r"(?P<passed>\d+) passed; (?P<failed>\d+) failed; (?P<ignored>\d+) ignored; "
    r"(?P<measured>\d+) measured; (?P<filtered>\d+) filtered out"
)
ERROR_CODE_RE = re.compile(r"error\[(E\d{4})\]")
FENCE_RE = re.compile(r"^\s*(?:(?:///|//!|\*)\s*)?(?:`{3,}|~{3,})(?P<attrs>[^\n]*)$")
CODE_TOKEN_RE = re.compile(r"^E\d{4}$")


@dataclass(frozen=True)
class DoctestCase:
    path: str
    item: str
    line: int
    compile_fail: bool
    output: str
    status: str


@dataclass(frozen=True)
class Summary:
    outcome: str
    passed: int
    failed: int
    ignored: int
    measured: int
    filtered: int


def strip_ansi(text: str) -> str:
    return ANSI_RE.sub("", text)


def parse_fence_error_codes(repo_root: Path, rel_path: str, line_number: int) -> tuple[list[str], str | None]:
    """Read the opening fence at `rel_path:line_number` and return (codes, err)."""
    full_path = repo_root / rel_path
    try:
        lines = full_path.read_text(encoding="utf-8").splitlines()
    except OSError as exc:
        return [], f"cannot read source file `{rel_path}`: {exc}"
    if line_number < 1 or line_number > len(lines):
        return [], f"line {line_number} out of range for `{rel_path}` ({len(lines)} lines)"

    # rustdoc reports the 1-indexed line of the opening fence (or occasionally
    # the line immediately before/after in unusual doc attributes); check the
    # reported line first, then +/-1 if not a fence.
    candidate_indices = [line_number - 1]
    if line_number - 2 >= 0:
        candidate_indices.append(line_number - 2)
    if line_number < len(lines):
        candidate_indices.append(line_number)

    for idx in candidate_indices:
        match = FENCE_RE.match(lines[idx])
        if match is None:
            continue
        raw_attrs = match.group("attrs").strip()
        tokens = [tok for tok in re.split(r"[\s,]+", raw_attrs) if tok]
        codes = [tok for tok in tokens if CODE_TOKEN_RE.match(tok)]
        return codes, None

    return [], (
        f"no code fence found at `{rel_path}:{line_number}` "
        f"(line text: {lines[line_number - 1]!r})"
    )


def _extract_status(captured: str) -> str:
    stripped = captured.rstrip()
    for token in ("ok", "FAILED", "ignored"):
        if stripped == token or stripped.endswith("\n" + token) or stripped.endswith(" " + token):
            return token
    return "unknown"


def parse_doctest_log(text: str) -> tuple[list[DoctestCase], list[Summary]]:
    """Parse `--nocapture --test-threads=1` rustdoc output into cases and summaries."""
    clean = strip_ansi(text)
    cases: list[DoctestCase] = []
    summaries: list[Summary] = []

    current_header: re.Match[str] | None = None
    current_lines: list[str] = []

    def flush() -> None:
        nonlocal current_header, current_lines
        if current_header is None:
            return
        body = "\n".join(current_lines)
        cases.append(
            DoctestCase(
                path=current_header.group("path"),
                item=current_header.group("item"),
                line=int(current_header.group("line")),
                compile_fail=current_header.group("compile_fail") is not None,
                output=body,
                status=_extract_status(body),
            )
        )
        current_header = None
        current_lines = []

    for line in clean.splitlines():
        summary_match = SUMMARY_RE.match(line)
        if summary_match is not None:
            flush()
            summaries.append(
                Summary(
                    outcome=summary_match.group("outcome"),
                    passed=int(summary_match.group("passed")),
                    failed=int(summary_match.group("failed")),
                    ignored=int(summary_match.group("ignored")),
                    measured=int(summary_match.group("measured")),
                    filtered=int(summary_match.group("filtered")),
                )
            )
            continue

        if line == "failures:" or line.startswith("   Doc-tests "):
            flush()
            continue

        start_match = TEST_START_RE.match(line)
        if start_match is not None:
            flush()
            current_header = start_match
            current_lines = [start_match.group("rest")]
            continue

        if current_header is not None:
            current_lines.append(line)

    flush()
    return cases, summaries


def validate_crate_log(
    crate: str,
    log_text: str,
    repo_root: Path,
    *,
    min_passed: int,
    exit_code: int = 0,
) -> tuple[bool, list[str]]:
    """Validate one crate's doctest log. Returns (ok, report_lines)."""
    cases, summaries = parse_doctest_log(log_text)
    errors: list[str] = []

    if exit_code != 0:
        errors.append(f"{crate}: cargo test --doc exited with status {exit_code}")

    if not summaries:
        errors.append(
            f"{crate}: no `test result:` summary line found (cargo test --doc did not run)"
        )
        return False, errors

    if len(summaries) != 1:
        errors.append(
            f"{crate}: expected exactly 1 `test result:` summary, saw {len(summaries)}"
        )

    summary = summaries[-1]
    if summary.outcome != "ok" or summary.failed > 0:
        errors.append(
            f"{crate}: doctest failures reported ({summary.passed} passed, {summary.failed} failed)"
        )
    if summary.ignored > 0:
        ignored_locs = [
            f"{c.path}:{c.line}" for c in cases if c.status == "ignored"
        ]
        detail = f" ({', '.join(ignored_locs)})" if ignored_locs else ""
        errors.append(
            f"{crate}: {summary.ignored} ignored doctest(s) are forbidden{detail}; "
            "make each executable, a verified compile_fail,E0xxx contract, or fence as `text`"
        )
    if summary.filtered > 0:
        errors.append(f"{crate}: {summary.filtered} doctest(s) filtered out")
    if summary.passed < min_passed:
        errors.append(
            f"{crate}: only {summary.passed} doctest(s) passed (expected >= {min_passed})"
        )

    for case in cases:
        loc = f"{case.path}:{case.line}"
        if case.status == "FAILED":
            errors.append(f"{crate}: FAILED doctest `{case.item}` at {loc}")
            continue
        if not case.compile_fail:
            continue

        expected_codes, fence_err = parse_fence_error_codes(repo_root, case.path, case.line)
        if fence_err is not None:
            errors.append(f"{crate}: {loc}: {fence_err}")
            continue
        if not expected_codes:
            errors.append(
                f"{crate}: {loc} (`{case.item}`): bare `compile_fail` without an explicit "
                "`E0xxx` error code is forbidden (passes on any unrelated typo/import error)"
            )
            continue

        observed_codes = sorted(set(ERROR_CODE_RE.findall(case.output)))
        missing = [code for code in expected_codes if code not in observed_codes]
        if missing:
            observed_desc = ", ".join(observed_codes) if observed_codes else "none"
            errors.append(
                f"{crate}: {loc} (`{case.item}`): compile_fail expected "
                f"{', '.join(expected_codes)} but compiler emitted [{observed_desc}] "
                "(false-green compile_fail contract)"
            )

    if errors:
        return False, errors

    compile_fail_count = sum(1 for c in cases if c.compile_fail)
    return True, [
        f"{crate}: OK ({summary.passed} passed, 0 failed, 0 ignored; "
        f"{compile_fail_count} compile_fail error-code contract(s) verified)"
    ]


def run_crate_doctests(crate: str, repo_root: Path) -> tuple[int, str]:
    cmd = [
        "cargo",
        "test",
        "--doc",
        "--no-fail-fast",
        "-p",
        crate,
        "--",
        "--nocapture",
        "--test-threads=1",
    ]
    proc = subprocess.run(
        cmd,
        cwd=repo_root,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    return proc.returncode, proc.stdout


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Run and validate Rust doctests per crate with strict compile_fail code checks."
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="Repository root directory (defaults to two levels above this script).",
    )
    parser.add_argument(
        "--crate",
        action="append",
        dest="crates",
        help="Crate name to test (repeatable). Defaults to neovm-value, neovm-eval, neovm-core.",
    )
    parser.add_argument(
        "--min-passed",
        type=int,
        default=None,
        help="Override minimum passing doctest count (only valid with a single --crate or --check-log).",
    )
    parser.add_argument(
        "--check-log",
        nargs=2,
        metavar=("CRATE", "LOG_PATH"),
        help="Validate an existing doctest log file instead of invoking cargo.",
    )
    args = parser.parse_args(argv)

    default_floors = dict(DEFAULT_CRATES)
    repo_root = args.repo_root.resolve()

    if args.check_log is not None:
        crate, log_path = args.check_log
        floor = args.min_passed if args.min_passed is not None else default_floors.get(crate, 0)
        log_text = Path(log_path).read_text(encoding="utf-8")
        ok, lines = validate_crate_log(crate, log_text, repo_root, min_passed=floor)
        for line in lines:
            print(line, file=sys.stdout if ok else sys.stderr)
        return 0 if ok else 1

    if args.crates:
        if args.min_passed is not None and len(args.crates) != 1:
            parser.error("--min-passed can only be used with a single --crate")
        targets = [
            (
                crate,
                args.min_passed
                if args.min_passed is not None
                else default_floors.get(crate, 0),
            )
            for crate in args.crates
        ]
    else:
        targets = list(DEFAULT_CRATES)

    all_ok = True
    for crate, floor in targets:
        print(f"==> running isolated doctests for {crate} (min_passed={floor})", flush=True)
        rc, output = run_crate_doctests(crate, repo_root)
        ok, lines = validate_crate_log(
            crate, output, repo_root, min_passed=floor, exit_code=rc
        )
        if not ok:
            all_ok = False
            # Print tail of raw cargo output on failure so CI logs show the underlying error.
            raw_lines = output.splitlines()
            tail = raw_lines[-60:] if len(raw_lines) > 60 else raw_lines
            print(f"--- raw output tail ({crate}) ---", file=sys.stderr)
            for raw in tail:
                print(raw, file=sys.stderr)
            print(f"--- validation failures ({crate}) ---", file=sys.stderr)
            for line in lines:
                print(f"FAIL: {line}", file=sys.stderr)
        else:
            for line in lines:
                print(line, flush=True)

    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
