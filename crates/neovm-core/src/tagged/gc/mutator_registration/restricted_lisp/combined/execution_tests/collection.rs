//! Real independent owners, exact root families, and completed full jobs.
//! Counterfactual omissions inspect copied ledgers, never a damaged live heap.
use super::*;
use crate::tagged::gc::mutator_registration::image::PublishedGcWork;
use std::sync::mpsc;

#[derive(Clone, Copy, Debug)]
enum Cohort {
    Two,
    Four,
}
impl Cohort {
    fn roles(self) -> &'static [Role] {
        match self {
            Self::Two => &[Role::Publisher, Role::Caller],
            Self::Four => &[
                Role::Publisher,
                Role::Caller,
                Role::Observer,
                Role::Observer,
            ],
        }
    }
}
#[derive(Clone, Copy)]
enum Role {
    Publisher,
    Caller,
    Observer,
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Function,
    Selected,
    Replaced,
    Interned,
}
#[derive(Clone, Copy)]
enum Scenario {
    Transfer,
    Noop,
    Unwind,
}
enum GraphSetup {
    Empty,
    Ready(GraphWords),
}
#[derive(Clone, Copy)]
enum Command {
    Advance,
    Repark,
    Finish,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnerId(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Retention {
    Live,
    Reclaimed,
}
#[derive(Clone, Copy)]
struct GraphWords([WordIdentity; 4]);
impl GraphWords {
    fn definition(self) -> WordIdentity {
        self.0[0]
    }
    fn name(self) -> WordIdentity {
        self.0[3]
    }
}
#[derive(Default, Debug)]
struct Ledger {
    world: Vec<WordIdentity>,
    names: Vec<WordIdentity>,
    journal: Vec<WordIdentity>,
    external: Vec<WordIdentity>,
    tables: Vec<WordIdentity>,
    owners: Vec<(OwnerId, Vec<WordIdentity>)>,
    other_work: Vec<String>,
}
impl Ledger {
    fn without_world(&self) -> impl Iterator<Item = &WordIdentity> {
        self.names
            .iter()
            .chain(&self.journal)
            .chain(&self.external)
            .chain(&self.tables)
            .chain(self.owners.iter().flat_map(|(_, roots)| roots))
    }
    fn without_owners(&self) -> impl Iterator<Item = &WordIdentity> {
        self.world
            .iter()
            .chain(&self.names)
            .chain(&self.journal)
            .chain(&self.external)
            .chain(&self.tables)
    }
    fn without_names(&self) -> impl Iterator<Item = &WordIdentity> {
        self.world
            .iter()
            .chain(&self.journal)
            .chain(&self.external)
            .chain(&self.tables)
            .chain(self.owners.iter().flat_map(|(_, roots)| roots))
    }
}
fn ledger(era: &ParallelHeapEra<'_>, stopped: &mut AllStopped<'_>) -> Ledger {
    let mut result = Ledger::default();
    let mut words = Vec::new();
    era.keeper
        .as_ref()
        .unwrap()
        .collect_parallel_world_roots(stopped, &mut words)
        .unwrap();
    result.world = words
        .drain(..)
        .map(|value| WordIdentity(value.bits()))
        .collect();
    stopped
        .symbols()
        .unwrap()
        .append_name_roots(&mut words)
        .unwrap();
    result.names = words
        .drain(..)
        .map(|value| WordIdentity(value.bits()))
        .collect();
    stopped
        .symbols()
        .unwrap()
        .append_function_roots(&mut words)
        .unwrap();
    result.journal = words
        .drain(..)
        .map(|value| WordIdentity(value.bits()))
        .collect();
    era.legacy_roots.as_ref().unwrap().append_words(&mut words);
    era.with_stopped_heap_for_test(stopped, |heap| {
        crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut words);
    })
    .unwrap();
    result.external = words
        .drain(..)
        .map(|value| WordIdentity(value.bits()))
        .collect();
    let (keys, frozen): (Vec<_>, _) = {
        let gate = era.registry().lock().unwrap();
        (
            gate.entries.iter().map(|entry| entry.key).collect(),
            gate.rendezvous.frozen.clone(),
        )
    };
    for key in keys {
        // The legacy keeper image has no owner root table. Match the real
        // collector: only frozen nonlegacy participants publish table batches.
        if frozen.contains(&key) {
            let mut table_words = Vec::new();
            stopped
                .with_root_reader(key, |reader| {
                    reader.append_words(&mut table_words);
                    reader.finish()
                })
                .unwrap()
                .unwrap();
            result
                .tables
                .extend(table_words.into_iter().map(WordIdentity));
        }
        let mut roots = Vec::new();
        stopped
            .with_owner_image(key, |image| {
                image.visit_work(|work| match work {
                    PublishedGcWork::Root(word) => roots.push(WordIdentity(word.value().bits())),
                    PublishedGcWork::Remembered(_)
                    | PublishedGcWork::ConsWrite(_)
                    | PublishedGcWork::MappedOwner(_)
                    | PublishedGcWork::BlackHeader(_)
                    | PublishedGcWork::BlackRegion(_)
                    | PublishedGcWork::SymbolPreimage(_)
                    | PublishedGcWork::SnapshottedOwner(_)
                    | PublishedGcWork::StringPreimageAddress(_)
                    | PublishedGcWork::YoungBlock(_) => result.other_work.push(format!("{work:?}")),
                });
            })
            .unwrap();
        result.owners.push((OwnerId(key.owner()), roots));
    }
    result
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CollectionEpoch(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CollectionCycle(usize);
struct CollectionReceipt {
    roots: Ledger,
    live: [Retention; 4],
    before_journal: Vec<WordIdentity>,
    epoch: CollectionEpoch,
    cycle: CollectionCycle,
}
fn round(era: &ParallelHeapEra<'_>, graph: GraphWords, cohort: Cohort) -> CollectionReceipt {
    let mut stopped = contention::retry(
        Deadline::contention(),
        || era.stop(),
        contention::rendezvous,
    )
    .unwrap();
    // Main's stopped reconciliation is the only legacy function-cache boundary.
    // The root enumeration itself uses the production KeeperSymbols path.
    let mut before_journal = Vec::new();
    stopped
        .symbols()
        .unwrap()
        .append_function_roots(&mut before_journal)
        .unwrap();
    let before_journal = before_journal
        .into_iter()
        .map(|value| WordIdentity(value.bits()))
        .collect();
    stopped.symbols().unwrap().reconcile_functions().unwrap();
    let roots = ledger(era, &mut stopped);
    let report = era.collect_stopped(&mut stopped).unwrap();
    let live = era
        .with_stopped_heap_for_test(&stopped, |heap| {
            graph.0.map(|word| {
                match heap.owns_heap_value_for_test(TaggedValue::from_bits(word.0)) {
                    true => Retention::Live,
                    false => Retention::Reclaimed,
                }
            })
        })
        .unwrap();
    stopped.complete().unwrap();
    assert!(report.major, "C6.3d requires full jobs, not minor evidence");
    assert_eq!(report.frozen_owners, cohort.roles().len());
    eprintln!(
        "P3SYM_C6_COHORT_FULL epoch={} cycle={} owners={} live={live:?}",
        report.epoch, report.cycle, report.frozen_owners
    );
    CollectionReceipt {
        roots,
        live,
        before_journal,
        epoch: CollectionEpoch(report.epoch),
        cycle: CollectionCycle(report.cycle),
    }
}
fn clear_operands() -> Vec<CombinedInstruction> {
    [home(0), home(1), home(2)]
        .into_iter()
        .map(|home| CombinedInstruction::ClearHome { home })
        .collect()
}
fn program(role: Role, scenario: Scenario) -> CombinedTape {
    match scenario {
        Scenario::Transfer => {}
        Scenario::Noop | Scenario::Unwind => return tape(vec![CombinedInstruction::Safepoint; 4]),
    }
    let mut code = Vec::new();
    for phase in [
        Phase::Function,
        Phase::Selected,
        Phase::Replaced,
        Phase::Interned,
    ] {
        match (role, phase) {
            (Role::Publisher, Phase::Function) => {
                code.extend([
                    CombinedInstruction::GlobalToHome {
                        input: InputIndex(0),
                        destination: home(0),
                    },
                    CombinedInstruction::HomeToFunction {
                        source: home(0),
                        destination: home(2),
                    },
                    // NIL is an existing reserved operand, never raw planting.
                    CombinedInstruction::HomeToGlobal {
                        input: InputIndex(0),
                        source: home(1),
                        destination: home(2),
                    },
                ]);
                code.extend(clear_operands());
            }
            (Role::Caller, Phase::Selected) => {
                code.extend([
                    CombinedInstruction::CallBegin,
                    CombinedInstruction::ClearCache,
                ]);
            }
            (Role::Publisher, Phase::Replaced) => {
                code.extend([
                    CombinedInstruction::GlobalToHome {
                        input: InputIndex(1),
                        destination: home(0),
                    },
                    CombinedInstruction::HomeToFunction {
                        source: home(0),
                        destination: home(2),
                    },
                ]);
                code.extend(clear_operands());
            }
            (Role::Caller, Phase::Interned) => {
                code.extend([
                    CombinedInstruction::CallStep {
                        destination: home(1),
                    },
                    CombinedInstruction::CallStep {
                        destination: home(1),
                    },
                    CombinedInstruction::InitialIntern {
                        name: home(1),
                        destination: home(2),
                    },
                    CombinedInstruction::InitialSoft {
                        name: home(1),
                        destination: home(0),
                    },
                    CombinedInstruction::ClearCache,
                    CombinedInstruction::ClearActivation,
                ]);
                code.extend(clear_operands());
            }
            (Role::Observer, Phase::Interned) => {
                code.extend([
                    CombinedInstruction::CallBegin,
                    CombinedInstruction::CallStep {
                        destination: home(1),
                    },
                    CombinedInstruction::CallStep {
                        destination: home(1),
                    },
                    CombinedInstruction::ClearCache,
                    CombinedInstruction::ClearActivation,
                ]);
                code.extend(clear_operands());
            }
            (Role::Publisher, Phase::Selected | Phase::Interned)
            | (Role::Caller, Phase::Function | Phase::Replaced)
            | (Role::Observer, Phase::Function | Phase::Selected | Phase::Replaced) => {}
        }
        code.push(CombinedInstruction::Safepoint);
    }
    tape(code)
}
#[must_use = "finish every native worker before leaving the scoped cohort"]
struct Commands(Vec<mpsc::Sender<Command>>);
impl Drop for Commands {
    fn drop(&mut self) {
        for tx in &self.0 {
            let _ = tx.send(Command::Finish);
        }
    }
}
fn cohort(
    backing: PlainSymbolBacking,
    cohort: Cohort,
    scenario: Scenario,
) -> std::thread::Result<()> {
    let setup = RefCell::new(GraphSetup::Empty);
    Context::with_test_symbol_functions_init_era(
        backing,
        |obarray, ids| {
            let nonce = crate::emacs_core::intern::intern_uninterned("c6-cohort-nonce");
            let name = TaggedValue::string(format!("p3sym-c6-cohort-{}", nonce.0));
            let body = TaggedValue::cons(name, TaggedValue::NIL);
            let args = TaggedValue::cons(TaggedValue::NIL, body);
            let definition = TaggedValue::cons(
                TaggedValue::from_sym_id(crate::emacs_core::intern::intern("lambda")),
                args,
            );
            obarray.seed_value_id(ids[0], definition);
            for id in ids {
                obarray.clear_function_silent_id(*id);
            }
            *setup.borrow_mut() = GraphSetup::Ready(GraphWords(
                [definition, args, body, name].map(|value| WordIdentity(value.bits())),
            ));
        },
        |era, ids| {
            let graph = match std::mem::replace(&mut *setup.borrow_mut(), GraphSetup::Empty) {
                GraphSetup::Empty => panic!("missing serial cohort graph"),
                GraphSetup::Ready(graph) => graph,
            };
            let handles: Vec<_> = ids
                .into_iter()
                .map(|id| era.admit_plain_symbol(id).unwrap())
                .collect();
            let inputs = [
                CombinedInput::new(handles[0], CombinedInputRole::NameDefinition),
                CombinedInput::new(handles[1], CombinedInputRole::ScalarDefinition),
            ];
            let session = Session::admit(
                &era,
                &handles[0],
                &inputs,
                cohort
                    .roles()
                    .iter()
                    .map(|role| program(*role, scenario))
                    .collect(),
            )
            .unwrap();
            let mut permits = Vec::new();
            for (index, _) in cohort.roles().iter().enumerate() {
                let program =
                    ProgramIndex::checked(u16::try_from(index).unwrap(), &session).unwrap();
                let owner = OwnerId(u64::try_from(index).unwrap() + 1);
                let permit = era
                    .register(owner.0, session.root_capacity(program).unwrap().as_usize())
                    .unwrap();
                permits.push((owner, program, permit));
            }
            let (outcome, workers) = std::thread::scope(|scope| {
                let (ready_tx, ready_rx) = mpsc::channel();
                let mut commands = Commands(Vec::new());
                let mut joins = Vec::new();
                for (id, program, permit) in permits {
                    let session = session.clone();
                    let ready = ready_tx.clone();
                    let (tx, rx) = mpsc::channel();
                    commands.0.push(tx);
                    let heap = era.heap_identity();
                    joins.push(scope.spawn(move || {
                        let owner = retry_owned(
                            Deadline::contention(),
                            permit,
                            |permit| permit.activate(heap, id.0),
                            |failed| match contention::registration(&failed.reason) {
                                Retry::Busy => ControlFlow::Continue(failed.into_permit()),
                                Retry::Refuse => ControlFlow::Break(failed),
                            },
                        )
                        .unwrap();
                        let receipt = Rc::new(RefCell::new(CleanupReceipt::Pending));
                        let mut guard = Guard {
                            resources: Resources::Resident(Worker {
                                owner,
                                frame: FrameState::Empty,
                            }),
                            receipt: Rc::clone(&receipt),
                        };
                        let body = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let worker = match &mut guard.resources {
                                Resources::Resident(worker) => worker,
                                Resources::Released => panic!("missing cohort worker"),
                            };
                            worker.frame = FrameState::Ready(
                                contention::retry(
                                    Deadline::contention(),
                                    || session.seed(&mut worker.owner, program),
                                    executor::classify,
                                )
                                .unwrap(),
                            );
                            loop {
                                let command = {
                                    let _probes = a8_probes::suppress();
                                    worker
                                        .owner
                                        .with_native_extent(&[], |_| {
                                            ready.send(id).unwrap();
                                            #[cfg(not(miri))]
                                            {
                                                rx.recv_timeout(SPIN_WAIT)
                                                    .map_err(|error| format!("{error:?}"))
                                            }
                                            #[cfg(miri)]
                                            {
                                                rx.recv().map_err(|error| format!("{error:?}"))
                                            }
                                        })
                                        .unwrap()
                                        .result
                                        .unwrap()
                                };
                                match command {
                                    Command::Finish => break,
                                    Command::Repark => {}
                                    Command::Advance => loop {
                                        match worker.step(&session).unwrap() {
                                            ReadyStep::Progress | ReadyStep::Yielded => {}
                                            ReadyStep::Safepoint => break,
                                            ReadyStep::Done => {
                                                panic!("cohort advanced beyond its admitted phases")
                                            }
                                        }
                                    },
                                }
                            }
                        }));
                        drop(guard);
                        assert_eq!(
                            *receipt.borrow(),
                            CleanupReceipt::Finished(Ok(())),
                            "C6 cohort independent teardown"
                        );
                        body
                    }));
                }
                drop(ready_tx);
                let body = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let await_all = || {
                        let mut seen = Vec::new();
                        for _ in cohort.roles() {
                            #[cfg(not(miri))]
                            seen.push(ready_rx.recv_timeout(SPIN_WAIT).unwrap());
                            #[cfg(miri)]
                            seen.push(ready_rx.recv().unwrap());
                        }
                        seen.sort_by_key(|owner| owner.0);
                        seen.dedup();
                        assert_eq!(
                            seen.len(),
                            cohort.roles().len(),
                            "every independent owner published Native"
                        );
                    };
                    await_all();
                    match scenario {
                        Scenario::Unwind => {
                            panic!("controlled cohort unwind after native publication")
                        }
                        Scenario::Transfer | Scenario::Noop => {}
                    }
                    for phase in [
                        Phase::Function,
                        Phase::Selected,
                        Phase::Replaced,
                        Phase::Interned,
                    ] {
                        for tx in &commands.0 {
                            tx.send(Command::Advance).unwrap();
                        }
                        await_all();
                        match scenario {
                            Scenario::Noop => continue,
                            Scenario::Transfer | Scenario::Unwind => {}
                        }
                        match phase {
                            Phase::Selected => {}
                            Phase::Function | Phase::Replaced | Phase::Interned => {
                                // First job drains birth/history work. The second is
                                // the decisive live-census/omission witness.
                                let first = round(&era, graph, cohort);
                                // Each owner applies the first response and
                                // republishes before the decisive next job.
                                for tx in &commands.0 {
                                    tx.send(Command::Repark).unwrap();
                                }
                                await_all();
                                let second = round(&era, graph, cohort);
                                assert!(second.epoch > first.epoch);
                                assert!(second.cycle > first.cycle);
                                let CollectionReceipt { roots, live, .. } = second;
                                assert!(
                                    roots.other_work.is_empty(),
                                    "hidden history work: {roots:?}"
                                );
                                match phase {
                                    Phase::Function => {
                                        assert_eq!(live, [Retention::Live; 4]);
                                        assert_eq!(
                                            roots
                                                .world
                                                .iter()
                                                .filter(|word| **word == graph.definition())
                                                .count(),
                                            1
                                        );
                                        assert!(
                                            roots
                                                .without_world()
                                                .all(|word| !graph.0.contains(word)),
                                            "function-only family masked: {roots:?}"
                                        );
                                        eprintln!(
                                            "P3SYM_C6_COHORT_WITNESS omitted_function_family {backing:?} {cohort:?}"
                                        );
                                    }
                                    Phase::Replaced => {
                                        assert_eq!(live, [Retention::Live; 4]);
                                        assert!(
                                            first.before_journal.contains(&graph.definition()),
                                            "replacement must first journal the actual old definition"
                                        );
                                        assert!(
                                            roots
                                                .without_owners()
                                                .all(|word| !graph.0.contains(word)),
                                            "selected-only family masked: {roots:?}"
                                        );
                                        let selected: Vec<_> = roots
                                            .owners
                                            .iter()
                                            .filter(|(_, words)| {
                                                words.iter().any(|word| graph.0.contains(word))
                                            })
                                            .collect();
                                        assert_eq!(selected.len(), 1);
                                        assert_eq!(selected[0].0, OwnerId(2));
                                        assert_eq!(
                                            selected[0]
                                                .1
                                                .iter()
                                                .filter(|word| **word == graph.definition())
                                                .count(),
                                            1
                                        );
                                        eprintln!(
                                            "P3SYM_C6_COHORT_WITNESS omitted_selected_owner_family {backing:?} {cohort:?}"
                                        );
                                    }
                                    Phase::Interned => {
                                        assert_eq!(
                                            roots
                                                .names
                                                .iter()
                                                .filter(|word| **word == graph.name())
                                                .count(),
                                            1
                                        );
                                        assert!(
                                            roots
                                                .without_names()
                                                .all(|word| !graph.0.contains(word)),
                                            "registry-only family masked: {roots:?}"
                                        );
                                        assert_eq!(
                                            live,
                                            [
                                                Retention::Reclaimed,
                                                Retention::Reclaimed,
                                                Retention::Reclaimed,
                                                Retention::Live
                                            ],
                                            "old definition reclaimed, exact name survives"
                                        );
                                        eprintln!(
                                            "P3SYM_C6_COHORT_WITNESS omitted_exact_name_family old_definition_reclaimed {backing:?} {cohort:?}"
                                        );
                                    }
                                    Phase::Selected => {
                                        unreachable!("no collection at selection boundary")
                                    }
                                }
                            }
                        }
                    }
                }));
                drop(commands);
                let workers = joins
                    .into_iter()
                    .map(|join| join.join())
                    .collect::<Vec<_>>();
                (body, workers)
            });
            let mut stopped = era.stop().unwrap();
            stopped.symbols().unwrap().reconcile_functions().unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era
                .finish()
                .expect("cohort finish after all independent banks and journals");
            drop(collector);
            drop(keeper);
            eprintln!("P3SYM_C6_COHORT_TEARDOWN_OK {backing:?} {cohort:?}");
            // Propagate every assertion only after all worker banks, journal,
            // and the actual era have been released independently.
            for worker in workers {
                match worker {
                    Ok(Ok(())) => {}
                    Ok(Err(panic)) | Err(panic) => return Err(panic),
                }
            }
            outcome
        },
    )
}
#[test]
fn p3sym_c6_cohort_two_plain_full_root_transfers() {
    cohort(
        PlainSymbolBacking::Unprojected,
        Cohort::Two,
        Scenario::Transfer,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_cohort_two_projected_full_root_transfers() {
    cohort(
        PlainSymbolBacking::Projected,
        Cohort::Two,
        Scenario::Transfer,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_cohort_four_plain_full_root_transfers() {
    cohort(
        PlainSymbolBacking::Unprojected,
        Cohort::Four,
        Scenario::Transfer,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_cohort_four_projected_full_root_transfers() {
    cohort(
        PlainSymbolBacking::Projected,
        Cohort::Four,
        Scenario::Transfer,
    )
    .unwrap();
}

#[test]
fn p3sym_c6_cohort_noop_control_releases_every_owner() {
    cohort(PlainSymbolBacking::Projected, Cohort::Two, Scenario::Noop).unwrap();
}
#[test]
fn p3sym_c6_cohort_unwind_releases_every_owner() {
    let error = cohort(PlainSymbolBacking::Projected, Cohort::Two, Scenario::Unwind).unwrap_err();
    assert_eq!(
        error.downcast_ref::<&str>(),
        Some(&"controlled cohort unwind after native publication")
    );
}
