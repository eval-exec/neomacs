//! Native GVN must preserve GNU mutation, identity, errors and heap roots.
//! Observable flows are captured in tmp/t34-o34-gnu-prep and the separate
//! tmp/t34-o34-poll-gnu-prep. Answers here come from forced Tier-0/reference
//! execution; native baseline runs precede assertions about optimization.
//! Threading: every plan, context, root and override is invocation-owned.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::function;
use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    bytecode_branch_poll_count, push_scratch_gc_roots, reset_bytecode_branch_poll_count,
    restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::{
    build, eval, ir, mem,
    passes::{cfg, fold, gvn},
};
use crate::emacs_core::print::print_value;

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            fold: true,
            gvn: true,
            ..OptPasses::default()
        }));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_deopt_for_test(false);
        force_tail_unrooted_for_test(None);
    }
}

/// Existing scratch roots on the owning mutator; no additional runtime state.
struct Roots(usize);
impl Roots {
    fn new(values: &[Value]) -> Self {
        let saved = save_scratch_gc_roots();
        push_scratch_gc_roots(values);
        Self(saved)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

/// Test-local bytecode assembly tracks actual stack depth, independently of SSA.
struct Program {
    ops: Vec<Op>,
    depth: usize,
}
impl Program {
    fn new(arity: usize) -> Self {
        Self {
            ops: Vec::new(),
            depth: arity,
        }
    }
    fn op(&mut self, op: Op) -> usize {
        let pc = self.ops.len();
        match op {
            Op::StackRef(_) | Op::Constant(_) | Op::VarRef(_) | Op::Nil | Op::True | Op::Dup => {
                self.depth += 1
            }
            Op::Pop | Op::VarSet(_) | Op::StackSet(_) | Op::GotoIfNil(_) => self.depth -= 1,
            Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe | Op::Consp | Op::Sub1 => {}
            Op::Cons | Op::Eq | Op::Setcar | Op::Setcdr | Op::Gtr => self.depth -= 1,
            Op::Call(n) => self.depth -= n as usize,
            Op::List(n) => self.depth = self.depth - n as usize + 1,
            Op::Goto(_) | Op::Return => {}
            _ => panic!("fixture stack effect must be declared: {op:?}"),
        }
        self.ops.push(op);
        pc
    }
    fn copy(&mut self, bottom_slot: usize) {
        self.op(Op::StackRef(
            (self.depth - 1 - bottom_slot).try_into().unwrap(),
        ));
    }
    fn read(&mut self, bottom_slot: usize, cdr: bool) -> usize {
        self.copy(bottom_slot);
        self.op(if cdr { Op::Cdr } else { Op::Car })
    }
    fn finish(mut self, consts: Vec<Value>, arity: usize, outputs: u16) -> ByteCodeFunction {
        self.op(Op::List(outputs));
        self.op(Op::Return);
        function(self.ops, consts, arity)
    }
}

fn prepared(source: &ByteCodeFunction) -> ir::Func {
    let arity = source.params.required.len();
    let cfg = analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let consts = source
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    let mut plan = build::build(build::BuildInput {
        ops: source.executable_ops(),
        constants: &consts,
        cfg: &cfg,
        params: ir::ParamShape {
            required: arity,
            ..ir::ParamShape::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .unwrap();
    fold::run(&mut plan).unwrap();
    cfg::cleanup(&mut plan).unwrap();
    plan.verify().unwrap();
    plan
}

fn lower(source: &ByteCodeFunction, plan: &ir::Func) -> CompiledLeaf {
    plan.verify().expect("GVN native input verifies");
    let leaf = lower_opt_ir_for_test(
        source.executable_ops(),
        &source.constants,
        source.params.required.len(),
        source.executable_gnu_byte_offset_map(),
        plan,
    )
    .expect("baseline/candidate verified GVN IR lowers");
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    leaf
}
fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[Value]) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec())
}
fn reference(ctx: &mut Context, plan: &ir::Func, args: &[Value]) -> Value {
    let run = eval::evaluate(
        plan,
        ctx,
        eval::Inputs {
            args,
            ..eval::Inputs::default()
        },
    )
    .unwrap();
    let eval::Outcome::Returned(value) = run.outcome else {
        panic!("reference return required")
    };
    value.to_value()
}
fn native(ctx: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    match leaf.call(ctx as *mut Context as *mut u8, args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("GVN success fixture must run natively: {other:?}"),
    }
}
fn check<F: FnMut(&mut Context) -> Vec<Value>>(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    plan: &ir::Func,
    leaf: &CompiledLeaf,
    mut args: F,
) {
    let a = args(ctx);
    let _roots = Roots::new(&a);
    let expected = print_value(&tier0(ctx, source, &a).unwrap());
    let b = args(ctx);
    let _roots = Roots::new(&b);
    assert_eq!(print_value(&reference(ctx, plan, &b)), expected);
    let c = args(ctx);
    let _roots = Roots::new(&c);
    assert_eq!(print_value(&native(ctx, leaf, &c)), expected);
    assert_eq!(ctx.jit_root_stack_top, 0);
}
fn count(plan: &ir::Func, wanted: &ir::Opcode) -> usize {
    plan.blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter(|inst| &plan.insts[inst.index()].op == wanted)
        .count()
}
fn result_at(plan: &ir::Func, pc: usize) -> ir::Value {
    plan.insts
        .iter()
        .find(|inst| {
            inst.pc == pc as u32
                && inst.result.is_some()
                && matches!(inst.op, ir::Opcode::LoadCar | ir::Opcode::LoadCdr)
        })
        .expect("typed load at source pc")
        .result
        .unwrap()
}
fn children() -> [Value; 3] {
    [
        Value::list(vec![Value::symbol("car-child")]),
        Value::list(vec![Value::symbol("cdr-child")]),
        Value::list(vec![Value::symbol("new-child")]),
    ]
}

#[test]
fn opt_gvn_native_repeated_cons_fields_preserve_heap_identity() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, _] = children();
    let _roots = Roots::new(&[a, d]);
    // GNU repeated-cons: four reads and two identity comparisons, with a
    // genuinely allocated cons giving fold its declared CONS proof.
    let mut p = Program::new(2);
    p.copy(0);
    p.copy(1);
    p.op(Op::Cons);
    for cdr in [false, false, true, true] {
        p.read(2, cdr);
    }
    for cdr in [false, true] {
        p.read(2, cdr);
        p.read(2, cdr);
        p.op(Op::Eq);
    }
    let source = p.finish(vec![], 2, 6);
    let before = prepared(&source);
    assert_eq!(count(&before, &ir::Opcode::LoadCar), 4);
    assert_eq!(count(&before, &ir::Opcode::LoadCdr), 4);
    let baseline = lower(&source, &before);
    check(&mut ctx, &source, &before, &baseline, |_| vec![a, d]);
    let mut after = before.clone();
    let stats = gvn::run(&mut after).unwrap();
    let candidate = lower(&source, &after);
    check(&mut ctx, &source, &after, &candidate, |_| vec![a, d]);
    assert!(
        stats.load_reuses >= 2,
        "both mutable field loads actually reuse"
    );
    assert!(count(&after, &ir::Opcode::LoadCar) < count(&before, &ir::Opcode::LoadCar));
    assert!(count(&after, &ir::Opcode::LoadCdr) < count(&before, &ir::Opcode::LoadCdr));
}

