#!/usr/bin/env python3
"""Unit contracts for the packaged-browser smoke oracle."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).parent))

from browser_opfs_smoke import startup_warning_rendered  # noqa: E402
from browser_release_upgrade import capture_failure as capture_upgrade_failure  # noqa: E402


class BrokenDiagnosticsDriver:
    def save_screenshot(self, path: str) -> None:
        raise RuntimeError("screenshot unavailable")

    @property
    def page_source(self) -> str:
        raise RuntimeError("page source unavailable")


class StartupWarningDetectionTest(unittest.TestCase):
    def test_detects_each_distinctive_fragment_from_the_gnu_emacs_warning(self) -> None:
        fragments = (
            "Unable to create `user-emacs-directory'",
            "Any data that would normally be written there may be lost!",
            "customize the variable `user-emacs-directory-warning'",
        )

        for fragment in fragments:
            with self.subTest(fragment=fragment):
                self.assertTrue(startup_warning_rendered([fragment]))

    def test_detects_a_warning_in_an_earlier_startup_frame(self) -> None:
        self.assertTrue(
            startup_warning_rendered(
                [
                    "Any data that would normally be written there may be lost!",
                    *[f"redisplay {index}" for index in range(128)],
                    "Welcome to Neomacs *scratch*",
                ]
            )
        )

    def test_accepts_warning_free_startup_frames(self) -> None:
        self.assertFalse(
            startup_warning_rendered(
                ["Loading Neomacs", "Welcome to Neomacs *scratch*"]
            )
        )


class UpgradeDiagnosticTest(unittest.TestCase):
    def test_artifact_failure_does_not_replace_the_browser_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_upgrade_failure(BrokenDiagnosticsDriver(), Path(directory))

            errors = Path(directory, "browser-release-upgrade-artifact-errors.txt")
            self.assertIn("screenshot unavailable", errors.read_text())
            self.assertIn("page source unavailable", errors.read_text())


if __name__ == "__main__":
    unittest.main()
