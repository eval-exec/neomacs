use super::*;
use crate::tagged::gc::PlainSymbolBacking;

pub(super) fn source(ops: Vec<Op>, nslots: usize) -> Source {
    Source {
        insns: ops
            .into_iter()
            .map(|op| match op {
                Op::Function(op) => Insn::Function(op),
                Op::Global(op) => Insn::Global(op),
                op => Insn::Allowed(op),
            })
            .collect(),
        nslots,
        bound: 0,
    }
}

#[test]
fn p3sym_c4_executor_accepts_rooted_function_tapes_before_activation() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        let accepted = Context::with_test_symbol_functions_era(kind, |era, ids| {
            let tape = source(
                vec![Op::Function(FunctionOp::Function { symbol: 0, dst: 0 })],
                1,
            );
            let accepted = Session::admit(&era, &ids, &[tape]).map(|_| ());
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
            eprintln!("P3SYM_C4_EXECUTOR_ADMISSION_TEARDOWN_OK");
            accepted
        });
        assert_eq!(accepted, Ok(()), "missing function executor admission");
    }
}

use std::num::NonZeroU32;

fn oracle() -> Vec<&'static str> {
    include_str!("../../symbol_functions_oracle.expected")
        .lines()
        .collect()
}
fn drive<'era>(
    session: &Session,
    owner: &mut Mutator<'era>,
    frame: &mut Frame<'era>,
) -> Result<(), ExecError> {
    loop {
        match session.wait_step(owner, frame)? {
            Step::Done => return Ok(()),
            Step::Retry | Step::Yielded => thread::yield_now(),
            Step::Progress | Step::Safepoint => {}
        }
    }
}
fn words<'era>(owner: &mut Mutator<'era>, frame: &Frame<'era>) -> Vec<usize> {
    must(with_run(owner, |run| {
        Ok(frame
            .slots
            .iter()
            .map(|slot| slot.word_for_test(run))
            .collect())
    }))
}

/// Finish the real era independently of an executor refusal or assertion.
pub(super) fn case(
    kind: PlainSymbolBacking,
    ops: Vec<Op>,
    nslots: usize,
    body: impl for<'era> FnOnce(
        &ParallelHeapEra<'_>,
        &Session,
        &mut Mutator<'era>,
        &mut Frame<'era>,
        &[SymId],
    ),
) {
    Context::with_test_symbol_functions_era(kind, |era, ids| {
        let session = Session::admit(&era, &ids, &[source(ops, nslots)]).unwrap();
        let mut stopped = era.stop().unwrap();
        let nil = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: immediate NIL under the matching stop, never a payload import.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        era.start_cons_storage().unwrap();
        let capacity = session.root_capacity(0).unwrap();
        let mut owner = era
            .register(1, capacity)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let mut frame = session.seed(&mut owner, 0, &nil).unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            body(&era, &session, &mut owner, &mut frame, &ids)
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
        stopped.symbols().unwrap().reconcile_functions().unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        eprintln!("P3SYM_C4_EXECUTOR_TEARDOWN_OK {kind:?}");
        if let Err(payload) = outcome {
            std::panic::resume_unwind(payload);
        }
    });
}
fn begin() -> Op {
    Op::Function(FunctionOp::CallBegin { symbol: 0 })
}
fn call(dst: u16) -> Op {
    Op::Function(FunctionOp::CallStep {
        dst,
        fuel: NonZeroU32::MIN,
    })
}

#[test]
fn p3sym_c4_executor_noop_control() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        case(kind, vec![], 0, |_, session, owner, frame, _| {
            assert_eq!(session.root_capacity(0), Ok(2));
            assert_eq!(session.step(owner, frame), Ok(Step::Done));
            assert_eq!(frame.cache.observed_epoch(), None);
            assert_eq!(frame.activation.selected_epoch(), None);
        });
    }
}

#[test]
fn p3sym_c4_executor_primitives_match_gnu() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for row in 0..4 {
            let ops = match row {
                0 => vec![
                    Op::Global(GlobalOp::Value { symbol: 0, dst: 0 }),
                    Op::Function(FunctionOp::Function { symbol: 0, dst: 1 }),
                ],
                1 => vec![
                    Op::Global(GlobalOp::Value { symbol: 1, dst: 0 }),
                    Op::Function(FunctionOp::Fset {
                        symbol: 0,
                        src: 0,
                        dst: 1,
                    }),
                ],
                2 => vec![Op::Function(FunctionOp::Fmakunbound { symbol: 0, dst: 1 })],
                _ => vec![begin(), call(1)],
            };
            case(kind, ops, 2, |_, session, owner, frame, ids| {
                drive(session, owner, frame).unwrap();
                let result = words(owner, frame);
                let printed = match row {
                    0 | 1 => {
                        assert_eq!(result[1], result[0]);
                        "t".to_owned()
                    }
                    2 => {
                        assert_eq!(result[1], TaggedValue::from_sym_id(ids[0]).bits());
                        "t".to_owned()
                    }
                    _ => prin1(result[1]),
                };
                assert_eq!(printed, oracle()[row]);
            });
        }
    }
}

