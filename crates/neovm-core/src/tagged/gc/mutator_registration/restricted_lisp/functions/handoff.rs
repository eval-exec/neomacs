//! Ordered real-owner calls, concurrent writers, and stopped definition lifetime.
use super::*;
use crate::tagged::gc::PlainSymbolBacking;
use std::num::NonZeroU32;
mod gc;
mod worker;
use worker::{Actor, Command, Event, Snapshot};

const OWNER_BASE: u64 = 4100;
const NSLOTS: usize = 4;
const PHASES: usize = 5;
fn function(op: FunctionOp) -> Op {
    Op::Function(op)
}
fn begin() -> Op {
    function(FunctionOp::CallBegin { symbol: 0 })
}
fn call(dst: u16) -> Op {
    function(FunctionOp::CallStep {
        dst,
        fuel: NonZeroU32::MIN,
    })
}
fn nil(dst: u16) -> Op {
    Op::ConstNil { dst }
}
fn unbind(symbol: u16) -> Op {
    function(FunctionOp::Fmakunbound { symbol, dst: 3 })
}
fn source(index: usize) -> Source {
    let mut ops = Vec::new();
    for phase in 0..PHASES {
        match (index, phase) {
            (0, 0) => ops.push(function(FunctionOp::Function { symbol: 0, dst: 3 })),
            (1, 0) => ops.push(function(FunctionOp::Function { symbol: 1, dst: 3 })),
            (0, 1) => ops.extend([nil(3), begin()]),
            (1, 1) => ops.extend([
                Op::Global(GlobalOp::Value { symbol: 1, dst: 0 }),
                function(FunctionOp::Fset {
                    symbol: 0,
                    src: 0,
                    dst: 3,
                }),
                Op::Global(GlobalOp::Makunbound { symbol: 0, dst: 3 }),
                nil(0),
                nil(3),
            ]),
            // A second actual writer changes the other symbol concurrently.
            (2, 1) => ops.extend([
                function(FunctionOp::Fset {
                    symbol: 1,
                    src: 0,
                    dst: 3,
                }),
                nil(3),
            ]),
            (0, 2) => ops.extend([call(1), begin(), call(2)]),
            (0, 3) => ops.push(begin()),
            (1, 3) => ops.extend([
                unbind(0),
                unbind(1),
                Op::Global(GlobalOp::Makunbound { symbol: 1, dst: 3 }),
                nil(3),
            ]),
            (0, 4) => ops.extend([call(2), Op::ConstFixnum { dst: 3, value: 23 }]),
            _ => {}
        }
        ops.push(Op::Safepoint);
    }
    if index == 0 {
        ops.extend([begin(), call(3)]);
    }
    super::tests::source(ops, NSLOTS)
}
struct Round {
    journal_before: Vec<usize>,
    roots: gc::Ledger,
    live: Vec<Vec<bool>>,
    epoch: u64,
    major: bool,
    owners: usize,
}
fn collect(
    era: &ParallelHeapEra<'_>,
    stopped: &mut AllStopped<'_>,
    graphs: &[Vec<usize>],
    reconcile: bool,
) -> Round {
    let journal_before = gc::ledger(era, stopped).journal;
    if reconcile {
        stopped.symbols().unwrap().reconcile_functions().unwrap();
    }
    let roots = gc::ledger(era, stopped);
    let report = era.collect_stopped(stopped).unwrap();
    let live = gc::live(era, stopped, graphs);
    era.resume_cons_storage(stopped).unwrap();
    Round {
        journal_before,
        roots,
        live,
        epoch: report.epoch,
        major: report.major,
        owners: report.frozen_owners,
    }
}
fn round(
    era: &ParallelHeapEra<'_>,
    owner: &mut Mutator<'_>,
    graphs: &[Vec<usize>],
    reconcile: bool,
) -> Round {
    owner.enter_native(&[]).unwrap();
    let mut stopped = era.stop().unwrap();
    era.return_cons_storage(&stopped).unwrap();
    let result = collect(era, &mut stopped, graphs, reconcile);
    stopped.complete().unwrap();
    owner.leave_native().unwrap();
    result
}
fn advance(actor: &Actor) -> Snapshot {
    actor.commands.send(Command::Advance).unwrap();
    paused(actor)
}
fn paused(actor: &Actor) -> Snapshot {
    match worker::receive(&actor.events) {
        Event::Paused(snapshot) => snapshot,
        event => panic!("expected paused owner: {event:?}"),
    }
}
fn scenario(kind: PlainSymbolBacking, count: usize) {
    let _probes = crate::tagged::gc::mutator_registration::a8_probes::suppress();
    let (rounds, snapshots, finished, graphs) =
        Context::with_test_symbol_functions_era(kind, |era, ids| {
            let sources: Vec<_> = (0..count).map(source).collect();
            let session = Session::admit(&era, &ids, &sources).unwrap();
            let mut stopped = era.stop().unwrap();
            let nil = era
                .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                    // SAFETY: same stopped heap and immediate scalar seed.
                    SharedRoot::new(heap, TaggedValue::NIL)
                })
                .unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            era.start_cons_storage().unwrap();
            let (rounds, snapshots, finished, graphs) = thread::scope(|scope| {
                let permits: Vec<_> = (0..count)
                    .map(|index| {
                        era.register(
                            OWNER_BASE + index as u64,
                            session.root_capacity(index).unwrap(),
                        )
                        .unwrap()
                    })
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
                let mut rounds = Vec::new();
                let mut snapshots = Vec::new();
                let mut graphs = Vec::new();
                for phase in 0..PHASES {
                    let mut before: Vec<Option<Snapshot>> = (0..count).map(|_| None).collect();
                    // Successful selection precedes the writer phase. Remaining
                    // owners advance concurrently, including the two writers.
                    if phase == 1 || phase == 3 {
                        before[0] = Some(advance(&actors[0]));
                    }
                    for (index, actor) in actors.iter().enumerate() {
                        if before[index].is_none() {
                            actor.commands.send(Command::Advance).unwrap();
                        }
                    }
                    for (index, actor) in actors.iter().enumerate() {
                        if before[index].is_none() {
                            before[index] = Some(paused(actor));
                        }
                    }
                    let before: Vec<_> = before.into_iter().map(Option::unwrap).collect();
                    let mut stopped = era.stop().unwrap();
                    era.return_cons_storage(&stopped).unwrap();
                    if phase == 0 {
                        graphs = vec![
                            gc::graph(&era, &stopped, before[0].words[3]),
                            gc::graph(&era, &stopped, before[1].words[3]),
                        ];
                    }
                    rounds.push(collect(&era, &mut stopped, &graphs, true));
                    stopped.complete().unwrap();
                    for actor in &actors {
                        actor.commands.send(Command::Resume).unwrap();
                    }
                    let after: Vec<_> = actors
                        .iter()
                        .map(|actor| match worker::receive(&actor.events) {
                            Event::Resumed(snapshot) => snapshot,
                            event => panic!("expected resumed owner: {event:?}"),
                        })
                        .collect();
                    snapshots.push((before, after));
                }
                for actor in &actors {
                    actor.commands.send(Command::Finish).unwrap();
                }
                let finished: Vec<_> = actors
                    .iter()
                    .map(|actor| match worker::receive(&actor.events) {
                        Event::Finished { result, cleanup } => (result, cleanup),
                        event => panic!("expected completed owner: {event:?}"),
                    })
                    .collect();
                for job in jobs {
                    job.join().unwrap();
                }
                (rounds, snapshots, finished, graphs)
            });
            drop(nil);
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            stopped.symbols().unwrap().reconcile_functions().unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
            (rounds, snapshots, finished, graphs)
        });
    // Check semantic facts only after every frame/image and era has finished.
    assert_eq!(graphs.iter().map(Vec::len).collect::<Vec<_>>(), [3, 3]);
    assert!(graphs[0].iter().all(|word| !graphs[1].contains(word)));
    let oracle: Vec<_> = include_str!("../../symbol_functions_oracle.expected")
        .lines()
        .collect();
    for (phase, (round, (before, after))) in rounds.iter().zip(&snapshots).enumerate() {
        assert!(round.major);
        assert_eq!(round.owners, count);
        if phase > 0 {
            assert!(round.epoch > rounds[phase - 1].epoch);
        }
        assert_eq!(round.live[0], vec![phase <= 1; 3], "old graph at {phase}");
        assert_eq!(round.live[1], vec![phase <= 3; 3], "new graph at {phase}");
        for (a, b) in before.iter().zip(after) {
            assert_eq!(
                (&a.words, a.cache, a.selected, a.pc, a.points),
                (&b.words, b.cache, b.selected, b.pc, b.points)
            );
            assert_eq!(b.points, phase + 1);
            assert_eq!(
                b.facts, after[0].facts,
                "canonical clock/stamps agree after writers complete"
            );
        }
        if phase == 1 || phase == 3 {
            let graph = &graphs[if phase == 1 { 0 } else { 1 }];
            let head = graph[0];
            assert!(round.journal_before.contains(&head));
            assert!(
                round.roots.journal.is_empty(),
                "actual stopped reconciliation released originals"
            );
            assert_eq!(round.roots.other_work, 0, "{:?}", round.roots.work_details);
            assert!(
                round
                    .roots
                    .world
                    .iter()
                    .chain(&round.roots.external)
                    .all(|word| !graph.contains(word))
            );
            for (owner, roots) in &round.roots.owners {
                assert_eq!(
                    roots.iter().filter(|&&word| word == head).count(),
                    usize::from(*owner == OWNER_BASE)
                );
            }
            assert!(before[0].cache.is_none());
            assert!(before[0].selected.is_some());
            assert!(
                before
                    .iter()
                    .flat_map(|snap| &snap.words)
                    .all(|word| !graph.contains(word))
            );
            // Omit the activation FAMILY from this ledger only, never from
            // the real collector while a live activation can still use it.
            let without_activation = round
                .roots
                .owners
                .iter()
                .filter(|(owner, _)| *owner != OWNER_BASE)
                .flat_map(|(_, roots)| roots);
            assert!(
                without_activation
                    .into_iter()
                    .all(|word| !graph.contains(word))
            );
            eprintln!(
                "P3SYM_C4_GC_WITNESS omitted_activation_family_loses_only_definition_root {kind:?} owners={count} phase={phase}"
            );
        }
        if phase == 2 || phase == 4 {
            let graph = &graphs[if phase == 2 { 0 } else { 1 }];
            assert!(!round.roots.ordinary_has(graph));
            assert!(round.roots.journal.iter().all(|word| !graph.contains(word)));
        }
        eprintln!(
            "P3SYM_C4_HANDOFF_GC kind={kind:?} owners={count} phase={phase} epoch={} major={} old_live={} new_live={}",
            round.epoch,
            round.major,
            round.live[0].iter().filter(|&&v| v).count(),
            round.live[1].iter().filter(|&&v| v).count()
        );
    }
    let initial = snapshots[0].1[0].facts;
    let changed = snapshots[1].1[0].facts;
    let increments = if count >= 3 { 2 } else { 1 };
    assert_eq!(changed[0].0, initial[0].0 + increments);
    assert_eq!(changed[1].0, changed[0].0);
    if count >= 3 {
        let mut winners = [changed[0].1, changed[1].1];
        winners.sort_unstable();
        assert_eq!(
            winners,
            [initial[0].0 + 1, initial[0].0 + 2],
            "two writers keep both distinct commits"
        );
    } else {
        assert_eq!(changed[0].1, changed[0].0);
        assert_eq!(changed[1].1, initial[1].1);
    }
    let unbound = snapshots[3].1[0].facts;
    assert_eq!(
        unbound,
        [
            (changed[0].0 + 2, changed[0].0 + 1),
            (changed[0].0 + 2, changed[0].0 + 2)
        ]
    );
    let semantic = &snapshots[2].1[0].words;
    assert_eq!(
        format!("({} {})", prin1(semantic[1]), prin1(semantic[2])),
        oracle[5]
    );
    for (index, (result, cleanup)) in finished.iter().enumerate() {
        assert_eq!(cleanup, &Ok(()), "owner {index} cleanup");
        let terminal = result.as_ref().unwrap();
        if index == 0 {
            assert_eq!(
                terminal.execution,
                Err(ExecError::Function(FunctionError::VoidFunction))
            );
            assert_eq!(
                format!("(void-function {})", prin1(terminal.snapshot.words[3])),
                oracle[13]
            );
            assert_eq!(
                format!(
                    "({} {})",
                    prin1(terminal.snapshot.words[1]),
                    prin1(terminal.snapshot.words[2])
                ),
                oracle[5]
            );
        } else {
            assert_eq!(terminal.execution, Ok(()));
        }
    }
    eprintln!(
        "P3SYM_C4_HANDOFF kind={kind:?} owners={count} property_gcs={PHASES} graphs=2 nodes=6"
    );
}
#[cfg(not(miri))]
#[test]
fn p3sym_c4_plain_multi_owner_function_handoff_gc() {
    for count in 2..=4 {
        scenario(PlainSymbolBacking::Unprojected, count);
    }
}
#[cfg(not(miri))]
#[test]
fn p3sym_c4_projected_multi_owner_function_handoff_gc() {
    for count in 2..=4 {
        scenario(PlainSymbolBacking::Projected, count);
    }
}

