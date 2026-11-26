//! The original certificate remains authoritative while repeated consumers
//! at one owner journal sequence share the same validation result.
use super::*;
use crate::value::Value;

#[test]
fn repeated_certificate_validation_scans_once_per_owner_sequence() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("source certificate");
    assert_eq!(reads.full_validation_invocations_for_test(), 0);
    // Model the unrelated command/frame writes between source capture and
    // the many consumers of its retained source proof.
    for i in 0..64 {
        unrelated.set_car(Value::fixnum(i));
    }
    for _ in 0..128 {
        assert!(reads.unchanged());
    }
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        1,
        "unchanged consumers must share one full validation at this sequence"
    );

    unrelated.set_car(Value::T);
    for _ in 0..128 {
        assert!(reads.unchanged());
    }
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        2,
        "one unrelated write requires exactly one additional validation"
    );

    source.set_car(Value::T);
    for _ in 0..128 {
        assert!(!reads.unchanged());
    }
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        3,
        "the original dependency's mutation must reject, then reuse that rejection"
    );
}

#[test]
fn repeated_validation_never_rebases_original_journal_history() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("source certificate");
    let original = reads.reads.dependencies().expect("live lease").1;
    assert!(reads.unchanged());
    for i in 0..JOURNAL_SIZE / 2 {
        unrelated.set_car(Value::fixnum(i as i64));
    }
    assert!(reads.unchanged());
    for i in JOURNAL_SIZE / 2..JOURNAL_SIZE {
        unrelated.set_car(Value::fixnum(i as i64));
    }
    assert!(reads.unchanged(), "the original 8192 writes still fit");
    unrelated.set_car(Value::fixnum(JOURNAL_SIZE as i64));
    for _ in 0..8 {
        assert!(
            !reads.unchanged(),
            "8193 writes lose the original history despite successful intermediate checks"
        );
    }
    assert_eq!(reads.reads.dependencies().expect("live lease").1, original);
    assert_eq!(reads.full_validation_invocations_for_test(), 4);
    assert_eq!(
        reads.journal_slots_scanned_for_test(),
        JOURNAL_SIZE,
        "incremental validation must scan each journal slot at most once (0->4096->8192 = 8192, not 12288)"
    );
}

#[test]
fn incremental_validation_scans_only_new_journal_delta_and_latches_invalidation() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("source certificate");

    const STEPS: usize = 256;
    for i in 0..STEPS {
        unrelated.set_car(Value::fixnum(i as i64));
        assert!(reads.unchanged());
    }
    assert_eq!(
        reads.journal_slots_scanned_for_test(),
        STEPS,
        "validating after every unrelated write must scan O(N) total slots, not O(N^2)"
    );

    // Mutating the dependency invalidates the certificate on the next check
    // (scanning only the 1 new slot), and invalidation is terminal even across
    // subsequent unrelated writes.
    source.set_car(Value::T);
    assert!(!reads.unchanged());
    assert_eq!(reads.journal_slots_scanned_for_test(), STEPS + 1);
    let invocations_at_invalidation = reads.full_validation_invocations_for_test();

    for i in 0..16 {
        unrelated.set_car(Value::fixnum((STEPS + i) as i64));
        assert!(!reads.unchanged());
    }
    assert_eq!(
        reads.journal_slots_scanned_for_test(),
        STEPS + 1,
        "terminal invalidation must never re-scan later journal writes"
    );
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        invocations_at_invalidation
    );
}

