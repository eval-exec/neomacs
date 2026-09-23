use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::value::LambdaParams;

fn raw_loop(ops: Vec<Op>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![Value::make_int(0), Value::make_int(3), Value::make_int(99)].into();
    f.ops = ops;
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    f
}

#[test]
fn osr_raw_zero_is_truthy_for_plain_and_else_pop_branches() {
    for (on_nil, else_pop) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut ops = vec![Op::Constant(0), Op::StackRef(0), Op::Goto(0)];
        // The unexpected false path of a NotNil branch returns 99. The Nil
        // variants jump straight to that failure block; their good path falls through.
        if !on_nil {
            ops.push(Op::Goto(0));
        }
        let good = ops.len() as u32;
        if !on_nil && else_pop {
            ops.push(Op::Pop);
        }
        ops.extend([
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(1),
            Op::Lss,
            Op::GotoIfNotNil(1),
            Op::Return,
        ]);
        let bad = ops.len() as u32;
        ops.extend([Op::Constant(2), Op::Return]);
        ops[2] = match (on_nil, else_pop) {
            (true, false) => Op::GotoIfNil(bad),
            (true, true) => Op::GotoIfNilElsePop(bad),
            (false, false) => Op::GotoIfNotNil(good),
            (false, true) => Op::GotoIfNotNilElsePop(good),
        };
        if !on_nil {
            ops[3] = Op::Goto(bad);
        }
        let f = raw_loop(ops);
        let cfg = analyze_cfg(&f.ops, &f.constants, None, 0).unwrap();
        let facts = compute_known_fixnum_slots(&f.ops, &f.constants, &cfg);
        assert!(uniform_raw_osr_slots(&cfg, &facts, Some(1))[0]);
        let mut ctx = Context::new();
        let snapshot = [Value::make_int(0)];
        ctx.bc_buf.extend_from_slice(&snapshot);
        assert_eq!(
            cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[]),
            Some(NativeRun::Ok(Value::make_int(3).bits())),
            "on_nil={on_nil} else_pop={else_pop}"
        );
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn osr_raw_condition_is_popped_before_normalizing_a_reused_temporary_slot() {
    // Slot 1 is a proven integer at leader 7, but it temporarily holds the
    // boolean comparison at pc 4. The edge conversion must pop that condition
    // before converting surviving slots to their uniform representation.
    let f = raw_loop(vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::Constant(1),
        Op::Lss,
        Op::GotoIfNil(13),
        Op::Constant(2),
        Op::Goto(7),
        Op::Pop,
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Return, // unreachable, retained to distinguish the exit leader
        Op::Return,
    ]);
    let cfg = analyze_cfg(&f.ops, &f.constants, None, 0).unwrap();
    let facts = compute_known_fixnum_slots(&f.ops, &f.constants, &cfg);
    let raw = uniform_raw_osr_slots(&cfg, &facts, Some(1));
    assert!(raw[0] && raw[1]);
    let mut ctx = Context::new();
    let snapshot = [Value::make_int(0)];
    ctx.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(3).bits()))
    );
}

