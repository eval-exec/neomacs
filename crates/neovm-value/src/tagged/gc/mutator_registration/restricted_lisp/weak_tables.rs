//! B8: shared weak entries are resolved only by the stopped keeper.
use super::super::KeeperTableMut;
use super::*;
use crate::tagged::gc::ProofEra as ParallelHeapEra;

const NSLOTS: usize = 64;
const KEY: u16 = 50;
const VALUE: u16 = 51;
const NIL: u16 = 52;
const NUMBER: u16 = 53;
const FOUND: u16 = 56;
const MISSING: u16 = 57;
const ANSWER: u16 = 58;

fn every_stop(_: usize, _: usize) -> Option<Go> {
    Some(Go::Forced)
}

/// Serial transport roots retain TABLES ONLY. All weak payloads are actual
/// owner allocations; the snapshots below contain inert bits, not GC roots.
fn session(
    setup: &str,
    programs: Vec<Vec<Op>>,
    env: &HtEnv<'_>,
    body: impl FnOnce(&mut Session<'_, '_>, &Overlap, &[usize]),
) {
    let n = programs.len();
    let mut context = Context::new();
    let mut cursor = context.eval_str(setup).expect("B8 serial weakness setup");
    let mut tables = Vec::new();
    while cursor.is_cons() {
        tables.push(cursor.cons_car_unobserved());
        cursor = cursor.cons_cdr_unobserved();
    }
    assert!(cursor.is_nil());
    let bits: Vec<_> = tables.iter().map(|table| table.bits()).collect();
    let facts: Vec<_> = (0..tables.len())
        .map(|index| (index as u16, Abs::Table))
        .collect();
    for ops in &programs {
        prevalidate_with(ops, NSLOTS, 0, STEP_CAP, &facts)
            .unwrap_or_else(|error| panic!("B8 prevalidate: {error:?}"));
    }
    // SAFETY: serial Context owns these table shells and nil. Payloads are
    // deliberately absent from transport roots and frozen legacy variables.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let mut planted = vec![None; NSLOTS];
    for (index, &table) in tables.iter().enumerate() {
        planted[index] = Some(unsafe { SharedRoot::new(&context.tagged_heap, table) });
    }
    let abort = AtomicBool::new(false);
    let overlap = Overlap::new(n, true);
    context
        .admit_owned_parallel_with_weak_tables_for_test(&tables, |projection| {
            {
                let era = projection.era();
                era.start_cons_storage().unwrap();
                thread::scope(|scope| {
                    let _panic = PanicFlag {
                        registry: era.registry(),
                        abort: &abort,
                    };
                    let (pause_tx, pause_rx) = channel();
                    let (end_tx, end_rx) = channel();
                    let (ready_tx, ready_rx) = channel();
                    let (retired_tx, retired_rx) = channel();
                    let permits: Vec<_> = (0..n)
                        .map(|index| retry_reg(|| era.register(9800 + index as u64, NSLOTS)))
                        .collect();
                    let mut go_txs = Vec::new();
                    let mut done_txs = Vec::new();
                    for (index, permit) in permits.into_iter().enumerate() {
                        let (go_tx, go_rx) = channel();
                        let (done_tx, done_rx) = channel();
                        go_txs.push(go_tx);
                        done_txs.push(done_tx);
                        let launch = OwnerLaunch {
                            permit,
                            heap: era.heap_identity(),
                            id: 9800 + index as u64,
                            nil: &nil,
                            programs: vec![programs[index].clone()],
                            nslots: NSLOTS,
                            bound: 0,
                            line_slot: usize::from(NIL),
                            stop_at: every_stop,
                            overlap: Some(&overlap),
                            index,
                            pause_tx: pause_tx.clone(),
                            go_rx,
                            end_tx: end_tx.clone(),
                            ready_tx: ready_tx.clone(),
                            done_rx,
                            retired_tx: retired_tx.clone(),
                            registry: era.registry(),
                            abort: &abort,
                            planted: &planted,
                            ht: Some(env),
                        };
                        scope.spawn(move || launch.run());
                    }
                    drop((pause_tx, end_tx, ready_tx, retired_tx));
                    let mut session = Session {
                        era,
                        go_txs,
                        done_txs,
                        pause_rx,
                        end_rx,
                        ready_rx,
                        retired_rx,
                        next_epoch: 1,
                    };
                    body(&mut session, &overlap, &bits);
                });
            }
            let (collector, keeper) = projection.finish().expect("B8 finish");
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("B8 weak admission: {error:?}"));
}

fn counts(stopped: &AllStopped<'_>, tables: &[usize]) -> Vec<usize> {
    tables
        .iter()
        .map(|&bits| {
            KeeperTableMut::enter(stopped, TaggedValue::from_bits(bits))
                .unwrap()
                .inspect(|table| table.data.len())
        })
        .collect()
}

fn live_payloads(era: &ParallelHeapEra<'_>, stopped: &AllStopped<'_>, snaps: &[Snap]) -> usize {
    // Freed slots may be reused by the next allocation batch. Count current
    // addresses once rather than counting every historical birth at that slot.
    let words: HashSet<_> = snaps
        .iter()
        .flat_map(|snap| snap.conses.iter().copied())
        .collect();
    live_words(era, stopped, words.iter().copied())
}

fn live_words(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    words: impl IntoIterator<Item = usize>,
) -> usize {
    let words: Vec<_> = words.into_iter().collect();
    on_heap(era, stopped, |heap| {
        words
            .iter()
            .filter(|&&bits| heap.owns_heap_value_for_test(TaggedValue::from_bits(bits)))
            .count()
    })
}

fn witnessed(overlap: &Overlap) {
    assert_eq!(overlap.entered.load(Ordering::SeqCst), overlap.n);
    assert_eq!(overlap.allocated.load(Ordering::SeqCst), overlap.n);
    for seen in &overlap.seen_alloc {
        assert_eq!(seen.load(Ordering::SeqCst), overlap.n);
    }
}

fn clear(ops: &mut Vec<Op>, slots: impl IntoIterator<Item = u16>) {
    ops.extend(slots.into_iter().map(|dst| Op::ConstNil { dst }));
}

const MATRIX_SETUP: &str = r#"(list
  (make-hash-table :test 'eq :weakness 'key)
  (make-hash-table :test 'eq :weakness 'value)
  (make-hash-table :test 'eq :weakness 'key-or-value)
  (make-hash-table :test 'eq :weakness 'key-and-value))"#;

