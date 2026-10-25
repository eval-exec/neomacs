use super::*;
use crate::tagged::gc::PlainSymbolBacking;

mod gc;
mod histories;

fn source(ops: Vec<Op>, nslots: usize) -> Source {
    Source {
        insns: ops
            .into_iter()
            .map(|op| match op {
                Op::Intern(op) => Insn::Intern(op),
                Op::Global(op) => Insn::Global(op),
                op @ (Op::Function(_)
                | Op::ConstNil { .. }
                | Op::ConstT { .. }
                | Op::ConstFixnum { .. }
                | Op::Move { .. }
                | Op::Add { .. }
                | Op::Sub { .. }
                | Op::Lt { .. }
                | Op::Eq { .. }
                | Op::BranchIfNil { .. }
                | Op::Jump { .. }
                | Op::Cons { .. }
                | Op::Car { .. }
                | Op::Cdr { .. }
                | Op::SetCar { .. }
                | Op::SetCdr { .. }
                | Op::HtGet { .. }
                | Op::HtPut { .. }
                | Op::HtRem { .. }
                | Op::HtClear { .. }
                | Op::HtMake { .. }
                | Op::HtCopy { .. }
                | Op::HtCount { .. }
                | Op::Safepoint) => Insn::Allowed(op),
            })
            .collect(),
        nslots,
        bound: 0,
    }
}
fn intern(name: u16, dst: u16) -> Op {
    Op::Intern(InternOp::InitialIntern { name, dst })
}
fn soft(name: u16, dst: u16) -> Op {
    Op::Intern(InternOp::InitialInternSoft { name, dst })
}
fn load(symbol: u16, dst: u16) -> Op {
    Op::Global(GlobalOp::Value { symbol, dst })
}
fn drive<'era>(
    session: &Session,
    owner: &mut Mutator<'era>,
    frame: &mut Frame<'era>,
) -> Result<(), ExecError> {
    loop {
        match session.step(owner, frame)? {
            Step::Done => return Ok(()),
            Step::Retry => thread::yield_now(),
            Step::Progress | Step::Safepoint => {}
        }
    }
}
fn words<'era>(owner: &mut Mutator<'era>, frame: &Frame<'era>) -> Vec<usize> {
    must(with_run(owner, |run| {
        Ok(frame.slots.iter().map(|s| s.word_for_test(run)).collect())
    }))
}

/// Seed failure, typed execution errors and assertion unwind all erase the
/// actual owner root bank before retirement and ordinary era finish.
fn case(
    kind: PlainSymbolBacking,
    ops: Vec<Op>,
    nslots: usize,
    body: impl for<'era> FnOnce(&ParallelHeapEra<'_>, &Session, &mut Mutator<'era>, &mut Frame<'era>),
) {
    case_capacity(kind, ops, nslots, nslots, body).unwrap();
}
fn case_capacity(
    kind: PlainSymbolBacking,
    ops: Vec<Op>,
    nslots: usize,
    capacity: usize,
    body: impl for<'era> FnOnce(&ParallelHeapEra<'_>, &Session, &mut Mutator<'era>, &mut Frame<'era>),
) -> std::thread::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let spelling = format!("p3sym-c5-executor-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    assert!(crate::emacs_core::intern::lookup_interned(&spelling).is_none());
    Context::with_test_symbol_name_inputs_era(kind, &spelling, |era, ids| {
        let session = Session::admit(&era, &ids[1..], &[source(ops, nslots)]).unwrap();
        let mut stopped = era.stop().unwrap();
        let nil = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: actual matching stopped heap and an immediate seed.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        // with_slice requires the catalog even for allocation-free tapes.
        era.start_cons_storage().unwrap();
        let mut owner = era
            .register(1, capacity)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let mut frame = None;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            frame = Some(session.seed(&mut owner, 0, &nil).unwrap_or_else(|error| {
                panic!(
                    "C5 executor seed: {error:?}; rooted_slots={}",
                    owner.local.as_ref().unwrap().root_scratch.len()
                )
            }));
            body(&era, &session, &mut owner, frame.as_mut().unwrap());
        }));
        drop(frame);
        must(with_run(&mut owner, |run| {
            run.clear_root_slots()?;
            Ok(())
        }));
        owner.retire_rooted(&[]).unwrap();
        drop(nil);
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let (collector, keeper) = era
            .finish()
            .expect("C5 executor after complete root teardown");
        drop(collector);
        drop(keeper);
        eprintln!("P3SYM_C5_EXECUTOR_TEARDOWN_OK {kind:?}");
        outcome
    })
}

