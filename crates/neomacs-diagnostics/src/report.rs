//! Turn Brendan-Gregg folded stacks into a ranked, token-bounded JSON report.
//!
//! This is the AI-agent-facing projection of a Lisp CPU capture: instead of a
//! flamegraph image or a raw multi-thousand-line folded blob, an agent gets the
//! top-N functions with self/total sample counts and percentages — the same
//! `self` vs `total` (cumulative) vocabulary every Lisp profiler uses.

use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// One ranked function in a CPU report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hotspot {
    pub function: String,
    /// Samples where this function was the leaf (executing directly).
    pub self_samples: u64,
    /// Samples where this function appeared anywhere on the stack.
    pub total_samples: u64,
    pub self_pct: f64,
    pub total_pct: f64,
}

/// A ranked CPU report derived from folded stacks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CpuReport {
    /// Total samples across all stacks (the denominator for percentages).
    pub total_samples: u64,
    /// Number of distinct collapsed stacks in the capture.
    pub distinct_stacks: usize,
    /// Top-N functions, ranked (self or total per `sort_by_self`).
    pub top: Vec<Hotspot>,
}

/// Build a ranked report from folded stacks.
///
/// `top_n` caps the returned hotspots (keeps the response token-bounded).
/// `sort_by_self` ranks by self (leaf) time; otherwise by total (cumulative).
pub fn cpu_report_from_folded(folded: &str, top_n: usize, sort_by_self: bool) -> CpuReport {
    let mut self_counts: HashMap<&str, u64> = HashMap::new();
    let mut total_counts: HashMap<&str, u64> = HashMap::new();
    let mut grand_total: u64 = 0;
    let mut distinct_stacks: usize = 0;

    for line in folded.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Each folded line is `frameA;frameB;frameC <count>`.
        let Some((stack, count_str)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(count) = count_str.trim().parse::<u64>() else {
            continue;
        };
        let frames: Vec<&str> = stack.split(';').filter(|f| !f.is_empty()).collect();
        if frames.is_empty() {
            continue;
        }
        distinct_stacks += 1;
        grand_total += count;

        // Total: each distinct frame in the stack accrues `count` once (a
        // recursive frame appearing twice must not be double-counted).
        let mut seen: HashSet<&str> = HashSet::new();
        for frame in &frames {
            if seen.insert(frame) {
                *total_counts.entry(frame).or_default() += count;
            }
        }
        // Self: only the leaf frame was executing directly.
        let leaf = frames[frames.len() - 1];
        *self_counts.entry(leaf).or_default() += count;
    }

    // Every self key is also a total key (a leaf is on its own stack), so
    // ranging over total_counts covers all functions.
    let mut hotspots: Vec<Hotspot> = total_counts
        .iter()
        .map(|(func, &total)| {
            let self_samples = self_counts.get(func).copied().unwrap_or(0);
            Hotspot {
                function: (*func).to_string(),
                self_samples,
                total_samples: total,
                self_pct: pct(self_samples, grand_total),
                total_pct: pct(total, grand_total),
            }
        })
        .collect();

    hotspots.sort_by(|a, b| {
        let (pa, sa) = if sort_by_self {
            (a.self_samples, a.total_samples)
        } else {
            (a.total_samples, a.self_samples)
        };
        let (pb, sb) = if sort_by_self {
            (b.self_samples, b.total_samples)
        } else {
            (b.total_samples, b.self_samples)
        };
        // Descending by primary then secondary; function name breaks ties for
        // a deterministic ordering.
        pb.cmp(&pa)
            .then(sb.cmp(&sa))
            .then(a.function.cmp(&b.function))
    });
    hotspots.truncate(top_n);

    CpuReport {
        total_samples: grand_total,
        distinct_stacks,
        top: hotspots,
    }
}

fn pct(n: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (n as f64 / total as f64) * 100.0
    }
}

/// One call edge (a caller of, or a callee called by, the target function).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CallEdge {
    pub function: String,
    pub samples: u64,
}

/// Callers and callees of a single function, for agent drill-down.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CallersReport {
    pub function: String,
    /// Samples in stacks where `function` appears anywhere.
    pub total_samples: u64,
    /// Functions that directly called `function` (the frame toward the root).
    pub callers: Vec<CallEdge>,
    /// Functions `function` directly called (the frame toward the leaf).
    pub callees: Vec<CallEdge>,
}

