//! Trusted, bounded native construction, outside the closed tape language.
//! Every tested definition is allocated after a completed collection response.
use super::minor_adapter::{Age, age};
use super::*;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::transport::SharedRoot;
use std::sync::Arc;

pub(super) struct Atoms {
    lambda: SharedRoot,
    pub(super) answer: SharedRoot,
    pub(super) nil: SharedRoot,
}
#[derive(Clone)]
pub(super) enum Preparation {
    Disabled,
    Enabled(Arc<Atoms>),
}
impl Preparation {
    pub(super) fn admit(
        era: &ParallelHeapEra<'_>,
        lambda: crate::emacs_core::intern::SymId,
        scenario: Scenario,
    ) -> Self {
        match scenario {
            Scenario::Transfer | Scenario::Noop | Scenario::Unwind | Scenario::MinorAdapter => {
                Self::Disabled
            }
            Scenario::Fresh
            | Scenario::FreshUnwind
            | Scenario::FreshNoop
            | Scenario::Relog
            | Scenario::RelogNoop
            | Scenario::RelogUnwind => {
                let mut stopped = contention::retry(
                    Deadline::contention(),
                    || era.stop(),
                    contention::rendezvous,
                )
                .unwrap();
                let atoms = era
                    .with_stopped_heap_for_test(&stopped, |heap| {
                        // SAFETY: matching real stop and retained serial heap; lambda
                        // was resolved in this registry before era issuance. Only
                        // this symbol and immediates enter transport, never a graph.
                        unsafe {
                            Atoms {
                                lambda: SharedRoot::new(heap, TaggedValue::from_sym_id(lambda)),
                                answer: SharedRoot::new(heap, TaggedValue::fixnum(97)),
                                nil: SharedRoot::new(heap, TaggedValue::NIL),
                            }
                        }
                    })
                    .unwrap();
                stopped.trace(|_, _| Ok(())).unwrap();
                stopped.complete().unwrap();
                Self::Enabled(Arc::new(atoms))
            }
        }
    }
    pub(super) fn construct(&self, worker: &mut Worker<'_>) -> FreshGraph {
        let atoms = match self {
            Self::Disabled => panic!("fresh preparation not admitted"),
            Self::Enabled(atoms) => atoms,
        };
        let frame = match &worker.frame {
            FrameState::Empty => panic!("fresh preparation before frame seed"),
            FrameState::Ready(frame) => frame,
        };
        // Clones alias existing homes: no extra root, allocation or raw store.
        let mut body = frame.home(home(0)).unwrap().clone();
        let mut args = frame.home(home(1)).unwrap().clone();
        let mut head = frame.home(home(2)).unwrap().clone();
        with_allocation(&mut worker.owner, |run| {
            run.cons_into(&atoms.answer, &atoms.nil, &mut body)?;
            run.cons_into(&atoms.nil, &body, &mut args)?;
            run.cons_into(&atoms.lambda, &args, &mut head)?;
            let nodes = [&head, &args, &body].map(|slot| WordIdentity(slot.word_for_test(run)));
            run.clear_owner_root_for_test(&args)?;
            run.clear_owner_root_for_test(&body)?;
            // A real unrelated young allocation in the SAME inventory;
            // inventory publication must not retain every allocated cell.
            run.cons_into(&atoms.answer, &atoms.nil, &mut body)?;
            let garbage = WordIdentity(body.word_for_test(run));
            run.clear_owner_root_for_test(&body)?;
            Ok(FreshGraph { nodes, garbage })
        })
    }
}

