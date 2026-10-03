//! Differential tests compare independent SSA execution with Tier-0, including
//! an instrumented Tier-0 trace that snapshots every source operand stack.

use super::build::{BuildInput, build};
use super::eval::{Inputs, Outcome, evaluate};
use super::ir::{Func, ParamShape, ValueBits};
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{LambdaParams, Value};

fn function(ops: &[Op], constants: &[Value], shape: ParamShape) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (0..shape.required)
            .map(|_| intern("opt-reference-arg"))
            .collect(),
        optional: (0..shape.optional)
            .map(|_| intern("opt-reference-optional"))
            .collect(),
        rest: shape.has_rest.then(|| intern("opt-reference-rest")),
    });
    function.lexical = true;
    function.max_stack = 128;
    function.ops = ops.to_vec();
    function.constants = constants.to_vec().into();
    function
}

fn ir(function: &ByteCodeFunction, prefix: usize) -> Func {
    let shape = ParamShape {
        required: function.params.required.len(),
        optional: function.params.optional.len(),
        has_rest: function.params.rest.is_some(),
    };
    let cfg = crate::emacs_core::jit::compile::analyze_cfg(
        function.executable_ops(),
        &function.constants,
        function.executable_gnu_byte_offset_map(),
        shape.native_arity(),
    )
    .expect("source CFG");
    let constants = function
        .constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    build(BuildInput {
        ops: function.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: shape,
        dynamic_prefix: prefix,
        fused: None,
        osr: None,
    })
    .expect("builds optimizing IR")
}

fn tier0(ctx: &mut Context, function: &ByteCodeFunction, args: &[Value]) -> Value {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(function, args.to_vec())
        .expect("Tier-0 executes")
}

/// Inject recording bytecode before each source opcode. The recording copies
/// the full operand stack, prints it while rooted, and appends `(pc . text)` to
/// a normal Lisp variable. The real Tier-0 handles all operators and branches;
/// this is not a second hand-written stack interpreter.
fn traced(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    ir: &Func,
    args: &[Value],
) -> (Value, Vec<(u32, String)>) {
    let trace = Value::symbol("opt-reference-trace");
    ctx.eval_str("(setq opt-reference-trace nil)")
        .expect("trace variable");
    let mut output = function(&[], &source.constants, ir.arity);
    let trace_index = output.add_constant(trace);
    let printer = intern("prin1-to-string");
    let mut relocated = vec![0_u32; source.ops.len()];
    let mut original_sites = Vec::new();
    for (pc, op) in source.ops.iter().enumerate() {
        relocated[pc] = output.ops.len() as u32;
        if let Some(state) = &ir.source_states[pc] {
            let depth = state.pre.len();
            let index = output.add_constant(Value::fixnum(pc as i64));
            output.ops.push(Op::Constant(index));
            for _ in 0..depth {
                output.ops.push(Op::StackRef(depth as u16));
            }
            output.ops.extend([
                Op::List(depth as u16),
                Op::CallBuiltinSym(printer, 1),
                Op::Cons,
                Op::VarRef(trace_index),
                Op::Cons,
                Op::VarSet(trace_index),
            ]);
        }
        original_sites.push(output.ops.len());
        output.ops.push(op.clone());
    }
    for site in original_sites {
        match &mut output.ops[site] {
            Op::Goto(target)
            | Op::GotoIfNil(target)
            | Op::GotoIfNotNil(target)
            | Op::GotoIfNilElsePop(target)
            | Op::GotoIfNotNilElsePop(target) => {
                *target = relocated[*target as usize];
            }
            _ => (),
        }
    }
    // Keep the original tables and their printed values intact. Relocate the
    // resolver's GNU-byte-offset map, so Switch still uses real source keys.
    use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
    output.gnu_byte_offset_map = Some(match source.executable_gnu_byte_offset_map() {
        Some(map) => map
            .iter()
            .map(|entry| GnuByteOffsetMapEntry {
                byte_offset: entry.byte_offset,
                instruction_index: relocated[entry.instruction_index] as usize,
            })
            .collect(),
        None => relocated
            .iter()
            .enumerate()
            .map(|(byte_offset, &pc)| GnuByteOffsetMapEntry {
                byte_offset,
                instruction_index: pc as usize,
            })
            .collect(),
    });
    let result = tier0(ctx, &output, args);
    let mut trace_value = ctx.eval_str("opt-reference-trace").expect("read trace");
    let mut steps = Vec::new();
    while trace_value.is_cons() {
        let entry = trace_value.cons_car();
        let pc = entry.cons_car();
        let stack = entry.cons_cdr();
        steps.push((
            pc.as_fixnum().unwrap() as u32,
            stack.as_str_owned().unwrap(),
        ));
        trace_value = trace_value.cons_cdr();
    }
    steps.reverse();
    (result, steps)
}

