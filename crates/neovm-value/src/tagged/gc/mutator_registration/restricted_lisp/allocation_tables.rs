//! B7: made/copied shells, their payload roots, and refreshed birth parity.
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
        Op::HtMake {
            dst: 0,
            test: HashTableTest::Eq,
            size: 0,
        },
        Op::HtPut {
            table: 0,
            key: 3,
            value: 4,
        },
        Op::HtCopy { table: 0, dst: 1 },
        Op::HtClear { table: 0 },
        Op::ConstNil { dst: 3 },
        Op::ConstNil { dst: 4 },
        Op::Safepoint,
        Op::HtCopy { table: 1, dst: 1 },
        Op::Safepoint,
        Op::HtClear { table: 1 },
        Op::ConstNil { dst: 0 },
        Op::ConstNil { dst: 1 },
        Op::Safepoint,
    ]
}

fn check_graphs(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    snaps: &[Snap],
    expected: bool,
) {
    on_heap(era, stopped, |heap| {
        for snap in snaps {
            assert_eq!(snap.conses.len(), 3);
            for &bits in &snap.conses {
                assert_eq!(
                    heap.owns_heap_value_for_test(TaggedValue::from_bits(bits)),
                    expected
                );
            }
        }
    });
}

fn check_shells(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    snaps: &[Snap],
    live: bool,
    both_born: bool,
) {
    on_heap(era, stopped, |heap| {
        let mut addresses = HashSet::new();
        let mut births = HashSet::new();
        for snap in snaps {
            for (index, &bits) in snap.slots[..2].iter().enumerate() {
                let value = TaggedValue::from_bits(bits);
                assert_eq!(heap.owns_heap_value_for_test(value), live);
                if !live {
                    continue;
                }
                let header = value.as_veclike_ptr().unwrap();
                let address = header as usize;
                assert!(
                    addresses.insert(address),
                    "every owner gets distinct shells"
                );
                assert!(heap.non_cons_object_addrs.contains(&address));
                // SAFETY: the stopped owner root and address set retain this
                // shell; only immutable header facts are read outside a lease.
                unsafe {
                    assert!((*header).gc.flags.boxed());
                    if both_born || index == 1 {
                        births.insert(address);
                        assert!((*header).gc.is_marked_at(heap.mark_parity));
                    }
                }
            }
        }
        if live {
            let mut linked = HashSet::new();
            let mut young = HashSet::new();
            for (index, head) in [heap.all_objects, heap.generational.old_objects]
                .into_iter()
                .enumerate()
            {
                let mut current = head;
                while !current.is_null() {
                    assert!(
                        linked.insert(current as usize),
                        "disjoint acyclic boxed lists"
                    );
                    if index == 0 {
                        young.insert(current as usize);
                    }
                    // SAFETY: stopped heap owns every node on both lists.
                    current = unsafe { (*current).next };
                }
            }
            // Fresh shells always join all_objects. After a GEN1 collection,
            // the unchanged source shell may instead be on the old list.
            assert!(births.is_subset(&young));
            assert!(addresses.is_subset(&linked));
        }
    });
}