fn matrix_program(owner: usize) -> Vec<Op> {
    let mut ops = vec![Op::ConstNil { dst: NIL }];
    for table in 0..4u16 {
        for state in 0..4u16 {
            let key = 8 + 8 * table + 2 * state;
            let value = key + 1;
            ops.extend([
                Op::ConstFixnum {
                    dst: NUMBER,
                    value: (1000 * owner + usize::from(key)) as i64,
                },
                Op::Cons {
                    dst: key,
                    car: NUMBER,
                    cdr: NIL,
                },
                Op::Cons {
                    dst: value,
                    car: NUMBER,
                    cdr: NIL,
                },
                Op::HtPut { table, key, value },
            ]);
            // States are key only, value only, both, neither. Independent
            // roots on every owner contribute to one global mark relation.
            if matches!(state, 1 | 3) {
                clear(&mut ops, [key]);
            }
            if matches!(state, 0 | 3) {
                clear(&mut ops, [value]);
            }
        }
    }
    ops.extend([Op::Safepoint, Op::Safepoint]);
    clear(&mut ops, 8..40);
    ops.extend([Op::Safepoint, Op::Safepoint]);
    ops
}

#[test]
fn a6_ht_b8_four_weaknesses_shared_root_union_and_reclamation() {
    for n in 2..=4 {
        let env = HtEnv::new(&[], 60, 61, 62);
        session(
            MATRIX_SETUP,
            (0..n).map(matrix_program).collect(),
            &env,
            |session, overlap, tables| {
                let payloads = session.pauses();
                witnessed(overlap);
                assert!(payloads.iter().all(|snap| snap.cons_count == 32));
                session.round(Go::Forced, true, |_, _| {});
                session.pauses();
                // An idle major drains the previous cycle's conservative
                // owner write obligations before exact weak assertions.
                session.round(Go::Forced, true, |era, stopped| {
                    assert_eq!(counts(stopped, tables), [2 * n, 2 * n, 3 * n, n]);
                    // A rooted half of a rejected entry stays alive too:
                    // key/value/or/and retain 5/5/6/4 individual conses.
                    assert_eq!(live_payloads(era, stopped, &payloads), 20 * n);
                });
                session.pauses();
                session.round(Go::Forced, true, |_, _| {});
                session.pauses();
                session.round(Go::Forced, true, |era, stopped| {
                    assert_eq!(counts(stopped, tables), [0; 4]);
                    assert_eq!(live_payloads(era, stopped, &payloads), 0);
                });
                let ends = session.finish_owners(|era, stopped| {
                    assert_eq!(counts(stopped, tables), [0; 4]);
                    assert_eq!(live_payloads(era, stopped, &payloads), 0);
                });
                assert!(ends.iter().all(|end| end.safepoints == 4));
            },
        );
    }
}

const CHAIN_SETUP: &str = r#"(list
  (make-hash-table :test 'eq :weakness 'key)
  (make-hash-table :test 'eq :weakness 'key)
  (make-hash-table :test 'eq :weakness 'value))"#;

