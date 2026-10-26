//! Every winner operation runs through the real closed tape and owner homes.
use super::*;
use crate::emacs_core::symbol::parallel::InitialInternFacts;
use crate::tagged::gc::HeapIdentity;
use std::collections::BTreeSet;
use std::sync::Barrier;

const SLOTS: usize = 7;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    SameObject,
    SameName,
    DifferentNames,
}
struct Row {
    input: usize,
    miss: usize,
    result: Vec<usize>,
    id: SymId,
    safepoints: usize,
    conses: usize,
}
struct Receipt {
    rows: Vec<Row>,
    before: InitialInternFacts,
    after: InitialInternFacts,
    cells: Vec<InitialInternFacts>,
    names: Vec<(Option<usize>, bool, bool, bool)>,
    gaps_absent: bool,
    registry_before: (u64, usize),
    registry_after: (u64, usize),
}

fn activate<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
) -> Result<Mutator<'era>, String> {
    retry_owned(
        Deadline::contention(),
        permit,
        |permit| permit.activate(heap, id),
        |failed| match contention::registration(&failed.reason) {
            Retry::Busy => ControlFlow::Continue(failed.into_permit()),
            Retry::Refuse => ControlFlow::Break(failed),
        },
    )
    .map_err(|failed| format!("C5 executor activation: {failed:?}"))
}
fn guarded<'era, T>(
    mut owner: Mutator<'era>,
    body: impl FnOnce(&mut Mutator<'era>) -> Result<T, String>,
) -> Result<T, String> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut owner)))
        .unwrap_or_else(|_| Err("C5 executor actor assertion".into()));
    // Body/partial Frame homes have ended even if seeding or execution failed.
    let cleared = retry_exec(|| with_run(&mut owner, |run| Ok(run.clear_root_slots()?)))
        .map_err(|error| format!("C5 executor clear: {error:?}"));
    retry_owned(
        Deadline::contention(),
        owner,
        |owner| owner.retire_rooted(&[]),
        |failed| match contention::rendezvous(&failed.reason) {
            Retry::Busy => ControlFlow::Continue(failed.into_owner()),
            Retry::Refuse => ControlFlow::Break(failed),
        },
    )
    .map_err(|failed| format!("C5 executor retirement: {failed:?}"))?;
    cleared.and(outcome)
}
fn pump<'era>(
    session: &Session,
    owner: &mut Mutator<'era>,
    frame: &mut Frame<'era>,
    pause: bool,
) -> Result<(), String> {
    loop {
        match session
            .wait_step(owner, frame)
            .map_err(|error| format!("C5 executor step: {error:?}"))?
        {
            Step::Done if !pause => return Ok(()),
            Step::Done => return Err("C5 executor omitted its first safepoint".into()),
            Step::Safepoint if pause => return Ok(()),
            Step::Retry => unreachable!("wait_step resolves contention"),
            Step::Progress | Step::Safepoint => {}
        }
    }
}
fn read_words<'era>(owner: &mut Mutator<'era>, frame: &Frame<'era>) -> Result<Vec<usize>, String> {
    with_run(owner, |run| {
        Ok(frame.slots.iter().map(|s| s.word_for_test(run)).collect())
    })
    .map_err(|error| format!("C5 executor observation: {error:?}"))
}
fn actor<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    index: usize,
    session: &Session,
    nil: &SharedRoot,
    ready: &Barrier,
) -> Result<Row, String> {
    let mut owner = activate(permit, heap, index as u64 + 1);
    let mut frame = None;
    let prepared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let owner = owner.as_mut().map_err(|error| error.clone())?;
        frame = Some(session.seed(owner, index, nil).map_err(|error| {
            format!(
                "C5 executor seed: {error:?}; rooted_slots={}",
                owner
                    .local
                    .as_ref()
                    .map_or(0, |local| local.root_scratch.len())
            )
        })?);
        let frame = frame.as_mut().ok_or("C5 executor frame absent")?;
        pump(session, owner, frame, true)?;
        let result = read_words(owner, frame)?;
        Ok::<_, String>((result[0], result[1]))
    }))
    .unwrap_or_else(|_| Err("C5 executor preparation assertion".into()));
    // ALL actors arrive even if activation, partial seeding or soft lookup
    // refused. No later wait depends on an operation succeeding.
    ready.wait();
    let owner = owner?;
    guarded(owner, move |owner| {
        let (input, miss) = prepared?;
        let frame = frame
            .as_mut()
            .ok_or("C5 executor frame absent after preparation")?;
        pump(session, owner, frame, false)?;
        let result = read_words(owner, frame)?;
        // Observation only: actual rooted output must grant symbol authority.
        retry_exec(|| {
            with_run(owner, |run| {
                run.initial_symbol_handle(&frame.slots[2])?;
                Ok(())
            })
        })
        .map_err(|error| format!("C5 executor rooted answer: {error:?}"))?;
        let id = TaggedValue::from_bits(result[2])
            .as_bare_symbol_id()
            .ok_or("C5 executor answer is not a bare symbol")?;
        Ok(Row {
            input,
            miss,
            result,
            id,
            safepoints: frame.machine.safepoints,
            conses: frame.machine.cons_count,
        })
    })
}