#[test]
fn a6_ht_b7_make_copy_collect_resume_and_reclaim() {
    const N: usize = 4;
    const NSLOTS: usize = 6;
    let mut context = Context::new();
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let planted = vec![None; NSLOTS];
    let started = AtomicUsize::new(0);
    let programs: Vec<_> = (0..N).map(program).collect();
    for ops in &programs {
        try_admit(
            &ops.iter().copied().map(Insn::Allowed).collect::<Vec<_>>(),
            NSLOTS,
            0,
            &[],
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
                        .map(|i| retry_reg(|| era.register(9700 + i as u64, NSLOTS)))
                        .collect();
                    for (index, permit) in permits.into_iter().enumerate() {
                        let (go_tx, go_rx) = channel();
                        let (done_tx, done_rx) = channel();
                        go_txs.push(go_tx);
                        done_txs.push(done_tx);
                        let launch = OwnerLaunch {
                            permit,
                            heap: era.heap_identity(),
                            id: 9700 + index as u64,
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
                    // The frozen legacy image is not a substitute for owner
                    // accounting. Each actual owner published three conses and
                    // two shells, including the copied storage charge.
                    {
                        let gate = retry_reg(|| era.registry().lock());
                        for entry in gate.entries.iter().skip(1) {
                            let state = entry.state.lock().unwrap();
                            let summary = state.image.as_ref().unwrap().summary();
                            assert_eq!(summary.allocated_count, 5);
                            assert_eq!(
                                summary.memory_use_counts
                                    [crate::tagged::gc::MemoryUseCountSlot::VectorCells.index()],
                                2
                            );
                        }
                    }
                    check_shells(era, &stopped, &graphs, true, true);
                    collect_major(era, &mut stopped);
                    check_graphs(era, &stopped, &graphs, true);
                    release_cycle(era, stopped, true);

                    // A new self-copy after the first GC checks both aliasing
                    // homes and the refreshed catalog parity on resume.
                    let recopied = session.pauses();
                    let mut stopped =
                        open_collection(era, &session.go_txs, Go::Forced, &mut session.next_epoch);
                    check_shells(era, &stopped, &recopied, true, false);
                    for (first, second) in graphs.iter().zip(&recopied) {
                        assert_ne!(first.slots[1], second.slots[1]);
                    }
                    collect_major(era, &mut stopped);
                    check_graphs(era, &stopped, &graphs, true);
                    on_heap(era, &stopped, |heap| {
                        for snap in &graphs {
                            assert!(
                                !heap.owns_heap_value_for_test(TaggedValue::from_bits(
                                    snap.slots[1]
                                )),
                                "the replaced copy shell must be reclaimed"
                            );
                        }
                    });
                    release_cycle(era, stopped, true);

                    let cleared = session.pauses();
                    for snap in &cleared {
                        assert_eq!(snap.slots[0], TaggedValue::NIL.bits());
                        assert_eq!(snap.slots[1], TaggedValue::NIL.bits());
                    }
                    session.round(Go::Forced, true, |era, stopped| {
                        check_graphs(era, stopped, &graphs, false);
                        check_shells(era, stopped, &graphs, false, false);
                        on_heap(era, stopped, |heap| {
                            for snap in &recopied {
                                // GEN1 clear logs the old owner. The stopped
                                // collector deliberately roots that pending
                                // obligation for this cycle, then drains it.
                                // Its empty payload no longer keeps the graph.
                                assert_eq!(
                                    heap.owns_heap_value_for_test(TaggedValue::from_bits(
                                        snap.slots[1]
                                    )),
                                    heap.generational_enabled()
                                );
                            }
                        });
                    });
                    let ends = session.finish_owners(|era, stopped| {
                        check_graphs(era, stopped, &graphs, false);
                        check_shells(era, stopped, &graphs, false, false);
                        check_shells(era, stopped, &recopied, false, false);
                    });
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
}

/// One owner repeatedly replaces a shared source with an increasing prefix;
/// three others take independent snapshots and inspect every entry. All
/// inspection is of the copy, so a later writer edit must never affect it.
#[test]
fn a6_ht_b7_copy_while_another_owner_mutates() {
    const KEYS: usize = 15;
    const REPS: usize = 32;
    let setup = HtSetup {
        source: "(list (make-hash-table :test 'eq))",
        facts: vec![(0, Abs::Table)],
        nslots: 20 + 2 * KEYS,
    };
    let mut writer = vec![Op::HtClear { table: 0 }];
    for key in 1..=KEYS {
        writer.extend([
            Op::ConstFixnum {
                dst: 2,
                value: key as i64,
            },
            Op::ConstFixnum {
                dst: 3,
                value: 3 * key as i64,
            },
            Op::HtPut {
                table: 0,
                key: 2,
                value: 3,
            },
        ]);
    }
    writer.extend(finish_line(&[]));
    let mut reader = vec![
        Op::HtCopy { table: 0, dst: 1 },
        Op::HtCount { table: 1, dst: 4 },
    ];
    let mut outputs = vec![4];
    for key in 1..=KEYS {
        let found = 20 + 2 * (key - 1) as u16;
        let value = found + 1;
        reader.extend([
            Op::ConstFixnum {
                dst: 2,
                value: key as i64,
            },
            Op::HtGet {
                table: 1,
                key: 2,
                found,
                value,
            },
        ]);
        outputs.extend([found, value]);
    }
    reader.extend(finish_line(&outputs));
    let programs = vec![writer, reader.clone(), reader.clone(), reader];
    for ops in &programs {
        prevalidate_with(ops, setup.nslots, 0, STEP_CAP, &setup.initial()).unwrap();
    }
    let env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    let (lines, _) = run_ht_overlap_session(&setup, &programs, REPS, 0, &env, 9800, &[]);
    for owner in &lines[1..] {
        assert_eq!(owner.len(), REPS);
        for line in owner {
            let fields: Vec<_> = line.trim_matches(['(', ')']).split_whitespace().collect();
            assert_eq!(fields.len(), 1 + 2 * KEYS);
            let count: usize = fields[0].parse().unwrap();
            assert!(count <= KEYS);
            for key in 1..=KEYS {
                let found = fields[2 * key - 1];
                let value = fields[2 * key];
                if key <= count {
                    assert_eq!(found, "t");
                    assert_eq!(value.parse::<usize>().unwrap(), 3 * key);
                } else {
                    assert_eq!((found, value), ("nil", "nil"));
                }
            }
        }
    }

    // Witness the actual copying leases: all three snapshots hold their
    // source reader count while the writer attempts its first insertion.
    // Omitting the stress program's initial clear keeps that writer from
    // entering before the readers have reached the barrier.
    let witnessed = vec![
        programs[1].clone(),
        programs[2].clone(),
        programs[3].clone(),
        programs[0][1..].to_vec(),
    ];
    let abort = AtomicBool::new(false);
    let witness = HtWitness::new(0, 3, 1, &abort);
    let mut env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.witness = Some(&witness);
    let (lines, _) = run_ht_overlap_session(&setup, &witnessed, 1, 0, &env, 9850, &[]);
    assert!(witness.max_readers.load(Ordering::SeqCst) >= 3);
    for seen in &witness.seen {
        assert_eq!(seen.load(Ordering::SeqCst), 3);
    }
    assert!(
        witness.drain_observed.load(Ordering::SeqCst),
        "source writer attempted while copying readers were live"
    );
    assert_eq!(witness.writer_word.load(Ordering::SeqCst), HT_WRITER_BIT);
    for owner in &lines[..3] {
        assert_eq!(owner.len(), 1);
        let fields: Vec<_> = owner[0]
            .trim_matches(['(', ')'])
            .split_whitespace()
            .collect();
        assert_eq!(fields.len(), 1 + 2 * KEYS);
        assert_eq!(fields[0], "0", "copy precedes the drained insertion");
        assert!(fields[1..].iter().all(|field| *field == "nil"));
    }
}

#[test]
fn a6_ht_b7_copy_active_test_starts_quiescent_and_bad_home_is_atomic() {
    Context::with_test_collection_era(
        |context| {
            let table = context
                .eval_str(
                    "(make-hash-table :test 'eq :size 32 :rehash-size 2.0 :rehash-threshold 0.8)",
                )
                .unwrap();
            unsafe {
                (
                    SharedRoot::new(&context.tagged_heap, table),
                    SharedRoot::new(&context.tagged_heap, TaggedValue::NIL),
                )
            }
        },
        |era, (source, nil)| {
            era.start_cons_storage().unwrap();
            let mut owner = era
                .register(9900, 3)
                .unwrap()
                .activate(era.heap_identity(), 9900)
                .unwrap();
            with_run(&mut owner, |run| {
                let src = run.root_slot(&source)?;
                let dst = run.root_slot(&nil)?;
                let source_value = TaggedValue::from_bits(src.word_for_test(run));
                let hold = TableRef::enter(run, source_value)?.hold_user_test()?;
                run.copy_hash_table_into(&src, &dst)?;
                let copy_value = TaggedValue::from_bits(dst.word_for_test(run));
                assert_ne!(source_value, copy_value);
                {
                    let lease = TableRef::enter(run, copy_value)?;
                    assert_eq!(lease.authority_word().load(Ordering::Acquire), 1);
                    assert_eq!(lease.user_test_depth(), 0);
                    lease.read(|storage| assert_eq!(storage.len(), 0));
                }
                {
                    let mut lease = TableMut::enter(run, copy_value)?;
                    assert!(!lease.user_test_mutation_refused());
                    lease.edit(|table| table.data.clear());
                }
                assert_eq!(TableRef::enter(run, source_value)?.user_test_depth(), 1);
                drop(hold);
                let before = run.owner.local.as_ref().unwrap().hot.allocated_count;
                let bytes = run.owner.local.as_ref().unwrap().hot.bytes_since_gc;
                run.clear_root_slots()?;
                let fresh = run.root_slot(&nil)?;
                assert_eq!(
                    run.make_hash_table_into(HashTableTest::Eq, 4, &dst),
                    Err(RegistrationError::StaleRegistration)
                );
                assert_eq!(
                    run.copy_hash_table_into(&fresh, &dst),
                    Err(RegistrationError::StaleRegistration)
                );
                assert_eq!(
                    run.owner.local.as_ref().unwrap().hot.allocated_count,
                    before
                );
                assert_eq!(run.owner.local.as_ref().unwrap().hot.bytes_since_gc, bytes);
                assert_eq!(fresh.word_for_test(run), TaggedValue::NIL.bits());
                run.clear_root_slots()?;
                Ok(())
            })
            .unwrap();
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped(&mut stopped).unwrap();
            complete(stopped);
            owner.retire_rooted(&[]).unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop((collector, keeper));
        },
    );
}
