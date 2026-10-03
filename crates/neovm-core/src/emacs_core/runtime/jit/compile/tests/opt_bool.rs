//! Selected Bool transport is tested against Tier-0 and the independent IR
//! evaluator. GNU observations/offsets are frozen in tmp/t34-o32-gnu-prep.
//! Threading: plans, contexts and temporary roots are invocation-owned; scoped
//! overrides contain compiler settings only and are restored on scope exit.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::{captured_clif, function};
use super::*;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::eval::{
    bytecode_branch_poll_count, push_scratch_gc_roots, reset_bytecode_branch_poll_count,
    restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::{build, eval, ir, mem, passes::bools, types::TypeSet};

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            bool_rep: true,
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

fn plan(source: &ByteCodeFunction) -> ir::Func {
    let arity = source.params.required.len();
    let cfg = analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        arity,
    )
    .expect("source CFG");
    let constants = source
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    build::build(build::BuildInput {
        ops: source.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: ir::ParamShape {
            required: arity,
            ..ir::ParamShape::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .expect("source SSA")
}

fn boolean_plan(source: &ByteCodeFunction) -> ir::Func {
    let mut plan = plan(source);
    bools::run(&mut plan).expect("selected Bool pass");
    plan.verify().expect("Boolean SSA verifies");
    plan
}

fn lower(source: &ByteCodeFunction, plan: &ir::Func) -> CompiledLeaf {
    plan.verify().expect("native input verifies");
    let leaf = lower_opt_ir_for_test(
        source.executable_ops(),
        &source.constants,
        source.params.required.len(),
        source.executable_gnu_byte_offset_map(),
        plan,
    )
    .expect("selected Bool native lowering");
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    leaf
}

fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[Value]) -> Value {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec()).expect("Tier-0 return")
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
    .expect("reference execution");
    let eval::Outcome::Returned(value) = run.outcome else {
        panic!("reference must return")
    };
    value.to_value()
}

fn native(ctx: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    let NativeRun::Ok(bits) = leaf.call(ctx as *mut Context as *mut u8, args) else {
        panic!("native must return")
    };
    Value::from_bits(bits)
}

fn opaque_count(plan: &ir::Func) -> usize {
    plan.insts
        .iter()
        .filter(|inst| matches!(inst.op, ir::Opcode::OpaqueBool(_)))
        .count()
}

fn bool_phi_count(plan: &ir::Func) -> usize {
    plan.blocks
        .iter()
        .flat_map(|block| &block.params)
        .filter(|value| plan.values[value.index()].rep == ir::Rep::Bool)
        .count()
}

fn direct_compare_branch(clif: &str) -> bool {
    clif.lines().any(|line| {
        let Some((value, operation)) = line.trim().split_once(" = ") else {
            return false;
        };
        let mut tokens = operation.split_whitespace();
        matches!(tokens.next(), Some("icmp" | "icmp.i64"))
            && tokens.next() == Some("slt")
            && clif
                .lines()
                .any(|branch| branch.trim().starts_with(&format!("brif {value},")))
    })
}

#[test]
fn opt_bool_native_compare_reaches_branch_without_tagged_projection() {
    let _settings = Settings::enter();
    assert!(direct_compare_branch(
        "v35 = icmp.i64 slt v27, v28\n    brif v35, block4, block5"
    ));
    assert!(direct_compare_branch(
        "v35 = icmp slt v27, v28\n    brif v35, block4, block5"
    ));
    assert!(!direct_compare_branch(
        "v35 = icmp.i64 slt v27, v28\n    v36 = select v35, v1, v2\n    brif v36, block4, block5"
    ));
    assert!(!direct_compare_branch(
        "v35 = icmp.i64 slt v27, v28\n    brif v350, block4, block5"
    ));
    let source = function(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Lss,
            Op::GotoIfNil(6),
            Op::Constant(0),
            Op::Return,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::fixnum(7), Value::fixnum(9)],
        2,
    );
    let enabled = boolean_plan(&source);
    assert_eq!(opaque_count(&enabled), 1);
    let mut leaf = None;
    let clif = captured_clif(|| leaf = Some(lower(&source, &enabled)));
    assert_eq!(clif.len(), 1);
    assert!(
        direct_compare_branch(&clif[0]),
        "successful compare must directly feed brif: {}",
        clif[0]
    );
    let leaf = leaf.unwrap();
    let mut ctx = Context::new();
    for args in [
        [Value::fixnum(1), Value::fixnum(2)],
        [Value::fixnum(2), Value::fixnum(1)],
    ] {
        let expected = tier0(&mut ctx, &source, &args);
        assert_eq!(reference(&mut ctx, &enabled, &args), expected);
        assert_eq!(native(&mut ctx, &leaf, &args), expected);
    }
}

