//! Collector knobs: environment switches read once per process, each the
//! same-binary A/B of one measured behaviour. All default off.
//!
//! | knob | default | effect |
//! |---|---|---|
//! | `NEOVM_GC_CENSUS=1` | off | the generation census, one record per cycle (`census.rs`) |
//! | `NEOVM_GC_CENSUS_REMSET=1` | off | the census plus its remembered-set estimate: the barrier window covers every owner, so every store reaches the census |
//! | `NEOVM_GC_CENSUS_FILE=<path>` | unset | also append each census record to this file (read once, by `census.rs`) |
//! | `NEOVM_GC_CHUNK_MAP=1` | off | page and block ownership through the chunk map (`chunk_map.rs`), on the mutator and on the GC thread |
//! | `NEOVM_GC_VEC_SCAN=defer` | `snapshot` | MEASUREMENT ONLY (falsifier F-G (c), P3.2 F1b): no Tier-B vector snapshot and no vector claims, so page vectors defer to the stop-the-world termination and are traced by reachability |
//!
//! A heap reads the knobs once, in `TaggedHeap::new`; tests override them
//! per thread before creating the heap they run on.

use std::sync::OnceLock;

/// What the generation census records (`census.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CensusMode {
    /// No census.
    Off,
    /// Survivor classes per cycle: old, young, promoted-then-dead.
    Survivors,
    /// Survivors plus the remembered-set estimate (every store reaches the
    /// census, which perturbs time but not counts).
    SurvivorsAndRemset,
}

/// How the concurrent marker handles page vectors (`NEOVM_GC_VEC_SCAN`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VecScanMode {
    /// The start handshake snapshots every vector backing (Tier B), the GC
    /// thread scans the snapshot and claims page vectors: today's marker.
    Snapshot,
    /// MEASUREMENT ONLY (P3.2 F1b): no snapshot, no claims, no
    /// clone-on-write; every page vector the marker meets defers to the
    /// termination, whose `mark_value` traces its current backing. A dead
    /// vector's children are then no longer marked through the snapshot.
    Defer,
}

fn env_is_on(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1" | "on" | "true" | "yes")
    )
}

#[cfg(test)]
thread_local! {
    static CENSUS_OVERRIDE: std::cell::Cell<Option<CensusMode>> = const { std::cell::Cell::new(None) };
    static CHUNK_MAP_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static VEC_SCAN_OVERRIDE: std::cell::Cell<Option<VecScanMode>> = const { std::cell::Cell::new(None) };
}

/// `NEOVM_GC_CENSUS` / `NEOVM_GC_CENSUS_REMSET`.
pub(crate) fn census_mode() -> CensusMode {
    #[cfg(test)]
    if let Some(mode) = CENSUS_OVERRIDE.with(|c| c.get()) {
        return mode;
    }
    static MODE: OnceLock<CensusMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        if env_is_on("NEOVM_GC_CENSUS_REMSET") {
            CensusMode::SurvivorsAndRemset
        } else if env_is_on("NEOVM_GC_CENSUS") {
            CensusMode::Survivors
        } else {
            CensusMode::Off
        }
    })
}

/// `NEOVM_GC_CHUNK_MAP`.
pub(crate) fn chunk_map_on() -> bool {
    #[cfg(test)]
    if let Some(on) = CHUNK_MAP_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| env_is_on("NEOVM_GC_CHUNK_MAP"))
}

/// `NEOVM_GC_VEC_SCAN`.
pub(crate) fn vec_scan_mode() -> VecScanMode {
    #[cfg(test)]
    if let Some(mode) = VEC_SCAN_OVERRIDE.with(|c| c.get()) {
        return mode;
    }
    static MODE: OnceLock<VecScanMode> = OnceLock::new();
    *MODE.get_or_init(
        || match std::env::var("NEOVM_GC_VEC_SCAN").ok().as_deref() {
            Some("defer") => VecScanMode::Defer,
            _ => VecScanMode::Snapshot,
        },
    )
}

/// Test hook: the census mode heaps created on this thread use (`None`
/// restores the environment's).
#[cfg(test)]
pub(crate) fn set_census_mode_for_test(mode: Option<CensusMode>) {
    CENSUS_OVERRIDE.with(|c| c.set(mode));
}

/// Test hook: whether heaps created on this thread use the chunk map
/// (`None` restores the environment's).
#[cfg(test)]
pub(crate) fn set_chunk_map_for_test(on: Option<bool>) {
    CHUNK_MAP_OVERRIDE.with(|c| c.set(on));
}

/// Test hook: the vector scan mode heaps created on this thread use (`None`
/// restores the environment's).
#[cfg(test)]
pub(crate) fn set_vec_scan_mode_for_test(mode: Option<VecScanMode>) {
    VEC_SCAN_OVERRIDE.with(|c| c.set(mode));
}
