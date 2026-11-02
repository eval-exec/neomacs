//! Dynamic operands through both VM monomorphizations and the native/deopt
//! path, compared to captured GNU31.1 results. See the worktree's
//! tmp/vmcall/arithmetic-shape-oracles/provenance.md for the capture command.
//! Keep this module separate from the lever-5 verifier corpus.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::decode::parse_arglist_descriptor;
use crate::emacs_core::error::{EvalResult, FlowKind};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use std::collections::BTreeMap;

struct Case<'a> {
    id: &'a str,
    operation: &'a str,
    left: &'a str,
    right: &'a str,
    expected: &'a str,
}

fn cases() -> Vec<Case<'static>> {
    let expected: BTreeMap<_, _> = include_str!("arithmetic_shape_oracle/gnu.expect.tsv")
        .lines()
        .map(|line| line.split_once('\t').expect("GNU result has a case id"))
        .collect();
    let cases: Vec<_> = include_str!("arithmetic_shape_oracle/cases.tsv")
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 4);
            Case {
                id: fields[0],
                operation: fields[1],
                left: fields[2],
                right: fields[3],
                expected: expected[fields[0]],
            }
        })
        .collect();
    assert_eq!(cases.len(), 482);
    assert_eq!(expected.len(), cases.len());
    cases
}

fn opcode(name: &str) -> Op {
    match name {
        "add1" => Op::Add1,
        "sub1" => Op::Sub1,
        "add" => Op::Add,
        "sub" => Op::Sub,
        "eq" => Op::Eqlsign,
        "lt" => Op::Lss,
        "gt" => Op::Gtr,
        "le" => Op::Leq,
        "ge" => Op::Geq,
        _ => panic!("unknown fixture operation {name}"),
    }
}

fn function(name: &str, verified: bool) -> ByteCodeFunction {
    let nargs = usize::from(!matches!(name, "add1" | "sub1")) + 1;
    let descriptor = (nargs | (nargs << 8)) as i64;
    let mut function = ByteCodeFunction::new(parse_arglist_descriptor(descriptor));
    function.ops = vec![opcode(name), Op::Return];
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(nargs);
    function.seal_hand_assembled_ops_for_test();
    assert!(function.executes_sealed_ops());
    assert!(
        function.executes_verified_ops(),
        "the real verifier admits this body"
    );
    if !verified {
        // Checked execution still requires normalized, sealed instructions.
        // Conservatively revoke only the stack proof, as existing checked
        // fixtures do; never manufacture a proof for unverified code.
        function.stack_verified = false;
    }
    assert_eq!(function.executes_verified_ops(), verified);
    function
}

fn operand(ctx: &mut Context, token: &str) -> Value {
    match token {
        "max" => Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
        "min" => Value::fixnum(Value::MOST_NEGATIVE_FIXNUM),
        "maxm1" => Value::fixnum(Value::MOST_POSITIVE_FIXNUM - 1),
        "minp1" => Value::fixnum(Value::MOST_NEGATIVE_FIXNUM + 1),
        "nil" => Value::NIL,
        "t" => Value::T,
        "symbol" => Value::symbol("vmcall-shape-symbol"),
        "positioned" => ctx
            .tagged_heap
            .alloc_symbol_with_pos(Value::symbol("vmcall-shape-symbol"), Value::fixnum(17)),
        "string" => Value::string("shape"),
        "cons" => Value::cons(Value::fixnum(4), Value::fixnum(5)),
        "vector" => Value::vector(vec![Value::fixnum(4), Value::fixnum(5)]),
        "float" => Value::make_float(0.5),
        "big" => Value::make_int(Value::MOST_POSITIVE_FIXNUM + 1),
        "marker" => ctx.eval_str("(point-marker)").expect("point marker"),
        number => Value::fixnum(number.parse().expect("fixture fixnum")),
    }
}

fn arguments(ctx: &mut Context, case: &Case<'_>) -> Vec<Value> {
    let left = operand(ctx, case.left);
    ctx.push_vm_frame_root(left);
    let mut args = vec![left];
    if case.right != "-" {
        let right = operand(ctx, case.right);
        ctx.push_vm_frame_root(right);
        args.push(right);
    }
    args
}

fn answer(result: EvalResult) -> String {
    match result {
        Ok(value) => {
            let kind = if value.is_fixnum() {
                "fixnum"
            } else if value.is_bignum() {
                "bignum"
            } else if value.is_float() {
                "float"
            } else if value.is_nil() {
                "nil"
            } else if value.is_t() {
                "t"
            } else {
                panic!("unexpected arithmetic result {}", print_value(&value));
            };
            format!("ok:{kind}:{}", print_value(&value))
        }
        Err(flow) => match flow.into_kind() {
            FlowKind::Signal(signal) => format!(
                "error:{}:{}",
                signal.symbol_name(),
                print_value(signal.data.first().expect("wrong-type predicate")),
            ),
            other => panic!("unexpected arithmetic flow {other:?}"),
        },
    }
}

