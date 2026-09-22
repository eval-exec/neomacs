use super::*;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::LambdaParams;

fn branch(on_nil: bool, else_pop: bool, target: u32) -> Op {
    match (on_nil, else_pop) {
        (true, false) => Op::GotoIfNil(target),
        (false, false) => Op::GotoIfNotNil(target),
        (true, true) => Op::GotoIfNilElsePop(target),
        (false, true) => Op::GotoIfNotNilElsePop(target),
    }
}

fn expected_comparison(op: &Op, a: i64, b: i64) -> bool {
    match op {
        Op::Eqlsign => a == b,
        Op::Lss => a < b,
        Op::Leq => a <= b,
        Op::Gtr => a > b,
        Op::Geq => a >= b,
        _ => unreachable!(),
    }
}

#[test]
fn predicate_branches_preserve_comparison_polarity_and_retained_values() {
    let values = [
        Value::MOST_NEGATIVE_FIXNUM,
        -1,
        0,
        1,
        Value::MOST_POSITIVE_FIXNUM,
    ];
    let constants = [Value::make_int(111), Value::make_int(222)];
    for op in [Op::Eqlsign, Op::Lss, Op::Leq, Op::Gtr, Op::Geq] {
        for (on_nil, else_pop) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut ops = vec![
                Op::StackRef(1),
                Op::StackRef(1),
                op.clone(),
                branch(on_nil, else_pop, 6),
                Op::Constant(0),
                Op::Return,
            ];
            if !else_pop {
                ops.push(Op::Constant(1));
            }
            ops.push(Op::Return);
            let leaf = lower_leaf(&ops, &constants, 2).unwrap();
            for a in values {
                for b in values {
                    let yes = expected_comparison(&op, a, b);
                    let taken = yes != on_nil;
                    let want = if !taken {
                        constants[0]
                    } else if !else_pop {
                        constants[1]
                    } else if yes {
                        Value::T
                    } else {
                        Value::NIL
                    };
                    assert_eq!(
                        leaf.call_for_test(&[Value::make_int(a), Value::make_int(b)]),
                        Some(want.bits()),
                        "{op:?} {a} {b}, nil={on_nil} else_pop={else_pop}"
                    );
                }
            }
        }
    }
}

#[test]
fn predicate_branches_keep_unknown_nonboolean_truthiness() {
    let _ctx = Context::new();
    let values = [
        Value::NIL,
        Value::T,
        Value::make_int(0),
        Value::make_int(-1),
        Value::make_float(0.0),
        Value::string(""),
        Value::cons(Value::NIL, Value::NIL),
    ];
    let constants = [Value::make_int(111), Value::make_int(222)];
    for (on_nil, else_pop) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut ops = vec![
            Op::StackRef(0),
            branch(on_nil, else_pop, 4),
            Op::Constant(0),
            Op::Return,
        ];
        if !else_pop {
            ops.push(Op::Constant(1));
        }
        ops.push(Op::Return);
        let leaf = lower_leaf(&ops, &constants, 1).unwrap();
        for value in values {
            let taken = (value == Value::NIL) == on_nil;
            let want = if !taken {
                constants[0]
            } else if else_pop {
                value
            } else {
                constants[1]
            };
            assert_eq!(
                leaf.call_for_test(&[value]),
                Some(want.bits()),
                "{value:?}, nil={on_nil} else_pop={else_pop}"
            );
        }
    }
}

#[test]
fn predicate_branches_merge_predicates_with_unknown_values() {
    let _ctx = Context::new();
    let constants = [Value::make_int(111), Value::make_int(222)];
    for (on_nil, else_pop) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut ops = vec![
            Op::StackRef(2),
            Op::GotoIfNil(6),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Lss,
            Op::Goto(8),
            Op::StackRef(0),
            Op::Goto(8),
            branch(on_nil, else_pop, 11),
            Op::Constant(0),
            Op::Return,
        ];
        if !else_pop {
            ops.push(Op::Constant(1));
        }
        ops.push(Op::Return);
        let leaf = lower_leaf(&ops, &constants, 3).unwrap();
        for (selector, a, b, condition) in [
            (Value::T, Value::make_int(1), Value::make_int(2), Value::T),
            (Value::T, Value::make_int(2), Value::make_int(1), Value::NIL),
            (Value::NIL, Value::NIL, Value::NIL, Value::NIL),
            (
                Value::NIL,
                Value::NIL,
                Value::make_int(0),
                Value::make_int(0),
            ),
        ] {
            let taken = (condition == Value::NIL) == on_nil;
            let want = if !taken {
                constants[0]
            } else if else_pop {
                condition
            } else {
                constants[1]
            };
            assert_eq!(
                leaf.call_for_test(&[selector, a, b]),
                Some(want.bits()),
                "{selector:?}, {a:?}, {b:?}, nil={on_nil} else_pop={else_pop}"
            );
        }
    }
}

#[test]
fn predicate_branches_resume_type_deopt_without_repeating_effects() {
    let mut ctx = Context::new();
    for (on_nil, else_pop) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![intern("a"), intern("b")],
            optional: vec![],
            rest: None,
        });
        f.lexical = true;
        f.constants = vec![
            Value::symbol("predicate-branch-effects"),
            Value::make_int(111),
            Value::make_int(222),
        ]
        .into();
        f.ops = vec![
            Op::VarRef(0),
            Op::Add1,
            Op::VarSet(0),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Lss,
            branch(on_nil, else_pop, 9),
            Op::Constant(1),
            Op::Return,
        ];
        if !else_pop {
            f.ops.push(Op::Constant(2));
        }
        f.ops.push(Op::Return);
        f.max_stack = 8;
        f.seal_hand_assembled_ops();
        let leaf = lower_leaf_full(&f.ops, &f.constants, 2, None, Some(&ctx.obarray), 0).unwrap();
        for (a, b, yes) in [
            (Value::make_float(0.5), Value::make_int(1), true),
            (Value::make_int(1), Value::make_float(0.5), false),
        ] {
            ctx.eval_str("(setq predicate-branch-effects 0)").unwrap();
            let NativeRun::DeoptAt(resume) =
                leaf.call(&mut ctx as *mut Context as *mut u8, &[a, b])
            else {
                panic!("float comparison must deopt precisely")
            };
            assert_eq!(resume.pc, 5);
            assert_eq!(resume.stack, [a, b, a, b]);
            let result = Vm::from_context(&mut ctx)
                .run_resumed_frame(
                    &f,
                    Value::NIL,
                    resume.pc,
                    &resume.stack,
                    resume.handlers,
                    &resume.binds,
                    resume.spec_base,
                    resume.cond_base,
                )
                .unwrap();
            let taken = yes != on_nil;
            let want = if !taken {
                Value::make_int(111)
            } else if !else_pop {
                Value::make_int(222)
            } else if yes {
                Value::T
            } else {
                Value::NIL
            };
            assert_eq!(result, want);
            assert_eq!(
                ctx.eval_str("predicate-branch-effects").unwrap(),
                Value::make_int(1)
            );
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
    }
}
