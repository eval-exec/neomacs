#!/usr/bin/env python3
"""Count eval-thread retired instructions for identical Org body searches.

Example (run with other local timing idle; this is not a wall-time verdict):
  python3 crates/neovm-core/scripts/regex-gap-instructions.py \
    --editor /absolute/release/neomacs --runtime /absolute/runtime \
    --cpus 14 --output-dir /absolute/fresh-output

Both arms insert/delete the same byte and search the same unchanged bytes.
Startup/setup/warmup finish before perf attaches, disabled.  The measured span
starts at enable ACK, includes GO-file observation, 500 searches, DONE-file
publication and disable ACK latency.  File waits sleep for 10ms.  This is an
instruction regression guard, not an exact matcher-only instruction total or
an all-thread metric. Perf targets the editor's main thread explicitly with
--tid; --pid would also attach threads already present after warmup, even
with --no-inherit. No elapsed-time comparison determines the verdict.

The default explicit P-core event fails on unsupported hosts; select a known
supported event with --event.  Counter unavailability never counts as a pass.
Every attempt preserves raw files. Only children created here are signalled.
"""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def write_json(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def wait_file(path, child, timeout):
    deadline = time.monotonic() + timeout
    while not path.exists():
        code = child.poll()
        if code is not None:
            raise RuntimeError(f"editor exited {code} before {path.name}")
        if time.monotonic() >= deadline:
            raise TimeoutError(f"editor did not publish {path.name}")
        time.sleep(0.01)
    return json.loads(path.read_text())


def control(command_fd, ack_fd, command, perf, timeout=10):
    os.write(command_fd, command.encode() + b"\n")
    deadline = time.monotonic() + timeout
    response = b""
    while b"\n" not in response:
        if perf.poll() is not None:
            raise RuntimeError(f"perf exited {perf.returncode} before {command} ACK")
        if time.monotonic() >= deadline:
            raise TimeoutError(f"perf did not ACK {command}")
        if select.select([ack_fd], [], [], 0.1)[0]:
            response += os.read(ack_fd, 4096)
    # perf 7.2.7 writes the protocol C string including its NUL terminator.
    # Accept only the documented acknowledgement and that exact variant.
    if response not in (b"ack\n", b"ack\n\x00"):
        raise RuntimeError(f"invalid perf ACK {response!r}")
    return {"command": command, "ack": response.decode(), "at": now()}


def reap_owned(proc, label, receipt, graceful=False):
    if proc is None:
        return
    if proc.poll() is None:
        proc.send_signal(signal.SIGINT if graceful else signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=10)
    receipt[label + "_returncode"] = proc.returncode
    receipt[label + "_reaped"] = proc.poll() is not None


def parse_instructions(path, event_spec):
    if "name=" in event_spec:
        expected_labels = {event_spec.split("name=", 1)[1].split("/", 1)[0].split(",", 1)[0]}
    elif event_spec == "instructions:u":
        expected_labels = {"instructions:u", "cpu_core/instructions/u", "cpu_atom/instructions/u"}
    else:
        expected_labels = {event_spec}
    rows = []
    for line in path.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        fields = [field.strip() for field in line.split(",")]
        if len(fields) < 5:
            continue
        # Only one explicit event is requested; other perf footer lines have
        # no counter running-time field. Preserve and reject unsupported rows.
        if fields[2] not in expected_labels:
            raise RuntimeError("unexpected perf counter event label: " + line)
        if fields[0].startswith("<"):
            raise RuntimeError("counter unavailable: " + line)
        count = float(fields[0])
        running = float(fields[4].removesuffix("%"))
        if not math.isfinite(count) or count <= 0 or running != 100:
            raise RuntimeError("counter must be positive and 100% running: " + line)
        rows.append({"event": fields[2], "instructions": count,
                     "running_percent": running, "raw": line})
    if len(rows) != 1:
        raise RuntimeError(f"expected one instruction counter row, got {len(rows)}")
    return rows[0]


def run_arm(args, root, arm, repetition):
    directory = root / f"r{repetition}-{arm}"
    directory.mkdir()
    receipt = {"arm": arm, "repetition": repetition, "started_utc": now(),
               "status": "INVALID", "scope": "main-thread gated user instructions (--tid)"}
    editor = perf = None
    fds = []
    try:
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("NEOMACS_", "NEOVM_")) and k != "RUST_LOG"}
        env.update(NEOMACS_RUNTIME_ROOT=str(args.runtime), RGI_ARM=arm,
                   RGI_DIRECTORY=str(directory), RGI_LINES=str(args.lines),
                   RGI_ITERATIONS=str(args.iterations))
        fixture = Path(__file__).with_suffix(".el").resolve()
        argv = ["taskset", "-c", args.cpus, str(args.editor), "-Q", "--batch", "-l", str(fixture)]
        receipt["editor_command"] = argv
        with (directory / "editor.stdout").open("x") as out, (directory / "editor.stderr").open("x") as err:
            editor = subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err)
            receipt["editor_pid"] = editor.pid
            receipt["target_main_tid"] = editor.pid
            ready = wait_file(directory / "ready.json", editor, args.timeout)
            receipt["ready"] = ready
            for name in ("control.fifo", "ack.fifo"):
                path = directory / name
                os.mkfifo(path, 0o600)
                fds.append(os.open(path, os.O_RDWR | os.O_NONBLOCK))
            perf_argv = ["perf", "stat", "--no-big-num", "-x", ",", "-o", str(directory / "perf.csv"),
                         "--no-inherit", "-t", str(editor.pid), "-e", args.event,
                         "--delay=-1", "--control=fifo:" + str(directory / "control.fifo") + "," + str(directory / "ack.fifo")]
            receipt["perf_command"] = perf_argv
            with (directory / "perf.stdout").open("x") as pout, (directory / "perf.stderr").open("x") as perr:
                perf = subprocess.Popen(perf_argv, stdout=pout, stderr=perr, stdin=subprocess.DEVNULL)
                receipt["perf_pid"] = perf.pid
                receipt["enable"] = control(fds[0], fds[1], "enable", perf)
                (directory / "go").touch(exist_ok=False)
                done = wait_file(directory / "done.json", editor, args.timeout)
                receipt["disable"] = control(fds[0], fds[1], "disable", perf)
                receipt["done"] = done
                reap_owned(perf, "perf", receipt, graceful=True)
            (directory / "exit").touch(exist_ok=False)
            editor.wait(timeout=10)
            receipt["editor_returncode"] = editor.returncode
            receipt["editor_reaped"] = True
        if receipt["perf_returncode"] not in (0, -signal.SIGINT, 128 + signal.SIGINT):
            raise RuntimeError(f"perf failed: {receipt['perf_returncode']}")
        if editor.returncode != 0:
            raise RuntimeError(f"editor failed: {editor.returncode}")
        expected_text = b"ordinary * emphasis * and *inline* text in journal\n" * args.lines
        expected_bytes = len(expected_text)
        expected_sha256 = hashlib.sha256(expected_text).hexdigest()
        expected_gap = 1 + expected_bytes // 2 if arm == "mid" else 1 + expected_bytes
        for witness in (ready, done):
            if (witness.get("schema") != 1 or witness.get("arm") != arm
                    or witness.get("iterations") != args.iterations
                    or witness.get("bytes") != expected_bytes
                    or witness.get("sha256") != expected_sha256
                    or witness.get("gap") != expected_gap):
                raise RuntimeError("invalid ready/done result or gap witness")
        if done.get("hits") != 0 or done.get("expected_gap") != expected_gap or ready["sha256"] != done["sha256"]:
            raise RuntimeError("search changed result, gap, or text")
        receipt["counter"] = parse_instructions(directory / "perf.csv", args.event)
        receipt["status"] = "VALID"
    except BaseException as error:
        receipt["error"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        reap_owned(perf, "perf", receipt, graceful=True)
        reap_owned(editor, "editor", receipt)
        for fd in fds:
            os.close(fd)
        receipt["ended_utc"] = now()
        write_json(directory / "receipt.json", receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--editor", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--cpus", default="14")
    parser.add_argument("--event", default="cpu_core/event=0xc0,umask=0,name=regex_gap_instructions/u")
    parser.add_argument("--lines", type=int, default=1024)
    parser.add_argument("--iterations", type=int, default=500)
    parser.add_argument("--pairs", type=int, default=3)
    parser.add_argument("--max-ratio", type=float, default=1.25)
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()

    def terminate(signum, _frame):
        # Raising keeps run_arm/main finally blocks active so only our own
        # Popen children are stopped and reaped before the driver exits.
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, terminate)
    args.editor = args.editor.resolve(strict=True)
    args.runtime = args.runtime.resolve(strict=True)
    root = args.output_dir.resolve()
    if not args.runtime.is_dir() or min(args.lines, args.iterations, args.pairs) <= 0 or not math.isfinite(args.max_ratio) or args.max_ratio < 1:
        parser.error("runtime must be a directory and sizes/ratio valid")
    root.mkdir(parents=True, exist_ok=False)
    config = {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()}
    config.update(editor_sha256=digest(args.editor), driver_sha256=digest(Path(__file__)),
                  fixture_sha256=digest(Path(__file__).with_suffix(".el")),
                  target_kind="explicit editor main thread ID (--tid), no inheritance",
                  exposure="eval-thread user instructions; startup excluded; GO/DONE file-control overhead included")
    write_json(root / "config.json", config)
    summary = {"started_utc": now(), "status": "INVALID", "cases": [], "pairs": [], "max_ratio": args.max_ratio}
    try:
        for repetition in range(1, args.pairs + 1):
            pair = {}
            for arm in (("end", "mid") if repetition % 2 else ("mid", "end")):
                case = run_arm(args, root, arm, repetition)
                summary["cases"].append(case)
                pair[arm] = case
            if pair["mid"]["done"]["sha256"] != pair["end"]["done"]["sha256"]:
                raise RuntimeError("arms did not search identical text")
            ratio = pair["mid"]["counter"]["instructions"] / pair["end"]["counter"]["instructions"]
            summary["pairs"].append({"repetition": repetition, "mid_over_end": ratio, "pass": ratio <= args.max_ratio})
        summary["status"] = "PASS" if all(p["pass"] for p in summary["pairs"]) else "REGRESSION"
        print(json.dumps({"status": summary["status"], "pairs": summary["pairs"], "output_dir": str(root)}))
        return 0 if summary["status"] == "PASS" else 1
    except BaseException as error:
        summary["error"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        summary["ended_utc"] = now()
        write_json(root / "summary.json", summary)


if __name__ == "__main__":
    sys.exit(main())
