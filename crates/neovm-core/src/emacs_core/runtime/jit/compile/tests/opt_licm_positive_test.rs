//! Native invariant-motion parity and exact cold replay.
//!
//! Intended as a child test module of compile/tests/opt_reps.rs so `super::*`
//! uses the existing real builder, checked native lowering, roots and Tier-0
//! helpers. The reference delegates opaque behavior to the real Tier-0 path.
//! Source programs are independently sealed Tier-0 controls, grounded in the
//! frozen GNU67 cold-loop/zero-trip and scalar arithmetic semantics. They do
//! not claim to be a newly captured GNU bytecode artifact or normal admission.
//! Threading: all plans, contexts and scratch-root scopes belong to this test
//! invocation's mutator; no runtime state/cache is added.

use super::*;
use crate::emacs_core::jit::opt::passes::{bools, fold, gvn, licm, range, reps, reps_lift};

fn licm_settings() -> Settings {
    let settings = Settings::enter();
    force_opt_passes_for_test(Some(OptPasses {
        reps: true,
        bool_rep: true,
        range: true,
        licm: true,
        ..OptPasses::default()
    }));
    settings
}

fn licm_prepared(source: &ByteCodeFunction) -> ir::Func {
    let mut func = plan(source);
    // The explicit checked lift makes speculation visible in authoritative IR.
    // It never stamps the unknown Arg as FIXNUM or supplies runtime feedback.
    reps_lift::run(&mut func, &[]).unwrap();
    fold::run(&mut func).unwrap();
    bools::run(&mut func).unwrap();
    gvn::run(&mut func).unwrap();
    // LICM receives the real Range-before-LICM pipeline: interval-proven
    // arithmetic has already lost its observable overflow check.
    range::run(&mut func).unwrap();
    func.verify().unwrap();
    func
}

fn selected_reps(mut func: ir::Func) -> ir::Func {
    reps::run(&mut func).unwrap();
    func.verify().unwrap();
    func
}

fn same_original_observers(before: &ir::Func, after: &ir::Func) {
    assert_eq!(
        &after.frames[..before.frames.len()],
        before.frames.as_slice()
    );
    assert_eq!(after.entry_stacks, before.entry_stacks);
    assert_eq!(after.source_states.len(), before.source_states.len());
    for (actual, old) in after.source_states.iter().zip(&before.source_states) {
        match (actual, old) {
            (Some(actual), Some(old)) => {
                assert_eq!(actual.frame, old.frame);
                assert_eq!(actual.block, old.block);
                assert_eq!(actual.pre, old.pre);
                assert_eq!(actual.post, old.post);
            }
            (None, None) => (),
            _ => panic!("original source observation presence changed"),
        }
    }
    for (old, new) in before.insts.iter().zip(&after.insts) {
        assert_eq!(new.pc, old.pc);
        assert_eq!(new.result, old.result);
        assert_eq!(new.frame, old.frame);
    }
}

fn owner(func: &ir::Func, id: ir::Inst) -> ir::Block {
    ir::Block(
        func.blocks
            .iter()
            .position(|block| block.insts.contains(&id))
            .unwrap() as u32,
    )
}

fn defining_inst(func: &ir::Func, value: ir::Value) -> ir::Inst {
    let ir::ValueDef::Inst(id) = func.values[func.resolve(value).unwrap().index()].def else {
        panic!("attached instruction definition required")
    };
    id
}

fn header_n_guard(func: &ir::Func) -> ir::Inst {
    let arg = func
        .insts
        .iter()
        .find(|inst| inst.op == ir::Opcode::Arg(0))
        .unwrap()
        .result
        .unwrap();
    ir::Inst(
        func.insts
            .iter()
            .position(|inst| {
                inst.op == ir::Opcode::CheckType(TypeSet::FIXNUM)
                    && inst.args.len() == 1
                    && func.resolve(inst.args[0]) == func.resolve(arg)
            })
            .expect("real invariant checked view at loop header") as u32,
    )
}

fn replay_stack(func: &ir::Func, header: ir::Block, preheader: ir::Block) -> Vec<ir::Value> {
    let ir::Term::Jump(edge) = &func.blocks[preheader.index()].term else {
        panic!("one unconditional original preheader")
    };
    assert_eq!(edge.target, header);
    func.entry_stacks[header.index()]
        .iter()
        .map(|&value| {
            let value = func.resolve(value).unwrap();
            match func.values[value.index()].def {
                ir::ValueDef::Param { block, index } if block == header => {
                    edge.args[index as usize]
                }
                _ => value,
            }
        })
        .collect()
}

