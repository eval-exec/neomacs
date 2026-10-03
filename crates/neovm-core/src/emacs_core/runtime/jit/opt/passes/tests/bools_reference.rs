//! Bool execution delegates Lisp semantics to the real Tier-0 opcode routine.
//! Threading: every plan, context and temporary root belongs to this invocation;
//! these tests introduce no runtime caches or compiler-worker Lisp ownership.

use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::{
    Context, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::{build, eval, ir, mem, types::TypeSet};
use crate::emacs_core::value::{LambdaParams, Value};

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

fn source(op: Op, arity: usize) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|_| crate::emacs_core::intern::intern("opt-bool-reference-arg"))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.max_stack = 16;
    function.ops = if arity == 1 {
        vec![Op::StackRef(0), op, Op::Return]
    } else {
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return]
    };
    function.seal_hand_assembled_ops_for_test();
    function
}

fn plan(source: &ByteCodeFunction) -> ir::Func {
    let arity = source.params.required.len();
    let cfg = crate::emacs_core::jit::compile::analyze_cfg(
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

/// The result's source identities stay intact; only its return gets a tagged
/// view. This constructs OpaqueBool directly so evaluator tests do not depend
/// on the pass solver or on native result projection.
fn promote_return(plan: &mut ir::Func, op: &Op) {
    let index = plan
        .insts
        .iter()
        .position(|inst| matches!(&inst.op, ir::Opcode::Opaque(found) if found == op))
        .expect("original producer");
    let original = plan.insts[index].result.expect("producer result");
    plan.insts[index].op = ir::Opcode::OpaqueBool(op.clone());
    plan.values[original.index()].rep = ir::Rep::Bool;
    plan.values[original.index()].ty = TypeSet::BOOLEAN;
    let conversion = ir::Inst(plan.insts.len() as u32);
    let tagged = ir::Value(plan.values.len() as u32);
    plan.values.push(ir::ValueData {
        def: ir::ValueDef::Inst(conversion),
        rep: ir::Rep::Tagged,
        ty: TypeSet::BOOLEAN,
    });
    plan.insts.push(ir::InstData {
        op: ir::Opcode::BoolToLisp,
        args: vec![original],
        result: Some(tagged),
        eff: mem::Effects::PURE,
        mem: mem::AliasClass::None,
        frame: None,
        pc: plan.insts[index].pc,
    });
    let block = plan
        .blocks
        .iter_mut()
        .find(|block| block.insts.contains(&ir::Inst(index as u32)))
        .expect("producer block");
    block.insts.push(conversion);
    assert!(matches!(block.term, ir::Term::Return(value) if value == original));
    block.term = ir::Term::Return(tagged);
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
    .expect("reference Boolean execution");
    let eval::Outcome::Returned(value) = run.outcome else {
        panic!("Boolean reference must return")
    };
    value.to_value()
}

fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[Value]) -> Value {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec()).expect("Tier-0 return")
}

#[test]
fn opt_bool_reference_predicates_use_tier0_and_exact_t_nil() {
    let mut ctx = Context::new();
    let inputs = [
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::make_float(-0.0),
        Value::cons(Value::fixnum(1), Value::NIL),
        ctx.eval_str("\"text\"").unwrap(),
        ctx.eval_str("[1]").unwrap(),
        Value::symbol("opt-bool-symbol"),
    ];
    let _roots = Roots::new(&inputs);
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
        let source = source(op.clone(), 1);
        let mut boolean = plan(&source);
        promote_return(&mut boolean, &op);
        boolean.verify().expect("Boolean predicate IR");
        for argument in inputs {
            let expected = tier0(&mut ctx, &source, &[argument]);
            assert!(expected.is_t() || expected.is_nil());
            assert_eq!(
                reference(&mut ctx, &boolean, &[argument]),
                expected,
                "{op:?}"
            );
        }
    }
}

#[test]
fn opt_bool_reference_numeric_and_identity_results_match_tier0() {
    let mut ctx = Context::new();
    let float = Value::make_float(-0.0);
    let nan = Value::make_float(f64::NAN);
    let upper = Value::make_float(9_007_199_254_740_992.0);
    let pairs = [
        [
            Value::fixnum(Value::MOST_NEGATIVE_FIXNUM),
            Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
        ],
        [Value::fixnum(0), Value::fixnum(0)],
        [float, Value::fixnum(0)],
        [Value::fixnum(0), float],
        [nan, Value::fixnum(0)],
        [Value::fixnum(0), nan],
        [Value::fixnum(9_007_199_254_740_993), upper],
        [upper, Value::fixnum(9_007_199_254_740_993)],
    ];
    let roots = pairs.iter().flatten().copied().collect::<Vec<_>>();
    let _roots = Roots::new(&roots);
    for op in [Op::Eq, Op::Eqlsign, Op::Lss, Op::Gtr, Op::Leq, Op::Geq] {
        let source = source(op.clone(), 2);
        let mut boolean = plan(&source);
        promote_return(&mut boolean, &op);
        boolean.verify().expect("Boolean comparison IR");
        for args in pairs {
            let expected = tier0(&mut ctx, &source, &args);
            assert!(expected.is_t() || expected.is_nil());
            assert_eq!(reference(&mut ctx, &boolean, &args), expected, "{op:?}");
        }
    }
}

#[test]
fn opt_bool_reference_authoritative_opaque_opcode_controls_semantics() {
    let mut ctx = Context::new();
    let original = source(Op::Consp, 1);
    let expected_source = source(Op::Not, 1);
    let mut changed = plan(&original);
    promote_return(&mut changed, &Op::Consp);
    let inst = changed
        .insts
        .iter_mut()
        .find(|inst| matches!(inst.op, ir::Opcode::OpaqueBool(Op::Consp)))
        .unwrap();
    inst.op = ir::Opcode::OpaqueBool(Op::Not);
    changed.verify().expect("changed authoritative Bool opcode");
    for argument in [Value::NIL, Value::fixnum(0)] {
        let expected = tier0(&mut ctx, &expected_source, &[argument]);
        assert_eq!(reference(&mut ctx, &changed, &[argument]), expected);
    }
}

#[test]
fn opt_bool_reference_rejects_non_boolean_success_without_truthiness_shortcut() {
    let mut ctx = Context::new();
    let source = source(Op::Add, 2);
    let mut malformed = plan(&source);
    promote_return(&mut malformed, &Op::Add);
    assert!(
        malformed.verify().is_err(),
        "Add is outside the Bool whitelist"
    );
    let args = [Value::fixnum(0), Value::fixnum(0)];
    let error = eval::evaluate(
        &malformed,
        &mut ctx,
        eval::Inputs {
            args: &args,
            ..eval::Inputs::default()
        },
    )
    .expect_err("successful numeric zero must not become Lisp NIL");
    assert!(matches!(error, eval::EvalError::Invalid(message) if message.contains("non-Boolean")));
}