fn compare(ctx: &mut Context, function: &ByteCodeFunction, args: &[Value]) -> Func {
    let roots_base = ctx.bc_buf.len();
    ctx.bc_buf.extend_from_slice(&function.constants);
    ctx.bc_buf.extend_from_slice(args);
    let ir = ir(function, 0);
    let expected = tier0(ctx, function, args);
    let expected = crate::emacs_core::print::print_value(&expected);
    let run = evaluate(
        &ir,
        ctx,
        Inputs {
            args,
            ..Inputs::default()
        },
    )
    .expect("IR evaluates");
    let Outcome::Returned(answer) = run.outcome else {
        panic!("unexpected deopt")
    };
    assert_eq!(
        crate::emacs_core::print::print_value(&answer.to_value()),
        expected
    );
    let (answer, trace) = traced(ctx, function, &ir, args);
    assert_eq!(crate::emacs_core::print::print_value(&answer), expected);
    let snapshots = run
        .trace
        .into_iter()
        .map(|frame| (frame.pc, frame.printed_stack))
        .collect::<Vec<_>>();
    assert_eq!(snapshots, trace, "exact pre-op GNU operand stacks");
    ctx.bc_buf.truncate(roots_base);
    ir
}

#[test]
fn opt_reference_arithmetic_matches_tier0_and_every_source_stack() {
    let mut ctx = Context::new();
    let function = function(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Add,
            Op::Add1,
            Op::Constant(2),
            Op::Mul,
            Op::Constant(1),
            Op::Sub,
            Op::Negate,
            Op::Constant(2),
            Op::Div,
            Op::Return,
        ],
        &[Value::fixnum(11), Value::fixnum(5), Value::fixnum(3)],
        ParamShape::default(),
    );
    compare(&mut ctx, &function, &[]);
}

#[test]
fn opt_reference_loop_phis_and_invariant_arguments_match_tier0() {
    let mut ctx = Context::new();
    let function = function(
        &[
            Op::Constant(0),
            Op::Constant(0),
            Op::StackRef(1),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(14),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Add,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::Goto(2),
            Op::Return,
        ],
        &[Value::fixnum(0)],
        ParamShape {
            required: 1,
            ..ParamShape::default()
        },
    );
    for n in [0, 1, 7, 16] {
        compare(&mut ctx, &function, &[Value::fixnum(n)]);
    }
}

#[test]
fn opt_reference_diamond_and_else_pop_edges_match_tier0() {
    let mut ctx = Context::new();
    let shape = ParamShape {
        required: 1,
        ..ParamShape::default()
    };
    let diamond = function(
        &[
            Op::GotoIfNil(3),
            Op::Constant(0),
            Op::Goto(4),
            Op::Constant(1),
            Op::Return,
        ],
        &[Value::fixnum(11), Value::fixnum(22)],
        shape,
    );
    let else_pop = function(
        &[Op::GotoIfNotNilElsePop(2), Op::Constant(0), Op::Return],
        &[Value::fixnum(22)],
        shape,
    );
    for arg in [Value::NIL, Value::T, Value::fixnum(0)] {
        compare(&mut ctx, &diamond, &[arg]);
        compare(&mut ctx, &else_pop, &[arg]);
    }
}

#[test]
fn opt_reference_cons_allocations_and_heap_writes_match_tier0() {
    let mut ctx = Context::new();
    let function = function(
        &[
            Op::Constant(0),
            Op::Nil,
            Op::Cons,
            Op::Dup,
            Op::Constant(1),
            Op::Setcar,
            Op::Pop,
            Op::Dup,
            Op::Constant(2),
            Op::Setcdr,
            Op::Pop,
            Op::Dup,
            Op::Car,
            Op::StackRef(1),
            Op::Cdr,
            Op::List(3),
            Op::Return,
        ],
        &[Value::fixnum(1), Value::fixnum(2), Value::fixnum(7)],
        ParamShape::default(),
    );
    compare(&mut ctx, &function, &[]);
}