fn reference_ok(ctx: &mut Context, func: &ir::Func, args: &[Value]) -> Value {
    let eval::Outcome::Returned(value) = eval::evaluate(
        func,
        ctx,
        eval::Inputs {
            args,
            ..Default::default()
        },
    )
    .unwrap()
    .outcome
    else {
        panic!("reference success before feature assertion")
    };
    value.to_value()
}

fn plain_no_gc(ctx: &mut Context) {
    ctx.gc_stress = false;
    ctx.set_gc_threshold(usize::MAX);
}

fn resume_reference(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    snapshot: &eval::Snapshot,
) -> Result<Value, Flow> {
    assert_eq!(snapshot.handlers, 0);
    assert_eq!(snapshot.binds, 0);
    let stack = snapshot
        .stack
        .iter()
        .map(|bits| bits.to_value())
        .collect::<Vec<_>>();
    let _roots = Roots::new(&stack);
    let spec_base = ctx.specpdl.len();
    let condition_base = ctx.condition_stack_depth_for_test();
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.run_resumed_frame(
        source,
        Value::NIL,
        snapshot.pc as usize,
        &stack,
        0,
        &[],
        spec_base,
        condition_base,
    )
}

fn reset_watcher(ctx: &mut Context) {
    ctx.obarray
        .set_symbol_value("opt-licm-watch-count", Value::fixnum(0));
}

fn assert_one_store(ctx: &Context) {
    assert_eq!(
        ctx.obarray.symbol_value_copied("opt-licm-side"),
        Some(Value::symbol("stored"))
    );
    assert_eq!(
        ctx.obarray.symbol_value_copied("opt-licm-watch-count"),
        Some(Value::fixnum(1))
    );
}

