//! Collector knobs: legacy measurement switches are cached per process.
//! Generational pacing limits are read once per heap, only when enabled.
//! Measured behaviours default off except the chunk map.
//!
//! | knob | default | effect |
//! |---|---|---|
//! | `NEOVM_GC_CENSUS=1` | off | the generation census, one record per cycle (`census.rs`) |
//! | `NEOVM_GC_CENSUS_REMSET=1` | off | the census plus its remembered-set estimate: the barrier window covers every owner, so every store reaches the census |
//! | `NEOVM_GC_CENSUS_FILE=<path>` | unset | also append each census record to this file (read once, by `census.rs`) |
//! | `NEOVM_GC_MEMORY_TELEMETRY=1` | off | stopped-world retained/live inventory (requires `gc-memory-telemetry` feature) |
//! | `NEOVM_GC_MEMORY_FILE=<path>` | unset | append memory snapshots as JSONL when telemetry or GC trace is enabled |
//! | `NEOVM_GC_CHUNK_MAP` | on (`=0` disables) | page and block ownership through the chunk map (`chunk_map.rs`), on the mutator and on the GC thread |
//! | `NEOVM_GC_MAJOR_GROWTH_PERCENT` | `15` | major growth limit, with an 8 MiB floor; generational only |
//! | `NEOVM_GC_MAJOR_MAX_MINORS` | `64` | maximum completed minors between majors; generational only |
//! | `NEOVM_GC_STRESS_MAJOR_EVERY` | `8` | stressed cycle stride, normalized to at least one; generational only |
//! | `NEOVM_GC_VEC_SCAN=defer` | `snapshot` | MEASUREMENT ONLY (falsifier F-G (c), P3.2 F1b): no Tier-B vector snapshot and no vector claims, so page vectors defer to the stop-the-world termination and are traced by reachability |
//!
//! A heap reads the knobs once, in `TaggedHeap::new`. Legacy test overrides
//! are per thread; generational pacing needs no new global or TLS override.

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

/// Log, once per process, that a knob is away from its default: the
/// engagement check for a same-binary A/B run
/// (`RUST_LOG=neovm::gc::knobs=info`).
fn note_knob(name: &str, value: &str) {
    tracing::info!(target: "neovm::gc::knobs", "{name}={value} is on in this process");
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
            note_knob("NEOVM_GC_CENSUS_REMSET", "1");
            CensusMode::SurvivorsAndRemset
        } else if env_is_on("NEOVM_GC_CENSUS") {
            note_knob("NEOVM_GC_CENSUS", "1");
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
    *ON.get_or_init(|| {
        // On by default since the same-binary A/B: owns_* share 7.15% ->
        // 0.01% on the GC probe (-4.05% instructions), and the elb rows
        // -0.2..-1.1% (bubble, pidigits, elb-bytecomp, elb-pcase).
        let on = !matches!(
            std::env::var("NEOVM_GC_CHUNK_MAP").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        );
        if !on {
            note_knob("NEOVM_GC_CHUNK_MAP", "0");
        }
        on
    })
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
            Some("defer") => {
                note_knob("NEOVM_GC_VEC_SCAN", "defer");
                VecScanMode::Defer
            }
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

/// Per-heap major limits. Unlike the legacy measurement switches above,
/// these are read only by the heap constructor and add no global or TLS state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GenerationalPacingKnobs {
    pub(super) major_growth_percent: usize,
    pub(super) major_max_minors: usize,
    pub(super) stress_major_every: usize,
}

impl Default for GenerationalPacingKnobs {
    fn default() -> Self {
        Self {
            major_growth_percent: 15,
            major_max_minors: 64,
            stress_major_every: 8,
        }
    }
}

impl GenerationalPacingKnobs {
    fn parse_or(value: Option<&str>, default: usize) -> usize {
        value
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    pub(super) fn from_values(
        growth_percent: Option<&str>,
        max_minors: Option<&str>,
        stress_every: Option<&str>,
    ) -> Self {
        let defaults = Self::default();
        Self {
            // Zero growth still has the 8 MiB floor. Zero minor cap requests
            // a major at every due cycle; zero stress interval means every
            // stressed cycle, without a division or underflow corner case.
            major_growth_percent: Self::parse_or(growth_percent, defaults.major_growth_percent),
            major_max_minors: Self::parse_or(max_minors, defaults.major_max_minors),
            stress_major_every: Self::parse_or(stress_every, defaults.stress_major_every).max(1),
        }
    }
}

/// Call exactly once in GenState::new, reached by TaggedHeap::new. Disabled
/// heaps do not inspect the new environment variables or log new behaviour.
#[cold]
#[inline(never)]
pub(super) fn generational_pacing_knobs(enabled: bool) -> GenerationalPacingKnobs {
    if !enabled {
        // Disabled heaps never consult these limits. Preserve their existing
        // constructor values while changing the generational policy.
        return GenerationalPacingKnobs {
            major_growth_percent: 100,
            ..GenerationalPacingKnobs::default()
        };
    }
    let growth = std::env::var("NEOVM_GC_MAJOR_GROWTH_PERCENT").ok();
    let minors = std::env::var("NEOVM_GC_MAJOR_MAX_MINORS").ok();
    let stress = std::env::var("NEOVM_GC_STRESS_MAJOR_EVERY").ok();
    GenerationalPacingKnobs::from_values(growth.as_deref(), minors.as_deref(), stress.as_deref())
}