fn assert_frontiers(ctx: &Context, specpdl_base: usize, case: &Case<'_>) {
    assert!(ctx.bc_buf.is_empty(), "{}: operand frontier", case.id);
    assert!(ctx.bc_frames.is_empty(), "{}: frame frontier", case.id);
    assert_eq!(ctx.specpdl.len(), specpdl_base, "{}: specpdl", case.id);
    assert_eq!(ctx.depth, 0, "{}: eval depth", case.id);
    assert_eq!(ctx.condition_stack_len(), 0, "{}: conditions", case.id);
}

#[test]
fn arithmetic_shape_checked_and_verified_dynamic_operands_match_gnu() {
    for verified in [false, true] {
        let mut ctx = Context::new_vm_runtime_harness();
        ctx.set_variable("symbols-with-pos-enabled", Value::T);
        let specpdl_base = ctx.specpdl.len();
        let functions: BTreeMap<_, _> =
            ["add1", "sub1", "add", "sub", "eq", "lt", "gt", "le", "ge"]
                .map(|name| (name, function(name, verified)))
                .into_iter()
                .collect();
        for case in cases() {
            let roots = ctx.save_vm_roots();
            let args = arguments(&mut ctx, &case);
            let all_fixnums = args.iter().all(|value| value.is_fixnum());
            let direct_before = arith_integer_fast_count();
            let function = &functions[case.operation];
            let actual = {
                let mut vm = Vm::from_context(&mut ctx);
                #[cfg(feature = "jit")]
                vm.force_interpreter_only_for_test();
                // Preserve the normalization seal while the checked variant
                // deliberately disables only its stronger stack proof.
                answer(vm.execute_with_func_value(function, args, Value::NIL))
            };
            assert_eq!(
                actual, case.expected,
                "{} {}({}, {}), verified={verified}",
                case.id, case.operation, case.left, case.right,
            );
            if all_fixnums {
                // A false overflow still returns the right number through
                // the generic integer helper. Count that path so max+0 and
                // min-0 detect an erroneous intermediate tagged overflow.
                let expected_slow = usize::from(case.expected.starts_with("ok:bignum:"));
                assert_eq!(
                    arith_integer_fast_count() - direct_before,
                    expected_slow,
                    "{}: exact fixnum fast/overflow admission, verified={verified}",
                    case.id,
                );
            }
            assert_eq!(function.executes_verified_ops(), verified);
            assert_frontiers(&ctx, specpdl_base, &case);
            ctx.restore_vm_roots(roots);
        }
    }
}

#[cfg(feature = "jit")]
#[test]
fn arithmetic_shape_dynamic_native_and_precise_deopt_match_gnu() {
    use crate::emacs_core::jit::compile::{NativeRun, lower_leaf, take_pending_flow};

    let mut ctx = Context::new_vm_runtime_harness();
    ctx.set_variable("symbols-with-pos-enabled", Value::T);
    let specpdl_base = ctx.specpdl.len();
    let functions: BTreeMap<_, _> = ["add1", "sub1", "add", "sub", "eq", "lt", "gt", "le", "ge"]
        .map(|name| (name, function(name, true)))
        .into_iter()
        .collect();
    let leaves: BTreeMap<_, _> = functions
        .iter()
        .map(|(&name, function)| {
            let nargs = usize::from(!matches!(name, "add1" | "sub1")) + 1;
            let leaf =
                lower_leaf(function.executable_ops(), &[], nargs).expect("arithmetic lowers");
            (name, leaf)
        })
        .collect();
    let mut native_answers = 0;
    for case in cases() {
        let roots = ctx.save_vm_roots();
        let args = arguments(&mut ctx, &case);
        let function = &functions[case.operation];
        let ctx_ptr = &mut ctx as *mut Context as *mut u8;
        let result = match leaves[case.operation].call(ctx_ptr, &args) {
            NativeRun::Ok(bits) => {
                native_answers += 1;
                Ok(Value::from_bits(bits))
            }
            NativeRun::Signal => Err(take_pending_flow().expect("native arithmetic signal")),
            NativeRun::Deopt => {
                let mut vm = Vm::from_context(&mut ctx);
                vm.force_interpreter_only_for_test();
                vm.execute_with_func_value(function, args, Value::NIL)
            }
            NativeRun::DeoptAt(resume) => {
                let mut vm = Vm::from_context(&mut ctx);
                vm.force_interpreter_only_for_test();
                vm.run_resumed_frame(
                    function,
                    Value::NIL,
                    resume.pc,
                    &resume.stack,
                    resume.handlers,
                    &resume.binds,
                    resume.spec_base,
                    resume.cond_base,
                )
            }
        };
        assert_eq!(answer(result), case.expected, "{}: native/deopt", case.id);
        assert_frontiers(&ctx, specpdl_base, &case);
        assert_eq!(ctx.jit_root_stack_top, 0, "{}: native roots", case.id);
        ctx.restore_vm_roots(roots);
    }
    assert!(
        native_answers > 0,
        "dynamic fixnums must execute in native code"
    );
}
