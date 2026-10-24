use super::*;
use crate::tagged::gc::PlainSymbolBacking;

#[test]
fn p3sym_c5_global_executor_cannot_admit_intern_even_if_unreachable() {
    Context::with_test_symbol_era(|era, id| {
        let tape = source(
            vec![
                Op::Jump { target: 2 },
                Op::Intern(InternOp::InitialIntern { name: 0, dst: 0 }),
            ],
            1,
        );
        assert!(matches!(
            GlobalSession::admit(&era, &[id], &[tape]),
            Err(AdmissionError::Program(Reject::Excluded("initial obarray")))
        ));
        era.admit_plain_symbol(id).unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

pub(super) fn source(ops: Vec<Op>, nslots: usize) -> Source {
    Source {
        insns: ops
            .into_iter()
            .map(|op| match op {
                Op::Global(global) => Insn::Global(global),
                other => Insn::Allowed(other),
            })
            .collect(),
        nslots,
        bound: 0,
    }
}

pub(super) fn drive<'era>(
    session: &GlobalSession,
    owner: &mut Mutator<'era>,
    frame: &mut Frame<'era>,
) -> Result<(), ExecError> {
    loop {
        match session.wait_step(owner, frame)? {
            Step::Done => return Ok(()),
            Step::Retry => thread::yield_now(),
            Step::Progress | Step::Safepoint => {}
        }
    }
}

/// The assertion body may unwind or the executor may refuse. Clear every
/// census root and retire before finishing the era in either case.
fn case(
    kind: PlainSymbolBacking,
    ops: Vec<Op>,
    nslots: usize,
    body: impl for<'era> FnOnce(
        &ParallelHeapEra<'_>,
        &GlobalSession,
        &mut Mutator<'era>,
        &mut Frame<'era>,
        SymId,
    ),
) {
    case_many(&[kind], ops, nslots, |era, session, owner, frame, ids| {
        body(era, session, owner, frame, ids[0]);
    });
}

pub(super) fn case_many(
    kinds: &[PlainSymbolBacking],
    ops: Vec<Op>,
    nslots: usize,
    body: impl for<'era> FnOnce(
        &ParallelHeapEra<'_>,
        &GlobalSession,
        &mut Mutator<'era>,
        &mut Frame<'era>,
        &[SymId],
    ),
) {
    Context::with_test_symbol_backings_era(kinds, |era, ids| {
        let session = GlobalSession::admit(&era, &ids, &[source(ops, nslots)]).unwrap();
        let mut stopped = era.stop().unwrap();
        let nil = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: same stopped heap and an immediate scalar.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        era.start_cons_storage().unwrap();
        let mut owner = era
            .register(1, nslots)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let mut frame = session.seed(&mut owner, 0, &nil).unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            body(&era, &session, &mut owner, &mut frame, &ids);
        }));
        let allocated = frame.machine.cons_count != 0;
        drop(frame);
        must(with_run(&mut owner, |run| {
            run.clear_root_slots()?;
            Ok(())
        }));
        owner.retire_rooted(&[]).unwrap();
        drop(nil);
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        if allocated {
            // Published young births require a stopped collection before the
            // era can return to serial. This is teardown, never setup GC.
            era.collect_stopped(&mut stopped).unwrap();
        } else {
            stopped.trace(|_, _| Ok(())).unwrap();
        }
        stopped.complete().unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        eprintln!("P3SYM_C3_EXECUTOR_TEARDOWN_OK {kinds:?}");
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    });
}

pub(super) fn words<'era>(owner: &mut Mutator<'era>, frame: &Frame<'era>) -> Vec<usize> {
    must(with_run(owner, |run| {
        Ok(frame
            .slots
            .iter()
            .map(|slot| slot.word_for_test(run))
            .collect())
    }))
}

#[test]
fn p3sym_c3_executor_primitives_match_gnu() {
    let oracle: Vec<_> = include_str!("../../symbol_globals_oracle.expected")
        .lines()
        .collect();
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for (row, op) in [
            GlobalOp::Value { symbol: 0, dst: 1 },
            GlobalOp::Set {
                symbol: 0,
                src: 0,
                dst: 1,
            },
            GlobalOp::Boundp { symbol: 0, dst: 1 },
            GlobalOp::Makunbound { symbol: 0, dst: 1 },
        ]
        .into_iter()
        .enumerate()
        {
            case(
                kind,
                vec![Op::ConstFixnum { dst: 0, value: 97 }, Op::Global(op)],
                2,
                |_era, session, owner, frame, id| {
                    drive(session, owner, frame).unwrap();
                    let result = words(owner, frame)[1];
                    let printed = if row == 3 {
                        assert_eq!(result, TaggedValue::from_sym_id(id).bits());
                        "t".to_owned()
                    } else {
                        prin1(result)
                    };
                    assert_eq!(printed, oracle[row]);
                },
            );
        }
    }
}

