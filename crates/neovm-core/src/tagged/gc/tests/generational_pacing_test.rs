//! Due-only generational pacing and real promotion/completion accounting.

use super::generational::GenerationCycle;
use super::knobs::GenerationalPacingKnobs;
use super::pacing::{
    GenerationPacingCounters, MIN_MAJOR_GROWTH_BYTES, major_due, major_growth_bytes,
    sum_pacing_counters,
};
use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::heap_types::LispString;

fn counters(promoted: usize, minors: usize, stress_cycles: usize) -> GenerationPacingCounters {
    GenerationPacingCounters {
        promoted_since_major: promoted,
        minors_since_major: minors,
        stress_cycles_since_major: stress_cycles,
    }
}

fn ready_heap(enabled: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = enabled;
    heap.generational.pacing_knobs = GenerationalPacingKnobs::default();
    heap.bootstrap_collected = true;
    heap.publish_barrier_window();
    heap
}

#[test]
fn generational_pacing_knob_defaults_and_invalid_values() {
    let defaults = GenerationalPacingKnobs::default();
    assert_eq!(
        (
            defaults.major_growth_percent,
            defaults.major_max_minors,
            defaults.stress_major_every
        ),
        (15, 64, 8)
    );
    assert_eq!(
        GenerationalPacingKnobs::from_values(None, None, None),
        defaults
    );
    assert_eq!(
        GenerationalPacingKnobs::from_values(Some("-1"), Some("no"), Some("")),
        defaults
    );
    let overflow = format!("{}0", usize::MAX);
    assert_eq!(
        GenerationalPacingKnobs::from_values(Some(&overflow), Some(&overflow), Some(&overflow)),
        defaults
    );
}

#[test]
fn generational_pacing_zero_limits_and_constructor_off_defaults() {
    let knobs = GenerationalPacingKnobs::from_values(Some("0"), Some("0"), Some("0"));
    assert_eq!(
        (
            knobs.major_growth_percent,
            knobs.major_max_minors,
            knobs.stress_major_every
        ),
        (0, 0, 1)
    );
    assert_eq!(
        major_growth_bytes(usize::MAX, knobs.major_growth_percent),
        MIN_MAJOR_GROWTH_BYTES
    );
    assert!(major_due(knobs, 0, counters(0, 0, 0), false, false));
    assert_eq!(
        super::knobs::generational_pacing_knobs(false),
        GenerationalPacingKnobs {
            major_growth_percent: 100,
            ..GenerationalPacingKnobs::default()
        }
    );
}

#[test]
fn generational_pacing_growth_floor_has_exact_boundary() {
    let knobs = GenerationalPacingKnobs::default();
    assert!(!major_due(
        knobs,
        0,
        counters(MIN_MAJOR_GROWTH_BYTES - 1, 0, 0),
        false,
        false
    ));
    assert!(major_due(
        knobs,
        0,
        counters(MIN_MAJOR_GROWTH_BYTES, 0, 0),
        false,
        false
    ));
}

#[test]
fn generational_pacing_default_growth_bounds_large_old_heaps() {
    let mib = 1024 * 1024;
    let baseline = 80 * mib;
    let budget = 12 * mib;
    let defaults = GenerationalPacingKnobs::default();
    assert_eq!(
        major_growth_bytes(baseline, defaults.major_growth_percent),
        budget
    );
    assert!(!major_due(
        defaults,
        baseline,
        counters(budget - 1, 0, 0),
        false,
        false
    ));
    assert!(major_due(
        defaults,
        baseline,
        counters(budget, 0, 0),
        false,
        false
    ));

    let previous_policy = GenerationalPacingKnobs::from_values(Some("100"), None, None);
    assert_eq!(
        major_growth_bytes(baseline, previous_policy.major_growth_percent),
        baseline
    );
    assert!(!major_due(
        previous_policy,
        baseline,
        counters(budget, 0, 0),
        false,
        false
    ));
    assert!(!major_due(
        previous_policy,
        baseline,
        counters(baseline - 1, 0, 0),
        false,
        false
    ));
    assert!(major_due(
        previous_policy,
        baseline,
        counters(baseline, 0, 0),
        false,
        false
    ));
}

