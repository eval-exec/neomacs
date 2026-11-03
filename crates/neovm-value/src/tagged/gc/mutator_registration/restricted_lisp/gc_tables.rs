//! B6: only shared tables retain the owner-created graphs at a rendezvous.
use super::*;
use crate::tagged::gc::ProofEra as ParallelHeapEra;

fn every_stop(_: usize, _: usize) -> Option<Go> {
    Some(Go::Forced)
}

fn program(index: usize) -> Vec<Op> {
    vec![
        Op::ConstFixnum {
            dst: 2,
            value: 100 + index as i64,
        },
        Op::ConstNil { dst: 5 },
        Op::Cons {
            dst: 3,
            car: 2,
            cdr: 5,
        },
        Op::Cons {
            dst: 4,
            car: 2,
            cdr: 5,
        },
        Op::Cons {
            dst: 4,
            car: 4,
            cdr: 5,
        },
        // The first table retains a transitive value graph; the second also
        // retains an independent cons key through its key snapshot. Every
        // owner shares and edits both tables, racing their remembered claims.
        Op::HtPut {
            table: 0,
            key: 2,
            value: 4,
        },
        Op::HtPut {
            table: 1,
            key: 3,
            value: 4,
        },
        Op::ConstNil { dst: 3 },
        Op::ConstNil { dst: 4 },
        Op::Safepoint,
        // A returned answer is installed into the owner's root file before
        // its next poll, then survives a second collection in that home.
        Op::ConstFixnum {
            dst: 2,
            value: 100 + ((index + 1) % 4) as i64,
        },
        Op::HtGet {
            table: 0,
            key: 2,
            found: 5,
            value: 4,
        },
        Op::Safepoint,
        Op::HtClear { table: 0 },
        Op::HtClear { table: 1 },
        Op::ConstNil { dst: 4 },
        Op::ConstNil { dst: 5 },
        Op::Safepoint,
    ]
}

fn owns(era: &ParallelHeapEra<'_>, stopped: &AllStopped<'_>, snaps: &[Snap], expected: bool) {
    on_heap(era, stopped, |heap| {
        for snap in snaps {
            assert_eq!(snap.conses.len(), 3);
            for &bits in &snap.conses {
                assert_eq!(
                    heap.owns_heap_value_for_test(TaggedValue::from_bits(bits)),
                    expected,
                    "owner {} graph word {bits:#x}",
                    snap.index
                );
            }
        }
    });
}

