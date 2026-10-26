//! Actual canonical/answer roots, after birth logs drain, then reclamation.
use super::tests::{case_many, drive, words};
use super::*;
use crate::tagged::gc::PlainSymbolBacking;

#[derive(Default)]
pub(super) struct Ledger {
    pub(super) world: Vec<usize>,
    symbols: Vec<usize>,
    projections: Vec<usize>,
    pub(super) external: Vec<usize>,
    pub(super) extra_work: usize,
}

pub(super) fn ledger(era: &ParallelHeapEra<'_>, stopped: &mut AllStopped<'_>) -> Ledger {
    let mut result = Ledger::default();
    let keeper = era.keeper.as_ref().unwrap();
    let mut world = Vec::new();
    keeper
        .collect_parallel_world_roots(stopped, &mut world)
        .unwrap();
    result.world = world.into_iter().map(TaggedValue::bits).collect();
    (result.symbols, result.projections) = keeper.symbol_root_ledger_for_test(stopped).unwrap();
    let mut external = Vec::new();
    era.legacy_roots
        .as_ref()
        .unwrap()
        .append_words(&mut external);
    era.with_stopped_heap_for_test(stopped, |heap| {
        crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut external);
    })
    .unwrap();
    result
        .external
        .extend(external.into_iter().map(TaggedValue::bits));
    let keys: Vec<_> = {
        let gate = era.registry().lock().unwrap();
        gate.entries.iter().map(|entry| entry.key).collect()
    };
    for key in keys {
        stopped
            .with_owner_image(key, |image| {
                image.visit_logs(|word| result.external.push(word.value().bits()));
                let s = image.summary();
                result.extra_work += s.mapped_owners
                    + s.black_headers
                    + s.black_regions
                    + s.symbol_preimages
                    + s.snapshotted_owners
                    + s.string_preimage_addrs
                    + s.young_blocks;
            })
            .unwrap();
    }
    result
}

/// Capture all assertions as facts, and resume the owner before checking them.
/// A failing witness must not strand its otherwise healthy fixture in Native.
fn round(
    era: &ParallelHeapEra<'_>,
    owner: &mut Mutator<'_>,
    graph: &[usize],
) -> (Ledger, Vec<bool>, u64) {
    owner.enter_native(&[]).unwrap();
    let mut stopped = era.stop().unwrap();
    era.return_cons_storage(&stopped).unwrap();
    let before = ledger(era, &mut stopped);
    let report = era.collect_stopped(&mut stopped).unwrap();
    let live = era
        .with_stopped_heap_for_test(&stopped, |heap| {
            graph
                .iter()
                .map(|&bits| heap.owns_heap_value_for_test(TaggedValue::from_bits(bits)))
                .collect::<Vec<_>>()
        })
        .unwrap();
    era.resume_cons_storage(&stopped).unwrap();
    stopped.complete().unwrap();
    owner.leave_native().unwrap();
    assert!(
        report.major,
        "this adapter runs full stopped jobs, not minors"
    );
    assert_eq!(report.frozen_owners, 1, "one nonlegacy participant");
    eprintln!(
        "P3SYM_C3_GC_ROUND epoch={} cycle={} major={} roots={}",
        report.epoch, report.cycle, report.major, report.roots
    );
    (before, live, report.epoch)
}

fn pause<'era>(session: &GlobalSession, owner: &mut Mutator<'era>, frame: &mut Frame<'era>) {
    loop {
        match session.wait_step(owner, frame).unwrap() {
            Step::Safepoint => return,
            Step::Progress => {}
            Step::Retry => thread::yield_now(),
            Step::Done => panic!("expected a stopped-collection boundary"),
        }
    }
}

