use super::*;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::{
    Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::cache;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

fn function(arity: usize, constants: Vec<Value>, ops: Vec<Op>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| intern(&format!("fixnum-range-arg-{i}")))
            .collect(),
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = constants.into();
    f.ops = ops;
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    f
}

#[test]
fn fixnum_range_add_sub_boundaries_and_raw_chains_match_wide_arithmetic() {
    let min = Value::MOST_NEGATIVE_FIXNUM;
    let max = Value::MOST_POSITIVE_FIXNUM;
    let values = [
        min,
        min + 1,
        -(1 << 53),
        -2,
        -1,
        0,
        1,
        2,
        1 << 53,
        max - 1,
        max,
    ];
    for op in [Op::Add, Op::Sub] {
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
                    let wide = if matches!(op, Op::Sub) {
                        i128::from(a) - i128::from(b)
                    } else {
                        i128::from(a) + i128::from(b)
                    };
                    let want = (i128::from(min)..=i128::from(max))
                        .contains(&wide)
                        .then(|| Value::make_int(wide as i64).bits());
                    assert_eq!(
                        leaf.call_for_test(&[Value::make_int(a), Value::make_int(b)]),
                        want,
                        "{op:?}({a}, {b}), raw_mask={raw_mask}"
                    );
                }
            }
        }
    }
}

#[test]
fn fixnum_range_overflow_resumes_as_bignum_without_replaying_prior_effects() {
    let mut ctx = Context::new();
    for (op, a, b) in [
        (Op::Add, Value::MOST_POSITIVE_FIXNUM, 1),
        (Op::Add, Value::MOST_NEGATIVE_FIXNUM, -1),
        (Op::Sub, Value::MOST_POSITIVE_FIXNUM, -1),
        (Op::Sub, Value::MOST_NEGATIVE_FIXNUM, 1),
    ] {
        ctx.eval_str("(setq fixnum-range-effects 0)").unwrap();
        let wide = if matches!(op, Op::Sub) {
            i128::from(a) - i128::from(b)
        } else {
            i128::from(a) + i128::from(b)
        };
        let f = function(
            2,
            vec![Value::symbol("fixnum-range-effects")],
            vec![
                Op::VarRef(0),
                Op::Add1,
                Op::VarSet(0),
                Op::StackRef(1),
                Op::StackRef(1),
                op,
                Op::Return,
            ],
        );
        let leaf = lower_leaf_full(&f.ops, &f.constants, 2, None, Some(&ctx.obarray), 0).unwrap();
        let args = [Value::make_int(a), Value::make_int(b)];
        let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &args)
        else {
            panic!("overflow must deopt precisely after the observable effect");
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
        assert_eq!(print_value(&result), wide.to_string());
        assert_eq!(
            ctx.eval_str("fixnum-range-effects").unwrap(),
            Value::make_int(1)
        );
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn fixnum_range_osr_overflow_after_gc_poll_keeps_current_operands() {
    let mut ctx = Context::new();
    for (op, step, bound) in [
        (Op::Add, 1, Value::MOST_POSITIVE_FIXNUM),
        (Op::Add, -1, Value::MOST_NEGATIVE_FIXNUM),
        (Op::Sub, -1, Value::MOST_POSITIVE_FIXNUM),
        (Op::Sub, 1, Value::MOST_NEGATIVE_FIXNUM),
    ] {
        let delta = if matches!(op, Op::Sub) { -step } else { step };
        let f = function(
            3,
            vec![Value::make_int(0)],
            vec![
                Op::StackRef(2),
                Op::Constant(0),
                Op::Gtr,
                Op::GotoIfNil(12),
                Op::StackRef(1),
                Op::StackRef(1),
                op,
                Op::StackSet(2),
                Op::StackRef(2),
                Op::Sub1,
                Op::StackSet(3),
                Op::Goto(0),
                Op::StackRef(1),
                Op::Return,
            ],
        );
        let snapshot = [
            Value::make_int(512),
            Value::make_int(bound - 300 * delta),
            Value::make_int(step),
        ];
        let collections = ctx.tagged_heap.gc_collections();
        reset_bytecode_branch_poll_count();
        ctx.gc_stress = true;
        let result = cache::try_run_osr(&mut ctx, &f, 0, &snapshot, &[]);
        ctx.gc_stress = false;
        let Some(NativeRun::DeoptAt(resume)) = result else {
            panic!("OSR must run through a poll and then deopt on overflow");
        };
        assert_eq!(bytecode_branch_poll_count(), 1);
        assert!(ctx.tagged_heap.gc_collections() > collections);
        assert_eq!(resume.pc, 6);
        assert_eq!(
            resume.stack,
            [
                Value::make_int(212),
                Value::make_int(bound),
                Value::make_int(step),
                Value::make_int(bound),
                Value::make_int(step),
            ]
        );
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}