fn chain_program(owner: usize, owners: usize) -> Vec<Op> {
    let mut ops = vec![Op::ConstNil { dst: NIL }];
    for dst in 8..15u16 {
        ops.extend([
            Op::ConstFixnum {
                dst: NUMBER,
                value: (1000 * owner + usize::from(dst)) as i64,
            },
            Op::Cons {
                dst,
                car: NUMBER,
                cdr: NIL,
            },
        ]);
    }
    // A->B->C->D->E alternates tables. Either table order requires another
    // fixpoint pass. The unrooted X<->Y cycle must never bootstrap itself.
    for (table, key, value) in [
        (0, 8, 9),
        (0, 10, 11),
        (1, 9, 10),
        (1, 11, 12),
        (0, 13, 14),
        (1, 14, 13),
    ] {
        ops.push(Op::HtPut { table, key, value });
    }
    // A weak-value bridge lets another owner take over A's only strong
    // root through an ordinary lookup, without a raw cross-owner slot edit.
    ops.extend([
        Op::ConstFixnum {
            dst: KEY,
            value: owner as i64,
        },
        Op::HtPut {
            table: 2,
            key: KEY,
            value: 8,
        },
    ]);
    clear(&mut ops, 9..15);
    ops.extend([
        Op::Safepoint,
        Op::ConstFixnum {
            dst: KEY,
            value: ((owner + 1) % owners) as i64,
        },
        Op::HtGet {
            table: 2,
            key: KEY,
            found: FOUND,
            value: 8,
        },
        Op::Safepoint,
    ]);
    clear(&mut ops, [8]);
    ops.extend([Op::Safepoint, Op::Safepoint]);
    ops
}

#[test]
fn a6_ht_b8_cross_table_fixpoint_and_unrooted_cycles() {
    const N: usize = 3;
    let env = HtEnv::new(&[], 60, 61, 62);
    session(
        CHAIN_SETUP,
        (0..N).map(|owner| chain_program(owner, N)).collect(),
        &env,
        |session, overlap, tables| {
            let payloads = session.pauses();
            witnessed(overlap);
            session.round(Go::Forced, true, |_, _| {});
            let transferred = session.pauses();
            for (index, snap) in transferred.iter().enumerate() {
                assert_eq!(snap.slots[8], payloads[(index + 1) % N].slots[8]);
                assert_eq!(snap.slots[usize::from(FOUND)], TaggedValue::T.bits());
            }
            session.round(Go::Forced, true, |era, stopped| {
                assert_eq!(counts(stopped, tables), [2 * N, 2 * N, N]);
                assert_eq!(live_payloads(era, stopped, &payloads), 5 * N);
            });
            session.pauses();
            session.round(Go::Forced, true, |_, _| {});
            session.pauses();
            session.round(Go::Forced, true, |era, stopped| {
                assert_eq!(counts(stopped, tables), [0; 3]);
                assert_eq!(live_payloads(era, stopped, &payloads), 0);
            });
            session.finish_owners(|_, stopped| assert_eq!(counts(stopped, tables), [0; 3]));
        },
    );
}

const MEMO_SETUP: &str = r#"(progn
  (define-hash-table-test 'a6-b8-collisions
    (lambda (a b) (eq a b)) (lambda (_key) 7))
  (list (make-hash-table :test 'a6-b8-collisions :weakness 'value :size 4)
        (make-hash-table :test 'a6-b8-collisions :weakness 'value :size 4)
        (make-hash-table :test 'a6-b8-collisions :weakness 'value :size 4)
        (make-hash-table :test 'a6-b8-collisions :weakness 'value :size 4)))"#;

fn memo_batch(ops: &mut Vec<Op>, owner: usize, batch: usize) {
    for index in 0..24usize {
        ops.extend([
            Op::ConstFixnum {
                dst: KEY,
                value: (owner * 1000 + batch * 24 + index) as i64,
            },
            Op::Cons {
                dst: VALUE,
                car: KEY,
                cdr: NIL,
            },
            Op::HtPut {
                table: owner as u16,
                key: KEY,
                value: VALUE,
            },
        ]);
        if index % 8 == 0 {
            ops.push(Op::Move {
                dst: 8 + (index / 8) as u16,
                src: VALUE,
            });
        }
        clear(ops, [VALUE]);
    }
    // All owners can reach and inspect every custom weak shell. Mutating
    // a table while another owner runs its user test is deliberately refused
    // by the accepted B3 test_depth rule, so each table has one writer.
    for table in 0..4 {
        ops.push(Op::HtCount { table, dst: 54 });
    }
}