fn store_source(cdr: bool) -> ByteCodeFunction {
    let mut p = Program::new(2); // p,x
    p.read(0, false);
    p.read(0, true); // old a,d remain observable
    p.copy(0);
    p.copy(1);
    p.op(if cdr { Op::Setcdr } else { Op::Setcar });
    p.read(0, cdr);
    p.read(0, cdr);
    p.read(0, !cdr);
    p.read(0, cdr);
    p.copy(1);
    p.op(Op::Eq);
    p.read(0, !cdr);
    p.copy(if cdr { 2 } else { 3 });
    p.op(Op::Eq);
    p.finish(vec![], 2, 8)
}

#[test]
fn opt_gvn_native_retained_stores_forward_with_aliases_and_barrier() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, x] = children();
    let _roots = Roots::new(&[a, d, x]);
    // Frozen store-car-same-base/store-cdr-same-base; the opposite field and
    // setter return identity remain observable. Store helper/barrier survives.
    for cdr in [false, true] {
        let source = store_source(cdr);
        let before = prepared(&source);
        let setter = ir::Opcode::Opaque(if cdr { Op::Setcdr } else { Op::Setcar });
        assert_eq!(count(&before, &setter), 1);
        let baseline = lower(&source, &before);
        check(&mut ctx, &source, &before, &baseline, |_| {
            vec![Value::cons(a, d), x]
        });
        let mut after = before.clone();
        let stats = gvn::run(&mut after).unwrap();
        assert_eq!(
            count(&after, &setter),
            1,
            "the observable setter is retained"
        );
        let store = after.insts.iter().find(|inst| inst.op == setter).unwrap();
        assert!(store.eff.intersects(mem::Effects::WRITE_HEAP));
        assert_eq!(
            store.mem,
            if cdr {
                mem::AliasClass::ConsCdr
            } else {
                mem::AliasClass::ConsCar
            }
        );
        let candidate = lower(&source, &after);
        check(&mut ctx, &source, &after, &candidate, |_| {
            vec![Value::cons(a, d), x]
        });
        for leaf in [&baseline, &candidate] {
            let cell = Value::cons(a, d);
            let _roots = Roots::new(&[cell]);
            let tracking = ctx.tagged_heap.write_tracking_mode();
            ctx.tagged_heap
                .set_write_tracking_mode(crate::tagged::gc::WriteTrackingMode::OwnersAndRecords);
            ctx.tagged_heap.clear_dirty_writes();
            let revision = crate::tagged::mutate::LispCollectionRevision::current();
            let shim_calls = super::dispatch::LIST_STORE_SHIM_CALLS.with(|calls| calls.get());
            native(&mut ctx, leaf, &[cell, x]);
            let actual_shim_calls =
                super::dispatch::LIST_STORE_SHIM_CALLS.with(|calls| calls.get());
            let dirty_owner = ctx.tagged_heap.is_dirty_owner(cell);
            // Restoring Disabled deliberately clears both trackers. Preserve
            // their evidence before restoring the prior heap configuration.
            let writes = ctx.tagged_heap.dirty_writes().to_vec();
            ctx.tagged_heap.set_write_tracking_mode(tracking);
            assert_ne!(
                crate::tagged::mutate::LispCollectionRevision::current(),
                revision
            );
            assert_eq!(
                actual_shim_calls - shim_calls,
                1,
                "the covering barrier window sends the retained setter to its shared shim"
            );
            assert!(
                dirty_owner,
                "the shared barrier records the retained setter's owner"
            );
            let kind = if cdr {
                crate::tagged::gc::HeapWriteKind::ConsCdr
            } else {
                crate::tagged::gc::HeapWriteKind::ConsCar
            };
            assert!(
                writes.iter().any(|write| write.owner.bits() == cell.bits()
                    && write.kind == kind
                    && write.slot == Some(usize::from(cdr))
                    && write.value.is_some_and(|value| value.bits() == x.bits())),
                "retained setter reaches the real selected-field barrier"
            );
        }
        assert!(stats.store_forwards > 0, "post-store load really forwards");
    }
}

#[test]
fn opt_gvn_native_one_arm_write_kills_predecessor_load_at_join() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, x] = children();
    let _roots = Roots::new(&[a, d, x]);
    // Frozen diamond-{car,cdr}-{t,nil}, on a known CONS allocation.
    for cdr in [false, true] {
        let mut p = Program::new(4); // a,d,x,choose
        p.copy(0);
        p.copy(1);
        p.op(Op::Cons); // p at4
        let old = p.read(4, cdr);
        p.read(4, !cdr);
        p.copy(3);
        let branch = p.op(Op::GotoIfNil(0));
        p.copy(4);
        p.copy(2);
        p.op(if cdr { Op::Setcdr } else { Op::Setcar });
        p.op(Op::Pop);
        let join = p.ops.len() as u32;
        p.ops[branch] = Op::GotoIfNil(join);
        let first_join_load = p.read(4, cdr);
        p.read(4, !cdr);
        p.read(4, cdr);
        p.read(4, !cdr);
        let source = p.finish(vec![], 4, 6);
        let before = prepared(&source);
        let old_value = result_at(&before, old);
        let joined = result_at(&before, first_join_load);
        let baseline = lower(&source, &before);
        for choose in [Value::NIL, Value::T] {
            check(&mut ctx, &source, &before, &baseline, |_| {
                vec![a, d, x, choose]
            });
        }
        let mut after = before.clone();
        let stats = gvn::run(&mut after).unwrap();
        assert_ne!(
            after.resolve(joined),
            after.resolve(old_value),
            "a store on one incoming arm kills the dominating stale field"
        );
        let candidate = lower(&source, &after);
        for choose in [Value::NIL, Value::T] {
            check(&mut ctx, &source, &after, &candidate, |_| {
                vec![a, d, x, choose]
            });
        }
        assert!(
            stats.load_reuses > 0,
            "repeated loads after the join still optimize"
        );
    }
}

