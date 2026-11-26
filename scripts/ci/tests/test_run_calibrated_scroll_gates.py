#!/usr/bin/env python3
"""Unit tests for scripts/ci/run-calibrated-scroll-gates.sh placement and provenance."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
SCRIPT = REPO_ROOT / "scripts/ci/run-calibrated-scroll-gates.sh"


def write_executable(path: Path, body: str) -> None:
    path.write_text(body, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


class RunCalibratedScrollGatesTests(unittest.TestCase):
    def run_script(
        self, args: list[str], extra_env: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        for var in (
            "SCROLL120_REMOTE_HOST",
            "SCROLL120_REMOTE_RUN_BIN",
            "SCROLL120_FAIL_IF_BUSY",
            "SCROLL120_QUEUE_TIMEOUT_SEC",
            "SCROLL120_LANE",
            "SCROLL120_ARTIFACT",
            "SCROLL120_TEST_ACTUAL_HOSTNAME",
            "SCROLL120_USE_PREBUILT_BINARY",
            "SCROLL120_ORACLE_BIN",
            "SCROLL120_ORACLE_MANIFEST",
            "NEOMACS_GUI_SCROLL_STREAM_INPUTS",
        ):
            env.pop(var, None)
        if extra_env:
            env.update(extra_env)
        if not env.get("TMPDIR") or not Path(env["TMPDIR"]).is_dir():
            env["TMPDIR"] = "/tmp"
        return subprocess.run(
            ["bash", str(SCRIPT), *args],
            cwd=REPO_ROOT,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def make_minimal_bin(self, bin_dir: Path, tools: tuple[str, ...]) -> None:
        bin_dir.mkdir(exist_ok=True)
        for tool in tools:
            found = subprocess.run(
                ["which", tool], capture_output=True, text=True, check=False
            ).stdout.strip()
            if found:
                target = bin_dir / tool
                if not target.exists():
                    target.symlink_to(found)

    def test_refuses_local_execution_by_default_with_hard_stop(self) -> None:
        result = self.run_script([])
        self.assertEqual(result.returncode, 2)
        self.assertIn("REFUSING local execution", result.stderr)
        self.assertIn("--remote treeroot", result.stderr)
        self.assertIn("--allow-local", result.stderr)

    def test_on_remote_host_refuses_mismatched_hostname_hard_stop(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            artifact = Path(td) / "prov.json"
            result = self.run_script(
                ["--on-remote-host", "treeroot", "--artifact", str(artifact)],
                {"SCROLL120_TEST_ACTUAL_HOSTNAME": "Matrix"},
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn(
                "REFUSING --on-remote-host treeroot on mismatched host Matrix",
                result.stderr,
            )
            self.assertFalse(artifact.exists())

    def test_allow_local_refuses_when_load_is_two_or_higher(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            loadavg = td_path / "loadavg"
            loadavg.write_text("5.12 4.80 4.50 3/900 12345\n", encoding="utf-8")
            artifact = td_path / "prov.json"
            result = self.run_script(
                ["--allow-local", "--artifact", str(artifact)],
                {"SCROLL120_LOADAVG_FILE": str(loadavg)},
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("REFUSING loaded local host", result.stderr)
            self.assertIn("provenance=local-UNTRUSTED", result.stderr)
            self.assertTrue(artifact.is_file())
            doc = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(doc["provenance"], "local-UNTRUSTED")
            self.assertEqual(doc["loadavg_start"], "5.12 4.80 4.50")
            self.assertEqual(doc["n"], {"stream_inputs": 480, "probe_samples": 5, "scenarios": 2})
            self.assertEqual(doc["exit_code"], 2)

    def test_allow_local_and_loaded_stamps_local_untrusted_and_records_n(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            bin_dir = td_path / "bin"
            bin_dir.mkdir()
            fake_neomacs = td_path / "neomacs"
            write_executable(fake_neomacs, "#!/usr/bin/env bash\nexit 0\n")
            write_executable(bin_dir / "sway", "#!/usr/bin/env bash\nexit 0\n")
            write_executable(
                bin_dir / "glmark2",
                "#!/usr/bin/env bash\n"
                "echo '    GL_RENDERER: llvmpipe (LLVM 22, 256 bits)'\n"
                "for f in 2.400 2.500 2.600 2.700 2.800; do echo \"FrameTime: $f ms\"; done\n",
            )
            write_executable(
                bin_dir / "xvfb-run",
                '#!/usr/bin/env bash\n'
                'while [[ "$1" == -* ]]; do shift; [[ "$1" == -screen* ]] && shift; done\n'
                'exec "$@"\n',
            )
            write_executable(
                bin_dir / "taskset",
                '#!/usr/bin/env bash\nshift 2\nexec "$@"\n',
            )
            write_executable(
                bin_dir / "timeout",
                '#!/usr/bin/env bash\nwhile [[ "$1" == -* ]]; do shift; done\nshift\nexec "$@"\n',
            )
            cargo_log = td_path / "cargo.args"
            write_executable(
                bin_dir / "cargo",
                f'#!/usr/bin/env bash\nprintf "%s\\n" "$*" > "{cargo_log}"\nexit 0\n',
            )
            loadavg = td_path / "loadavg"
            loadavg.write_text("5.25 4.90 4.10 4/900 12345\n", encoding="utf-8")
            governor = td_path / "gov"
            governor.write_text("performance\n", encoding="utf-8")
            artifact = td_path / "prov.json"

            result = self.run_script(
                [
                    "--allow-local",
                    "--allow-loaded-local",
                    "--artifact",
                    str(artifact),
                ],
                {
                    "PATH": f"{bin_dir}:{os.environ['PATH']}",
                    "NEOMACS_GUI_TEST_BINARY": str(fake_neomacs),
                    "NEOMACS_GUI_SCROLL_STREAM_INPUTS": "600",
                    "SCROLL120_LOADAVG_FILE": str(loadavg),
                    "SCROLL120_GOVERNOR_FILE": str(governor),
                    "SCROLL120_CPU_PIN": "0-3",
                },
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("provenance=local-UNTRUSTED", result.stdout)
            self.assertIn("provenance=local-UNTRUSTED", result.stderr)
            expected_cmd = (
                "nextest run -p neomacs-gui-tests -E test(continuous_120hz_) --no-fail-fast"
            )
            self.assertIn(expected_cmd, cargo_log.read_text(encoding="utf-8"))
            doc = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(doc["provenance"], "local-UNTRUSTED")
            self.assertEqual(doc["governor"], "performance")
            self.assertEqual(doc["loadavg_start"], "5.25 4.90 4.10")
            self.assertEqual(doc["loadavg_end"], "5.25 4.90 4.10")
            self.assertEqual(doc["n"], {"stream_inputs": 600, "probe_samples": 5, "scenarios": 2})
            self.assertEqual(doc["capacity_tier"], "SoftwareVulkanFast")
            self.assertEqual(doc["cpu_pin"], "0-3")
            self.assertEqual(doc["exit_code"], 0)
            self.assertEqual(len(doc["binary_sha256"]), 64)

    def test_remote_clears_stale_artifact_and_uses_run_unique_remote_path(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            recorded = td_path / "remote-run.record"
            remote_run = td_path / "remote-run.sh"
            write_executable(
                remote_run,
                "#!/usr/bin/env bash\n"
                f'printf "WHOLE=%s\\nARGS=%s\\n" "${{REMOTE_RUN_WHOLE_HOST:-}}" "$*" > "{recorded}"\n'
                "exit 0\n",
            )
            fake_scp = td_path / "scp"
            write_executable(
                fake_scp,
                "#!/usr/bin/env bash\n"
                'dst="${@: -1}"\n'
                'printf \'{"provenance":"treeroot-calibrated","host":"TreeRoot"}\\n\' > "$dst"\n',
            )
            fake_ssh = td_path / "ssh"
            write_executable(fake_ssh, "#!/usr/bin/env bash\nexit 0\n")
            artifact = td_path / "out-prov.json"
            artifact.write_text('{"stale": true}\n', encoding="utf-8")

            result = self.run_script(
                [
                    "--remote",
                    "treeroot",
                    "--remote-runner",
                    str(remote_run),
                    "--lane",
                    "scroll120test",
                    "--queue-timeout",
                    "60",
                    "--artifact",
                    str(artifact),
                ],
                {
                    "SCROLL120_SCP_BIN": str(fake_scp),
                    "SCROLL120_SSH_BIN": str(fake_ssh),
                },
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("queueing on treeroot whole-host lock", result.stderr)
            rec_text = recorded.read_text(encoding="utf-8")
            self.assertIn("WHOLE=1", rec_text)
            self.assertIn("treeroot scroll120test", rec_text)
            self.assertIn("--on-remote-host treeroot", rec_text)
            self.assertIn("rm -f /tmp/scroll120-scroll120test-", rec_text)
            doc = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(doc["provenance"], "treeroot-calibrated")

            # On failure when scp has no new remote file, stale local artifact
            # must have been removed instead of surviving.
            fail_runner = td_path / "fail-runner.sh"
            write_executable(fail_runner, "#!/usr/bin/env bash\nexit 124\n")
            fail_scp = td_path / "fail-scp"
            write_executable(fail_scp, "#!/usr/bin/env bash\nexit 1\n")
            stale_artifact = td_path / "stale-prov.json"
            stale_artifact.write_text('{"provenance":"old"}\n', encoding="utf-8")
            fail_res = self.run_script(
                [
                    "--remote",
                    "treeroot",
                    "--remote-runner",
                    str(fail_runner),
                    "--artifact",
                    str(stale_artifact),
                ],
                {
                    "SCROLL120_SCP_BIN": str(fail_scp),
                    "SCROLL120_SSH_BIN": str(fake_ssh),
                },
            )
            self.assertEqual(fail_res.returncode, 124)
            self.assertFalse(stale_artifact.exists())

    def test_remote_fail_if_busy_distinguishes_254_busy_from_255_ssh_error(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            remote_run = td_path / "remote-run.sh"
            write_executable(remote_run, "#!/usr/bin/env bash\nexit 99\n")

            busy_ssh = td_path / "ssh-busy"
            write_executable(busy_ssh, "#!/usr/bin/env bash\nexit 254\n")
            busy_res = self.run_script(
                [
                    "--remote",
                    "treeroot",
                    "--remote-runner",
                    str(remote_run),
                    "--fail-if-busy",
                ],
                {"SCROLL120_SSH_BIN": str(busy_ssh)},
            )
            self.assertEqual(busy_res.returncode, 3)
            self.assertIn("treeroot is busy", busy_res.stderr)
            self.assertIn("without --fail-if-busy", busy_res.stderr)

            err_ssh = td_path / "ssh-err"
            write_executable(err_ssh, "#!/usr/bin/env bash\nexit 255\n")
            err_res = self.run_script(
                [
                    "--remote",
                    "treeroot",
                    "--remote-runner",
                    str(remote_run),
                    "--fail-if-busy",
                ],
                {"SCROLL120_SSH_BIN": str(err_ssh)},
            )
            self.assertEqual(err_res.returncode, 255)
            self.assertNotIn("is busy", err_res.stderr)
            self.assertIn("SSH busy-probe to treeroot failed (exit 255)", err_res.stderr)

    def test_on_remote_host_always_rebuilds_even_when_binary_exists_and_traps_early(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            fake_neomacs = td_path / "neomacs"
            write_executable(fake_neomacs, "#!/usr/bin/env bash\nexit 0\n")
            bin_dir = td_path / "bin"
            self.make_minimal_bin(
                bin_dir,
                (
                    "bash",
                    "awk",
                    "cut",
                    "date",
                    "dirname",
                    "mkdir",
                    "nproc",
                    "python3",
                    "rm",
                    "sha256sum",
                    "hostname",
                    "cat",
                    "grep",
                    "tr",
                ),
            )
            cargo_log = td_path / "cargo.calls"
            write_executable(
                bin_dir / "cargo",
                f'#!/usr/bin/env bash\nprintf "%s\\n" "$*" >> "{cargo_log}"\nexit 17\n',
            )
            loadavg = td_path / "loadavg"
            loadavg.write_text("0.42 0.35 0.28 1/650 999\n", encoding="utf-8")
            governor = td_path / "gov"
            governor.write_text("performance\n", encoding="utf-8")
            artifact = td_path / "remote-prov.json"

            # Even though fake_neomacs already exists, --on-remote-host MUST
            # invoke `cargo xtask fresh-build --release` and trap its failure
            # into the provenance JSON artifact (B1 + B2).
            result = self.run_script(
                ["--on-remote-host", "treeroot", "--artifact", str(artifact)],
                {
                    "PATH": str(bin_dir),
                    "SCROLL120_TEST_ACTUAL_HOSTNAME": "TreeRoot",
                    "NEOMACS_GUI_TEST_BINARY": str(fake_neomacs),
                    "SCROLL120_TIP_SHA": "9ca34ffd1c9450183825ab916cac313729bdf419",
                    "SCROLL120_LOADAVG_FILE": str(loadavg),
                    "SCROLL120_GOVERNOR_FILE": str(governor),
                    "SCROLL120_CPU_PIN": "0-11",
                },
            )
            self.assertEqual(result.returncode, 17)
            self.assertIn("xtask fresh-build --release", cargo_log.read_text(encoding="utf-8"))
            self.assertTrue(artifact.is_file())
            doc = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(doc["provenance"], "treeroot-calibrated")
            self.assertEqual(doc["host"], "TreeRoot")
            self.assertEqual(doc["tip_sha"], "9ca34ffd1c9450183825ab916cac313729bdf419")
            self.assertEqual(doc["exit_code"], 17)

    def test_on_remote_host_prebuilt_stamp_and_oracle_profile_recorded(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            fake_neomacs = td_path / "neomacs"
            write_executable(fake_neomacs, "#!/usr/bin/env bash\nexit 0\n")
            sha = hashlib.sha256(fake_neomacs.read_bytes()).hexdigest()
            stamp = td_path / "neomacs.fresh-build-stamp"
            stamp.write_text(f"binary_sha256 = {sha}\n", encoding="utf-8")

            # Fake GNU oracle matching check-gnu-oracle.sh contract.
            mc_line = next(
                line
                for line in (REPO_ROOT / "parity-reference.toml")
                .read_text(encoding="utf-8")
                .splitlines()
                if line.startswith("mirror_commit = ")
            )
            mirror_commit = mc_line.split('"')[1]
            ev_line = next(
                line
                for line in (REPO_ROOT / "parity-reference.toml")
                .read_text(encoding="utf-8")
                .splitlines()
                if line.startswith("emacs_version = ")
            )
            emacs_version = ev_line.split('"')[1]
            oracle_prefix = td_path / "oracle-prefix"
            (oracle_prefix / "lib" / "pkgconfig").mkdir(parents=True)
            (oracle_prefix / "lib" / "pkgconfig" / "tree-sitter.pc").write_text(
                f"prefix={oracle_prefix}\nVersion: 0.25.0\n",
                encoding="utf-8",
            )
            bindings_dir = oracle_prefix / "share" / "emacs" / emacs_version / "lisp"
            bindings_dir.mkdir(parents=True)
            (bindings_dir / "bindings.elc").write_text("(#$ . 10)\n", encoding="utf-8")
            oracle_manifest = td_path / "build-manifest"
            oracle_manifest.write_text(
                f"prefix={oracle_prefix}\n"
                f"emacs_version={emacs_version}\n"
                f"commit={mirror_commit}\n"
                "profile=linux-gtk3\n"
                "tree_sitter_version=0.25.0\n"
                "tree_sitter_sha256=deadbeef\n"
                f"configure_arg=--prefix={oracle_prefix}\n"
                "configure_arg=--with-x-toolkit=gtk3\n",
                encoding="utf-8",
            )
            fake_oracle = td_path / "emacs-oracle"
            features = (
                "CAIRO DBUS FREETYPE GIF GLIB GMP GNUTLS GSETTINGS HARFBUZZ "
                "JPEG LCMS2 LIBXML2 MODULES NOTIFY INOTIFY PDUMPER PNG RSVG "
                "SECCOMP THREADS TIFF TOOLKIT_SCROLL_BARS TREE_SITTER WEBP "
                "X11 XDBE XIM XINERAMA XINPUT2 XPM XRANDR GTK3 ZLIB"
            )
            write_executable(
                fake_oracle,
                "#!/usr/bin/env bash\n"
                'if [[ "$1" == "--version" ]]; then\n'
                f'  echo "GNU Emacs {emacs_version}"\n'
                "  exit 0\n"
                "fi\n"
                'if [[ "$*" == *"emacs-version"* ]]; then\n'
                f'  printf "%s\\n" "{emacs_version}"\n'
                "  exit 0\n"
                "fi\n"
                'if [[ "$*" == *"system-configuration-features"* ]]; then\n'
                f'  printf "%s\\n" "{features}"\n'
                "  exit 0\n"
                "fi\n"
                'if [[ "$*" == *"system-configuration-options"* ]]; then\n'
                f'  printf "%s\\n" "--prefix={oracle_prefix} --with-x-toolkit=gtk3"\n'
                "  exit 0\n"
                "fi\n"
                "exit 0\n",
            )

            bin_dir = td_path / "emptybin"
            self.make_minimal_bin(
                bin_dir,
                (
                    "bash",
                    "awk",
                    "cut",
                    "date",
                    "dirname",
                    "mkdir",
                    "nproc",
                    "python3",
                    "rm",
                    "sha256sum",
                    "hostname",
                    "cat",
                    "grep",
                    "tr",
                    "uname",
                    "sed",
                    "head",
                    "sort",
                    "comm",
                    "mktemp",
                    "wc",
                ),
            )
            loadavg = td_path / "loadavg"
            loadavg.write_text("0.42 0.35 0.28 1/650 999\n", encoding="utf-8")
            governor = td_path / "gov"
            governor.write_text("performance\n", encoding="utf-8")
            artifact = td_path / "remote-prov.json"

            result = self.run_script(
                ["--on-remote-host", "treeroot", "--artifact", str(artifact)],
                {
                    "PATH": str(bin_dir),
                    "SCROLL120_TEST_ACTUAL_HOSTNAME": "TreeRoot",
                    "SCROLL120_USE_PREBUILT_BINARY": "1",
                    "NEOMACS_GUI_TEST_BINARY": str(fake_neomacs),
                    "SCROLL120_ORACLE_BIN": str(fake_oracle),
                    "SCROLL120_ORACLE_MANIFEST": str(oracle_manifest),
                    "SCROLL120_TIP_SHA": "9ca34ffd1c9450183825ab916cac313729bdf419",
                    "SCROLL120_LOADAVG_FILE": str(loadavg),
                    "SCROLL120_GOVERNOR_FILE": str(governor),
                    "SCROLL120_CPU_PIN": "0-11",
                },
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("sway is missing on TreeRoot", result.stderr)
            self.assertTrue(artifact.is_file())
            doc = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(doc["provenance"], "treeroot-calibrated")
            self.assertEqual(doc["host"], "TreeRoot")
            self.assertEqual(doc["binary_sha256"], sha)
            self.assertIsNotNone(doc["oracle_profile"])
            self.assertIn("oracle profile: linux-gtk3", doc["oracle_profile"])
            self.assertEqual(doc["n"], {"stream_inputs": 480, "probe_samples": 5, "scenarios": 2})
            self.assertEqual(doc["exit_code"], 2)


if __name__ == "__main__":
    unittest.main()
