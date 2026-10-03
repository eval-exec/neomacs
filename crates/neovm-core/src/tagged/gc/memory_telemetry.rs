//! Optional stopped-world GC inventory and allocator observations.
//!
//! The feature is absent from ordinary builds: all callers and this module
//! compile out. In diagnostic builds, NEOVM_GC_MEMORY_TELEMETRY=1 (or the
//! existing GC trace switch) enables snapshots. NEOVM_GC_MEMORY_FILE also
//! appends JSONL without depending on a process tracing subscriber.
//!
//! Observations do not purge the allocator or change collection decisions.
//! Inventory describes retained allocations, including floating garbage;
//! reachability and dead-old bytes require a full final mark before promotion.

use super::memory_inventory::{self, MemoryInventory};
use super::pacing::{major_growth_bytes, sum_pacing_counters};
use super::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Unsupported allocator observations remain absent rather than zero.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct AllocatorSnapshot {
    pub available: bool,
    pub stats_enabled: bool,
    pub error: i32,
    pub allocated_bytes: Option<usize>,
    pub active_bytes: Option<usize>,
    pub resident_bytes: Option<usize>,
    pub mapped_bytes: Option<usize>,
    pub retained_bytes: Option<usize>,
}

static ALLOCATOR_SAMPLER: OnceLock<fn() -> AllocatorSnapshot> = OnceLock::new();

/// The process installing its allocator supplies the matching observer.
pub fn set_allocator_sampler(sampler: fn() -> AllocatorSnapshot) {
    let _ = ALLOCATOR_SAMPLER.set(sampler);
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct ProcessSnapshot {
    rss_bytes: Option<usize>,
    peak_rss_bytes: Option<usize>,
}

impl ProcessSnapshot {
    fn read() -> Self {
        #[cfg(target_os = "linux")]
        {
            // Read into stack storage, before the inventory allocates its
            // small serialization vectors. /proc reports both fields in KiB.
            let mut bytes = [0u8; 8192];
            // SAFETY: the static path is nul-terminated; fd is used only here.
            let fd = unsafe { libc::open(c"/proc/self/status".as_ptr(), libc::O_RDONLY) };
            if fd >= 0 {
                // SAFETY: bytes covers its advertised writable length.
                let len = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
                // SAFETY: fd belongs to this observation, even if read failed.
                unsafe { libc::close(fd) };
                if len > 0 {
                    return Self::parse(&bytes[..len as usize]);
                }
            }
        }
        Self::default()
    }

    fn parse(bytes: &[u8]) -> Self {
        let Ok(status) = std::str::from_utf8(bytes) else {
            return Self::default();
        };
        fn field(status: &str, name: &str) -> Option<usize> {
            let mut values = status
                .lines()
                .find_map(|line| line.strip_prefix(name))?
                .split_whitespace();
            let value = values.next()?.parse::<usize>().ok()?;
            if values.next()? != "kB" {
                return None;
            }
            value.checked_mul(1024)
        }
        Self {
            rss_bytes: field(status, "VmRSS:"),
            peak_rss_bytes: field(status, "VmHWM:"),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Phase {
    CollectionBegin { stw_entry: bool },
    ConcurrentJoined,
    FinalMark,
    SweepComplete,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::CollectionBegin { .. } => "collection_begin",
            Self::ConcurrentJoined => "concurrent_joined",
            Self::FinalMark => "final_mark",
            Self::SweepComplete => "sweep_complete",
        }
    }
}

#[derive(Default)]
struct HeapObservation {
    selection_cycle: usize,
    selection_reasons: Vec<&'static str>,
    promoted_bytes: usize,
    last_major_dead_pacing_bytes: Option<usize>,
}

struct Session {
    file: Option<File>,
    started: Instant,
    heaps: HashMap<usize, HeapObservation>,
}

static SESSION: OnceLock<Option<Mutex<Session>>> = OnceLock::new();

fn session() -> Option<&'static Mutex<Session>> {
    SESSION.get_or_init(|| {
        let on = |name| matches!(std::env::var(name).ok().as_deref(), Some("1" | "on" | "true" | "yes"));
        if !on("NEOVM_GC_MEMORY_TELEMETRY") && !on("NEOVM_GC_TRACE") {
            return None;
        }
        let file = std::env::var_os("NEOVM_GC_MEMORY_FILE").and_then(|path| {
            match OpenOptions::new().create(true).append(true).open(path) {
                Ok(file) => Some(file),
                Err(error) => {
                    tracing::warn!(target: "neovm::gc::memory", %error, "cannot open GC memory telemetry file");
                    None
                }
            }
        });
        Some(Mutex::new(Session { file, started: Instant::now(), heaps: HashMap::new() }))
    }).as_ref()
}

#[derive(Serialize)]
struct PacingSnapshot {
    old_bytes_after_major: usize,
    old_bytes_current: usize,
    promoted_since_major: usize,
    minors_since_major: usize,
    stress_cycles_since_major: usize,
    major_growth_percent: usize,
    major_growth_budget_bytes: usize,
    major_max_minors: usize,
    stress_major_every: usize,
}

fn pacing_snapshot(heap: &TaggedHeap) -> PacingSnapshot {
    let counters = sum_pacing_counters(heap.mutators().map(|mutator| &mutator.pacing));
    let gen_state = &heap.generational;
    let knobs = gen_state.pacing_knobs;
    PacingSnapshot {
        old_bytes_after_major: gen_state.old_bytes_after_major,
        old_bytes_current: gen_state.old_bytes,
        promoted_since_major: counters.promoted_since_major,
        minors_since_major: counters.minors_since_major,
        stress_cycles_since_major: counters.stress_cycles_since_major,
        major_growth_percent: knobs.major_growth_percent,
        major_growth_budget_bytes: major_growth_bytes(
            gen_state.old_bytes_after_major,
            knobs.major_growth_percent,
        ),
        major_max_minors: knobs.major_max_minors,
        stress_major_every: knobs.stress_major_every,
    }
}

fn selection_reasons(heap: &TaggedHeap, memory_full: bool, stress: bool) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if !heap.generational.enabled {
        reasons.push("generational_disabled");
        return reasons;
    }
    let state = pacing_snapshot(heap);
    if !heap.should_run_concurrent() {
        reasons.push("bootstrap_or_partition");
    }
    if memory_full {
        reasons.push("memory_full");
    }
    if state.promoted_since_major >= state.major_growth_budget_bytes {
        reasons.push("old_growth");
    }
    if state.minors_since_major >= state.major_max_minors {
        reasons.push("minor_limit");
    }
    if stress && state.stress_cycles_since_major.saturating_add(1) >= state.stress_major_every {
        reasons.push("stress_stride");
    }
    reasons
}

