#!/usr/bin/env python3
"""Enforce one integration test binary per crate.

Every integration test target is a separate executable, and each executable
statically links the crate's dependency closure. Nextest runs every test in
its own process, so extra binaries buy no runtime isolation. Merging test
files into modules of one binary (see
crates/neomacs-tui-tests/tests/tui.rs) keeps every test while paying the
closure link once.

Reads cargo metadata JSON on stdin.

Usage:
    cargo metadata --no-deps --format-version 1 \
        | python3 -I scripts/ci/check-test-targets.py
"""

import json
import sys

# Targets that must stay separate. Keep every entry justified:
# - harness = false needs its own binary
# - CI selects the target with --test
# - a process-wide global allocator must not leak into other tests
ALLOWED_EXTRA_TARGETS = {
    ("neovm-core", "main_thread_stack"),
    ("neovm-core", "thread_safety_contracts"),
    ("neovm-core", "chartable_write_contracts"),
    ("neovm-core", "p0s3_contracts"),
    ("neovm-core", "p1rv_contracts"),
    ("neovm-core", "gnu_overlay_deletion_allocations"),
    ("neomacs", "daemon_lifecycle"),
    ("neomacs", "windows_launcher"),
    ("neomacs-gui-tests", "native_display"),
}


def main() -> int:
    metadata = json.load(sys.stdin)
    failures = []
    for package in metadata["packages"]:
        name = package["name"]
        targets = sorted(
            target["name"]
            for target in package["targets"]
            if "test" in target["kind"] and target.get("test", False)
        )
        unmerged = [target for target in targets if (name, target) not in ALLOWED_EXTRA_TARGETS]
        if len(unmerged) > 1:
            failures.append((name, unmerged))

    if failures:
        print("error: crates with more than one integration test binary:", file=sys.stderr)
        for name, targets in failures:
            print(f"  {name}: {', '.join(targets)}", file=sys.stderr)
        print(
            "Merge the test files into one #[path]-module binary (see "
            "crates/neomacs-tui-tests/tests/tui.rs), or add a justified "
            "allowlist entry in scripts/ci/check-test-targets.py.",
            file=sys.stderr,
        )
        return 1

    print(f"ok: checked {len(metadata['packages'])} crates")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
