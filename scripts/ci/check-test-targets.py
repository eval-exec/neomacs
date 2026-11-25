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

import argparse
import io
import json
import os
import selectors
import stat
import sys

USAGE = (
    "cargo metadata --no-deps --format-version 1 "
    "| python3 -I scripts/ci/check-test-targets.py"
)
DEFAULT_STDIN_TIMEOUT_SECONDS = 30.0

# Targets that must stay separate. Keep every entry justified:
# - harness = false needs its own binary
# - CI selects the target with --test
# - a process-wide global allocator must not leak into other tests
ALLOWED_EXTRA_TARGETS = {
    ("neovm-core", "main_thread_stack"),
    # cfg(miri)-only era fixture entry avoids enumerating the full core harness.
    ("neovm-core", "numeric_storage_miri"),
    # Compile-fail contract runners: the Linux compile-contracts job selects
    # each one with `--test` and counts the runner that ran
    # (.github/workflows/test-suite.yml, "Run the compile-fail contracts").
    # mint_seal_contracts joined that list with its trybuild fixtures.
    # heap_capability_contracts is the U1 collect/sweep E0133 runner.
    # retained_leaf_contracts is the same kind of runner: trybuild pass and
    # compile-fail fixtures, selected by both compile-contracts jobs.
    ("neovm-core", "thread_safety_contracts"),
    ("neovm-core", "chartable_write_contracts"),
    ("neovm-core", "p0s3_contracts"),
    ("neovm-core", "p1rv_contracts"),
    ("neovm-core", "mint_seal_contracts"),
    ("neovm-core", "heap_capability_contracts"),
    ("neovm-core", "retained_leaf_contracts"),
    # A process-wide global allocator must not leak into other tests.
    ("neovm-core", "gnu_overlay_deletion_allocations"),
    ("neomacs", "daemon_lifecycle"),
    ("neomacs", "windows_launcher"),
    ("neomacs-gui-tests", "native_display"),
}


class InputError(ValueError):
    """Stdin was not fed with valid cargo metadata JSON."""


def _wait_for_first_byte(stream, timeout: float) -> bool:
    """Wait up to `timeout` seconds for a pipe/socket stdin to become readable.

    Returns True when reading can proceed (or when the stream is not a
    pollable pipe/socket, such as a regular file or in-memory StringIO), and
    False when a pipe/socket yielded neither data nor EOF before `timeout`.
    """
    if os.name != "posix":
        return True
    try:
        fd = stream.fileno()
    except (AttributeError, io.UnsupportedOperation, ValueError, OSError):
        return True
    try:
        mode = os.fstat(fd).st_mode
    except OSError:
        return True
    if not (stat.S_ISFIFO(mode) or stat.S_ISSOCK(mode)):
        return True
    with selectors.DefaultSelector() as selector:
        try:
            selector.register(fd, selectors.EVENT_READ)
        except (OSError, ValueError):
            return True
        return bool(selector.select(timeout=timeout))


def read_metadata(
    stream, *, first_byte_timeout: float = DEFAULT_STDIN_TIMEOUT_SECONDS
) -> dict:
    try:
        is_tty = stream.isatty()
    except (AttributeError, ValueError, OSError):
        is_tty = False
    if is_tty:
        raise InputError(
            "expected cargo metadata JSON on stdin, but stdin is a TTY"
        )
    if not _wait_for_first_byte(stream, first_byte_timeout):
        raise InputError(
            f"timed out after {first_byte_timeout:g}s waiting for cargo metadata JSON on stdin"
        )
    raw = stream.read()
    if not raw.strip():
        raise InputError("stdin was empty; expected cargo metadata JSON")
    try:
        metadata = json.loads(raw)
    except json.JSONDecodeError as error:
        raise InputError(f"invalid cargo metadata JSON on stdin: {error}") from error
    if not isinstance(metadata, dict) or not isinstance(metadata.get("packages"), list):
        raise InputError(
            "expected cargo metadata JSON object with a 'packages' list on stdin"
        )
    return metadata


def check_metadata(metadata: dict) -> list[tuple[str, list[str]]]:
    failures = []
    for package in metadata["packages"]:
        name = package["name"]
        targets = sorted(
            target["name"]
            for target in package["targets"]
            if "test" in target["kind"] and target.get("test", False)
        )
        unmerged = [
            target for target in targets if (name, target) not in ALLOWED_EXTRA_TARGETS
        ]
        if len(unmerged) > 1:
            failures.append((name, unmerged))
    return failures


def parse_args(argv=None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Enforce one integration test binary per crate from cargo metadata JSON on stdin.",
        epilog=f"Usage: {USAGE}",
    )
    parser.add_argument(
        "--stdin-timeout",
        type=float,
        default=DEFAULT_STDIN_TIMEOUT_SECONDS,
        help=(
            "seconds to wait for the first byte on an unfed pipe stdin before "
            f"failing fast (default: {DEFAULT_STDIN_TIMEOUT_SECONDS:g})"
        ),
    )
    args = parser.parse_args(argv)
    if args.stdin_timeout < 0:
        parser.error("--stdin-timeout must be non-negative")
    return args


def main(argv=None, stdin=None) -> int:
    args = parse_args(argv)
    stream = sys.stdin if stdin is None else stdin
    try:
        metadata = read_metadata(stream, first_byte_timeout=args.stdin_timeout)
    except InputError as error:
        print(f"error: {error}", file=sys.stderr)
        print(f"usage: {USAGE}", file=sys.stderr)
        return 2

    failures = check_metadata(metadata)
    if failures:
        print(
            "error: crates with more than one integration test binary:",
            file=sys.stderr,
        )
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
