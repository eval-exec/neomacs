//! Issue #851: repeated redisplay captures over a rich viewport (>256 distinct
//! owners) inside a populated session must not thrash `ObservedEnvelope::owners`
//! or invoke `reject_foreign_capture_reads` on every active read.
//!
//! Each capture starts with an empty `CaptureReads` and cleared `RECENT_READS`.
//! Without an epoch-validated working-set cache in `ObservedEnvelope`, every
//! already-published viewport owner falls through to `self.owners.contains` +
//! radix metadata lookup on every capture pass, scaling to ~19% CPU when
//! `ObservedEnvelope::owners` has grown large.

use super::*;
use crate::heap_types::LispString;
use crate::tagged::gc::{TaggedHeap, collection_observed_metadata, set_tagged_heap};

const POPULATION: usize = 32_768;
const VIEWPORT: usize = 1_024;
const PASSES: usize = 32;

struct ObservedMode;

impl ObservedMode {
    fn enter() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

#[test]
fn issue_851_rich_viewport_captures_avoid_repeated_owners_set_probes_and_empty_foreign_rejects() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::enter();
    let mut heap = Box::new(TaggedHeap::new());
    heap.set_gc_threshold(usize::MAX);
    set_tagged_heap(&mut heap);

    // Populate ObservedEnvelope::owners with a large session population
    // (mix of cons cells, vectors, and strings).
    let mut population = Vec::with_capacity(POPULATION);
    for i in 0..POPULATION {
        let val = match i % 8 {
            0 => heap.alloc_vector(vec![TaggedValue::make_int(i as i64)]),
            1 => heap.alloc_string(LispString::from_utf8("span")),
            _ => heap.alloc_cons(TaggedValue::make_int(i as i64), TaggedValue::NIL),
        };
        population.push(val);
    }

    let read_val = |v: TaggedValue| -> i64 {
        if v.is_cons() {
            v.cons_car().as_int().expect("cons car")
        } else if v.is_string() {
            i64::from(v.as_str_owned().expect("string").as_bytes()[0])
        } else {
            v.as_vector_data()
                .expect("vector")
                .at(0)
                .as_int()
                .expect("vec slot")
        }
    };

    // Publish all POPULATION owners in chunks (< MAX_READS per capture).
    for chunk in population.chunks(8_192) {
        let (_, cert) = capture(|| {
            for &owner in chunk {
                std::hint::black_box(read_val(owner));
            }
        });
        assert!(cert.expect("population chunk capture coherent").unchanged());
    }

    let viewport = &population[..VIEWPORT];

    // Warm one viewport pass so the working-set cache can absorb the viewport.
    let (_, warm_cert) = capture(|| {
        for &owner in viewport {
            std::hint::black_box(read_val(owner));
        }
    });
    assert!(warm_cert.expect("warm viewport capture").unchanged());

    // Measure PASSES consecutive redisplay-style captures over the >256-owner
    // viewport (each pass reads every viewport owner twice, overflowing the
    // 256-slot RECENT_READS table on every pass).
    with_state(|state| state.borrow().observed.reset_owners_set_probes_for_test());
    store_state::reset_foreign_reject_empty_calls_for_test();

    let started = std::time::Instant::now();
    let mut last_cert = None;
    for _ in 0..PASSES {
        let (_, cert) = capture(|| {
            for _ in 0..2 {
                for &owner in viewport {
                    std::hint::black_box(read_val(owner));
                }
            }
        });
        last_cert = Some(cert.expect("viewport capture coherent"));
    }
    let elapsed = started.elapsed();

    let owners_probes = with_state(|state| state.borrow().observed.owners_set_probes_for_test());
    let empty_foreign_rejects = store_state::foreign_reject_empty_calls_for_test();
    let total_scope_reads = PASSES * VIEWPORT;

    eprintln!(
        "issue #851 working_set: population={POPULATION} viewport={VIEWPORT} passes={PASSES} \
         owners_set_probes={owners_probes} (vs uncached {total_scope_reads}) \
         empty_foreign_rejects={empty_foreign_rejects} elapsed={elapsed:?}"
    );

    // Without the epoch-validated working-set cache, every viewport owner
    // probes `ObservedEnvelope::owners` on every pass (owners_probes == 32,768).
    // With the 4-way 4,096-entry cache, a 1,024-owner viewport has almost zero
    // fallback probes across all 32 passes.
    assert!(
        owners_probes < total_scope_reads / 16,
        "issue #851: {PASSES} passes over {VIEWPORT} already-published owners probed \
         ObservedEnvelope::owners {owners_probes} times (expected < {})",
        total_scope_reads / 16
    );
    assert_eq!(
        empty_foreign_rejects, 0,
        "issue #851: active reads with no suspended foreign captures must not call \
         reject_foreign_capture_reads ({empty_foreign_rejects} empty calls)"
    );

    // Soundness check 1: mutations to viewport owners still invalidate certificates.
    let last_cert = last_cert.unwrap();
    assert!(last_cert.unchanged());
    let mut_cons = viewport.iter().copied().find(|v| v.is_cons()).unwrap();
    mut_cons.set_car(TaggedValue::make_int(-99));
    assert!(
        !last_cert.unchanged(),
        "mutating a cached viewport cons must still invalidate its certificate"
    );

    // Soundness check 2: GC reclamation advancing OBSERVATION_EPOCH must
    // invalidate the working-set cache immediately so a reclaimed and reused
    // address is NEVER treated as already published.
    let doomed_indices: Vec<usize> = (0..VIEWPORT).filter(|i| i % 4 == 0).collect();
    let kept: Vec<TaggedValue> = population
        .iter()
        .enumerate()
        .filter(|(i, _)| *i >= VIEWPORT || i % 4 != 0)
        .map(|(_, &v)| v)
        .collect();
    let doomed_bits: rustc_hash::FxHashSet<usize> = doomed_indices
        .iter()
        .map(|&i| population[i].bits())
        .collect();