#[test]
fn opt_bool_native_closed_diamond_uses_i8_phi_and_returns_t_nil() {
    let _settings = Settings::enter();
    let source = function(
        vec![
            Op::StackRef(2),
            Op::GotoIfNil(6),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Lss,
            Op::Goto(9),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Eq,
            Op::Dup,
            Op::GotoIfNil(12),
            Op::Return,
            Op::Return,
        ],
        vec![],
        3,
    );
    let enabled = boolean_plan(&source);
    assert!(
        bool_phi_count(&enabled) > 0,
        "closed diamond must promote its real phi"
    );
    let mut leaf = None;
    let clif = captured_clif(|| leaf = Some(lower(&source, &enabled)));
    assert!(
        clif[0]
            .lines()
            .any(|line| line.trim().starts_with("block") && line.contains(": i8")),
        "a promoted phi must physically transport I8: {}",
        clif[0]
    );
    let leaf = leaf.unwrap();
    let mut ctx = Context::new();
    for selector in [Value::NIL, Value::T] {
        for pair in [
            [Value::fixnum(1), Value::fixnum(2)],
            [Value::fixnum(2), Value::fixnum(1)],
            [Value::fixnum(1), Value::fixnum(1)],
        ] {
            let args = [selector, pair[0], pair[1]];
            let expected = tier0(&mut ctx, &source, &args);
            assert!(expected.is_t() || expected.is_nil());
            assert_eq!(reference(&mut ctx, &enabled, &args), expected);
            assert_eq!(native(&mut ctx, &leaf, &args), expected);
        }
    }
}

fn bool_loop() -> ByteCodeFunction {
    function(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Eq,
            Op::StackRef(3),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(14),
            Op::StackRef(0),
            Op::Not,
            Op::StackSet(1),
            Op::StackRef(3),
            Op::Sub1,
            Op::StackSet(4),
            Op::Goto(3),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::fixnum(0)],
        3,
    )
}