fn call_source(callback: Value, gc: bool) -> ByteCodeFunction {
    let mut p = Program::new(3); // a,d,x
    p.copy(0);
    p.copy(1);
    p.op(Op::Cons);
    if gc {
        p.copy(3);
        p.op(Op::VarSet(1));
    }
    p.read(3, false);
    p.read(3, true);
    p.op(Op::Constant(0));
    if !gc {
        p.copy(3);
        p.copy(2);
    }
    p.op(Op::Call(if gc { 0 } else { 2 }));
    if gc {
        p.op(Op::Pop);
    }
    p.read(3, false);
    p.read(3, true);
    if gc {
        p.op(Op::VarRef(2));
    }
    p.finish(
        vec![
            callback,
            Value::symbol("t34-o34-gc-cell"),
            Value::symbol("t34-o34-gc-count"),
        ],
        3,
        5,
    )
}

fn install_gc_hook(ctx: &mut Context) {
    ctx.eval_str(
        "(setq t34-o34-gc-count 0 t34-o34-gc-cell nil t34-o34-gc-new nil)
        (fset 't34-o34-gc-hook '(lambda ()
           (setq t34-o34-gc-count (1+ t34-o34-gc-count))
           (setcar t34-o34-gc-cell t34-o34-gc-new)
           (setcdr t34-o34-gc-cell 'gc-cdr)))
        (setq post-gc-hook '(t34-o34-gc-hook))",
    )
    .unwrap();
}

fn poll_source() -> ByteCodeFunction {
    let mut p = Program::new(2); // p,n; only run on real cons cells
    p.copy(0);
    p.op(Op::Consp);
    let not_cons = p.op(Op::GotoIfNil(0));
    p.read(0, false);
    p.read(0, true); // p,n,a,d
    let header = p.ops.len() as u32;
    p.copy(1);
    p.op(Op::Constant(0));
    p.op(Op::Gtr);
    let done = p.op(Op::GotoIfNil(0));
    // Cons allocation is non-collecting. It makes the exact GC stress request
    // due at the existing cold Poll, without a mutating Call in this loop.
    p.op(Op::Nil);
    p.op(Op::Nil);
    p.op(Op::Cons);
    p.op(Op::Pop);
    p.copy(1);
    p.op(Op::Sub1);
    p.op(Op::StackSet(3));
    p.op(Op::Goto(header));
    let exit = p.ops.len() as u32;
    p.ops[done] = Op::GotoIfNil(exit);
    p.read(0, false);
    p.read(0, true);
    p.op(Op::VarRef(1));
    p.op(Op::List(5));
    p.op(Op::Return);
    let nil = p.ops.len() as u32;
    p.ops[not_cons] = Op::GotoIfNil(nil);
    p.depth = 2;
    p.op(Op::Nil);
    p.op(Op::Return);
    function(
        p.ops,
        vec![Value::fixnum(0), Value::symbol("t34-o34-gc-count")],
        2,
    )
}

#[test]
fn opt_gvn_native_call_gc_and_poll_invalidate_mutable_fields() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, x] = children();
    let _roots = Roots::new(&[a, d, x]);
    ctx.eval_str(
        "(fset 't34-o34-call-car '(lambda (p x) (setcar p x)))
        (fset 't34-o34-call-cdr '(lambda (p x) (setcdr p x)))",
    )
    .unwrap();
    // Frozen call-mutates-car/cdr and gc-hook-mutates-loads.
    for callback in [
        Value::symbol("t34-o34-call-car"),
        Value::symbol("t34-o34-call-cdr"),
    ] {
        let source = call_source(callback, false);
        let before = prepared(&source);
        let baseline = lower(&source, &before);
        check(&mut ctx, &source, &before, &baseline, |_| vec![a, d, x]);
        let mut after = before.clone();
        gvn::run(&mut after).unwrap();
        let candidate = lower(&source, &after);
        check(&mut ctx, &source, &after, &candidate, |_| vec![a, d, x]);
        assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Call(2))), 1);
    }
    install_gc_hook(&mut ctx);
    let source = call_source(Value::symbol("garbage-collect"), true);
    let before = prepared(&source);
    let baseline = lower(&source, &before);
    let setup = |ctx: &mut Context| {
        ctx.obarray
            .set_symbol_value("t34-o34-gc-count", Value::fixnum(0));
        ctx.obarray.set_symbol_value("t34-o34-gc-new", x);
        vec![a, d, x]
    };
    check(&mut ctx, &source, &before, &baseline, setup);
    let mut after = before.clone();
    gvn::run(&mut after).unwrap();
    let candidate = lower(&source, &after);
    check(&mut ctx, &source, &after, &candidate, setup);
    assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Call(0))), 1);

    // Supplementary GNU explicit-GC-every-255 oracle, independently exercising
    // the native automatic Poll cadence and real post-GC mutation hook.
    let source = poll_source();
    let before = prepared(&source);
    assert!(count(&before, &ir::Opcode::Poll) > 0);
    let baseline = lower(&source, &before);
    let mut after = before.clone();
    gvn::run(&mut after).unwrap();
    assert_eq!(
        count(&after, &ir::Opcode::Poll),
        count(&before, &ir::Opcode::Poll)
    );
    let candidate = lower(&source, &after);
    for n in [0, 254, 255, 510] {
        let setup = |ctx: &mut Context| {
            let cell = Value::cons(a, d);
            ctx.obarray.set_symbol_value("t34-o34-gc-cell", cell);
            ctx.obarray.set_symbol_value("t34-o34-gc-new", x);
            ctx.obarray
                .set_symbol_value("t34-o34-gc-count", Value::fixnum(0));
            ctx.tagged_heap.reset_bytes_since_gc();
            ctx.gc_stress = true;
            vec![cell, Value::fixnum(n)]
        };
        for (plan, leaf) in [(&before, &baseline), (&after, &candidate)] {
            let args = setup(&mut ctx);
            let _roots = Roots::new(&args);
            reset_bytecode_branch_poll_count();
            let expected = print_value(&tier0(&mut ctx, &source, &args).unwrap());
            assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
            let args = setup(&mut ctx);
            let _roots = Roots::new(&args);
            reset_bytecode_branch_poll_count();
            assert_eq!(print_value(&reference(&mut ctx, plan, &args)), expected);
            assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
            let args = setup(&mut ctx);
            let _roots = Roots::new(&args);
            reset_bytecode_branch_poll_count();
            let actual = native(&mut ctx, leaf, &args);
            ctx.gc_stress = false;
            assert_eq!(print_value(&actual), expected);
            assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
            assert_eq!(
                ctx.obarray.symbol_value("t34-o34-gc-count").copied(),
                Some(Value::fixnum(n / 255))
            );
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
    }
    ctx.obarray.set_symbol_value("post-gc-hook", Value::NIL);
}

