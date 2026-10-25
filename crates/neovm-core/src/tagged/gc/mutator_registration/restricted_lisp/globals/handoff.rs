//! Ordered native histories on one real World, with all owners stopped at GC.
use super::tests::source;
use super::*;
use crate::tagged::gc::PlainSymbolBacking;

mod worker;
use worker::{Actor, Command, Event, Snapshot};

const NSLOTS: usize = 4;
const PHASES: usize = 5;

fn publish() -> Vec<Op> {
    vec![
        Op::ConstFixnum { dst: 0, value: 42 },
        Op::Cons {
            dst: 1,
            car: 0,
            cdr: 2,
        },
        Op::ConstFixnum { dst: 0, value: 41 },
        Op::Cons {
            dst: 1,
            car: 0,
            cdr: 1,
        },
        Op::Global(GlobalOp::Set {
            symbol: 0,
            src: 1,
            dst: 2,
        }),
        Op::ConstNil { dst: 1 },
        Op::ConstNil { dst: 2 },
    ]
}

fn erase() -> Vec<Op> {
    vec![
        Op::Global(GlobalOp::Makunbound { symbol: 0, dst: 2 }),
        Op::ConstNil { dst: 2 },
    ]
}

fn program(index: usize) -> Source {
    let mut ops = Vec::new();
    for phase in 0..PHASES {
        match (index, phase) {
            (0, 0) | (1, 2) => ops.extend(publish()),
            (0, 1) | (1, 3) => ops.extend(erase()),
            (1, 0) | (0, 2) => ops.push(Op::Global(GlobalOp::Value { symbol: 0, dst: 1 })),
            (_, 4) => ops.extend([Op::ConstNil { dst: 1 }, Op::ConstNil { dst: 2 }]),
            _ => {}
        }
        ops.extend([
            Op::Global(GlobalOp::Boundp { symbol: 0, dst: 3 }),
            Op::Safepoint,
        ]);
    }
    source(ops, NSLOTS)
}

struct Cycle {
    before: Vec<Snapshot>,
    after: Vec<Snapshot>,
    roots: gc::Ledger,
    live: Vec<(usize, bool)>,
    printed: Option<String>,
    epoch: u64,
    major: bool,
    frozen: usize,
}

fn collect(era: &ParallelHeapEra<'_>, before: Vec<Snapshot>, phase: usize) -> Cycle {
    let mut stopped = era.stop().unwrap();
    era.return_cons_storage(&stopped).unwrap();
    let roots = gc::ledger(era, &stopped);
    let report = era.collect_stopped(&mut stopped).unwrap();
    let (live, printed) = era
        .with_stopped_heap_for_test(&stopped, |heap| {
            let live = before
                .iter()
                .flat_map(|snap| &snap.owned)
                .map(|&bits| {
                    (
                        bits,
                        heap.owns_heap_value_for_test(TaggedValue::from_bits(bits)),
                    )
                })
                .collect();
            let reader = match phase {
                1 => Some(1),
                3 => Some(0),
                _ => None,
            };
            let printed = reader.and_then(|index| {
                let words = &before[index].words;
                let value = TaggedValue::from_bits(words[1]);
                // Decode only a still-live copied answer under the matching stop.
                (value.is_cons() && heap.owns_heap_value_for_test(value))
                    .then(|| format!("({} {})", prin1(words[1]), prin1(words[3])))
            });
            (live, printed)
        })
        .unwrap();
    era.resume_cons_storage(&stopped).unwrap();
    stopped.complete().unwrap();
    Cycle {
        before,
        after: Vec::new(),
        roots,
        live,
        printed,
        epoch: report.epoch,
        major: report.major,
        frozen: report.frozen_owners,
    }
}

