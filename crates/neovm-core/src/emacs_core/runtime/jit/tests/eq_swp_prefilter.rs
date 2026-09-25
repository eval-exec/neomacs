//! `eq` and `symbolp` in native code answer inline unless an operand is a
//! veclike: only a symbol-with-pos (a veclike) can make two differing values
//! `eq` or a non-symbol `symbolp` (GNU `EQ` → `slow_eq`, `SYMBOLP`). Every
//! answer must be the interpreter's, with `symbols-with-pos-enabled` on and
//! off, on both compile tiers and under the every-guard-fails harness; and a
//! pair with no veclike operand must never reach the slow-path shim.

use super::shims::{EQ_SLOW_CALLS, SYMBOLP_SLOW_CALLS};
use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

/// How the body's parameters are declared, which picks the compile tier:
/// a required-only body is attempted on the MIR tier, an `&optional` one
/// stays on the baseline.
#[derive(Clone, Copy, Debug)]
enum Shape {
    RequiredOnly,
    WithOptional,
}

fn body(ops: Vec<Op>, constants: Vec<Value>, arity: usize, shape: Shape) -> ByteCodeFunction {
    let params: Vec<SymId> = (0..arity).map(|i| SymId(i as u32 + 1)).collect();
    let (required, optional) = match shape {
        Shape::RequiredOnly => (params, Vec::new()),
        Shape::WithOptional => (params[..1].to_vec(), params[1..].to_vec()),
    };
    let mut f = ByteCodeFunction::new(LambdaParams {
        required,
        optional,
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    f
}

/// `(lambda (a b) (eq a b))`
fn eq_body(shape: Shape) -> ByteCodeFunction {
    body(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Eq, Op::Return],
        Vec::new(),
        2,
        shape,
    )
}

/// `(lambda (x) (symbolp x))`
fn symbolp_body() -> ByteCodeFunction {
    body(
        vec![Op::StackRef(0), Op::Symbolp, Op::Return],
        Vec::new(),
        1,
        Shape::RequiredOnly,
    )
}

/// `(lambda (a b) (if (eq a b) 'yes 'no))` — the result feeds a branch.
fn eq_branch_body() -> ByteCodeFunction {
    body(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Eq,
            Op::GotoIfNil(6),
            Op::Constant(0),
            Op::Return,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::symbol("yes"), Value::symbol("no")],
        2,
        Shape::RequiredOnly,
    )
}