#[test]
fn opt_gvn_native_eliminated_load_alias_roots_heap_only_across_gc() {
    let _settings = Settings::enter();
    force_tail_unrooted_for_test(Some(true));
    let mut ctx = Context::new();
    // The inner child is created natively, not supplied or scratch-rooted.
    // Both source-stack reads are dropped before GC; only authoritative SSA
    // Return keeps the repeated load live. This is compiler-liveness evidence.
    let source = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Nil,
            Op::Cons,
            Op::Dup,
            Op::Car,
            Op::StackRef(1),
            Op::Car,
            Op::Pop,
            Op::Pop,
            Op::Pop,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Nil,
            Op::Return,
        ],
        vec![
            Value::fixnum(7),
            Value::fixnum(9),
            Value::symbol("garbage-collect"),
        ],
        0,
    );
    let mut before = prepared(&source);
    let first = result_at(&before, 6);
    let second = result_at(&before, 8);
    for block in &mut before.blocks {
        if matches!(block.term, ir::Term::Return(_)) {
            block.term = ir::Term::Return(second);
        }
    }
    before.verify().unwrap();
    let call = before
        .insts
        .iter()
        .find(|inst| inst.op == ir::Opcode::Opaque(Op::Call(0)))
        .unwrap();
    let original_stack = &before.frames[call.frame.unwrap().index()].stack;
    assert!(!original_stack.contains(&first) && !original_stack.contains(&second));
    // Ordinary source-equivalent execution derives the returned child's GNU
    // meaning; unlike the authoritative IR it keeps a GNU operand-stack root.
    let oracle = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Return,
        ],
        source.constants.to_vec(),
        0,
    );
    let expected = print_value(&tier0(&mut ctx, &oracle, &[]).unwrap());
    assert_eq!(print_value(&reference(&mut ctx, &before, &[])), expected);
    let baseline = lower(&source, &before);
    let collections = ctx.tagged_heap.gc_collections();
    assert_eq!(print_value(&native(&mut ctx, &baseline, &[])), expected);
    assert!(ctx.tagged_heap.gc_collections() > collections);
    let mut after = before.clone();
    let stats = gvn::run(&mut after).unwrap();
    let candidate = lower(&source, &after);
    let collections = ctx.tagged_heap.gc_collections();
    assert_eq!(print_value(&reference(&mut ctx, &after, &[])), expected);
    assert_eq!(print_value(&native(&mut ctx, &candidate, &[])), expected);
    assert!(ctx.tagged_heap.gc_collections() > collections);
    assert_eq!(ctx.jit_root_stack_top, 0);
    assert!(
        stats.load_reuses > 0,
        "compiler-only live heap load must really alias"
    );
    let ir::ValueDef::Inst(inst) = after.values[second.index()].def else {
        panic!("reused result retains its original SSA instruction identity")
    };
    assert!(matches!(
        after.insts[inst.index()].op,
        ir::Opcode::Refine(_)
    ));
    assert_eq!(after.insts[inst.index()].args, vec![first]);
}

fn failure_source(cdr: bool, setter_failure: bool) -> ByteCodeFunction {
    let mut p = Program::new(4); // p,x,bad,payload
    p.copy(0);
    p.copy(1);
    p.op(Op::Setcar);
    p.op(Op::Pop);
    p.op(Op::Constant(1));
    p.copy(0);
    p.read(0, false);
    p.copy(3);
    p.op(Op::List(4));
    p.op(Op::VarSet(0));
    p.read(0, false);
    if setter_failure {
        p.copy(2);
        p.copy(1);
        p.op(if cdr { Op::Setcdr } else { Op::Setcar });
    } else {
        p.read(2, cdr);
    }
    p.read(0, true);
    p.copy(3);
    p.finish(
        vec![Value::symbol("t34-o34-side"), Value::symbol("stored")],
        4,
        4,
    )
}

fn signal_text(flow: &Flow) -> (String, String) {
    let signal = flow.as_signal().expect("GNU wrong-type signal");
    (
        signal.symbol_name().to_string(),
        print_value(&Value::list(signal.data.clone())),
    )
}