#[test]
fn opt_licm_native_anticipated_header_guard_replays_full_entry_after_store() {
    let _settings = licm_settings();
    let source = function(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::Constant(2),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Gtr,
            Op::GotoIfNil(11),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(3),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            Value::symbol("stored"),
            Value::symbol("opt-licm-side"),
            Value::fixnum(0),
        ],
        1,
    );
    let before = licm_prepared(&source);
    let guard = header_n_guard(&before);
    let header = owner(&before, guard);
    assert!(before.blocks[header.index()].loop_header.is_some());
    assert_eq!(before.blocks[header.index()].pc, 3);
    let baseline_plan = selected_reps(before.clone());
    let baseline = lower(&source, &baseline_plan);
    let mut ctx = Context::new();
    plain_no_gc(&mut ctx);
    ctx.eval_str("(setq opt-licm-watch-count 0) (add-variable-watcher 'opt-licm-side (lambda (&rest _) (setq opt-licm-watch-count (1+ opt-licm-watch-count))))").unwrap();
    for n in [0, 1, 255, 510] {
        let args = [Value::fixnum(n)];
        reset_watcher(&mut ctx);
        let expected = tier0(&mut ctx, &source, &args).unwrap();
        assert_eq!(expected, Value::fixnum(n));
        assert_one_store(&ctx);
        reset_watcher(&mut ctx);
        assert_eq!(reference_ok(&mut ctx, &baseline_plan, &args), expected);
        assert_one_store(&ctx);
        reset_watcher(&mut ctx);
        assert_eq!(
            native_ok(&mut ctx, &baseline, &args),
            expected,
            "real checked baseline precedes candidate/hoist assertion"
        );
        assert_one_store(&ctx);
    }
    let wrong = [Value::symbol("wrong")];
    reset_watcher(&mut ctx);
    let original_error = tier0(&mut ctx, &source, &wrong).unwrap_err();
    assert_one_store(&ctx);
    reset_watcher(&mut ctx);
    let original_plan = plan(&source);
    let eval::EvalError::Flow(reference_error) = eval::evaluate(
        &original_plan,
        &mut ctx,
        eval::Inputs {
            args: &wrong,
            ..Default::default()
        },
    )
    .unwrap_err() else {
        panic!("untouched opaque reference must preserve the original signal")
    };
    assert_eq!(
        format!("{reference_error:?}"),
        format!("{original_error:?}")
    );
    assert_one_store(&ctx);
    reset_watcher(&mut ctx);
    let eval::Outcome::Deopt(snapshot) = eval::evaluate(
        &baseline_plan,
        &mut ctx,
        eval::Inputs {
            args: &wrong,
            ..Default::default()
        },
    )
    .unwrap()
    .outcome
    else {
        panic!("explicit checked baseline must deopt before the original comparison")
    };
    assert_eq!(snapshot.pc, 5);
    let resumed_error = resume_reference(&mut ctx, &source, &snapshot).unwrap_err();
    assert_eq!(format!("{resumed_error:?}"), format!("{original_error:?}"));
    assert_one_store(&ctx);
    reset_watcher(&mut ctx);
    let NativeRun::DeoptAt(resume) = baseline.call(&mut ctx as *mut Context as *mut u8, &wrong)
    else {
        panic!("checked baseline native wrong type must provide its original frame")
    };
    let _resume = Roots::new(&resume.stack);
    assert_eq!(resume.pc, 5);
    let resumed_error = resumed(&mut ctx, &source, &resume).unwrap_err();
    assert_eq!(format!("{resumed_error:?}"), format!("{original_error:?}"));
    assert_one_store(&ctx);
    let mut after = before.clone();
    let stats = licm::run(&mut after).unwrap();
    after.verify().unwrap();
    same_original_observers(&before, &after);
    let candidate_plan = selected_reps(after.clone());
    let candidate = lower(&source, &candidate_plan);
    for n in [0, 1, 255, 510] {
        let args = [Value::fixnum(n)];
        reset_watcher(&mut ctx);
        assert_eq!(
            reference_ok(&mut ctx, &candidate_plan, &args),
            Value::fixnum(n)
        );
        assert_one_store(&ctx);
        reset_watcher(&mut ctx);
        assert_eq!(native_ok(&mut ctx, &candidate, &args), Value::fixnum(n));
        assert_one_store(&ctx);
    }
    reset_watcher(&mut ctx);
    let eval::Outcome::Deopt(snapshot) = eval::evaluate(
        &candidate_plan,
        &mut ctx,
        eval::Inputs {
            args: &wrong,
            ..Default::default()
        },
    )
    .unwrap()
    .outcome
    else {
        panic!("hoisted guard must cold replay the first loop entry on wrong type")
    };
    assert_eq!(snapshot.pc, 3);
    let resumed_error = resume_reference(&mut ctx, &source, &snapshot).unwrap_err();
    assert_eq!(format!("{resumed_error:?}"), format!("{original_error:?}"));
    assert_one_store(&ctx);
    reset_watcher(&mut ctx);
    let NativeRun::DeoptAt(resume) = candidate.call(&mut ctx as *mut Context as *mut u8, &wrong)
    else {
        panic!("hoisted native guard must deopt at first loop entry")
    };
    let _resume = Roots::new(&resume.stack);
    assert_eq!(resume.pc, 3);
    let resumed_error = resumed(&mut ctx, &source, &resume).unwrap_err();
    assert_eq!(format!("{resumed_error:?}"), format!("{original_error:?}"));
    assert_one_store(&ctx);
    // Assertions follow original/reference/native semantic validity.
    assert_eq!(stats.guards_hoisted, 1);
    let replaced = &after.insts[guard.index()];
    assert!(matches!(replaced.op, ir::Opcode::Refine(_)));
    let moved = defining_inst(&after, replaced.args[0]);
    let preheader = owner(&after, moved);
    assert_ne!(preheader, header);
    let new_guard = &after.insts[moved.index()];
    assert_eq!(new_guard.op, ir::Opcode::CheckType(TypeSet::FIXNUM));
    let frame = &after.frames[new_guard.frame.unwrap().index()];
    assert_eq!(new_guard.pc, 3);
    assert_eq!(frame.pc, 3);
    assert_eq!(
        frame.stack.as_ref(),
        replay_stack(&before, header, preheader).as_slice()
    );
    assert_eq!(frame.stack.len(), 2);
    let state = before.source_states[3].as_ref().unwrap();
    assert_eq!(state.pre, before.entry_stacks[header.index()]);
    let instructions = &after.blocks[preheader.index()].insts;
    let store_position = instructions
        .iter()
        .position(|id| {
            matches!(
                after.insts[id.index()].op,
                ir::Opcode::Opaque(Op::VarSet(1))
            )
        })
        .unwrap();
    let moved_position = instructions.iter().position(|&id| id == moved).unwrap();
    assert!(store_position < moved_position);
    assert_eq!(
        after
            .insts
            .iter()
            .filter(|inst| inst.op == ir::Opcode::Poll)
            .count(),
        before
            .insts
            .iter()
            .filter(|inst| inst.op == ir::Opcode::Poll)
            .count()
    );

    // Compile-time force affects only guards. A valid 255 trip input cold-
    // replays the original header-entry stack after the one visible store.
    force_deopt_for_test(true);
    let forced = lower(&source, &candidate_plan);
    force_deopt_for_test(false);
    let args = [Value::fixnum(255)];
    reset_watcher(&mut ctx);
    let NativeRun::DeoptAt(resume) = forced.call(&mut ctx as *mut Context as *mut u8, &args) else {
        panic!("forced actual anticipated guard must deopt")
    };
    let _resume = Roots::new(&resume.stack);
    assert_eq!(resume.pc, 3);
    assert_eq!(resume.stack, vec![args[0], Value::fixnum(0)]);
    assert_one_store(&ctx);
    assert_eq!(
        resumed(&mut ctx, &source, &resume).unwrap(),
        Value::fixnum(255)
    );
    assert_eq!(
        ctx.obarray.symbol_value_copied("opt-licm-watch-count"),
        Some(Value::fixnum(1)),
        "replay must not repeat the completed preheader store"
    );
}

