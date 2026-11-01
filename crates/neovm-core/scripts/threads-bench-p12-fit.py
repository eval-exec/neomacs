#!/usr/bin/env python3
"""P12 P9-PERF ROWS fit and report.

Reads the raw.tsv written by p12-run.sh; one tab-separated line per run:

    arm row rep pass slot event rc ins wall_ms loadavg result

(`result` is last because it contains spaces.)

The two fitted forms over N in {100, 300, 600, 1000}, medians per N:

    total:      I(N) = a + b*N + c*N^2
    per-thread: P(N) = alpha + beta*N + gamma*N^2,  P(N) = (I(N)-startup)/N

The per-thread form subtracts the startup row measured in the SAME
repetition: the startup row is a separate process and its count moves by
hundreds of millions between runs on loaded build cores, so a global
startup median would inject that drift into every P(N) point.

"Both orders" means separate fits for the increasing-order runs
(100,300,600,1000) and the decreasing-order runs (1000,600,300,100), plus
the combined fit.  The total-form `a` folds in the process/startup floor,
which is the sched-index DESIGN's convention; the per-thread form cancels
the startup row's median first.

Validity: every cell must have rc=0, a numeric instruction count, and one
and the same RESULT line per row across arms, reps and passes (the row
grader); otherwise the report says P12-FIT INVALID and exits 2.

--aa-raw PATH reads a baseline (A/A) raw.tsv and prints its band next to a
pair-mode A/B delta.  --selftest checks the fit math on an exact quadratic
and the validity gate on a crafted deviation.
"""

import argparse
import collections
import contextlib
import io
import math
import os
import re
import statistics
import sys
import tempfile

GROUPS = (100, 300, 600, 1000)  # canonical N-series points


def die(message):
    print("P12-FIT INVALID: " + message, file=sys.stderr)
    raise SystemExit(2)


