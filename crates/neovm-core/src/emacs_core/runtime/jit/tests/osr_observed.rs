use super::*;
use crate::emacs_core::eval::{
    Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::cache;
use crate::emacs_core::value::LambdaParams;

fn observed_loop_template() -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("payload"), intern("limit"), intern("i")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Goto(1),
        Op::StackRef(0), // header: copy i
        Op::StackRef(2), // then limit (the copied i changed the depth)
        Op::Lss,
        Op::GotoIfNil(9),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Pop,
        Op::Pop,
        Op::Return,
    ];
    f.max_stack = 8;
    f
}

fn observed_loop() -> ByteCodeFunction {
    let mut f = observed_loop_template();
    f.seal_hand_assembled_ops();
    f
}

#[test]
fn osr_observed_malformed_snapshot_does_not_poison_the_cache() {
    for bad_binding_depth in [false, true] {
        let mut ctx = Context::new();
        let f = observed_loop();
        let snapshot = [Value::make_int(7), Value::make_int(5), Value::make_int(0)];
        ctx.bc_buf.extend_from_slice(&snapshot);
        let rejected = if bad_binding_depth {
            cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[0])
        } else {
            cache::try_run_osr(&mut ctx, &f, 1, &snapshot[..2], &[])
        };
        assert!(rejected.is_none());
        let Some(NativeRun::Ok(bits)) = cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[]) else {
            panic!("the next valid snapshot must still compile and run");
        };
        assert_eq!(Value::from_bits(bits), snapshot[0]);
        // The valid snapshot selected limit/i, but not the integer payload.
        let incompatible = [snapshot[0], Value::make_float(5.5), snapshot[2]];
        ctx.bc_buf.clear();
        ctx.bc_buf.extend_from_slice(&incompatible);
        let Some(NativeRun::DeoptAt(resume)) =
            cache::try_run_osr(&mut ctx, &f, 1, &incompatible, &[])
        else {
            panic!("the cached leaf must guard each new entry");
        };
        assert_eq!(resume.pc, 1);
        assert_eq!(resume.stack, incompatible);
        assert!(resume.binds.is_empty());
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn osr_observed_unguarded_payload_can_change_to_heap_through_a_real_gc_poll() {
    let mut ctx = Context::new();
    let f = observed_loop();
    let first = [Value::make_int(7), Value::make_int(3), Value::make_int(0)];
    ctx.bc_buf.extend_from_slice(&first);
    assert!(matches!(
        cache::try_run_osr(&mut ctx, &f, 1, &first, &[]),
        Some(NativeRun::Ok(_))
    ));
    let payload = Value::list(vec![Value::string("observed payload survives a poll")]);
    let second = [payload, Value::make_int(700), Value::make_int(0)];
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(&second);
    let before = ctx.tagged_heap.gc_collections();
    reset_bytecode_branch_poll_count();
    ctx.gc_stress = true;
    let run = cache::try_run_osr(&mut ctx, &f, 1, &second, &[]);
    ctx.gc_stress = false;
    let Some(NativeRun::Ok(bits)) = run else {
        panic!("payload type must not be a cached integer assumption");
    };
    assert_eq!(Value::from_bits(bits), payload);
    assert!(bytecode_branch_poll_count() >= 2);
    assert!(ctx.tagged_heap.gc_collections() > before);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn osr_observed_float_and_other_feedback_keep_generic_comparison_semantics() {
    use crate::emacs_core::jit::NumericFeedback;
    for feedback in [NumericFeedback::Float, NumericFeedback::Other] {
        let mut ctx = Context::new();
        let f = observed_loop();
        f.jit_runtime().record_numeric(3, f.ops.len(), feedback);
        let first = [Value::make_int(7), Value::make_int(3), Value::make_int(0)];
        ctx.bc_buf.extend_from_slice(&first);
        assert!(matches!(
            cache::try_run_osr(&mut ctx, &f, 1, &first, &[]),
            Some(NativeRun::Ok(_))
        ));
        let second = [
            Value::make_int(9),
            Value::make_float(3.5),
            Value::make_int(0),
        ];
        ctx.bc_buf.clear();
        ctx.bc_buf.extend_from_slice(&second);
        let Some(NativeRun::Ok(bits)) = cache::try_run_osr(&mut ctx, &f, 1, &second, &[]) else {
            panic!("non-fixnum comparison feedback must not gain an integer entry guard");
        };
        assert_eq!(Value::from_bits(bits), second[0]);
    }
}

#[test]
fn osr_observed_backedge_assignment_can_remove_an_observed_type_fact() {
    let mut ctx = Context::new();
    let mut f = observed_loop_template();
    let changed_limit = Value::make_float(5.5);
    f.constants = vec![changed_limit].into();
    f.ops = vec![
        Op::Goto(1),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(11),
        Op::Constant(0),
        Op::StackSet(2), // limit becomes a float
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Pop,
        Op::Pop,
        Op::Return,
    ];
    f.seal_hand_assembled_ops();
    let snapshot = [Value::make_int(7), Value::make_int(3), Value::make_int(0)];
    ctx.bc_buf.extend_from_slice(&snapshot);
    let Some(NativeRun::DeoptAt(resume)) = cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[])
    else {
        panic!("the next comparison must reject the changed limit");
    };
    assert_eq!(resume.pc, 3);
    assert_eq!(
        resume.stack,
        [
            snapshot[0],
            changed_limit,
            Value::make_int(1),
            Value::make_int(1),
            changed_limit
        ]
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn osr_observed_comparison_after_an_effect_keeps_that_effect_before_deopt() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq osr-observed-effect 0)").unwrap();
    let mut f = observed_loop_template();
    f.constants = vec![Value::symbol("osr-observed-effect"), Value::make_int(42)].into();
    f.ops = vec![
        Op::Goto(1),
        Op::Constant(1),
        Op::VarSet(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(11),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Pop,
        Op::Pop,
        Op::Return,
    ];
    f.seal_hand_assembled_ops();
    let first = [Value::make_int(7), Value::make_int(3), Value::make_int(0)];
    ctx.bc_buf.extend_from_slice(&first);
    assert!(matches!(
        cache::try_run_osr(&mut ctx, &f, 1, &first, &[]),
        Some(NativeRun::Ok(_))
    ));
    ctx.eval_str("(setq osr-observed-effect 0)").unwrap();
    let second = [first[0], Value::make_float(3.5), first[2]];
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(&second);
    let Some(NativeRun::DeoptAt(resume)) = cache::try_run_osr(&mut ctx, &f, 1, &second, &[]) else {
        panic!("the incompatible comparison must precisely deopt");
    };
    assert_eq!(resume.pc, 5);
    assert_eq!(
        ctx.eval_str("osr-observed-effect").unwrap(),
        Value::make_int(42)
    );
    assert_eq!(&resume.stack[..3], &second);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn osr_observed_interior_prefix_join_keeps_the_existing_entry_contract() {
    let mut ctx = Context::new();
    let mut f = observed_loop_template();
    f.ops = vec![
        Op::StackRef(0),
        Op::Goto(3), // normal entry joins inside the prefix
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(10),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(2),
        Op::Pop,
        Op::Pop,
        Op::Return,
    ];
    f.seal_hand_assembled_ops();
    let first = [Value::make_int(7), Value::make_int(3), Value::make_int(0)];
    ctx.bc_buf.extend_from_slice(&first);
    assert!(matches!(
        cache::try_run_osr(&mut ctx, &f, 2, &first, &[]),
        Some(NativeRun::Ok(_))
    ));
    let second = [first[0], Value::make_float(3.5), first[2]];
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(&second);
    let Some(NativeRun::DeoptAt(resume)) = cache::try_run_osr(&mut ctx, &f, 2, &second, &[]) else {
        panic!("the untyped limit must retain its comparison-site guard");
    };
    assert_eq!(resume.pc, 4);
    assert_eq!(&resume.stack[..3], &second);
}

#[test]
fn osr_observed_patched_constant_can_change_type_after_compilation() {
    let mut ctx = Context::new();
    let mut f = observed_loop_template();
    f.constants = vec![Value::make_int(5), Value::make_int(2)].into();
    f.ops = vec![
        Op::Goto(1),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(15),
        // Bound this test even if a broken guard treats a heap value as a large integer.
        Op::StackRef(0),
        Op::Constant(1),
        Op::Geq,
        Op::GotoIfNotNil(15),
        Op::Constant(0),
        Op::StackSet(2),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Pop,
        Op::Pop,
        Op::Return,
    ];
    f.jit_runtime().note_patched_prefix(1);
    f.seal_hand_assembled_ops();
    let snapshot = [Value::make_int(7), Value::make_int(3), Value::make_int(0)];
    ctx.bc_buf.extend_from_slice(&snapshot);
    assert!(matches!(
        cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[]),
        Some(NativeRun::Ok(_))
    ));
    // The marked prefix is per-instance data; the same cached source must load
    // this new constant base without using the first instance's integer type.
    let changed_limit = Value::make_float(5.5);
    f.constants = vec![changed_limit, Value::make_int(2)].into();
    let Some(NativeRun::DeoptAt(resume)) = cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[])
    else {
        panic!("a patched prefix must stay unknown in the backedge analysis");
    };
    assert_eq!(resume.pc, 3);
    assert_eq!(
        resume.stack,
        [
            snapshot[0],
            changed_limit,
            Value::make_int(1),
            Value::make_int(1),
            changed_limit
        ]
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}