#[test]
fn incremental_validation_matches_full_rescan_on_randomized_journals() {
    struct XorShift64(u64);
    impl XorShift64 {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn next_usize(&mut self, bound: usize) -> usize {
            (self.next() as usize) % bound
        }
    }

    struct TrackedCert {
        cert: CollectionReads,
        deps: rustc_hash::FxHashSet<usize>,
        dep_values: Vec<Value>,
        origin_log_index: usize,
        last_checked_log_index: usize,
        ever_checked: bool,
        was_valid_at_last_check: bool,
        pre_capture_origin_dep_written: bool,
    }

    let mutable_pool: Vec<Value> = (0..16)
        .map(|i| Value::cons(Value::fixnum(i), Value::NIL))
        .collect();
    // Quiet cells are never written by random bursts after capture (only
    // optionally right *before* capture to test `0..delta` off-by-one), so
    // non-empty certificates over `quiet_pool` cleanly survive past JOURNAL_SIZE.
    let quiet_pool: Vec<Value> = (0..8)
        .map(|i| Value::cons(Value::fixnum(100 + i), Value::NIL))
        .collect();
    let unrelated = Value::cons(Value::NIL, Value::NIL);

    let mut rng = XorShift64(0x9E37_79B9_7F4A_7C15);
    let mut write_log: Vec<usize> = Vec::new();
    let mut certs: Vec<TrackedCert> = Vec::new();
    let mut write_seq_val: i64 = 0;

    let mut push_write = |target: Value, log: &mut Vec<usize>| {
        write_seq_val += 1;
        target.set_car(Value::fixnum(write_seq_val));
        log.push(target.bits());
    };

    // Coverage counters proving the randomized workload actually reaches every
    // soundness boundary rather than masking horizon/wrapover/endpoint bugs.
    let mut horizon_only_invalidations = 0usize;
    let mut exact_8192_clean_hits = 0usize;
    let mut exact_8193_horizon_only_hits = 0usize;
    let mut ring_wrapover_past_tracked_mutation = 0usize;
    let mut mutation_only_within_horizon = 0usize;
    let mut empty_past_horizon = 0usize;
    let mut live_true_to_false = 0usize;
    let mut first_write_in_burst_invalidations = 0usize;
    let mut last_write_in_burst_invalidations = 0usize;
    let mut pre_capture_origin_write_survived = 0usize;

    let mut validate_cert = |tracked: &mut TrackedCert,
                             write_log: &[usize],
                             burst_start: usize,
                             round: usize,
                             rng: &mut XorShift64| {
        let total_writes = write_log.len() - tracked.origin_log_index;
        let delta_since_last = write_log.len() - tracked.last_checked_log_index;
        let mutated_in_history = !tracked.deps.is_empty()
            && write_log[tracked.origin_log_index..]
                .iter()
                .any(|bits| tracked.deps.contains(bits));
        let expected = if tracked.deps.is_empty() {
            true
        } else if total_writes > JOURNAL_SIZE {
            false
        } else {
            !mutated_in_history
        };

        let actual = tracked.cert.unchanged();
        assert_eq!(
            actual,
            expected,
            "round={round} total_writes={total_writes} delta={delta_since_last} deps={} mutated={mutated_in_history} mismatch",
            tracked.deps.len()
        );
        if rng.next_usize(2) == 0 {
            assert_eq!(
                tracked.cert.unchanged(),
                expected,
                "repeat same-sequence validation mismatch at round={round}"
            );
        }

        let scanned = tracked.cert.journal_slots_scanned_for_test();
        if tracked.deps.is_empty() {
            assert_eq!(scanned, 0, "empty certificate must scan 0 journal slots");
            if total_writes > JOURNAL_SIZE {
                empty_past_horizon += 1;
            }
        } else {
            assert!(
                scanned <= total_writes.min(JOURNAL_SIZE),
                "incremental validation scanned {scanned} slots (> min({total_writes}, {JOURNAL_SIZE}))"
            );
            if total_writes > JOURNAL_SIZE && !mutated_in_history {
                horizon_only_invalidations += 1;
                if total_writes == JOURNAL_SIZE + 1 {
                    exact_8193_horizon_only_hits += 1;
                }
            } else if total_writes == JOURNAL_SIZE && !mutated_in_history {
                exact_8192_clean_hits += 1;
            } else if total_writes <= JOURNAL_SIZE && mutated_in_history {
                mutation_only_within_horizon += 1;
            }
            if tracked.was_valid_at_last_check
                && !expected
                && delta_since_last > JOURNAL_SIZE
                && write_log[tracked.last_checked_log_index..write_log.len() - JOURNAL_SIZE]
                    .iter()
                    .any(|bits| tracked.deps.contains(bits))
            {
                ring_wrapover_past_tracked_mutation += 1;
            }
            if tracked.was_valid_at_last_check && !expected && total_writes <= JOURNAL_SIZE {
                let burst_len = write_log.len() - burst_start;
                if burst_len >= 2 && tracked.last_checked_log_index == burst_start {
                    let first_mut = tracked.deps.contains(&write_log[burst_start]);
                    let last_mut = tracked.deps.contains(&write_log[write_log.len() - 1]);
                    let middle_mut = write_log[burst_start + 1..write_log.len() - 1]
                        .iter()
                        .any(|bits| tracked.deps.contains(bits));
                    if first_mut && !middle_mut && !last_mut {
                        first_write_in_burst_invalidations += 1;
                    }
                    if last_mut && !first_mut && !middle_mut {
                        last_write_in_burst_invalidations += 1;
                    }
                }
            }
            if tracked.pre_capture_origin_dep_written && expected && total_writes >= 1 {
                pre_capture_origin_write_survived += 1;
            }
        }

        if tracked.ever_checked && tracked.was_valid_at_last_check && !expected {
            live_true_to_false += 1;
        }
        tracked.ever_checked = true;
        tracked.was_valid_at_last_check = expected;
        tracked.last_checked_log_index = write_log.len();
    };

    for round in 0..160 {
        // Capture a fresh certificate with 0..=8 random dependencies from either
        // `mutable_pool` or `quiet_pool`.
        if certs.len() < 12 || rng.next_usize(4) == 0 {
            let mode = rng.next_usize(10);
            let chosen: Vec<Value> = match mode {
                0 => Vec::new(),
                1..=3 => {
                    let count = 1 + rng.next_usize(4);
                    (0..count)
                        .map(|_| quiet_pool[rng.next_usize(quiet_pool.len())])
                        .collect()
                }
                _ => {
                    let count = 1 + rng.next_usize(7);
                    (0..count)
                        .map(|_| mutable_pool[rng.next_usize(mutable_pool.len())])
                        .collect()
                }
            };

            // On ~1/3 of non-empty captures, write chosen[0] right BEFORE capture
            // so `journal[origin % JOURNAL_SIZE] == chosen[0].bits()`, catching
            // any `0..delta` off-by-one scan that inspects the origin slot.
            let pre_capture_origin_dep_written = !chosen.is_empty() && rng.next_usize(3) == 0;
            if pre_capture_origin_dep_written {
                push_write(chosen[0], &mut write_log);
            }

            let (_, captured) = capture(|| {
                let mut acc = 0usize;
                for &cell in &chosen {
                    acc ^= cell.cons_car().bits();
                }
                acc
            });
            let cert = captured.expect("randomized capture");
            let deps: rustc_hash::FxHashSet<usize> = chosen.iter().map(|v| v.bits()).collect();
            if certs.len() >= 18 {
                certs.swap_remove(rng.next_usize(certs.len()));
            }
            let now = write_log.len();
            certs.push(TrackedCert {
                cert,
                deps,
                dep_values: chosen,
                origin_log_index: now,
                last_checked_log_index: now,
                ever_checked: false,
                was_valid_at_last_check: true,
                pre_capture_origin_dep_written,
            });
        }

        let burst_start = write_log.len();
        let mut force_validate_all = false;

        // Choose a randomized workload pattern that covers short/medium/large
        // bursts, exact 8192->8193 horizon crossings, ring-buffer wrapovers
        // past a tracked mutation (`delta > JOURNAL_SIZE`), and burst-endpoint
        // mutations (`step == 0` and `step == burst - 1`).
        match rng.next_usize(12) {
            0 => {
                // 0-write round (tests same-sequence memo hits).
            }
            1..=3 => {
                // Short burst (2..=8 writes); occasionally place a tracked
                // mutation strictly on the first OR last write of a valid live
                // cert to catch `0..delta` / `1..delta` / `2..=delta` off-by-ones.
                let burst = 2 + rng.next_usize(7);
                let live_candidate = certs.iter().position(|c| {
                    !c.dep_values.is_empty()
                        && c.was_valid_at_last_check
                        && c.last_checked_log_index == burst_start
                        && write_log.len() - c.origin_log_index + burst <= JOURNAL_SIZE
                        && c.dep_values
                            .iter()
                            .all(|v| mutable_pool.iter().any(|m| m.bits() == v.bits()))
                });
                let place_mode = if live_candidate.is_some() {
                    rng.next_usize(3)
                } else {
                    2
                };
                let target_dep = live_candidate.map(|idx| {
                    let c = &certs[idx];
                    c.dep_values[rng.next_usize(c.dep_values.len())]
                });
                for step in 0..burst {
                    let target = match (place_mode, target_dep) {
                        (0, Some(dep)) if step == 0 => dep,
                        (1, Some(dep)) if step + 1 == burst => dep,
                        _ => unrelated,
                    };
                    push_write(target, &mut write_log);
                }
                force_validate_all = live_candidate.is_some();
            }
            4..=6 => {
                // Medium burst (16..=192 writes) with sparse random mutations.
                let burst = 16 + rng.next_usize(177);
                for _ in 0..burst {
                    let target = if rng.next_usize(512) == 0 {
                        mutable_pool[rng.next_usize(mutable_pool.len())]
                    } else {
                        unrelated
                    };
                    push_write(target, &mut write_log);
                }
            }
            7..=8 => {
                // Unrelated-only large burst (1024..=3072 writes) advancing clean
                // certificates toward and past JOURNAL_SIZE without mutating them.
                let burst = 1024 + rng.next_usize(2049);
                for _ in 0..burst {
                    push_write(unrelated, &mut write_log);
                }
            }
            9 => {
                // Ring-buffer wrapover burst (`delta > JOURNAL_SIZE`): mutate a
                // live cert's dependency early, then overwrite the entire 8192-slot
                // ring buffer with unrelated writes so the tracked entry is gone
                // from the ring buffer before the next `unchanged()` call.
                let live_idx = certs
                    .iter()
                    .position(|c| !c.dep_values.is_empty() && c.was_valid_at_last_check);
                let early_offset = rng.next_usize(8);
                let tail_unrelated = JOURNAL_SIZE + 1 + rng.next_usize(256);
                for step in 0..=early_offset {
                    let target = if step == early_offset
                        && let Some(idx) = live_idx
                    {
                        let c = &certs[idx];
                        c.dep_values[rng.next_usize(c.dep_values.len())]
                    } else {
                        unrelated
                    };
                    push_write(target, &mut write_log);
                }
                for _ in 0..tail_unrelated {
                    push_write(unrelated, &mut write_log);
                }
                force_validate_all = true;
            }
            _ => {
                // Exact horizon boundary probe: advance an unmutated non-empty
                // cert to exact `total_writes == JOURNAL_SIZE`, validate all,
                // then write 1 more unrelated write to hit `JOURNAL_SIZE + 1`.
                let clean_near = certs.iter().find_map(|c| {
                    if c.deps.is_empty() {
                        return None;
                    }
                    let tw = write_log.len() - c.origin_log_index;
                    let mutated = write_log[c.origin_log_index..]
                        .iter()
                        .any(|b| c.deps.contains(b));
                    (!mutated && tw < JOURNAL_SIZE).then(|| JOURNAL_SIZE - tw)
                });
                if let Some(remaining) = clean_near {
                    for _ in 0..remaining {
                        push_write(unrelated, &mut write_log);
                    }
                    for tracked in &mut certs {
                        validate_cert(tracked, &write_log, burst_start, round, &mut rng);
                    }
                    let step_start = write_log.len();
                    push_write(unrelated, &mut write_log);
                    for tracked in &mut certs {
                        validate_cert(tracked, &write_log, step_start, round, &mut rng);
                    }
                    continue;
                } else {
                    let burst = 256 + rng.next_usize(512);
                    for _ in 0..burst {
                        push_write(unrelated, &mut write_log);
                    }
                }
            }
        }

        // Randomly skip ~1/3 of certs on non-boundary rounds so inter-check
        // deltas also accumulate across multiple rounds.
        for tracked in &mut certs {
            if !force_validate_all && rng.next_usize(3) == 0 {
                continue;
            }
            validate_cert(tracked, &write_log, burst_start, round, &mut rng);
        }
    }

    assert!(
        horizon_only_invalidations >= 3,
        "workload must exercise clean JOURNAL_SIZE horizon crossings without tracked mutations (got {horizon_only_invalidations})"
    );
    assert!(
        exact_8192_clean_hits >= 1 && exact_8193_horizon_only_hits >= 1,
        "workload must hit exact 8192/8193 horizon boundaries (8192={exact_8192_clean_hits}, 8193={exact_8193_horizon_only_hits})"
    );
    assert!(
        ring_wrapover_past_tracked_mutation >= 2,
        "workload must roll the ring buffer past a tracked mutation between validations (got {ring_wrapover_past_tracked_mutation})"
    );
    assert!(
        mutation_only_within_horizon >= 5,
        "workload must exercise in-horizon mutation invalidations (got {mutation_only_within_horizon})"
    );
    assert!(
        empty_past_horizon >= 5,
        "workload must exercise empty certificates staying valid past JOURNAL_SIZE (got {empty_past_horizon})"
    );
    assert!(
        live_true_to_false >= 5,
        "workload must exercise live true->false transitions (got {live_true_to_false})"
    );
    assert!(
        first_write_in_burst_invalidations >= 1 && last_write_in_burst_invalidations >= 1,
        "workload must exercise first-write and last-write burst endpoints (first={first_write_in_burst_invalidations}, last={last_write_in_burst_invalidations})"
    );
    assert!(
        pre_capture_origin_write_survived >= 1,
        "workload must exercise pre-capture writes at the origin slot (got {pre_capture_origin_write_survived})"
    );
}

