#!/usr/bin/env python3
"""Unit contracts for the packaged-browser smoke oracle."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).parent))

from browser_opfs_smoke import startup_warning_rendered  # noqa: E402


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


if __name__ == "__main__":
    unittest.main()
