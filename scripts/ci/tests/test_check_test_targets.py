"""Unit and subprocess tests for scripts/ci/check-test-targets.py."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest

try:
    import pty
except ImportError:
    pty = None

WORKSPACE = Path(__file__).resolve().parents[3]
SCRIPT = WORKSPACE / "scripts/ci/check-test-targets.py"
SPEC = importlib.util.spec_from_file_location("check_test_targets", SCRIPT)
checker = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = checker
SPEC.loader.exec_module(checker)


def sample_metadata(packages):
    return json.dumps({"packages": packages})


class CheckTestTargetsTests(unittest.TestCase):
    def test_tty_stdin_fails_fast_with_usage(self):
        class FakeTty(io.StringIO):
            def isatty(self):
                return True

        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            rc = checker.main([], stdin=FakeTty(""))
        self.assertEqual(rc, 2)
        self.assertIn("TTY", stderr.getvalue())
        self.assertIn(checker.USAGE, stderr.getvalue())

    def test_real_pty_subprocess_fails_fast_without_blocking(self):
        if pty is None:
            self.skipTest("pty unavailable on this platform")
        master_fd, slave_fd = pty.openpty()
        try:
            start = time.monotonic()
            proc = subprocess.run(
                [sys.executable, "-I", str(SCRIPT)],
                stdin=slave_fd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=5,
                check=False,
            )
            elapsed = time.monotonic() - start
        finally:
            os.close(master_fd)
            os.close(slave_fd)
        self.assertEqual(proc.returncode, 2)
        self.assertLess(elapsed, 3.0)
        self.assertIn("TTY", proc.stderr)
        self.assertIn(checker.USAGE, proc.stderr)

    def test_unfed_pipe_subprocess_fails_fast_after_bounded_wait(self):
        if os.name != "posix":
            self.skipTest("POSIX pipe selector test")
        proc = subprocess.Popen(
            [sys.executable, "-I", str(SCRIPT), "--stdin-timeout", "0.1"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            start = time.monotonic()
            # Intentionally keep proc.stdin open and write nothing to simulate
            # invoking the script without piping cargo metadata.
            rc = proc.wait(timeout=5)
            elapsed = time.monotonic() - start
            stdout = proc.stdout.read()
            stderr = proc.stderr.read()
        finally:
            if proc.stdin is not None:
                proc.stdin.close()
            if proc.stdout is not None:
                proc.stdout.close()
            if proc.stderr is not None:
                proc.stderr.close()
        self.assertEqual(rc, 2)
        self.assertLess(elapsed, 3.0)
        self.assertEqual(stdout, "")
        self.assertIn("timed out", stderr)
        self.assertIn(checker.USAGE, stderr)

    def test_devnull_empty_stdin_fails_with_usage(self):
        proc = subprocess.run(
            [sys.executable, "-I", str(SCRIPT)],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=5,
            check=False,
        )
        self.assertEqual(proc.returncode, 2)
        self.assertIn("empty", proc.stderr)
        self.assertIn(checker.USAGE, proc.stderr)

    def test_delayed_valid_pipe_succeeds_within_timeout(self):
        payload = sample_metadata(
            [
                {
                    "name": "example-crate",
                    "targets": [
                        {"name": "example-crate", "kind": ["lib"], "test": True},
                        {"name": "suite", "kind": ["test"], "test": True},
                    ],
                }
            ]
        )
        proc = subprocess.Popen(
            [sys.executable, "-I", str(SCRIPT), "--stdin-timeout", "5"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        def feed_later():
            time.sleep(0.05)
            assert proc.stdin is not None
            proc.stdin.write(payload)
            proc.stdin.close()

        writer = threading.Thread(target=feed_later)
        writer.start()
        try:
            rc = proc.wait(timeout=5)
            stdout = proc.stdout.read()
            stderr = proc.stderr.read()
        finally:
            writer.join(timeout=5)
            if proc.stdout is not None:
                proc.stdout.close()
            if proc.stderr is not None:
                proc.stderr.close()
        self.assertEqual(rc, 0, stderr)
        self.assertIn("ok: checked 1 crates", stdout)

    def test_regular_file_stdin_succeeds(self):
        payload = sample_metadata(
            [
                {
                    "name": "neomacs",
                    "targets": [
                        {"name": "main_tests", "kind": ["test"], "test": True},
                        {"name": "daemon_lifecycle", "kind": ["test"], "test": True},
                    ],
                }
            ]
        )
        with tempfile.TemporaryFile(mode="w+") as handle:
            handle.write(payload)
            handle.seek(0)
            proc = subprocess.run(
                [sys.executable, "-I", str(SCRIPT)],
                stdin=handle,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=5,
                check=False,
            )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("ok: checked 1 crates", proc.stdout)

    def test_unmerged_extra_integration_targets_fail_with_exit_1(self):
        payload = sample_metadata(
            [
                {
                    "name": "example-crate",
                    "targets": [
                        {"name": "alpha", "kind": ["test"], "test": True},
                        {"name": "beta", "kind": ["test"], "test": True},
                    ],
                }
            ]
        )
        stdout = io.StringIO()
        stderr = io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            rc = checker.main([], stdin=io.StringIO(payload))
        self.assertEqual(rc, 1)
        self.assertIn("example-crate: alpha, beta", stderr.getvalue())

    def test_invalid_json_fails_with_usage(self):
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            rc = checker.main([], stdin=io.StringIO("{not json"))
        self.assertEqual(rc, 2)
        self.assertIn("invalid cargo metadata JSON", stderr.getvalue())
        self.assertIn(checker.USAGE, stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
