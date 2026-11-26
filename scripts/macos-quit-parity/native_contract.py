"""Paired native quit callbacks, run in one guarded fresh-build job on macOS.

The injected helper observes the installed delegate and drives the actual menu
item. It does not replace quit policy. Each editor and control path is owned by
this invocation; missing injection, oracle, desktop, or input prerequisites fail.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from driver_socket import DriverBlocked, PhysicalDriver, preflight as driver_preflight


class BlockedPrerequisite(Exception):
    pass


def load_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def wait_for(child, path, predicate, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = load_json(path)
        if value is not None and predicate(value):
            return value
        if child.poll() is not None:
            raise AssertionError(f"owned editor exited {child.returncode}; waiting for {path}")
        time.sleep(.05)
    raise AssertionError(f"deadline waiting for {path}: {load_json(path)}")


def stop(child):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=4)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def run(args):
    if sys.platform != "darwin":
        raise RuntimeError("native quit contract requires a macOS desktop")
    root = Path.cwd()
    fixture = root / "scripts/macos-quit-parity"
    artifacts = root / "tmp" / f"native-quit-contract-{os.getpid()}-{time.time_ns()}"
    artifacts.mkdir(parents=True)
    oracle = args.gnu_prefix / "bin/emacs"
    checker = ["scripts/ci/check-gnu-oracle.sh", str(oracle), "31.1",
               str(args.gnu_prefix / "share/gnu-emacs-oracle/build-manifest"),
               "a360712c9d272d950d8d8255ef74570f7e90b7d9"]
    checked = subprocess.run(checker, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (artifacts / "oracle-check.log").write_text(checked.stdout)
    print("ATTEST", json.dumps(checker), "rc", checked.returncode, checked.stdout, flush=True)
    checked.check_returncode()
    assert "oracle profile: macos-ns" in checked.stdout, "native Cocoa oracle profile required"
    driver = artifacts / "native-driver.dylib"
    subprocess.run(["xcrun", "clang", "-fobjc-arc", "-fblocks", "-dynamiclib",
                    "-framework", "AppKit", "-framework", "Foundation",
                    str(fixture / "native_driver.m"), "-o", str(driver)], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", str(driver)], check=True)
    request = artifacts / "native-request"
    subprocess.run(["xcrun", "swiftc", str(fixture / "request.swift"), "-o", str(request)], check=True)
    preflight = driver_preflight(args.driver_socket)
    print("PERSISTENT_DRIVER_PREFLIGHT", json.dumps(preflight), flush=True)
    editors = [("gnu", oracle), ("neo", args.neo_binary)]
    cases = [("menu", "nil", "super"), ("menu", "t", "meta"),
             ("terminate", "nil", "super"), ("apple-event", "nil", "super"),
             ("lisp-key", "nil", "super"),
             ("delegate-nil", "nil", "super"),
             ("delegate-nil", "let-restored", "super"),
             ("delegate-cancel", "t", "super"), ("delegate-accept", "t", "super")]
    cases += [("physical-key", "nil", "super"), ("physical-key", "nil", "meta")]
    results = []
    binding_form = r"""(let ((original-q (lookup-key global-map [?\s-q])))
  (unwind-protect
      (progn (global-set-key [?\s-q] #'ignore)
        (prin1 (list (eq (key-binding [ns-power-off]) #'save-buffers-kill-emacs)
                     (eq (get 'ns-power-off 'non-key-event) t)
                     (eq (key-binding [?\s-q]) #'ignore))))
    (define-key global-map [?\s-q] original-q)))"""
    for editor, binary in editors:
        print("BINARY", editor, binary, hashlib.sha256(binary.read_bytes()).hexdigest(), flush=True)
        binding = subprocess.run([str(binary), "--batch", "-Q", "--eval", binding_form],
                                 env=dict(os.environ, NEOMACS_RUNTIME_ROOT=str(root)),
                                 text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        print("FORM", binding_form, "EDITOR", editor, "RC", binding.returncode,
              "OUT", binding.stdout, "ERR", binding.stderr, flush=True)
        assert binding.returncode == 0 and binding.stdout.strip() == "(t t t)", "default NS bindings"
        for action, confirm, modifier in cases:
            label = "-".join((editor, action, confirm, modifier))
            control = artifacts / (label + ".control")
            native_path = artifacts / (label + ".native.json")
            lisp_base = artifacts / (label + ".lisp")
            env = dict(os.environ, NEOMACS_RUNTIME_ROOT=str(root),
                       DYLD_INSERT_LIBRARIES=str(driver),
                       NEOMACS_QUIT_NATIVE_CONTROL=str(control),
                       NEOMACS_QUIT_NATIVE_RESULT=str(native_path),
                       NEOMACS_QUIT_NATIVE_LISP=str(lisp_base),
                       NEOMACS_QUIT_NATIVE_CONFIRM=confirm,
                       NEOMACS_QUIT_NATIVE_MODIFIER=modifier,
                       NEOMACS_QUIT_NATIVE_LISP_KEY="t" if action == "lisp-key" else "nil")
            env.pop("DISPLAY", None)
            env.pop("WAYLAND_DISPLAY", None)
            row = {"case": label, "editor": editor, "action": action, "confirm": confirm,
                   "modifier": modifier, "passed": False}
            with open(artifacts / (label + ".log"), "w") as log:
                child = subprocess.Popen([str(binary), "-Q", "-l", str(fixture / "native-contract.el")],
                                         env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL)
                try:
                    ready = wait_for(child, Path(str(lisp_base) + ".ready"), lambda _: True)
                    native_ready = wait_for(child, native_path, lambda v: v.get("status") in ("ready", "error"))
                    row.update(ready=ready, native_ready=native_ready)
                    assert ready["pid"] == child.pid and ready["window-system"] == ("ns" if editor == "gnu" else "neo")
                    assert ready["confirm"] is (confirm == "t"), ready
                    assert ready["power-off-binding"] == "save-buffers-kill-emacs" and ready["power-off-non-key"] is True
                    assert native_ready["status"] == "ready" and native_ready["pid"] == child.pid
                    assert native_ready["image_loaded"] is True, "driver injection must be observed"
                    assert native_ready["main_thread"] is True and native_ready["witness_installed"] is True
                    assert Path(native_ready["driver_image"]).resolve() == driver.resolve()
                    command = action
                    if action == "lisp-key":
                        command = "snapshot"
                    if action == "physical-key":
                        try:
                            with PhysicalDriver(args.driver_socket) as physical:
                                row["driver_preflight"] = physical.command_q(child.pid, binary)
                        except DriverBlocked as error:
                            raise BlockedPrerequisite(str(error)) from error
                        row["input_source"] = "persistent org.neomacs.GuiDriver socket"
                        command = "snapshot"
                    if action == "apple-event":
                        request_action = "apple-event"
                        external = subprocess.run([str(request), str(child.pid), request_action],
                                                  text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=8)
                        row["external"] = {"rc": external.returncode, "output": external.stdout}
                        if external.returncode in (42, 43):
                            raise BlockedPrerequisite(external.stdout.strip())
                        assert external.returncode == 0, external.stdout
                        command = "snapshot"
                    control.write_text(command + "\n")
                    native = wait_for(child, native_path, lambda v: v.get("status") in ("done", "error"))
                    row["native"] = native
                    assert native["status"] == "done", native
                    assert native["pid"] == child.pid and native["image_loaded"] is True
                    if action.startswith("delegate-"):
                        expected = 0 if action == "delegate-cancel" else 1
                        assert native["native_reply"] == expected, native
                        assert native["delegate_calls"] == 1, native
                        assert native["alert_seen"] is (confirm == "t"), native
                        if confirm == "t":
                            button = "Cancel" if action == "delegate-cancel" else "Save Buffers and Exit"
                            assert native["button_title"] == button, native
                        time.sleep(.2)
                        state_path = Path(str(lisp_base) + ".state")
                        state = (wait_for(child, state_path, lambda _: True, seconds=2)
                                 if state_path.exists() else ready)
                        assert state["command-count"] == state["query-count"] == state["kill-count"] == 0
                    elif action in ("physical-key", "lisp-key"):
                        state = wait_for(child, Path(str(lisp_base) + ".state"), lambda v: v.get("key") is not None)
                        assert state["key"] == modifier + "-q", state
                        assert state["command-count"] == state["query-count"] == state["kill-count"] == 0
                        assert native["delegate_calls"] == 0, native
                    else:
                        state = wait_for(child, Path(str(lisp_base) + ".state"), lambda v: v.get("query-count") == 1 and v.get("command-completed") is True)
                        assert state["command-count"] == state["query-count"] == 1, state
                        assert state["kill-count"] == 0 and state["key"] is None, state
                        if action != "apple-event":
                            assert native["delegate_calls"] == 0, native
                        # AppKit's AppleEvent path is observed against live GNU:
                        # on this host it asks the delegate before terminate:.
                        # The complete count/reply tuple is paired below.
                    assert child.poll() is None, "query veto/direct delegate call must leave editor alive"
                    row.update(state=state, alive=True, passed=True)
                except BlockedPrerequisite as error:
                    row["blocked"] = str(error)
                    row["passed"] = None
                except Exception as error:
                    row["failure"] = repr(error)
                    row["native_last"] = load_json(native_path)
                    row["lisp_last"] = load_json(Path(str(lisp_base) + ".state"))
                finally:
                    # Freeze the observation before SIGTERM teardown: GNU may
                    # legitimately run kill-emacs-hook while handling that signal.
                    (artifacts / (label + ".before-teardown.json")).write_text(json.dumps(row, indent=2))
                    stop(child)
                    row["teardown_exit"] = child.returncode
                    row["teardown_state"] = load_json(Path(str(lisp_base) + ".state"))
            if row.get("failure"):
                row["editor_log_tail"] = (artifacts / (label + ".log")).read_text(errors="replace")[-8000:]
            results.append(row)
            print("ROW", json.dumps(row, sort_keys=True), flush=True)
            (artifacts / "results.json").write_text(json.dumps(results, indent=2))
    pairs = []
    by_case = {(row["editor"], row["action"], row["confirm"], row["modifier"]): row for row in results}
    for action, confirm, modifier in cases:
        gnu = by_case[("gnu", action, confirm, modifier)]
        neo = by_case[("neo", action, confirm, modifier)]
        pair = {"action": action, "confirm": confirm, "modifier": modifier}
        if gnu["passed"] is True and neo["passed"] is True:
            def recorded_tuple(row):
                return {**{key: row["state"][key] for key in
                           ("command-count", "command-completed", "query-count", "kill-count", "key", "event")},
                        **{key: row["native"][key] for key in
                           ("delegate_calls", "native_reply", "alert_seen", "button_title")}}
            expected = recorded_tuple(gnu)
            actual = recorded_tuple(neo)
            pair.update(gnu=expected, neo=actual, equal=actual == expected)
            if actual != expected:
                neo["passed"] = False
                neo["pair_failure"] = {"gnu": expected, "neo": actual}
        else:
            pair.update(equal=None, gnu_passed=gnu["passed"], neo_passed=neo["passed"],
                        gnu_blocked=gnu.get("blocked"), neo_blocked=neo.get("blocked"))
        pairs.append(pair)
        print("PAIR", json.dumps(pair, sort_keys=True), flush=True)
    (artifacts / "pairs.json").write_text(json.dumps(pairs, indent=2))
    (artifacts / "results.json").write_text(json.dumps(results, indent=2))
    failures = [row["case"] for row in results if row["passed"] is False
                or (row.get("blocked") and (args.physical or row["action"] != "physical-key"))]
    blocked = [row["case"] for row in results if row.get("blocked")]
    print("BLOCKED", blocked, flush=True)
    print("ARTIFACTS", artifacts, "FAILED", failures, flush=True)
    return bool(failures)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gnu-prefix", type=Path, required=True)
    parser.add_argument("--neo-binary", type=Path,
                        default=Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "release/neomacs")
    parser.add_argument("--driver-socket", default=os.environ.get("NEOMACS_GUI_DRIVER_SOCKET"))
    parser.add_argument("--physical", action="store_true", help="also require permitted physical Cmd-Q injection")
    sys.exit(run(parser.parse_args()))