#[test]
fn opt_licm_native_scalar_hoist_preserves_compiler_only_cons_across_255_510_gc() {
    let _settings = licm_settings();
    let source = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Pop,
            Op::Constant(2),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Gtr,
            Op::GotoIfNil(28),
            Op::Constant(3),
            Op::Constant(4),
            Op::Add,
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(5),
            Op::Eqlsign,
            Op::GotoIfNotNil(24),
            Op::StackRef(0),
            Op::Constant(7),
            Op::Eqlsign,
            Op::GotoIfNil(27),
            Op::Constant(6),
            Op::Call(0),
            Op::Pop,
            Op::Goto(5),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            Value::fixnum(7),
            Value::fixnum(9),
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(255),
            Value::symbol("garbage-collect"),
            Value::fixnum(510),
        ],
        1,
    );
    let mut ctx = Context::new();
    plain_no_gc(&mut ctx);
    let cell_control = function(
        vec![Op::Constant(0), Op::Constant(1), Op::Cons, Op::Return],
        vec![Value::fixnum(7), Value::fixnum(9)],
        0,
    );
    let cell_expected = tier0(&mut ctx, &cell_control, &[]).unwrap();
    let expected_text = print_value(&cell_expected);
    let _cell_expected = Roots::new(&[cell_expected]);
    let original_plan = selected_reps(licm_prepared(&source));
    let original_native = lower(&source, &original_plan);
    // Validate the unchanged sealed source independently before making the
    // deliberate authoritative-IR-only return select its compiler-only value.
    for n in [255, 510] {
        let start = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        assert_eq!(
            tier0(&mut ctx, &source, &[Value::fixnum(n)]).unwrap(),
            Value::fixnum(n)
        );
        assert_eq!(ctx.gc_count - start.0, (n / 255) as u64);
        assert_eq!(
            ctx.tagged_heap.gc_collections() - start.1,
            (n / 255) as usize
        );
        let start = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        assert_eq!(
            reference_ok(&mut ctx, &original_plan, &[Value::fixnum(n)]),
            Value::fixnum(n)
        );
        assert_eq!(ctx.gc_count - start.0, (n / 255) as u64);
        assert_eq!(
            ctx.tagged_heap.gc_collections() - start.1,
            (n / 255) as usize
        );
        let start = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        reset_bytecode_branch_poll_count();
        assert_eq!(
            native_ok(&mut ctx, &original_native, &[Value::fixnum(n)]),
            Value::fixnum(n)
        );
        assert_eq!(ctx.gc_count - start.0, (n / 255) as u64);
        assert_eq!(
            ctx.tagged_heap.gc_collections() - start.1,
            (n / 255) as usize
        );
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
    }
    let mut before = licm_prepared(&source);
    let cell = before
        .insts
        .iter()
        .find(|inst| matches!(inst.op, ir::Opcode::Opaque(Op::Cons)) && inst.pc == 2)
        .unwrap()
        .result
        .unwrap();
    let invariant = *before.source_states[11]
        .as_ref()
        .unwrap()
        .post
        .last()
        .unwrap();
    let invariant_id = defining_inst(&before, invariant);
    assert_eq!(
        before.insts[invariant_id.index()].op,
        ir::Opcode::FixAdd { checked: false },
        "actual invariant must be range-proven before pure LICM"
    );
    assert_eq!(
        before.insts[invariant_id.index()].eff,
        crate::emacs_core::jit::opt::mem::Effects::PURE
    );
    let old_owner = owner(&before, invariant_id);
    let header = before.source_states[5].as_ref().unwrap().block;
    for block in &mut before.blocks {
        if matches!(block.term, ir::Term::Return(_)) {
            block.term = ir::Term::Return(cell);
        }
    }
    before.verify().unwrap();
    let baseline_plan = selected_reps(before.clone());
    let baseline = lower(&source, &baseline_plan);
    for n in [255, 510] {
        let args = [Value::fixnum(n)];
        let start = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        let expected = reference_ok(&mut ctx, &baseline_plan, &args);
        let _expected = Roots::new(&[expected]);
        assert_eq!(print_value(&expected), expected_text);
        assert_eq!(ctx.gc_count - start.0, (n / 255) as u64);
        assert_eq!(
            ctx.tagged_heap.gc_collections() - start.1,
            (n / 255) as usize
        );
        let start = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        reset_bytecode_branch_poll_count();
        let actual = native_ok(&mut ctx, &baseline, &args);
        let _actual = Roots::new(&[actual]);
        assert_eq!(print_value(&actual), print_value(&expected));
        assert_eq!(ctx.gc_count - start.0, (n / 255) as u64);
        assert_eq!(
            ctx.tagged_heap.gc_collections() - start.1,
            (n / 255) as usize
        );
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
    }
    let mut after = before.clone();
    let stats = licm::run(&mut after).unwrap();
    after.verify().unwrap();
    same_original_observers(&before, &after);
    let candidate_plan = selected_reps(after.clone());
    let candidate = lower(&source, &candidate_plan);
    for n in [255, 510] {
        let args = [Value::fixnum(n)];
        let start = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        reset_bytecode_branch_poll_count();
        let actual = native_ok(&mut ctx, &candidate, &args);
        let _actual = Roots::new(&[actual]);
        assert_eq!(print_value(&actual), expected_text);
        assert_eq!(ctx.gc_count - start.0, (n / 255) as u64);
        assert_eq!(
            ctx.tagged_heap.gc_collections() - start.1,
            (n / 255) as usize
        );
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
        assert!(!ctx.tagged_heap.mark_in_progress());
        assert!(!ctx.tagged_heap.sweep_in_progress());
        assert_eq!(ctx.jit_root_stack_top, 0);
        let start = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        let observed = reference_ok(&mut ctx, &candidate_plan, &args);
        let _observed = Roots::new(&[observed]);
        assert_eq!(print_value(&observed), expected_text);
        assert_eq!(ctx.gc_count - start.0, (n / 255) as u64);
        assert_eq!(
            ctx.tagged_heap.gc_collections() - start.1,
            (n / 255) as usize
        );
    }
    // These are genuine semantic/native paths, not merely safety negatives.
    assert!(stats.pure_hoisted > 0);
    let old = &after.insts[invariant_id.index()];
    assert!(matches!(old.op, ir::Opcode::Refine(_)));
    let moved = defining_inst(&after, old.args[0]);
    assert_ne!(owner(&after, moved), old_owner);
    assert!(matches!(after.blocks[owner(&after, moved).index()].term,
        ir::Term::Jump(ref edge) if edge.target == header));
    for inst in after.insts.iter().filter(|inst| {
        inst.op == ir::Opcode::Poll || matches!(inst.op, ir::Opcode::Opaque(Op::Call(0)))
    }) {
        let frame = &after.frames[inst.frame.unwrap().index()];
        assert!(
            !frame.stack.contains(&cell),
            "compiler-only cons has no GNU-stack/external root"
        );
    }
}
