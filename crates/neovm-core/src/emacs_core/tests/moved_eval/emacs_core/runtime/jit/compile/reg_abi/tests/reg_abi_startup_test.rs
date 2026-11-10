//! Register and memory ABIs answer the same on the runtime-startup image.
//!
//! The cranelift ISA probe stays in eval. Required-parameter count and the
//! register ceiling stay behind the authority fences. Printed answers stay
//! the compared values.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::{
    NativeRun, STATUS_OK, STATUS_SIGNAL, compile_bytecode_function_with, take_pending_flow,
};
use neovm_eval::test_support::authority::{
    TestLeafAbi, compiled_leaf_required_for_test, entry_call_raw_consts_for_test,
    force_direct_call_for_test, force_profit_gate_for_test, force_register_abi_for_test,
    leaf_abi_for_test, max_reg_args_for_test,
};
use neovm_value::value::Value;

/// The functions of the differential: every arity up to one past the
/// register limit, `&optional` and `&rest`, and the non-OK exits (a
/// precise deopt, a signal).
const SHAPES: &str = r#"(progn
  (defun neovm--ra-0 () 42)
  (defun neovm--ra-1 (a) (+ a 1))
  (defun neovm--ra-2 (a b) (- a b))
  (defun neovm--ra-3 (a b c) (list a b c))
  (defun neovm--ra-4 (a b c d) (+ a (* b c) d))
  (defun neovm--ra-5 (a b c d e) (list a b c d e))
  (defun neovm--ra-6 (a b c d e f) (vector a b c d e f))
  (defun neovm--ra-7 (a b c d e f g) (list a b c d e f g))
  (defun neovm--ra-opt (a &optional b) (list a b))
  (defun neovm--ra-rest (a &rest r) (cons a r))
  (defun neovm--ra-car (x) (car x))
  (defun neovm--ra-sig (x) (signal 'error (list "reg-abi" x)))
  (dolist (f '(neovm--ra-0 neovm--ra-1 neovm--ra-2 neovm--ra-3 neovm--ra-4 neovm--ra-5
               neovm--ra-6 neovm--ra-7 neovm--ra-opt neovm--ra-rest neovm--ra-car
               neovm--ra-sig))
    (byte-compile f)))"#;

fn outcome(ev: &mut Context, run: NativeRun) -> String {
    match run {
        NativeRun::Ok(bits) => crate::emacs_core::print::print_value(&unsafe {
            /* SAFETY: NativeRun::Ok is a real Value's tagged bits */
            Value::from_bits(bits)
        }),
        NativeRun::Deopt => "deopt".into(),
        NativeRun::DeoptAt(resume) => format!("deopt-at {}", resume.pc),
        NativeRun::Signal => {
            let flow = take_pending_flow().expect("a signal stashes its flow");
            let _ = ev;
            format!("signal {}", without_addresses(&format!("{flow:?}")))
        }
    }
}

/// `text` with every `@0x…` heap address dropped (a signal's fresh conses).
fn without_addresses(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("@0x") {
        out.push_str(&rest[..at]);
        rest = rest[at + 3..].trim_start_matches(|c: char| c.is_ascii_hexdigit());
    }
    out.push_str(rest);
    out
}

