"""Unit tests for scripts/ci/run-doctests.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT_PATH = Path(__file__).resolve().parents[1] / "run-doctests.py"
SPEC = importlib.util.spec_from_file_location("run_doctests", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
run_doctests = importlib.util.module_from_spec(SPEC)
import sys
sys.modules["run_doctests"] = run_doctests
SPEC.loader.exec_module(run_doctests)


class ValidateCrateLogTests(unittest.TestCase):
    def _write_src(self, root: Path, rel: str, content: str) -> None:
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def test_accepts_passing_compile_fail_with_matching_error_code(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_src(
                root,
                "crates/demo/src/lib.rs",
                "/// ```compile_fail,E0603\n/// let _ = demo::Private;\n/// ```\npub struct Marker;\n",
            )
            log = (
                "   Doc-tests demo\n"
                "running 1 test\n"
                "test crates/demo/src/lib.rs - Marker (line 1) - compile fail ... "
                "error[E0603]: struct `Private` is private\n"
                "ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s\n"
            )
            ok, lines = run_doctests.validate_crate_log("demo", log, root, min_passed=1)
            self.assertTrue(ok, lines)
            self.assertIn("1 compile_fail error-code contract(s) verified", lines[0])

    def test_rejects_false_green_compile_fail_wrong_error_code(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_src(
                root,
                "crates/demo/src/lib.rs",
                "/// ```compile_fail,E0616\n/// use wrong_crate::Foo;\n/// ```\npub struct Marker;\n",
            )
            log = (
                "   Doc-tests demo\n"
                "running 1 test\n"
                "test crates/demo/src/lib.rs - Marker (line 1) - compile fail ... "
                "error[E0433]: cannot find module or crate `wrong_crate` in this scope\n"
                "ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s\n"
            )
            ok, lines = run_doctests.validate_crate_log("demo", log, root, min_passed=1)
            self.assertFalse(ok)
            self.assertTrue(any("expected E0616" in line and "E0433" in line for line in lines), lines)

    def test_rejects_bare_uncoded_compile_fail(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_src(
                root,
                "crates/demo/src/lib.rs",
                "/// ```compile_fail\n/// let x: i32 = \"no\";\n/// ```\npub struct Marker;\n",
            )
            log = (
                "   Doc-tests demo\n"
                "running 1 test\n"
                "test crates/demo/src/lib.rs - Marker (line 1) - compile fail ... "
                "error[E0308]: mismatched types\n"
                "ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s\n"
            )
            ok, lines = run_doctests.validate_crate_log("demo", log, root, min_passed=1)
            self.assertFalse(ok)
            self.assertTrue(any("bare `compile_fail`" in line for line in lines), lines)

    def test_rejects_ignored_doctests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = (
                "   Doc-tests demo\n"
                "running 1 test\n"
                "test crates/demo/src/lib.rs - foo (line 10) ... ignored\n"
                "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
            )
            ok, lines = run_doctests.validate_crate_log("demo", log, root, min_passed=0)
            self.assertFalse(ok)
            self.assertTrue(any("1 ignored doctest(s) are forbidden" in line for line in lines), lines)

    def test_rejects_failed_doctest_and_nonzero_exit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = (
                "   Doc-tests demo\n"
                "running 1 test\n"
                "test crates/demo/src/lib.rs - broken (line 4) ... FAILED\n"
                "failures:\n"
                "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s\n"
            )
            ok, lines = run_doctests.validate_crate_log(
                "demo", log, root, min_passed=1, exit_code=101
            )
            self.assertFalse(ok)
            self.assertTrue(any("exited with status 101" in line for line in lines), lines)
            self.assertTrue(any("FAILED doctest" in line for line in lines), lines)

    def test_rejects_missing_summary(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            ok, lines = run_doctests.validate_crate_log("demo", "compiling...\n", root, min_passed=0)
            self.assertFalse(ok)
            self.assertTrue(any("no `test result:` summary" in line for line in lines), lines)

    def test_rejects_passed_below_floor(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = (
                "   Doc-tests demo\n"
                "running 0 tests\n"
                "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
            )
            ok, lines = run_doctests.validate_crate_log("demo", log, root, min_passed=3)
            self.assertFalse(ok)
            self.assertTrue(any("only 0 doctest(s) passed (expected >= 3)" in line for line in lines), lines)

    def test_accepts_inner_doc_comment_fence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_src(
                root,
                "crates/demo/src/lib.rs",
                "//! ```rust,compile_fail,E0599\n//! demo::Nope::missing();\n//! ```\n",
            )
            log = (
                "   Doc-tests demo\n"
                "running 1 test\n"
                "test crates/demo/src/lib.rs - (line 1) - compile fail ... "
                "error[E0599]: no associated item named `missing`\n"
                "ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s\n"
            )
            ok, lines = run_doctests.validate_crate_log("demo", log, root, min_passed=1)
            self.assertTrue(ok, lines)


if __name__ == "__main__":
    unittest.main()
