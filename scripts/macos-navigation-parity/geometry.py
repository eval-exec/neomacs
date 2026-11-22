"""Paired native GUI row: N-607 frame/font geometry, with attested GNU NS."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument("--gnu-prefix", type=Path, required=True)
parser.add_argument("--product-sha", required=True)
parser.add_argument("--warmup", type=float, default=20)
parser.add_argument("--startup-phases", action="store_true",
                    help="Use isolated early/init files instead of -Q and record startup phases")
parser.add_argument("--rust-log", default="error",
                    help="Existing targeted Rust logs; use separately from clean timing rows")
parser.add_argument("--resize-policy", choices=("gtk-ns", "own-bars"),
                    help="Controlled early-init policy; requires --startup-phases")
args = parser.parse_args()
if args.resize_policy and not args.startup_phases:
    parser.error("--resize-policy requires --startup-phases")
root = Path.cwd()
source = root / "scripts/macos-navigation-parity"
artifacts = root / "tmp" / f"macos-frame-geometry-{os.getpid()}"
artifacts.mkdir(parents=True)
print("ARTIFACTS", artifacts, flush=True)
oracle = args.gnu_prefix / "bin/emacs"
attest = subprocess.run(
    ["scripts/ci/check-gnu-oracle.sh", str(oracle), "31.1",
     str(args.gnu_prefix / "share/gnu-emacs-oracle/build-manifest"),
     "a360712c9d272d950d8d8255ef74570f7e90b7d9"],
    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(artifacts / "oracle.txt").write_text(attest.stdout)
print(attest.stdout, flush=True)
attest.check_returncode()
harness_files = [source / "geometry.el", source / "geometry.py"]
if args.startup_phases:
    harness_files.append(source / "startup-phases.el")
editors = {"gnu": oracle, "neo": Path(os.environ["CARGO_TARGET_DIR"]) / "release/neomacs"}
metadata = {
    "row": "N-607-frame-geometry", "product_source_sha": args.product_sha,
    "warmup_seconds": args.warmup,
    "startup_phases": args.startup_phases, "rust_log": args.rust_log,
    "resize_policy_override": args.resize_policy,
    "controller_host_policy": os.environ.get("NEOMACS_BENCH_CONTROLLER_POLICY", "not recorded"),
    "harness_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                       for p in harness_files},
    "editors": {name: {"path": str(binary),
                        "version": subprocess.check_output([str(binary), "--version"], text=True),
                        "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
                for name, binary in editors.items()}}
assert args.product_sha in metadata["editors"]["neo"]["version"]
(artifacts / "metadata.json").write_text(json.dumps(metadata, indent=2))
results = {}
for name, binary in editors.items():
    case = artifacts / name
    case.mkdir()
    env = dict(os.environ, GEOMETRY_BENCH_DIR=str(case), NEOMACS_RUNTIME_ROOT=str(root),
               RUST_LOG=args.rust_log)
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    startup = ["-Q"]
    if args.startup_phases:
        init = case / "init"
        init.mkdir()
        policy = ""
        if args.resize_policy:
            tail = " tool-bar-lines" if args.resize_policy == "own-bars" else ""
            policy = "(setq frame-inhibit-implied-resize '(tab-bar-lines" + tail + "))\n"
        (init / "early-init.el").write_text(
            policy + "(load " + json.dumps(str(source / "startup-phases.el")) + " nil nil t)\n")
        (init / "init.el").write_text("(setq inhibit-startup-screen t)\n")
        startup = ["--no-site-file", "--no-site-lisp", "--init-directory", str(init)]
        env["NEOMACS_TRACE_FRAME_GEOMETRY"] = "1"
    argv = [str(binary), *startup, "--font=Menlo-13", "--geometry=100x35",
            "-l", str(source / "geometry.el")]
    (case / "command.json").write_text(json.dumps(argv))
    with (case / "editor.log").open("w") as log:
        child = subprocess.Popen(argv, env=env, stdout=log, stderr=log)
        try:
            time.sleep(args.warmup)
            assert child.poll() is None, (name, child.returncode)
            (case / "go").write_text("go\n")
            deadline = time.monotonic() + 30
            result = case / "result.json"
            while not result.exists():
                if child.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f"missing {result}; status={child.poll()}")
                time.sleep(.05)
            results[name] = json.loads(result.read_text())
            assert results[name]["pid"] == child.pid
            assert results[name]["window_system"] in ("ns", "neo")
            print(name.upper(), json.dumps(results[name]), flush=True)
            assert child.wait(timeout=5) == 0
            if args.startup_phases:
                phases = {}
                for phase in ("early-init", "after-init", "window-setup"):
                    path = case / f"phase-{phase}.json"
                    if path.exists():
                        phases[phase] = json.loads(path.read_text())
                assert "early-init" in phases and "after-init" in phases, (name, phases)
                (case / "phases.json").write_text(json.dumps(phases, indent=2))
                print("PHASES", name, json.dumps(phases), flush=True)
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
(artifacts / "results.json").write_text(json.dumps(results, indent=2))
fields = ("frame_pixel_height", "frame_char_height", "line_pixel_height", "frame_rows")
differences = {field: {editor: results[editor][field] for editor in editors}
               for field in fields if results["gnu"][field] != results["neo"][field]}
(artifacts / "differences.json").write_text(json.dumps(differences, indent=2))
print("DIFFERENCES", json.dumps(differences), flush=True)
assert not differences, "N-607 frame/font geometry differs from attested GNU"