/// Every shape compiled with each ABI answers the same through the Rust
/// callers (`invoke_native`, and the spec shim's raw entry for a
/// frameless body), on OK, precise-deopt and signal exits alike.
#[test]
fn a_body_answers_the_same_through_either_abi() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(SHAPES).expect("shapes defined");
    let calls: &[(&str, &[Value])] = &[
        ("neovm--ra-0", &[]),
        ("neovm--ra-1", &[Value::fixnum(41)]),
        ("neovm--ra-2", &[Value::fixnum(50), Value::fixnum(8)]),
        (
            "neovm--ra-3",
            &[Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)],
        ),
        (
            "neovm--ra-4",
            &[
                Value::fixnum(1),
                Value::fixnum(2),
                Value::fixnum(3),
                Value::fixnum(4),
            ],
        ),
        (
            "neovm--ra-5",
            &[
                Value::fixnum(1),
                Value::fixnum(2),
                Value::fixnum(3),
                Value::fixnum(4),
                Value::fixnum(5),
            ],
        ),
        (
            "neovm--ra-6",
            &[
                Value::fixnum(1),
                Value::fixnum(2),
                Value::fixnum(3),
                Value::fixnum(4),
                Value::fixnum(5),
                Value::fixnum(6),
            ],
        ),
        (
            "neovm--ra-7",
            &[
                Value::fixnum(1),
                Value::fixnum(2),
                Value::fixnum(3),
                Value::fixnum(4),
                Value::fixnum(5),
                Value::fixnum(6),
                Value::fixnum(7),
            ],
        ),
        ("neovm--ra-opt", &[Value::fixnum(1)]),
        ("neovm--ra-opt", &[Value::fixnum(1), Value::fixnum(2)]),
        (
            "neovm--ra-rest",
            &[Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)],
        ),
        ("neovm--ra-car", &[Value::fixnum(3)]),
        ("neovm--ra-sig", &[Value::fixnum(9)]),
    ];
    let ctx = std::ptr::from_mut(&mut ev);
    let mut register_leaves = 0;
    // The ABI knob alone decides (direct calls would imply the register ABI).
    force_direct_call_for_test(Some(false));
    for &(name, args) in calls {
        let sym = neovm_value::intern::intern(name);
        let f = ev.obarray().symbol_function_id(sym).expect("defined");
        let bc = f.get_bytecode_data().expect("byte-compiled");
        let mut answers = Vec::new();
        for on in [false, true] {
            force_register_abi_for_test(Some(on));
            let leaf = compile_bytecode_function_with(bc, Some(ev.obarray())).expect("compiles");
            // Required parameters only: `&optional` and `&rest` bodies are
            // never a direct call's callee (`LeafAbi::for_build`).
            let arity = leaf.arity();
            let required = compiled_leaf_required_for_test(&leaf);
            let fits = arity <= max_reg_args_for_test() && required == arity;
            let abi = leaf_abi_for_test(&leaf);
            assert_eq!(
                abi,
                if on && fits {
                    TestLeafAbi::Register { arity: arity as u8 }
                } else {
                    TestLeafAbi::Memory
                },
                "{name}"
            );
            register_leaves += usize::from(matches!(abi, TestLeafAbi::Register { .. }));
            // SAFETY: the Context outlives the call; `args` are immediates.
            let run = leaf.call_consts(ctx.cast(), bc.constants.as_ptr(), args);
            // SAFETY: as above.
            answers.push(outcome(unsafe { &mut *ctx }, run));
            // The spec shim's raw entry, for a frameless exact-arity body.
            if leaf.direct_call_eligible() && leaf.is_pure_passthrough(args.len()) {
                let words: Vec<i64> = args.iter().map(|v| v.bits() as i64).collect();
                let mut out = 0i64;
                // SAFETY: `words` holds exactly the leaf's arity.
                let status = unsafe {
                    entry_call_raw_consts_for_test(
                        &leaf,
                        ctx.cast(),
                        bc.constants.as_ptr(),
                        words.as_ptr(),
                        &mut out,
                    )
                };
                if status == STATUS_OK {
                    answers.push(crate::emacs_core::print::print_value(&unsafe {
                        // SAFETY: the raw entry wrote a real Value's tagged bits.
                        Value::from_bits(out as usize)
                    }));
                } else if status == STATUS_SIGNAL {
                    let _ = take_pending_flow();
                    answers.push("raw signal".into());
                } else {
                    answers.push(format!("raw status {status}"));
                }
            }
        }
        let (memory, register) = answers.split_at(answers.len() / 2);
        assert_eq!(memory, register, "{name} {args:?}");
    }
    force_register_abi_for_test(None);
    force_direct_call_for_test(None);
    assert!(register_leaves >= 9, "the register ABI engaged");
}