fn interpret(eval: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> Value {
    let mut vm = Vm::from_context(eval);
    vm.execute(f, args.to_vec()).expect("interpreter answers")
}

/// Run natively: `(answer, eq slow calls, symbolp slow calls)`.
fn native(ctx_ptr: *mut u8, leaf: &CompiledLeaf, args: &[Value]) -> (Value, usize, usize) {
    EQ_SLOW_CALLS.with(|c| c.set(0));
    SYMBOLP_SLOW_CALLS.with(|c| c.set(0));
    let answer = match leaf.call(ctx_ptr, args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("must not leave native code: {other:?}"),
    };
    (
        answer,
        EQ_SLOW_CALLS.with(|c| c.get()),
        SYMBOLP_SLOW_CALLS.with(|c| c.get()),
    )
}

fn eval_value(eval: &mut Context, src: &str) -> Value {
    let v = eval
        .eval_str(src)
        .unwrap_or_else(|e| panic!("{src}: {e:?}"));
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

fn set_symbols_with_pos(eval: &mut Context, on: bool) {
    eval_value(
        eval,
        if on {
            "(setq symbols-with-pos-enabled t)"
        } else {
            "(setq symbols-with-pos-enabled nil)"
        },
    );
    assert_eq!(eval.symbols_with_pos_enabled, on);
}

/// One value of every tag, plus symbols-with-pos over two symbols (and two
/// distinct objects over the same one).
fn matrix(eval: &mut Context) -> Vec<(&'static str, Value)> {
    vec![
        ("fixnum", Value::fixnum(7)),
        ("nil", Value::NIL),
        ("t", Value::T),
        ("x", Value::symbol("x")),
        ("y", Value::symbol("y")),
        ("cons", eval_value(eval, "(cons 1 2)")),
        ("string", eval_value(eval, "(copy-sequence \"s\")")),
        ("float", eval_value(eval, "1.5")),
        ("vector", eval_value(eval, "(vector 1 2)")),
        ("record", eval_value(eval, "(record 'foo 1)")),
        ("bytecode", {
            let v = Value::make_bytecode(symbolp_body());
            crate::emacs_core::eval::push_scratch_gc_root(v);
            v
        }),
        ("swp-x", eval_value(eval, "(position-symbol 'x 5)")),
        ("swp-x-again", eval_value(eval, "(position-symbol 'x 9)")),
        ("swp-y", eval_value(eval, "(position-symbol 'y 5)")),
    ]
}

fn check_eq_matrix(eval: &mut Context, f: &ByteCodeFunction, leaf: &CompiledLeaf, what: &str) {
    let ctx_ptr = eval as *mut Context as *mut u8;
    let values = matrix(eval);
    assert!(values.iter().any(|(_, v)| v.is_symbol_with_pos()));
    for on in [false, true] {
        set_symbols_with_pos(eval, on);
        for &(an, a) in &values {
            for &(bn, b) in &values {
                let want = interpret(eval, f, &[a, b]);
                let (got, eq_calls, _) = native(ctx_ptr, leaf, &[a, b]);
                let label = format!("{what}: (eq {an} {bn}), symbols-with-pos-enabled={on}");
                assert_eq!(print_value(&got), print_value(&want), "{label}");
                if a.bits() == b.bits() || !(a.is_veclike() || b.is_veclike()) {
                    assert_eq!(eq_calls, 0, "{label}: must answer inline");
                } else {
                    assert_eq!(eq_calls, 1, "{label}: a veclike operand takes the shim");
                }
            }
        }
    }
    // The symbols-with-pos answers the matrix must have exercised.
    set_symbols_with_pos(eval, true);
    let swp_x = values
        .iter()
        .find(|(name, _)| *name == "swp-x")
        .expect("swp-x in the matrix")
        .1;
    let (got, _, _) = native(ctx_ptr, leaf, &[swp_x, Value::symbol("x")]);
    assert_eq!(got, Value::T, "{what}: swp-x eq x while enabled");
    set_symbols_with_pos(eval, false);
    let (got, _, _) = native(ctx_ptr, leaf, &[swp_x, Value::symbol("x")]);
    assert_eq!(got, Value::NIL, "{what}: swp-x not eq x while disabled");
}

#[test]
fn eq_prefilter_matches_the_interpreter_on_both_tiers() {
    force_profit_gate_for_test(false);
    let mut eval = Context::new();
    let mir = eq_body(Shape::RequiredOnly);
    let mir_leaf = compile_bytecode_function(&mir).expect("compiles");
    assert_eq!(mir_leaf.tier, leaf::LeafTier::Mir);
    check_eq_matrix(&mut eval, &mir, &mir_leaf, "mir");
    let baseline = eq_body(Shape::WithOptional);
    let baseline_leaf = compile_bytecode_function(&baseline).expect("compiles");
    assert_eq!(baseline_leaf.tier, leaf::LeafTier::Baseline);
    check_eq_matrix(&mut eval, &baseline, &baseline_leaf, "baseline");
}

#[test]
fn eq_prefilter_matches_the_interpreter_under_forced_deopt() {
    force_profit_gate_for_test(false);
    force_deopt_for_test(true);
    let mut eval = Context::new();
    for shape in [Shape::RequiredOnly, Shape::WithOptional] {
        let f = eq_body(shape);
        let leaf = compile_bytecode_function(&f).expect("compiles");
        check_eq_matrix(&mut eval, &f, &leaf, &format!("forced deopt {shape:?}"));
    }
    force_deopt_for_test(false);
}

#[test]
fn symbolp_prefilter_matches_the_interpreter() {
    force_profit_gate_for_test(false);
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = symbolp_body();
    for deopt in [false, true] {
        force_deopt_for_test(deopt);
        let leaf = compile_bytecode_function(&f).expect("compiles");
        let values = matrix(&mut eval);
        for on in [false, true] {
            set_symbols_with_pos(&mut eval, on);
            for &(name, v) in &values {
                let want = interpret(&mut eval, &f, &[v]);
                let (got, _, calls) = native(ctx_ptr, &leaf, &[v]);
                let label = format!("(symbolp {name}), enabled={on}, deopt={deopt}");
                assert_eq!(print_value(&got), print_value(&want), "{label}");
                let expected_calls = usize::from(v.is_veclike());
                assert_eq!(calls, expected_calls, "{label}: shim calls");
                if name.starts_with("swp") {
                    assert_eq!(got == Value::T, on, "{label}");
                }
            }
        }
    }
    force_deopt_for_test(false);
}

#[test]
fn eq_prefilter_feeds_branches() {
    force_profit_gate_for_test(false);
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = eq_branch_body();
    let leaf = compile_bytecode_function(&f).expect("compiles");
    let values = matrix(&mut eval);
    for on in [false, true] {
        set_symbols_with_pos(&mut eval, on);
        for &(an, a) in &values {
            for &(bn, b) in &values {
                let want = interpret(&mut eval, &f, &[a, b]);
                let (got, _, _) = native(ctx_ptr, &leaf, &[a, b]);
                assert_eq!(
                    print_value(&got),
                    print_value(&want),
                    "(if (eq {an} {bn}) 'yes 'no), enabled={on}"
                );
            }
        }
    }
}

/// `NEOVM_JIT_EQ_PREFILTER=off` (the A/B baseline) compiles the former
/// shape: every mismatching `eq` and every non-symbol `symbolp` calls the
/// shim, with the same answers.
#[test]
fn eq_prefilter_off_calls_the_shim_for_every_mismatch() {
    force_profit_gate_for_test(false);
    force_eq_prefilter_for_test(false);
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let eq = eq_body(Shape::RequiredOnly);
    let eq_leaf = compile_bytecode_function(&eq).expect("compiles");
    let symbolp = symbolp_body();
    let symbolp_leaf = compile_bytecode_function(&symbolp).expect("compiles");
    let values = matrix(&mut eval);
    for on in [false, true] {
        set_symbols_with_pos(&mut eval, on);
        for &(an, a) in &values {
            for &(bn, b) in &values {
                let want = interpret(&mut eval, &eq, &[a, b]);
                let (got, calls, _) = native(ctx_ptr, &eq_leaf, &[a, b]);
                assert_eq!(print_value(&got), print_value(&want), "(eq {an} {bn})");
                assert_eq!(calls, usize::from(a.bits() != b.bits()), "(eq {an} {bn})");
            }
            let want = interpret(&mut eval, &symbolp, &[a]);
            let (got, _, calls) = native(ctx_ptr, &symbolp_leaf, &[a]);
            assert_eq!(print_value(&got), print_value(&want), "(symbolp {an})");
            assert_eq!(calls, usize::from(!a.is_symbol()), "(symbolp {an})");
        }
    }
    force_eq_prefilter_for_test(true);
}
