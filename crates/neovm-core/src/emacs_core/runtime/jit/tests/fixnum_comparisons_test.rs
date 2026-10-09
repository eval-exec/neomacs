use super::*;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::LambdaParams;

fn expected(op: &Op, a: i64, b: i64) -> Value {
    let yes = match op {
        Op::Eqlsign => a == b,
        Op::Lss => a < b,
        Op::Gtr => a > b,
        Op::Leq => a <= b,
        Op::Geq => a >= b,
        _ => unreachable!(),
    };
    if yes { Value::T } else { Value::NIL }
}

#[test]
fn fixnum_comparisons_preserve_order_at_boundaries_and_across_raw_arithmetic() {
    // Arguments stay dynamic: constant folding cannot satisfy these checks.
    let values = [
        Value::MOST_NEGATIVE_FIXNUM,
        Value::MOST_NEGATIVE_FIXNUM + 1,
        -(1_i64 << 53) - 1,
        -2,
        -1,
        0,
        1,
        2,
        (1_i64 << 53) + 1,
        Value::MOST_POSITIVE_FIXNUM - 1,
        Value::MOST_POSITIVE_FIXNUM,
    ];
    for op in [Op::Eqlsign, Op::Lss, Op::Gtr, Op::Leq, Op::Geq] {
        for raw_mask in 0..4 {
            let mut ops = vec![Op::StackRef(1)];
            if raw_mask & 1 != 0 {
                ops.extend([Op::Constant(0), Op::Add]);
            }
            ops.push(Op::StackRef(1));
            if raw_mask & 2 != 0 {
                ops.extend([Op::Constant(0), Op::Add]);
            }
            ops.extend([op.clone(), Op::Return]);
            let leaf = lower_leaf(&ops, &[Value::make_int(0)], 2).unwrap();
            for a in values {
                for b in values {
                    assert_eq!(
                        leaf.call_for_test(&[Value::make_int(a), Value::make_int(b)]),
                        Some(expected(&op, a, b).bits()),
                        "{op:?}({a}, {b}), raw_mask={raw_mask}"
                    );
                }
            }
        }
    }
}

#[test]
fn fixnum_comparisons_keep_both_operand_type_guards() {
    let _ctx = Context::new();
    let invalid = [
        Value::NIL,
        Value::T,
        Value::string("not a number"),
        Value::make_float(-0.5),
    ];
    for op in [Op::Eqlsign, Op::Lss, Op::Gtr, Op::Leq, Op::Geq] {
        let leaf = lower_leaf(
            &[Op::StackRef(1), Op::StackRef(1), op.clone(), Op::Return],
            &[],
            2,
        )
        .unwrap();
        for bad in invalid {
            for args in [[bad, Value::make_int(0)], [Value::make_int(0), bad]] {
                assert_eq!(leaf.call_for_test(&args), None, "{op:?}: {args:?}");
            }
        }
    }
}

#[test]
fn fixnum_comparison_deopt_resumes_after_effect_without_replaying_it() {
    let mut ctx = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("a"), intern("b")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![Value::symbol("fixnum-comparison-effects")].into();
    f.ops = vec![
        Op::VarRef(0),
        Op::Add1,
        Op::VarSet(0),
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Lss,
        Op::Return,
    ];
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    let leaf = lower_leaf_full(&f.ops, &f.constants, 2, None, Some(&ctx.obarray), 0).unwrap();
    for (args, want) in [
        ([Value::make_float(0.5), Value::make_int(1)], Value::T),
        ([Value::make_int(1), Value::make_float(0.5)], Value::NIL),
    ] {
        ctx.eval_str("(setq fixnum-comparison-effects 0)").unwrap();
        let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &args)
        else {
            panic!("a float at the fixnum site must deopt precisely");
        };
        assert_eq!(resume.pc, 5);
        assert_eq!(resume.stack, [args[0], args[1], args[0], args[1]]);
        assert_eq!(ctx.jit_root_stack_top, 0);
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
        assert_eq!(result, want);
        assert_eq!(
            ctx.eval_str("fixnum-comparison-effects").unwrap(),
            Value::make_int(1)
        );
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}