#[test]
fn opt_gvn_native_errors_nil_and_identity_keep_gnu_observations() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, x] = children();
    let payload = Value::list(vec![Value::symbol("heap-companion")]);
    let _roots = Roots::new(&[a, d, x, payload]);
    ctx.obarray.set_symbol_value("t34-o34-side", Value::NIL);
    // Frozen direct-mutation-error-frame/setcar/setcdr nil/symbol outcomes.
    for setter_failure in [false, true] {
        for cdr in [false, true] {
            let source = failure_source(cdr, setter_failure);
            let function_value = Value::make_bytecode(source.clone());
            ctx.obarray.set_symbol_function_id(
                crate::emacs_core::intern::intern(if setter_failure {
                    "t34-o34-invalid-store"
                } else {
                    "t34-o34-store-fail"
                }),
                function_value,
            );
            let _function_root = Roots::new(&[function_value]);
            let before = prepared(&source);
            let baseline = lower(&source, &before);
            check(&mut ctx, &source, &before, &baseline, |_| {
                vec![Value::cons(a, d), x, Value::cons(a, d), payload]
            });
            let mut after = before.clone();
            gvn::run(&mut after).unwrap();
            let candidate = lower(&source, &after);
            check(&mut ctx, &source, &after, &candidate, |_| {
                vec![Value::cons(a, d), x, Value::cons(a, d), payload]
            });
            for bad in [Value::NIL, Value::symbol("wrong")] {
                if !setter_failure && bad.is_nil() {
                    continue;
                }
                let args = [Value::cons(a, d), x, bad, payload];
                let _roots = Roots::new(&args);
                let expected = tier0(&mut ctx, &source, &args).unwrap_err();
                let expected = signal_text(&expected);
                for (plan, leaf) in [(&before, &baseline), (&after, &candidate)] {
                    let args = [Value::cons(a, d), x, bad, payload];
                    let _roots = Roots::new(&args);
                    ctx.obarray.set_symbol_value("t34-o34-side", Value::NIL);
                    let run = eval::evaluate(
                        plan,
                        &mut ctx,
                        eval::Inputs {
                            args: &args,
                            ..eval::Inputs::default()
                        },
                    )
                    .unwrap();
                    let eval::Outcome::Deopt(snapshot) = run.outcome else {
                        panic!("original GNU guard and source snapshot are retained")
                    };
                    ctx.obarray.set_symbol_value("t34-o34-side", Value::NIL);
                    let NativeRun::DeoptAt(resume) =
                        leaf.call(&mut ctx as *mut Context as *mut u8, &args)
                    else {
                        panic!("precise native guard failure after visible store")
                    };
                    assert_eq!(resume.pc as u32, snapshot.pc);
                    assert_eq!(
                        resume
                            .stack
                            .iter()
                            .copied()
                            .map(ir::ValueBits::from_value)
                            .collect::<Vec<_>>(),
                        snapshot.stack
                    );
                    assert_eq!(args[0].cons_car(), x);
                    let side = ctx.obarray.symbol_value("t34-o34-side").copied().unwrap();
                    assert_eq!(side.cons_cdr().cons_cdr().cons_car(), x);
                    assert_eq!(side.cons_cdr().cons_cdr().cons_cdr().cons_car(), payload);
                    let mut vm = Vm::from_context(&mut ctx);
                    vm.force_interpreter_only_for_test();
                    let flow = vm
                        .run_resumed_frame(
                            &source,
                            function_value,
                            resume.pc,
                            &resume.stack,
                            resume.handlers,
                            &resume.binds,
                            resume.spec_base,
                            resume.cond_base,
                        )
                        .expect_err("Tier-0 resumes original GNU type error");
                    assert_eq!(signal_text(&flow), expected);
                }
            }
        }
    }
    // Car/cdr nil remains legal, with exact noncons listp failures tested above.
    for cdr in [false, true] {
        let source = function(
            vec![
                Op::StackRef(0),
                if cdr { Op::Cdr } else { Op::Car },
                Op::Return,
            ],
            vec![],
            1,
        );
        let before = prepared(&source);
        let baseline = lower(&source, &before);
        check(&mut ctx, &source, &before, &baseline, |_| vec![Value::NIL]);
        let mut after = before.clone();
        gvn::run(&mut after).unwrap();
        let candidate = lower(&source, &after);
        check(&mut ctx, &source, &after, &candidate, |_| vec![Value::NIL]);
    }
    // Frozen O32 independent float/string Eq and O34 fresh-allocation identity.
    // GVN may reuse scalar constants, but cannot merge allocations or implement
    // heap Eq by comparing contents, numeric values, or guessed type tags.
    let mut p = Program::new(4); // f0,f1,s0,s1, supplied distinct objects
    p.copy(0);
    p.copy(1);
    p.op(Op::Eq);
    p.copy(2);
    p.copy(3);
    p.op(Op::Eq);
    for _ in 0..2 {
        p.op(Op::Constant(0));
        p.op(Op::Constant(1));
        p.op(Op::Cons);
    }
    p.op(Op::Eq);
    let source = p.finish(vec![Value::fixnum(7), Value::fixnum(9)], 4, 3);
    let before = prepared(&source);
    let baseline = lower(&source, &before);
    let make = |_: &mut Context| {
        vec![
            Value::make_float(1.25),
            Value::make_float(1.25),
            Value::string("equal-text"),
            Value::string("equal-text"),
        ]
    };
    check(&mut ctx, &source, &before, &baseline, make);
    let mut after = before.clone();
    gvn::run(&mut after).unwrap();
    assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Cons)), 2);
    assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Eq)), 3);
    let candidate = lower(&source, &after);
    check(&mut ctx, &source, &after, &candidate, make);
}

// GNU sources: src/lisp.h CAR/CDR, data.c Fcar/Fcdr; frozen O34 stdout rows
// repeated-nil/repeated-cons/store-car-same-base/direct-mutation-error-frame.
// Threading: all plans/overrides/roots are invocation-owned compiler scratch.

use crate::emacs_core::jit::opt::types::TypeSet as ListTypeSet;

/// Compare every source mapping field without adding equality to production IR.
fn list_source_states_equal(before: &ir::Func, after: &ir::Func) {
    assert_eq!(before.source_states.len(), after.source_states.len());
    for (old, new) in before.source_states.iter().zip(&after.source_states) {
        match (old, new) {
            (None, None) => {}
            (Some(old), Some(new)) => {
                let ir::SourceState {
                    pre,
                    post,
                    frame,
                    block,
                } = old;
                assert_eq!(&new.pre, pre);
                assert_eq!(&new.post, post);
                assert_eq!(&new.frame, frame);
                assert_eq!(&new.block, block);
            }
            _ => panic!("source mapping presence changed"),
        }
    }
}