    let epoch_before = super::super::gc::collection_observation_epoch();
    heap.collect_rooted(kept.iter().copied());
    let epoch_after = super::super::gc::collection_observation_epoch();
    assert!(
        epoch_after > epoch_before,
        "reclaiming observed viewport owners must advance OBSERVATION_EPOCH"
    );

    for &bits in &doomed_bits {
        assert!(!collection_observed_metadata(bits));
        assert!(
            !with_state(|state| state.borrow().observed.already_published(bits)),
            "reclaimed owner must not hit published cache after epoch advance"
        );
    }

    // Reallocate cons/vector slots and verify any reused address is re-marked
    // and still invalidates fresh certificates on mutation.
    let mut reused = 0usize;
    for i in 0..(doomed_bits.len() * 4) {
        let fresh = if i % 2 == 0 {
            heap.alloc_cons(TaggedValue::make_int(i as i64), TaggedValue::NIL)
        } else {
            heap.alloc_vector(vec![TaggedValue::make_int(i as i64)])
        };
        if !doomed_bits.contains(&fresh.bits()) {
            continue;
        }
        reused += 1;
        assert!(!collection_observed_metadata(fresh.bits()));
        let (_, cert) = capture(|| std::hint::black_box(read_val(fresh)));
        let cert = cert.expect("reused owner capture coherent");
        assert!(
            collection_observed_metadata(fresh.bits()),
            "reused address must receive a fresh sticky mark"
        );
        assert!(cert.unchanged());
        if fresh.is_cons() {
            fresh.set_car(TaggedValue::make_int(-7));
        } else {
            assert!(crate::tagged::mutate::set_vector_slot(
                fresh,
                0,
                TaggedValue::make_int(-7)
            ));
        }
        assert!(
            !cert.unchanged(),
            "mutating a reused address must still invalidate its certificate"
        );
    }
    assert!(
        reused > 0,
        "allocator must reuse at least one reclaimed slot"
    );
}

#[test]
fn issue_851_world_switch_in_install_unconditionally_invalidates_published_cache() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::enter();
    let mut heap_a = Box::new(TaggedHeap::new());
    heap_a.set_gc_threshold(usize::MAX);
    let mut heap_b = Box::new(TaggedHeap::new());
    heap_b.set_gc_threshold(usize::MAX);

    // Install World A (dump span BarrierWindow::NONE) and publish + cache a cell.
    set_tagged_heap(&mut heap_a);
    let cell = heap_a.alloc_cons(TaggedValue::make_int(42), TaggedValue::NIL);
    let (_, cert1) = capture(|| std::hint::black_box(cell.cons_car().as_int().expect("car")));
    assert!(cert1.expect("first capture coherent").unchanged());

    // Confirm warm cache hit on World A before switching Worlds.
    let epoch = crate::tagged::gc::collection_observation_epoch();
    assert!(
        with_state(|state| state
            .borrow()
            .observed
            .is_published_in_cache(epoch, cell.bits())),
        "cell must be cached in World A after publication"
    );
    with_state(|state| state.borrow().observed.reset_owners_set_probes_for_test());
    let (_, cert2) = capture(|| std::hint::black_box(cell.cons_car().as_int().expect("car")));
    assert!(cert2.expect("warm capture coherent").unchanged());
    assert_eq!(
        with_state(|state| state.borrow().observed.owners_set_probes_for_test()),
        0,
        "warm capture before World switch must hit PublishedCache with zero owners probes"
    );

    // Switch to World B and back to World A with unchanged dump span (both NONE).
    // SAFETY: bare `TaggedHeap` unit test switching heaps on this thread.
    unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(&heap_a) };
    set_tagged_heap(&mut heap_b);
    // SAFETY: bare `TaggedHeap` unit test restoring `heap_a`.
    unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(&heap_b) };
    set_tagged_heap(&mut heap_a);

    // ObservationWorld::install must have invalidated World A's PublishedCache
    // unconditionally on the World switch, even though previous_dump == dump.
    let epoch_after = crate::tagged::gc::collection_observation_epoch();
    assert!(
        !with_state(|state| state
            .borrow()
            .observed
            .is_published_in_cache(epoch_after, cell.bits())),
        "World switch with unchanged dump span must invalidate arriving World's PublishedCache"
    );

    // First capture after World switch misses PublishedCache (1 owners probe)
    // and repopulates the cache; second capture hits PublishedCache (0 probes).
    with_state(|state| state.borrow().observed.reset_owners_set_probes_for_test());
    let (_, cert3) = capture(|| std::hint::black_box(cell.cons_car().as_int().expect("car")));
    assert!(cert3.expect("post-switch capture coherent").unchanged());
    assert_eq!(
        with_state(|state| state.borrow().observed.owners_set_probes_for_test()),
        1,
        "first capture after World switch must miss invalidated PublishedCache once"
    );

    with_state(|state| state.borrow().observed.reset_owners_set_probes_for_test());
    let (_, cert4) = capture(|| std::hint::black_box(cell.cons_car().as_int().expect("car")));
    assert!(cert4.expect("repopulated capture coherent").unchanged());
    assert_eq!(
        with_state(|state| state.borrow().observed.owners_set_probes_for_test()),
        0,
        "second capture after World switch must hit repopulated PublishedCache"
    );
}