#[test]
fn p3sym_c3_executor_nil_void_and_graph_reads() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
        PlainSymbolBacking::VoidProjection,
    ] {
        let ops = vec![
            Op::ConstFixnum { dst: 0, value: 41 },
            Op::Cons {
                dst: 1,
                car: 0,
                cdr: 2,
            },
            Op::Global(GlobalOp::Set {
                symbol: 0,
                src: 1,
                dst: 2,
            }),
            Op::ConstNil { dst: 1 },
            Op::ConstNil { dst: 2 },
            Op::Global(GlobalOp::Value { symbol: 0, dst: 1 }),
            Op::Car { dst: 0, cell: 1 },
            Op::Global(GlobalOp::Set {
                symbol: 0,
                src: 2,
                dst: 1,
            }),
            Op::Global(GlobalOp::Value { symbol: 0, dst: 1 }),
            Op::Car { dst: 3, cell: 1 },
            Op::Cdr { dst: 4, cell: 1 },
            Op::Global(GlobalOp::Makunbound { symbol: 0, dst: 1 }),
            Op::Global(GlobalOp::Boundp { symbol: 0, dst: 2 }),
            Op::Global(GlobalOp::Value { symbol: 0, dst: 2 }),
        ];
        case(kind, ops, 5, |_era, session, owner, frame, _id| {
            assert_eq!(drive(session, owner, frame), Err(ExecError::VoidVariable));
            let result = words(owner, frame);
            assert_eq!(result[0], TaggedValue::fixnum(41).bits());
            assert_eq!(
                result[2],
                TaggedValue::NIL.bits(),
                "void read preserves dst"
            );
            assert_eq!(result[3], TaggedValue::NIL.bits(), "car of nil");
            assert_eq!(result[4], TaggedValue::NIL.bits(), "cdr of nil");
            assert_eq!(frame.machine.pc, 13, "error does not advance pc");
        });
    }
}

#[test]
fn p3sym_c3_executor_gate_checks_all_participants_and_handles() {
    Context::with_test_symbol_era(|era, id| {
        let global = source(vec![Op::Global(GlobalOp::Value { symbol: 0, dst: 0 })], 1);
        for mutation in [
            Op::SetCar { cell: 0, value: 0 },
            Op::SetCdr { cell: 0, value: 0 },
            Op::HtGet {
                table: 0,
                key: 0,
                found: 0,
                value: 0,
            },
            Op::HtPut {
                table: 0,
                key: 0,
                value: 0,
            },
            Op::HtRem {
                table: 0,
                key: 0,
                found: 0,
            },
            Op::HtClear { table: 0 },
            Op::HtMake {
                dst: 0,
                test: HashTableTest::Eq,
                size: 1,
            },
            Op::HtCopy { table: 0, dst: 0 },
            Op::HtCount { table: 0, dst: 0 },
        ] {
            let peer = source(vec![Op::Jump { target: 2 }, mutation], 1);
            let sources = [global.clone(), peer];
            assert!(matches!(
                GlobalSession::admit(&era, &[id], &sources),
                Err(AdmissionError::Program(Reject::Excluded(
                    "global session mutation/table"
                )))
            ));
        }
        for op in [
            GlobalOp::Value { symbol: 1, dst: 0 },
            GlobalOp::Set {
                symbol: 0,
                src: 1,
                dst: 0,
            },
            GlobalOp::Boundp { symbol: 0, dst: 1 },
            GlobalOp::Makunbound { symbol: 0, dst: 1 },
        ] {
            let bad = source(vec![Op::Jump { target: 2 }, Op::Global(op)], 1);
            assert!(matches!(
                GlobalSession::admit(&era, &[id], &[bad]),
                Err(AdmissionError::Program(Reject::Slot))
            ));
        }
        assert_eq!(
            prevalidate(
                &[Op::Global(GlobalOp::Value { symbol: 0, dst: 0 })],
                1,
                0,
                STEP_CAP
            ),
            Err(Reject::Excluded("symbol handles"))
        );
        // All refusals preceded registration; the same real handle still admits.
        era.admit_plain_symbol(id).unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn p3sym_c3_executor_rejects_transitive_mutable_global_before_activation() {
    let mut context = Context::new();
    context
        .eval_str("(setq p3sym-c3-bad-import '(1 . [2]))")
        .unwrap();
    let id = crate::emacs_core::intern::intern("p3sym-c3-bad-import");
    context
        .admit_owned_parallel(|projection| {
            let source = source(vec![Op::Global(GlobalOp::Value { symbol: 0, dst: 0 })], 1);
            assert!(matches!(
                GlobalSession::admit(projection.era(), &[id], &[source]),
                Err(AdmissionError::Program(Reject::RejectedType))
            ));
            let (collector, keeper) = projection.finish().unwrap();
            (collector, keeper, ())
        })
        .unwrap();
}

#[test]
fn p3sym_c3_executor_refusal_preserves_pc_and_retries() {
    case(
        PlainSymbolBacking::Unprojected,
        vec![Op::Global(GlobalOp::Value { symbol: 0, dst: 0 })],
        1,
        |era, session, owner, frame, _id| {
            let mut peer = era
                .register(2, 1)
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
                assert_eq!(
                    frame.machine.pc, 0,
                    "Busy is a legal refusal, never a completed op"
                );
                assert_eq!(words(owner, frame)[0], TaggedValue::NIL.bits());
                drop(writer);
                drop(run);
                frame.attempts_left = 0;
                assert_eq!(session.step(owner, frame), Err(ExecError::Backedge));
                assert_eq!(frame.machine.pc, 0);
                frame.attempts_left = STEP_CAP;
                drive(session, owner, frame).unwrap();
                assert_eq!(words(owner, frame)[0], TaggedValue::fixnum(73).bits());
            }));
            peer.retire_rooted(&[]).unwrap();
            if let Err(panic) = outcome {
                std::panic::resume_unwind(panic);
            }
        },
    );
}

#[test]
fn p3sym_c3_executor_uses_only_the_rooted_global_surface() {
    let driver = include_str!("../globals.rs");
    for forbidden in [
        "SymbolBacking",
        "symbols.backing",
        "probe(",
        "symbol_ref(",
        "symbol_mut(",
        "Value::from_bits",
        "unsafe {",
        "from_current_heap",
    ] {
        assert!(!driver.contains(forbidden), "executor bypass: {forbidden}");
    }
    for operation in [
        "sym_value_into(",
        "sym_set_into(",
        "sym_boundp_into(",
        "sym_makunbound_into(",
    ] {
        assert!(driver.contains(operation));
    }
}
