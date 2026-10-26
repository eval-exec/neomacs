//! A real closed tape leaves the registry index as the sole winning-name root.
use super::*;
use crate::tagged::gc::AllStopped;
use crate::tagged::gc::mutator_registration::image::PublishedGcWork;

#[derive(Default, Debug)]
struct Ledger {
    names: Vec<usize>,
    world: Vec<usize>,
    external: Vec<usize>,
    owners: Vec<usize>,
    journal: Vec<usize>,
    other_work: Vec<String>,
}
impl Ledger {
    fn without_names(&self) -> impl Iterator<Item = &usize> {
        self.world
            .iter()
            .chain(&self.external)
            .chain(&self.owners)
            .chain(&self.journal)
    }
}

fn ledger(era: &ParallelHeapEra<'_>, stopped: &mut AllStopped<'_>) -> Ledger {
    let mut result = Ledger::default();
    let mut names = Vec::new();
    stopped
        .symbols()
        .unwrap()
        .append_name_roots(&mut names)
        .unwrap();
    result.names = names.into_iter().map(TaggedValue::bits).collect();
    let mut world = Vec::new();
    era.keeper
        .as_ref()
        .unwrap()
        .collect_parallel_world_roots(stopped, &mut world)
        .unwrap();
    result.world = world.into_iter().map(TaggedValue::bits).collect();
    let mut external = Vec::new();
    era.legacy_roots
        .as_ref()
        .unwrap()
        .append_words(&mut external);
    era.with_stopped_heap_for_test(stopped, |heap| {
        crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut external);
    })
    .unwrap();
    result.external = external.into_iter().map(TaggedValue::bits).collect();
    let keys: Vec<_> = {
        let gate = era.registry().lock().unwrap();
        gate.entries.iter().map(|entry| entry.key).collect()
    };
    for key in keys {
        stopped
            .with_owner_image(key, |image| {
                image.visit_work(|work| match work {
                    PublishedGcWork::Root(word) => result.owners.push(word.value().bits()),
                    other => result.other_work.push(format!("{other:?}")),
                });
            })
            .unwrap();
    }
    let mut journal = Vec::new();
    stopped
        .symbols()
        .unwrap()
        .append_function_roots(&mut journal)
        .unwrap();
    result.journal = journal.into_iter().map(TaggedValue::bits).collect();
    result
}

/// Return the actual slice catalog around collection. Recover the catalog,
/// stop and owner before checking observations or propagating a test assertion.
fn round(
    era: &ParallelHeapEra<'_>,
    owner: &mut Mutator<'_>,
    names: [usize; 2],
) -> (Ledger, [bool; 2], Option<Vec<u8>>, u64) {
    round_with_preflight(era, owner, names, || {})
}

fn round_with_preflight(
    era: &ParallelHeapEra<'_>,
    owner: &mut Mutator<'_>,
    names: [usize; 2],
    before_collection: impl FnOnce(),
) -> (Ledger, [bool; 2], Option<Vec<u8>>, u64) {
    let _probes = crate::tagged::gc::mutator_registration::a8_probes::suppress();
    // This synchronous keeper performs stopped heap work itself; the native
    // blocking-call wrapper excludes keeper callers and heap access in its body.
    owner.enter_native(&[]).unwrap();
    let mut stopped = match era.stop() {
        Ok(stopped) => stopped,
        Err(error) => {
            owner.leave_native().unwrap();
            panic!("executor name collection stop refused: {error:?}");
        }
    };
    let mut returned_catalog = false;
    let mut collected = false;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        era.return_cons_storage(&stopped).unwrap();
        returned_catalog = true;
        before_collection();
        let before = ledger(era, &mut stopped);
        let report = era.collect_stopped(&mut stopped);
        collected = report.is_ok();
        let observed = report.as_ref().ok().map(|_| {
            era.with_stopped_heap_for_test(&stopped, |heap| {
                let live =
                    names.map(|word| heap.owns_heap_value_for_test(TaggedValue::from_bits(word)));
                // A reclaimed object's word remains inert. Test allocation
                // membership before reading the surviving winner's payload.
                let text = live[0]
                    .then(|| {
                        TaggedValue::from_bits(names[0])
                            .as_lisp_string()
                            .map(|name| name.as_bytes().to_vec())
                    })
                    .flatten();
                (live, text)
            })
        });
        (before, report, observed)
    }));
    // A nonterminal preflight failure has not traced the stop. Complete its
    // trace without collecting before normal catalog/owner recovery. A failed
    // collector callback is terminal and must never use this recovery path.
    let trace_recovery = if collected {
        Ok(())
    } else {
        stopped.trace(|_, _| Ok(()))
    };
    trace_recovery.expect("terminal collector failure cannot resume a test owner");
    let resumed = returned_catalog.then(|| era.resume_cons_storage(&stopped));
    let completed = stopped.complete();
    let returned_owner = owner.leave_native();
    if let Some(result) = resumed {
        result.unwrap();
    }
    completed.unwrap();
    returned_owner.unwrap();
    let (before, report, observed) =
        outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    let report = report.expect("executor name collection after owner/catalog recovery");
    assert!(report.major, "C5 qualifies full stopped collections");
    assert_eq!(report.frozen_owners, 1);
    let (live, text) = observed.unwrap().unwrap();
    eprintln!(
        "P3SYM_C5_EXECUTOR_NAME_GC_ROUND epoch={} cycle={} names={} live={live:?}",
        report.epoch,
        report.cycle,
        before.names.len()
    );
    (before, live, text, report.epoch)
}