fn global_graph(kind: PlainSymbolBacking) {
    let ops = vec![
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
        Op::Global(GlobalOp::Boundp { symbol: 1, dst: 3 }),
        Op::Safepoint, // Drain births; global is the only ordinary root.
        Op::Safepoint, // Reseed again with no birth/history retention available.
        Op::Global(GlobalOp::Value { symbol: 0, dst: 1 }),
        Op::Global(GlobalOp::Makunbound { symbol: 0, dst: 2 }),
        Op::Global(GlobalOp::Boundp { symbol: 1, dst: 3 }),
        Op::Safepoint, // Only the owner's copied answer retains the graph.
        Op::ConstNil { dst: 1 },
        Op::ConstNil { dst: 2 },
        Op::Safepoint, // Both nodes must now be reclaimed.
    ];
    case_many(
        &[kind, PlainSymbolBacking::VoidProjection],
        ops,
        4,
        |era, session, owner, frame, _ids| {
            pause(session, owner, frame);
            let graph: Vec<_> = frame.machine.owned.iter().copied().collect();
            assert_eq!(graph.len(), 2);
            assert_eq!(words(owner, frame)[1..], [TaggedValue::NIL.bits(); 3]);
            let (_, live, epoch1) = round(era, owner, &graph);
            assert!(
                live.iter().all(|&v| v),
                "new canonical graph survived birth drain"
            );

            pause(session, owner, frame);
            let (roots, live, epoch2) = round(era, owner, &graph);
            assert!(epoch2 > epoch1);
            assert!(
                live.iter().all(|&v| v),
                "current World alone must reseed the graph"
            );
            assert_eq!(
                roots.extra_work, 0,
                "no black births, young blocks or address-family history may mask missing World roots"
            );
            let head = *graph
                .iter()
                .find(|&&bits| roots.world.contains(&bits))
                .unwrap();
            assert!(
                roots.external.iter().all(|word| !graph.contains(word)),
                "no owner, frozen legacy or transport root retains either node"
            );
            assert_eq!(
                roots.world.iter().filter(|&&word| word == head).count(),
                roots
                    .symbols
                    .iter()
                    .chain(&roots.projections)
                    .filter(|&&word| word == head)
                    .count()
            );
            // Negative controls operate on the real ledger, never by freeing
            // a graph still named by a live World (which would be UB).
            let without_world = &roots.external;
            assert!(!without_world.contains(&head));
            eprintln!("P3SYM_C3_GC_WITNESS omitted_live_world_drops_global_root {kind:?}");
            if kind == PlainSymbolBacking::Unprojected {
                assert!(roots.symbols.contains(&head));
                assert!(!roots.projections.contains(&head));
            } else {
                assert!(roots.projections.contains(&head));
                assert!(!roots.symbols.contains(&head));
                let mut without_projections = roots.external.clone();
                without_projections.extend(
                    roots
                        .world
                        .iter()
                        .copied()
                        .filter(|word| !roots.projections.contains(word)),
                );
                assert!(!without_projections.contains(&head));
                eprintln!(
                    "P3SYM_C3_GC_WITNESS omitted_projection_roots_drops_canonical_root {kind:?}"
                );
            }

            pause(session, owner, frame);
            let (roots, live, epoch3) = round(era, owner, &graph);
            assert!(epoch3 > epoch2);
            assert!(
                live.iter().all(|&v| v),
                "copied answer must survive overwrite"
            );
            assert!(roots.world.iter().all(|word| !graph.contains(word)));
            assert!(roots.external.contains(&head));
            let result = words(owner, frame);
            let printed = format!("({} {})", prin1(result[1]), prin1(result[3]));
            let oracle: Vec<_> = include_str!("../../symbol_globals_oracle.expected")
                .lines()
                .collect();
            assert_eq!(printed, oracle[12], "GNU rooted answer after makunbound/GC");

            pause(session, owner, frame);
            let (roots, live, epoch4) = round(era, owner, &graph);
            assert!(epoch4 > epoch3);
            assert!(
                roots
                    .world
                    .iter()
                    .chain(&roots.external)
                    .all(|word| !graph.contains(word))
            );
            assert!(
                live.iter().all(|&v| !v),
                "clearing the last answer must reclaim both nodes"
            );
            drive(session, owner, frame).unwrap();
            assert_eq!(frame.machine.safepoints, 4);
        },
    );
}

#[test]
fn p3sym_c3_plain_global_graph_four_stops() {
    global_graph(PlainSymbolBacking::Unprojected);
}

#[test]
fn p3sym_c3_projected_global_graph_four_stops() {
    global_graph(PlainSymbolBacking::Projected);
}

#[test]
fn p3sym_c3_void_global_graph_four_stops() {
    global_graph(PlainSymbolBacking::VoidProjection);
}
