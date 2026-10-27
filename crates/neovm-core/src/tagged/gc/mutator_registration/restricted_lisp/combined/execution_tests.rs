//! Actual single-owner mixed tapes. Source holders remain live in this item;
//! no cross-owner, sole-root, sweep or minor receipt is inferred from them.
//! The collection child supplies independent cross-owner full-GC receipts.
mod collection;

use super::executor::{Error, Frame, ProgramIndex, ReadyStep, Session};
use super::*;
use crate::tagged::gc::mutator_registration::symbol_authority::combined_inputs::{
    CombinedInput, CombinedInputRole,
};
use crate::tagged::gc::{InitialInternError, PlainSymbolBacking, SymbolHandle};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WordIdentity(usize);
#[derive(Clone, Copy)]
struct Facts {
    definition: WordIdentity,
    name: WordIdentity,
}
enum SetupState {
    Empty,
    Ready(Facts),
}
#[derive(Clone, Copy)]
enum Graph {
    Name,
    Keyword,
}
enum RootReservation {
    Exact,
    Short,
}
enum FrameState<'era> {
    Empty,
    Ready(Frame<'era>),
}
struct Worker<'era> {
    owner: Mutator<'era>,
    frame: FrameState<'era>,
}
enum Resources<'era> {
    Resident(Worker<'era>),
    Released,
}
#[derive(Debug, PartialEq, Eq)]
enum CleanupError {
    Registration(RegistrationError),
    Retirement(RendezvousError),
}
#[derive(Debug, PartialEq, Eq)]
enum CleanupReceipt {
    Pending,
    Finished(Result<(), CleanupError>),
}
#[must_use = "drop the frame and clear the actual bank before owner retirement"]
struct Guard<'era> {
    resources: Resources<'era>,
    receipt: Rc<RefCell<CleanupReceipt>>,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let result = match std::mem::replace(&mut self.resources, Resources::Released) {
            Resources::Released => return,
            Resources::Resident(Worker { mut owner, frame }) => (|| {
                drop(frame);
                contention::retry(
                    Deadline::contention(),
                    || owner.run_access()?.clear_root_slots(),
                    contention::registration,
                )
                .map_err(CleanupError::Registration)?;
                retry_owned(
                    Deadline::contention(),
                    owner,
                    |owner| owner.retire_rooted(&[]),
                    |failed| match contention::rendezvous(&failed.reason) {
                        Retry::Busy => ControlFlow::Continue(failed.into_owner()),
                        Retry::Refuse => ControlFlow::Break(failed),
                    },
                )
                .map_err(|failed| CleanupError::Retirement(failed.reason))
            })(),
        };
        *self.receipt.borrow_mut() = CleanupReceipt::Finished(result);
    }
}
impl Worker<'_> {
    fn seed(&mut self, session: &Session) -> Result<(), Error> {
        let program = ProgramIndex::checked(0, session)?;
        let frame = contention::retry(
            Deadline::contention(),
            || session.seed(&mut self.owner, program),
            executor::classify,
        )?;
        self.frame = FrameState::Ready(frame);
        Ok(())
    }
    fn step(&mut self, session: &Session) -> Result<ReadyStep, Error> {
        match &mut self.frame {
            FrameState::Empty => Err(Error::Program),
            FrameState::Ready(frame) => session.wait_step(&mut self.owner, frame),
        }
    }
    fn word(&mut self, register: RootRegister) -> WordIdentity {
        match &self.frame {
            FrameState::Empty => panic!("C6 mixed frame not seeded"),
            FrameState::Ready(frame) => contention::retry(
                Deadline::contention(),
                || {
                    let run = self.owner.run_access()?;
                    Ok(WordIdentity(
                        frame.home(register).unwrap().word_for_test(&run),
                    ))
                },
                contention::registration,
            )
            .unwrap(),
        }
    }
    fn complete(&mut self, session: &Session) -> Vec<ReadyStep> {
        let mut steps = Vec::new();
        loop {
            let step = self.step(session).unwrap();
            match step {
                ReadyStep::Done => return steps,
                ReadyStep::Progress | ReadyStep::Yielded | ReadyStep::Safepoint => steps.push(step),
            }
        }
    }
}
fn capacity() -> RootCapacity {
    RootCapacity::try_from(3).unwrap()
}
fn home(index: u16) -> RootRegister {
    RootRegister::checked(index, capacity()).unwrap()
}
fn tape(instructions: Vec<CombinedInstruction>) -> CombinedTape {
    CombinedTape::admit(
        instructions,
        capacity(),
        SuccessfulStepBudget::try_from(100).unwrap(),
    )
    .unwrap()
}
fn prefix() -> Vec<CombinedInstruction> {
    vec![
        CombinedInstruction::GlobalToHome {
            input: InputIndex(0),
            destination: home(0),
        },
        CombinedInstruction::HomeToFunction {
            source: home(0),
            destination: home(2),
        },
        CombinedInstruction::CallBegin,
        CombinedInstruction::CallStep {
            destination: home(1),
        },
        CombinedInstruction::CallStep {
            destination: home(1),
        },
    ]
}