fn shared_table_gc(old_table: bool) {
    const N: usize = 4;
    const NSLOTS: usize = 6;
    let mut context = Context::new();
    let generational = context.tagged_heap.generational_enabled();
    let tables = context.eval_str("(progn (setq a6-gc-t0 (make-hash-table :test 'eq) a6-gc-t1 (make-hash-table :test 'eq)) (list a6-gc-t0 a6-gc-t1))").unwrap();
    // Only the shells are transport roots; no graph node has a SharedRoot.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let mut planted = vec![None; NSLOTS];
    let mut cursor = tables;
    for home in &mut planted[..2] {
        *home =
            Some(unsafe { SharedRoot::new(&context.tagged_heap, cursor.cons_car_unobserved()) });
        cursor = cursor.cons_cdr_unobserved();
    }
    if old_table {
        context.eval_str("(garbage-collect)").unwrap();
        // GEN0 runs the same row as the strong-table control. GEN1 must
        // actually start with old shells, before any young owner allocation.
        if generational {
            for name in ["a6-gc-t0", "a6-gc-t1"] {
                let table = context.eval_str(name).unwrap();
                let ptr = table.as_veclike_ptr().unwrap();
                assert!(unsafe { (*ptr).gc.tenured }, "old-table setup must promote");
            }
        }
    }
    let started = AtomicUsize::new(0);
    let programs: Vec<_> = (0..N).map(program).collect();
    for ops in &programs {
        try_admit(
            &ops.iter().copied().map(Insn::Allowed).collect::<Vec<_>>(),
            NSLOTS,
            0,
            &[(0, Abs::Table), (1, Abs::Table)],
            &started,
        )
        .unwrap();
    }
    let abort = AtomicBool::new(false);
    let overlap = Overlap::new(N, true);
    context
        .admit_owned_proof_parallel(|projection| {
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
                    let mut go_txs = Vec::new();
                    let mut done_txs = Vec::new();
                    let permits: Vec<_> = (0..N)
                        .map(|i| retry_reg(|| era.register(9100 + i as u64, NSLOTS)))
                        .collect();
                    for (index, permit) in permits.into_iter().enumerate() {
                        let (go_tx, go_rx) = channel();
                        let (done_tx, done_rx) = channel();
                        go_txs.push(go_tx);
                        done_txs.push(done_tx);
                        let launch = OwnerLaunch {
                            permit,
                            heap: era.heap_identity(),
                            id: 9100 + index as u64,
                            nil: &nil,
                            programs: vec![programs[index].clone()],
                            nslots: NSLOTS,
                            bound: 0,
                            line_slot: 4,
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
                            ht: None,
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
                    let graphs = session.pauses();
                    assert_eq!(overlap.entered.load(Ordering::SeqCst), N);
                    assert_eq!(overlap.allocated.load(Ordering::SeqCst), N);
                    for seen in &overlap.seen_alloc {
                        assert_eq!(seen.load(Ordering::SeqCst), N);
                    }
                    for snap in &graphs {
                        assert_eq!(snap.slots[3], TaggedValue::NIL.bits());
                        assert_eq!(snap.slots[4], TaggedValue::NIL.bits());
                    }
                    let mut stopped =
                        open_collection(era, &session.go_txs, Go::Forced, &mut session.next_epoch);
                    // Observe the real per-owner published logs, before the major
                    // drain could hide a missing barrier by tracing table roots.
                    let receipts = retry_reg(|| era.stopped_summaries()).census().unwrap();
                    // Admission may collect while retiring the bootstrap weak
                    // table, promoting shells even in the general survival row.
                    // Read their actual ages before this row's first collection.
                    let old_shells = on_heap(era, &stopped, |_heap| {
                        graphs[0].slots[..2]
                            .iter()
                            .filter(|&&bits| {
                                let ptr = TaggedValue::from_bits(bits).as_veclike_ptr().unwrap();
                                // SAFETY: owner homes retain both shells and all
                                // owners are stopped; this observes only age.
                                unsafe { (*ptr).gc.tenured }
                            })
                            .count()
                    });
                    assert_eq!(receipts.remembered_words, old_shells);
                    if old_table && generational {
                        assert_eq!(old_shells, 2, "old-table row requires old shells");
                    }
                    if !generational {
                        assert_eq!(old_shells, 0, "GEN0 has no promoted shells");
                    }
                    collect_major(era, &mut stopped);
                    owns(era, &stopped, &graphs, true);
                    release_cycle(era, stopped, true);

                    let fetched = session.pauses();
                    for snap in &fetched {
                        assert_eq!(snap.slots[5], TaggedValue::T.bits());
                        assert!(graphs[(snap.index + 1) % N].conses.contains(&snap.slots[4]));
                        assert_eq!(
                            prin1(snap.slots[4]),
                            format!("(({}))", 100 + ((snap.index + 1) % N))
                        );
                    }
                    session.round(Go::Forced, true, |era, stopped| {
                        owns(era, stopped, &graphs, true)
                    });

                    let cleared = session.pauses();
                    for snap in &cleared {
                        assert_eq!(snap.slots[4], TaggedValue::NIL.bits());
                    }
                    session.round(Go::Forced, true, |era, stopped| {
                        owns(era, stopped, &graphs, false)
                    });
                    let ends =
                        session.finish_owners(|era, stopped| owns(era, stopped, &graphs, false));
                    assert_eq!(ends.len(), N);
                    assert!(
                        ends.iter()
                            .all(|end| end.cons_count == 3 && end.safepoints == 3)
                    );
                });
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
    assert_eq!(
        context
            .eval_str("(+ (hash-table-count a6-gc-t0) (hash-table-count a6-gc-t1))")
            .unwrap(),
        TaggedValue::fixnum(0)
    );
}

#[test]
fn a6_ht_gc_shared_roots_survive_and_clrhash_reclaims() {
    shared_table_gc(false);
}

#[test]
fn a6_ht_gc_old_tables_remember_young_values() {
    shared_table_gc(true);
}