#[test]
fn p3sym_c5_executor_partial_seed_refusal_has_total_teardown() {
    let result = case_capacity(
        PlainSymbolBacking::VoidProjection,
        vec![],
        2,
        1,
        |_, _, _, _| panic!("seed failure incorrectly reached executor body"),
    );
    let panic = result.expect_err("one root home cannot seed two registers");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("C5 executor seed: Registration(Capacity)")
            && message.contains("rooted_slots=1"),
        "unexpected seed witness: {message}"
    );
    eprintln!("P3SYM_C5_EXECUTOR_PARTIAL_SEED_REFUSAL cleaned=1");
}

#[test]
fn p3sym_c5_executor_noop_control_finishes() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        case(kind, vec![], 1, |_, session, owner, frame| {
            drive(session, owner, frame).unwrap()
        });
    }
}

#[test]
fn p3sym_c5_executor_intern_and_soft_match_gnu() {
    let oracle: Vec<_> = include_str!("../../symbol_intern_oracle.expected")
        .lines()
        .collect();
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        case(
            kind,
            vec![
                load(0, 0),
                load(1, 1),
                soft(0, 4),
                intern(0, 2),
                intern(1, 3),
                soft(1, 5),
                Op::Eq { dst: 6, a: 2, b: 3 },
                Op::Eq { dst: 7, a: 2, b: 5 },
                Op::Safepoint,
            ],
            8,
            |_, session, owner, frame| {
                drive(session, owner, frame).unwrap();
                let w = words(owner, frame);
                assert_eq!(prin1(w[4]), oracle[0]);
                assert_eq!(prin1(w[6]), oracle[2]);
                assert_eq!(prin1(w[7]), oracle[3]);
                assert_eq!((w[2], w[3]), (w[5], w[5]));
                assert_ne!(w[0], w[1], "distinct real equal-name input objects");
                // Capability is still minted only from the actual result home.
                must(with_run(owner, |run| {
                    run.initial_symbol_handle(&frame.slots[2])?;
                    Ok(())
                }));
                assert_eq!(frame.machine.cons_count, 0);
                assert_eq!(frame.machine.safepoints, 1);
                eprintln!("P3SYM_C5_EXECUTOR_GNU {kind:?} miss=nil repeated=t soft=t");
            },
        );
    }
}

#[test]
fn p3sym_c5_executor_aliased_name_and_destination() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        case(
            kind,
            vec![
                load(0, 0),
                intern(0, 0),
                Op::Move { dst: 1, src: 0 },
                load(1, 0),
                soft(0, 0),
                Op::Eq { dst: 2, a: 0, b: 1 },
            ],
            3,
            |_, session, owner, frame| {
                drive(session, owner, frame).unwrap();
                assert_eq!(words(owner, frame)[2], TaggedValue::T.bits());
            },
        );
    }
}

#[test]
fn p3sym_c5_executor_wrong_type_preserves_destination_and_pc() {
    for op in [intern(0, 1), soft(0, 1)] {
        for name in [
            Op::ConstNil { dst: 0 },
            Op::ConstT { dst: 0 },
            Op::ConstFixnum { dst: 0, value: 97 },
        ] {
            case(
                PlainSymbolBacking::Unprojected,
                vec![name, Op::ConstFixnum { dst: 1, value: 23 }, op],
                2,
                |_, session, owner, frame| {
                    assert_eq!(
                        drive(session, owner, frame),
                        Err(ExecError::Intern(InitialInternError::ExpectedString))
                    );
                    assert_eq!(frame.machine.pc, 2);
                    assert_eq!(words(owner, frame)[1], TaggedValue::fixnum(23).bits());
                },
            );
        }
    }
}