fn fixture(
    backing: PlainSymbolBacking,
    graph: Graph,
    reservation: RootReservation,
    programs: Vec<CombinedTape>,
    body: impl for<'era> FnOnce(&Session, &mut Worker<'era>, Facts, &[SymbolHandle]),
) -> std::thread::Result<()> {
    let setup = RefCell::new(SetupState::Empty);
    Context::with_test_symbol_functions_init_era(
        backing,
        |obarray, ids| {
            // Serial fixture-only nonce: no previous fixture can supply this
            // spelling's winner. The full spelling itself is never interned here.
            let nonce = crate::emacs_core::intern::intern_uninterned("c6-mixed-name-nonce");
            let name = match graph {
                Graph::Name => TaggedValue::string(format!("p3sym-c6-mixed-name-{}", nonce.0)),
                Graph::Keyword => TaggedValue::string(":p3sym-c6-mixed-keyword"),
            };
            let definition = TaggedValue::cons(
                TaggedValue::from_sym_id(crate::emacs_core::intern::intern("lambda")),
                TaggedValue::cons(TaggedValue::NIL, TaggedValue::cons(name, TaggedValue::NIL)),
            );
            obarray.seed_value_id(ids[0], definition);
            obarray.clear_function_silent_id(ids[0]);
            obarray.clear_function_silent_id(ids[1]);
            *setup.borrow_mut() = SetupState::Ready(Facts {
                definition: WordIdentity(definition.bits()),
                name: WordIdentity(name.bits()),
            });
        },
        |era, ids| {
            let facts = match *setup.borrow() {
                SetupState::Ready(facts) => facts,
                SetupState::Empty => panic!("C6 mixed setup incomplete"),
            };
            let handles: Vec<_> = ids
                .into_iter()
                .map(|id| era.admit_plain_symbol(id).unwrap())
                .collect();
            let inputs = [
                CombinedInput::new(handles[0], CombinedInputRole::NameDefinition),
                CombinedInput::new(handles[1], CombinedInputRole::ScalarDefinition),
            ];
            let session = Session::admit(&era, &handles[0], &inputs, programs).unwrap();
            let required = session
                .root_capacity(ProgramIndex::checked(0, &session).unwrap())
                .unwrap()
                .as_usize();
            let reserved = match reservation {
                RootReservation::Exact => required,
                RootReservation::Short => required.checked_sub(1).unwrap(),
            };
            let permit = era.register(1, reserved).unwrap();
            let owner = retry_owned(
                Deadline::contention(),
                permit,
                |permit| permit.activate(era.heap_identity(), 1),
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
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match &mut guard.resources {
                        Resources::Resident(worker) => body(&session, worker, facts, &handles),
                        Resources::Released => panic!("C6 mixed fixture released before body"),
                    }
                }));
            drop(guard);
            assert_eq!(
                *receipt.borrow(),
                CleanupReceipt::Finished(Ok(())),
                "C6 mixed independent teardown"
            );
            let mut stopped = era.stop().unwrap();
            stopped.symbols().unwrap().reconcile_functions().unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era
                .finish()
                .expect("C6 mixed finish after roots and function reconciliation");
            drop(collector);
            drop(keeper);
            eprintln!("P3SYM_C6_MIXED_TEARDOWN_OK {backing:?}");
            outcome
        },
    )
}
#[derive(Clone, Copy)]
pub(super) enum Transfer {
    Function,
    Intern,
}
pub(super) fn transfer_case(backing: PlainSymbolBacking, transfer: Transfer) {
    let mut instructions = prefix();
    match transfer {
        Transfer::Function => {}
        Transfer::Intern => {
            instructions.push(CombinedInstruction::InitialIntern {
                name: home(1),
                destination: home(2),
            });
            instructions.push(CombinedInstruction::InitialSoft {
                name: home(1),
                destination: home(0),
            });
        }
    }
    fixture(
        backing,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(instructions)],
        |session, worker, facts, _| {
            worker.seed(session).unwrap();
            assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
            assert_eq!(worker.word(home(0)), facts.definition);
            assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
            assert_eq!(worker.word(home(2)), facts.definition);
            assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
            assert_eq!(worker.step(session), Ok(ReadyStep::Yielded));
            assert_eq!(worker.word(home(1)), WordIdentity(TaggedValue::NIL.bits()));
            assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
            assert_eq!(
                worker.word(home(1)),
                facts.name,
                "exact child object, no copied spelling"
            );
            match transfer {
                Transfer::Function => {}
                Transfer::Intern => {
                    assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
                    let symbol = worker.word(home(2));
                    let id = TaggedValue::from_bits(symbol.0)
                        .as_bare_symbol_id()
                        .unwrap();
                    // Existing assertion-only scalar census: no string is
                    // materialized and no legacy heap/TLS scope is entered.
                    let publication =
                        crate::emacs_core::intern::parallel::name_publication_for_test(
                            worker.owner.key().heap_identity(),
                            id,
                            facts.name.0,
                        );
                    assert_eq!(publication.0, Some(facts.name.0));
                    assert!(publication.1 && publication.2 && publication.3);
                    assert_eq!(worker.word(home(1)), facts.name);
                    assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
                    assert_eq!(worker.word(home(0)), symbol);
                    // The keeper name-index census itself is exercised in the
                    // collector stage; these holders deliberately remain live.
                }
            }
            assert_eq!(worker.step(session), Ok(ReadyStep::Done));
            assert_eq!(worker.step(session), Ok(ReadyStep::Done));
            eprintln!("P3SYM_C6_MIXED exact_global_function_return_intern {backing:?}");
        },
    )
    .unwrap();
}

