//! `Vm::arith_integer_fast`, the all-integer answer of the arithmetic
//! opcodes' slow arm, against the full builtin it short-circuits
//! (`call_arith_builtin_slow_on_context`) for every kind over fixnums at
//! the edges, bignums of both signs, a marker, floats and non-numbers:
//! the same value (a fixnum exactly when it fits, a bignum never an
//! operand object), and `None` for everything that is not two integers.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use strum::IntoEnumIterator;

fn rooted(eval: &mut Context, v: Value) -> Value {
    eval.push_specpdl_root(v);
    v
}

/// The operand pool: `(source, is-an-integer)`.
const POOL: &[(&str, bool)] = &[
    ("0", true),
    ("1", true),
    ("-1", true),
    ("7", true),
    ("most-positive-fixnum", true),
    ("most-negative-fixnum", true),
    ("(1+ most-positive-fixnum)", true),
    ("(1- most-negative-fixnum)", true),
    ("(expt 2 64)", true),
    ("(- (expt 2 64))", true),
    ("(expt 3 100)", true),
    ("(- (expt 7 80))", true),
    ("1.5", false),
    ("-0.0", false),
    ("(point-marker)", false),
    ("\"s\"", false),
    ("nil", false),
];

fn slow(eval: &mut Context, kind: ArithGenericKind, args: &[Value]) -> String {
    let args_start = eval.bc_buf.len();
    eval.bc_buf.extend_from_slice(args);
    let result = Vm::call_arith_builtin_slow_on_context(eval, kind, args_start, args.len())
        .expect("static arithmetic subr");
    eval.bc_buf.truncate(args_start);
    crate::emacs_core::error::format_eval_result(
        &result.map_err(crate::emacs_core::error::map_flow),
    )
}

#[test]
fn integer_fast_path_matches_the_full_builtin_for_every_kind() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(insert \"abcdef\")").unwrap();
    eval.eval_str("(goto-char 3)").unwrap();
    let pool: Vec<(Value, bool, &str)> = POOL
        .iter()
        .map(|&(src, integer)| {
            let v = eval.eval_str(src).expect(src);
            (rooted(&mut eval, v), integer, src)
        })
        .collect();
    let mut answered = 0usize;
    for kind in ArithGenericKind::iter() {
        let pairs: Vec<Vec<usize>> = if kind.arity() == 2 {
            (0..pool.len())
                .flat_map(|i| (0..pool.len()).map(move |j| vec![i, j]))
                .collect()
        } else {
            (0..pool.len()).map(|i| vec![i]).collect()
        };
        for idx in pairs {
            let args: Vec<Value> = idx.iter().map(|&i| pool[i].0).collect();
            let all_integers = idx.iter().all(|&i| pool[i].1);
            let what = format!(
                "{kind:?} {:?}",
                idx.iter().map(|&i| pool[i].2).collect::<Vec<_>>()
            );
            let fast = Vm::arith_integer_fast(kind, &args);
            if kind.integer_op().is_none() || !all_integers {
                assert!(fast.is_none(), "{what}: must take the full builtin");
                continue;
            }
            let fast = fast.unwrap_or_else(|| panic!("{what}: two integers answer directly"));
            let fast = rooted(&mut eval, fast);
            assert_eq!(
                format!("OK {}", print_value(&fast)),
                slow(&mut eval, kind, &args),
                "{what}"
            );
            if fast.is_bignum() {
                assert!(
                    args.iter().all(|a| a.bits() != fast.bits()),
                    "{what}: a bignum result is a fresh object, never an operand",
                );
            }
            if let Some(n) = fast.as_bignum() {
                assert!(
                    *n > Value::MOST_POSITIVE_FIXNUM || *n < Value::MOST_NEGATIVE_FIXNUM,
                    "{what}: a result that fits a fixnum is a fixnum",
                );
            }
            answered += 1;
        }
    }
    // 8 binary kinds x 12^2 integer pairs + 2 unary kinds x 12.
    assert_eq!(answered, 8 * 144 + 2 * 12);
}

/// The direct answer never signals, so it can never enter the debugger or
/// the signal hook, even with both armed.
#[test]
fn integer_fast_path_never_reaches_the_debugger_or_signal_hook() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        "(setq afp-entered nil
               debug-on-error t
               debugger (lambda (&rest _) (setq afp-entered 'debugger))
               signal-hook-function (lambda (&rest _) (setq afp-entered 'hook)))",
    )
    .unwrap();
    let big = eval.eval_str("(expt 2 100)").unwrap();
    let big = rooted(&mut eval, big);
    for kind in ArithGenericKind::iter().filter(|k| k.integer_op().is_some()) {
        let args: &[Value] = if kind.arity() == 2 {
            &[big, Value::fixnum(-3)]
        } else {
            &[big]
        };
        let v = Vm::arith_integer_fast(kind, args).expect("integers answer directly");
        rooted(&mut eval, v);
    }
    let entered = eval.eval_str("afp-entered").unwrap();
    assert!(entered.is_nil(), "entered {}", print_value(&entered));
    eval.eval_str("(setq signal-hook-function nil debug-on-error nil)")
        .unwrap();
}

/// Through the interpreter's opcode arms: the direct answer is what the
/// opcode returns, for bignum operands on both sides and the fixnum
/// overflow edges.
#[test]
fn arithmetic_opcodes_answer_integer_operands_directly() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let cases: &[(Op, &str, &str, &str)] = &[
        (Op::Mul, "(expt 3 50)", "-9", "(* (expt 3 50) -9)"),
        (
            Op::Mul,
            "most-positive-fixnum",
            "most-positive-fixnum",
            "(* most-positive-fixnum most-positive-fixnum)",
        ),
        (Op::Add, "(expt 2 64)", "(- (expt 2 64))", "0"),
        (
            Op::Sub,
            "most-negative-fixnum",
            "1",
            "(1- most-negative-fixnum)",
        ),
        (Op::Lss, "(expt 2 70)", "(expt 2 71)", "t"),
        (Op::Geq, "(- (expt 2 70))", "most-negative-fixnum", "nil"),
        (
            Op::Eqlsign,
            "(expt 2 70)",
            "(* (expt 2 35) (expt 2 35))",
            "t",
        ),
    ];
    for (op, a, b, want) in cases {
        let a = eval.eval_str(a).unwrap();
        let a = rooted(&mut eval, a);
        let b = eval.eval_str(b).unwrap();
        let b = rooted(&mut eval, b);
        let want = eval.eval_str(want).unwrap();
        let want = rooted(&mut eval, want);
        let mut f = ByteCodeFunction::new(crate::emacs_core::value::LambdaParams {
            required: vec![
                crate::emacs_core::intern::SymId(1),
                crate::emacs_core::intern::SymId(2),
            ],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::StackRef(1), Op::StackRef(1), op.clone(), Op::Return];
        f.max_stack = 8;
        let direct0 = crate::emacs_core::bytecode::vm::arith_integer_fast_count();
        let got = Vm::from_context(&mut eval)
            .execute(&f, vec![a, b])
            .expect("integer arithmetic");
        assert_eq!(print_value(&got), print_value(&want), "{op:?}");
        assert_eq!(
            crate::emacs_core::bytecode::vm::arith_integer_fast_count() - direct0,
            1,
            "{op:?}: the opcode's slow arm answered directly",
        );
    }
}