#[test]
fn p3sym_c4_executor_old_and_new_calls_match_gnu() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        let ops = vec![
            begin(),
            Op::Safepoint,
            Op::Global(GlobalOp::Value { symbol: 1, dst: 0 }),
            Op::Function(FunctionOp::Fset {
                symbol: 0,
                src: 0,
                dst: 1,
            }),
            call(2),
            begin(),
            call(3),
        ];
        case(kind, ops, 4, |_, session, owner, frame, _| {
            drive(session, owner, frame).unwrap();
            let result = words(owner, frame);
            assert_eq!(
                format!("({} {})", prin1(result[2]), prin1(result[3])),
                oracle()[5]
            );
            assert_eq!(frame.machine.safepoints, 1);
        });
    }
}

#[test]
fn p3sym_c4_executor_unbind_preserves_selected_call_and_error_answer() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        let ops = vec![
            begin(),
            Op::Function(FunctionOp::Fmakunbound { symbol: 0, dst: 0 }),
            call(1),
            Op::ConstFixnum { dst: 2, value: 23 },
            begin(),
            call(2),
        ];
        case(kind, ops, 3, |_, session, owner, frame, _| {
            assert_eq!(
                drive(session, owner, frame),
                Err(ExecError::Function(FunctionError::VoidFunction))
            );
            let result = words(owner, frame);
            assert_eq!(prin1(result[1]), oracle()[3]);
            assert_eq!(
                format!("(void-function {})", prin1(result[2])),
                oracle()[13]
            );
            assert_eq!(frame.machine.pc, 4);
        });
    }
}

#[test]
fn p3sym_c4_executor_busy_and_yield_preserve_instruction_and_answer() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        case(
            kind,
            vec![begin(), call(0)],
            1,
            |era, session, owner, frame, _| {
                let mut peer = era
                    .register(2, 0)
                    .unwrap()
                    .activate(era.heap_identity(), 2)
                    .unwrap();
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut run = peer.run_access().unwrap();
                    let writer = run.symbol_mut(&session.handles[0]).unwrap();
                    let budget = frame.attempts_left;
                    frame.attempts_left = 1;
                    assert_eq!(session.step(owner, frame), Ok(Step::Retry));
                    assert_eq!(frame.attempts_left, 1);
                    assert_eq!(session.step(owner, frame), Ok(Step::Retry));
                    assert_eq!(frame.attempts_left, 1);
                    frame.attempts_left = budget;
                    assert_eq!(frame.machine.pc, 0);
                    assert_eq!(frame.cache.observed_epoch(), None);
                    assert_eq!(frame.activation.selected_epoch(), None);
                    assert_eq!(words(owner, frame), vec![TaggedValue::NIL.bits()]);
                    drop(writer);
                    drop(run);
                    assert_eq!(session.step(owner, frame), Ok(Step::Progress));
                    assert_eq!(frame.machine.pc, 1);
                    assert_eq!(session.step(owner, frame), Ok(Step::Yielded));
                    assert_eq!(frame.machine.pc, 1);
                    assert_eq!(words(owner, frame), vec![TaggedValue::NIL.bits()]);
                    assert_eq!(session.step(owner, frame), Ok(Step::Progress));
                    assert_eq!(frame.machine.pc, 2);
                    assert_eq!(prin1(words(owner, frame)[0]), oracle()[3]);
                    let before = words(owner, frame);
                    frame.attempts_left = 0;
                    assert_eq!(session.step(owner, frame), Err(ExecError::Backedge));
                    assert_eq!(frame.machine.pc, 2);
                    assert_eq!(words(owner, frame), before);
                }));
                peer.retire_rooted(&[]).unwrap();
                if let Err(payload) = outcome {
                    std::panic::resume_unwind(payload);
                }
            },
        );
    }
}