#[test]
fn opt_bool_native_loop_phi_keeps_normalized_flags_and_poll_cadence() {
    let _settings = Settings::enter();
    let source = bool_loop();
    let enabled = boolean_plan(&source);
    assert!(
        bool_phi_count(&enabled) > 0,
        "seeded cyclic Bool phi must promote"
    );
    let leaf = lower(&source, &enabled);
    let mut ctx = Context::new();
    for n in [0, 1, 2, 3, 254, 255, 256, 510] {
        let args = [Value::fixnum(n), Value::fixnum(7), Value::fixnum(7)];
        reset_bytecode_branch_poll_count();
        let expected = tier0(&mut ctx, &source, &args);
        let polls = bytecode_branch_poll_count();
        assert_eq!(polls, (n / 255) as usize);
        assert_eq!(reference(&mut ctx, &enabled, &args), expected);
        reset_bytecode_branch_poll_count();
        assert_eq!(native(&mut ctx, &leaf, &args), expected);
        assert_eq!(bytecode_branch_poll_count(), polls);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_bool_native_unknown_phi_keeps_arbitrary_lisp_identity() {
    let _settings = Settings::enter();
    let source = function(
        vec![
            Op::StackRef(3),
            Op::GotoIfNil(6),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Lss,
            Op::Goto(7),
            Op::StackRef(0),
            Op::GotoIfNotNilElsePop(9),
            Op::Constant(0),
            Op::Return,
        ],
        vec![Value::symbol("fallback")],
        4,
    );
    let enabled = boolean_plan(&source);
    assert!(opaque_count(&enabled) > 0);
    assert!(
        enabled
            .blocks
            .iter()
            .filter(|block| block.preds.len() >= 2)
            .flat_map(|block| &block.params)
            .any(|value| enabled.values[value.index()].rep == ir::Rep::Tagged)
    );
    let leaf = lower(&source, &enabled);
    let mut ctx = Context::new();
    let inputs = [
        Value::fixnum(0),
        Value::make_float(-0.0),
        Value::cons(Value::fixnum(7), Value::NIL),
    ];
    let _roots = Roots::new(&inputs);
    for input in inputs {
        let args = [Value::NIL, Value::fixnum(1), Value::fixnum(2), input];
        let expected = tier0(&mut ctx, &source, &args);
        assert_eq!(expected, input);
        assert_eq!(reference(&mut ctx, &enabled, &args), expected);
        assert_eq!(native(&mut ctx, &leaf, &args), expected);
    }
    for pair in [
        [Value::fixnum(1), Value::fixnum(2)],
        [Value::fixnum(2), Value::fixnum(1)],
    ] {
        let args = [Value::T, pair[0], pair[1], inputs[2]];
        let expected = tier0(&mut ctx, &source, &args);
        assert_eq!(reference(&mut ctx, &enabled, &args), expected);
        assert_eq!(native(&mut ctx, &leaf, &args), expected);
    }
}

#[test]
fn opt_bool_native_else_pop_preserves_zero_float_and_heap_identity() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let fallback = Value::cons(Value::symbol("fallback"), Value::NIL);
    let inputs = [
        Value::NIL,
        Value::fixnum(0),
        Value::make_float(0.0),
        Value::make_float(-0.0),
        Value::cons(Value::fixnum(7), Value::NIL),
        ctx.eval_str("[7]").unwrap(),
        ctx.eval_str("\"identity\"").unwrap(),
        Value::symbol("identity-symbol"),
    ];
    let mut roots = inputs.to_vec();
    roots.push(fallback);
    let _roots = Roots::new(&roots);
    for branch in [Op::GotoIfNilElsePop(4), Op::GotoIfNotNilElsePop(4)] {
        let source = function(
            vec![
                Op::StackRef(1),
                branch,
                Op::StackRef(0),
                Op::Goto(4),
                Op::Return,
            ],
            vec![],
            2,
        );
        let enabled = boolean_plan(&source);
        assert_eq!(
            bool_phi_count(&enabled),
            0,
            "unknown else-pop inputs are never Boolean proofs"
        );
        let leaf = lower(&source, &enabled);
        for input in inputs {
            let args = [input, fallback];
            let expected = tier0(&mut ctx, &source, &args);
            assert_eq!(reference(&mut ctx, &enabled, &args), expected);
            assert_eq!(native(&mut ctx, &leaf, &args), expected);
        }
    }
}

fn emit(
    func: &mut ir::Func,
    block: ir::Block,
    op: ir::Opcode,
    args: Vec<ir::Value>,
    rep: ir::Rep,
    ty: TypeSet,
) -> ir::Value {
    let inst = ir::Inst(func.insts.len() as u32);
    let value = ir::Value(func.values.len() as u32);
    func.values.push(ir::ValueData {
        def: ir::ValueDef::Inst(inst),
        rep,
        ty,
    });
    func.insts.push(ir::InstData {
        op,
        args,
        result: Some(value),
        eff: mem::Effects::PURE,
        mem: mem::AliasClass::None,
        frame: None,
        pc: func.blocks[block.index()].pc,
    });
    func.blocks[block.index()].insts.push(inst);
    value
}

fn bool_param(func: &mut ir::Func, block: ir::Block) -> ir::Value {
    let value = ir::Value(func.values.len() as u32);
    let index = func.blocks[block.index()].params.len() as u32;
    func.values.push(ir::ValueData {
        def: ir::ValueDef::Param { block, index },
        rep: ir::Rep::Bool,
        ty: TypeSet::BOOLEAN,
    });
    func.blocks[block.index()].params.push(value);
    value
}

fn select_plan(local_value: bool, local_on_true: bool) -> ir::Func {
    let entry = ir::Block(0);
    let join = ir::Block(1);
    let mut plan = ir::Func::new(
        Vec::new().into_boxed_slice(),
        ir::ParamShape {
            required: 2,
            ..ir::ParamShape::default()
        },
        0,
    );
    plan.blocks = vec![ir::BlockData::new(0), ir::BlockData::new(1)];
    let mut flags = Vec::new();
    for argument in 0..2 {
        let input = emit(
            &mut plan,
            entry,
            ir::Opcode::Arg(argument),
            vec![],
            ir::Rep::Tagged,
            TypeSet::TOP,
        );
        flags.push(emit(
            &mut plan,
            entry,
            ir::Opcode::IsNonNil,
            vec![input],
            ir::Rep::Bool,
            TypeSet::BOOLEAN,
        ));
    }
    plan.blocks[0].term = ir::Term::Jump(ir::Edge {
        target: join,
        args: flags,
    });
    plan.blocks[1].preds = vec![entry];
    let condition = bool_param(&mut plan, join);
    let incoming = bool_param(&mut plan, join);
    let local = emit(
        &mut plan,
        join,
        ir::Opcode::BoolConst(local_value),
        vec![],
        ir::Rep::Bool,
        if local_value {
            TypeSet::T
        } else {
            TypeSet::NIL
        },
    );
    let (yes, no) = if local_on_true {
        (local, incoming)
    } else {
        (incoming, local)
    };
    let selected = emit(
        &mut plan,
        join,
        ir::Opcode::Select,
        vec![condition, yes, no],
        ir::Rep::Bool,
        TypeSet::BOOLEAN,
    );
    let tagged = emit(
        &mut plan,
        join,
        ir::Opcode::BoolToLisp,
        vec![selected],
        ir::Rep::Tagged,
        TypeSet::BOOLEAN,
    );
    plan.blocks[1].term = ir::Term::Return(tagged);
    plan.verify().expect("local/incoming Bool Select");
    plan
}

#[test]
fn opt_bool_native_select_accepts_local_and_incoming_flags_both_arm_orders() {
    let _settings = Settings::enter();
    let source = function(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Eq, Op::Return],
        vec![],
        2,
    );
    let mut ctx = Context::new();
    for local_value in [false, true] {
        for local_on_true in [false, true] {
            let plan = select_plan(local_value, local_on_true);
            let leaf = lower(&source, &plan);
            for condition in [Value::NIL, Value::T] {
                for incoming in [Value::NIL, Value::fixnum(0)] {
                    let args = [condition, incoming];
                    let expected = reference(&mut ctx, &plan, &args);
                    assert!(expected.is_t() || expected.is_nil());
                    assert_eq!(native(&mut ctx, &leaf, &args), expected);
                }
            }
        }
    }
}

