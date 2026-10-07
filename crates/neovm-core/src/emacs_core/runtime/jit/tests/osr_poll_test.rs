use super::*;
use crate::emacs_core::eval::{
    Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::cache;
use crate::emacs_core::value::{HashTableTest, LambdaParams};

fn loop_function(arity: usize, constants: Vec<Value>, ops: Vec<Op>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| intern(&format!("osr-poll-{i}")))
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

// Invoke an actual cached OSR entry, bypassing only the interpreter heat gate.
fn run(ctx: &mut Context, f: &ByteCodeFunction, pc: usize, stack: &[Value]) -> NativeRun {
    cache::try_run_osr(ctx, f, pc, stack, &[]).expect("loop must transfer into native OSR")
}

#[test]
fn osr_poll_conditional_edges_preserve_cadence_and_quit() {
    let mut ctx = Context::new();
    for (on_nil, else_pop) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut ops = vec![];
        if else_pop {
            ops.extend([Op::Nil, Op::Pop]);
        }
        let target = if else_pop { 1 } else { 0 };
        ops.extend([
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(0),
            if on_nil { Op::Leq } else { Op::Gtr },
            match (on_nil, else_pop) {
                (false, false) => Op::GotoIfNotNil(target),
                (true, false) => Op::GotoIfNil(target),
                (false, true) => Op::GotoIfNotNilElsePop(target),
                (true, true) => Op::GotoIfNilElsePop(target),
            },
            Op::Return,
        ]);
        let f = loop_function(1, vec![Value::make_int(0)], ops);
        for n in [1, 254, 255, 256, 511] {
            let mut snapshot = vec![Value::make_int(n)];
            if else_pop {
                snapshot.push(Value::NIL);
            }
            reset_bytecode_branch_poll_count();
            assert_eq!(
                run(&mut ctx, &f, target as usize, &snapshot),
                NativeRun::Ok(Value::make_int(0).bits())
            );
            assert_eq!(
                bytecode_branch_poll_count(),
                ((n - 1) / 255) as usize,
                "on_nil={on_nil} else_pop={else_pop} n={n}"
            );
        }
        let mut snapshot = vec![Value::make_int(256)];
        if else_pop {
            snapshot.push(Value::NIL);
        }
        reset_bytecode_branch_poll_count();
        ctx.set_quit_flag_value(Value::T);
        let result = run(&mut ctx, &f, target as usize, &snapshot);
        ctx.set_quit_flag_value(Value::NIL);
        assert_eq!(result, NativeRun::Signal);
        assert_eq!(bytecode_branch_poll_count(), 1);
        assert!(take_pending_flow().is_some());
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn osr_poll_shares_one_counter_across_alternating_backedges() {
    let mut ctx = Context::new();
    let f = loop_function(
        1,
        vec![Value::make_int(0), Value::make_int(2)],
        vec![
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Leq,
            Op::GotoIfNotNil(14),
            Op::StackRef(0),
            Op::Constant(1),
            Op::Rem,
            Op::Constant(0),
            Op::Eqlsign,
            Op::GotoIfNotNil(0),
            Op::Goto(0),
            Op::Return,
        ],
    );
    for n in [1, 254, 255, 256, 511, 1024] {
        reset_bytecode_branch_poll_count();
        assert_eq!(
            run(&mut ctx, &f, 0, &[Value::make_int(n)]),
            NativeRun::Ok(Value::make_int(0).bits())
        );
        assert_eq!(bytecode_branch_poll_count(), ((n - 1) / 255) as usize);
    }
}

#[test]
fn osr_poll_shares_one_counter_across_nested_headers() {
    let mut ctx = Context::new();
    let f = loop_function(
        1,
        vec![Value::make_int(0), Value::make_int(3)],
        vec![
            Op::Constant(1),
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNotNil(1),
            Op::Pop,
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNotNil(0),
            Op::Return,
        ],
    );
    for n in [1, 85, 86, 170, 171, 512] {
        reset_bytecode_branch_poll_count();
        assert_eq!(
            run(&mut ctx, &f, 0, &[Value::make_int(n)]),
            NativeRun::Ok(Value::make_int(0).bits())
        );
        assert_eq!(bytecode_branch_poll_count(), ((3 * n - 1) / 255) as usize);
    }
}

#[test]
fn osr_poll_switch_siblings_preserve_cadence_and_native_list_roots() {
    let mut ctx = Context::new();
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (key, target) in [(0, 0), (1, 2)] {
            let value = Value::make_int(key);
            ht.insert(value.to_hash_key(&ht.test), value, Value::make_int(target));
        }
    });
    ctx.bc_buf.push(table);
    let f = loop_function(
        2,
        vec![Value::make_int(0), Value::make_int(2), table],
        vec![
            Op::Nil,
            Op::Pop,
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Cons,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::StackRef(1),
            Op::Constant(0),
            Op::Leq,
            Op::GotoIfNotNil(18),
            Op::StackRef(1),
            Op::Constant(1),
            Op::Rem,
            Op::Constant(2),
            Op::Switch,
            Op::Return,
        ],
    );
    for n in [1, 255, 256, 511, 1024] {
        let before = ctx.tagged_heap.gc_collections();
        reset_bytecode_branch_poll_count();
        ctx.gc_stress = true;
        let result = run(&mut ctx, &f, 0, &[Value::make_int(n), Value::NIL]);
        ctx.gc_stress = false;
        let polls = ((n - 1) / 255) as usize;
        assert_eq!(bytecode_branch_poll_count(), polls);
        assert!(ctx.tagged_heap.gc_collections() >= before + polls);
        assert_eq!(ctx.jit_root_stack_top, 0);
        let NativeRun::Ok(bits) = result else {
            panic!("{result:?}");
        };
        let mut list = Value::from_bits(bits);
        for expected in 1..=n {
            assert!(list.is_cons(), "missing cell {expected}");
            assert_eq!(list.cons_car(), Value::make_int(expected));
            list = list.cons_cdr();
        }
        assert!(list.is_nil());
    }
}