def read_tsv(path):
    rows = []
    with open(path, "r", encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            line = line.rstrip("\n")
            if not line:
                continue
            if number == 1 and line.startswith("arm\trow\trep\t"):
                continue  # tolerate an optional header; raw.tsv has none
            parts = line.split("\t", 10)
            if len(parts) != 11:
                die("%s:%d: expected 11 tab-separated fields, got %d"
                    % (path, number, len(parts)))
            (arm, row, rep, pass_, slot, event, rc, ins, wall, load,
             result) = parts
            try:
                rep = int(rep)
                rc = int(rc)
                wall = int(wall)
                load = float(load)
                ins = None if ins == "NA" else int(ins)
            except ValueError:
                die("%s:%d: non-numeric field" % (path, number))
            rows.append(dict(arm=arm, row=row, rep=rep, pass_=pass_,
                             slot=slot, event=event, rc=rc, ins=ins,
                             wall_ms=wall, loadavg=load, result=result))
    if not rows:
        die("%s: no runs" % path)
    return rows


def read_meta(path):
    meta = {}
    if path and os.path.exists(path):
        with open(path, "r", encoding="utf-8") as handle:
            for line in handle:
                line = line.rstrip("\n")
                if "=" in line:
                    key, value = line.split("=", 1)
                    meta[key] = value
    return meta


def is_n_row(row):
    return row.startswith("many_threads_") and row[len("many_threads_"):].isdigit()


def n_of(row):
    return int(row[len("many_threads_"):])


def cell_rows(rows, event=None, slot=None, arm=None):
    out = []
    for row in rows:
        if event is not None and row["event"] != event:
            continue
        if slot is not None and row["slot"] != slot:
            continue
        if arm is not None and row["arm"] != arm:
            continue
        out.append(row)
    return out


def validate(rows, meta):
    """Exit 2 unless every measured cell is rc=0 with a usable count and the
    row's RESULT line is identical across arms, reps and passes."""
    bad = []
    for row in rows:
        if row["rc"] != 0:
            bad.append("rc=%d in %s" % (row["rc"], run_id(row)))
        elif row["ins"] is None:
            bad.append("missing instruction count in %s" % run_id(row))
    if bad:
        die("failed runs: " + "; ".join(bad[:8]))
    # One RESULT per row, across arms/reps/passes (the rows are
    # route- and order-independent within an invocation).
    results = collections.defaultdict(set)
    for row in rows:
        results[row["row"]].add(row["result"])
    for name, values in sorted(results.items()):
        if len(values) != 1:
            die("RESULT deviates for row %s: %s"
                % (name, sorted(values)[:4]))
        value = next(iter(values))
        if not value.startswith("("):
            die("row %s printed no RESULT line" % name)
        if name == "intern_fresh_10k":
            shape = (r"^\(intern-fresh seed=20261031 ok=10000 fresh-pre=nil "
                     r"sample=p12-intern-04999-[0-9a-f]{8} eq=t name-ok=t\)$")
            if not re.fullmatch(shape, value):
                die("intern row RESULT shape: %s" % value)
        elif is_n_row(name):
            threads = meta.get("threads", "")
            joined = "%d" % n_of(name)
            if threads == "sim":
                if "joined=0" not in value:
                    die("sim row %s must report joined=0: %s" % (name, value))
            elif threads == "green":
                if ("joined=%s" % joined) not in value:
                    die("green row %s must report joined=%s: %s"
                        % (name, joined, value))
            elif "joined=0" not in value and ("joined=%s" % joined) not in value:
                die("row %s did not join its workers: %s" % (name, value))


def run_id(row):
    return "%s/%s rep=%d pass=%s slot=%s %s" % (
        row["arm"], row["row"], row["rep"], row["pass_"], row["slot"],
        row["event"])


def median(values):
    return statistics.median(values)


def stdev(values):
    return statistics.stdev(values) if len(values) > 1 else 0.0


def solve(matrix, rhs):
    """Gaussian elimination with partial pivoting; None if singular."""
    size = len(matrix)
    work = [list(line) + [value] for line, value in zip(matrix, rhs)]
    for column in range(size):
        pivot = max(range(column, size), key=lambda r: abs(work[r][column]))
        if abs(work[pivot][column]) < 1e-12:
            return None
        work[column], work[pivot] = work[pivot], work[column]
        divisor = work[column][column]
        for j in range(column, size + 1):
            work[column][j] /= divisor
        for r in range(size):
            if r == column:
                continue
            factor = work[r][column]
            for j in range(column, size + 1):
                work[r][j] -= factor * work[column][j]
    return [work[i][size] for i in range(size)]


def polyfit(xs, ys, degree=2):
    """Least squares polynomial coefficients in ascending order.

    Returns (coefficients, r2, ss_res, n) or None when underdetermined or
    singular (e.g. fewer distinct x than degree+1)."""
    n = len(xs)
    if n <= degree or len(set(xs)) <= degree:
        return None
    size = degree + 1
    matrix = [[sum(x ** (i + j) for x in xs) for j in range(size)]
              for i in range(size)]
    rhs = [sum((x ** i) * y for x, y in zip(xs, ys)) for i in range(size)]
    coefficients = solve(matrix, rhs)
    if coefficients is None:
        return None
    mean = sum(ys) / n
    ss_res = sum((y - sum(c * x ** i for i, c in enumerate(coefficients))) ** 2
                 for x, y in zip(xs, ys))
    ss_tot = sum((y - mean) ** 2 for y in ys)
    r2 = float("nan") if ss_tot == 0 else 1.0 - ss_res / ss_tot
    return coefficients, r2, ss_res, n


def fnum(value):
    if value is None:
        return "NA"
    if abs(value) >= 1e6:
        return "%.0f" % value
    if value == int(value):
        return "%d" % value
    return "%.4f" % value


def startup_medians(rows, event):
    out = {}
    for arm in sorted({row["arm"] for row in rows}):
        values = [row["ins"] for row in rows
                  if row["row"] == "startup" and row["event"] == event
                  and row["arm"] == arm]
        if values:
            out[arm] = float(median(values))
    return out


def startup_by_rep(rows, event, arm):
    """The startup row is a fresh process; its instruction count moves
    hundreds of millions between runs on loaded build cores, so the net
    price subtracts the startup measured in the SAME repetition."""
    out = {}
    for row in rows:
        if (row["row"] == "startup" and row["event"] == event
                and row["arm"] == arm):
            out[row["rep"]] = row["ins"]
    return out


def startup_fallback(by_rep):
    return median(list(by_rep.values())) if by_rep else None


def band_baseline(rows, event, row_name):
    """A/A band for the one-binary protocol: for each rep the two slots
    (inc/dec for the N rows, s1/s2 for the intern row) are the pair."""
    per_rep = collections.defaultdict(dict)
    for row in rows:
        if row["row"] != row_name or row["event"] != event:
            continue
        per_rep[row["rep"]][row["slot"]] = row["ins"]
    rels = []
    deltas = []
    for rep, slots in per_rep.items():
        values = list(slots.values())
        if len(values) < 2:
            continue
        a, b = values[0], values[1]
        deltas.append(b - a)
        if min(a, b) > 0:
            rels.append(abs(b - a) / min(a, b) * 100.0)
    if not rels:
        return None
    return dict(n=len(rels), median_rel=median(rels), max_rel=max(rels),
                median_delta=median(deltas), max_abs=max(abs(d) for d in deltas))


def band_pair(rows, event, row_name):
    """Paired A/B deltas: for each rep and slot, B minus A (adjacent runs of
    the two-binary layout)."""
    by = collections.defaultdict(dict)
    for row in rows:
        if row["row"] != row_name or row["event"] != event:
            continue
        by[(row["rep"], row["slot"])][row["arm"]] = row["ins"]
    deltas = []
    rels = []
    for (_rep, _slot), arms in by.items():
        if "A" not in arms or "B" not in arms:
            continue
        a, b = arms["A"], arms["B"]
        deltas.append(b - a)
        if min(a, b) > 0:
            rels.append(abs(b - a) / min(a, b) * 100.0)
    if not deltas:
        return None
    return dict(n=len(deltas), median_delta=median(deltas),
                stdev_delta=stdev(deltas),
                median_rel=median(rels) if rels else float("nan"),
                max_rel=max(rels) if rels else float("nan"),
                positive=sum(1 for d in deltas if d > 0))


def report_row_a(rows, meta, aa_rows):
    print("\n## Row A -- intern 10,000 fresh names (P9.2 producer-cost gate)")
    events = sorted({row["event"] for row in rows})
    arms = sorted({row["arm"] for row in rows})
    for event in events:
        print("\n### event %s" % event)
        for arm in arms:
            samples = [(row["rep"], row["ins"]) for row in rows
                       if row["row"] == "intern_fresh_10k"
                       and row["event"] == event and row["arm"] == arm]
            if not samples:
                continue
            values = [ins for _rep, ins in samples]
            raw = median(values)
            by_rep = startup_by_rep(rows, event, arm)
            fallback = startup_fallback(by_rep)
            nets = [ins - by_rep.get(rep, fallback) for rep, ins in samples] \
                if fallback is not None else []
            net = median(nets) if nets else float("nan")
            print("arm %s: raw med=%s min=%s max=%s n=%d | startup med=%s | "
                  "net med (per-rep startup)=%s"
                  % (arm, fnum(raw), fnum(min(values)), fnum(max(values)),
                     len(values), fnum(fallback), fnum(net)))
        if "B" not in arms:
            band = band_baseline(rows, event, "intern_fresh_10k")
            if band:
                print("A/A band (per-rep pair): median rel=%.4f%% max rel=%.4f%% "
                      "median delta=%s max|delta|=%s n=%d"
                      % (band["median_rel"], band["max_rel"],
                         fnum(band["median_delta"]), fnum(band["max_abs"]),
                         band["n"]))
        delta = band_pair(rows, event, "intern_fresh_10k")
        if delta:
            print("A/B paired delta: median=%s stdev=%s (rel median=%.4f%% "
                  "max=%.4f%%) positive=%d/%d"
                  % (fnum(delta["median_delta"]), fnum(delta["stdev_delta"]),
                     delta["median_rel"], delta["max_rel"],
                     delta["positive"], delta["n"]))
        if "B" in arms:
            starts = {arm: startup_by_rep(rows, event, arm) for arm in ("A", "B")}
            falls = {arm: startup_fallback(starts[arm]) for arm in starts}
            by = collections.defaultdict(dict)
            for row in rows:
                if row["row"] == "intern_fresh_10k" and row["event"] == event:
                    by[(row["rep"], row["slot"])][row["arm"]] = row["ins"]
            net_deltas = []
            for (rep, _slot), arms_seen in by.items():
                if "A" not in arms_seen or "B" not in arms_seen:
                    continue
                if falls["A"] is None or falls["B"] is None:
                    continue
                a_net = arms_seen["A"] - starts["A"].get(rep, falls["A"])
                b_net = arms_seen["B"] - starts["B"].get(rep, falls["B"])
                net_deltas.append(b_net - a_net)
            if net_deltas:
                print("A/B net delta (per-rep startup cancelled): median=%s "
                      "stdev=%s positive=%d/%d"
                      % (fnum(median(net_deltas)), fnum(stdev(net_deltas)),
                         sum(1 for v in net_deltas if v > 0), len(net_deltas)))
        if aa_rows:
            aa_band = band_baseline(aa_rows, event, "intern_fresh_10k")
            if aa_band:
                print("A/A band from --aa-raw: median rel=%.4f%% max rel=%.4f%%"
                      % (aa_band["median_rel"], aa_band["max_rel"]))


def samples_by_n(rows, event, arm, slot=None):
    """N -> [(rep, ins), ...]."""
    out = collections.defaultdict(list)
    for row in rows:
        if (row["event"] == event and row["arm"] == arm
                and is_n_row(row["row"])
                and (slot is None or row["slot"] == slot)):
            out[n_of(row["row"])].append((row["rep"], row["ins"]))
    return out


def fit_block(label, xs, ys):
    fit = polyfit(xs, ys, 2)
    if fit is None:
        print("  %s: fit skipped (need >=3 distinct N)" % label)
        return None
    coefficients, r2, _ss_res, n = fit
    print("  %s: %s + %s*N + %s*N^2   R2=%s  n=%d"
          % (label, fnum(coefficients[0]), fnum(coefficients[1]),
             fnum(coefficients[2]), ("%.6f" % r2) if not math.isnan(r2) else "NA",
             n))
    return coefficients


def report_n(rows, meta, aa_rows):
    print("\n## N-series -- many_threads_100/300/600/1000")
    events = sorted({row["event"] for row in rows})
    arms = sorted({row["arm"] for row in rows})
    ns_present = sorted({n_of(row["row"]) for row in rows if is_n_row(row["row"])})
    print("N points present: %s" % ns_present)
    fits = {}
    for event in events:
        for arm in arms:
            print("\n### event %s, arm %s" % (event, arm))
            inc = samples_by_n(rows, event, arm, "inc")
            dec = samples_by_n(rows, event, arm, "dec")
            both = samples_by_n(rows, event, arm)
            by_rep = startup_by_rep(rows, event, arm)
            fallback = startup_fallback(by_rep)
            print("N\tmed_inc\tmed_dec\tmed_all\tnet_all\tper_thread")
            for n in ns_present:
                allv = both.get(n, [])
                if not allv:
                    continue
                med_all = median([ins for _rep, ins in allv])
                if fallback is not None:
                    nets = [ins - by_rep.get(rep, fallback) for rep, ins in allv]
                    net = median(nets)
                    price = net / n
                else:
                    net = price = float("nan")
                print("%d\t%s\t%s\t%s\t%s\t%s"
                      % (n,
                         fnum(median([ins for _rep, ins in inc[n]]))
                         if inc.get(n) else "NA",
                         fnum(median([ins for _rep, ins in dec[n]]))
                         if dec.get(n) else "NA",
                         fnum(med_all), fnum(net), fnum(price)))
            subsets = [("order=inc", inc), ("order=dec", dec),
                       ("order=all", both)]
            for label, subset in subsets:
                xs = [float(n) for n in sorted(subset)]
                ys = [median([ins for _rep, ins in subset[n]])
                      for n in sorted(subset)]
                coefficients = fit_block("I(N) " + label, xs, ys)
                fits[(event, arm, label, "total")] = coefficients
                if fallback is not None:
                    # Per-rep net price: (row process - that rep's startup
                    # process) / N, so rep-level startup drift cancels.
                    ys_price = []
                    for n in sorted(subset):
                        prices = [(ins - by_rep.get(rep, fallback)) / n
                                  for rep, ins in subset[n]]
                        ys_price.append(median(prices))
                    coefficients = fit_block("P(N) " + label, xs, ys_price)
                    fits[(event, arm, label, "price")] = coefficients
            # Per-rep paired marginal, cired2 nscale.sh style: mean of the
            # two order passes per N within a rep, then
            # (mean(N)-mean(N0))/(N-N0).
            per_rep = collections.defaultdict(dict)
            for row in rows:
                if (row["event"] == event and row["arm"] == arm
                        and is_n_row(row["row"])):
                    per_rep[row["rep"]].setdefault(n_of(row["row"]), []).append(
                        row["ins"])
            if ns_present:
                n0 = ns_present[0]
                for n in ns_present[1:]:
                    values = []
                    for rep, by_n in per_rep.items():
                        if n0 not in by_n or n not in by_n:
                            continue
                        values.append((statistics.mean(by_n[n])
                                       - statistics.mean(by_n[n0])) / (n - n0))
                    if values:
                        print("per-rep paired marginal N=%d: median=%s "
                              "mean=%s stdev=%s n=%d"
                              % (n, fnum(median(values)),
                                 fnum(statistics.mean(values)),
                                 fnum(stdev(values)), len(values)))
        if "B" not in arms:
            # Per-N A/A band (baseline): inc vs dec within the one arm, per rep.
            per_rep = collections.defaultdict(dict)
            for row in rows:
                if row["event"] == event and is_n_row(row["row"]):
                    per_rep[(row["rep"], n_of(row["row"]))][row["slot"]] = row["ins"]
            rels = collections.defaultdict(list)
            for (_rep, n), slots in per_rep.items():
                if "inc" in slots and "dec" in slots:
                    a, b = slots["inc"], slots["dec"]
                    if min(a, b) > 0:
                        rels[n].append(abs(b - a) / min(a, b) * 100.0)
            if rels:
                print("\nA/A band per N (inc vs dec, per rep):")
                for n in sorted(rels):
                    print("  N=%d median=%.4f%% max=%.4f%% n=%d"
                          % (n, median(rels[n]), max(rels[n]), len(rels[n])))
        if "B" in arms:
            print("\nPaired A/B delta per N (B-A, same slot/rep):")
            by = collections.defaultdict(dict)
            for row in rows:
                if row["event"] == event and is_n_row(row["row"]):
                    by[(row["rep"], n_of(row["row"]), row["slot"])][row["arm"]] = row["ins"]
            starts = {arm: startup_by_rep(rows, event, arm) for arm in ("A", "B")}
            falls = {arm: startup_fallback(starts[arm]) for arm in starts}
            grouped = collections.defaultdict(list)
            net_grouped = collections.defaultdict(list)
            for (rep, n, slot), arms_seen in by.items():
                if "A" not in arms_seen or "B" not in arms_seen:
                    continue
                grouped[n].append(arms_seen["B"] - arms_seen["A"])
                if falls["A"] is not None and falls["B"] is not None:
                    a_net = arms_seen["A"] - starts["A"].get(rep, falls["A"])
                    b_net = arms_seen["B"] - starts["B"].get(rep, falls["B"])
                    net_grouped[n].append(b_net - a_net)
            for n in sorted(grouped):
                values = grouped[n]
                print("  N=%d median delta=%s stdev=%s positive=%d/%d"
                      % (n, fnum(median(values)), fnum(stdev(values)),
                         sum(1 for v in values if v > 0), len(values)))
            for n in sorted(net_grouped):
                values = net_grouped[n]
                print("  N=%d net delta (per-rep startup cancelled): median=%s "
                      "stdev=%s positive=%d/%d"
                      % (n, fnum(median(values)), fnum(stdev(values)),
                         sum(1 for v in values if v > 0), len(values)))
            print("\nFit coefficient delta (B-A; per-thread marginal = "
                  "b + c*(N+N0)):")
            n0 = ns_present[0] if ns_present else 100
            for label in ("order=inc", "order=dec", "order=all"):
                total_a = fits.get((event, "A", label, "total"))
                total_b = fits.get((event, "B", label, "total"))
                if total_a and total_b:
                    db = total_b[1] - total_a[1]
                    dc = total_b[2] - total_a[2]
                    print("  %s: delta b=%s delta c=%s | predicted "
                          "per-thread marginal delta N=%d: %s, N=%d: %s"
                          % (label, fnum(db), fnum(dc),
                             n0, fnum(db + dc * (n0 + n0)),
                             ns_present[-1], fnum(db + dc * (ns_present[-1] + n0))))
        if aa_rows:
            aa_rels = collections.defaultdict(list)
            for row in aa_rows:
                if row["event"] == event and is_n_row(row["row"]):
                    aa_rels[(row["rep"], n_of(row["row"]))].setdefault(
                        row["slot"], row["ins"])
            per_n = collections.defaultdict(list)
            for (_rep, n), slots in aa_rels.items():
                if "inc" in slots and "dec" in slots:
                    a, b = slots["inc"], slots["dec"]
                    if min(a, b) > 0:
                        per_n[n].append(abs(b - a) / min(a, b) * 100.0)
            if per_n:
                print("\nA/A band from --aa-raw per N:")
                for n in sorted(per_n):
                    print("  N=%d median=%.4f%% max=%.4f%% n=%d"
                          % (n, median(per_n[n]), max(per_n[n]),
                             len(per_n[n])))


def report(rows, meta, aa_rows_path):
    validate(rows, meta)
    aa_rows = read_tsv(aa_rows_path) if aa_rows_path else []
    print("P12 P9-PERF ROWS -- fit report")
    for key in ("tag", "threads", "gen", "reps", "events", "cpus", "lock",
                "bin_a", "bin_a_head", "bin_b", "bin_b_head"):
        if key in meta:
            print("meta.%s=%s" % (key, meta[key]))
    runs = len(rows)
    failures = sum(1 for row in rows if row["rc"] != 0)
    noins = sum(1 for row in rows if row["ins"] is None)
    print("validity: runs=%d rc-fail=%d missing-ins=%d results=consistent"
          % (runs, failures, noins))
    arms = sorted({row["arm"] for row in rows})
    print("arms present: %s" % ",".join(arms))
    if "intern_fresh_10k" in {row["row"] for row in rows}:
        report_row_a(rows, meta, aa_rows)
    if any(is_n_row(row["row"]) for row in rows):
        report_n(rows, meta, aa_rows)
    print("\nP12-FIT OK")


def selftest():
    ns = [float(n) for n in GROUPS]
    a, b, c = 1_000_000.0, 2000.0, 0.5
    totals = [a + b * n + c * n * n for n in ns]
    fit = polyfit(ns, totals, 2)
    assert fit is not None, "exact quadratic must fit"
    coefficients, r2, _ss, _n = fit
    for got, want in zip(coefficients, (a, b, c)):
        assert abs(got - want) <= 1e-6 * max(1.0, abs(want)), (got, want)
    assert abs(r2 - 1.0) < 1e-9, r2
    startup = a
    prices = [(total - startup) / n for total, n in zip(totals, ns)]
    price_fit = polyfit(ns, prices, 2)
    assert price_fit is not None, "per-thread form must fit"
    # P(N) = alpha + beta*N + gamma*N^2 with P = b + c*N here: the constant
    # per-thread term alpha is the total form's b, beta its c, gamma ~ 0.
    alpha, beta, gamma = price_fit[0]
    assert abs(alpha - b) <= 1e-6 * max(1.0, abs(b)), alpha
    assert abs(beta - c) <= 1e-6 * max(1.0, abs(c)), beta
    assert abs(gamma) <= 1e-6 * max(1.0, abs(c)), gamma
    assert abs(price_fit[1] - 1.0) < 1e-9, price_fit[1]
    # Fewer than three distinct N must be refused rather than guessed.
    assert polyfit([100.0, 200.0], [1.0, 2.0], 2) is None
    # Validity gate: a deviating RESULT line is INVALID (exit 2).
    header = "arm\trow\trep\tpass\tslot\tevent\trc\tins\twall_ms\tloadavg\tresult\n"
    good = (header
            + "A\tintern_fresh_10k\t1\t1\ts1\tinstructions:u\t0\t100\t1\t0.1\t"
              "(intern-fresh seed=20261031 ok=10000 fresh-pre=nil "
              "sample=p12-intern-04999-00000000 eq=t name-ok=t)\n")
    bad = good + ("A\tintern_fresh_10k\t1\t2\ts2\tinstructions:u\t0\t100\t1\t"
                  "0.1\t(intern-fresh seed=20261031 ok=9999 fresh-pre=nil "
                  "sample=p12-intern-04999-00000000 eq=t name-ok=t)\n")
    with tempfile.NamedTemporaryFile("w", suffix=".tsv", delete=False) as handle:
        handle.write(good)
        good_path = handle.name
    with tempfile.NamedTemporaryFile("w", suffix=".tsv", delete=False) as handle:
        handle.write(bad)
        bad_path = handle.name
    try:
        assert len(read_tsv(good_path)) == 1
        try:
            with contextlib.redirect_stderr(io.StringIO()):
                validate(read_tsv(bad_path), {"threads": "green"})
        except SystemExit as exit_error:
            assert exit_error.code == 2, exit_error.code
        else:
            raise AssertionError("deviating RESULT must be INVALID")
    finally:
        os.unlink(good_path)
        os.unlink(bad_path)
    print("P12-FIT SELFTEST PASS (exact quadratic, per-thread form, "
          "underdetermined refusal, RESULT-deviation rejection)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("raw", nargs="?", help="raw.tsv from p12-run.sh")
    parser.add_argument("--meta", help="meta.env (default: sibling of raw)")
    parser.add_argument("--aa-raw", help="baseline raw.tsv for the A/A band")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        selftest()
        return 0
    if not args.raw:
        parser.error("raw.tsv is required (or --selftest)")
    meta_path = args.meta or os.path.join(os.path.dirname(args.raw), "meta.env")
    meta = read_meta(meta_path)
    report(read_tsv(args.raw), meta, args.aa_raw)
    return 0


if __name__ == "__main__":
    sys.exit(main())
