"""Diagnose native GNU quit delivery through LaunchServices, not a shell launch."""

import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--gnu-prefix", type=Path, required=True)
parser.add_argument("--bundle", type=Path, required=True)
args = parser.parse_args()
root = Path.cwd()
fixture = root / "scripts/macos-quit-parity"
artifacts = root / "tmp" / ("macos-bundle-quit-" + str(os.getpid()))
artifacts.mkdir(parents=True)
# The oracle recipe deliberately installs a standard prefix rather than a
# self-contained app. The retained source bundle's executable may be from a
# different configure invocation. Copy only its packaging into our own tmp
# directory and replace its executable with the exact checked installed one.
bundle = artifacts / "Emacs.app"
shutil.copytree(args.bundle, bundle, symlinks=True)
executable = bundle / "Contents/MacOS/Emacs"
executable.unlink(missing_ok=True)
shutil.copy2((args.gnu_prefix / "bin/emacs").resolve(), executable)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


assert digest(executable) == digest(args.gnu_prefix / "bin/emacs")
print("PRIVATE ORACLE BUNDLE", bundle, "sha256", digest(executable), flush=True)
subprocess.run(
    ["scripts/ci/check-gnu-oracle.sh", str(executable),
     "31.1", str(args.gnu_prefix / "share/gnu-emacs-oracle/build-manifest"),
     "a360712c9d272d950d8d8255ef74570f7e90b7d9"], check=True,
)
subprocess.run(
    ["xcrun", "swiftc", str(fixture / "request.swift"),
     "-o", str(artifacts / "native-request")], check=True,
)


def owned_pids(init):
    """Only processes launched with this invocation's unique init-file path."""
    table = subprocess.check_output(["ps", "-axo", "pid=,stat=,command="], text=True)
    result = []
    for line in table.splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) == 3 and str(init) in fields[2] and " -l " in fields[2]:
            if "Emacs" in fields[2] and not fields[1].startswith("Z"):
                result.append(int(fields[0]))
    return result


def stop_owned(init):
    for sig in (signal.SIGTERM, signal.SIGKILL):
        for pid in owned_pids(init):
            try:
                os.kill(pid, sig)
            except ProcessLookupError:
                pass
        deadline = time.monotonic() + 4
        while owned_pids(init) and time.monotonic() < deadline:
            time.sleep(.05)


rows = []
for heartbeat in (False, True):
    for action, confirm in [("terminate", "nil"), ("terminate", "t"),
                            ("apple-event", "nil"), ("apple-event", "t")]:
        label = "-".join((action, confirm, "timer" if heartbeat else "idle"))
        ready = artifacts / (label + ".ready")
        result = artifacts / (label + ".result")
        init = artifacts / (label + "-init.el")
        settings = dict(NEOMACS_QUIT_READY=str(ready), NEOMACS_QUIT_RESULT=str(result),
                        NEOMACS_QUIT_CONFIRM=confirm)
        forms = [f"(setenv {json.dumps(k)} {json.dumps(v)})" for k, v in settings.items()]
        forms.append(f"(load {json.dumps(str(fixture / 'init.el'))} nil t)")
        if heartbeat:
            # GNU terminate: stores a keyboard event without explicitly
            # stopping NSApp's run loop. Observe again with normal Lisp
            # timer wakeups; retain the idle observations separately.
            forms.append("(run-at-time 0.1 0.1 #'ignore)")
        init.write_text("\n".join(forms) + "\n")
        try:
            launch = subprocess.run(
                ["open", "-n", "-a", str(bundle), "--args", "-Q", "-l", str(init)],
                text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=10,
            )
            deadline = time.monotonic() + 15
            while not ready.exists() and time.monotonic() < deadline:
                time.sleep(.05)
            if not ready.exists():
                row = dict(case=label, blocked="no bundle readiness", launch_rc=launch.returncode,
                           launch=launch.stdout, owned=owned_pids(init))
            else:
                witness = json.loads(ready.read_text())
                pid = witness["pid"]
                if pid not in owned_pids(init):
                    raise RuntimeError("ready PID did not belong to this owned bundle launch")
                control = subprocess.run(
                    [str(artifacts / "native-request"), str(pid), action],
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=8,
                )
                time.sleep(2)
                row = dict(case=label, ready=witness, control_rc=control.returncode,
                           control=control.stdout.strip(), alive=pid in owned_pids(init),
                           event=result.read_text() if result.exists() else None)
            rows.append(row)
            print(json.dumps(row), flush=True)
        finally:
            stop_owned(init)
(artifacts / "results.json").write_text(json.dumps(rows, indent=2))
print("BUNDLE ARTIFACTS", artifacts, flush=True)
