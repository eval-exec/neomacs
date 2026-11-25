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
        origin_log_index: usize,
    }

    let pool: Vec<Value> = (0..24)
        .map(|i| Value::cons(Value::fixnum(i), Value::NIL))
        .collect();
    let unrelated = Value::cons(Value::NIL, Value::NIL);

    let mut rng = XorShift64(0x9E37_79B9_7F4A_7C15);
    let mut write_log: Vec<usize> = Vec::new();
    let mut certs: Vec<TrackedCert> = Vec::new();

    for round in 0..160 {
        // Capture a fresh certificate with 0..=8 random dependencies from `pool`.
        if certs.len() < 12 || rng.next_usize(4) == 0 {
            let dep_count = match rng.next_usize(10) {
                0 => 0,
                1..=3 => 1,
                _ => 2 + rng.next_usize(7),
            };
            let mut chosen = Vec::with_capacity(dep_count);
            for _ in 0..dep_count {
                chosen.push(pool[rng.next_usize(pool.len())]);
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
            if certs.len() >= 16 {
                certs.swap_remove(rng.next_usize(certs.len()));
            }
            certs.push(TrackedCert {
                cert,
                deps,
                origin_log_index: write_log.len(),
            });
        }

        // Perform a random burst of writes (mostly unrelated so certificates stay
        // valid long enough to exercise multi-step deltas and horizon crossings,
        // interspersed with real mutations to `pool` entries).
        let burst = match rng.next_usize(10) {
            0 => 0,
            1..=4 => 1 + rng.next_usize(5),
            5..=7 => 16 + rng.next_usize(128),
            8 => 1024 + rng.next_usize(2048),
            _ => JOURNAL_SIZE / 2 + rng.next_usize(128),
        };
        for step in 0..burst {
            let target = if rng.next_usize(64) == 0 {
                pool[rng.next_usize(pool.len())]
            } else {
                unrelated
            };
            target.set_car(Value::fixnum((round * 10_000 + step) as i64));
            write_log.push(target.bits());
        }

        // Validate every live certificate (including a duplicate same-sequence
        // check on some) and assert exact equivalence with a naive full rescan
        // of `(origin_log_index .. write_log.len())` enforcing `JOURNAL_SIZE`.
        for tracked in &certs {
            let total_writes = write_log.len() - tracked.origin_log_index;
            let expected = if tracked.deps.is_empty() {
                true
            } else if total_writes > JOURNAL_SIZE {
                false
            } else {
                write_log[tracked.origin_log_index..]
                    .iter()
                    .all(|bits| !tracked.deps.contains(bits))
            };
            let actual = tracked.cert.unchanged();
            assert_eq!(
                actual,
                expected,
                "round={round} total_writes={total_writes} deps={} mismatch",
                tracked.deps.len()
            );
            if rng.next_usize(2) == 0 {
                assert_eq!(
                    tracked.cert.unchanged(),
                    expected,
                    "repeat same-sequence validation mismatch at round={round}"
                );
            }
            assert!(
                tracked.cert.journal_slots_scanned_for_test() <= JOURNAL_SIZE,
                "incremental validation scanned {} slots (> JOURNAL_SIZE={JOURNAL_SIZE})",
                tracked.cert.journal_slots_scanned_for_test()
            );
        }
    }
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

    set_tagged_heap(&mut first);
    assert!(reads.unchanged_and_observe());
    assert_eq!(reads.full_validation_invocations_for_test(), 1);
    source.set_car(TaggedValue::NIL);
    assert!(!reads.unchanged());
    assert_eq!(reads.full_validation_invocations_for_test(), 2);
}
