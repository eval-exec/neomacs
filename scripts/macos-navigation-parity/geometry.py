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
args = parser.parse_args()
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
editors = {"gnu": oracle, "neo": Path(os.environ["CARGO_TARGET_DIR"]) / "release/neomacs"}
metadata = {
    "row": "N-607-frame-geometry", "product_source_sha": args.product_sha,
    "warmup_seconds": args.warmup,
    "controller_host_policy": os.environ.get("NEOMACS_BENCH_CONTROLLER_POLICY", "not recorded"),
    "harness_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                       for p in (source / "geometry.el", source / "geometry.py")},
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
               RUST_LOG="error")
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    argv = [str(binary), "-Q", "--font=Menlo-13", "--geometry=100x35",
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