fn memo_program(owner: usize) -> Vec<Op> {
    let mut ops = vec![Op::ConstNil { dst: NIL }];
    memo_batch(&mut ops, owner, 0);
    // The callback scratch homes are roots too: remove the final candidate
    // and key before deciding which entries are independently reachable.
    clear(&mut ops, [60, 61, 62]);
    ops.extend([Op::Safepoint, Op::Safepoint]);
    ops.extend([
        Op::ConstFixnum {
            dst: KEY,
            value: (owner * 1000) as i64,
        },
        Op::HtGet {
            table: owner as u16,
            key: KEY,
            found: FOUND,
            value: ANSWER,
        },
        Op::ConstFixnum {
            dst: KEY,
            value: (owner * 1000 + 1) as i64,
        },
        Op::HtGet {
            table: owner as u16,
            key: KEY,
            found: MISSING,
            value: ANSWER,
        },
    ]);
    clear(&mut ops, 8..11);
    memo_batch(&mut ops, owner, 1);
    clear(&mut ops, [ANSWER, 60, 61, 62]);
    ops.extend([Op::Safepoint, Op::Safepoint]);
    clear(&mut ops, 8..11);
    clear(&mut ops, [60, 61, 62]);
    ops.extend([Op::Safepoint, Op::Safepoint]);
    ops
}

fn exact_memos(stopped: &AllStopped<'_>, table: usize, expected: usize) {
    let lease = KeeperTableMut::enter(stopped, TaggedValue::from_bits(table)).unwrap();
    lease.inspect(|table| {
        assert_eq!(table.data.len(), expected);
        let live = table.data.live_hash_keys_in_slot_order();
        let memo = table.data.remembered_user_hash_keys();
        assert_eq!(memo.len(), live.len(), "weak sweep left a stale user memo");
        assert!(memo.iter().all(|key| table.data.contains_key(key)));
        assert!(live.iter().all(|key| table.data.user_hash(key) == Some(7)));
        let candidates = table.data.user_candidates(7);
        assert_eq!(
            candidates.len(),
            live.len(),
            "weak sweep left a stale bucket"
        );
        assert!(candidates.iter().all(|key| table.data.contains_key(key)));
        assert!(!table.data.user_hash_incomplete());
        assert!(table.user_cmp_function.is_some());
        assert!(table.user_hash_function.is_some());
    });
}

#[test]
fn a6_ht_b8_removal_heavy_custom_hash_memos_and_slot_reuse() {
    const N: usize = 4;
    let hash_ops = [Op::ConstFixnum { dst: 62, value: 7 }];
    let cmp_ops = [Op::Eq {
        dst: 62,
        a: 60,
        b: 61,
    }];
    let users: Vec<_> = (0..N)
        .map(|index| UserTestProg {
            slot: index as u16,
            hash_ops: &hash_ops,
            cmp_ops: &cmp_ops,
        })
        .collect();
    let env = HtEnv::new(&users, 60, 61, 62);
    session(
        MEMO_SETUP,
        (0..N).map(memo_program).collect(),
        &env,
        |session, overlap, tables| {
            let first = session.pauses();
            witnessed(overlap);
            session.round(Go::Forced, true, |_, _| {});
            session.pauses();
            session.round(Go::Forced, true, |era, stopped| {
                for &table in tables {
                    exact_memos(stopped, table, 3);
                }
                assert_eq!(live_payloads(era, stopped, &first), 3 * N);
            });
            // These specific first-batch survivors remain allocated until
            // after batch two is built, so their addresses cannot be reused
            // by that batch. Other historical first-batch addresses can be.
            let old_survivors: Vec<_> = first
                .iter()
                .flat_map(|snap| snap.slots[8..11].iter().copied())
                .collect();
            let second = session.pauses();
            for snap in &second {
                assert_eq!(snap.slots[usize::from(FOUND)], TaggedValue::T.bits());
                assert_eq!(snap.slots[usize::from(MISSING)], TaggedValue::NIL.bits());
                assert_eq!(snap.cons_count, 48);
            }
            session.round(Go::Forced, true, |_, _| {});
            session.pauses();
            session.round(Go::Forced, true, |era, stopped| {
                for &table in tables {
                    exact_memos(stopped, table, 3);
                }
                assert_eq!(live_words(era, stopped, old_survivors.iter().copied()), 0);
                assert_eq!(live_payloads(era, stopped, &second), 3 * N);
            });
            session.pauses();
            session.round(Go::Forced, true, |_, _| {});
            session.pauses();
            session.round(Go::Forced, true, |era, stopped| {
                for &table in tables {
                    exact_memos(stopped, table, 0);
                }
                assert_eq!(live_payloads(era, stopped, &second), 0);
            });
            session.finish_owners(|_, stopped| {
                for &table in tables {
                    exact_memos(stopped, table, 0);
                }
            });
        },
    );
}
