"""Summarize samply 0.13.1 processed JSON without counting off-CPU samples.

The .syms.json sidecar is optional; unresolved addresses remain explicit.
CPU deltas measure intervals ending at each sample, not sample multiplicity.
Witness-window boundary intervals are prorated by their wall-time overlap;
their CPU distribution within that interval is necessarily an estimate.
"""

import argparse
from collections import Counter
import gzip
import json
import math
from pathlib import Path


def read_json(path):
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt") as stream:
        return json.load(stream)


def debug_id(value):
    # DebugId::Display omits a zero appendix; BreakpadFormat always emits it.
    # Parse both into UUID + numeric appendix, preserving nonzero ages too.
    if "-" in value:
        parts = value.split("-")
        if len(parts) not in (5, 6) or [len(p) for p in parts[:5]] != [8, 4, 4, 4, 12]:
            raise ValueError(f"invalid UUID debug ID: {value}")
        uuid = "".join(parts[:5])
        age = parts[5] if len(parts) == 6 else "0"
    else:
        uuid, age = value[:32], value[32:]
    if len(uuid) != 32 or any(c not in "0123456789abcdefABCDEF" for c in uuid):
        raise ValueError(f"invalid UUID debug ID: {value}")
    return uuid.upper(), int(age, 16)


class Symbols:
    def __init__(self, profile, sidecar):
        self.libs = profile.get("libs", [])
        self.strings = sidecar.get("string_table", [])
        self.maps = {}
        for lib in sidecar.get("data", []):
            key = (lib["debug_name"], debug_id(lib["debug_id"]))
            self.maps[key] = (
                dict(lib["known_addresses"]), lib["symbol_table"]
            )

    def resolve(self, lib_index, address):
        lib = self.libs[lib_index]
        key = (lib["debugName"], debug_id(lib["breakpadId"]))
        addresses, symbols = self.maps.get(key, ({}, []))
        index = addresses.get(address)
        if index is None:
            return None
        return self.strings[symbols[index]["symbol"]]

    def frame(self, thread, frame):
        table = thread["frameTable"]
        func = table["func"][frame]
        resource = thread["funcTable"]["resource"][func]
        address = table["address"][frame]
        if resource is None or resource < 0 or address is None or address < 0:
            return None  # Process/thread labels are not native functions.
        lib_index = thread["resourceTable"]["lib"][resource]
        if lib_index is None or lib_index < 0:
            return None
        lib = self.libs[lib_index]
        library = lib.get("name", lib["debugName"])
        name = self.resolve(lib_index, address)
        if name is None:
            name = thread["stringArray"][thread["funcTable"]["name"][func]]
            if not name or name.startswith("0x"):
                return (f"{library}!0x{address:x}", True)
        return (f"{library}!{name}", False)


def sample_times(samples):
    if "time" in samples:
        return samples["time"]
    total = 0.0
    times = []
    for delta in samples["timeDeltas"]:
        total += delta
        times.append(total)
    return times


def top_rows(counts, total, limit):
    return [
        {"function": name, "cpu_ms": value / 1000,
         "percent_of_cpu": value / total * 100 if total else 0}
        for name, value in counts.most_common(limit)
    ]


