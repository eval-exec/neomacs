"""Run after fresh-build, in the same guarded remote-run invocation.

These are observations, not a parity assertion: an accepted native request
without a Lisp route witness does not establish which GNU callback ran.
"""

import argparse
import json
import os
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--gnu-prefix", required=True)
parser.add_argument("--heartbeat", action="store_true")
args = parser.parse_args()
root = Path.cwd()
fixture = root / "scripts/macos-quit-parity"
artifacts = root / "tmp" / ("macos-quit-" + str(os.getpid()))
artifacts.mkdir(parents=True)
prefix = Path(args.gnu_prefix)
binding_form = r"""(let ((original-q (lookup-key global-map [?\s-q])))
  (unwind-protect
      (progn
        (global-set-key [?\s-q] #'ignore)
        (prin1
         (list (eq (key-binding [ns-power-off])
                   (when (eq system-type 'darwin) #'save-buffers-kill-emacs))
               (eq (get 'ns-power-off 'non-key-event) (eq system-type 'darwin))
               (eq (key-binding [?\s-q]) #'ignore))))
    (define-key global-map [?\s-q] original-q)))"""
attest = subprocess.run(
    ["scripts/ci/check-gnu-oracle.sh", str(prefix / "bin/emacs"), "31.1",
     str(prefix / "share/gnu-emacs-oracle/build-manifest"),
     "a360712c9d272d950d8d8255ef74570f7e90b7d9"],
    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
)
print("GNU attestation rc", attest.returncode, attest.stdout, flush=True)
subprocess.run(
    ["xcrun", "swiftc", str(fixture / "request.swift"),
     "-o", str(artifacts / "native-request")], check=True,
)
rows = []


def stop(child):
    """Reap only the editor created by this probe, including modal alerts."""
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=4)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


editors = [("gnu", prefix / "bin/emacs"),
           ("neo", Path(os.environ["CARGO_TARGET_DIR"]) / "release/neomacs")]
actions = [("terminate", "nil", "super", "none"), ("terminate", "t", "super", "none"),
           ("apple-event", "nil", "super", "none"), ("apple-event", "t", "super", "none"),
           ("terminate", "nil", "super", "f6"), ("apple-event", "nil", "super", "f6"),
           ("menu", "nil", "super", "none"), ("menu", "nil", "meta", "none"),
           ("menu-unbound", "nil", "super", "none")]
for editor, binary in editors:
    if editor == "gnu" and attest.returncode:
        continue
    print(editor, subprocess.check_output([str(binary), "--version"], text=True), flush=True)
    binding = subprocess.run(
        [str(binary), "--batch", "-Q", "--eval", binding_form],
        env=dict(os.environ, NEOMACS_RUNTIME_ROOT=str(root)),
        text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
    )
    print(editor, "binding-form", binding_form, "rc", binding.returncode,
          "stdout", binding.stdout, "stderr", binding.stderr, flush=True)
    for action, confirm, modifier, pending_prefix in actions:
        label = "-".join((editor, action, confirm, modifier, pending_prefix))
        ready = artifacts / (label + ".ready")
        result = artifacts / (label + ".result")
        env = dict(os.environ, NEOMACS_RUNTIME_ROOT=str(root),
                   NEOMACS_QUIT_READY=str(ready), NEOMACS_QUIT_RESULT=str(result),
                   NEOMACS_QUIT_CONFIRM=confirm,
                   NEOMACS_QUIT_COMMAND_MODIFIER=modifier,
                   NEOMACS_QUIT_UNBIND_SUPER_Q="t" if action == "menu-unbound" else "nil",
                   NEOMACS_QUIT_PREFIX=pending_prefix,
                   NEOMACS_QUIT_HEARTBEAT="t" if args.heartbeat else "nil")
        env.pop("DISPLAY", None)
        env.pop("WAYLAND_DISPLAY", None)
        with open(artifacts / (label + ".log"), "w") as log:
            child = subprocess.Popen(
                [str(binary), "-Q", "-l", str(fixture / "init.el")],
                env=env, stdout=log, stderr=log,
            )
            try:
                deadline = time.monotonic() + 18
                while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                    time.sleep(.05)
                if not ready.exists():
                    row = {"case": label, "blocked": "no GUI fixture readiness",
                           "exit": child.poll()}
                else:
                    control = subprocess.run(
                        [str(artifacts / "native-request"), str(child.pid), action],
                        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=8,
                    )
                    time.sleep(2)
                    row = {"case": label, "control_rc": control.returncode,
                           "control": control.stdout.strip(), "alive": child.poll() is None,
                           "event": result.read_text() if result.exists() else None,
                           "state": json.loads(Path(str(result) + ".state").read_text())
                           if Path(str(result) + ".state").exists() else None,
                           "ready": json.loads(ready.read_text())}
                rows.append(row)
                print(json.dumps(row), flush=True)
            finally:
                stop(child)
(artifacts / "results.json").write_text(json.dumps(rows, indent=2))
print("ARTIFACTS", artifacts, flush=True)
