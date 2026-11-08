#!/usr/bin/env python3
"""Validate a compile-fail contract runner log produced by cargo nextest.

CI runs the five contract targets under `cargo nextest run` (B5: Rust tests
only under nextest).  Nextest reports one status line per test function:

        PASS [   0.011s] ( 1/18) neovm-core::p0s3_contracts <test>

and one summary line:

     Summary [  123.456s] 18 tests run: 18 passed, 0 skipped

The old cargo-test parser summed every `test result: ok. N passed` line; a
nested trybuild harness could print `ok. 1` and mask a FAILED runner.  Here the
per-test status lines are authoritative: any non-PASS status fails, every
expected target must contribute exactly its expected runner count, and the
summary must report at least 18 tests run with zero failed, timed out or
skipped.

Nextest prints a failing test twice: once in the streaming section and again in
the failure summary.  Status lines are therefore de-duplicated per (binary,
test), taking the most severe status, so the replay cannot inflate a per-target
runner count.

Usage: check_compile_contracts.py <contracts.log>
       check_compile_contracts.py --self-test
"""

import re
import sys

EXPECTED = {
    "p0s3_contracts": 1,
    "p1rv_contracts": 3,
    "thread_safety_contracts": 12,
    "chartable_write_contracts": 1,
    "mint_seal_contracts": 1,
}
EXPECTED_TOTAL = sum(EXPECTED.values())  # 18 nextest test functions

ANSI = re.compile(r"\x1b\[[0-9;]*m")
STATUS = re.compile(
    r"^\s+(?P<status>PASS|FAIL|SKIP|IGNORED|TIMEOUT|LEAK)\s+"
    r"\[\s*[^\]]*\]\s+\(\s*(?P<index>\d+)/(?P<total>\d+)\)\s+"
    r"(?P<binary>\S+)\s+(?P<test>\S+)\s*$"
)
SUMMARY = re.compile(
    r"Summary \[[^\]]*\]\s+(?P<run>\d+) tests run:\s+(?P<passed>\d+) passed"
    r"(?:,\s*(?P<failed>\d+) failed)?"
    r"(?:,\s*(?P<timedout>\d+) timed out)?"
    r"(?:,\s*(?P<skipped>\d+) skipped)?"
)
SEVERITY = {"PASS": 0, "IGNORED": 1, "SKIP": 2, "FAIL": 3, "TIMEOUT": 4, "LEAK": 5}


def check(text):
    """Return (ok, lines).  ok is True iff the nextest log proves every runner ran green."""
    text = ANSI.sub("", text)
    out = []
    ok = True
    statuses = {}
    for line in text.splitlines():
        match = STATUS.match(line)
        if match is None:
            continue
        key = (match.group("binary"), match.group("test"))
        status = match.group("status")
        previous = statuses.get(key)
        if previous is None or SEVERITY.get(status, 0) >= SEVERITY.get(previous, 0):
            statuses[key] = status
    counts = {name: 0 for name in EXPECTED}
    bad = []
    for (binary, test), status in sorted(statuses.items()):
        target = binary.rsplit("::", 1)[-1]
        if target in counts:
            counts[target] += 1
        if status != "PASS":
            bad.append(f"{status} {binary} {test}")
    for line in bad:
        out.append("NON-PASS " + line)
    if bad:
        ok = False
    for name, expected in EXPECTED.items():
        out.append(f"{name}: {counts[name]} runners (expected {expected})")
        if counts[name] != expected:
            ok = False
    summaries = list(SUMMARY.finditer(text))
    if not summaries:
        out.append("MISSING nextest Summary line")
        ok = False
    else:
        summary = summaries[-1]
        run = int(summary.group("run"))
        passed = int(summary.group("passed"))
        failed = int(summary.group("failed") or 0)
        timedout = int(summary.group("timedout") or 0)
        skipped = int(summary.group("skipped") or 0)
        out.append(
            f"summary: {run} tests run: {passed} passed, {failed} failed, "
            f"{timedout} timed out, {skipped} skipped"
        )
        if run < EXPECTED_TOTAL:
            out.append(f"MISSING run floor {EXPECTED_TOTAL}: summary reports {run}")
            ok = False
        if passed < EXPECTED_TOTAL:
            ok = False
        if failed or timedout:
            ok = False
        if skipped:
            out.append("SKIPPED runners are not evidence")
            ok = False
    if len(statuses) < EXPECTED_TOTAL:
        out.append(f"distinct status lines seen {len(statuses)} (need >= {EXPECTED_TOTAL})")
        ok = False
    return ok, out


