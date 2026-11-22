"""Profile only owned Neo processes after the clean paired baseline.

The pinned profiler is extracted into this lane's ignored tmp directory.
It saves locally, and the fixture exits itself after the navigation witness.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tarfile
import time
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("--source-sha", required=True)
parser.add_argument("--modes", nargs="+", default=["timer", "synthetic-cn"])
parser.add_argument("--coverage-reuse", choices=["off", "on"], default="on")
args = parser.parse_args()
root = Path.cwd()
source = root / "scripts/macos-navigation-parity"
artifacts = root / "tmp" / f"macos-navigation-profile-{os.getpid()}"
artifacts.mkdir(parents=True)
tools = root / "tmp/macos-navigation-tools"
tools.mkdir(exist_ok=True)
samply = tools / "samply-0.13.1"
archive = tools / "samply-aarch64-apple-darwin.tar.xz"
expected = "7597239aa3769e75058be5ed359dbfe067f5e7714a5f052c45dd81d509aec17f"
if not archive.exists():
    url = ("https://github.com/mstange/samply/releases/download/samply-v0.13.1/"
           "samply-aarch64-apple-darwin.tar.xz")
    urllib.request.urlretrieve(url, archive)
assert hashlib.sha256(archive.read_bytes()).hexdigest() == expected
with tarfile.open(archive) as package:
    members = [m for m in package.getmembers() if m.isfile() and Path(m.name).name == "samply"]
    assert len(members) == 1
    with package.extractfile(members[0]) as stream:
        samply.write_bytes(stream.read())
    samply.chmod(0o755)
print("PROFILER", subprocess.check_output([str(samply), "--version"], text=True), flush=True)
print("ARTIFACTS", artifacts, flush=True)
subprocess.run(["xcrun", "swiftc", str(source / "desktop.swift"),
                "-o", str(artifacts / "desktop")], check=True)
fixture = artifacts / "navigation.md"
fixture.write_text("".join(f"Line {i:03d}: cursor navigation test.\n" for i in range(260)))
editor = Path(os.environ["CARGO_TARGET_DIR"]) / "release/neomacs"


def command_for(pid):
    return subprocess.run(["ps", "-p", str(pid), "-o", "command="], text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL).stdout


def wait_json(path, child, timeout):
    deadline = time.monotonic() + timeout
    while not path.exists():
        if child.poll() is not None or time.monotonic() > deadline:
            raise RuntimeError(f"missing {path}; profiler status={child.poll()}")
        time.sleep(.05)
    return json.loads(path.read_text())


def signal_owned_editor(pid, token, sig):
    if token not in command_for(pid):
        return False
    try:
        os.kill(pid, sig)
        return True
    except ProcessLookupError:
        return False


def owned_editor_pids(group, token):
    listing = subprocess.check_output(["ps", "-axo", "pid=,pgid=,command="], text=True)
    for line in listing.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[1] == str(group) and token in parts[2]:
            yield int(parts[0])


for mode in args.modes:
    if mode not in ["timer", "synthetic-cn"]:
        raise ValueError(mode)
    case = artifacts / mode
    case.mkdir()
    profile = case / "profile.json"
    # The unique path remains in the editor argv, so cleanup can validate
    # ownership even though samply, rather than Python, starts the editor.
    token_form = "(setq navigation-bench-profile-token " + json.dumps(str(case)) + ")"
    argv = [str(samply), "record", "--save-only", "--unstable-presymbolicate",
            "--output", str(profile), "--", str(editor), "-Q", "--font=Menlo-13",
            "--geometry=100x35", "--eval", token_form, "-l", str(source / "init.el")]
    env = dict(os.environ, RUST_LOG="error", NEOMACS_RUNTIME_ROOT=str(root),
               NEOMACS_COVERAGE_EXPORT_REUSE=args.coverage_reuse,
               NAVIGATION_BENCH_DIR=str(case), NAVIGATION_BENCH_MODE=mode,
               NAVIGATION_BENCH_FILE=str(fixture), NAVIGATION_BENCH_EXIT="1")
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    (case / "command.json").write_text(json.dumps({"source": args.source_sha,
                                                  "coverage_export_reuse": args.coverage_reuse,
                                                  "controller_host_policy": os.environ.get(
                                                      "NEOMACS_BENCH_CONTROLLER_POLICY", "not recorded"),
                                                  "argv": argv}))
    editor_pid = None
    with (case / "profile.log").open("w") as log:
        child = subprocess.Popen(argv, env=env, stdout=log, stderr=log,
                                 start_new_session=True)
        try:
            ready = wait_json(case / "ready.json", child, 45)
            editor_pid = ready["pid"]
            assert str(case) in command_for(editor_pid), "profile ownership token absent"
            subprocess.run([str(artifacts / "desktop"), str(editor_pid)], check=True)
            time.sleep(20)
            (case / "go").write_text("go\n")
            done = wait_json(case / "done.json", child, 100)
            assert done["moves"] == 260 and done["final_line"] == 261, done
            status = child.wait(timeout=120)
            print("PROFILE", mode, "rc", status, "witness", json.dumps(done), flush=True)
            if status != 0:
                raise RuntimeError(f"samply failed: see {case / 'profile.log'}")
            with (case / "top.json").open("w") as top:
                subprocess.run(["python3", str(source / "profile_top.py"), str(profile),
                                "--witness", str(case / "done.json")], stdout=top, check=True)
            ranking = json.loads((case / "top.json").read_text())["navigation_window"]
            print("TOP", mode, json.dumps(ranking), flush=True)
        finally:
            # Also discover our uniquely tagged editor if the profiler failed
            # before ready.json and therefore before we learned its PID.
            owned = set(owned_editor_pids(child.pid, str(case)))
            if editor_pid is not None:
                owned.add(editor_pid)
            for pid in owned:
                signal_owned_editor(pid, str(case), signal.SIGTERM)
                deadline = time.monotonic() + 4
                while str(case) in command_for(pid) and time.monotonic() < deadline:
                    time.sleep(.05)
                signal_owned_editor(pid, str(case), signal.SIGKILL)
            if child.poll() is None:
                # This process group was created above exclusively for our
                # profiler and its children, never an existing editor session.
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                child.wait(timeout=10)