#[test]
fn opt_bool_native_mixed_tagged_select_materializes_only_boolean_arm() {
    let _settings = Settings::enter();
    let source = function(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Eq, Op::Return],
        vec![],
        2,
    );
    let mut ctx = Context::new();
    let payload = Value::cons(Value::fixnum(7), Value::NIL);
    let _roots = Roots::new(&[payload]);
    for boolean_on_true in [false, true] {
        let block = ir::Block(0);
        let mut changed = ir::Func::new(
            vec![ir::ValueBits::from_value(Value::T)].into_boxed_slice(),
            ir::ParamShape {
                required: 2,
                ..ir::ParamShape::default()
            },
            0,
        );
        changed.blocks.push(ir::BlockData::new(0));
        let input = emit(
            &mut changed,
            block,
            ir::Opcode::Arg(0),
            vec![],
            ir::Rep::Tagged,
            TypeSet::TOP,
        );
        let unknown = emit(
            &mut changed,
            block,
            ir::Opcode::Arg(1),
            vec![],
            ir::Rep::Tagged,
            TypeSet::TOP,
        );
        let condition = emit(
            &mut changed,
            block,
            ir::Opcode::IsNonNil,
            vec![input],
            ir::Rep::Bool,
            TypeSet::BOOLEAN,
        );
        let boolean = emit(
            &mut changed,
            block,
            ir::Opcode::Const(0),
            vec![],
            ir::Rep::Tagged,
            TypeSet::T,
        );
        let (yes, no) = if boolean_on_true {
            (boolean, unknown)
        } else {
            (unknown, boolean)
        };
        let result = emit(
            &mut changed,
            block,
            ir::Opcode::Select,
            vec![condition, yes, no],
            ir::Rep::Tagged,
            TypeSet::TOP,
        );
        changed.blocks[0].term = ir::Term::Return(result);
        changed.verify().expect("original mixed tagged Select");
        let original = changed.clone();
        bools::run(&mut changed).expect("Bool arm projection");
        assert_eq!(
            changed.values[changed.resolve(result).unwrap().index()].rep,
            ir::Rep::Tagged
        );
        assert!(
            changed
                .insts
                .iter()
                .any(|inst| inst.op == ir::Opcode::BoolToLisp)
        );
        let leaf = lower(&source, &changed);
        for condition in [Value::NIL, Value::T] {
            for arbitrary in [Value::fixnum(0), payload] {
                let args = [condition, arbitrary];
                let expected = reference(&mut ctx, &original, &args);
                assert_eq!(reference(&mut ctx, &changed, &args), expected);
                assert_eq!(native(&mut ctx, &leaf, &args), expected);
            }
        }
    }
}

#[test]
fn opt_bool_native_predicates_and_eq_keep_dynamic_positioned_symbol_semantics() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let bare = Value::symbol("opt-bool-positioned");
    let positioned = ctx
        .eval_str("(position-symbol 'opt-bool-positioned 7)")
        .unwrap();
    let other_position = ctx
        .eval_str("(position-symbol 'opt-bool-positioned 9)")
        .unwrap();
    let inputs = [
        Value::NIL,
        Value::fixnum(0),
        Value::make_float(-0.0),
        Value::cons(Value::fixnum(7), Value::NIL),
        ctx.eval_str("[7]").unwrap(),
        ctx.eval_str("\"text\"").unwrap(),
        bare,
        positioned,
    ];
    let mut roots = inputs.to_vec();
    roots.push(other_position);
    let _roots = Roots::new(&roots);
    for op in [
        Op::Null,
        Op::Not,
        Op::Consp,
        Op::Stringp,
        Op::Listp,
        Op::Symbolp,
        Op::Integerp,
        Op::Numberp,
    ] {
        let source = function(vec![Op::StackRef(0), op, Op::Return], vec![], 1);
        let enabled = boolean_plan(&source);
        assert_eq!(opaque_count(&enabled), 1);
        let leaf = lower(&source, &enabled);
        for mode in [false, true] {
            ctx.symbols_with_pos_enabled = mode;
            for argument in inputs {
                let expected = tier0(&mut ctx, &source, &[argument]);
                assert!(expected.is_t() || expected.is_nil());
                assert_eq!(reference(&mut ctx, &enabled, &[argument]), expected);
                assert_eq!(native(&mut ctx, &leaf, &[argument]), expected);
            }
        }
    }
    let source = function(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Eq, Op::Return],
        vec![],
        2,
    );
    let enabled = boolean_plan(&source);
    let leaf = lower(&source, &enabled);
    for mode in [false, true] {
        ctx.symbols_with_pos_enabled = mode;
        for pair in [
            [bare, positioned],
            [positioned, bare],
            [positioned, other_position],
            [positioned, positioned],
        ] {
            let expected = tier0(&mut ctx, &source, &pair);
            assert_eq!(reference(&mut ctx, &enabled, &pair), expected);
            assert_eq!(native(&mut ctx, &leaf, &pair), expected);
        }
    }
    ctx.symbols_with_pos_enabled = false;
}