fn observe<T>(
    era: &ParallelHeapEra<'_>,
    observer_id: u64,
    body: impl FnOnce(&mut RunAccess<'_, '_>) -> Result<T, ExecError>,
) -> Result<T, String> {
    let permit = era
        .register(observer_id, 0)
        .map_err(|error| format!("C5 executor observer: {error:?}"))?;
    guarded(
        activate(permit, era.heap_identity(), observer_id)?,
        |owner| with_run(owner, body).map_err(|error| format!("C5 executor facts: {error:?}")),
    )
}
fn exercise(
    era: &ParallelHeapEra<'_>,
    ids: &[SymId],
    scenario: Scenario,
    partial_seed: bool,
) -> Result<Receipt, String> {
    let count = ids.len() - 1;
    let mut catalog_started = false;
    let outcome = (|| {
        let tapes: Vec<_> = (0..count)
            .map(|index| {
                source(
                    vec![
                        load(
                            if scenario == Scenario::SameObject {
                                0
                            } else {
                                index as u16
                            },
                            0,
                        ),
                        soft(0, 1),
                        Op::Safepoint,
                        intern(0, 2),
                        intern(0, 3),
                        soft(0, 4),
                        Op::Eq { dst: 5, a: 2, b: 3 },
                        Op::Eq { dst: 6, a: 2, b: 4 },
                        Op::Safepoint,
                    ],
                    SLOTS,
                )
            })
            .collect();
        let session = Session::admit(era, &ids[1..], &tapes)
            .map_err(|error| format!("C5 executor admission: {error:?}"))?;
        let mut stopped = era
            .stop()
            .map_err(|error| format!("C5 executor seed stop: {error:?}"))?;
        let nil = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: actual matching stopped heap and immediate NIL only.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .map_err(|error| format!("C5 executor seed heap: {error:?}"))?;
        stopped
            .trace(|_, _| Ok(()))
            .map_err(|error| format!("C5 executor seed trace: {error:?}"))?;
        stopped
            .complete()
            .map_err(|failed| format!("C5 executor seed complete: {:?}", failed.reason))?;
        era.start_cons_storage()
            .map_err(|error| format!("C5 executor catalog start: {error:?}"))?;
        catalog_started = true;
        // Even facts observers start only AFTER the real catalog is installed.
        let before = observe(era, 9000, |run| Ok(run.initial_facts_for_test(ids[0])?))?;
        let registry_before = crate::emacs_core::intern::parallel::identity_census_for_test();
        let permits = (0..count)
            .map(|index| {
                era.register(
                    index as u64 + 1,
                    if partial_seed && index == 0 { 1 } else { SLOTS },
                )
                .map_err(|error| format!("C5 executor registration: {error:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ready = Barrier::new(count);
        let heap = era.heap_identity();
        let rows = thread::scope(|scope| {
            let jobs: Vec<_> = permits
                .into_iter()
                .enumerate()
                .map(|(index, permit)| {
                    let (session, nil, ready) = (&session, &nil, &ready);
                    scope.spawn(move || actor(permit, heap, index, session, nil, ready))
                })
                .collect();
            // Join EVERY actor before extracting any failure or property.
            jobs.into_iter()
                .map(|job| {
                    job.join()
                        .unwrap_or_else(|_| Err("C5 executor actor join assertion".into()))
                })
                .collect::<Vec<_>>()
        });
        if partial_seed {
            // The injected actor must fail after installing one actual home;
            // every other actor must finish its tape, not merely clean up.
            let expected = "C5 executor seed: Registration(Capacity); rooted_slots=1";
            if rows
                .first()
                .and_then(|row| row.as_ref().err())
                .map(String::as_str)
                != Some(expected)
                || rows.iter().skip(1).any(Result::is_err)
            {
                let errors: Vec<_> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(index, row)| row.as_ref().err().map(|error| (index, error)))
                    .collect();
                return Err(format!(
                    "C5 executor partial-seed control outcomes: {errors:?}"
                ));
            }
        }
        let rows = rows.into_iter().collect::<Result<Vec<_>, _>>()?;
        let registry_after = crate::emacs_core::intern::parallel::identity_census_for_test();
        let (after, cells, gaps_absent) = observe(era, 9001, |run| {
            let after = run.initial_facts_for_test(ids[0])?;
            let cells = rows
                .iter()
                .map(|row| run.initial_facts_for_test(row.id))
                .collect::<Result<Vec<_>, _>>()?;
            let first = rows
                .iter()
                .map(|row| row.id.0)
                .min()
                .ok_or(ExecError::Slot)?;
            let gaps_absent = first > 0 && !run.initial_facts_for_test(SymId(first - 1))?.present;
            Ok((after, cells, gaps_absent))
        })?;
        let names = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                crate::emacs_core::intern::parallel::name_publication_for_test(
                    heap,
                    row.id,
                    rows[(index + 1) % count].input,
                )
            })
            .collect();
        Ok(Receipt {
            rows,
            before,
            after,
            cells,
            names,
            gaps_absent,
            registry_before,
            registry_after,
        })
    })();
    // Shared seed and all owner frames have ended before catalog return.
    if catalog_started {
        let mut stopped = era
            .stop()
            .map_err(|error| format!("C5 executor final stop: {error:?}"))?;
        era.return_cons_storage(&stopped)
            .map_err(|error| format!("C5 executor catalog return: {error:?}"))?;
        stopped
            .trace(|_, _| Ok(()))
            .map_err(|error| format!("C5 executor final trace: {error:?}"))?;
        stopped
            .complete()
            .map_err(|failed| format!("C5 executor final complete: {:?}", failed.reason))?;
    }
    outcome
}

fn cohort(
    kind: PlainSymbolBacking,
    scenario: Scenario,
    count: usize,
    partial_seed: bool,
) -> Result<Receipt, String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let prefix = format!(
        "p3sym-c5-tape-history-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let spellings: Vec<_> = (0..count)
        .map(|index| {
            format!(
                "{prefix}-{}",
                if scenario == Scenario::DifferentNames {
                    index
                } else {
                    0
                }
            )
        })
        .collect();
    for spelling in &spellings {
        assert!(crate::emacs_core::intern::lookup_interned(spelling).is_none());
    }
    Context::with_test_symbol_intern_race_era(kind, &spellings, |era, ids| {
        let receipt = exercise(&era, &ids, scenario, partial_seed);
        let (collector, keeper) = era
            .finish()
            .expect("C5 executor cohort after root/owner/catalog teardown");
        drop(collector);
        drop(keeper);
        eprintln!(
            "P3SYM_C5_EXECUTOR_COHORT_TEARDOWN_OK {scenario:?} {kind:?} owners={count} partial_seed={partial_seed}"
        );
        receipt
    })
}
fn histories(scenario: Scenario) {
    let oracle: Vec<_> = include_str!("../../../symbol_intern_oracle.expected")
        .lines()
        .collect();
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        for count in 2..=4 {
            let receipt =
                cohort(kind, scenario, count, false).expect("C5 executor after clean era finish");
            let ids: BTreeSet<_> = receipt.rows.iter().map(|row| row.id.0).collect();
            let inputs: BTreeSet<_> = receipt.rows.iter().map(|row| row.input).collect();
            let expected = if scenario == Scenario::DifferentNames {
                count
            } else {
                1
            };
            assert_eq!(ids.len(), expected);
            assert_eq!(
                inputs.len(),
                if scenario == Scenario::SameObject {
                    1
                } else {
                    count
                }
            );
            assert_eq!(
                receipt.registry_after,
                (
                    receipt.registry_before.0 + expected as u64,
                    receipt.registry_before.1 + expected
                )
            );
            assert_eq!(
                (receipt.after.members, receipt.after.epoch),
                (
                    receipt.before.members + expected,
                    receipt.before.epoch + expected as u64
                )
            );
            assert!(
                receipt.after.chunks >= receipt.before.chunks + 2
                    && receipt.after.slots > receipt.before.slots
            );
            assert_eq!(receipt.after.chunks, receipt.after.sides);
            assert!(receipt.gaps_absent);
            assert_eq!(receipt.after.projections, receipt.before.projections);
            for (index, row) in receipt.rows.iter().enumerate() {
                assert_eq!(prin1(row.miss), oracle[0]);
                assert_eq!(row.result[0], row.input);
                assert_eq!(row.result[1], row.miss);
                assert_eq!(row.result[2], TaggedValue::from_sym_id(row.id).bits());
                assert_eq!(
                    (row.result[2], row.result[3]),
                    (row.result[4], row.result[4])
                );
                assert_eq!(prin1(row.result[5]), oracle[2]);
                assert_eq!(prin1(row.result[6]), oracle[3]);
                assert_eq!((row.safepoints, row.conses), (2, 0));
                let cell = &receipt.cells[index];
                assert!(cell.present && cell.member && cell.spine_current);
                assert_eq!(cell.backing, Some(kind));
                assert_eq!(
                    cell.cells,
                    Some((
                        TaggedValue::NIL.bits(),
                        TaggedValue::NIL.bits(),
                        false,
                        false
                    ))
                );
                let name = receipt.names[index];
                assert!(name.1 && name.3);
                let winner = name.0.expect("C5 executor winning exact name index");
                assert!(inputs.contains(&winner));
                if scenario == Scenario::DifferentNames {
                    assert_eq!(winner, row.input);
                    assert!(
                        name.2,
                        "every other successful spelling has its own indexed name"
                    );
                } else {
                    assert_eq!(name.2, receipt.rows[(index + 1) % count].input == winner);
                }
            }
            eprintln!(
                "P3SYM_C5_EXECUTOR_COHORT {scenario:?} {kind:?} owners={count} winners={} members_added={} chunks_added={} safepoints_per_owner=2 conses=0",
                ids.len(),
                receipt.after.members - receipt.before.members,
                receipt.after.chunks - receipt.before.chunks
            );
        }
    }
}

#[test]
fn p3sym_c5_executor_cohort_same_object_winner() {
    histories(Scenario::SameObject);
}
#[test]
fn p3sym_c5_executor_cohort_equal_name_winner() {
    histories(Scenario::SameName);
}
#[test]
fn p3sym_c5_executor_cohort_distinct_name_growth() {
    histories(Scenario::DifferentNames);
}
#[test]
fn p3sym_c5_executor_cohort_partial_seed_still_joins_and_cleans_all_owners() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        let error = match cohort(kind, Scenario::SameName, 4, true) {
            Err(error) => error,
            Ok(_) => panic!("undersized cohort root bank unexpectedly seeded"),
        };
        assert_eq!(
            error,
            "C5 executor seed: Registration(Capacity); rooted_slots=1"
        );
        eprintln!("P3SYM_C5_EXECUTOR_COHORT_PARTIAL_SEED cleaned=4 joined=4 completed=3 {kind:?}");
    }
}
