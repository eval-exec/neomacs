//! Selected native Boolean results at recorded Float and generic numeric sites.
//! GNU inputs/flows are frozen in tmp/t34-o32-gnu-prep; semantic answers below
//! come from forced Tier-0 execution. Threading: contexts, roots and compiler
//! overrides are test-owned and restored; no new runtime state is introduced.

use super::compile_pipeline_tests::{captured_clif, function};
use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::{NumericFeedback, opt::ir};
use crate::emacs_core::print::print_value;

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            bool_rep: true,
            ..OptPasses::default()
        }));
        force_flonum_mode_for_test(Some(FlonumMode::OpLocal));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_flonum_mode_for_test(None);
        force_deopt_for_test(false);
        lowering::force_clif_dump_for_test(None);
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

fn comparisons() -> [Op; 5] {
    [Op::Eqlsign, Op::Lss, Op::Gtr, Op::Leq, Op::Geq]
}

fn branch_source(op: Op) -> ByteCodeFunction {
    function(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            op,
            Op::GotoIfNil(6),
            Op::Constant(0),
            Op::Return,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::T, Value::NIL],
        2,
    )
}

fn compile_recorded(
    ctx: &Context,
    source: &ByteCodeFunction,
    pcs: &[usize],
    feedback: NumericFeedback,
) -> (CompiledLeaf, String) {
    for &pc in pcs {
        source
            .jit_runtime()
            .record_numeric(pc, source.executable_ops().len(), feedback);
        assert_eq!(source.jit_runtime().numeric_feedback(pc), feedback);
    }
    let mut leaf = None;
    let clif = captured_clif(|| {
        leaf = Some(
            compile_bytecode_function_requested(
                source,
                Some(&ctx.obarray),
                CompileRequest {
                    regalloc: RegallocPolicy::Auto,
                    bypass_profit_gate: true,
                    origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
                    tier: crate::emacs_core::jit::tier2::CompileTier::Upgrade(
                        crate::emacs_core::jit::tier2::T2Upgrade::Feedback,
                    ),
                },
            )
            .expect("recorded numeric Boolean body compiles"),
        );
    });
    let leaf = leaf.expect("captured native leaf");
    assert_eq!(leaf.tier(), LeafTier::Opt);
    assert_eq!(
        crate::emacs_core::jit::compile::opt_census::snapshot(&leaf.obs)
            .opt_bool
            .expect("selected Bool census")
            .opaque_producers,
        pcs.len(),
        "every numeric producer must actually be OpaqueBool"
    );
    assert_eq!(clif.len(), 1, "one native Opt body must be emitted");
    (leaf, clif.into_iter().next().unwrap())
}

fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[Value]) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec())
}

fn native_ok(ctx: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    match leaf.call(ctx as *mut Context as *mut u8, args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("recorded numeric site must finish natively without deopt: {other:?}"),
    }
}

fn has_i8_phi_branch(clif: &str) -> bool {
    clif.lines().any(|line| {
        let line = line.trim();
        if !line.starts_with("block") {
            return false;
        }
        let Some((_, params)) = line.split_once('(') else {
            return false;
        };
        params.split(',').any(|param| {
            let Some((value, ty)) = param.split_once(':') else {
                return false;
            };
            ty.trim().starts_with("i8")
                && clif.lines().any(|branch| {
                    branch
                        .trim()
                        .starts_with(&format!("brif {},", value.trim()))
                })
        })
    })
}

fn i64_operation<'a>(operation: &'a str, opcode: &str) -> Option<&'a str> {
    let (name, args) = operation.split_once(' ')?;
    (name == opcode || name.strip_suffix(".i64") == Some(opcode)).then_some(args.trim())
}

fn zero_offset_base(operand: &str) -> Option<&str> {
    match operand.split_once('+') {
        Some((base, "0")) => Some(base),
        Some(_) => None,
        None => Some(operand),
    }
}