/// Make the current source's already-guarded LIST read explicitly typed. This
/// does not invoke the missing optimization, invent a guard, or change a frame.
fn list_read_plan_for_native(source: &ByteCodeFunction) -> ir::Func {
    let mut plan = prepared(source);
    let mut changed = 0;
    for inst in &mut plan.insts {
        let (opcode, field) = match inst.op {
            ir::Opcode::Opaque(Op::Car) => (ir::Opcode::LoadCar, mem::AliasClass::ConsCar),
            ir::Opcode::Opaque(Op::Cdr) => (ir::Opcode::LoadCdr, mem::AliasClass::ConsCdr),
            _ => continue,
        };
        let input = plan.values[inst.args[0].index()].ty;
        assert!(!input.is_bottom() && input.is_subset(ListTypeSet::LIST));
        assert_eq!(inst.mem, field);
        assert_eq!(
            inst.eff,
            mem::Effects::READ_HEAP
                .with(mem::Effects::MAY_SIGNAL)
                .with(mem::Effects::MAY_DEOPT)
        );
        inst.op = opcode;
        inst.eff = mem::Effects::READ_HEAP;
        changed += 1;
    }
    assert!(
        changed > 0,
        "fixture contains an actual guarded LIST source read"
    );
    plan.verify().unwrap();
    plan
}

#[test]
fn opt_gvn_native_typed_list_reads_preserve_nil_and_cons_identity() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, _] = children();
    let _roots = Roots::new(&[a, d]);
    for cdr in [false, true] {
        let source = function(
            vec![
                Op::StackRef(0),
                if cdr { Op::Cdr } else { Op::Car },
                Op::Return,
            ],
            vec![],
            1,
        );
        let before = prepared(&source);
        let baseline = lower(&source, &before);
        for nil in [false, true] {
            check(&mut ctx, &source, &before, &baseline, |_| {
                vec![if nil { Value::NIL } else { Value::cons(a, d) }]
            });
        }
        let after = list_read_plan_for_native(&source);
        assert_eq!(after.frames, before.frames);
        list_source_states_equal(&before, &after);
        assert_eq!(count(&after, &ir::Opcode::CheckType(ListTypeSet::LIST)), 1);
        // Reference already accepts LIST, including nil. Baseline success
        // above precedes the genuine missing native cons-read-proof refusal.
        assert_eq!(reference(&mut ctx, &after, &[Value::NIL]), Value::NIL);
        force_opt_passes_for_test(Some(OptPasses {
            gvn: true,
            ..OptPasses::default()
        }));
        let candidate = lower(&source, &after);
        force_opt_passes_for_test(Some(OptPasses {
            fold: true,
            gvn: true,
            ..OptPasses::default()
        }));
        for nil in [false, true] {
            check(&mut ctx, &source, &after, &candidate, |_| {
                vec![if nil { Value::NIL } else { Value::cons(a, d) }]
            });
        }
        let args = [Value::symbol("wrong")];
        let _roots = Roots::new(&args);
        let run = eval::evaluate(
            &after,
            &mut ctx,
            eval::Inputs {
                args: &args,
                ..eval::Inputs::default()
            },
        )
        .unwrap();
        let eval::Outcome::Deopt(snapshot) = run.outcome else {
            panic!("non-list still fails the original source guard")
        };
        let NativeRun::DeoptAt(resume) = candidate.call(&mut ctx as *mut Context as *mut u8, &args)
        else {
            panic!("typed LIST load cannot absorb non-list guard failure")
        };
        assert_eq!(resume.pc as u32, snapshot.pc);
        assert_eq!(
            resume
                .stack
                .iter()
                .copied()
                .map(ir::ValueBits::from_value)
                .collect::<Vec<_>>(),
            snapshot.stack
        );
    }
}

#[test]
fn opt_gvn_native_guarded_list_sources_reuse_without_prefix_change() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, _] = children();
    let _roots = Roots::new(&[a, d]);
    let mut p = Program::new(1);
    for cdr in [false, false, true, true] {
        p.read(0, cdr);
    }
    for cdr in [false, true] {
        p.read(0, cdr);
        p.read(0, cdr);
        p.op(Op::Eq);
    }
    let source = p.finish(vec![], 1, 6);
    let before = prepared(&source);
    assert_eq!(count(&before, &ir::Opcode::LoadCar), 0);
    assert_eq!(count(&before, &ir::Opcode::LoadCdr), 0);
    assert_eq!(count(&before, &ir::Opcode::Opaque(Op::Car)), 4);
    assert_eq!(count(&before, &ir::Opcode::Opaque(Op::Cdr)), 4);
    let baseline = lower(&source, &before);
    for nil in [false, true] {
        check(&mut ctx, &source, &before, &baseline, |_| {
            vec![if nil { Value::NIL } else { Value::cons(a, d) }]
        });
    }
    let mut after = before.clone();
    let stats = gvn::run(&mut after).unwrap();
    assert_eq!(after.frames, before.frames);
    list_source_states_equal(&before, &after);
    // This assertion is an actual missing selected-pass transformation. The
    // fold-only prepared prefix above must remain opaque and unchanged.
    assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Car)), 0);
    assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Cdr)), 0);
    let candidate = lower(&source, &after);
    for nil in [false, true] {
        check(&mut ctx, &source, &after, &candidate, |_| {
            vec![if nil { Value::NIL } else { Value::cons(a, d) }]
        });
    }
    assert!(
        stats.load_reuses >= 2,
        "nil-aware selected reads actually reuse"
    );
}