#[test]
fn opt_bool_native_return_call_and_store_escape_as_gnu_t_nil() {
    let _settings = Settings::enter();
    let source = function(
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Lss,
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Setcar,
            Op::Pop,
            Op::Dup,
            Op::VarSet(0),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::StackRef(1),
            Op::StackRef(3),
            Op::Car,
            Op::List(3),
            Op::Return,
        ],
        vec![Value::symbol("opt-bool-escape"), Value::symbol("identity")],
        3,
    );
    let enabled = boolean_plan(&source);
    assert!(opaque_count(&enabled) > 0);
    assert!(
        enabled
            .insts
            .iter()
            .filter(|inst| inst.op == ir::Opcode::BoolToLisp)
            .count()
            > 0
    );
    let leaf = lower(&source, &enabled);
    let mut ctx = Context::new();
    ctx.eval_str("(setq opt-bool-escape nil)").unwrap();
    let cell = Value::cons(Value::symbol("old"), Value::NIL);
    let _roots = Roots::new(&[cell]);
    for pair in [
        [Value::fixnum(1), Value::fixnum(2)],
        [Value::fixnum(2), Value::fixnum(1)],
    ] {
        let args = [pair[0], pair[1], cell];
        let expected = crate::emacs_core::print::print_value(&tier0(&mut ctx, &source, &args));
        let reference_value = reference(&mut ctx, &enabled, &args);
        assert_eq!(
            crate::emacs_core::print::print_value(&reference_value),
            expected
        );
        let actual = native(&mut ctx, &leaf, &args);
        assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
        let stored = ctx.eval_str("opt-bool-escape").unwrap();
        assert!(stored.is_t() || stored.is_nil());
        assert_eq!(cell.cons_car(), stored);
        let direct = function(
            vec![Op::StackRef(1), Op::StackRef(1), Op::Lss, Op::Return],
            vec![],
            2,
        );
        let direct_plan = boolean_plan(&direct);
        let direct_leaf = lower(&direct, &direct_plan);
        assert_eq!(native(&mut ctx, &direct_leaf, &pair), stored);
    }
}

fn cold_source(first: Op) -> ByteCodeFunction {
    // Exact operators and byte offsets from the frozen GNU direct/inner oracle:
    // first comparison 2, visible varset 7, failing lss 11. The public GNU
    // backtrace omits PCs; these are disassembly offsets, not invented results.
    let mut source = function(
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            first,
            Op::Constant(0),
            Op::StackRef(1),
            Op::StackRef(3),
            Op::List(3),
            Op::VarSet(1),
            Op::Dup,
            Op::StackRef(5),
            Op::Constant(2),
            Op::Lss,
            Op::StackRef(3),
            Op::List(3),
            Op::Return,
        ],
        vec![
            Value::symbol("direct"),
            Value::symbol("t34-o32-side"),
            Value::fixnum(1),
        ],
        4,
    );
    source.gnu_byte_offset_map = Some(
        (0..15)
            .map(|instruction_index| GnuByteOffsetMapEntry {
                byte_offset: instruction_index,
                instruction_index,
            })
            .collect(),
    );
    source
}