#[test]
fn p3sym_c4_executor_gate_checks_every_tape_and_activation_lifecycle() {
    Context::with_test_symbol_functions_era(PlainSymbolBacking::Unprojected, |era, ids| {
        let valid = source(vec![begin(), call(0)], 1);
        let mut outcomes = Vec::new();
        for op in [
            Op::SetCar { cell: 0, value: 0 },
            Op::SetCdr { cell: 0, value: 0 },
            Op::HtCount { table: 0, dst: 0 },
            Op::Cons {
                dst: 0,
                car: 0,
                cdr: 0,
            },
            Op::Function(FunctionOp::Function { symbol: 2, dst: 0 }),
            Op::Function(FunctionOp::Fset {
                symbol: 0,
                src: 1,
                dst: 0,
            }),
            Op::Function(FunctionOp::Fmakunbound { symbol: 0, dst: 1 }),
            Op::Function(FunctionOp::CallBegin { symbol: 2 }),
            Op::Function(FunctionOp::CallStep {
                dst: 1,
                fuel: NonZeroU32::MIN,
            }),
        ] {
            let hidden = source(vec![Op::Jump { target: 2 }, op], 1);
            outcomes.push(Session::admit(&era, &ids, &[valid.clone(), hidden]).err());
        }
        let unselected = Session::admit(&era, &ids, &[source(vec![call(0)], 1)]).err();
        let repeated = Session::admit(&era, &ids, &[source(vec![begin(), begin()], 1)]).err();
        let overflow = Session::admit(&era, &ids, &[source(vec![], usize::MAX)]).err();
        let legacy = prevalidate(&[begin()], 1, 0, STEP_CAP);
        let global = prevalidate_inner(&[begin()], 1, 0, STEP_CAP, &[], Some(ids.len()), None);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        for (index, actual) in outcomes.into_iter().enumerate() {
            let reject = match index {
                0..=2 => Reject::Excluded("global session mutation/table"),
                3 => Reject::Excluded("function session allocation"),
                _ => Reject::Slot,
            };
            assert_eq!(actual, Some(AdmissionError::Program(reject)));
        }
        for actual in [unselected, repeated] {
            assert_eq!(actual, Some(AdmissionError::Program(Reject::RejectedType)));
        }
        assert_eq!(overflow, Some(AdmissionError::Program(Reject::Slot)));
        assert_eq!(legacy, Err(Reject::Excluded("function handles")));
        assert_eq!(global, Err(Reject::Excluded("function handles")));
    });
}

#[test]
fn p3sym_c4_executor_private_home_capacity_refusal_cleans_up() {
    Context::with_test_symbol_functions_era(PlainSymbolBacking::Unprojected, |era, ids| {
        let session = Session::admit(&era, &ids, &[source(vec![begin(), call(0)], 1)]).unwrap();
        let capacity = session.root_capacity(0).unwrap();
        let mut stopped = era.stop().unwrap();
        let nil = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: matching stopped heap, immediate NIL seed.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let mut owner = era
            .register(1, capacity - 1)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let error = session.seed(&mut owner, 0, &nil).err();
        // Even a partial NIL-home seed must be total to clear and retire.
        must(with_run(&mut owner, |run| {
            run.clear_root_slots()?;
            Ok(())
        }));
        owner.retire_rooted(&[]).unwrap();
        drop(nil);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        assert_eq!(capacity, 3);
        assert_eq!(
            error,
            Some(ExecError::Registration(RegistrationError::Capacity))
        );
    });
}

#[test]
fn p3sym_c4_executor_uses_only_the_sealed_function_surface() {
    let driver = include_str!("../functions.rs");
    for forbidden in [
        "SymbolBacking",
        "symbols.backing",
        "symbol_ref(",
        "symbol_mut(",
        "Value::from_bits",
        "unsafe {",
        "from_current_heap",
        "load_value_atomic",
        "store_value_atomic",
    ] {
        assert!(!driver.contains(forbidden), "executor bypass: {forbidden}");
    }
    for required in [
        "sym_function_into(",
        "sym_fset_into(",
        "sym_fmakunbound_into(",
        "sym_call_begin(",
        "sym_call_step(",
        "FunctionCallCache::new(",
        "FunctionActivation::new(",
    ] {
        assert!(
            driver.contains(required),
            "missing sealed dispatch: {required}"
        );
    }
}

#[test]
fn p3sym_c4_executor_call_small() {
    case(
        PlainSymbolBacking::Projected,
        vec![begin(), call(0)],
        1,
        |_, session, owner, frame, _| {
            drive(session, owner, frame).unwrap();
            assert_eq!(prin1(words(owner, frame)[0]), oracle()[3]);
        },
    );
}