#[test]
fn p3sym_c6_mixed_noop_control_finishes_with_a_real_frame() {
    fixture(
        PlainSymbolBacking::Unprojected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(vec![])],
        |session, worker, _, _| {
            worker.seed(session).unwrap();
            assert_eq!(worker.step(session), Ok(ReadyStep::Done));
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c6_mixed_unwind_releases_reader_and_selected_activation() {
    let outcome = fixture(
        PlainSymbolBacking::Projected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(prefix())],
        |session, worker, _, handles| {
            worker.seed(session).unwrap();
            for _ in 0..3 {
                assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
            }
            let mut run = worker.owner.run_access().unwrap();
            let _reader = run.symbol_ref(&handles[0]).unwrap();
            panic!("C6 controlled mixed unwind");
        },
    );
    assert_eq!(
        outcome.unwrap_err().downcast_ref::<&str>(),
        Some(&"C6 controlled mixed unwind")
    );
}
#[test]
fn p3sym_c6_mixed_selected_old_name_survives_scalar_replacement() {
    let mut instructions = prefix();
    instructions.truncate(4); // selection and its first bounded step only
    instructions.extend([
        CombinedInstruction::ClearCache,
        CombinedInstruction::GlobalToHome {
            input: InputIndex(1),
            destination: home(0),
        },
        CombinedInstruction::HomeToFunction {
            source: home(0),
            destination: home(2),
        },
        CombinedInstruction::CallStep {
            destination: home(1),
        },
        CombinedInstruction::InitialIntern {
            name: home(1),
            destination: home(2),
        },
        CombinedInstruction::CallBegin,
        CombinedInstruction::CallStep {
            destination: home(0),
        },
        CombinedInstruction::CallStep {
            destination: home(0),
        },
        CombinedInstruction::ClearCache,
        CombinedInstruction::ClearActivation,
        CombinedInstruction::ClearHome { home: home(1) },
        CombinedInstruction::HomeToGlobal {
            input: InputIndex(0),
            source: home(1),
            destination: home(2),
        },
        CombinedInstruction::Safepoint,
    ]);
    fixture(
        PlainSymbolBacking::Projected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(instructions)],
        |session, worker, facts, _| {
            worker.seed(session).unwrap();
            for _ in 0..7 {
                worker.step(session).unwrap();
            }
            assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
            assert_eq!(
                worker.word(home(1)),
                facts.name,
                "resume original selection after F1 fset"
            );
            let tail = worker.complete(session);
            assert!(tail.contains(&ReadyStep::Yielded));
            assert_eq!(tail.last(), Some(&ReadyStep::Safepoint));
            assert_eq!(
                worker.word(home(0)),
                WordIdentity(TaggedValue::fixnum(97).bits())
            );
            assert_eq!(worker.word(home(1)), WordIdentity(TaggedValue::NIL.bits()));
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c6_mixed_intern_and_soft_allow_the_same_source_destination_home() {
    let mut instructions = prefix();
    instructions.extend([
        CombinedInstruction::InitialIntern {
            name: home(1),
            destination: home(1),
        },
        CombinedInstruction::CallBegin,
        CombinedInstruction::CallStep {
            destination: home(0),
        },
        CombinedInstruction::CallStep {
            destination: home(0),
        },
        CombinedInstruction::InitialSoft {
            name: home(0),
            destination: home(0),
        },
    ]);
    fixture(
        PlainSymbolBacking::Unprojected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(instructions)],
        |session, worker, _, _| {
            worker.seed(session).unwrap();
            worker.complete(session);
            assert_eq!(worker.word(home(0)), worker.word(home(1)));
            assert!(
                TaggedValue::from_bits(worker.word(home(0)).0)
                    .as_bare_symbol_id()
                    .is_some()
            );
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c6_mixed_keyword_refusal_preserves_pc_and_destination() {
    let mut instructions = prefix();
    instructions.push(CombinedInstruction::InitialIntern {
        name: home(1),
        destination: home(2),
    });
    fixture(
        PlainSymbolBacking::Projected,
        Graph::Keyword,
        RootReservation::Exact,
        vec![tape(instructions)],
        |session, worker, facts, _| {
            worker.seed(session).unwrap();
            for _ in 0..5 {
                worker.step(session).unwrap();
            }
            let before = match &worker.frame {
                FrameState::Empty => unreachable!(),
                FrameState::Ready(frame) => frame.position(),
            };
            assert_eq!(
                worker.step(session),
                Err(Error::Intern(InitialInternError::UnsupportedName))
            );
            assert_eq!(worker.word(home(1)), facts.name);
            assert_eq!(worker.word(home(2)), facts.definition);
            match &worker.frame {
                FrameState::Empty => unreachable!(),
                FrameState::Ready(frame) => assert_eq!(frame.position(), before),
            }
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c6_mixed_seed_capacity_and_repeated_seed_preserve_the_bank() {
    fixture(
        PlainSymbolBacking::Unprojected,
        Graph::Name,
        RootReservation::Short,
        vec![tape(prefix())],
        |session, worker, _, _| {
            assert_eq!(
                worker.seed(session),
                Err(Error::Authority(RegistrationError::Capacity))
            );
            assert!(
                worker
                    .owner
                    .run_access()
                    .unwrap()
                    .owner
                    .local
                    .as_ref()
                    .unwrap()
                    .root_scratch
                    .is_empty()
            );
        },
    )
    .unwrap();
    fixture(
        PlainSymbolBacking::Projected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(prefix())],
        |session, worker, facts, _| {
            let program = ProgramIndex::checked(0, session).unwrap();
            assert_eq!(
                session
                    .seed_refusal_after_cache_for_test(&mut worker.owner, program)
                    .err(),
                Some(Error::Seed(executor::SeedError::Authority(
                    RegistrationError::Capacity
                )))
            );
            assert!(
                worker
                    .owner
                    .run_access()
                    .unwrap()
                    .owner
                    .local
                    .as_ref()
                    .unwrap()
                    .root_scratch
                    .is_empty()
            );
            worker.seed(session).unwrap();
            worker.step(session).unwrap();
            assert_eq!(worker.seed(session), Err(Error::OccupiedFrame));
            assert_eq!(worker.word(home(0)), facts.definition);
            worker.complete(session);
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c6_mixed_expired_pending_instruction_never_restarts() {
    fixture(
        PlainSymbolBacking::Unprojected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(prefix())],
        |session, worker, _, _| {
            worker.seed(session).unwrap();
            let before = match &mut worker.frame {
                FrameState::Empty => unreachable!(),
                FrameState::Ready(frame) => {
                    frame.expire_pending_for_test();
                    frame.position()
                }
            };
            for _ in 0..2 {
                assert_eq!(
                    worker.step(session),
                    Err(Error::Authority(RegistrationError::Busy))
                );
                assert_eq!(worker.word(home(0)), WordIdentity(TaggedValue::NIL.bits()));
                match &worker.frame {
                    FrameState::Empty => unreachable!(),
                    FrameState::Ready(frame) => assert_eq!(frame.position(), before),
                }
            }
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c6_mixed_program_budget_is_independent_of_contention() {
    let program = CombinedTape::admit(
        prefix(),
        capacity(),
        SuccessfulStepBudget::try_from(1).unwrap(),
    )
    .unwrap();
    fixture(
        PlainSymbolBacking::Projected,
        Graph::Name,
        RootReservation::Exact,
        vec![program],
        |session, worker, facts, _| {
            worker.seed(session).unwrap();
            assert_eq!(worker.step(session), Ok(ReadyStep::Progress));
            assert_eq!(worker.word(home(0)), facts.definition);
            assert_eq!(worker.step(session), Err(Error::StepBudget));
            assert_eq!(worker.word(home(2)), WordIdentity(TaggedValue::NIL.bits()));
        },
    )
    .unwrap();
}

#[test]
fn p3sym_c6_mixed_retired_session_and_stale_frame_refuse_before_access() {
    enum Retained {
        Empty,
        Ready(Session),
    }
    let retained = RefCell::new(Retained::Empty);
    fixture(
        PlainSymbolBacking::Unprojected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(prefix())],
        |session, _, _, _| {
            *retained.borrow_mut() = Retained::Ready(session.clone());
        },
    )
    .unwrap();
    let retired = match retained.into_inner() {
        Retained::Empty => unreachable!(),
        Retained::Ready(session) => session,
    };
    fixture(
        PlainSymbolBacking::Projected,
        Graph::Name,
        RootReservation::Exact,
        vec![tape(prefix())],
        |session, worker, _, _| {
            let program = ProgramIndex::checked(0, &retired).unwrap();
            assert_eq!(
                retired.seed(&mut worker.owner, program).err(),
                Some(Error::Authority(RegistrationError::WrongHeap))
            );
            assert!(
                worker
                    .owner
                    .run_access()
                    .unwrap()
                    .owner
                    .local
                    .as_ref()
                    .unwrap()
                    .root_scratch
                    .is_empty()
            );
            worker.seed(session).unwrap();
            match &mut worker.frame {
                FrameState::Empty => unreachable!(),
                FrameState::Ready(frame) => assert_eq!(
                    retired.step(&mut worker.owner, frame),
                    Err(Error::ForeignSession)
                ),
            }
            worker
                .owner
                .run_access()
                .unwrap()
                .clear_root_slots()
                .unwrap();
            assert_eq!(
                worker.step(session),
                Err(Error::Authority(RegistrationError::StaleRegistration))
            );
        },
    )
    .unwrap();
}

#[test]
fn p3sym_c6_mixed_admission_checks_every_program_before_preparing_or_registering() {
    Context::with_test_symbol_functions_era(PlainSymbolBacking::Unprojected, |era, ids| {
        let handles: Vec<_> = ids
            .into_iter()
            .map(|id| era.admit_plain_symbol(id).unwrap())
            .collect();
        // ScalarDefinition is the actual frozen role in this standalone input.
        let inputs = [CombinedInput::new(
            handles[0],
            CombinedInputRole::ScalarDefinition,
        )];
        assert_eq!(InputIndex::checked(1, &inputs), Err(TapeError::Input));
        let late = CombinedTape {
            instructions: vec![CombinedInstruction::Excluded(ExcludedOperation::SetCar)],
            capacity: capacity(),
            steps: SuccessfulStepBudget::try_from(1).unwrap(),
        };
        assert_eq!(
            Session::admit(&era, &handles[0], &inputs, vec![tape(vec![]), late]).err(),
            Some(executor::AdmissionError::Tape(TapeError::Excluded(
                ExcludedOperation::SetCar
            )))
        );
        let late_index = tape(vec![CombinedInstruction::GlobalToHome {
            input: InputIndex(1),
            destination: home(0),
        }]);
        assert_eq!(
            Session::admit(&era, &handles[0], &inputs, vec![tape(vec![]), late_index]).err(),
            Some(executor::AdmissionError::Tape(TapeError::Input))
        );
        // A subsequent genuine session and finish prove preflight left no
        // owner, frame or partially published roots behind.
        let session = Session::admit(&era, &handles[0], &inputs, vec![tape(vec![])]).unwrap();
        assert_eq!(
            ProgramIndex::checked(u16::MAX, &session),
            Err(Error::Program)
        );
        drop(session);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

pub(super) fn minor_case(backing: PlainSymbolBacking) {
    collection::minor_case(backing);
}