#[test]
fn generational_pacing_default_growth_keeps_small_heap_floor_and_minor_cap() {
    let defaults = GenerationalPacingKnobs::default();
    let baseline = 48 * 1024 * 1024;
    assert_eq!(
        major_growth_bytes(baseline, defaults.major_growth_percent),
        MIN_MAJOR_GROWTH_BYTES
    );
    assert!(!major_due(
        defaults,
        baseline,
        counters(MIN_MAJOR_GROWTH_BYTES - 1, 0, 0),
        false,
        false
    ));
    assert!(major_due(
        defaults,
        baseline,
        counters(MIN_MAJOR_GROWTH_BYTES, 0, 0),
        false,
        false
    ));
    assert!(!major_due(
        defaults,
        baseline,
        counters(0, 63, 0),
        false,
        false
    ));
    assert!(major_due(
        defaults,
        baseline,
        counters(0, 64, 0),
        false,
        false
    ));
}

#[test]
fn generational_pacing_growth_uses_last_completed_major_baseline() {
    let knobs = GenerationalPacingKnobs::from_values(Some("150"), None, None);
    let baseline = 32 * 1024 * 1024;
    let threshold = 48 * 1024 * 1024;
    assert_eq!(
        major_growth_bytes(baseline, knobs.major_growth_percent),
        threshold
    );
    assert!(!major_due(
        knobs,
        baseline,
        counters(threshold - 1, 0, 0),
        false,
        false
    ));
    assert!(major_due(
        knobs,
        baseline,
        counters(threshold, 0, 0),
        false,
        false
    ));
}

#[test]
fn generational_pacing_growth_overflow_never_wraps_to_a_small_threshold() {
    assert_eq!(major_growth_bytes(usize::MAX, usize::MAX), usize::MAX);
    assert_eq!(major_growth_bytes(usize::MAX, 100), usize::MAX);
    let expected = (((usize::MAX as u128) * 50 / 100) as usize).max(MIN_MAJOR_GROWTH_BYTES);
    assert_eq!(major_growth_bytes(usize::MAX, 50), expected);
}

#[test]
fn generational_pacing_minor_cap_has_exact_boundary() {
    let knobs = GenerationalPacingKnobs::default();
    assert!(!major_due(knobs, 0, counters(0, 63, 0), false, false));
    assert!(major_due(knobs, 0, counters(0, 64, 0), false, false));
}

#[test]
fn generational_pacing_memory_full_forces_major_when_cycle_is_due() {
    let mut heap = ready_heap(true);
    assert!(heap.should_run_minor(false, false));
    assert!(!heap.should_run_minor(true, false));
    heap.partition_dump = true;
    heap.dump_blackened = false;
    assert!(!heap.should_run_minor(false, false));
}

#[test]
fn generational_pacing_stress_selects_the_eighth_cycle_and_major_resets_stride() {
    let mut heap = ready_heap(true);
    for expected in 1..=7 {
        assert!(heap.should_run_minor(false, true));
        heap.note_generation_stress_cycle_started();
        heap.generational.cycle = GenerationCycle::Minor;
        heap.finish_generation_pacing();
        assert_eq!(
            heap.current_mutator_gc().pacing.stress_cycles_since_major,
            expected
        );
    }
    assert!(!heap.should_run_minor(false, true));
    heap.note_generation_stress_cycle_started();
    heap.generational.cycle = GenerationCycle::Major;
    heap.finish_generation_pacing();
    assert_eq!(
        heap.current_mutator_gc().pacing,
        GenerationPacingCounters::default()
    );
    assert!(heap.should_run_minor(false, true));
}

#[test]
fn generational_pacing_poll_is_read_only_and_normal_cycles_ignore_stress_stride() {
    let mut heap = ready_heap(true);
    heap.current_mutator_gc_mut().pacing = counters(0, 7, 7);
    let before = heap.current_mutator_gc().pacing;
    for _ in 0..16 {
        assert!(!heap.should_run_minor(false, true));
        assert!(heap.should_run_minor(false, false));
    }
    assert_eq!(heap.current_mutator_gc().pacing, before);
}

#[test]
fn generational_pacing_sums_every_mutator_counter_with_saturation() {
    let left = counters(4 * 1024 * 1024, 32, 3);
    let right = counters(4 * 1024 * 1024, 32, 4);
    let totals = sum_pacing_counters([&left, &right].into_iter());
    assert_eq!(totals, counters(MIN_MAJOR_GROWTH_BYTES, 64, 7));
    assert!(major_due(
        GenerationalPacingKnobs::default(),
        0,
        totals,
        false,
        false
    ));
    assert_eq!(
        sum_pacing_counters([&counters(usize::MAX, usize::MAX, usize::MAX), &right].into_iter()),
        counters(usize::MAX, usize::MAX, usize::MAX)
    );
}