fn pause<'era>(session: &Session, owner: &mut Mutator<'era>, frame: &mut Frame<'era>) {
    loop {
        match session.wait_step(owner, frame).unwrap() {
            Step::Safepoint => return,
            Step::Progress => {}
            Step::Retry => unreachable!("wait_step resolves contention"),
            Step::Done => panic!("executor expected a collection boundary"),
        }
    }
}

fn sole_name_root(kind: PlainSymbolBacking) {
    let mut ops = vec![
        load(0, 0),
        load(1, 1),
        Op::Safepoint,
        intern(0, 2),
        intern(1, 3),
        soft(1, 4),
        Op::Eq { dst: 5, a: 2, b: 3 },
        Op::Eq { dst: 6, a: 2, b: 4 },
        Op::Safepoint,
        Op::Global(GlobalOp::Makunbound { symbol: 0, dst: 7 }),
        Op::Global(GlobalOp::Makunbound { symbol: 1, dst: 7 }),
    ];
    ops.extend((0..8).map(|dst| Op::ConstNil { dst }));
    ops.extend([Op::Safepoint, Op::Safepoint]);
    case(kind, ops, 8, |era, session, owner, frame| {
        pause(session, owner, frame);
        // Only inert words and independent Rust bytes cross the collections.
        let w = words(owner, frame);
        let names = [w[0], w[1]];
        assert_ne!(names[0], names[1], "distinct equal-name inputs");
        let spelling = TaggedValue::from_bits(names[0])
            .as_lisp_string()
            .unwrap()
            .as_bytes()
            .to_vec();
        pause(session, owner, frame);
        let w = words(owner, frame);
        let oracle: Vec<_> = include_str!("../../../symbol_intern_oracle.expected")
            .lines()
            .collect();
        assert_eq!(prin1(w[5]), oracle[2]);
        assert_eq!(prin1(w[6]), oracle[3]);
        let id = TaggedValue::from_bits(w[2]).as_bare_symbol_id().unwrap();
        pause(session, owner, frame);
        assert_eq!(words(owner, frame), [TaggedValue::NIL.bits(); 8]);
        assert_eq!(frame.machine.cons_count, 0);
        let (roots, live, text, first_epoch) = round(era, owner, names);
        assert_eq!(
            live,
            [true, false],
            "winner retained, equal losing name reclaimed"
        );
        assert_eq!(text.as_ref(), Some(&spelling));
        assert!(
            roots.without_names().all(|word| !names.contains(word)),
            "tape cleared every ordinary name root: {roots:?}"
        );
        pause(session, owner, frame);
        let (roots, live, text, epoch) = round(era, owner, names);
        assert!(epoch > first_epoch);
        assert_eq!(live, [true, false]);
        assert_eq!(text, Some(spelling));
        assert!(
            roots.other_work.is_empty(),
            "no birth/address/history work masks the index: {roots:?}"
        );
        assert_eq!(
            roots.names.iter().filter(|&&word| word == names[0]).count(),
            1
        );
        assert!(!roots.names.contains(&names[1]));
        assert!(
            roots.without_names().all(|word| !names.contains(word)),
            "no World/owner/transport/frozen/journal root retains either name: {roots:?}"
        );
        // Negative control on the copied real census only. Never collect after
        // omitting a required live family from the actual collector.
        assert!(!roots.without_names().any(|&word| word == names[0]));
        assert_eq!(
            crate::emacs_core::intern::parallel::name_publication_for_test(
                era.heap_identity(),
                id,
                names[1]
            ),
            (Some(names[0]), true, false, true)
        );
        drive(session, owner, frame).unwrap();
        assert_eq!(frame.machine.safepoints, 4);
        eprintln!(
            "P3SYM_C5_EXECUTOR_NAME_GC_WITNESS omitted_live_name_index_loses_only_winner {kind:?}"
        );
    });
}

#[test]
fn p3sym_c5_executor_plain_name_survives_two_stops() {
    sole_name_root(PlainSymbolBacking::Unprojected);
}

#[test]
fn p3sym_c5_executor_projected_name_survives_two_stops() {
    sole_name_root(PlainSymbolBacking::VoidProjection);
}

#[test]
fn p3sym_c5_executor_name_gc_pretrace_unwind_recovers() {
    case(
        PlainSymbolBacking::VoidProjection,
        vec![],
        1,
        |era, session, owner, frame| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                round_with_preflight(era, owner, [TaggedValue::NIL.bits(); 2], || {
                    panic!("C5 executor name-GC pretrace control");
                });
            }));
            let panic = result.expect_err("deliberate pretrace witness");
            let message = panic.downcast_ref::<&str>().copied().unwrap_or("");
            assert_eq!(message, "C5 executor name-GC pretrace control");
            assert_eq!(words(owner, frame), [TaggedValue::NIL.bits()]);
            drive(session, owner, frame).unwrap();
            eprintln!("P3SYM_C5_EXECUTOR_NAME_GC_PRETRACE_RECOVERED owner=1 catalog=1");
        },
    );
}