#[test]
fn p3sym_c5_executor_busy_has_separate_budget_and_retries_same_pc() {
    for op in [intern(0, 1), soft(0, 1)] {
        let creates = matches!(op, Op::Intern(InternOp::InitialIntern { .. }));
        case(
            PlainSymbolBacking::VoidProjection,
            vec![load(0, 0), op],
            2,
            |_, session, owner, frame| {
                assert_eq!(session.step(owner, frame), Ok(Step::Progress));
                let before = words(owner, frame);
                let steps = frame.steps_left;
                let registry = crate::emacs_core::intern::parallel::identity_census_for_test();
                crate::emacs_core::intern::parallel::with_registry_writer_for_test(|| {
                    assert_eq!(session.step(owner, frame), Ok(Step::Retry));
                    assert_eq!(frame.machine.pc, 1);
                    assert_eq!(words(owner, frame), before);
                    assert_eq!(frame.steps_left, steps);
                    assert_eq!(frame.refusals_left, REFUSAL_CAP - 1);
                    assert_eq!(
                        owner.entry.admission.load(Ordering::Acquire),
                        super::super::super::admission::AdmissionWord::OUT
                    );
                    must(with_run(owner, |run| {
                        assert_eq!(run.symbol_word_for_test(), 0);
                        Ok(())
                    }));
                    frame.refusals_left = 0;
                    assert_eq!(
                        session.step(owner, frame),
                        Err(ExecError::Registration(RegistrationError::Busy))
                    );
                    assert_eq!(frame.machine.pc, 1);
                    assert_eq!(frame.steps_left, steps);
                });
                assert_eq!(
                    crate::emacs_core::intern::parallel::identity_census_for_test(),
                    registry
                );
                frame.steps_left = 0;
                assert_eq!(session.step(owner, frame), Err(ExecError::Backedge));
                assert_eq!(frame.machine.pc, 1);
                frame.steps_left = steps;
                frame.refusals_left = REFUSAL_CAP;
                drive(session, owner, frame).unwrap();
                assert_eq!(
                    crate::emacs_core::intern::parallel::identity_census_for_test(),
                    if creates {
                        (registry.0 + 1, registry.1 + 1)
                    } else {
                        registry
                    }
                );
                assert_eq!(words(owner, frame)[1] == TaggedValue::NIL.bits(), !creates);
                eprintln!(
                    "P3SYM_C5_EXECUTOR_BUSY_RETRY creates={creates} unchanged_pc=1 released_authority=1"
                );
            },
        );
    }
}