#[test]
fn generational_pacing_completed_major_resets_counters_and_snapshots_exact_old_bytes() {
    let mut heap = ready_heap(true);
    heap.current_mutator_gc_mut().pacing = counters(usize::MAX, 64, 7);
    heap.generational.old_bytes = 19 * 1024 * 1024;
    heap.generational.old_bytes_after_major = 3;
    heap.generational.cycle = GenerationCycle::Major;
    heap.finish_generation_pacing();
    assert_eq!(heap.generational.old_bytes_after_major, 19 * 1024 * 1024);
    assert_eq!(
        heap.current_mutator_gc().pacing,
        GenerationPacingCounters::default()
    );
}

#[test]
fn generational_pacing_first_partition_baseline_refresh_does_not_reset_again() {
    let mut heap = ready_heap(true);
    heap.generational.cycle = GenerationCycle::Major;
    heap.generational.old_bytes = 1000;
    heap.finish_generation_pacing();
    heap.current_mutator_gc_mut().pacing = counters(17, 0, 0);
    heap.generational.old_bytes = size_of::<ConsCell>();
    heap.refresh_generation_major_baseline_world_stopped();
    assert_eq!(
        heap.generational.old_bytes_after_major,
        size_of::<ConsCell>()
    );
    assert_eq!(heap.current_mutator_gc().pacing, counters(17, 0, 0));
}

#[test]
fn generational_pacing_off_does_not_select_or_touch_new_accounting() {
    let mut heap = ready_heap(false);
    heap.current_mutator_gc_mut().pacing = counters(2, 3, 4);
    heap.generational.old_bytes_after_major = 5;
    heap.generational.old_bytes = 6;
    assert!(!heap.should_run_minor(false, false));
    heap.record_generation_promoted_bytes(8);
    heap.note_generation_stress_cycle_started();
    heap.finish_generation_pacing();
    heap.refresh_generation_major_baseline_world_stopped();
    assert_eq!(heap.current_mutator_gc().pacing, counters(2, 3, 4));
    assert_eq!(heap.generational.old_bytes_after_major, 5);
}

#[test]
fn generational_pacing_real_minor_credits_promotion_and_major_resets_after_reclamation() {
    struct Roots(usize);
    impl Drop for Roots {
        fn drop(&mut self) {
            restore_scratch_gc_roots(self.0);
        }
    }
    let mut heap = ready_heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots(save_scratch_gc_roots());
    let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    push_scratch_gc_root(owner);
    heap.collect_exact([owner].into_iter());
    assert_eq!(
        heap.generational.old_bytes_after_major,
        size_of::<ConsCell>()
    );
    let child = heap.alloc_string(LispString::from_utf8("minor promotion credit"));
    push_scratch_gc_root(child);
    let child_addr = TaggedHeap::value_heap_addr(child).unwrap();
    let child_bytes = TaggedHeap::object_bytes_from_header(child_addr as *const GcHeader);
    assert!(crate::tagged::mutate::set_cons_car(owner, child));
    heap.begin_minor_collection();
    heap.seed_root(owner);
    heap.complete_minor_collection();
    assert_eq!(heap.current_mutator_gc().pacing.minors_since_major, 0);
    assert_eq!(
        heap.current_mutator_gc().pacing.promoted_since_major,
        child_bytes
    );
    heap.finish_incremental_sweep_now();
    assert_eq!(heap.current_mutator_gc().pacing.minors_since_major, 1);
    assert_eq!(
        heap.generational.old_bytes_after_major,
        size_of::<ConsCell>()
    );
    heap.collect_exact([owner].into_iter());
    assert!(heap.owns_heap_value_for_test(child));
    assert_eq!(
        heap.generational.old_bytes_after_major,
        size_of::<ConsCell>() + child_bytes
    );
    assert_eq!(
        heap.current_mutator_gc().pacing,
        GenerationPacingCounters::default()
    );
    drop(roots);
    heap.collect_exact(std::iter::empty());
    assert!(!heap.owns_heap_value_for_test(owner));
    assert!(!heap.owns_heap_value_for_test(child));
    assert_eq!(heap.generational.old_bytes_after_major, 0);
}
