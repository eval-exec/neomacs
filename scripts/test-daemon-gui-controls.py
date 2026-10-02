#!/usr/bin/env python3
"""Cheap fail-closed controls for the opt-in GUI acceptance entry point."""
import ast
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock

SCRIPT = Path(__file__).with_name("test-daemon-gui.py")


class GuiReplyBudgetTests(unittest.TestCase):
    def test_eval_reply_budget_tracks_bounded_subprocess_timeout(self):
        # Exercise the nested helper without starting a compositor or daemon.
        source = ast.parse(SCRIPT.read_text())
        evaluate = next(node for node in ast.walk(source)
                        if isinstance(node, ast.FunctionDef) and node.name == "evaluate")
        runner = Mock(return_value=subprocess.CompletedProcess([], 0, "42\n", ""))
        records = []
        namespace = {"subprocess": Mock(run=runner), "bin_dir": Path("/native"),
                     "ROOT": Path("/source"), "host_env": {}, "records": records}
        exec(compile(ast.Module(body=[evaluate], type_ignores=[]), str(SCRIPT), "exec"),
             namespace)
        for timeout in [30, 60, 90, 15]:
            with self.subTest(timeout=timeout):
                self.assertEqual(namespace["evaluate"]("(+ 20 22)", timeout), "42")
                argv = runner.call_args.args[0]
                self.assertEqual(argv[argv.index("-w") + 1], str(timeout + 5))
                self.assertEqual(runner.call_args.kwargs["timeout"], timeout)
        self.assertEqual(len(records), 4)
        runner.return_value = subprocess.CompletedProcess([], 1, "", "reply failed")
        with self.assertRaisesRegex(RuntimeError, "reply failed"):
            namespace["evaluate"]("(+ 20 22)")
        runner.side_effect = subprocess.TimeoutExpired("neomacsclient", 30)
        with self.assertRaises(subprocess.TimeoutExpired):
            namespace["evaluate"]("(+ 20 22)")


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