#[test]
fn opt_gvn_native_list_bridge_retains_setcar_and_reuses_other_field() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, x] = children();
    let _roots = Roots::new(&[a, d, x]);
    // Real normal bubble shape: known CONS Cdr; LIST guard+Car on its tail;
    // retained Setcar on the outer cons; same-base Cdr again. The old Cdr
    // remains valid across the Car and opposite-field store, including nil.
    let mut p = Program::new(3); // a,tail,x
    p.copy(0);
    p.copy(1);
    p.op(Op::Cons); // outer at3
    let first_pc = p.read(3, true); // old tail at4
    p.read(4, false); // Car tail, nil allowed, result at5
    p.copy(3);
    p.copy(2);
    p.op(Op::Setcar); // setter result at6
    let second_pc = p.read(3, true); // unchanged tail at7
    p.copy(4);
    p.copy(7);
    p.op(Op::Eq);
    let source = p.finish(vec![], 3, 5);
    let before = prepared(&source);
    let first = result_at(&before, first_pc);
    let second = result_at(&before, second_pc);
    let baseline = lower(&source, &before);
    for nil in [false, true] {
        check(&mut ctx, &source, &before, &baseline, |_| {
            vec![a, if nil { Value::NIL } else { Value::cons(a, d) }, x]
        });
    }
    let mut after = before.clone();
    let stats = gvn::run(&mut after).unwrap();
    assert_eq!(after.frames, before.frames);
    list_source_states_equal(&before, &after);
    assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Setcar)), 1);
    assert_eq!(count(&after, &ir::Opcode::Opaque(Op::Car)), 0);
    let candidate = lower(&source, &after);
    for nil in [false, true] {
        check(&mut ctx, &source, &after, &candidate, |_| {
            vec![a, if nil { Value::NIL } else { Value::cons(a, d) }, x]
        });
    }
    let ir::ValueDef::Inst(id) = after.values[second.index()].def else {
        panic!("reused source result retains its instruction ID")
    };
    assert!(matches!(after.insts[id.index()].op, ir::Opcode::Refine(_)));
    assert_eq!(after.insts[id.index()].args, vec![first]);
    assert!(stats.load_reuses > 0);
    assert_eq!(
        stats.store_forwards, 0,
        "Setcar cannot forward the Cdr field"
    );
    let tracking = ctx.tagged_heap.write_tracking_mode();
    ctx.tagged_heap
        .set_write_tracking_mode(crate::tagged::gc::WriteTrackingMode::OwnersAndRecords);
    ctx.tagged_heap.clear_dirty_writes();
    let shims = super::dispatch::LIST_STORE_SHIM_CALLS.with(|v| v.get());
    native(&mut ctx, &candidate, &[a, Value::NIL, x]);
    let shim_delta = super::dispatch::LIST_STORE_SHIM_CALLS.with(|v| v.get()) - shims;
    let writes = ctx.tagged_heap.dirty_writes().to_vec();
    ctx.tagged_heap.set_write_tracking_mode(tracking);
    assert_eq!(
        shim_delta, 1,
        "the selected setter still uses the shared barrier"
    );
    assert!(
        writes
            .iter()
            .any(|w| w.kind == crate::tagged::gc::HeapWriteKind::ConsCar
                && w.slot == Some(0)
                && w.value.is_some_and(|v| v.bits() == x.bits()))
    );
}

// Regression is actual duplicate native guard lowering, not missing GVN IR.
// GNU grounding: frozen O34 repeated-nil/repeated-cons/store-fail observations.
// Baseline/candidate run every observable case before the CLIF assertion.
// Threading: plans/roots/capture paths are invocation-owned compiler scratch.

/// Mask ONLY positive canonical x86-64 userspace pointer immediates. The
/// fixture has no mathematical or tagged literal in this range. Preserve all
/// smaller constants, tag masks, PCs, opcodes, branches and SSA identities.
fn coemit_mask_pointer_immediates(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("0x") {
        output.push_str(&rest[..at]);
        let suffix = &rest[at + 2..];
        let end = suffix
            .find(|c: char| !(c.is_ascii_hexdigit() || c == '_'))
            .unwrap_or(suffix.len());
        let token = &suffix[..end];
        let value = u64::from_str_radix(&token.replace('_', ""), 16).ok();
        if value.is_some_and(|v| (0x100_0000_0000..0x8000_0000_0000).contains(&v)) {
            output.push_str("0xPOINTER");
        } else {
            output.push_str("0x");
            output.push_str(token);
        }
        rest = &suffix[end..];
    }
    output.push_str(rest);
    output
}

fn coemit_after_store_source(cdr: bool) -> (ByteCodeFunction, usize) {
    let mut p = Program::new(4); // p,x,bad,payload
    p.copy(0);
    p.copy(1);
    p.op(Op::Setcar);
    p.op(Op::Pop);
    p.op(Op::Constant(0));
    p.copy(0);
    p.read(0, false);
    p.copy(3);
    p.op(Op::List(4));
    p.op(Op::VarSet(1));
    let target = p.read(2, cdr);
    p.read(0, false);
    p.copy(3);
    (
        p.finish(
            vec![Value::symbol("stored"), Value::symbol("t34-o34-side")],
            4,
            3,
        ),
        target,
    )
}

fn coemit_typed_target(before: &ir::Func, pc: usize, cdr: bool) -> ir::Func {
    let mut after = before.clone();
    let id = after
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .copied()
        .find(|id| {
            after.insts[id.index()].pc == pc as u32
                && after.insts[id.index()].op
                    == ir::Opcode::Opaque(if cdr { Op::Cdr } else { Op::Car })
        })
        .expect("source actually contains the selected opaque accessor");
    let inst = &after.insts[id.index()];
    assert_eq!(after.values[inst.args[0].index()].ty, ListTypeSet::LIST);
    let owner = after.blocks.iter().find(|b| b.insts.contains(&id)).unwrap();
    let position = owner
        .insts
        .iter()
        .position(|candidate| *candidate == id)
        .unwrap();
    let guard = &after.insts[owner.insts[position.checked_sub(1).unwrap()].index()];
    assert_eq!(guard.op, ir::Opcode::CheckType(ListTypeSet::LIST));
    assert_eq!(guard.result, Some(inst.args[0]));
    assert_eq!(guard.frame, inst.frame);
    assert_eq!(guard.pc, inst.pc);
    assert_eq!(
        inst.eff,
        mem::Effects::READ_HEAP
            .with(mem::Effects::MAY_SIGNAL)
            .with(mem::Effects::MAY_DEOPT)
    );
    after.insts[id.index()].op = if cdr {
        ir::Opcode::LoadCdr
    } else {
        ir::Opcode::LoadCar
    };
    after.insts[id.index()].eff = mem::Effects::READ_HEAP;
    assert_eq!(before.frames, after.frames);
    list_source_states_equal(before, &after);
    after.verify().unwrap();
    after
}

fn coemit_lower_capture(source: &ByteCodeFunction, plan: &ir::Func) -> (CompiledLeaf, String) {
    let mut result = None;
    let clif = super::compile_pipeline_tests::captured_clif(|| {
        result = Some(lower(source, plan));
    });
    assert_eq!(clif.len(), 1, "one actual native Opt body");
    (result.unwrap(), coemit_mask_pointer_immediates(&clif[0]))
}

fn coemit_resume(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    frame: &DeoptResume,
) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.run_resumed_frame(
        source,
        Value::NIL,
        frame.pc,
        &frame.stack,
        frame.handlers,
        &frame.binds,
        frame.spec_base,
        frame.cond_base,
    )
}