impl Preparation {
    pub(super) fn atoms(&self) -> &Atoms {
        match self {
            Self::Disabled => panic!("native construction not admitted"),
            Self::Enabled(atoms) => atoms,
        }
    }
}
/// The only native allocation wrapper: unwind is caught while the run can
/// still close its real region. It never wraps an unfinished RunSlice panic.
pub(super) fn with_allocation<'era, R>(
    owner: &mut Mutator<'era>,
    mut body: impl FnMut(&mut RunAccess<'_, 'era>) -> Result<R, RegistrationError>,
) -> R {
    contention::retry(
        Deadline::contention(),
        || {
            let mut run = owner.run_access()?;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut run)));
            let close = run.close_cons_region();
            drop(run);
            close?;
            match result {
                Ok(result) => result,
                Err(panic) => std::panic::resume_unwind(panic),
            }
        },
        contention::registration,
    )
    .unwrap()
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FreshGraph {
    nodes: [WordIdentity; 3],
    garbage: WordIdentity,
}
impl FreshGraph {
    fn ages(self, heap: &TaggedHeap) -> ([Age; 3], Age) {
        (
            self.nodes.map(|word| age(heap, word)),
            age(heap, self.garbage),
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Cycle {
    Full,
    Minor,
}
#[must_use = "return the real catalog under a stop before finishing the era"]
pub(super) enum Placement {
    Unused,
    Workers(Cycle),
    Collector(Cycle),
}
impl Placement {
    pub(super) fn start(era: &ParallelHeapEra<'_>, scenario: Scenario) -> Self {
        match scenario {
            Scenario::Transfer | Scenario::Noop | Scenario::Unwind | Scenario::MinorAdapter => {
                Self::Unused
            }
            Scenario::Fresh
            | Scenario::FreshUnwind
            | Scenario::FreshNoop
            | Scenario::Relog
            | Scenario::RelogNoop
            | Scenario::RelogUnwind => {
                let mut stopped = era.stop().unwrap();
                let cycle = era
                    .with_stopped_heap_for_test(&stopped, |heap| {
                        match heap.generational_enabled() {
                            true => Cycle::Minor,
                            false => Cycle::Full,
                        }
                    })
                    .unwrap();
                stopped.trace(|_, _| Ok(())).unwrap();
                stopped.complete().unwrap();
                contention::retry(
                    Deadline::contention(),
                    || era.start_cons_storage(),
                    contention::registration,
                )
                .unwrap();
                Self::Workers(cycle)
            }
        }
    }
    pub(super) fn return_for_finish(
        &mut self,
        era: &ParallelHeapEra<'_>,
        stopped: &AllStopped<'_>,
    ) {
        match *self {
            Self::Unused | Self::Collector(_) => {}
            Self::Workers(cycle) => {
                contention::retry(
                    Deadline::contention(),
                    || era.return_cons_storage(stopped),
                    contention::registration,
                )
                .unwrap();
                *self = Self::Collector(cycle);
            }
        }
    }
    pub(super) fn resume(&mut self, era: &ParallelHeapEra<'_>, stopped: &AllStopped<'_>) {
        match *self {
            Self::Collector(cycle) => {
                contention::retry(
                    Deadline::contention(),
                    || era.resume_cons_storage(stopped),
                    contention::registration,
                )
                .unwrap();
                *self = Self::Workers(cycle);
            }
            Self::Unused | Self::Workers(_) => panic!("fresh response requires returned catalog"),
        }
    }
    pub(super) fn cycle(&self) -> Cycle {
        match *self {
            Self::Workers(cycle) | Self::Collector(cycle) => cycle,
            Self::Unused => panic!("fresh cycle not admitted"),
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Global,
    Function,
    InstallSelected,
    Select,
    Replace,
    Return,
    Release,
}
pub(super) fn program(role: Role) -> CombinedTape {
    let mut code = Vec::new();
    for phase in [
        Phase::Global,
        Phase::Function,
        Phase::InstallSelected,
        Phase::Select,
        Phase::Replace,
        Phase::Return,
        Phase::Release,
    ] {
        match (role, phase) {
            (Role::Publisher, Phase::Global) => {
                code.push(CombinedInstruction::HomeToGlobal {
                    input: InputIndex(0),
                    source: home(2),
                    destination: home(1),
                });
                code.extend(clear_operands());
            }
            (Role::Publisher, Phase::Function | Phase::InstallSelected) => {
                code.push(CombinedInstruction::HomeToFunction {
                    source: home(2),
                    destination: home(1),
                });
                code.extend(clear_operands());
            }
            (Role::Caller, Phase::Select) => {
                code.extend([
                    CombinedInstruction::CallBegin,
                    CombinedInstruction::ClearCache,
                ]);
            }
            (Role::Publisher, Phase::Replace) => {
                code.extend([
                    CombinedInstruction::GlobalToHome {
                        input: InputIndex(1),
                        destination: home(0),
                    },
                    CombinedInstruction::HomeToFunction {
                        source: home(0),
                        destination: home(1),
                    },
                ]);
                code.extend(clear_operands());
            }
            (Role::Caller, Phase::Return) => {
                code.extend([
                    CombinedInstruction::CallStep {
                        destination: home(1),
                    },
                    CombinedInstruction::CallStep {
                        destination: home(1),
                    },
                ]);
            }
            (Role::Caller, Phase::Release) => {
                code.extend([
                    CombinedInstruction::ClearCache,
                    CombinedInstruction::ClearActivation,
                ]);
                code.extend(clear_operands());
            }
            (Role::Publisher, Phase::Select | Phase::Return | Phase::Release)
            | (
                Role::Caller,
                Phase::Global | Phase::Function | Phase::InstallSelected | Phase::Replace,
            )
            | (
                Role::Observer,
                Phase::Global
                | Phase::Function
                | Phase::InstallSelected
                | Phase::Select
                | Phase::Replace
                | Phase::Return
                | Phase::Release,
            ) => {}
        }
        code.push(CombinedInstruction::Safepoint);
    }
    tape(code)
}
#[derive(Clone, Copy, Debug)]
enum Observation {
    Warm,
    Current(FreshGraph),
    Released([FreshGraph; 3]),
}
struct Receipt {
    roots: Ledger,
    epoch: CollectionEpoch,
    cycle: CollectionCycle,
}
fn round(
    era: &ParallelHeapEra<'_>,
    cohort: Cohort,
    placement: &RefCell<Placement>,
    observation: Observation,
) -> Receipt {
    let mut stopped = contention::retry(
        Deadline::contention(),
        || era.stop(),
        contention::rendezvous,
    )
    .unwrap();
    let mut placement = placement.borrow_mut();
    placement.return_for_finish(era, &stopped);
    let cycle = match observation {
        Observation::Warm | Observation::Current(_) => placement.cycle(),
        Observation::Released(_) => Cycle::Full,
    };
    stopped.symbols().unwrap().reconcile_functions().unwrap();
    let roots = ledger(era, &mut stopped);
    let before = match observation {
        Observation::Current(graph) => era
            .with_stopped_heap_for_test(&stopped, |heap| graph.ages(heap))
            .unwrap(),
        Observation::Warm | Observation::Released(_) => ([Age::Missing; 3], Age::Missing),
    };
    let report = match cycle {
        Cycle::Full => era.collect_stopped(&mut stopped).unwrap(),
        Cycle::Minor => era.collect_stopped_minor(&mut stopped).unwrap(),
    };
    let after = match observation {
        Observation::Current(graph) => vec![
            era.with_stopped_heap_for_test(&stopped, |heap| graph.ages(heap))
                .unwrap(),
        ],
        Observation::Released(graphs) => era
            .with_stopped_heap_for_test(&stopped, |heap| {
                graphs.map(|graph| graph.ages(heap)).to_vec()
            })
            .unwrap(),
        Observation::Warm => Vec::new(),
    };
    placement.resume(era, &stopped);
    stopped.complete().unwrap();
    assert_eq!(report.major, matches!(cycle, Cycle::Full));
    assert_eq!(report.frozen_owners, cohort.roles().len());
    match observation {
        Observation::Warm => {}
        Observation::Current(_) => {
            assert_eq!(
                before,
                ([Age::Young; 3], Age::Young),
                "fresh after previous completed response"
            );
            let expected_age = match cycle {
                Cycle::Minor => Age::Old,
                Cycle::Full => Age::Young,
            };
            assert_eq!(
                after,
                vec![([expected_age; 3], Age::Missing)],
                "fresh closure survives, unrelated young cell reclaimed"
            );
            assert!(
                roots.other_work.is_empty() && roots.cons_writes.is_empty(),
                "unexpected root-bearing work: {roots:?}"
            );
            assert!(
                roots.young_owners.iter().all(|owner| *owner == OwnerId(1)),
                "real young inventory belongs to constructor peer: {roots:?}"
            );
            match cycle {
                Cycle::Minor => assert!(
                    !roots.young_owners.is_empty(),
                    "new real young inventory required"
                ),
                Cycle::Full => {}
            }
        }
        Observation::Released(_) => {
            let age = match placement.cycle() {
                Cycle::Minor => Age::Old,
                Cycle::Full => Age::Young,
            };
            assert_eq!(
                after,
                vec![
                    ([age; 3], Age::Missing),
                    ([Age::Missing; 3], Age::Missing),
                    ([Age::Missing; 3], Age::Missing)
                ],
                "global survives; replaced and released selected definitions reclaimed by full job"
            );
        }
    }
    eprintln!(
        "P3SYM_C6_FRESH_COLLECTION kind={cycle:?} observation={observation:?} epoch={} cycle={} owners={}",
        report.epoch, report.cycle, report.frozen_owners
    );
    Receipt {
        roots,
        epoch: CollectionEpoch(report.epoch),
        cycle: CollectionCycle(report.cycle),
    }
}
fn dispatch(commands: &Commands, command: Command, await_all: &impl Fn()) {
    for tx in &commands.0 {
        tx.send(command).unwrap();
    }
    await_all();
}
pub(super) fn dispatch_role(
    commands: &Commands,
    cohort: Cohort,
    recipient: Role,
    command: Command,
    await_all: &impl Fn(),
) {
    for (role, tx) in cohort.roles().iter().zip(&commands.0) {
        tx.send(if *role == recipient {
            command
        } else {
            Command::Repark
        })
        .unwrap();
    }
    await_all();
}
fn prepare(
    commands: &Commands,
    cohort: Cohort,
    await_all: &impl Fn(),
    facts: &mpsc::Receiver<Construction>,
) -> FreshGraph {
    dispatch_role(
        commands,
        cohort,
        Role::Publisher,
        Command::PrepareFresh,
        await_all,
    );
    match facts
        .try_recv()
        .expect("constructor publishes facts before native acknowledgment")
    {
        Construction::Definition(graph) => graph,
        Construction::Relog(_) => panic!("wrong native construction kind"),
    }
}
fn sole_world(roots: &Ledger, graph: FreshGraph) {
    assert_eq!(
        roots
            .world
            .iter()
            .copied()
            .filter(|word| graph.nodes.contains(word))
            .collect::<Vec<_>>(),
        vec![graph.nodes[0]]
    );
    assert!(
        roots
            .without_world()
            .all(|word| !graph.nodes.contains(word)),
        "fresh World root masked: {roots:?}"
    );
}
pub(super) fn exercise(
    era: &ParallelHeapEra<'_>,
    cohort: Cohort,
    scenario: Scenario,
    commands: &Commands,
    await_all: &impl Fn(),
    facts: &mpsc::Receiver<Construction>,
    placement: &RefCell<Placement>,
) {
    let warm = round(era, cohort, placement, Observation::Warm);
    dispatch(commands, Command::Repark, await_all);
    match scenario {
        Scenario::FreshNoop => return,
        Scenario::Fresh | Scenario::FreshUnwind => {}
        Scenario::Transfer
        | Scenario::Noop
        | Scenario::Unwind
        | Scenario::MinorAdapter
        | Scenario::Relog
        | Scenario::RelogNoop
        | Scenario::RelogUnwind => {
            panic!("wrong fresh scenario")
        }
    }
    let global = prepare(commands, cohort, await_all, facts);
    dispatch(commands, Command::Advance, await_all);
    let first = round(era, cohort, placement, Observation::Current(global));
    sole_world(&first.roots, global);
    assert!(first.epoch > warm.epoch && first.cycle > warm.cycle);
    dispatch(commands, Command::Repark, await_all);
    match scenario {
        Scenario::FreshUnwind => panic!("controlled fresh unwind after completed young collection"),
        Scenario::Fresh | Scenario::FreshNoop => {}
        Scenario::Transfer
        | Scenario::Noop
        | Scenario::Unwind
        | Scenario::MinorAdapter
        | Scenario::Relog
        | Scenario::RelogNoop
        | Scenario::RelogUnwind => {
            panic!("wrong fresh scenario")
        }
    }
    let function = prepare(commands, cohort, await_all, facts);
    dispatch(commands, Command::Advance, await_all);
    let second = round(era, cohort, placement, Observation::Current(function));
    sole_world(&second.roots, function);
    assert!(second.epoch > first.epoch && second.cycle > first.cycle);
    dispatch(commands, Command::Repark, await_all);
    let selected = prepare(commands, cohort, await_all, facts);
    for _ in [Phase::InstallSelected, Phase::Select, Phase::Replace] {
        dispatch(commands, Command::Advance, await_all);
    }
    let third = round(era, cohort, placement, Observation::Current(selected));
    assert!(third.epoch > second.epoch && third.cycle > second.cycle);
    assert!(
        third
            .roots
            .without_owners()
            .all(|word| !selected.nodes.contains(word)),
        "fresh selected activation masked: {:?}",
        third.roots
    );
    let owners: Vec<_> = third
        .roots
        .owners
        .iter()
        .filter(|(_, words)| words.iter().any(|word| selected.nodes.contains(word)))
        .collect();
    assert_eq!(owners.len(), 1);
    assert_eq!(owners[0].0, OwnerId(2));
    assert_eq!(
        owners[0]
            .1
            .iter()
            .copied()
            .filter(|word| selected.nodes.contains(word))
            .collect::<Vec<_>>(),
        vec![selected.nodes[0]]
    );
    dispatch(commands, Command::Advance, await_all);
    dispatch_role(
        commands,
        cohort,
        Role::Caller,
        Command::CheckFreshAnswer,
        await_all,
    );
    dispatch(commands, Command::Advance, await_all);
    let final_receipt = round(
        era,
        cohort,
        placement,
        Observation::Released([global, function, selected]),
    );
    assert!(final_receipt.epoch > third.epoch && final_receipt.cycle > third.cycle);
    eprintln!(
        "P3SYM_C6_FRESH_WITNESSES global_only function_only selected_peer_only young_sweep full_reclamation {cohort:?}"
    );
}

#[test]
fn p3sym_c6_fresh_two_plain() {
    cohort(
        PlainSymbolBacking::Unprojected,
        Cohort::Two,
        Scenario::Fresh,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_fresh_two_projected() {
    cohort(PlainSymbolBacking::Projected, Cohort::Two, Scenario::Fresh).unwrap();
}
#[test]
fn p3sym_c6_fresh_four_plain() {
    cohort(
        PlainSymbolBacking::Unprojected,
        Cohort::Four,
        Scenario::Fresh,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_fresh_four_projected() {
    cohort(PlainSymbolBacking::Projected, Cohort::Four, Scenario::Fresh).unwrap();
}
#[test]
fn p3sym_c6_fresh_unwind_returns_catalog_and_releases_banks() {
    let error = cohort(
        PlainSymbolBacking::Projected,
        Cohort::Two,
        Scenario::FreshUnwind,
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<&str>(),
        Some(&"controlled fresh unwind after completed young collection")
    );
}

#[test]
fn p3sym_c6_fresh_noop_returns_catalog_and_releases_banks() {
    cohort(
        PlainSymbolBacking::Projected,
        Cohort::Two,
        Scenario::FreshNoop,
    )
    .unwrap();
}