#[test]
fn opt_bool_native_cold_guard_materializes_complete_frame_after_visible_store() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    ctx.eval_str("(setq t34-o32-side nil)").unwrap();
    let payload = Value::cons(Value::symbol("heap-companion"), Value::NIL);
    let _roots = Roots::new(&[payload]);
    for first in [Op::Lss, Op::Eq] {
        let source = cold_source(first);
        let enabled = boolean_plan(&source);
        let state = enabled.source_states[11]
            .as_ref()
            .expect("failure source state");
        assert_eq!(enabled.frames[state.frame.index()].pc, 11);
        assert!(
            state
                .pre
                .iter()
                .filter(|value| enabled.values[value.index()].rep == ir::Rep::Bool)
                .count()
                >= 2,
            "both duplicated source Boolean identities must remain in the cold frame"
        );
        let leaf = lower(&source, &enabled);
        let args = [
            Value::symbol("wrong"),
            Value::fixnum(1),
            Value::fixnum(2),
            payload,
        ];
        // The opaque reference executes Tier-0's comparison and signals, as
        // captured by GNU's direct/nested cold-frame oracle. Native instead
        // deopts before that comparison's implicit fixnum guard fails.
        let tier0_flow = {
            let mut vm = Vm::from_context(&mut ctx);
            vm.force_interpreter_only_for_test();
            vm.execute(&source, args.to_vec())
                .expect_err("Tier-0 comparison signals after its visible store")
        };
        let tier0_signal = tier0_flow.as_signal().expect("Tier-0 wrong-type signal");
        // GNU oracle rows 59/60 record this condition; its payload below is
        // derived from Tier-0 rather than reconstructed by this test.
        assert_eq!(tier0_signal.symbol_name(), "wrong-type-argument");
        let untouched = eval::evaluate(
            &enabled,
            &mut ctx,
            eval::Inputs {
                args: &args,
                ..eval::Inputs::default()
            },
        )
        .expect_err("untouched opaque reference must report the Tier-0 signal");
        let eval::EvalError::Flow(reference_flow) = untouched else {
            panic!("untouched reference must execute the failing Tier-0 operation")
        };
        let reference_signal = reference_flow
            .as_signal()
            .expect("reference wrong-type signal");
        assert_eq!(reference_signal.symbol, tier0_signal.symbol);
        assert_eq!(reference_signal.data, tier0_signal.data);
        assert_eq!(reference_signal.raw_data, tier0_signal.raw_data);
        let stored_before_deopt = ctx.eval_str("t34-o32-side").unwrap();
        let stored_text = crate::emacs_core::print::print_value(&stored_before_deopt);
        assert_eq!(
            stored_before_deopt.cons_cdr().cons_cdr().cons_car(),
            payload
        );

        // This test-only plan exposes the baseline's implicit numeric guard
        // to the independent evaluator. Keep the original comparison and
        // all its source metadata; the native plan above is unchanged.
        let mut guarded_reference = enabled.clone();
        let comparison = guarded_reference
            .insts
            .iter()
            .enumerate()
            .find(|(_, inst)| inst.pc == 11 && matches!(inst.op, ir::Opcode::OpaqueBool(Op::Lss)))
            .map(|(index, _)| ir::Inst(index as u32))
            .expect("original failing comparison");
        assert_eq!(
            guarded_reference.insts[comparison.index()].frame,
            Some(state.frame)
        );
        let guard = ir::Inst(guarded_reference.insts.len() as u32);
        let checked = ir::Value(guarded_reference.values.len() as u32);
        guarded_reference.values.push(ir::ValueData {
            ty: TypeSet::FIXNUM,
            rep: ir::Rep::Tagged,
            def: ir::ValueDef::Inst(guard),
        });
        guarded_reference.insts.push(ir::InstData {
            op: ir::Opcode::CheckType(TypeSet::FIXNUM),
            args: vec![guarded_reference.insts[comparison.index()].args[0]],
            result: Some(checked),
            eff: mem::Effects::MAY_DEOPT,
            mem: mem::AliasClass::None,
            frame: Some(state.frame),
            pc: 11,
        });
        let block = &mut guarded_reference.blocks[state.block.index()];
        let position = block
            .insts
            .iter()
            .position(|id| *id == comparison)
            .expect("comparison remains in its original source block");
        block.insts.insert(position, guard);
        guarded_reference
            .verify()
            .expect("explicit reference-only numeric guard verifies");
        assert_eq!(guarded_reference.frames, enabled.frames);
        assert_eq!(guarded_reference.entry_stacks, enabled.entry_stacks);
        assert_eq!(
            guarded_reference.source_states.len(),
            enabled.source_states.len()
        );
        for (actual, original) in guarded_reference
            .source_states
            .iter()
            .zip(&enabled.source_states)
        {
            match (actual, original) {
                (Some(actual), Some(original)) => {
                    assert_eq!(actual.frame, original.frame);
                    assert_eq!(actual.block, original.block);
                    assert_eq!(actual.pre, original.pre);
                    assert_eq!(actual.post, original.post);
                }
                (None, None) => {}
                _ => panic!("source-state presence must remain unchanged"),
            }
        }
        for (actual, original) in guarded_reference.insts.iter().zip(&enabled.insts) {
            assert_eq!(actual.op, original.op);
            assert_eq!(actual.args, original.args);
            assert_eq!(actual.result, original.result);
            assert_eq!(actual.eff, original.eff);
            assert_eq!(actual.mem, original.mem);
            assert_eq!(actual.frame, original.frame);
            assert_eq!(actual.pc, original.pc);
        }
        let run = eval::evaluate(
            &guarded_reference,
            &mut ctx,
            eval::Inputs {
                args: &args,
                ..eval::Inputs::default()
            },
        )
        .expect("reference explicit guard deopt");
        let eval::Outcome::Deopt(expected) = run.outcome else {
            panic!("reference-only numeric guard must deopt before the opaque comparison")
        };
        let NativeRun::DeoptAt(actual) = leaf.call(&mut ctx as *mut Context as *mut u8, &args)
        else {
            panic!("native failed guard must preserve its full frame")
        };
        assert_eq!(actual.pc, expected.pc as usize);
        assert_eq!(
            actual.stack,
            expected
                .stack
                .iter()
                .map(|bits| bits.to_value())
                .collect::<Vec<_>>()
        );
        assert_eq!(actual.stack.len(), 8);
        assert!(actual.stack[4].is_t() || actual.stack[4].is_nil());
        assert_eq!(actual.stack[4], actual.stack[5]);
        assert_eq!(actual.stack[3], payload);
        let stored = ctx.eval_str("t34-o32-side").unwrap();
        assert_eq!(crate::emacs_core::print::print_value(&stored), stored_text);
        assert_eq!(stored.cons_cdr().cons_car(), actual.stack[4]);
        assert_eq!(stored.cons_cdr().cons_cdr().cons_car(), payload);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

fn compiler_cons(plan: &ir::Func) -> ir::Value {
    plan.insts
        .iter()
        .find(|inst| matches!(inst.op, ir::Opcode::Opaque(Op::Cons)))
        .unwrap()
        .result
        .unwrap()
}

fn return_cons(plan: &mut ir::Func, cell: ir::Value) {
    for block in &mut plan.blocks {
        if matches!(block.term, ir::Term::Return(_)) {
            block.term = ir::Term::Return(cell);
        }
    }
    plan.verify()
        .expect("compiler-only cons remains live by the IR return");
}

#[test]
fn opt_bool_native_gc_call_roots_heap_companion_without_external_root() {
    let _settings = Settings::enter();
    force_tail_unrooted_for_test(Some(true));
    let source = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Pop,
            Op::True,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Dup,
            Op::GotoIfNil(13),
            Op::Pop,
            Op::Nil,
            Op::Return,
            Op::Pop,
            Op::True,
            Op::Return,
        ],
        vec![
            Value::fixnum(7),
            Value::fixnum(9),
            Value::symbol("garbage-collect"),
        ],
        0,
    );
    let mut enabled = boolean_plan(&source);
    let cell = compiler_cons(&enabled);
    return_cons(&mut enabled, cell);
    let call = enabled
        .insts
        .iter()
        .find(|inst| matches!(inst.op, ir::Opcode::Opaque(Op::Call(0))))
        .unwrap();
    let stack = &enabled.frames[call.frame.unwrap().index()].stack;
    assert!(
        !stack.contains(&cell),
        "heap companion has no original-stack root"
    );
    assert!(
        stack
            .iter()
            .any(|value| enabled.values[value.index()].rep == ir::Rep::Bool)
    );
    let mut ctx = Context::new();
    let expected = crate::emacs_core::print::print_value(&reference(&mut ctx, &enabled, &[]));
    let leaf = lower(&source, &enabled);
    let collections = ctx.tagged_heap.gc_collections();
    let actual = native(&mut ctx, &leaf, &[]);
    assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
    assert!(ctx.tagged_heap.gc_collections() > collections);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_bool_native_poll_roots_heap_companion_without_external_root() {
    let _settings = Settings::enter();
    let source = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Pop,
            Op::True,
            Op::StackRef(1),
            Op::Constant(2),
            Op::Gtr,
            Op::GotoIfNil(16),
            Op::StackRef(0),
            Op::Not,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(5),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::fixnum(7), Value::fixnum(9), Value::fixnum(0)],
        1,
    );
    let mut enabled = boolean_plan(&source);
    assert!(bool_phi_count(&enabled) > 0);
    let cell = compiler_cons(&enabled);
    return_cons(&mut enabled, cell);
    let polls = enabled
        .insts
        .iter()
        .filter(|inst| inst.op == ir::Opcode::Poll)
        .collect::<Vec<_>>();
    assert!(!polls.is_empty());
    for poll in polls {
        let stack = &enabled.frames[poll.frame.unwrap().index()].stack;
        assert!(!stack.contains(&cell));
        assert!(
            stack
                .iter()
                .any(|value| enabled.values[value.index()].rep == ir::Rep::Bool)
        );
    }
    let leaf = lower(&source, &enabled);
    let mut ctx = Context::new();
    for n in [255, 510] {
        let args = [Value::fixnum(n)];
        ctx.gc_stress = true;
        let expected = crate::emacs_core::print::print_value(&reference(&mut ctx, &enabled, &args));
        let collections = ctx.tagged_heap.gc_collections();
        reset_bytecode_branch_poll_count();
        let actual = native(&mut ctx, &leaf, &args);
        ctx.gc_stress = false;
        assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
        assert!(ctx.tagged_heap.gc_collections() > collections);
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_bool_native_quit_at_255_and_510_matches_tier0_flow_and_cadence() {
    let _settings = Settings::enter();
    // Set quit when the remaining count reaches 255. For n=510 this occurs
    // after the first due poll, so the second due poll must signal at 510.
    let source = function(
        vec![
            Op::True,
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(18),
            Op::StackRef(1),
            Op::Constant(1),
            Op::Eqlsign,
            Op::GotoIfNil(11),
            Op::True,
            Op::VarSet(2),
            Op::StackRef(0),
            Op::Not,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(1),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            Value::fixnum(0),
            Value::fixnum(255),
            Value::symbol("quit-flag"),
        ],
        1,
    );
    let enabled = boolean_plan(&source);
    assert!(bool_phi_count(&enabled) > 0);
    let leaf = lower(&source, &enabled);
    let mut ctx = Context::new();
    for n in [255, 510] {
        let args = [Value::fixnum(n)];
        ctx.set_quit_flag_value(Value::NIL);
        reset_bytecode_branch_poll_count();
        let expected = {
            let mut vm = Vm::from_context(&mut ctx);
            vm.force_interpreter_only_for_test();
            vm.execute(&source, args.to_vec())
                .expect_err("Tier-0 delayed quit")
        };
        let expected_polls = bytecode_branch_poll_count();
        ctx.set_quit_flag_value(Value::NIL);
        let expected = expected.as_signal().expect("Tier-0 quit signal");
        reset_bytecode_branch_poll_count();
        assert_eq!(
            leaf.call(&mut ctx as *mut Context as *mut u8, &args),
            NativeRun::Signal
        );
        ctx.set_quit_flag_value(Value::NIL);
        let actual = take_pending_flow().expect("native delayed quit");
        let actual = actual.as_signal().expect("native quit signal");
        assert_eq!(actual.symbol, expected.symbol);
        assert_eq!(actual.data, expected.data);
        assert_eq!(actual.raw_data, expected.raw_data);
        assert_eq!(bytecode_branch_poll_count(), expected_polls);
        assert_eq!(expected_polls, (n / 255) as usize);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

// Append to compile/tests/opt_bool.rs after the lead's current isolates finish.
// This bypasses the Boolean pass's first no-op seam: the one authorized typed
// producer is constructed directly so RED isolates shared native result resync.
#[test]
fn opt_bool_native_first_predicate_preserves_tagged_heap_residual() {
    let _settings = Settings::enter();
    struct RestoreFlonumMode;
    impl Drop for RestoreFlonumMode {
        fn drop(&mut self) {
            force_flonum_mode_for_test(None);
        }
    }
    let _restore_flonum_mode = RestoreFlonumMode;
    for mode in [FlonumMode::Resident, FlonumMode::Off] {
        force_flonum_mode_for_test(Some(mode));
        // Context comes first: symbol handles must belong to the active runtime.
        let mut ctx = Context::new();
        let source = function(
            vec![
                Op::Constant(0),
                Op::Constant(1),
                Op::Cons,
                Op::Dup,
                Op::Consp,
                Op::Pop,
                Op::Constant(2),
                Op::Call(0),
                Op::Pop,
                Op::Return,
            ],
            vec![
                Value::fixnum(7),
                Value::fixnum(9),
                Value::symbol("garbage-collect"),
            ],
            0,
        );
        let mut enabled = plan(&source);
        let predicate = enabled
            .insts
            .iter()
            .position(|inst| matches!(inst.op, ir::Opcode::Opaque(Op::Consp)))
            .expect("source has one total Consp predicate");
        let result = enabled.insts[predicate].result.expect("Consp has a result");
        enabled.insts[predicate].op = ir::Opcode::OpaqueBool(Op::Consp);
        enabled.values[result.index()].rep = ir::Rep::Bool;
        enabled
            .verify()
            .expect("exact typed producer with original source frames");

        let predicate_frame = enabled.insts[predicate].frame.expect("GNU predicate frame");
        let predicate_stack = &enabled.frames[predicate_frame.index()].stack;
        assert_eq!(predicate_stack.len(), 2);
        assert_eq!(
            enabled.resolve(predicate_stack[0]),
            enabled.resolve(predicate_stack[1])
        );
        assert_eq!(
            enabled.values[enabled.resolve(predicate_stack[0]).unwrap().index()].rep,
            ir::Rep::Tagged,
            "duplicated heap residual starts tagged"
        );
        assert!(
            !predicate_stack.iter().any(|value| {
                enabled.values[enabled.resolve(*value).unwrap().index()].rep == ir::Rep::Bool
            }),
            "the first Boolean producer has no preexisting flags; nonresident preparation returns keep=0"
        );

        // Freeze semantic answers as strings before any later collection. Neither
        // expected cell nor the native cell receives an external scratch root.
        let expected = crate::emacs_core::print::print_value(&tier0(&mut ctx, &source, &[]));
        let reference_value = reference(&mut ctx, &enabled, &[]);
        assert_eq!(
            crate::emacs_core::print::print_value(&reference_value),
            expected
        );
        // Before the fix this reaches I64-as-Bool materialization during native
        // lowering: the produced flag's rep must never replace the residual rep.
        let leaf = lower(&source, &enabled);
        let collections = ctx.tagged_heap.gc_collections();
        let actual = native(&mut ctx, &leaf, &[]);
        assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
        assert!(ctx.tagged_heap.gc_collections() > collections);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}
