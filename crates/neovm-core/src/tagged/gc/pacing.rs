//! Generational major pacing, separate from the existing minor trigger.
//!
//! The evaluator calls the selector only when its existing threshold/pending/
//! stress decision already requires a new cycle. Continuations never advance
//! a pacing counter. Counter ownership stays with each mutator, and the poll
//! sums the registry rather than relying on a collector-global approximation.

use super::generational::GenerationCycle;
use super::knobs::GenerationalPacingKnobs;
use super::*;

pub(super) const MIN_MAJOR_GROWTH_BYTES: usize = 8 * 1024 * 1024;

/// Stored inline in each MutatorGcState. No additional global or TLS cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct GenerationPacingCounters {
    pub(super) promoted_since_major: usize,
    pub(super) minors_since_major: usize,
    pub(super) stress_cycles_since_major: usize,
}

impl GenerationPacingCounters {
    #[inline]
    fn accumulate(&mut self, other: Self) {
        self.promoted_since_major = self
            .promoted_since_major
            .saturating_add(other.promoted_since_major);
        self.minors_since_major = self
            .minors_since_major
            .saturating_add(other.minors_since_major);
        self.stress_cycles_since_major = self
            .stress_cycles_since_major
            .saturating_add(other.stress_cycles_since_major);
    }
}

pub(super) fn sum_pacing_counters<'a>(
    counters: impl Iterator<Item = &'a GenerationPacingCounters>,
) -> GenerationPacingCounters {
    let mut totals = GenerationPacingCounters::default();
    for &counter in counters {
        totals.accumulate(counter);
    }
    totals
}

/// A wide intermediate keeps overflow from shrinking the major threshold.
/// Clamping the final result also makes the poll correct on 32-bit targets.
pub(super) fn major_growth_bytes(old_bytes_after_major: usize, growth_percent: usize) -> usize {
    let proportional = (old_bytes_after_major as u128) * (growth_percent as u128) / 100;
    (proportional.min(usize::MAX as u128) as usize).max(MIN_MAJOR_GROWTH_BYTES)
}

pub(super) fn major_due(
    knobs: GenerationalPacingKnobs,
    old_bytes_after_major: usize,
    counters: GenerationPacingCounters,
    memory_full: bool,
    stress: bool,
) -> bool {
    memory_full
        || counters.promoted_since_major
            >= major_growth_bytes(old_bytes_after_major, knobs.major_growth_percent)
        || counters.minors_since_major >= knobs.major_max_minors
        || (stress
            && counters.stress_cycles_since_major.saturating_add(1) >= knobs.stress_major_every)
}

impl TaggedHeap {
    /// Due-only selection. Explicit collections bypass this helper and always
    /// use begin_stw_collection. The bootstrap/first partition stays major.
    #[inline]
    pub(crate) fn should_run_minor(&self, memory_full: bool, stress: bool) -> bool {
        self.generational.enabled
            && self.should_run_concurrent()
            && !self.generation_major_due(memory_full, stress)
    }

    #[cold]
    #[inline(never)]
    fn generation_major_due(&self, memory_full: bool, stress: bool) -> bool {
        let counters = sum_pacing_counters(self.mutators().map(|mutator| &mutator.pacing));
        major_due(
            self.generational.pacing_knobs,
            self.generational.old_bytes_after_major,
            counters,
            memory_full,
            stress,
        )
    }

    /// Credit only actual newly-old bytes, after P-all promotion. The stopped
    /// collector credits the current mutator; polling sums all registered
    /// mutators. A major's credits are discarded only when its sweep finishes.
    #[cold]
    #[inline(never)]
    pub(super) fn record_generation_promoted_bytes(&mut self, bytes: usize) {
        if !self.generational.enabled {
            return;
        }
        let pacing = &mut self.current_mutator_gc_mut().pacing;
        pacing.promoted_since_major = pacing.promoted_since_major.saturating_add(bytes);
    }

    /// Invoke once after selecting a fresh automatic stress cycle, before its
    /// begin entry. Neither continuation polls nor explicit GC call this.
    #[cold]
    #[inline(never)]
    pub(crate) fn note_generation_stress_cycle_started(&mut self) {
        if !self.generational.enabled {
            return;
        }
        let pacing = &mut self.current_mutator_gc_mut().pacing;
        pacing.stress_cycles_since_major = pacing.stress_cycles_since_major.saturating_add(1);
    }

    /// Invoke exactly once at completed sweep, after exact old-byte recount.
    /// The cycle enum survives flag clearing, so this works for both eager and
    /// deferred sweeps. Future parallel mutators require collector exclusion
    /// before resetting their counters through the registry.
    #[cold]
    #[inline(never)]
    pub(super) fn finish_generation_pacing(&mut self) {
        if !self.generational.enabled {
            return;
        }
        match self.generational.cycle {
            GenerationCycle::Major => {
                self.generational.old_bytes_after_major = self.generational.old_bytes;
                for mutator in self.mutators_mut() {
                    mutator.pacing = GenerationPacingCounters::default();
                }
            }
            GenerationCycle::Minor => {
                let pacing = &mut self.current_mutator_gc_mut().pacing;
                pacing.minors_since_major = pacing.minors_since_major.saturating_add(1);
            }
        }
    }

    /// The deferred first partition's permanent splice follows ordinary sweep
    /// completion. Refresh its exact ordinary-old baseline after that splice;
    /// do not reset any mutator facts a second time.
    #[cold]
    #[inline(never)]
    pub(super) fn refresh_generation_major_baseline_world_stopped(&mut self) {
        if self.generational.enabled {
            debug_assert_eq!(self.generational.cycle, GenerationCycle::Major);
            self.generational.old_bytes_after_major = self.generational.old_bytes;
        }
    }
}