#[test]
fn osr_raw_switch_backedges_root_heap_payloads_and_preserve_quit_cadence() {
    use crate::emacs_core::eval::{bytecode_branch_poll_count, reset_bytecode_branch_poll_count};
    use crate::emacs_core::value::HashTableTest;
    let mut ctx = Context::new();
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (key, target) in [(0, 2), (1, 4)] {
            let value = Value::make_int(key);
            ht.insert(value.to_hash_key(&ht.test), value, Value::make_int(target));
        }
    });
    ctx.bc_buf.push(table);
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![
        Value::make_int(0),
        Value::make_int(2),
        table,
        Value::make_int(1024),
    ]
    .into();
    f.ops = vec![
        Op::Constant(3),
        Op::Nil,
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
        Op::GotoIfNotNil(20),
        Op::StackRef(1),
        Op::Constant(1),
        Op::Rem,
        Op::Constant(2),
        Op::Switch,
        Op::Return,
    ];
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    let cfg = analyze_cfg(&f.ops, &f.constants, None, 0).unwrap();
    let facts = compute_known_fixnum_slots(&f.ops, &f.constants, &cfg);
    let raw = uniform_raw_osr_slots(&cfg, &facts, Some(2));
    assert!(raw[0] && !raw[1]);
    for n in [1, 255, 256, 511, 1024] {
        let snapshot = [Value::make_int(n), Value::NIL];
        ctx.bc_buf.truncate(1);
        ctx.bc_buf.extend_from_slice(&snapshot);
        let before = ctx.tagged_heap.gc_collections();
        reset_bytecode_branch_poll_count();
        ctx.gc_stress = true;
        let result = cache::try_run_osr(&mut ctx, &f, 2, &snapshot, &[]);
        ctx.gc_stress = false;
        let Some(NativeRun::Ok(bits)) = result else {
            panic!("{result:?}");
        };
        let polls = ((n - 1) / 255) as usize;
        assert_eq!(bytecode_branch_poll_count(), polls);
        assert!(ctx.tagged_heap.gc_collections() >= before + polls);
        assert_eq!(ctx.jit_root_stack_top, 0);
        let mut list = Value::from_bits(bits);
        for expected in 1..=n {
            assert!(list.is_cons(), "missing cell {expected}");
            assert_eq!(list.cons_car(), Value::make_int(expected));
            list = list.cons_cdr();
        }
        assert!(list.is_nil());
    }
    let snapshot = [Value::make_int(256), Value::NIL];
    ctx.bc_buf.truncate(1);
    ctx.bc_buf.extend_from_slice(&snapshot);
    reset_bytecode_branch_poll_count();
    ctx.set_quit_flag_value(Value::T);
    let result = cache::try_run_osr(&mut ctx, &f, 2, &snapshot, &[]);
    ctx.set_quit_flag_value(Value::NIL);
    assert_eq!(result, Some(NativeRun::Signal));
    assert_eq!(bytecode_branch_poll_count(), 1);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn osr_raw_overflow_retags_the_snapshot_and_resumes_without_replaying_effects() {
    use crate::emacs_core::bytecode::vm::Vm;
    let mut ctx = Context::new();
    ctx.eval_str("(setq osr-raw-effects 0)").unwrap();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![Value::make_int(0), Value::symbol("osr-raw-effects")].into();
    f.ops = vec![
        Op::Constant(0),
        Op::VarRef(1),
        Op::Add1,
        Op::VarSet(1),
        Op::Goto(5),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Nil,
        Op::GotoIfNotNil(1),
        Op::Return,
    ];
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    let cfg = analyze_cfg(&f.ops, &f.constants, None, 0).unwrap();
    let facts = compute_known_fixnum_slots(&f.ops, &f.constants, &cfg);
    assert!(uniform_raw_osr_slots(&cfg, &facts, Some(1))[0]);
    let max = Value::MOST_POSITIVE_FIXNUM;
    let snapshot = [Value::make_int(max)];
    ctx.bc_buf.extend_from_slice(&snapshot);
    let Some(NativeRun::DeoptAt(resume)) = cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[])
    else {
        panic!("raw increment must precisely deopt on overflow");
    };
    assert_eq!(resume.pc, 6);
    assert_eq!(resume.stack, [snapshot[0], snapshot[0]]);
    assert_eq!(ctx.eval_str("osr-raw-effects").unwrap(), Value::make_int(1));
    assert_eq!(ctx.jit_root_stack_top, 0);
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
    } = *resume;
    ctx.bc_buf.clear();
    let value = Vm::from_context(&mut ctx)
        .run_resumed_frame_latched(
            &f,
            Value::NIL,
            pc,
            &stack,
            handlers,
            &binds,
            spec_base,
            cond_base,
            true,
        )
        .unwrap();
    assert_eq!(
        value.as_bignum().unwrap().to_string(),
        (max + 1).to_string()
    );
    assert_eq!(ctx.eval_str("osr-raw-effects").unwrap(), Value::make_int(1));
    assert!(ctx.bc_buf.is_empty());
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn osr_raw_varref_preserves_loop_slots_on_inline_and_fallback_reads() {
    use super::shims::VARREF_SHIM_CALLS;
    let mut ctx = Context::new();
    ctx.eval_str(
        "(progn (defvar raw-osr-read 7)
                (defvaralias 'raw-osr-read-alias 'raw-osr-read)
                (defvar raw-osr-read-local 8)
                (make-local-variable 'raw-osr-read-local)
                (setq raw-osr-read-local 9))",
    )
    .unwrap();
    for (name, value, inline) in [
        ("raw-osr-read", 7, true),
        ("raw-osr-read-alias", 7, false),
        ("raw-osr-read-local", 9, false),
    ] {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: None,
        });
        f.lexical = true;
        f.constants = vec![
            Value::make_int(0),
            Value::symbol(name),
            Value::make_int(1024),
        ]
        .into();
        f.ops = vec![
            Op::Constant(2),
            Op::Constant(0),
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(14),
            Op::StackRef(0),
            Op::VarRef(1),
            Op::Add,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(2),
            Op::List(2),
            Op::Return,
        ];
        f.max_stack = 8;
        f.seal_hand_assembled_ops();
        let cfg = analyze_cfg(&f.ops, &f.constants, None, 0).unwrap();
        let facts = compute_known_fixnum_slots(&f.ops, &f.constants, &cfg);
        let raw = uniform_raw_osr_slots(&cfg, &facts, Some(2));
        assert!(raw[0] && raw[1]);
        let snapshot = [Value::make_int(1024), Value::make_int(0)];
        ctx.bc_buf.clear();
        ctx.bc_buf.extend_from_slice(&snapshot);
        VARREF_SHIM_CALLS.with(|c| c.set(0));
        let result = cache::try_run_osr(&mut ctx, &f, 2, &snapshot, &[]);
        let Some(NativeRun::Ok(bits)) = result else {
            panic!("{name}: {result:?}")
        };
        assert_eq!(
            crate::emacs_core::print::print_value(&Value::from_bits(bits)),
            format!("(0 {})", 1024 * value)
        );
        assert_eq!(
            VARREF_SHIM_CALLS.with(|c| c.get()),
            if inline { 0 } else { 1024 }
        );
        assert_eq!(&ctx.bc_buf[..], &snapshot);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn osr_raw_varref_type_change_retags_live_slots_for_precise_resume() {
    use crate::emacs_core::bytecode::vm::Vm;
    let mut ctx = Context::new();
    ctx.eval_str("(defvar raw-osr-float-read 0.5)").unwrap();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![
        Value::make_int(0),
        Value::symbol("raw-osr-float-read"),
        Value::make_int(3),
    ]
    .into();
    f.ops = vec![
        Op::Constant(2),
        Op::Constant(0),
        Op::StackRef(1),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(14),
        Op::StackRef(0),
        Op::VarRef(1),
        Op::Add,
        Op::StackSet(1),
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(2),
        Op::List(2),
        Op::Return,
    ];
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    let cfg = analyze_cfg(&f.ops, &f.constants, None, 0).unwrap();
    let facts = compute_known_fixnum_slots(&f.ops, &f.constants, &cfg);
    let raw = uniform_raw_osr_slots(&cfg, &facts, Some(2));
    assert!(raw[0] && raw[1]);
    let snapshot = [Value::make_int(3), Value::make_int(0)];
    ctx.bc_buf.extend_from_slice(&snapshot);
    let Some(NativeRun::DeoptAt(resume)) = cache::try_run_osr(&mut ctx, &f, 2, &snapshot, &[])
    else {
        panic!("the arithmetic after the float read must precisely deopt");
    };
    assert_eq!(resume.pc, 8);
    assert_eq!(resume.stack.len(), 4);
    assert_eq!(&resume.stack[..3], &[snapshot[0], snapshot[1], snapshot[1]]);
    assert_eq!(resume.stack[3].as_float(), Some(0.5));
    assert_eq!(&ctx.bc_buf[..], &snapshot);
    assert_eq!(ctx.jit_root_stack_top, 0);
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
    } = *resume;
    ctx.bc_buf.clear();
    let result = Vm::from_context(&mut ctx)
        .run_resumed_frame_latched(
            &f,
            Value::NIL,
            pc,
            &stack,
            handlers,
            &binds,
            spec_base,
            cond_base,
            true,
        )
        .unwrap();
    assert_eq!(crate::emacs_core::print::print_value(&result), "(0 1.5)");
    assert!(ctx.bc_buf.is_empty());
    assert_eq!(ctx.jit_root_stack_top, 0);
}
