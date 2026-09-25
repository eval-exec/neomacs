//! `neovm_jit_arith_generic` answers two integer operands from registers
//! (`Vm::arith_integer_fast`) before staging anything: a compiled generic
//! arithmetic site must still agree with the interpreter's opcode arm on
//! every bignum/fixnum mix, natively (no deopt), with and without the
//! every-guard-fails harness.

use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

/// `(lambda (a [b]) (OP a [b]))` with `Other` feedback at the op, so the
/// site lowers with the generic fallback.
fn generic_site(op: Op, nargs: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs as u32)
            .map(crate::emacs_core::intern::SymId)
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = if nargs == 2 {
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return]
    } else {
        vec![Op::StackRef(0), op, Op::Return]
    };
    f.max_stack = 8;
    let pc = f.ops.len() - 2;
    f.jit_runtime()
        .record_numeric(pc, f.ops.len(), NumericFeedback::Other);
    f
}

const POOL: &[&str] = &[
    "0",
    "-1",
    "3",
    "most-positive-fixnum",
    "most-negative-fixnum",
    "(1+ most-positive-fixnum)",
    "(1- most-negative-fixnum)",
    "(expt 2 64)",
    "(- (expt 2 64))",
    "(expt 3 100)",
    "(- (expt 5 90))",
];

fn check_all(force_deopt: bool) {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_deopt_for_test(force_deopt);
    let mut eval = Context::new();
    let values: Vec<Value> = POOL
        .iter()
        .map(|src| {
            let v = eval.eval_str(src).expect(src);
            eval.push_specpdl_root(v);
            v
        })
        .collect();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let binary = [
        Op::Add,
        Op::Sub,
        Op::Mul,
        Op::Eqlsign,
        Op::Lss,
        Op::Gtr,
        Op::Leq,
        Op::Geq,
    ];
    let mut checked = 0usize;
    for (ops, nargs) in [(&binary[..], 2usize), (&[Op::Add1, Op::Sub1][..], 1usize)] {
        for op in ops {
            let f = generic_site(op.clone(), nargs);
            let leaf = compile_bytecode_function(&f).expect("compiles");
            let firsts = &values[..];
            let seconds: &[Value] = if nargs == 2 {
                &values[..]
            } else {
                &values[..1]
            };
            for (i, &a) in firsts.iter().enumerate() {
                for (j, &b) in seconds.iter().enumerate() {
                    let args = if nargs == 2 { vec![a, b] } else { vec![a] };
                    let want = {
                        // SAFETY: `ctx_ptr` is `eval`, alive for the whole test.
                        let eval = unsafe { &mut *(ctx_ptr as *mut Context) };
                        let v = Vm::from_context(eval)
                            .execute(&f, args.clone())
                            .expect("interpreter");
                        print_value(&v)
                    };
                    let direct0 = crate::emacs_core::bytecode::vm::arith_integer_fast_count();
                    let got = match leaf.call(ctx_ptr, &args) {
                        NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
                        other if force_deopt => {
                            // The harness may send the whole leaf to the
                            // interpreter; its answer must still agree.
                            let _ = other;
                            let _ = take_pending_flow();
                            continue;
                        }
                        other => panic!(
                            "{op:?} ({}, {}) must answer natively: {other:?}",
                            POOL[i], POOL[j]
                        ),
                    };
                    assert_eq!(got, want, "{op:?} ({}, {})", POOL[i], POOL[j]);
                    // A site whose operands are both fixnums may answer
                    // inline; any other shape reached the shim and must have
                    // been answered directly there, not by the builtin.
                    if !(args.iter().all(|v| v.is_fixnum())) || matches!(op, Op::Mul) {
                        let direct =
                            crate::emacs_core::bytecode::vm::arith_integer_fast_count() - direct0;
                        assert!(
                            direct <= 1,
                            "{op:?} ({}, {}): one site, one answer",
                            POOL[i],
                            POOL[j]
                        );
                        if !(args.iter().all(|v| v.is_fixnum())) {
                            assert_eq!(
                                direct, 1,
                                "{op:?} ({}, {}): the shim answers integers directly",
                                POOL[i], POOL[j]
                            );
                        }
                    }
                    checked += 1;
                }
            }
        }
    }
    // The generic fallback is not a speculation guard, so even the
    // every-guard-fails harness leaves these sites native.
    assert_eq!(checked, 8 * 121 + 2 * 11, "force_deopt={force_deopt}");
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
}

#[test]
fn generic_integer_sites_match_the_interpreter() {
    check_all(false);
}

#[test]
fn generic_integer_sites_match_the_interpreter_under_forced_deopt() {
    check_all(true);
}