fn has_normalized_slow_result(clif: &str) -> bool {
    // These branch-only fixtures have one five-argument ArithGeneric call.
    // Trace its final output-pointer argument to the exact stack slot and
    // result load, rather than accepting an unrelated load or tag test.
    let operations = clif
        .lines()
        .filter_map(|line| line.split(';').next()?.trim().split_once(" = "))
        .collect::<Vec<_>>();
    let addresses = operations
        .iter()
        .filter_map(|&(value, op)| {
            let slot = zero_offset_base(i64_operation(op, "stack_addr")?)?;
            Some((value, slot))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let output_slots = operations
        .iter()
        .enumerate()
        .filter_map(|(index, &(_, op))| {
            let call = op.strip_prefix("call ")?;
            let (_, args) = call.split_once('(')?;
            let args = args
                .strip_suffix(')')?
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>();
            (args.len() == 5).then(|| addresses.get(args[4]).copied().map(|slot| (index, slot)))?
        })
        .collect::<Vec<_>>();
    let nil_values = operations
        .iter()
        .filter_map(|&(value, op)| {
            let bits = i64_operation(op, "iconst")?.parse::<i64>().ok()?;
            (bits == Value::NIL.bits() as i64).then_some(value)
        })
        .collect::<std::collections::HashSet<_>>();
    let results = operations
        .iter()
        .enumerate()
        .filter_map(|(index, &(value, op))| {
            let slot = if let Some(slot) = i64_operation(op, "stack_load") {
                zero_offset_base(slot)?
            } else {
                let load = i64_operation(op, "load")?;
                let address = zero_offset_base(load.split_whitespace().last()?)?;
                *addresses.get(address)?
            };
            output_slots
                .iter()
                .any(|&(call, output)| call < index && slot == output)
                .then_some(value)
        })
        .collect::<std::collections::HashSet<_>>();
    operations.iter().any(|&(_, op)| {
        let (args, immediate) = if let Some(args) = i64_operation(op, "icmp_imm") {
            (args, true)
        } else if let Some(args) = i64_operation(op, "icmp") {
            (args, false)
        } else {
            return false;
        };
        let Some(args) = args.strip_prefix("ne ") else {
            return false;
        };
        let Some((loaded, nil)) = args.split_once(',') else {
            return false;
        };
        results.contains(loaded.trim())
            && if immediate {
                nil.trim().parse::<i64>().ok() == Some(Value::NIL.bits() as i64)
            } else {
                nil_values.contains(nil.trim())
            }
    })
}

#[test]
fn opt_bool_numeric_float_feedback_branches_match_tier0_extremes_and_nan() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let mut pairs = Vec::new();
    for (integer, float) in [
        (9_007_199_254_740_993, 9_007_199_254_740_992.0),
        (9_007_199_254_740_991, 9_007_199_254_740_992.0),
        (-9_007_199_254_740_993, -9_007_199_254_740_992.0),
        (-9_007_199_254_740_991, -9_007_199_254_740_992.0),
        (
            Value::MOST_NEGATIVE_FIXNUM,
            Value::MOST_NEGATIVE_FIXNUM as f64,
        ),
        (
            Value::MOST_POSITIVE_FIXNUM,
            Value::MOST_POSITIVE_FIXNUM as f64,
        ),
    ] {
        let pair = [Value::make_int(integer), Value::make_float(float)];
        pairs.extend([pair, [pair[1], pair[0]]]);
    }
    for (a, b) in [
        (0.0, -0.0),
        (-0.0, 0.0),
        (f64::NAN, f64::NAN),
        (f64::NAN, 1.0),
        (1.0, f64::NAN),
        (f64::INFINITY, f64::INFINITY),
        (f64::INFINITY, f64::NEG_INFINITY),
        (f64::NEG_INFINITY, f64::INFINITY),
        (1.5, 2.5),
    ] {
        pairs.push([Value::make_float(a), Value::make_float(b)]);
    }
    for float in [-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let pair = [Value::fixnum(0), Value::make_float(float)];
        pairs.extend([pair, [pair[1], pair[0]]]);
    }
    pairs.extend([
        [Value::fixnum(-1), Value::fixnum(0)],
        [Value::fixnum(0), Value::fixnum(0)],
    ]);
    let rooted = pairs.iter().flatten().copied().collect::<Vec<_>>();
    let _roots = Roots::new(&rooted);
    for op in comparisons() {
        let source = branch_source(op.clone());
        let (leaf, clif) = compile_recorded(&ctx, &source, &[2], NumericFeedback::Float);
        assert!(
            clif.lines().any(|line| {
                line.trim().split_once(" = ").is_some_and(|(_, operation)| {
                    matches!(
                        operation.split_whitespace().next(),
                        Some("fcmp" | "fcmp.f64")
                    )
                })
            }),
            "recorded Float lowering must emit floating compares: {clif}"
        );
        assert!(
            has_i8_phi_branch(&clif),
            "Float result must reach its branch as I8: {clif}"
        );
        for args in &pairs {
            let expected = tier0(&mut ctx, &source, args).expect("Tier-0 numeric comparison");
            assert!(expected.is_t() || expected.is_nil());
            assert_eq!(
                native_ok(&mut ctx, &leaf, args),
                expected,
                "{op:?} on {args:?}"
            );
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
    }
}

fn cold_source(op: Op, wrong_on_right: bool) -> ByteCodeFunction {
    // Mirrors the frozen GNU direct cold source: the successful flag and heap
    // payload are stored before the second comparison. Its full pre-op stack
    // contains all four parameters and two aliases of the first Boolean flag.
    function(
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            op.clone(),
            Op::Constant(0),
            Op::StackRef(1),
            Op::StackRef(3),
            Op::List(3),
            Op::VarSet(1),
            Op::Dup,
            if wrong_on_right {
                Op::Constant(2)
            } else {
                Op::StackRef(5)
            },
            if wrong_on_right {
                Op::StackRef(6)
            } else {
                Op::Constant(2)
            },
            op,
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
    )
}

fn assert_complete_cold_frame(source: &ByteCodeFunction) {
    let cfg = analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        4,
    )
    .expect("cold CFG");
    let plan = opt_backend::build_plan(
        source.executable_ops(),
        &source.constants,
        &cfg,
        ir::ParamShape {
            required: 4,
            ..ir::ParamShape::default()
        },
        0,
        None,
    )
    .expect("the same selected Boolean builder verifies");
    let state = plan.source_states[11]
        .as_ref()
        .expect("original failing source state");
    let frame = &plan.frames[state.frame.index()];
    assert_eq!(frame.pc, 11);
    assert_eq!(frame.stack.as_ref(), state.pre.as_ref());
    assert_eq!(
        frame.stack.len(),
        8,
        "complete GNU operand stack, including original parameters"
    );
    for slot in 0..4 {
        let value = plan.resolve(frame.stack[slot]).unwrap();
        let ir::ValueDef::Inst(id) = plan.values[value.index()].def else {
            panic!("entry argument definition");
        };
        assert_eq!(plan.insts[id.index()].op, ir::Opcode::Arg(slot as u16));
    }
    assert_eq!(plan.resolve(frame.stack[4]), plan.resolve(frame.stack[5]));
    assert_eq!(
        plan.values[plan.resolve(frame.stack[4]).unwrap().index()].rep,
        ir::Rep::Bool
    );
    let comparison = plan
        .insts
        .iter()
        .find(|inst| inst.pc == 11 && matches!(inst.op, ir::Opcode::OpaqueBool(_)))
        .expect("original failing typed operation");
    assert_eq!(comparison.frame, Some(state.frame));
}

fn signal_summary(flow: &Flow) -> (SymId, Vec<String>, Option<String>) {
    let signal = flow.as_signal().expect("numeric failure must be a signal");
    (
        signal.symbol,
        signal.data.iter().map(print_value).collect(),
        signal.raw_data.as_ref().map(print_value),
    )
}

#[test]
fn opt_bool_numeric_generic_feedback_normalizes_success_and_preserves_signal_after_store() {
    let _settings = Settings::enter();
    let canonical = format!(
        "v48 = stack_addr.i64 ss1\n\
         v49 = call fn1(v38, v47, v27, v28, v48)\n\
         v51 = stack_addr.i64 ss1\n\
         v52 = load.i64 notrap v51\n\
         v53 = iconst.i64 {}\n\
         v54 = icmp ne v52, v53",
        Value::NIL.bits()
    );
    assert!(has_normalized_slow_result(&canonical));
    assert!(has_normalized_slow_result(
        &canonical.replace("icmp ne", "icmp.i64 ne")
    ));
    let compact = canonical
        .replace("load.i64 notrap v51", "stack_load.i64 ss1")
        .replace(
            "icmp ne v52, v53",
            &format!("icmp_imm.i64 ne v52, {}", Value::NIL.bits()),
        );
    assert!(has_normalized_slow_result(&compact));
    assert!(has_normalized_slow_result(
        &compact.replace("icmp_imm.i64", "icmp_imm")
    ));
    assert!(!has_normalized_slow_result(
        &canonical.replace("icmp ne v52, v53", "icmp ne v99, v53")
    ));
    assert!(!has_normalized_slow_result(&canonical.replace(
        &format!("iconst.i64 {}", Value::NIL.bits()),
        &format!("iconst.i64 {}", Value::T.bits())
    )));
    assert!(!has_normalized_slow_result(&canonical.replace(
        "v51 = stack_addr.i64 ss1",
        "v51 = stack_addr.i64 ss0"
    )));
    assert!(!has_normalized_slow_result(
        &canonical.replace("load.i64 notrap v51", "load.i32 notrap v51")
    ));
    assert!(!has_normalized_slow_result(
        &canonical.replace("icmp ne", "icmp eq")
    ));
    let mut ctx = Context::new();
    let _roots = Roots::new(&[]);
    ctx.eval_str("(setq t34-o32-side nil)").unwrap();
    let big = ctx.eval_str("(ash 1 80)").expect("GNU oracle bignum");
    push_scratch_gc_roots(&[big]);
    let negative_big = ctx.eval_str("(- (ash 1 80))").unwrap();
    push_scratch_gc_roots(&[negative_big]);
    let over_hi = ctx.eval_str("(1+ most-positive-fixnum)").unwrap();
    push_scratch_gc_roots(&[over_hi]);
    let under_lo = ctx.eval_str("(1- most-negative-fixnum)").unwrap();
    push_scratch_gc_roots(&[under_lo]);
    let marker = ctx
        .eval_str("(progn (insert \"abc\") (copy-marker 2))")
        .expect("positioned marker");
    push_scratch_gc_roots(&[marker]);
    let unset_marker = ctx.eval_str("(make-marker)").expect("unset marker");
    push_scratch_gc_roots(&[unset_marker]);
    let payload = Value::cons(Value::symbol("heap-companion"), Value::NIL);
    push_scratch_gc_roots(&[payload]);
    let mut pairs = vec![
        [big, big],
        [big, negative_big],
        [negative_big, big],
        [over_hi, Value::make_int(Value::MOST_POSITIVE_FIXNUM)],
        [under_lo, Value::make_int(Value::MOST_NEGATIVE_FIXNUM)],
        [big, Value::make_float(f64::NAN)],
        [Value::make_float(f64::NAN), big],
        [marker, Value::fixnum(2)],
        [Value::fixnum(2), marker],
        [marker, Value::make_float(2.0)],
        [Value::make_float(2.0), marker],
        [Value::fixnum(1), Value::fixnum(2)],
    ];
    // Both operand orders exercise the generic shim's exact integer policy.
    pairs.extend([[pairs[3][1], over_hi], [pairs[4][1], under_lo]]);
    let mut rooted = pairs.iter().flatten().copied().collect::<Vec<_>>();
    rooted.extend([unset_marker, payload]);
    push_scratch_gc_roots(&rooted);
    for op in comparisons() {
        let source = branch_source(op.clone());
        let (leaf, clif) = compile_recorded(&ctx, &source, &[2], NumericFeedback::Other);
        assert!(
            has_normalized_slow_result(&clif),
            "successful generic T/NIL must normalize after its result load: {clif}"
        );
        assert!(
            has_i8_phi_branch(&clif),
            "generic result must reach its branch as I8: {clif}"
        );
        for args in &pairs {
            let expected = tier0(&mut ctx, &source, args).expect("Tier-0 generic comparison");
            assert!(expected.is_t() || expected.is_nil());
            assert_eq!(
                native_ok(&mut ctx, &leaf, args),
                expected,
                "{op:?} on {args:?}"
            );
        }
        for wrong_on_right in [false, true] {
            let source = cold_source(op.clone(), wrong_on_right);
            assert_complete_cold_frame(&source);
            let (leaf, _) = compile_recorded(&ctx, &source, &[2, 11], NumericFeedback::Other);
            for bad in [Value::symbol("wrong"), unset_marker] {
                let args = [bad, Value::fixnum(1), Value::fixnum(2), payload];
                ctx.eval_str("(setq t34-o32-side nil)").unwrap();
                let expected_flow =
                    tier0(&mut ctx, &source, &args).expect_err("Tier-0 failure after store");
                let expected_signal = signal_summary(&expected_flow);
                let stored = ctx.eval_str("t34-o32-side").unwrap();
                let expected_store = print_value(&stored);
                assert_eq!(stored.cons_cdr().cons_cdr().cons_car(), payload);
                ctx.eval_str("(setq t34-o32-side nil)").unwrap();
                assert_eq!(
                    leaf.call(&mut ctx as *mut Context as *mut u8, &args),
                    NativeRun::Signal,
                    "generic failure must use STATUS_SIGNAL without deopt"
                );
                let actual_flow = take_pending_flow().expect("native generic signal");
                assert_eq!(signal_summary(&actual_flow), expected_signal);
                let stored = ctx.eval_str("t34-o32-side").unwrap();
                assert_eq!(print_value(&stored), expected_store);
                assert_eq!(stored.cons_cdr().cons_cdr().cons_car(), payload);
                assert!(
                    stored.cons_cdr().cons_car().is_t() || stored.cons_cdr().cons_car().is_nil()
                );
                assert_eq!(ctx.jit_root_stack_top, 0);
            }
        }
    }
}