def _nextest_log(counts=None, overrides=None, extra="", summary_run=None, replay=()):
    """Build a synthetic nextest log; overrides keys are (target, index)."""
    counts = dict(EXPECTED if counts is None else counts)
    overrides = overrides or {}
    rows = []
    lines_by_key = {}
    index = 0
    total = sum(counts.values())
    for target, count in counts.items():
        package = {
            "p0s3_contracts": "neovm-core::p0s3_contracts",
            "p1rv_contracts": "neovm-core::p1rv_contracts",
            "thread_safety_contracts": "neovm-core::thread_safety_contracts",
            "chartable_write_contracts": "neovm-core::chartable_write_contracts",
            "mint_seal_contracts": "neovm-core::mint_seal_contracts",
        }[target]
        for offset in range(count):
            index += 1
            status = overrides.get((target, offset), "PASS")
            row = (
                f"        {status} [   0.01s] ( {index:2d}/{total}) {package} {target}_case_{offset}"
            )
            rows.append(row)
            lines_by_key[(target, offset)] = row
    # Nextest replays a failing test's status line in the failure summary.
    rows.extend(lines_by_key[key] for key in replay if key in lines_by_key)
    failed = sum(1 for (target, offset), status in overrides.items() if status == "FAIL"
                 and offset < counts.get(target, 0))
    timedout = sum(1 for (target, offset), status in overrides.items() if status == "TIMEOUT"
                   and offset < counts.get(target, 0))
    run = total if summary_run is None else summary_run
    passed = max(run - failed - timedout, 0)
    summary = f"     Summary [  12.3s] {run} tests run: {passed} passed"
    if failed:
        summary += f", {failed} failed"
    if timedout:
        summary += f", {timedout} timed out"
    summary += ", 0 skipped"
    lines = ([extra] if extra else []) + rows + [summary]
    return "\n".join(lines) + "\n"


SELF_TEST_CASES = {
    "green": (True, _nextest_log()),
    "nested-ok-alone-is-ignored": (
        True,
        _nextest_log(extra="test result: ok. 1 passed; 0 failed"),
    ),
    "failed-runner": (False, _nextest_log(overrides={("thread_safety_contracts", 0): "FAIL"})),
    "nested-ok-cannot-mask-a-failed-runner": (
        False,
        _nextest_log(
            overrides={("thread_safety_contracts", 0): "FAIL"},
            extra="test result: ok. 1 passed; 0 failed",
        ),
    ),
    "failure-summary-replay-does-not-inflate-counts": (
        False,
        _nextest_log(
            overrides={("thread_safety_contracts", 0): "FAIL"},
            replay=[("thread_safety_contracts", 0)],
        ),
        "thread_safety_contracts: 12 runners (expected 12)",
    ),
    "timeout-runner": (False, _nextest_log(overrides={("p1rv_contracts", 0): "TIMEOUT"})),
    "missing-runner": (
        False,
        _nextest_log(counts={**EXPECTED, "chartable_write_contracts": 0}),
    ),
    "wrong-per-target-count": (
        False,
        _nextest_log(counts={**EXPECTED, "thread_safety_contracts": 11}),
    ),
    "summary-below-floor": (False, _nextest_log(summary_run=16)),
    "ansi-colored-green": (
        True,
        "\x1b[32;1m" + _nextest_log().replace("PASS", "\x1b[32;1mPASS\x1b[0m") + "\x1b[0m",
    ),
}


def self_test():
    failures = 0
    for name, case in SELF_TEST_CASES.items():
        expected_ok, text = case[0], case[1]
        required = case[2] if len(case) > 2 else None
        ok, lines = check(text)
        good = ok == expected_ok and (
            required is None or any(required in line for line in lines)
        )
        if good:
            print(f"PASS self-test: {name} -> ok={ok}")
        else:
            failures += 1
            print(f"FAIL self-test: {name} -> ok={ok}, expected {expected_ok}")
            for line in lines:
                print("  " + line)
    return 1 if failures else 0


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test()
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    with open(argv[1], encoding="utf-8", errors="replace") as handle:
        text = handle.read()
    ok, lines = check(text)
    for line in lines:
        print(line)
    print("compile-fail contract runners: " + ("OK" if ok else "FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