/// Observe the selector's inputs without changing the selector or counters.
pub(super) fn note_selection(heap: &TaggedHeap, memory_full: bool, stress: bool) {
    let Some(session) = session() else {
        return;
    };
    let mut session = session.lock().unwrap_or_else(|error| error.into_inner());
    let observation = session.heaps.entry(heap.identity()).or_default();
    observation.selection_cycle = heap.gc_collections.saturating_add(1);
    observation.selection_reasons = selection_reasons(heap, memory_full, stress);
}

/// Reuse the collector's actual promotion accounting; do not estimate it.
pub(super) fn note_promotion(heap: &TaggedHeap, bytes: usize) {
    let Some(session) = session() else {
        return;
    };
    let mut session = session.lock().unwrap_or_else(|error| error.into_inner());
    let observation = session.heaps.entry(heap.identity()).or_default();
    observation.promoted_bytes = observation.promoted_bytes.saturating_add(bytes);
}

#[derive(Serialize)]
struct Record<'a> {
    schema: u32,
    heap_identity: usize,
    time_us: u64,
    phase: &'static str,
    cycle: usize,
    kind: &'static str,
    generational: bool,
    marks_final: bool,
    process: ProcessSnapshot,
    allocator: AllocatorSnapshot,
    pacing: PacingSnapshot,
    major_trigger_reasons: &'a [&'static str],
    promoted_bytes_this_cycle: usize,
    last_major_dead_pacing_bytes: Option<usize>,
    inventory: MemoryInventory,
}

/// Call only with closed regions and no concurrent marker. FinalMark must
/// follow weak/finalizer fixpoints and precede P-all promotion.
pub(super) fn observe(heap: &TaggedHeap, phase: Phase) {
    let Some(session) = session() else {
        return;
    };
    let process = ProcessSnapshot::read();
    let allocator = ALLOCATOR_SAMPLER
        .get()
        .map_or_else(AllocatorSnapshot::default, |sample| sample());
    let marks_final = matches!(phase, Phase::FinalMark);
    let inventory = memory_inventory::snapshot(heap, marks_final);
    let cycle = if matches!(phase, Phase::SweepComplete) {
        heap.gc_collections
    } else {
        heap.gc_collections.saturating_add(1)
    };
    let mut session = session.lock().unwrap_or_else(|error| error.into_inner());
    let elapsed_us = session.started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    let observation = session.heaps.entry(heap.identity()).or_default();
    if let Phase::CollectionBegin { stw_entry } = phase {
        observation.promoted_bytes = 0;
        if stw_entry {
            if observation.selection_cycle == cycle {
                // Automatic stress majors also use the synchronous entry.
                // Preserve the selector's reasons instead of calling them
                // explicit collections merely because the entry was STW.
                observation.selection_reasons.push("selected_stw");
            } else {
                observation.selection_reasons = vec!["explicit_or_fallback_stw"];
            }
        } else if observation.selection_cycle != cycle {
            observation.selection_reasons = selection_reasons(heap, false, false);
        }
    }
    if let Some(bytes) = inventory.old_dead_pacing_bytes {
        observation.last_major_dead_pacing_bytes = Some(bytes);
    }
    let record = Record {
        schema: 1,
        heap_identity: heap.identity(),
        time_us: elapsed_us,
        phase: phase.name(),
        cycle,
        kind: if !heap.generational.enabled {
            "full"
        } else if heap.is_minor_collection() {
            "minor"
        } else {
            "major"
        },
        generational: heap.generational.enabled,
        marks_final,
        process,
        allocator,
        pacing: pacing_snapshot(heap),
        major_trigger_reasons: &observation.selection_reasons,
        promoted_bytes_this_cycle: observation.promoted_bytes,
        last_major_dead_pacing_bytes: observation.last_major_dead_pacing_bytes,
        inventory,
    };
    match serde_json::to_string(&record) {
        Ok(json) => {
            tracing::info!(target: "neovm::gc::memory", snapshot = %json, "gc_memory_snapshot");
            if let Some(file) = &mut session.file {
                if let Err(error) = writeln!(file, "{json}") {
                    tracing::warn!(target: "neovm::gc::memory", %error, "cannot write GC memory telemetry");
                }
            }
        }
        Err(error) => {
            tracing::warn!(target: "neovm::gc::memory", %error, "cannot encode GC memory telemetry")
        }
    }
}

#[cfg(test)]
#[path = "tests/memory_telemetry_tests.rs"]
mod tests;