#[test]
fn p3sym_c5_executor_gate_checks_unreachable_ops_and_all_participants() {
    Context::with_test_symbol_name_inputs_era(
        PlainSymbolBacking::Unprojected,
        "p3sym-c5-gate-input",
        |era, ids| {
            let good = source(vec![load(0, 0), intern(0, 1)], 2);
            for op in [
                intern(2, 0),
                soft(0, 2),
                Op::Move { dst: 0, src: 2 },
                Op::ConstNil { dst: 2 },
                Op::Eq { dst: 0, a: 2, b: 0 },
                load(2, 0),
            ] {
                let bad = source(vec![Op::Jump { target: 2 }, op], 2);
                assert!(matches!(
                    Session::admit(&era, &ids[1..], &[good.clone(), bad]),
                    Err(AdmissionError::Program(Reject::Slot))
                ));
            }
            for op in [
                Op::Car { dst: 0, cell: 0 },
                Op::Cdr { dst: 0, cell: 0 },
                Op::Cons {
                    dst: 0,
                    car: 0,
                    cdr: 0,
                },
                Op::SetCar { cell: 0, value: 0 },
                Op::Global(GlobalOp::Set {
                    symbol: 0,
                    src: 0,
                    dst: 0,
                }),
                Op::Function(FunctionOp::Function { symbol: 0, dst: 0 }),
            ] {
                let bad = source(vec![Op::Jump { target: 2 }, op], 2);
                assert!(matches!(
                    Session::admit(&era, &ids[1..], &[good.clone(), bad]),
                    Err(AdmissionError::Program(Reject::Excluded(_)))
                ));
            }
            assert_eq!(
                prevalidate(&[intern(0, 0)], 1, 0, STEP_CAP),
                Err(Reject::Excluded("initial obarray"))
            );
            // Empty initial NIL cannot become an assumed truthy symbol.
            assert_eq!(
                prevalidate_inner(
                    &[intern(0, 0), Op::BranchIfNil { cond: 0, target: 2 }],
                    1,
                    0,
                    STEP_CAP,
                    &[],
                    TapeCapabilities::InitialIntern { globals: 1 }
                ),
                Err(Reject::RejectedType)
            );
            era.admit_plain_symbol(ids[1]).unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}

#[test]
fn p3sym_c5_executor_separate_input_gate_rejects_non_names() {
    for wrong_type in [false, true] {
        Context::with_test_symbol_name_inputs_init_era(
            PlainSymbolBacking::VoidProjection,
            ":p3sym-c5-excluded-keyword",
            |obarray, ids| {
                if wrong_type {
                    obarray.seed_value_id(ids[1], TaggedValue::fixnum(97));
                }
            },
            |era, ids| {
                assert!(matches!(
                    Session::admit(&era, &ids[1..], &[source(vec![], 1)]),
                    Err(AdmissionError::Program(Reject::RejectedType))
                ));
                // Rejection completed the stop and left serial authority healthy.
                era.admit_plain_symbol(ids[1]).unwrap();
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
            },
        );
    }
}

#[test]
fn p3sym_c5_executor_uses_only_rooted_intern_and_fixed_input_surface() {
    let driver = include_str!("../intern.rs");
    for forbidden in [
        "SymbolBacking",
        "symbols.backing",
        "probe(",
        "symbol_ref(",
        "symbol_mut(",
        "Value::from_bits",
        "unsafe {",
        "from_current_heap",
        "intern::intern(",
        "sym_set_into(",
    ] {
        assert!(
            !driver.contains(forbidden),
            "intern executor bypass: {forbidden}"
        );
    }
    for method in [
        "initial_intern_into(",
        "initial_intern_soft_into(",
        "sym_value_into(",
        "sym_makunbound_into(",
    ] {
        assert!(driver.contains(method));
    }
}

#[test]
fn p3sym_c5_executor_typed_capabilities_preserve_session_boundaries() {
    let initial = TapeCapabilities::InitialIntern { globals: 1 };
    assert_eq!(initial.global_handles(), Some(1));
    assert_eq!(initial.function_handles(), None);
    assert_eq!(check_tape(&[intern(0, 0), soft(0, 0)], 1, initial), Ok(()));
    for capabilities in [
        TapeCapabilities::Core,
        TapeCapabilities::Globals { symbols: 1 },
        TapeCapabilities::Functions { symbols: 1 },
    ] {
        assert_eq!(
            check_tape(&[intern(0, 0)], 1, capabilities),
            Err(Reject::Excluded("initial obarray"))
        );
        assert_eq!(check_tape(&[], 0, capabilities), Ok(()));
    }
    for excluded in [
        Op::Function(FunctionOp::Function { symbol: 0, dst: 0 }),
        Op::Global(GlobalOp::Set {
            symbol: 0,
            src: 0,
            dst: 0,
        }),
        Op::Cons {
            dst: 0,
            car: 0,
            cdr: 0,
        },
    ] {
        assert_eq!(
            check_tape(&[excluded], 1, initial),
            Err(Reject::Excluded("intern closed operations"))
        );
    }
    assert_eq!(check_tape(&[], 0, initial), Ok(()));
    assert_eq!(
        ExecError::from(InitialInternError::ExpectedString),
        ExecError::Intern(InitialInternError::ExpectedString)
    );
    assert_eq!(
        ExecError::from(InitialInternError::Authority(RegistrationError::Busy)),
        ExecError::Registration(RegistrationError::Busy)
    );
}