#[test]
fn opt_gvn_native_adjacent_list_accessor_coemits_original_guard_and_clif() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let [a, d, x] = children();
    let payload = Value::list(vec![Value::symbol("heap-companion")]);
    let _roots = Roots::new(&[a, d, x, payload]);
    let mut clif_pairs = Vec::new();
    for cdr in [false, true] {
        for after_store in [false, true] {
            let (source, pc) = if after_store {
                coemit_after_store_source(cdr)
            } else {
                (
                    function(
                        vec![
                            Op::StackRef(0),
                            if cdr { Op::Cdr } else { Op::Car },
                            Op::Return,
                        ],
                        vec![],
                        1,
                    ),
                    1,
                )
            };
            let before = prepared(&source);
            let after = coemit_typed_target(&before, pc, cdr);
            let (baseline, baseline_clif) = coemit_lower_capture(&source, &before);
            let (candidate, candidate_clif) = coemit_lower_capture(&source, &after);
            // Exact real GNU nil/cons behavior and identities before checking
            // optimization. Every invocation gets a fresh mutated source cell.
            for is_nil in [false, true] {
                let bad = if is_nil {
                    Value::NIL
                } else {
                    Value::cons(a, d)
                };
                let _bad = Roots::new(&[bad]);
                for (plan, leaf) in [(&before, &baseline), (&after, &candidate)] {
                    check(&mut ctx, &source, plan, leaf, |_| {
                        if after_store {
                            vec![Value::cons(a, d), x, bad, payload]
                        } else {
                            vec![bad]
                        }
                    });
                }
            }
            // Nonlists must deopt before the pointer read. Exact source PC and
            // full GNU stack match the old native and independent reference.
            let wrongs = [
                Value::symbol("wrong"),
                Value::fixnum(0),
                Value::string("wrong"),
            ];
            let _wrongs = Roots::new(&wrongs);
            for bad in wrongs {
                let args = if after_store {
                    vec![Value::cons(a, d), x, bad, payload]
                } else {
                    vec![bad]
                };
                let _roots = Roots::new(&args);
                ctx.obarray.set_symbol_value("t34-o34-side", Value::NIL);
                let expected = signal_text(&tier0(&mut ctx, &source, &args).unwrap_err());
                let mut old_frame = None;
                for (plan, leaf) in [(&before, &baseline), (&after, &candidate)] {
                    ctx.obarray.set_symbol_value("t34-o34-side", Value::NIL);
                    let run = eval::evaluate(
                        plan,
                        &mut ctx,
                        eval::Inputs {
                            args: &args,
                            ..eval::Inputs::default()
                        },
                    )
                    .unwrap();
                    let eval::Outcome::Deopt(snapshot) = run.outcome else {
                        panic!("reference executes the original list guard")
                    };
                    ctx.obarray.set_symbol_value("t34-o34-side", Value::NIL);
                    let NativeRun::DeoptAt(frame) =
                        leaf.call(&mut ctx as *mut Context as *mut u8, &args)
                    else {
                        panic!("nonlist must fail the actual accessor's three-way guard")
                    };
                    assert_eq!(frame.pc, pc);
                    assert_eq!(frame.pc as u32, snapshot.pc);
                    assert_eq!(
                        frame
                            .stack
                            .iter()
                            .copied()
                            .map(ir::ValueBits::from_value)
                            .collect::<Vec<_>>(),
                        snapshot.stack
                    );
                    assert_eq!(frame.handlers, snapshot.handlers as usize);
                    assert_eq!(frame.binds.len(), snapshot.binds as usize);
                    if let Some((old_pc, old_stack, old_handlers, old_binds)) = &old_frame {
                        assert_eq!(&frame.pc, old_pc);
                        assert_eq!(&frame.stack, old_stack);
                        assert_eq!(&frame.handlers, old_handlers);
                        assert_eq!(&frame.binds.len(), old_binds);
                    } else {
                        old_frame = Some((
                            frame.pc,
                            frame.stack.clone(),
                            frame.handlers,
                            frame.binds.len(),
                        ));
                    }
                    if after_store {
                        assert_eq!(args[0].cons_car(), x);
                        let side = ctx.obarray.symbol_value("t34-o34-side").copied().unwrap();
                        assert_eq!(side.cons_cdr().cons_cdr().cons_car(), x);
                        assert_eq!(side.cons_cdr().cons_cdr().cons_cdr().cons_car(), payload);
                    }
                    assert_eq!(
                        signal_text(&coemit_resume(&mut ctx, &source, &frame).unwrap_err()),
                        expected
                    );
                    assert_eq!(ctx.jit_root_stack_top, 0);
                }
            }
            if !after_store {
                // Forced cold recovery still belongs to a REAL shared guard;
                // it cannot disappear when the explicit CheckType is deferred.
                force_deopt_for_test(true);
                let forced_baseline = lower(&source, &before);
                let forced_candidate = lower(&source, &after);
                force_deopt_for_test(false);
                for is_nil in [false, true] {
                    let bad = if is_nil {
                        Value::NIL
                    } else {
                        Value::cons(a, d)
                    };
                    let args = [bad];
                    let _args = Roots::new(&args);
                    let expected = tier0(&mut ctx, &source, &args).unwrap();
                    let NativeRun::DeoptAt(old) =
                        forced_baseline.call(&mut ctx as *mut Context as *mut u8, &args)
                    else {
                        panic!("baseline forced accessor guard exits")
                    };
                    let NativeRun::DeoptAt(new) =
                        forced_candidate.call(&mut ctx as *mut Context as *mut u8, &args)
                    else {
                        panic!("deferred typed guard remains FORCE_DEOPT executable")
                    };
                    assert_eq!(new.pc, old.pc);
                    assert_eq!(new.stack, old.stack);
                    assert_eq!(coemit_resume(&mut ctx, &source, &new).unwrap(), expected);
                    assert_eq!(ctx.jit_root_stack_top, 0);
                }
            }
            // This is last: the current regression is a duplicate LIST guard
            // plus a separate nil branch. After the fix the typed accessor
            // uses the exact shared operation, including the actual guard.
            clif_pairs.push((cdr, after_store, candidate_clif, baseline_clif));
        }
    }
    for (cdr, after_store, candidate, baseline) in clif_pairs {
        assert_eq!(
            candidate, baseline,
            "adjacent LIST exposure retains native CLIF: cdr={cdr}, after_store={after_store}"
        );
    }
}