#[test]
fn opt_reference_call_and_variable_store_match_tier0() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq opt-reference-cell 0)").unwrap();
    let function = function(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::VarSet(2),
            Op::VarRef(2),
            Op::Return,
        ],
        &[
            Value::symbol("identity"),
            Value::fixnum(19),
            Value::symbol("opt-reference-cell"),
        ],
        ParamShape::default(),
    );
    compare(&mut ctx, &function, &[]);
}

#[test]
fn opt_reference_float_allocation_identity_matches_tier0() {
    let mut ctx = Context::new();
    let function = function(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::Dup,
            Op::Dup,
            Op::Eq,
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::StackRef(2),
            Op::Eq,
            Op::List(3),
            Op::Return,
        ],
        &[Value::make_float(1.25), Value::make_float(-0.0)],
        ParamShape::default(),
    );
    compare(&mut ctx, &function, &[]);
}

#[test]
fn opt_reference_normalized_optional_and_rest_slots_match_tier0() {
    let mut ctx = Context::new();
    let shape = ParamShape {
        required: 1,
        optional: 1,
        has_rest: true,
    };
    let function = function(&[Op::List(3), Op::Return], &[], shape);
    for args in [
        vec![Value::fixnum(1)],
        vec![Value::fixnum(1), Value::fixnum(2)],
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(3),
            Value::fixnum(4),
        ],
    ] {
        let mut slots = args.iter().take(2).copied().collect::<Vec<_>>();
        slots.resize(2, Value::NIL);
        slots.push(
            ctx.tagged_heap
                .list_from_slice(args.get(2..).unwrap_or_default()),
        );
        let roots_base = ctx.bc_buf.len();
        ctx.bc_buf.extend_from_slice(&slots);
        let ir = ir(&function, 0);
        let expected = tier0(&mut ctx, &function, &args);
        let expected = crate::emacs_core::print::print_value(&expected);
        let run = evaluate(
            &ir,
            &mut ctx,
            Inputs {
                args: &slots,
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Returned(answer) = run.outcome else {
            panic!("unexpected deopt")
        };
        assert_eq!(
            crate::emacs_core::print::print_value(&answer.to_value()),
            expected
        );
        let (_, trace) = traced(&mut ctx, &function, &ir, &args);
        assert_eq!(
            run.trace
                .into_iter()
                .map(|s| (s.pc, s.printed_stack))
                .collect::<Vec<_>>(),
            trace
        );
        ctx.bc_buf.truncate(roots_base);
    }
}

#[test]
fn opt_reference_patched_prefix_is_read_from_the_current_instance() {
    let mut ctx = Context::new();
    let source = function(
        &[Op::Constant(0), Op::Return],
        &[Value::NIL],
        ParamShape::default(),
    );
    let ir = ir(&source, 1);
    for captured in [Value::fixnum(11), Value::fixnum(22)] {
        let mut actual = source.clone();
        actual.constants.ensure_owned()[0] = captured;
        let expected = tier0(&mut ctx, &actual, &[]);
        let run = evaluate(
            &ir,
            &mut ctx,
            Inputs {
                prefix: &[captured],
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Returned(answer) = run.outcome else {
            panic!("unexpected deopt")
        };
        assert_eq!(answer.to_value(), expected);
    }
}

#[test]
fn opt_reference_switch_resolves_gnu_offsets_and_equal_keys_like_tier0() {
    use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
    use crate::emacs_core::value::HashTableTest;

    let mut ctx = Context::new();
    let table = Value::hash_table(HashTableTest::Equal);
    let symbol = Value::symbol("opt-reference-case");
    let list = Value::list(vec![symbol, Value::fixnum(7)]);
    table
        .with_hash_table_mut(|ht| {
            ht.insert(symbol.to_hash_key(&ht.test), symbol, Value::fixnum(50));
            ht.insert(list.to_hash_key(&ht.test), list, Value::fixnum(70));
        })
        .unwrap();
    let mut function = function(
        &[
            Op::StackRef(0),
            Op::Constant(0),
            Op::Switch,
            Op::Constant(1),
            Op::Return,
            Op::Constant(2),
            Op::Return,
            Op::Constant(3),
            Op::Return,
        ],
        &[
            table,
            Value::fixnum(-1),
            Value::fixnum(10),
            Value::fixnum(20),
        ],
        ParamShape {
            required: 1,
            ..ParamShape::default()
        },
    );
    function.gnu_byte_offset_map = Some(
        [0, 1, 2, 30, 40, 50, 60, 70, 80]
            .into_iter()
            .enumerate()
            .map(|(instruction_index, byte_offset)| GnuByteOffsetMapEntry {
                byte_offset,
                instruction_index,
            })
            .collect(),
    );
    for argument in [
        symbol,
        Value::list(vec![symbol, Value::fixnum(7)]),
        Value::fixnum(7),
        Value::NIL,
    ] {
        compare(&mut ctx, &function, &[argument]);
    }
}

#[test]
fn opt_reference_gc_keeps_full_frame_and_vector_store_result_like_tier0() {
    let mut ctx = Context::new();
    // GNU garbage-collect returns an allocation census. Return nil from a
    // wrapper so instrumentation allocations do not alter the observed stack.
    let collect = ctx.eval_str("(lambda () (garbage-collect) nil)").unwrap();
    let function = function(
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Call(2),
            Op::Dup,
            Op::Constant(3),
            Op::Constant(4),
            Op::Aset,
            Op::Pop,
            Op::Constant(5),
            Op::Call(0),
            Op::Pop,
            Op::Dup,
            Op::Constant(3),
            Op::Aref,
            Op::List(2),
            Op::Return,
        ],
        &[
            Value::symbol("vector"),
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(0),
            Value::fixnum(9),
            collect,
        ],
        ParamShape::default(),
    );
    compare(&mut ctx, &function, &[]);
}

#[test]
fn opt_reference_failed_type_guard_reads_exact_pre_operation_frame() {
    let mut ctx = Context::new();
    let function = function(
        &[Op::StackRef(0), Op::Car, Op::Return],
        &[],
        ParamShape {
            required: 1,
            ..ParamShape::default()
        },
    );
    let function_ir = ir(&function, 0);
    let argument = Value::fixnum(7);
    let run = evaluate(
        &function_ir,
        &mut ctx,
        Inputs {
            args: &[argument],
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(frame) = run.outcome else {
        panic!("type guard must fail")
    };
    assert_eq!(frame.pc, 1);
    assert_eq!(frame.stack, vec![ValueBits::from_value(argument); 2]);
    let mut stopped = function.clone();
    stopped.ops = vec![Op::StackRef(0), Op::Return];
    let stopped_ir = ir(&stopped, 0);
    let (_, trace) = traced(&mut ctx, &stopped, &stopped_ir, &[argument]);
    assert_eq!((frame.pc, frame.printed_stack), trace[1]);
}

#[test]
fn opt_reference_seeded_stack_programs_match_tier0() {
    let mut ctx = Context::new();
    for seed in 1_u64..=32 {
        let mut state = seed;
        let mut depth = 1;
        let mut ops = Vec::new();
        for _ in 0..24 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let op = match state % 7 {
                0 => {
                    depth += 1;
                    Op::Constant((state % 4) as u16)
                }
                1 if depth > 1 => {
                    depth -= 1;
                    Op::Add
                }
                2 if depth > 1 => {
                    depth -= 1;
                    Op::Sub
                }
                3 => Op::Negate,
                4 if depth > 1 => {
                    depth -= 1;
                    Op::StackSet(1)
                }
                5 if depth > 1 => {
                    depth -= 1;
                    Op::Pop
                }
                _ => {
                    depth += 1;
                    Op::StackRef((state as usize % (depth - 1)) as u16)
                }
            };
            ops.push(op);
        }
        ops.push(Op::Return);
        let source = function(
            &ops,
            &[
                Value::fixnum(-7),
                Value::fixnum(-1),
                Value::fixnum(0),
                Value::fixnum(3),
            ],
            ParamShape {
                required: 1,
                ..ParamShape::default()
            },
        );
        tracing::debug!(seed, "opt reference differential stack program");
        compare(&mut ctx, &source, &[Value::fixnum(seed as i64)]);
    }
}

#[test]
fn opt_reference_poll_cadence_matches_tier0_backedges() {
    let mut ctx = Context::new();
    let source = function(
        &[
            Op::Dup,
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(8),
            Op::Sub1,
            Op::Goto(0),
            Op::Nil,
            Op::Return,
            Op::Return,
        ],
        &[Value::fixnum(0)],
        ParamShape {
            required: 1,
            ..ParamShape::default()
        },
    );
    let ir = ir(&source, 0);
    for n in [254, 255, 256, 510] {
        crate::emacs_core::eval::reset_bytecode_branch_poll_count();
        let expected = tier0(&mut ctx, &source, &[Value::fixnum(n)]);
        let expected_polls = crate::emacs_core::eval::bytecode_branch_poll_count();
        crate::emacs_core::eval::reset_bytecode_branch_poll_count();
        let run = evaluate(
            &ir,
            &mut ctx,
            Inputs {
                args: &[Value::fixnum(n)],
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Returned(answer) = run.outcome else {
            panic!("unexpected deopt")
        };
        assert_eq!(answer.to_value(), expected);
        assert_eq!(
            crate::emacs_core::eval::bytecode_branch_poll_count(),
            expected_polls
        );
    }
}

fn typed_emit(
    func: &mut Func,
    op: super::ir::Opcode,
    args: Vec<super::ir::Value>,
    ty: super::types::TypeSet,
    rep: super::ir::Rep,
) -> super::ir::Value {
    use super::ir::{Inst, InstData, Value as IrValue, ValueData, ValueDef};
    use super::mem::{AliasClass, Effects};
    let eff = if matches!(
        op,
        super::ir::Opcode::AllocFloat | super::ir::Opcode::AllocCons
    ) {
        Effects::ALLOCATES
    } else {
        Effects::PURE
    };
    let inst = Inst(func.insts.len() as u32);
    let value = IrValue(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(inst),
    });
    func.insts.push(InstData {
        op,
        args,
        result: Some(value),
        eff,
        mem: AliasClass::None,
        frame: None,
        pc: 0,
    });
    func.blocks[0].insts.push(inst);
    value
}

#[test]
fn opt_reference_typed_checked_fixnum_operations_deopt_at_the_original_stack() {
    use super::ir::{Opcode, Rep};
    use super::types::TypeSet;
    let mut ctx = Context::new();
    let cases = [
        (
            Op::Add,
            Opcode::FixAdd { checked: true },
            1,
            Value::MOST_POSITIVE_FIXNUM,
        ),
        (
            Op::Sub,
            Opcode::FixSub { checked: true },
            1,
            Value::MOST_NEGATIVE_FIXNUM,
        ),
        (
            Op::Mul,
            Opcode::FixMul { checked: true },
            2,
            Value::MOST_POSITIVE_FIXNUM,
        ),
        (Op::Div, Opcode::FixDiv, -1, Value::MOST_NEGATIVE_FIXNUM),
        (Op::Rem, Opcode::FixRem, 0, 17),
    ];
    for (source_op, typed_op, constant, overflow_arg) in cases {
        let source = function(
            &[Op::Constant(0), source_op.clone(), Op::Return],
            &[Value::fixnum(constant)],
            ParamShape {
                required: 1,
                ..ParamShape::default()
            },
        );
        let mut func = ir(&source, 0);
        for data in &mut func.values {
            data.ty = TypeSet::FIXNUM;
            data.rep = Rep::TaggedFix;
        }
        for inst in &mut func.insts {
            if matches!(&inst.op, Opcode::Opaque(op) if op == &source_op) {
                inst.op = typed_op.clone();
                inst.eff = inst.eff.with(super::mem::Effects::MAY_DEOPT);
            }
        }
        func.verify().expect("typed IR verifies");
        let argument = Value::fixnum(overflow_arg);
        let run = evaluate(
            &func,
            &mut ctx,
            Inputs {
                args: &[argument],
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Deopt(frame) = run.outcome else {
            panic!("checked operation must deopt")
        };
        assert_eq!(frame.pc, 1);
        assert_eq!(
            frame.stack,
            [
                ValueBits::from_value(argument),
                ValueBits::from_value(Value::fixnum(constant))
            ]
        );
        if constant != 0 {
            let argument = Value::fixnum(11);
            let expected = tier0(&mut ctx, &source, &[argument]);
            let run = evaluate(
                &func,
                &mut ctx,
                Inputs {
                    args: &[argument],
                    ..Inputs::default()
                },
            )
            .unwrap();
            let Outcome::Returned(answer) = run.outcome else {
                panic!("in-range operation")
            };
            assert_eq!(answer.to_value(), expected);
        }
    }
}

#[test]
fn opt_reference_typed_float_negation_and_nan_comparison_match_tier0_bits() {
    use super::ir::{BlockData, Cmp, Opcode, Rep, Term};
    use super::types::TypeSet;
    let mut ctx = Context::new();
    let mut func = Func::new(
        Box::new([]),
        ParamShape {
            required: 2,
            ..ParamShape::default()
        },
        0,
    );
    func.blocks.push(BlockData::new(0));
    let a = typed_emit(
        &mut func,
        Opcode::Arg(0),
        vec![],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    let b = typed_emit(
        &mut func,
        Opcode::Arg(1),
        vec![],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    let raw_a = typed_emit(
        &mut func,
        Opcode::UnboxF64,
        vec![a],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let raw_b = typed_emit(
        &mut func,
        Opcode::UnboxF64,
        vec![b],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let product = typed_emit(
        &mut func,
        Opcode::F64Mul,
        vec![raw_a, raw_b],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let negated = typed_emit(
        &mut func,
        Opcode::F64Neg,
        vec![product],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let result = typed_emit(
        &mut func,
        Opcode::AllocFloat,
        vec![negated],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    func.blocks[0].term = Term::Return(result);
    func.verify().expect("raw float IR verifies");
    let source = function(
        &[
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Mul,
            Op::Negate,
            Op::Return,
        ],
        &[],
        func.arity,
    );
    for (a, b) in [
        (1.25, -0.0),
        (-1.25, 0.0),
        (f64::from_bits(0xfff8_0000_0000_0042), -1.0),
        (f64::INFINITY, 0.0),
    ] {
        let args = [Value::make_float(a), Value::make_float(b)];
        let root_base = ctx.bc_buf.len();
        ctx.bc_buf.extend_from_slice(&args);
        let expected = tier0(&mut ctx, &source, &args).xfloat().to_bits();
        let run = evaluate(
            &func,
            &mut ctx,
            Inputs {
                args: &args,
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Returned(answer) = run.outcome else {
            panic!("raw float result")
        };
        assert_eq!(answer.to_value().xfloat().to_bits(), expected);
        ctx.bc_buf.truncate(root_base);
    }
    let mut cmp = func.clone();
    cmp.insts.truncate(4);
    cmp.values.truncate(4);
    cmp.blocks[0].insts.truncate(4);
    let flag = typed_emit(
        &mut cmp,
        Opcode::F64Cmp(Cmp::Lt),
        vec![raw_a, raw_b],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let result = typed_emit(
        &mut cmp,
        Opcode::BoolToLisp,
        vec![flag],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    cmp.blocks[0].term = Term::Return(result);
    cmp.verify().expect("float comparison IR verifies");
    let source = function(
        &[Op::StackRef(1), Op::StackRef(1), Op::Lss, Op::Return],
        &[],
        cmp.arity,
    );
    for pair in [(f64::NAN, 1.0), (1.0, f64::NAN), (-0.0, 0.0), (1.0, 2.0)] {
        let args = [Value::make_float(pair.0), Value::make_float(pair.1)];
        let root_base = ctx.bc_buf.len();
        ctx.bc_buf.extend_from_slice(&args);
        let expected = tier0(&mut ctx, &source, &args);
        let run = evaluate(
            &cmp,
            &mut ctx,
            Inputs {
                args: &args,
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Returned(answer) = run.outcome else {
            panic!("float comparison result")
        };
        assert_eq!(answer.to_value(), expected);
        ctx.bc_buf.truncate(root_base);
    }
}

#[test]
fn opt_reference_typed_heap_and_symbol_operations_match_tier0() {
    use super::ir::Opcode;
    let mut ctx = Context::new();
    ctx.eval_str("(setq opt-reference-cell 0)").unwrap();
    let symbol = intern("opt-reference-cell");
    let source = function(
        &[
            Op::Constant(0),
            Op::Nil,
            Op::Cons,
            Op::Dup,
            Op::Constant(1),
            Op::Setcar,
            Op::VarSet(3),
            Op::Dup,
            Op::Constant(2),
            Op::Setcdr,
            Op::Pop,
            Op::Dup,
            Op::Car,
            Op::StackRef(1),
            Op::Cdr,
            Op::VarRef(3),
            Op::List(4),
            Op::Return,
        ],
        &[
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(7),
            Value::from_sym_id(symbol),
        ],
        ParamShape::default(),
    );
    let mut func = ir(&source, 0);
    for inst in &mut func.insts {
        inst.op = match &inst.op {
            Opcode::Opaque(Op::Cons) => Opcode::AllocCons,
            Opcode::Opaque(Op::Car) => Opcode::LoadCar,
            Opcode::Opaque(Op::Cdr) => Opcode::LoadCdr,
            Opcode::Opaque(Op::Setcar) => Opcode::StoreCar,
            Opcode::Opaque(Op::Setcdr) => Opcode::StoreCdr,
            Opcode::Opaque(Op::VarRef(_)) => Opcode::LoadSymValue(symbol),
            Opcode::Opaque(Op::VarSet(_)) => Opcode::StoreSymValue(symbol),
            Opcode::Opaque(op) => Opcode::Builtin(op.clone()),
            other => other.clone(),
        };
    }
    func.verify().expect("typed heap IR verifies");
    let expected = crate::emacs_core::print::print_value(&tier0(&mut ctx, &source, &[]));
    let run = evaluate(&func, &mut ctx, Inputs::default()).unwrap();
    let Outcome::Returned(answer) = run.outcome else {
        panic!("typed heap result")
    };
    assert_eq!(
        crate::emacs_core::print::print_value(&answer.to_value()),
        expected
    );
    let (_, trace) = traced(&mut ctx, &source, &func, &[]);
    assert_eq!(
        run.trace
            .into_iter()
            .map(|s| (s.pc, s.printed_stack))
            .collect::<Vec<_>>(),
        trace
    );
}

#[test]
fn opt_reference_extent_operators_are_rejected_without_persistent_frames() {
    use super::eval::EvalError;
    let mut ctx = Context::new();
    let source = function(
        &[Op::SaveCurrentBuffer, Op::Nil, Op::Unbind(1), Op::Return],
        &[],
        ParamShape::default(),
    );
    let func = ir(&source, 0);
    assert!(
        matches!(evaluate(&func, &mut ctx, Inputs::default()), Err(EvalError::Invalid(reason)) if reason.contains("persistent interpreter frame"))
    );
}

#[test]
fn opt_reference_seeded_loop_diamonds_match_tier0_every_stack() {
    let mut ctx = Context::new();
    for seed in 1_u64..=16 {
        let mut state = seed;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let source = function(
            &[
                Op::Constant(0),
                Op::Constant(0),
                Op::StackRef(1),
                Op::StackRef(3),
                Op::Lss,
                Op::GotoIfNil(23),
                Op::StackRef(1),
                Op::Constant(1),
                Op::Rem,
                Op::GotoIfNil(15),
                Op::StackRef(0),
                Op::Constant(2),
                Op::Add,
                Op::StackSet(1),
                Op::Goto(19),
                Op::StackRef(0),
                Op::Constant(3),
                Op::Sub,
                Op::StackSet(1),
                Op::StackRef(1),
                Op::Add1,
                Op::StackSet(2),
                Op::Goto(2),
                Op::Return,
            ],
            &[
                Value::fixnum(0),
                Value::fixnum(2),
                Value::fixnum((state % 13) as i64),
                Value::fixnum(((state >> 8) % 11) as i64),
            ],
            ParamShape {
                required: 1,
                ..ParamShape::default()
            },
        );
        tracing::debug!(seed, "opt reference differential loop diamond");
        compare(&mut ctx, &source, &[Value::fixnum((state % 18) as i64)]);
    }
}