def summarize(profile, symbols, window, target_pid, limit):
    self_cpu, inclusive_cpu = Counter(), Counter()
    threads = []
    total_cpu = unresolved_self = unresolved_stack = 0.0
    missing_stack = non_native_leaf = 0.0
    for thread in profile["threads"]:
        if target_pid is not None and str(thread["pid"]) != str(target_pid):
            continue
        samples = thread["samples"]
        deltas = samples.get("threadCPUDelta")
        if deltas is None:
            raise ValueError(f"thread {thread['name']} has no threadCPUDelta")
        if samples.get("weightType", "samples") != "samples":
            raise ValueError("expected samply time samples, not allocation/tracing data")
        times = sample_times(samples)
        stack_table = thread["stackTable"]
        cache = {}

        def frames_for(stack):
            if stack in cache:
                return cache[stack]
            original = stack
            frames = []
            while stack is not None:
                frame = symbols.frame(thread, stack_table["frame"][stack])
                if frame is not None:
                    frames.append(frame)
                stack = stack_table["prefix"][stack]
            cache[original] = frames
            return frames

        cpu = unresolved = 0.0
        positive = off_cpu_rows = off_cpu_weight = unknown = boundary = 0
        # First delta can include CPU accumulated before recording/attachment.
        for i in range(1, samples["length"]):
            start, end = times[i - 1], times[i]
            if window is None:
                fraction = 1.0
            else:
                overlap = min(end, window[1]) - max(start, window[0])
                if overlap <= 0 or end <= start:
                    continue
                fraction = overlap / (end - start)
            delta = deltas[i]
            if delta is None or not math.isfinite(delta) or delta < 0:
                unknown += 1
                continue
            if delta == 0:
                off_cpu_rows += 1
                weights = samples.get("weight")
                off_cpu_weight += weights[i] if weights is not None else 1
                continue
            positive += 1
            boundary += fraction < 1.0
            weight = delta * fraction  # Do NOT multiply by samples.weight.
            cpu += weight
            stack = samples["stack"][i]
            if stack is None:
                missing_stack += weight
                continue
            frames = frames_for(stack)
            if not frames:
                non_native_leaf += weight
                continue
            name, is_unresolved = frames[0]
            self_cpu[name] += weight
            if is_unresolved:
                unresolved_self += weight
                unresolved += weight
            if any(flag for _, flag in frames):
                unresolved_stack += weight
            # Recursive calls are inclusive only once per sampled function.
            for name in {name for name, _ in frames}:
                inclusive_cpu[name] += weight
        total_cpu += cpu
        threads.append({
            "pid": thread["pid"], "tid": thread["tid"], "name": thread["name"],
            "cpu_ms": cpu / 1000, "unresolved_self_cpu_ms": unresolved / 1000,
            "positive_cpu_rows": positive, "off_cpu_rows": off_cpu_rows,
            "off_cpu_sample_weight": off_cpu_weight, "unknown_cpu_rows": unknown,
            "prorated_boundary_rows": boundary,
        })
    if not threads:
        raise ValueError("no profile threads match the witness PID")
    return {
        "cpu_ms": total_cpu / 1000,
        "cpu_percent_one_core": (total_cpu / (window[1] - window[0]) / 10
                                 if window is not None else None),
        "unresolved_self_cpu_ms": unresolved_self / 1000,
        "stacks_with_unresolved_cpu_ms": unresolved_stack / 1000,
        "missing_stack_cpu_ms": missing_stack / 1000,
        "no_native_frame_cpu_ms": non_native_leaf / 1000,
        "threads": sorted(threads, key=lambda row: -row["cpu_ms"]),
        "hot_self": top_rows(self_cpu, total_cpu, limit),
        "hot_inclusive": top_rows(inclusive_cpu, total_cpu, limit),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", type=Path)
    parser.add_argument("--witness", type=Path, help="case/done.json navigation witness")
    parser.add_argument("--limit", type=int, default=25)
    args = parser.parse_args()
    profile = read_json(args.profile)
    units = profile["meta"].get("sampleUnits", {})
    if units.get("threadCPUDelta") != "µs" or units.get("time") != "ms":
        raise ValueError(f"expected samply microsecond CPU / millisecond time units: {units}")
    sidecar_path = args.profile.with_suffix(".syms.json")
    sidecar = read_json(sidecar_path) if sidecar_path.exists() else {}
    symbols = Symbols(profile, sidecar)
    result = {
        "profile": str(args.profile),
        "symbols": str(sidecar_path) if sidecar else None,
        "method": "positive CPU deltas once; first row excluded; native functions only",
        "all_recording": summarize(profile, symbols, None, None, args.limit),
    }
    if args.witness:
        witness = read_json(args.witness)
        origin = profile["meta"]["startTime"]
        window = (witness["start_wall"] * 1000 - origin,
                  witness["end_wall"] * 1000 - origin)
        if not all(math.isfinite(value) for value in window) or window[1] <= window[0]:
            raise ValueError("invalid navigation window")
        result["navigation_window"] = {
            "witness": str(args.witness), "pid": witness["pid"],
            "start_ms": window[0], "end_ms": window[1],
            "boundary_policy": "CPU interval prorated by wall-time overlap",
            **summarize(profile, symbols, window, witness["pid"], args.limit),
        }
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