fn scenario(kind: PlainSymbolBacking, owners: usize) {
    let _probes = crate::tagged::gc::mutator_registration::a8_probes::suppress();
    let (cycles, finished) = Context::with_test_symbol_backing_era(kind, |era, id| {
        let sources: Vec<_> = (0..owners).map(program).collect();
        let session = GlobalSession::admit(&era, &[id], &sources).unwrap();
        let mut stopped = era.stop().unwrap();
        let nil = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: this stopped heap owns the immediate seed.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        era.start_cons_storage().unwrap();
        let (cycles, finished) = thread::scope(|scope| {
            // Register all participants before the first activation can race
            // the registry gate. Every one runs a prevalidated session tape.
            let permits: Vec<_> = (0..owners)
                .map(|index| era.register(3000 + index as u64, NSLOTS).unwrap())
                .collect();
            let mut actors = Vec::new();
            let mut jobs = Vec::new();
            for (index, permit) in permits.into_iter().enumerate() {
                let (commands, receive_commands) = channel();
                let (send_events, events) = channel();
                let (session, nil) = (&session, &nil);
                let heap = era.heap_identity();
                jobs.push(scope.spawn(move || {
                    worker::run(
                        session,
                        nil,
                        permit,
                        heap,
                        index,
                        receive_commands,
                        send_events,
                    )
                }));
                actors.push(Actor { commands, events });
            }
            for actor in &actors {
                assert!(matches!(worker::receive(&actor.events), Event::Seeded));
            }
            let mut cycles = Vec::new();
            for phase in 0..PHASES {
                // These edges order only the specified writer/read/overwrite.
                // Other owner programs and all resume paths are real threads.
                let mut order: Vec<_> = (0..owners).collect();
                if phase == 2 || phase == 3 {
                    order.swap(0, 1);
                }
                let mut before: Vec<Option<Snapshot>> = (0..owners).map(|_| None).collect();
                for index in order {
                    actors[index].commands.send(Command::Advance).unwrap();
                    let event = worker::receive(&actors[index].events);
                    let Event::Paused(snapshot) = event else {
                        panic!("owner {index} phase {phase}: {event:?}");
                    };
                    before[index] = Some(snapshot);
                }
                let mut cycle = collect(
                    &era,
                    before.into_iter().map(Option::unwrap).collect(),
                    phase,
                );
                for actor in &actors {
                    actor.commands.send(Command::Resume).unwrap();
                }
                for actor in &actors {
                    let event = worker::receive(&actor.events);
                    let Event::Resumed(snapshot) = event else {
                        panic!("owner restore phase {phase}: {event:?}");
                    };
                    cycle.after.push(snapshot);
                }
                cycles.push(cycle);
            }
            for actor in &actors {
                actor.commands.send(Command::Finish).unwrap();
            }
            let finished: Vec<_> = actors
                .iter()
                .map(|actor| {
                    let event = worker::receive(&actor.events);
                    let Event::Finished { result, cleanup } = event else {
                        panic!("owner finish: {event:?}");
                    };
                    (result, cleanup)
                })
                .collect();
            for job in jobs {
                job.join().unwrap();
            }
            (cycles, finished)
        });
        drop(nil);
        // No live owner remains. Close the catalog and any residual births
        // before returning the actual World to serial/drop.
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        stopped.complete().unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        (cycles, finished)
    });

    // All semantic assertions follow complete owner and era teardown. A bad
    // observation must not masquerade as PendingRoots or a stuck native owner.
    assert_eq!(cycles.len(), PHASES);
    let first = &cycles[0].before[0].owned;
    let second = &cycles[2].before[1].owned;
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);
    assert!(
        first.iter().all(|word| !second.contains(word)),
        "B allocates while A's graph is still live"
    );
    let first_head = cycles[0].before[1].words[1];
    let second_head = cycles[2].before[0].words[1];
    assert!(first.contains(&first_head));
    assert!(second.contains(&second_head));
    let oracle: Vec<_> = include_str!("../../symbol_globals_oracle.expected")
        .lines()
        .collect();
    for (phase, cycle) in cycles.iter().enumerate() {
        assert!(cycle.major);
        assert_eq!(cycle.frozen, owners);
        if phase > 0 {
            assert!(cycle.epoch > cycles[phase - 1].epoch);
        }
        assert_eq!(
            cycle.before, cycle.after,
            "all owner homes restore unchanged"
        );
        let bound = TaggedValue::bool_val(phase == 0 || phase == 2).bits();
        assert!(cycle.before.iter().all(|snap| snap.words[3] == bound));
        for &(word, live) in &cycle.live {
            let expected = if first.contains(&word) {
                phase <= 1
            } else {
                phase <= 3
            };
            assert_eq!(
                live, expected,
                "{kind:?}/{owners}/phase{phase}: graph {word:#x}"
            );
        }
        if phase == 1 || phase == 3 {
            let head = if phase == 1 { first_head } else { second_head };
            assert_eq!(
                cycle.roots.extra_work, 0,
                "no birth/address-family history masks the answer root"
            );
            assert!(
                !cycle.roots.world.contains(&head),
                "canonical global has been made void"
            );
            assert!(
                cycle.roots.external.contains(&head),
                "the other owner's answer is the root"
            );
            assert_eq!(
                cycle.printed.as_deref(),
                Some(oracle[12]),
                "GNU copied answer after canonical overwrite/GC"
            );
        } else if phase == 0 || phase == 2 {
            let head = if phase == 0 { first_head } else { second_head };
            assert!(
                cycle.roots.world.contains(&head),
                "current writer's canonical backing must be reseeded"
            );
        } else {
            assert!(
                cycle
                    .roots
                    .world
                    .iter()
                    .chain(&cycle.roots.external)
                    .all(|word| !first.contains(word) && !second.contains(word))
            );
        }
        eprintln!(
            "P3SYM_C3_HANDOFF_GC kind={kind:?} owners={owners} phase={phase} epoch={} major={} frozen={}",
            cycle.epoch, cycle.major, cycle.frozen
        );
    }
    for (index, (result, cleanup)) in finished.iter().enumerate() {
        assert_eq!(cleanup, &Ok(()), "owner {index} cleanup");
        let snapshot = result.as_ref().unwrap();
        assert_eq!(snapshot.points, PHASES);
        assert_eq!(snapshot.owned.len(), if index < 2 { 2 } else { 0 });
    }
    eprintln!(
        "P3SYM_C3_HANDOFF kind={kind:?} owners={owners} phases={PHASES} property_gcs={PHASES} allocated=4"
    );
}

#[cfg(not(miri))]
#[test]
fn p3sym_c3_plain_multi_owner_handoff_gc() {
    for owners in 2..=4 {
        scenario(PlainSymbolBacking::Unprojected, owners);
    }
}

#[cfg(not(miri))]
#[test]
fn p3sym_c3_projected_multi_owner_handoff_gc() {
    for owners in 2..=4 {
        scenario(PlainSymbolBacking::Projected, owners);
    }
}

#[cfg(not(miri))]
#[test]
fn p3sym_c3_void_multi_owner_handoff_gc() {
    for owners in 2..=4 {
        scenario(PlainSymbolBacking::VoidProjection, owners);
    }
}

#[test]
fn p3sym_c3_two_owner_handoff_gc_small() {
    scenario(PlainSymbolBacking::Projected, 2);
}
