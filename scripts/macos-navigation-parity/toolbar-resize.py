"""N-607 toolbar resize: paired GTK3/Xvfb or native NS allocation witnesses."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import secrets
import select
import subprocess
import sys
import time


# No names/images, event posting or other editors' window details are emitted.
# https://developer.apple.com/documentation/coregraphics/required-window-list-keys
NATIVE_OBSERVER = r"""
import CoreGraphics
import Foundation
guard CommandLine.arguments.count == 2, let pid = Int(CommandLine.arguments[1]), pid > 0,
      let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements],
                                              kCGNullWindowID) as? [[String: Any]] else {
    fputs("No GUI window-server session or invalid owned PID\n", stderr)
    exit(2)
}
let owned = windows.compactMap { window -> [String: Any]? in
    guard let owner = window[kCGWindowOwnerPID as String] as? NSNumber,
          owner.intValue == pid,
          let layer = window[kCGWindowLayer as String] as? NSNumber, layer.intValue == 0,
          let number = window[kCGWindowNumber as String] as? NSNumber,
          let bounds = window[kCGWindowBounds as String] as? [String: NSNumber],
          let x = bounds["X"], let y = bounds["Y"],
          let width = bounds["Width"], let height = bounds["Height"],
          width.doubleValue > 0, height.doubleValue > 0 else { return nil }
    return ["owner_pid": owner.intValue, "window_id": number.stringValue,
            "x": x.doubleValue, "y": y.doubleValue,
            "width": width.doubleValue, "height": height.doubleValue,
            "units": "quartz-screen-coordinates"]
}
let data = try JSONSerialization.data(withJSONObject: owned, options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
"""


def stop(child):
    """Only a Popen owned by this invocation may reach this cleanup path."""
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=4)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def output(argv, **kwargs):
    return subprocess.check_output(argv, text=True, timeout=10, **kwargs)


@contextmanager
def display_session(platform, artifacts, runtime_root):
    env = dict(os.environ, RUST_LOG="error", NEOMACS_RUNTIME_ROOT=str(runtime_root))
    env.pop("WAYLAND_DISPLAY", None)
    if platform == "macos-ns":
        for name in ("DISPLAY", "XAUTHORITY", "GDK_BACKEND", "WINIT_UNIX_BACKEND"):
            env.pop(name, None)
        swift = artifacts / "native-windows.swift"
        swift.write_text(NATIVE_OBSERVER)
        observer = artifacts / "native-windows"
        subprocess.run(["xcrun", "swiftc", str(swift), "-o", str(observer)], check=True)

        def observe(pid):
            return json.loads(output([str(observer), str(pid)]))

        yield env, observe
        return

    authority = artifacts / "Xauthority"
    cookie = secrets.token_hex(16)
    subprocess.run(["xauth", "-f", str(authority), "add", "localhost:65535", ".", cookie],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
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
            env.update(DISPLAY=display, XAUTHORITY=str(authority), GDK_BACKEND="x11",
                       WINIT_UNIX_BACKEND="x11", GSETTINGS_BACKEND="memory")

            def observe(pid):
                found = subprocess.run(
                    ["xdotool", "search", "--onlyvisible", "--pid", str(pid)],
                    env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
                if found.returncode == 1:
                    return []
                found.check_returncode()
                windows = []
                for window in found.stdout.splitlines():
                    owner = int(output(["xdotool", "getwindowpid", window], env=env).strip())
                    assert owner == pid, (pid, owner, window)
                    raw = output(["xdotool", "getwindowgeometry", "--shell", window], env=env)
                    values = dict(line.split("=", 1) for line in raw.splitlines())
                    windows.append({"owner_pid": owner, "window_id": window,
                                    **{name.lower(): int(values[name]) for name in
                                       ("X", "Y", "WIDTH", "HEIGHT")},
                                    "units": "x11-pixels", "xdotool_geometry": raw})
                return windows

            yield env, observe
        finally:
            stop(xvfb)


def settled_window(observe, child, expected=None):
    deadline = time.monotonic() + 10
    previous, stable, windows = None, 0, []
    while time.monotonic() < deadline:
        assert child.poll() is None, ("editor exited during native observation", child.returncode)
        windows = observe(child.pid)
        if len(windows) == 1:
            current = windows[0]
            assert current["owner_pid"] == child.pid
            if expected is not None:
                assert current["window_id"] == expected, (expected, current)
            stable = stable + 1 if current == previous else 0
            previous = current
            if stable >= 2:
                return current
        else:
            previous, stable = None, 0
        time.sleep(.2)
    raise RuntimeError(f"owned PID {child.pid} did not expose one settled visible window: {windows}")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--gnu-prefix", type=Path, required=True)
parser.add_argument("--neo-binary", type=Path, required=True)
parser.add_argument("--runtime-root", type=Path, required=True)
parser.add_argument("--product-sha", required=True)
parser.add_argument("--warmup", type=float, default=20)
parser.add_argument("--platform", choices=("auto", "linux-x11", "macos-ns"), default="auto")
parser.add_argument("--arms", choices=("canonical", "inhibited"), nargs="+",
                    default=["canonical", "inhibited"])
parser.add_argument("--font", help="default: Menlo-13 on NS, DejaVu Sans Mono-12 on X11")
args = parser.parse_args()
host = "macos-ns" if sys.platform == "darwin" else "linux-x11" if sys.platform == "linux" else None
platform = host if args.platform == "auto" else args.platform
assert platform is not None and platform == host, "run each native twin on its own host platform"
assert args.warmup >= 0 and len(args.arms) == len(set(args.arms))
font = args.font or ("Menlo-13" if platform == "macos-ns" else "DejaVu Sans Mono-12")
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
assert ("macos-ns" if platform == "macos-ns" else "linux-gtk3") in attest.stdout
editors = {"gnu": oracle, "neo": args.neo_binary}
policies = {"canonical": "(tab-bar-lines)", "inhibited": "t"}
metadata = {
    "row": "N-607-toolbar-implied-resize", "platform": platform,
    "product_source_sha": args.product_sha, "runtime_root": str(args.runtime_root),
    "runtime_source_sha": output(["git", "-C", str(args.runtime_root), "rev-parse", "HEAD"]).strip(),
    "product_files_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
                             (args.neo_binary, args.neo_binary.with_suffix(".pdump"),
                              args.neo_binary.with_suffix(".fresh-build-stamp"))},
    "resize_policy_overrides": {arm: policies[arm] for arm in args.arms},
    "warmup_seconds": args.warmup, "font_argument": font,
    "controller_host_policy": os.environ.get("NEOMACS_BENCH_CONTROLLER_POLICY", "not recorded"),
    "harness_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
                       (source / "toolbar-resize.py", source / "toolbar-resize.el")},
    "editors": {name: {"path": str(binary), "version": output([str(binary), "--version"]),
                        "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
                for name, binary in editors.items()}}
assert args.product_sha in metadata["editors"]["neo"]["version"]
(artifacts / "metadata.json").write_text(json.dumps(metadata, indent=2))
results = {}
with display_session(platform, artifacts, args.runtime_root) as (env, observe):
    for arm in args.arms:
        results[arm] = {}
        for editor, binary in editors.items():
            case = artifacts / arm / editor
            case.mkdir(parents=True)
            argv = [str(binary), "-Q", f"--font={font}", "--geometry=100x35",
                    "-l", str(source / "toolbar-resize.el")]
            (case / "command.json").write_text(json.dumps(argv))
            with (case / "editor.log").open("w") as log:
                child = subprocess.Popen(
                    argv, env=dict(env, TOOLBAR_RESIZE_DIR=str(case), TOOLBAR_RESIZE_POLICY=arm),
                    stdin=subprocess.DEVNULL, stdout=log, stderr=log)
                try:
                    time.sleep(args.warmup)
                    initial = settled_window(observe, child)
                    stages = results[arm][editor] = {}
                    for stage in ("baseline", "off", "on"):
                        (case / f"go-{stage}").write_text("go\n")
                        result = case / f"{stage}.json"
                        deadline = time.monotonic() + 20
                        while not result.exists():
                            if child.poll() is not None or time.monotonic() > deadline:
                                raise RuntimeError(f"missing {result}; status={child.poll()}")
                            time.sleep(.05)
                        data = json.loads(result.read_text())
                        assert data["pid"] == child.pid and data["arm"] == arm
                        assert data["window_system"] == (
                            "neo" if editor == "neo" else "ns" if platform == "macos-ns" else "x")
                        assert data["inhibit_policy"] == policies[arm]
                        data["native_geometry"] = settled_window(observe, child, initial["window_id"])
                        stages[stage] = data
                        (artifacts / "results.json").write_text(json.dumps(results, indent=2))
                        print(arm.upper(), editor.upper(), stage, json.dumps(data), flush=True)
                    (case / "go-exit").write_text("go\n")
                    assert child.wait(timeout=5) == 0
                finally:
                    stop(child)

differences, contracts = {}, {}
fields = ("frame_height", "frame_text_lines", "frame_native_height",
          "frame_pixel_height", "frame_text_height")
stages_order = ("baseline", "off", "on")
for arm, editors_result in results.items():
    contracts[arm] = {}
    for editor, stages in editors_result.items():
        for stage, data in stages.items():
            if data["frame_height"] != data["frame_text_lines"]:
                differences[f"{arm}.{editor}.{stage}.height_equals_text_lines"] = [
                    data["frame_height"], data["frame_text_lines"]]
        toolbar = [stages[s]["tool_bar_lines"] for s in stages_order]
        if toolbar != [1, 0, 1]:
            differences[f"{arm}.{editor}.tool_bar_lines"] = toolbar
        observed = {field: [stages[s][field] for s in stages_order] for field in fields}
        observed.update({f"native_{field}": [stages[s]["native_geometry"][field] for s in stages_order]
                         for field in ("width", "height")})
        contracts[arm][editor] = {field: {"values": values,
                                        "deltas_from_baseline": [v - values[0] for v in values]}
                                  for field, values in observed.items()}
        conserved = ()
        if platform == "linux-x11":
            conserved = fields if arm == "canonical" else ("native_width", "native_height")
        for field in conserved:
            if len(set(observed[field])) != 1:
                differences[f"{arm}.{editor}.{field}"] = observed[field]
    if platform == "macos-ns":
        # NS's setter documents an inhibition TODO (nsfns.m:725-729). The
        # attested NS canonical arm itself changes [35,32,35] text lines and
        # [529,497,529] native pixels; GTK conservation is not its contract.
        # Both NS policies follow their own observed GNU transition deltas,
        # without normalizing away row or pixel magnitude mismatches.
        for field in contracts[arm]["gnu"]:
            deltas = {editor: contracts[arm][editor][field]["deltas_from_baseline"]
                      for editor in ("gnu", "neo")}
            if deltas["gnu"] != deltas["neo"]:
                differences[f"{arm}.ns_gnu_transition.{field}"] = deltas
(artifacts / "observed-contracts.json").write_text(json.dumps(contracts, indent=2))
(artifacts / "differences.json").write_text(json.dumps(differences, indent=2))
print("OBSERVED_CONTRACTS", json.dumps(contracts), flush=True)
print("DIFFERENCES", json.dumps(differences), flush=True)
assert not differences, "N-607 toolbar policy/height/allocation parity failed; inspect differences.json"
