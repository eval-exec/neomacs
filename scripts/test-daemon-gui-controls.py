#!/usr/bin/env python3
"""Cheap fail-closed controls for the opt-in GUI acceptance entry point."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("test-daemon-gui.py")


class GuiPreflightTests(unittest.TestCase):
    def rejected(self, arguments, environment, diagnostic):
        result = subprocess.run([sys.executable, str(SCRIPT), *arguments],
                                env=environment, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(diagnostic, result.stderr)

    def test_unselected_inputs_are_not_silently_skipped(self):
        env = {key: value for key, value in os.environ.items()
               if key not in ["NEOMACS_EWM_MODULE", "NEOMACS_GNU_EMACSCLIENT"]}
        self.rejected(["--bin-dir", "/missing"], env, "requires NEOMACS_EWM_MODULE")

    def test_selected_missing_module_fails_before_native_launch(self):
        env = dict(os.environ, NEOMACS_EWM_MODULE="/missing-ewm-module",
                   NEOMACS_GNU_EMACSCLIENT="/missing-gnu-client")
        self.rejected(["--bin-dir", "/missing"], env, "selected acceptance input missing")

    def test_selected_missing_oracle_fails_before_native_launch(self):
        with tempfile.TemporaryDirectory(prefix="gui-control-") as temporary:
            root = Path(temporary)
            for name in ["neomacs", "neomacsclient", "module.so"]:
                (root / name).touch()
            env = dict(os.environ, NEOMACS_EWM_MODULE=str(root / "module.so"),
                       NEOMACS_GNU_EMACSCLIENT=str(root / "missing-oracle"))
            self.rejected(["--bin-dir", str(root)], env, "selected acceptance input missing")
            self.assertEqual(set(path.name for path in root.iterdir()),
                             {"neomacs", "neomacsclient", "module.so"})


if __name__ == "__main__":
    unittest.main()