#[test]
fn osr_poll_unconditional_edge_keeps_native_list_alive_across_gc() {
    let mut ctx = Context::new();
    let f = loop_function(
        1,
        vec![Value::make_int(0)],
        vec![
            Op::Nil,
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(13),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Cons,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(1),
            Op::Return,
        ],
    );
    let before = ctx.tagged_heap.gc_collections();
    reset_bytecode_branch_poll_count();
    ctx.gc_stress = true;
    let result = run(&mut ctx, &f, 1, &[Value::make_int(1024), Value::NIL]);
    ctx.gc_stress = false;
    assert_eq!(bytecode_branch_poll_count(), 4);
    assert!(ctx.tagged_heap.gc_collections() >= before + 4);
    assert_eq!(ctx.jit_root_stack_top, 0);
    let NativeRun::Ok(bits) = result else {
        panic!("{result:?}");
    };
    let mut list = Value::from_bits(bits);
    for n in 1..=1024 {
        assert!(list.is_cons(), "missing cell {n}");
        assert_eq!(list.cons_car(), Value::make_int(n));
        list = list.cons_cdr();
    }
    assert!(list.is_nil());
}

#[test]
fn osr_poll_overflow_after_poll_keeps_current_iteration_for_deopt() {
    let mut ctx = Context::new();
    let f = loop_function(
        2,
        vec![Value::make_int(0)],
        vec![
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(11),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(0),
            Op::Return,
        ],
    );
    let max = (1_i64 << 61) - 1;
    reset_bytecode_branch_poll_count();
    let result = run(
        &mut ctx,
        &f,
        0,
        &[Value::make_int(512), Value::make_int(max - 300)],
    );
    assert_eq!(bytecode_branch_poll_count(), 1);
    let NativeRun::DeoptAt(resume) = result else {
        panic!("{result:?}");
    };
    assert_eq!(resume.pc, 5);
    assert_eq!(
        resume.stack,
        [
            Value::make_int(212),
            Value::make_int(max),
            Value::make_int(max)
        ]
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}