/// Build a callers/callees drill-down for `function` from folded stacks.
pub fn callers_report_from_folded(folded: &str, function: &str) -> CallersReport {
    let mut callers: HashMap<&str, u64> = HashMap::new();
    let mut callees: HashMap<&str, u64> = HashMap::new();
    let mut total: u64 = 0;

    for line in folded.lines() {
        let line = line.trim();
        let Some((stack, count_str)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(count) = count_str.trim().parse::<u64>() else {
            continue;
        };
        let frames: Vec<&str> = stack.split(';').filter(|f| !f.is_empty()).collect();
        let mut hit = false;
        for (i, frame) in frames.iter().enumerate() {
            if *frame == function {
                hit = true;
                if i > 0 {
                    *callers.entry(frames[i - 1]).or_default() += count;
                }
                if i + 1 < frames.len() {
                    *callees.entry(frames[i + 1]).or_default() += count;
                }
            }
        }
        // Count each stack once toward the total, even if `function` recurses.
        if hit {
            total += count;
        }
    }

    CallersReport {
        function: function.to_string(),
        total_samples: total,
        callers: sort_edges(callers),
        callees: sort_edges(callees),
    }
}

/// Per-function self and total sample counts, plus the grand total, from folded
/// stacks. (Owned keys so callers can compare two independent captures.)
fn self_total_maps(folded: &str) -> (HashMap<String, u64>, HashMap<String, u64>, u64) {
    let mut self_c: HashMap<String, u64> = HashMap::new();
    let mut total_c: HashMap<String, u64> = HashMap::new();
    let mut grand: u64 = 0;
    for line in folded.lines() {
        let Some((stack, count_str)) = line.trim().rsplit_once(' ') else {
            continue;
        };
        let Ok(count) = count_str.trim().parse::<u64>() else {
            continue;
        };
        let frames: Vec<&str> = stack.split(';').filter(|f| !f.is_empty()).collect();
        if frames.is_empty() {
            continue;
        }
        grand += count;
        let mut seen: HashSet<&str> = HashSet::new();
        for frame in &frames {
            if seen.insert(frame) {
                *total_c.entry((*frame).to_string()).or_default() += count;
            }
        }
        *self_c
            .entry(frames[frames.len() - 1].to_string())
            .or_default() += count;
    }
    (self_c, total_c, grand)
}

/// One function's change between two captures, in percentage points of self
/// time (normalized so captures of different sizes compare fairly).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FnDelta {
    pub function: String,
    pub self_pct_before: f64,
    pub self_pct_after: f64,
    /// after - before, in percentage points; positive = got hotter.
    pub self_pct_delta: f64,
}

/// A ranked before/after comparison of two captures ("did my change help?").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffReport {
    pub before_total_samples: u64,
    pub after_total_samples: u64,
    /// Functions ranked by absolute self% change, most-changed first.
    pub top: Vec<FnDelta>,
}

/// Diff two folded captures, ranking functions by absolute self% change.
pub fn diff_from_folded(before: &str, after: &str, top_n: usize) -> DiffReport {
    let (self_b, total_b, grand_b) = self_total_maps(before);
    let (self_a, total_a, grand_a) = self_total_maps(after);

    let mut names: HashSet<&str> = HashSet::new();
    for k in total_b.keys() {
        names.insert(k.as_str());
    }
    for k in total_a.keys() {
        names.insert(k.as_str());
    }

    let mut deltas: Vec<FnDelta> = names
        .into_iter()
        .map(|name| {
            let pct_b = pct(self_b.get(name).copied().unwrap_or(0), grand_b);
            let pct_a = pct(self_a.get(name).copied().unwrap_or(0), grand_a);
            FnDelta {
                function: name.to_string(),
                self_pct_before: pct_b,
                self_pct_after: pct_a,
                self_pct_delta: pct_a - pct_b,
            }
        })
        .collect();

    deltas.sort_by(|a, b| {
        b.self_pct_delta
            .abs()
            .partial_cmp(&a.self_pct_delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.function.cmp(&b.function))
    });
    deltas.truncate(top_n);

    DiffReport {
        before_total_samples: grand_b,
        after_total_samples: grand_a,
        top: deltas,
    }
}

fn sort_edges(map: HashMap<&str, u64>) -> Vec<CallEdge> {
    let mut edges: Vec<CallEdge> = map
        .into_iter()
        .map(|(function, samples)| CallEdge {
            function: function.to_string(),
            samples,
        })
        .collect();
    edges.sort_by(|a, b| b.samples.cmp(&a.samples).then(a.function.cmp(&b.function)));
    edges
}
