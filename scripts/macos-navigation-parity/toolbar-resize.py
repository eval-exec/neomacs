"""N-607-toolbar-implied-resize: attested GTK3/Neo GUI row on an owned Xvfb."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import select
import subprocess
import time


def stop(child):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=4)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


parser = argparse.ArgumentParser()
parser.add_argument("--gnu-prefix", type=Path, required=True)
parser.add_argument("--neo-binary", type=Path, required=True)
parser.add_argument("--runtime-root", type=Path, required=True)
parser.add_argument("--product-sha", required=True)
parser.add_argument("--warmup", type=float, default=20)
args = parser.parse_args()
root = Path.cwd()
source = root / "scripts/macos-navigation-parity"
artifacts = root / "tmp" / f"toolbar-implied-resize-{os.getpid()}"
artifacts.mkdir(mode=0o700, parents=True)
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
assert "linux-gtk3" in attest.stdout
editors = {"gnu": oracle, "neo": args.neo_binary}
metadata = {
    "row": "N-607-toolbar-implied-resize", "product_source_sha": args.product_sha,
    "runtime_root": str(args.runtime_root),
    "runtime_source_sha": subprocess.check_output(
        ["git", "-C", str(args.runtime_root), "rev-parse", "HEAD"], text=True).strip(),
    "product_files_sha256": {
        p.name: hashlib.sha256(p.read_bytes()).hexdigest()
        for p in (args.neo_binary, args.neo_binary.with_suffix(".pdump"),
                  args.neo_binary.with_suffix(".fresh-build-stamp"))},
    "resize_policy_override": "(tab-bar-lines)", "warmup_seconds": args.warmup,
    "controller_host_policy": os.environ.get("NEOMACS_BENCH_CONTROLLER_POLICY", "not recorded"),
    "harness_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                       for p in (source / "toolbar-resize.py", source / "toolbar-resize.el")},
    "editors": {name: {"path": str(binary),
                        "version": subprocess.check_output([str(binary), "--version"], text=True),
                        "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
                for name, binary in editors.items()}}
assert args.product_sha in metadata["editors"]["neo"]["version"]
(artifacts / "metadata.json").write_text(json.dumps(metadata, indent=2))

authority = artifacts / "Xauthority"
cookie = secrets.token_hex(16)
subprocess.run(["xauth", "-f", str(authority), "add", "localhost:65535", ".", cookie],
               check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
results = {}
with (artifacts / "xvfb.log").open("w") as log:
    xvfb = subprocess.Popen(
        ["Xvfb", "-displayfd", "1", "-screen", "0", "1920x1080x24",
         "-nolisten", "tcp", "-noreset", "-auth", str(authority)],
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=log, text=True)
    try:
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb did not publish its display")
        number = xvfb.stdout.readline().strip()
        assert number.isdecimal(), number
        display = ":" + number
        subprocess.run(["xauth", "-f", str(authority), "add", display, ".", cookie], check=True)
        env = dict(os.environ, DISPLAY=display, XAUTHORITY=str(authority),
                   GDK_BACKEND="x11", WINIT_UNIX_BACKEND="x11", GSETTINGS_BACKEND="memory",
                   RUST_LOG="error", NEOMACS_RUNTIME_ROOT=str(args.runtime_root))
        env.pop("WAYLAND_DISPLAY", None)
        for editor, binary in editors.items():
            case = artifacts / editor
            case.mkdir()
            argv = [str(binary), "-Q", "--font=DejaVu Sans Mono-12", "--geometry=100x35",
                    "-l", str(source / "toolbar-resize.el")]
            (case / "command.json").write_text(json.dumps(argv))
            with (case / "editor.log").open("w") as log:
                child = subprocess.Popen(argv, env=dict(env, TOOLBAR_RESIZE_DIR=str(case)),
                                         stdin=subprocess.DEVNULL, stdout=log, stderr=log)
                try:
                    time.sleep(args.warmup)
                    assert child.poll() is None, (editor, child.returncode)
                    windows = subprocess.check_output(
                        ["xdotool", "search", "--onlyvisible", "--pid", str(child.pid)],
                        env=env, text=True).splitlines()
                    assert len(windows) == 1, (editor, windows)
                    results[editor] = {}
                    for stage in ("baseline", "off", "on"):
                        (case / f"go-{stage}").write_text("go\n")
                        result = case / f"{stage}.json"
                        deadline = time.monotonic() + 20
                        while not result.exists():
                            if child.poll() is not None or time.monotonic() > deadline:
                                raise RuntimeError(f"missing {result}; status={child.poll()}")
                            time.sleep(.05)
                        data = json.loads(result.read_text())
                        assert data["pid"] == child.pid
                        assert data["window_system"] in ("x", "neo")
                        assert data["inhibit_policy"] == "(tab-bar-lines)"
                        data["xwindow_id"] = windows[0]
                        data["xwindow_geometry"] = subprocess.check_output(
                            ["xdotool", "getwindowgeometry", "--shell", windows[0]],
                            env=env, text=True)
                        results[editor][stage] = data
                        print(editor.upper(), stage, json.dumps(data), flush=True)
                    assert child.wait(timeout=5) == 0
                finally:
                    stop(child)
    finally:
        stop(xvfb)
(artifacts / "results.json").write_text(json.dumps(results, indent=2))
differences = {}
for editor, stages in results.items():
    # Preserve each editor's settled text grid, independently of the separate
    # startup geometry discrepancy and external versus internal toolbar bands.
    for field in ("frame_height", "frame_text_lines", "frame_pixel_height", "frame_text_height"):
        values = [stages[s][field] for s in ("baseline", "off", "on")]
        if len(set(values)) != 1:
            differences[f"{editor}.{field}"] = values
    assert [stages[s]["tool_bar_lines"] for s in ("baseline", "off", "on")] == [1, 0, 1]
(artifacts / "differences.json").write_text(json.dumps(differences, indent=2))
print("DIFFERENCES", json.dumps(differences), flush=True)
assert not differences, "N-607 toolbar change must preserve the settled text grid"
