//! First-young exact-name root, followed by an old-winner repetition.
//! No warm GC or new parallel string allocator; all three test inputs exist
//! before the era and the closed tape transfers their actual managed graph.
use super::fresh::{Cycle, dispatch_role};
use super::minor_adapter::{Age, age};
use super::*;
use crate::emacs_core::intern::SymId;

#[derive(Clone, Copy)]
pub(super) enum Layout {
    SharedTarget,
    SeparateTarget,
}
impl Layout {
    pub(super) fn for_scenario(scenario: Scenario) -> Self {
        match scenario {
            Scenario::NameMinor | Scenario::NameMinorNoop | Scenario::NameMinorUnwind => {
                Self::SeparateTarget
            }
            Scenario::Transfer
            | Scenario::Noop
            | Scenario::Unwind
            | Scenario::MinorAdapter
            | Scenario::Fresh
            | Scenario::FreshNoop
            | Scenario::FreshUnwind
            | Scenario::Relog
            | Scenario::RelogNoop
            | Scenario::RelogUnwind => Self::SharedTarget,
        }
    }
    pub(super) fn sources<T>(self, all: &[T]) -> [&T; 2] {
        match self {
            Self::SharedTarget => [&all[0], &all[1]],
            Self::SeparateTarget => [&all[1], &all[2]],
        }
    }
}
pub(super) enum Setup {
    Unused,
    Pair {
        candidate: WordIdentity,
        spelling: Box<str>,
    },
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Winner {
    id: SymId,
    soft: SymId,
    name: WordIdentity,
}
pub(super) fn observe(worker: &mut Worker<'_>) -> Winner {
    // Actual checked owner homes, before the separate final-clear phase.
    // Symbol results decode totally; the name remains an opaque observation.
    let id = TaggedValue::from_bits(worker.word(home(2)).0)
        .as_bare_symbol_id()
        .expect("intern returns a symbol");
    let soft = TaggedValue::from_bits(worker.word(home(0)).0)
        .as_bare_symbol_id()
        .expect("soft finds the same symbol");
    Winner {
        id,
        soft,
        name: worker.word(home(1)),
    }
}
#[derive(Clone, Copy, Debug)]
enum Pass {
    FirstYoung,
    SecondRepeat,
}
#[derive(Clone, Copy)]
enum Census {
    Baseline,
    SoleName(Winner),
}
#[derive(Debug, PartialEq, Eq)]
enum Text {
    Missing,
    Bytes(Vec<u8>),
}
struct Snapshot {
    graph: [Age; 4],
    candidate: Age,
    candidate_text: Text,
    text: Text,
}
fn snapshot(
    heap: &crate::tagged::gc::TaggedHeap,
    graph: GraphWords,
    candidate: WordIdentity,
) -> Snapshot {
    let ages = graph.0.map(|word| age(heap, word));
    let text = match ages[3] {
        Age::Missing => Text::Missing,
        Age::Young | Age::Old => {
            // Membership is established before decoding the exact known name.
            let name = TaggedValue::from_bits(graph.name().0);
            Text::Bytes(name.as_lisp_string().unwrap().as_bytes().to_vec())
        }
    };
    let candidate_age = age(heap, candidate);
    let candidate_text = match candidate_age {
        Age::Missing => Text::Missing,
        Age::Young | Age::Old => Text::Bytes(
            TaggedValue::from_bits(candidate.0)
                .as_lisp_string()
                .unwrap()
                .as_bytes()
                .to_vec(),
        ),
    };
    Snapshot {
        graph: ages,
        candidate: candidate_age,
        candidate_text,
        text,
    }
}
struct Receipt {
    epoch: CollectionEpoch,
    cycle: CollectionCycle,
}
fn round(
    era: &ParallelHeapEra<'_>,
    graph: GraphWords,
    names: &Setup,
    cohort: Cohort,
    pass: Pass,
    census: Census,
) -> Receipt {
    let (candidate, spelling) = match names {
        Setup::Unused => panic!("name pair not prepared"),
        Setup::Pair {
            candidate,
            spelling,
        } => (*candidate, spelling),
    };
    let mut stopped = contention::retry(
        Deadline::contention(),
        || era.stop(),
        contention::rendezvous,
    )
    .unwrap();
    stopped.symbols().unwrap().reconcile_functions().unwrap();
    let roots = ledger(era, &mut stopped);
    let (cycle, before) = era
        .with_stopped_heap_for_test(&stopped, |heap| {
            let cycle = match heap.generational_enabled() {
                true => Cycle::Minor,
                false => Cycle::Full,
            };
            (cycle, snapshot(heap, graph, candidate))
        })
        .unwrap();
    let report = match cycle {
        Cycle::Minor => era.collect_stopped_minor(&mut stopped).unwrap(),
        Cycle::Full => era.collect_stopped(&mut stopped).unwrap(),
    };
    let after = era
        .with_stopped_heap_for_test(&stopped, |heap| snapshot(heap, graph, candidate))
        .unwrap();
    let response = ledger(era, &mut stopped);
    stopped.complete().unwrap();
    assert!(
        response.young_owners.is_empty()
            && response.cons_writes.is_empty()
            && response.other_work.is_empty(),
        "canonical response inventory/history drained: {response:?}"
    );
    assert_eq!(report.major, matches!(cycle, Cycle::Full));
    assert_eq!(report.frozen_owners, cohort.roles().len());
    let survivor_age = match cycle {
        Cycle::Minor => Age::Old,
        Cycle::Full => Age::Young,
    };
    match pass {
        Pass::FirstYoung => {
            assert_eq!(
                before.graph,
                [Age::Young; 4],
                "no earlier collection promoted the transferred graph"
            );
            assert_eq!(
                before.candidate,
                Age::Young,
                "unused equal name also starts young"
            );
            assert_eq!(
                before.candidate_text,
                Text::Bytes(spelling.as_bytes().to_vec())
            );
            assert_eq!(
                before.text, before.candidate_text,
                "both first-stop names retain equal bytes"
            );
        }
        Pass::SecondRepeat => {
            assert_eq!(
                before.graph[3], survivor_age,
                "second pass repeats an old winner in GEN1"
            );
            assert_eq!(before.candidate, Age::Missing);
        }
    }
    assert_eq!(
        after.candidate,
        Age::Missing,
        "unused equal name must not enter the exact-name index"
    );
    assert_eq!(after.text, Text::Bytes(spelling.as_bytes().to_vec()));
    match census {
        Census::Baseline => assert_eq!(
            after.graph, [survivor_age; 4],
            "no-op fixture retains the source global"
        ),
        Census::SoleName(winner) => {
            assert_eq!(winner.id, winner.soft);
            assert_eq!(
                winner.name,
                graph.name(),
                "call returns the same physical name"
            );
            assert_eq!(
                after.graph,
                [Age::Missing, Age::Missing, Age::Missing, survivor_age],
                "unrooted definition reclaimed; exact winner survives"
            );
            assert_eq!(
                roots
                    .names
                    .iter()
                    .copied()
                    .filter(|word| graph.0.contains(word) || *word == candidate)
                    .collect::<Vec<_>>(),
                vec![graph.name()]
            );
            assert!(
                roots
                    .without_names()
                    .all(|word| !graph.0.contains(word) && *word != candidate),
                "World/projection/function/journal/owner/frozen/transport roots cannot mask exact index: {roots:?}"
            );
            assert!(
                roots.other_work.is_empty() && roots.cons_writes.is_empty(),
                "no birth/preimage/current-child history may retain a tested graph: {roots:?}"
            );
            // Setup's legacy young block inventory is a sweep candidate, not
            // a graph seed. Its three dead conses and equal dead name provide
            // the actual sweep control. No inventory is removed from the job.
            assert!(roots.young_owners.iter().all(|peer| *peer == OwnerId(0)));
            assert_eq!(
                crate::emacs_core::intern::parallel::name_publication_for_test(
                    era.heap_identity(),
                    winner.id,
                    candidate.0
                ),
                (Some(winner.name.0), true, false, true)
            );
            eprintln!(
                "P3SYM_C6_NAME_MINOR_WITNESS {pass:?} omitted_exact_index_no_other_seed winner_only_equal_candidate_reclaimed {cohort:?}"
            );
        }
    }
    eprintln!(
        "P3SYM_C6_NAME_MINOR_COLLECTION kind={cycle:?} pass={pass:?} epoch={} cycle={} owners={}",
        report.epoch, report.cycle, report.frozen_owners
    );
    Receipt {
        epoch: CollectionEpoch(report.epoch),
        cycle: CollectionCycle(report.cycle),
    }
}
fn advance(commands: &Commands, await_all: &impl Fn()) {
    for tx in &commands.0 {
        tx.send(Command::Advance).unwrap();
    }
    await_all();
}
pub(super) fn exercise(
    era: &ParallelHeapEra<'_>,
    graph: GraphWords,
    names: &Setup,
    cohort: Cohort,
    scenario: Scenario,
    commands: &Commands,
    await_all: &impl Fn(),
    facts: &mpsc::Receiver<Construction>,
) {
    let census = match scenario {
        Scenario::NameMinorNoop => Census::Baseline,
        Scenario::NameMinor | Scenario::NameMinorUnwind => {
            for _ in [
                Phase::Function,
                Phase::Selected,
                Phase::Replaced,
                Phase::Interned,
            ] {
                advance(commands, await_all);
            }
            dispatch_role(
                commands,
                cohort,
                Role::Caller,
                Command::ObserveName,
                await_all,
            );
            let winner = match facts
                .try_recv()
                .expect("intern result before native publication")
            {
                Construction::Winner(winner) => winner,
                Construction::Definition(_) | Construction::Relog(_) => {
                    panic!("wrong native observation kind")
                }
            };
            advance(commands, await_all); // final actual cache/activation/home clears
            Census::SoleName(winner)
        }
        Scenario::Transfer
        | Scenario::Noop
        | Scenario::Unwind
        | Scenario::MinorAdapter
        | Scenario::Fresh
        | Scenario::FreshNoop
        | Scenario::FreshUnwind
        | Scenario::Relog
        | Scenario::RelogNoop
        | Scenario::RelogUnwind => panic!("wrong name-minor scenario"),
    };
    let first = round(era, graph, names, cohort, Pass::FirstYoung, census);
    for tx in &commands.0 {
        tx.send(Command::Repark).unwrap();
    }
    await_all();
    match scenario {
        Scenario::NameMinorUnwind => {
            panic!("controlled exact-name unwind after first collection response")
        }
        Scenario::NameMinor | Scenario::NameMinorNoop => {}
        Scenario::Transfer
        | Scenario::Noop
        | Scenario::Unwind
        | Scenario::MinorAdapter
        | Scenario::Fresh
        | Scenario::FreshNoop
        | Scenario::FreshUnwind
        | Scenario::Relog
        | Scenario::RelogNoop
        | Scenario::RelogUnwind => panic!("wrong name-minor scenario"),
    }
    let second = round(era, graph, names, cohort, Pass::SecondRepeat, census);
    assert!(second.epoch > first.epoch && second.cycle > first.cycle);
}

#[test]
fn p3sym_c6_name_minor_four_plain() {
    cohort(
        PlainSymbolBacking::Unprojected,
        Cohort::Four,
        Scenario::NameMinor,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_name_minor_four_void() {
    cohort(
        PlainSymbolBacking::VoidProjection,
        Cohort::Four,
        Scenario::NameMinor,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_name_minor_noop_releases_banks() {
    cohort(
        PlainSymbolBacking::VoidProjection,
        Cohort::Two,
        Scenario::NameMinorNoop,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_name_minor_unwind_releases_banks() {
    let error = cohort(
        PlainSymbolBacking::VoidProjection,
        Cohort::Two,
        Scenario::NameMinorUnwind,
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<&str>(),
        Some(&"controlled exact-name unwind after first collection response")
    );
}

static_assertions::assert_impl_all!(Winner: Send, Sync, Copy);
