#!/usr/bin/env python3
"""Opt-in real EWM/native/GNU-client acceptance. No desktop or seat access.

The EWM adapter uses upstream's production State and HeadlessBackend fixture;
it is not stock ewm-start or full EWM desktop Lisp compatibility.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from daemon_gui_safety import validate_render_node

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "test/daemon-gui"
EWM_REV = "d5bf1e0e8c6b3e7423b88c64db5c199442b7fdb3"
EWM_URL = "https://codeberg.org/ezemtsov/ewm.git"


def checked(argv, **kwargs):
    return subprocess.run(argv, check=True, **kwargs)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare_ewm(destination, reuse):
    source = destination / "ewm"
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("GIT_")}
    if reuse:
        # Compare the complete tracked source to base + the disclosed adapter,
        # not merely whether the patch can be reversed.
        with tempfile.TemporaryDirectory(prefix="ewm-index-") as index:
            env["GIT_INDEX_FILE"] = str(Path(index) / "index")
            head = checked(["git", "-C", str(source), "rev-parse", "HEAD"],
                           env=env, capture_output=True, text=True).stdout.strip()
            if head != EWM_REV:
                raise RuntimeError("prepared EWM revision does not match")
            checked(["git", "-C", str(source), "read-tree", EWM_REV], env=env)
            checked(["git", "-C", str(source), "apply", "--cached",
                     str(FIXTURE / "ewm-headless.patch")], env=env)
            checked(["git", "-C", str(source), "diff", "--exit-code"], env=env)
    else:
        destination.mkdir(parents=True, exist_ok=False)
        checked(["git", "clone", "--no-checkout", EWM_URL, str(source)], env=env)
        checked(["git", "-C", str(source), "checkout", "--detach", EWM_REV], env=env)
        checked(["git", "-C", str(source), "apply",
                 str(FIXTURE / "ewm-headless.patch")], env=env)
        env.update(CARGO_HOME=str(destination / "cargo-home"),
                   CARGO_TARGET_DIR=str(destination / "target"))
        checked(["cargo", "build", "--locked", "--lib"],
                cwd=source / "compositor", env=env)
    module = destination / "target/debug/libewm_core.so"
    if not module.is_file():
        raise RuntimeError("selected EWM module is missing")
    receipt = {"ewm_revision": EWM_REV, "adapter_sha256": sha(FIXTURE / "ewm-headless.patch"),
               "module": str(module), "module_sha256": sha(module), "reused": reuse}
    print(json.dumps(receipt, indent=2))
    return module


def acceptance(bin_dir, module, gnu, render_node):
    for path in [module, bin_dir / "neomacs", bin_dir / "neomacsclient", gnu]:
        if not path.is_file():
            raise RuntimeError(f"selected acceptance input missing: {path}")
    validate_render_node(render_node)
    version = checked([str(gnu), "--version"], capture_output=True, text=True).stdout
    if "emacsclient" not in version:
        raise RuntimeError("selected GNU oracle is not emacsclient")
    if shutil.which("bwrap") is None:
        raise RuntimeError("Bubblewrap is required for hardware-safe acceptance")
    with tempfile.TemporaryDirectory(prefix="ng-") as temporary:
        run = Path(temporary)
        for name in ["home", "runtime", "config", "data", "cache", "tmp"]:
            (run / name).mkdir(mode=0o700)
        shutil.copyfile(module, run / "module.so")
        env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": "/run/fixture/home",
               "XDG_RUNTIME_DIR": "/run/ewm", "XDG_CONFIG_HOME": "/run/fixture/config",
               "XDG_DATA_HOME": "/run/fixture/data", "XDG_CACHE_HOME": "/run/fixture/cache",
               "TMPDIR": "/run/fixture/tmp", "NEOMACS_LOG_FILE": "/run/fixture/runtime.log",
               "NEOMACS_RUNTIME_ROOT": str(ROOT), "RUST_MIN_STACK": "134217728",
               "LIBSEAT_BACKEND": "disabled", "PROOF_MODULE": "/run/fixture/module.so",
               "PROOF_OUTPUT": "/run/fixture/receipt.el"}
        # Vulkan ICD selection may be explicit, but display variables never are.
        if "VK_DRIVER_FILES" in os.environ:
            env["VK_DRIVER_FILES"] = os.environ["VK_DRIVER_FILES"]
        host_env = dict(env, HOME=str(run / "home"), XDG_RUNTIME_DIR=str(run / "runtime"),
                        TMPDIR=str(run / "tmp"))
        argv = ["bwrap", "--die-with-parent", "--unshare-pid", "--unshare-net",
                "--ro-bind", "/", "/", "--tmpfs", "/home", "--tmpfs", "/run",
                "--tmpfs", "/tmp", "--dev", "/dev", "--proc", "/proc",
                "--ro-bind", str(ROOT), str(ROOT), "--ro-bind", str(bin_dir), str(bin_dir),
                "--bind", str(run), "/run/fixture", "--bind", str(run / "runtime"), "/run/ewm",
                "--dev-bind", render_node, render_node, "--chdir", str(ROOT),
                "/usr/bin/env", "-i", *[key + "=" + value for key, value in env.items()],
                str(bin_dir / "neomacs"), "-Q", "--fg-daemon=gui-acceptance"]
        records = []
        clients = []
        with (run / "launch.log").open("wb") as log:
            daemon = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT)
            def evaluate(expression, timeout=30):
                result = subprocess.run([str(bin_dir / "neomacsclient"), "-s", "gui-acceptance",
                                         "-w", str(timeout + 5), "-e", expression], cwd=ROOT, env=host_env,
                                        capture_output=True, text=True, timeout=timeout)
                records.append({"expression": expression, "exit": result.returncode,
                                "stdout": result.stdout, "stderr": result.stderr})
                if result.returncode:
                    raise RuntimeError(f"Lisp acceptance failed: {records[-1]}")
                return result.stdout.strip()
            def wait(expression, expected="t"):
                deadline = time.monotonic() + 30
                while evaluate(expression) != expected:
                    if daemon.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError(f"acceptance deadline: {expression}")
                    time.sleep(.05)
            def client(binary, name, nowait):
                operands = [str(binary), "-s", "gui-acceptance", "-c", "-d", "wayland-ewm-proof",
                            "-F", f'((name . "{name}"))']
                if nowait:
                    operands.append("-n")
                file = run / (name + ".txt")
                file.write_text("real client buffer\n")
                operands.append("/run/fixture/" + file.name)
                # Client file names resolve in the isolated daemon namespace.
                output = (run / (name + ".log")).open("wb")
                child = subprocess.Popen(operands, env=host_env, stdout=output,
                                         stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
                clients.append((child, output))
                return child
            failure = None
            try:
                deadline = time.monotonic() + 45
                socket = run / "runtime/emacs/gui-acceptance"
                while not socket.exists():
                    if daemon.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("display-free daemon never became ready")
                    time.sleep(.05)
                if evaluate("(+ 20 22)") != "42":
                    raise RuntimeError("readiness eval mismatch")
                evaluate(f'(load {json.dumps(str(FIXTURE / "start.el"))} nil t)', 60)
                evaluate(f'(load {json.dumps(str(FIXTURE / "cycle.el"))} nil t)', 90)
                evaluate("(delete-frame native-proof-third t)")
                wait("(= (native-proof-mapped-count) 0)")
                evaluate(f'(load {json.dumps(str(FIXTURE / "clients.el"))} nil t)')
                for label, binary in [("native", bin_dir / "neomacsclient"), ("gnu", gnu)]:
                    nowait = client(binary, label + "-nowait", True)
                    if nowait.wait(timeout=30) != 0:
                        raise RuntimeError("nowait client failed")
                    evaluate(f'(native-proof-check-client "{label}-nowait" t)')
                    evaluate(f'(delete-frame (native-proof-frame "{label}-nowait") t)')
                    wait("(= (native-proof-mapped-count) 0)")
                    waiting = client(binary, label + "-waiting", False)
                    # Do not enter a server eval filter while waiting for a
                    # second server request to be admitted: poll between filters.
                    wait(f'(and (native-proof-frame "{label}-waiting") t)')
                    evaluate(f'(native-proof-check-client "{label}-waiting" nil)')
                    evaluate(f'(native-proof-two-owned-frames "{label}-waiting")')
                    if waiting.poll() is not None:
                        raise RuntimeError("client disconnected while its second frame remained owned")
                    evaluate("(delete-frame native-proof-owned-second t)")
                    if waiting.wait(timeout=30) != 0:
                        raise RuntimeError("waiting client did not complete on last owned frame deletion")
                    wait("(= (native-proof-mapped-count) 0)")
                    for command in ["server-buffer-done", "server-edit"]:
                        name = label + "-" + command
                        completing = client(binary, name, False)
                        wait(f'(and (native-proof-frame "{name}") t)')
                        evaluate(f'(native-proof-check-client "{name}" nil)')
                        if completing.poll() is not None:
                            raise RuntimeError("waiting buffer client disconnected before completion")
                        evaluate(f'(native-proof-complete-buffer "{name}" \'{command})')
                        if completing.wait(timeout=30) != 0:
                            raise RuntimeError("waiting client did not complete on last buffer completion")
                        wait("(= (native-proof-mapped-count) 0)")
                        # Exercise ordinary admission again after each completed
                        # client, reusing the same daemon/display terminal.
                        recreated_name = name + "-recreated"
                        recreated = client(binary, recreated_name, True)
                        if recreated.wait(timeout=30) != 0:
                            raise RuntimeError("client frame recreation failed")
                        evaluate(f'(native-proof-check-client "{recreated_name}" t)')
                        if evaluate(f'(eq (frame-terminal (native-proof-frame "{recreated_name}")) native-proof-terminal)') != "t":
                            raise RuntimeError("recreated client frame lost terminal ownership")
                        evaluate(f'(delete-frame (native-proof-frame "{recreated_name}") t)')
                        wait("(= (native-proof-mapped-count) 0)")
                evaluate("(native-proof-final)")
                if "module.so" not in evaluate('(with-temp-buffer (insert-file-contents "/proc/self/maps") (buffer-string))'):
                    raise RuntimeError("real module mapping absent")
                print((run / "receipt.el").read_text())
            except Exception as error:
                failure = error
            finally:
                for child, output in clients:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)
                    output.close()
                if daemon.poll() is None:
                    try:
                        evaluate("(kill-emacs)", 15)
                        daemon.wait(timeout=15)
                    except Exception:
                        daemon.kill()
                daemon.wait(timeout=10)
                if daemon.returncode != 0 and failure is None:
                    failure = RuntimeError(f"daemon teardown exit {daemon.returncode}")
        print(json.dumps({"module_sha256": sha(module), "gnu_version": version.strip(),
                          "editor_sha256": sha(bin_dir / "neomacs"), "records": records,
                          "daemon_exit": daemon.returncode, "failure": str(failure) if failure else None}, indent=2))
        if failure:
            for name in ["launch.log", "runtime.log", *[p.name for p in run.glob("*-*.log")]]:
                if (run / name).exists():
                    print(name + "\n" + (run / name).read_text(errors="replace"))
            raise failure


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare-ewm", type=Path)
    parser.add_argument("--reuse", action="store_true", help="verify exact prepared source, do not rebuild")
    parser.add_argument("--bin-dir", type=Path)
    args = parser.parse_args()
    if args.prepare_ewm:
        prepare_ewm(args.prepare_ewm.resolve(), args.reuse)
    elif args.bin_dir:
        module = os.environ.get("NEOMACS_EWM_MODULE")
        gnu = os.environ.get("NEOMACS_GNU_EMACSCLIENT")
        if not module or not gnu:
            parser.error("GUI acceptance requires NEOMACS_EWM_MODULE and NEOMACS_GNU_EMACSCLIENT")
        acceptance(args.bin_dir.resolve(), Path(module).resolve(), Path(gnu).resolve(),
                   os.environ.get("NEOMACS_TEST_RENDER_NODE", "/dev/dri/renderD128"))
    else:
        parser.error("select --prepare-ewm or --bin-dir")


if __name__ == "__main__":
    main()
