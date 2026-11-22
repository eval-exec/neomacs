"""Isolated paired GUI measurements; run after fresh-build under remote-run.

Timer reproduces issue607. Synthetic-cn feeds event14 to the normal command
reader; it deliberately makes no claim about native keyboard repeat costs.
Synthetic-cv feeds C-u 1 C-v through that reader and scrolls the viewport on
every command, covering the full-export cost alongside cursor-only motion.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument("--gnu-prefix", required=True, type=Path)
parser.add_argument("--source-sha", required=True)
parser.add_argument("--samples", type=int, default=3)
parser.add_argument("--warmup", type=float, default=20)
parser.add_argument("--modes", nargs="+", default=["timer", "synthetic-cn", "native-cn"])
parser.add_argument("--neo-binary", type=Path,
                    help="Explicit OLD or NEW binary; recorded with version and SHA256")
parser.add_argument("--count-exports", action="store_true",
                    help="Separate instrumented cost row; never use its CPU/RSS as the clean timing row")
parser.add_argument("--coverage-reuse", choices=["off", "on"], default="on",
                    help="Same-binary old-exporter control versus certified renewal")
args = parser.parse_args()
root = Path.cwd()
source = root / "scripts/macos-navigation-parity"
artifacts = root / "tmp" / f"macos-navigation-{os.getpid()}"
artifacts.mkdir(parents=True)


def run(argv, **kwargs):
    return subprocess.run(argv, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, **kwargs)


attestation = run(["scripts/ci/check-gnu-oracle.sh", str(args.gnu_prefix / "bin/emacs"),
                   "31.1", str(args.gnu_prefix / "share/gnu-emacs-oracle/build-manifest"),
                   "a360712c9d272d950d8d8255ef74570f7e90b7d9"])
(artifacts / "oracle.txt").write_text(attestation.stdout)
print("ATTESTATION", attestation.returncode, attestation.stdout, flush=True)
attestation.check_returncode()
for compiler, filename, binary in [("clang", "process_usage.c", "usage"),
                                    ("swiftc", "desktop.swift", "desktop")]:
    flags = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O2"] if compiler == "clang" else []
    subprocess.run(["xcrun", compiler, *flags, str(source / filename), "-o", str(artifacts / binary)],
                   check=True)
desktop = json.loads(run([str(artifacts / "desktop")], check=True).stdout)
print("DESKTOP", json.dumps(desktop), flush=True)
if not desktop["desktop"]:
    raise RuntimeError("No native GUI desktop")
fixture = artifacts / "navigation.md"
fixture.write_text("".join(f"Line {i:03d}: cursor navigation test.\n" for i in range(260)))
assert len(fixture.read_bytes()) < 10_000
metadata = {"schema": "macos-navigation-v1", "warmup_seconds": args.warmup,
            "cost_row": "p607-navigation-export-calls" if args.count_exports else "p607-navigation-cpu-rss",
            "coverage_export_reuse": args.coverage_reuse,
            "production_source_sha": args.source_sha,
            "controller_host_policy": os.environ.get("NEOMACS_BENCH_CONTROLLER_POLICY", "not recorded"),
            "moves": 260, "interval_seconds": .1, "desktop": desktop,
            "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
            "fixture_bytes": fixture.stat().st_size,
            "native_keyboard": ("available; native-cn requested separately" if desktop["post_events"]
                                else "UNATTESTED: post_events permission blocked"),
            "host": {name: run(argv).stdout for name, argv in [
                ("os", ["sw_vers"]), ("hardware", ["sysctl", "hw.model", "hw.memsize", "hw.logicalcpu"]),
                ("profiler", ["sh", "-c", "command -v samply; command -v sample; xcrun --find xctrace"])]}}
(artifacts / "metadata.json").write_text(json.dumps(metadata, indent=2))


def usage(pid):
    return json.loads(run([str(artifacts / "usage"), str(pid)], check=True).stdout)


def stop(child):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=4)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def wait_file(path, child, timeout):
    deadline = time.monotonic() + timeout
    while not path.exists():
        if child.poll() is not None or time.monotonic() > deadline:
            raise RuntimeError(f"missing {path.name}; child status={child.poll()}")
        time.sleep(.02)
    return json.loads(path.read_text())


editors = {"gnu": args.gnu_prefix / "bin/emacs",
           "neo": args.neo_binary or Path(os.environ["CARGO_TARGET_DIR"]) / "release/neomacs"}


def file_sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


metadata["editors"] = {name: {"path": str(binary),
                              "version": run([str(binary), "--version"], check=True).stdout,
                              "sha256": file_sha256(binary)}
                       for name, binary in editors.items()}
if args.source_sha[:12] not in metadata["editors"]["neo"]["version"]:
    raise RuntimeError("Neo --version does not match --source-sha")
metadata["harness_sha256"] = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                              for path in source.iterdir() if path.is_file()}
(artifacts / "metadata.json").write_text(json.dumps(metadata, indent=2))
rows = []
for mode in args.modes:
    if mode not in ["timer", "synthetic-cn", "synthetic-cv", "native-cn"]:
        raise ValueError(mode)
    case_fixture = fixture
    if mode == "synthetic-cv":
        case_fixture = artifacts / "navigation-scroll.md"
        case_fixture.write_text("".join(f"Line {i:03d}: cursor navigation test.\n" for i in range(520)))
    if mode == "native-cn" and not desktop["post_events"]:
        print("UNATTESTED native-cn: post_events permission blocked", flush=True)
        continue
    for sample_index in range(args.samples):
        order = ["gnu", "neo"] if sample_index % 2 == 0 else ["neo", "gnu"]
        for editor in order:
            case = artifacts / f"{mode}-{sample_index}-{editor}"
            case.mkdir()
            env = dict(os.environ, NEOMACS_RUNTIME_ROOT=str(root), RUST_LOG="error",
                       NEOMACS_COVERAGE_EXPORT_REUSE=args.coverage_reuse,
                       NAVIGATION_BENCH_DIR=str(case), NAVIGATION_BENCH_MODE=mode,
                       NAVIGATION_BENCH_FILE=str(case_fixture))
            if args.count_exports:
                env["RUST_LOG"] = "error,neomacs_layout_engine::coverage_cost=trace"
            env.pop("DISPLAY", None)
            env.pop("WAYLAND_DISPLAY", None)
            argv = [str(editors[editor]), "-Q", "--font=Menlo-13", "--geometry=100x35",
                    "-l", str(source / "init.el")]
            (case / "command.json").write_text(json.dumps(argv))
            with (case / "editor.log").open("w") as log:
                child = subprocess.Popen(argv, env=env, stdout=log, stderr=log)
                stimulus = None
                try:
                    ready = wait_file(case / "ready.json", child, 40)
                    assert ready["pid"] == child.pid and ready["first_line"] == 1
                    assert ready["last_line"] == (521 if mode == "synthetic-cv" else 261)
                    run([str(artifacts / "desktop"), str(child.pid)], check=True)
                    time.sleep(args.warmup)
                    first = usage(child.pid)
                    trace_start = (case / "editor.log").stat().st_size
                    (case / "go").write_text("go\n")
                    wait_file(case / "running.json", child, 10)
                    if mode == "native-cn":
                        stimulus = subprocess.Popen([str(artifacts / "desktop"), str(child.pid), "repeat"],
                                                    stdout=log, stderr=log)
                    samples = [first]
                    deadline = time.monotonic() + 90
                    while not (case / "done.json").exists():
                        if time.monotonic() > deadline or child.poll() is not None:
                            raise RuntimeError("navigation did not complete")
                        time.sleep(.25)
                        samples.append(usage(child.pid))
                    done = json.loads((case / "done.json").read_text())
                    trace_end = (case / "editor.log").stat().st_size
                    if stimulus is not None:
                        assert stimulus.wait(timeout=5) == 0
                    assert done["moves"] == 260, done
                    if mode == "synthetic-cv":
                        assert done["queued"] == 260, done
                        assert done["window_start_line"] == 261, done
                    else:
                        assert done["final_line"] == 261, done
                    last = samples[-1]
                    assert first["proc_start_mach_ticks"] == last["proc_start_mach_ticks"]
                    cpu_ticks = sum(last[key] - first[key] for key in
                                    ["user_time_mach_ticks", "system_time_mach_ticks"])
                    wall = (last["sample_monotonic_ns"] - first["sample_monotonic_ns"]) / 1e9
                    cpu = cpu_ticks * first["timebase_numer"] / first["timebase_denom"] / 1e9
                    # Independent Lisp clock() / CLOCK_PROCESS_CPUTIME_ID witness
                    # catches a mistaken Mach timebase conversion immediately.
                    assert .7 < cpu / done["cpu_seconds"] < 1.3, (cpu, done)
                    row = {"editor": editor, "mode": mode, "sample": sample_index,
                           "fixture_sha256": hashlib.sha256(case_fixture.read_bytes()).hexdigest(),
                           "fixture_bytes": case_fixture.stat().st_size,
                           "ready": ready, "witness": done, "cpu_percent": cpu / wall * 100,
                           "wall_seconds": wall, "cpu_seconds": cpu,
                           "instructions": last["instructions"] - first["instructions"],
                           "cycles": last["cycles"] - first["cycles"],
                           "rss_mean_mib": statistics.mean(s["resident_bytes"] for s in samples) / 2**20,
                           "rss_peak_mib": max(s["resident_bytes"] for s in samples) / 2**20,
                           "footprint_mean_mib": statistics.mean(s["phys_footprint_bytes"] for s in samples) / 2**20,
                           "footprint_peak_mib": max(s["phys_footprint_bytes"] for s in samples) / 2**20}
                    if args.count_exports:
                        # Count only the witnessed navigation interval, excluding
                        # startup/warmup. The event is at the real export_window
                        # entry, before its eligibility scans or geometry work.
                        with (case / "editor.log").open("rb") as stream:
                            stream.seek(trace_start)
                            trace = stream.read(trace_end - trace_start).decode(errors="replace")
                        counts = {}
                        for line in trace.splitlines():
                            if "building scroll coverage" not in line:
                                continue
                            fields = dict(re.findall(r"\b(window|start|hscroll|vscroll)=(-?\d+)", line))
                            if len(fields) != 4:
                                raise RuntimeError(f"unparsed export cost event: {line}")
                            key = tuple(int(fields[field]) for field in ("window", "start", "hscroll", "vscroll"))
                            counts[key] = counts.get(key, 0) + 1
                        row["coverage_export_calls"] = (sum(counts.values()) if editor == "neo" else None)
                        row["coverage_export_calls_by_key"] = [
                            dict(zip(("window", "start", "hscroll", "vscroll"), key), calls=count)
                            for key, count in sorted(counts.items())]
                        row["timing_grade"] = "INSTRUMENTED; use the clean p607-navigation-cpu-rss row"
                    (case / "usage.json").write_text(json.dumps(samples, indent=2))
                    rows.append(row)
                    (artifacts / "results.json").write_text(json.dumps(rows, indent=2))
                    print("RESULT", json.dumps(row), flush=True)
                finally:
                    if stimulus is not None:
                        stop(stimulus)
                    stop(child)
print("ARTIFACTS", artifacts, flush=True)