#[test]
fn p3sym_c4_journal_only_definition_gc() {
    let _probes = a8_probes::suppress();
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        let ops = vec![
            function(FunctionOp::Function { symbol: 0, dst: 3 }),
            Op::Safepoint,
            Op::Global(GlobalOp::Value { symbol: 1, dst: 0 }),
            function(FunctionOp::Fset {
                symbol: 0,
                src: 0,
                dst: 2,
            }),
            Op::Global(GlobalOp::Makunbound { symbol: 0, dst: 2 }),
            nil(0),
            nil(2),
            nil(3),
            Op::Safepoint,
            Op::Safepoint,
        ];
        super::tests::case(kind, ops, NSLOTS, |era, session, owner, frame, _| {
            worker::pause(session, owner, frame).unwrap();
            let head = with_run(owner, |run| Ok(frame.slots[3].word_for_test(run))).unwrap();
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            let graphs = vec![gc::graph(era, &stopped, head)];
            let warm = collect(era, &mut stopped, &graphs, true);
            stopped.complete().unwrap();
            owner.leave_native().unwrap();
            worker::pause(session, owner, frame).unwrap();
            let journal = round(era, owner, &graphs, false);
            worker::pause(session, owner, frame).unwrap();
            let retired = round(era, owner, &graphs, true);
            assert_eq!(graphs[0].len(), 3);
            assert_eq!(warm.live[0], [true; 3]);
            assert!(journal.journal_before.contains(&head));
            assert!(journal.roots.journal.contains(&head));
            assert!(!journal.roots.ordinary_has(&graphs[0]));
            assert_eq!(
                journal.roots.other_work, 0,
                "{:?}",
                journal.roots.work_details
            );
            assert_eq!(journal.live[0], [true; 3]);
            assert!(retired.roots.journal.is_empty());
            assert!(!retired.roots.ordinary_has(&graphs[0]));
            assert_eq!(retired.live[0], [false; 3]);
            eprintln!("P3SYM_C4_GC_WITNESS omitted_journal_loses_only_definition_root {kind:?}");
        });
    }
}
#[test]
fn p3sym_c4_activation_gc_small() {
    let _probes = a8_probes::suppress();
    let ops = vec![
        function(FunctionOp::Function { symbol: 0, dst: 2 }),
        Op::Safepoint,
        begin(),
        Op::Global(GlobalOp::Value { symbol: 1, dst: 0 }),
        function(FunctionOp::Fset {
            symbol: 0,
            src: 0,
            dst: 3,
        }),
        Op::Global(GlobalOp::Makunbound { symbol: 0, dst: 3 }),
        nil(0),
        nil(2),
        nil(3),
        Op::Safepoint,
        call(1),
        Op::Safepoint,
    ];
    super::tests::case(
        PlainSymbolBacking::Projected,
        ops,
        NSLOTS,
        |era, session, owner, frame, _| {
            worker::pause(session, owner, frame).unwrap();
            let head = with_run(owner, |run| Ok(frame.slots[2].word_for_test(run))).unwrap();
            let graphs = vec![vec![head]];
            // This property collection drains serial setup births. Without
            // it, a YoungBlock can mask the private activation root under test.
            let warm = round(era, owner, &graphs, true);
            worker::pause(session, owner, frame).unwrap();
            with_run(owner, |run| frame.cache.clear(run).map_err(ExecError::from)).unwrap();
            let selected = round(era, owner, &graphs, true);
            worker::pause(session, owner, frame).unwrap();
            let returned = round(era, owner, &graphs, true);
            let answer = with_run(owner, |run| Ok(frame.slots[1].word_for_test(run))).unwrap();
            assert_eq!(warm.live[0], [true]);
            assert_eq!(selected.live[0], [true]);
            assert_eq!(returned.live[0], [false]);
            assert!(
                selected
                    .roots
                    .world
                    .iter()
                    .chain(&selected.roots.external)
                    .all(|word| *word != head)
            );
            assert_eq!(
                selected.roots.other_work, 0,
                "{:?}",
                selected.roots.work_details
            );
            assert!(selected.roots.journal.is_empty());
            assert!(!returned.roots.ordinary_has(&graphs[0]));
            let oracle: Vec<_> = include_str!("../../symbol_functions_oracle.expected")
                .lines()
                .collect();
            assert_eq!(prin1(answer), oracle[3]);
        },
    );
}
