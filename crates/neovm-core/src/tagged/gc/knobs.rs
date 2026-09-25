//! Collector knobs: environment switches read once per process, each the
//! same-binary A/B of one measured behaviour. All default off.
//!
//! | knob | default | effect |
//! |---|---|---|
//! | `NEOVM_GC_CENSUS=1` | off | the generation census, one record per cycle (`census.rs`) |
//! | `NEOVM_GC_CENSUS_REMSET=1` | off | the census plus its remembered-set estimate: the barrier window covers every owner, so every store reaches the census |
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

fn env_is_on(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1" | "on" | "true" | "yes")
    )
}

#[cfg(test)]
thread_local! {
    static CENSUS_OVERRIDE: std::cell::Cell<Option<CensusMode>> = const { std::cell::Cell::new(None) };
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

/// Test hook: the census mode heaps created on this thread use (`None`
/// restores the environment's).
#[cfg(test)]
pub(crate) fn set_census_mode_for_test(mode: Option<CensusMode>) {
    CENSUS_OVERRIDE.with(|c| c.set(mode));
}
