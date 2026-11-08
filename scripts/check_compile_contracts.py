#!/usr/bin/env python3
"""Validate a compile-fail contract runner log.

The CI step used to sum every `test result: ok. N passed` line in the log.
trybuild compiles and runs each fixture through a nested cargo invocation, so
those nested harnesses print `test result: ok. ...` lines too; a FAILED line
from a top-level runner could be masked by nested passes ("a nested `ok. 1`
masked thread_safety's FAILED line", 2026-11-07).

This parser attributes result lines to the cargo-test target section that
contains them and requires each expected target's OWN final result to be
`ok` with exactly its expected passed count.  The nested-pass total is still
checked (it proves the nested nmjit runner ran), but it can only add to the
count, never hide a failure.

Usage: check_compile_contracts.py <contracts.log>
       check_compile_contracts.py --self-test
"""

import re
import sys

EXPECTED = {
    "p0s3_contracts": 1,
    "p1rv_contracts": 3,
    "thread_safety_contracts": 11,
    "chartable_write_contracts": 1,
}
# The nested nmjit fixture runner is additional to the four target results.
EXPECTED_TOTAL_OK = sum(EXPECTED.values()) + 1

RUNNING = re.compile(r"^\s*Running (?:unittests )?tests/(?P<name>[A-Za-z0-9_]+)\.rs\b")
RESULT = re.compile(
    r"^test result: (?P<status>ok|FAILED)\. (?P<passed>\d+) passed; (?P<failed>\d+) failed"
)
OK_LINE = re.compile(r"test result: ok\. (?P<passed>\d+) passed")


def per_target_results(text):
    """Map each expected target to its OWN result inside its section.

    A section contains the target's libtest summary plus any nested trybuild
    harness results.  Layout is not trustworthy (captured failure output can
    be printed after the summary), so a FAILED line anywhere in the section is
    authoritative, and an `ok` line must carry the target's expected passed
    count (nested harnesses print `ok. 1`).
    """
    results = {name: [] for name in EXPECTED}
    current = None
    for line in text.splitlines():
        running = RUNNING.search(line)
        if running:
            name = running.group("name")
            current = name if name in EXPECTED else None
            continue
        result = RESULT.match(line)
        if result and current is not None:
            results[current].append(
                (
                    result.group("status"),
                    int(result.group("passed")),
                    int(result.group("failed")),
                )
            )
    resolved = {}
    for name, lines in results.items():
        failed = next((line for line in lines if line[0] == "FAILED"), None)
        if failed is not None:
            resolved[name] = failed
            continue
        ok = next(
            (line for line in reversed(lines) if line[0] == "ok" and line[1] == EXPECTED[name]),
            None,
        )
        resolved[name] = ok
    return resolved


def total_ok_passes(text):
    return sum(int(m.group("passed")) for m in OK_LINE.finditer(text))


def check(text):
    """Return (ok, lines).  ok is True iff every expected target is green."""
    results = per_target_results(text)
    out = []
    ok = True
    for name, expected in EXPECTED.items():
        result = results[name]
        if result is None:
            out.append(f"MISSING {name}: no result line in its own section")
            ok = False
            continue
        status, passed, failed = result
        out.append(f"{name}: {status} {passed} passed {failed} failed (expected {expected} passed)")
        if status != "ok" or failed != 0 or passed != expected:
            ok = False
    total = total_ok_passes(text)
    out.append(f"nested+target ok passes: {total} (need >= {EXPECTED_TOTAL_OK})")
    if total < EXPECTED_TOTAL_OK:
        ok = False
    return ok, out


SELF_TEST_CASES = {
    "green": (
        True,
        """\
     Running tests/p0s3_contracts.rs (target/debug/deps/p0s3_contracts-1)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/p1rv_contracts.rs (target/debug/deps/p1rv_contracts-2)
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 3 passed; 0 failed
     Running tests/thread_safety_contracts.rs (target/debug/deps/thread_safety_contracts-3)
    nested nmjit runner
test result: ok. 1 passed; 0 failed
test result: ok. 11 passed; 0 failed
     Running tests/chartable_write_contracts.rs (target/debug/deps/chartable_write_contracts-4)
test result: ok. 1 passed; 0 failed
""",
    ),
    # The old parser summed every ok line: 1 + 3 + 12 nested + 1 = 17, so it
    # passed even though thread_safety's own result is FAILED.  The fixed
    # parser must reject this log.
    "nested-passes-mask-a-failed-runner": (
        False,
        """\
     Running tests/p0s3_contracts.rs (target/debug/deps/p0s3_contracts-1)
test result: ok. 1 passed; 0 failed
     Running tests/p1rv_contracts.rs (target/debug/deps/p1rv_contracts-2)
test result: ok. 3 passed; 0 failed
     Running tests/thread_safety_contracts.rs (target/debug/deps/thread_safety_contracts-3)
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: FAILED. 5 passed; 6 failed
     Running tests/chartable_write_contracts.rs (target/debug/deps/chartable_write_contracts-4)
test result: ok. 1 passed; 0 failed
""",
    ),
    "missing-runner": (
        False,
        """\
     Running tests/p0s3_contracts.rs (target/debug/deps/p0s3_contracts-1)
test result: ok. 1 passed; 0 failed
     Running tests/p1rv_contracts.rs (target/debug/deps/p1rv_contracts-2)
test result: ok. 3 passed; 0 failed
     Running tests/chartable_write_contracts.rs (target/debug/deps/chartable_write_contracts-4)
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
""",
    ),
    "wrong-pass-count": (
        False,
        """\
     Running tests/p0s3_contracts.rs (target/debug/deps/p0s3_contracts-1)
test result: ok. 0 passed; 0 failed
     Running tests/p1rv_contracts.rs (target/debug/deps/p1rv_contracts-2)
test result: ok. 3 passed; 0 failed
     Running tests/thread_safety_contracts.rs (target/debug/deps/thread_safety_contracts-3)
test result: ok. 11 passed; 0 failed
     Running tests/chartable_write_contracts.rs (target/debug/deps/chartable_write_contracts-4)
test result: ok. 1 passed; 0 failed
""",
    ),
}


def self_test():
    failures = 0
    for name, (expected_ok, text) in SELF_TEST_CASES.items():
        ok, lines = check(text)
        if ok == expected_ok:
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