#[test]
fn warmed_validation_still_observes_dependencies_in_each_outer_capture() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, inner) = capture(|| source.cons_car());
    let inner = inner.expect("inner source certificate");
    unrelated.set_car(Value::T);
    assert!(inner.unchanged());
    for _ in 0..3 {
        let (_, outer) = capture(|| {
            for _ in 0..8 {
                assert!(inner.unchanged_and_observe());
            }
        });
        let outer = outer.expect("transitive outer certificate");
        assert!(outer.contains_owner_for_test(source.bits()));
        assert!(outer.unchanged());
    }
    assert_eq!(
        inner.full_validation_invocations_for_test(),
        1,
        "transitive observation must not repeat the full journal validation"
    );
    let (_, outer) = capture(|| assert!(inner.unchanged_and_observe()));
    source.set_car(Value::T);
    assert!(!outer.expect("last transitive certificate").unchanged());
    assert!(!inner.unchanged());
}

#[test]
fn warmed_validation_never_admits_a_foreign_world() {
    use crate::tagged::gc::{TaggedHeap, set_tagged_heap};
    use crate::tagged::value::TaggedValue;

    let mut first = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut first);
    let source = first.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let unrelated = first.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("first World certificate");
    unrelated.set_car(TaggedValue::T);
    assert!(reads.unchanged());
    let original_sequence = LispCollectionRevision::current();

    let mut second = Box::new(TaggedHeap::new());
    // SAFETY: bare `TaggedHeap` unit test switching heaps on this thread.
    unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(&first) };
    set_tagged_heap(&mut second);
    let other = second.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    other.set_car(TaggedValue::T);
    assert_eq!(LispCollectionRevision::current(), original_sequence);
    let (_, foreign) = capture(|| {
        for _ in 0..8 {
            assert!(!reads.unchanged_and_observe());
        }
    });
    assert!(foreign.expect("foreign capture").reads_empty_for_test());

    // SAFETY: bare `TaggedHeap` unit test restoring `first`.
    unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(&second) };
    set_tagged_heap(&mut first);
    assert!(reads.unchanged_and_observe());
    assert_eq!(reads.full_validation_invocations_for_test(), 1);
    source.set_car(TaggedValue::NIL);
    assert!(!reads.unchanged());
    assert_eq!(reads.full_validation_invocations_for_test(), 2);
}
