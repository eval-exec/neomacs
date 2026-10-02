//! The register leaf ABI (S2.1a): T1, Rust and Cranelift agree on
//! `(vmctx, aux, a0..) -> (value, status)` for every arity; and a compiled
//! body answers the same through either ABI, on every exit.

use super::*;
use crate::emacs_core::eval::Context;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;

/// `fn(vmctx, aux, a0..a{k-1}) -> (vmctx ^ 3*aux + Σ (i+2)*a_i, 10k + 7)`,
/// built from [`LeafAbi::signature`] with the JIT's own ISA.
fn build_probe(module: &mut JITModule, k: u8) -> cranelift_module::FuncId {
    let cfg = module.target_config();
    let sig = LeafAbi::Register { arity: k }.signature(cfg.default_call_conv, cfg.pointer_type());
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = cranelift_frontend::FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
        let block = fb.create_block();
        fb.append_block_params_for_function_params(block);
        fb.switch_to_block(block);
        fb.seal_block(block);
        let p = fb.block_params(block).to_vec();
        let three = fb.ins().iconst(types::I64, 3);
        let aux3 = fb.ins().imul(p[1], three);
        let mut acc = fb.ins().bxor(p[0], aux3);
        for i in 0..k as usize {
            let w = fb.ins().iconst(types::I64, i as i64 + 2);
            let term = fb.ins().imul(p[2 + i], w);
            acc = fb.ins().iadd(acc, term);
        }
        let status = fb.ins().iconst(types::I64, 10 * i64::from(k) + 7);
        fb.ins().return_(&[acc, status]);
        fb.finalize(cfg);
    }
    let fid = module
        .declare_function(&format!("neovm_reg_abi_probe_{k}"), Linkage::Local, &sig)
        .expect("declare");
    let mut ctx = module.make_context();
    ctx.func = func;
    module.define_function(fid, &mut ctx).expect("define");
    module.clear_context(&mut ctx);
    fid
}

/// T1: both return words, the two register parameters and every argument
/// (the fifth and sixth on the stack) arrive where each side puts them,
/// called directly and through the arity's thunk.
#[test]
fn rust_and_cranelift_agree_on_the_register_abi_for_every_arity() {
    let isa = super::lowering::jit_isa().expect("isa");
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let ids: Vec<_> = (0..=MAX_REG_ARGS as u8)
        .map(|k| build_probe(&mut module, k))
        .collect();
    module.finalize_definitions().expect("finalize");
    let args: [i64; MAX_REG_ARGS] = [
        0x1111,
        -0x2222,
        0x3333_0000_0000,
        -7,
        0x0bad_cafe,
        0x7fff_ffff_ffff,
    ];
    let vmctx = 0x5a5a_5a5a_0000usize as *mut u8;
    let aux = 0x0123_4567_89a0usize as *const u8;
    for (k, &id) in ids.iter().enumerate() {
        let entry = module.get_finalized_function(id);
        // SAFETY: `entry` has the register ABI for exactly `k` words and
        // never dereferences its pointer parameters.
        let ret = unsafe { call_register_entry(entry, k as u8, vmctx, aux, args.as_ptr()) };
        let mut want = (vmctx as i64) ^ (aux as i64).wrapping_mul(3);
        for (i, &a) in args[..k].iter().enumerate() {
            want = want.wrapping_add(a.wrapping_mul(i as i64 + 2));
        }
        assert_eq!(
            ret,
            NativeRet {
                value: want,
                status: 10 * k as i64 + 7
            },
            "arity {k}"
        );
        // The leaf's thunk for the arity answers the same, and strips the
        // spec slot's key flags from `aux`.
        let thunk = register_thunk_for(LeafAbi::Register { arity: k as u8 });
        for flagged in [
            aux,
            (aux as usize | SpecSlot::KEY_REGISTER as usize) as *const u8,
        ] {
            // SAFETY: as above.
            let via_thunk = unsafe { thunk(entry, vmctx, flagged, args.as_ptr()) };
            assert_eq!(via_thunk, ret, "arity {k} through its thunk");
        }
    }
}

#[test]
fn only_bodies_a_direct_call_can_enter_take_the_register_abi() {
    // This pin covers the original global implication. The self policy
    // requires an actual selected self site, covered by direct_self_tests.
    force_direct_sites_for_test(Some(DirectSitesMode::All));
    force_direct_memory_for_test(Some(false));
    // Direct calls imply the register ABI: pin them off, so the ABI knob
    // alone decides (the suite also runs with `NEOVM_JIT_DIRECT_CALL=on`).
    force_direct_call_for_test(Some(false));
    force_register_abi_for_test(Some(true));
    let build = |aot, osr, arity, frameless, prefix| {
        LeafAbi::for_build(aot, osr, arity, frameless, prefix, false)
    };
    assert_eq!(
        build(false, false, 0, true, 0),
        LeafAbi::Register { arity: 0 }
    );
    assert_eq!(
        build(false, false, MAX_REG_ARGS, true, 0),
        LeafAbi::Register {
            arity: MAX_REG_ARGS as u8
        }
    );
    assert_eq!(
        build(false, false, MAX_REG_ARGS + 1, true, 0),
        LeafAbi::Memory
    );
    assert_eq!(build(true, false, 1, true, 0), LeafAbi::Memory, "AOT");
    assert_eq!(build(false, true, 1, true, 0), LeafAbi::Memory, "OSR");
    assert_eq!(build(false, false, 1, false, 0), LeafAbi::Memory, "framed");
    assert_eq!(build(false, false, 1, true, 1), LeafAbi::Memory, "patched");
    {
        let _optional = LambdaListScope::enter(LambdaList::Optional);
        assert_eq!(
            build(false, false, 1, true, 0),
            LeafAbi::Memory,
            "&optional"
        );
    }
    assert_eq!(
        build(false, false, 1, true, 0),
        LeafAbi::Register { arity: 1 },
        "the scope restores"
    );
    force_register_abi_for_test(Some(false));
    assert_eq!(build(false, false, 1, true, 0), LeafAbi::Memory);
    force_direct_call_for_test(Some(true));
    assert_eq!(
        build(false, false, 1, true, 0),
        LeafAbi::Register { arity: 1 },
        "direct calls imply the register ABI"
    );
    force_register_abi_for_test(None);
    force_direct_call_for_test(None);
    force_direct_sites_for_test(None);
    force_direct_memory_for_test(None);
}

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
        NativeRun::Ok(bits) => crate::emacs_core::print::print_value(&Value::from_bits(bits)),
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
    super::force_profit_gate_for_test(false);
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
        let sym = crate::emacs_core::intern::intern(name);
        let f = ev.obarray.symbol_function_id(sym).expect("defined");
        let bc = f.get_bytecode_data().expect("byte-compiled");
        let mut answers = Vec::new();
        for on in [false, true] {
            force_register_abi_for_test(Some(on));
            let leaf = compile_bytecode_function_with(bc, Some(&ev.obarray)).expect("compiles");
            // Required parameters only: `&optional` and `&rest` bodies are
            // never a direct call's callee (`LeafAbi::for_build`).
            let fits = leaf.arity <= MAX_REG_ARGS && leaf.required == leaf.arity;
            assert_eq!(
                leaf.abi,
                if on && fits {
                    LeafAbi::Register {
                        arity: leaf.arity as u8,
                    }
                } else {
                    LeafAbi::Memory
                },
                "{name}"
            );
            register_leaves += usize::from(matches!(leaf.abi, LeafAbi::Register { .. }));
            // SAFETY: the Context outlives the call; `args` are immediates.
            let run = leaf.call_consts(ctx.cast(), bc.jit_constant_base(), args);
            // SAFETY: as above.
            answers.push(outcome(unsafe { &mut *ctx }, run));
            // The spec shim's raw entry, for a frameless exact-arity body.
            if leaf.direct_call_eligible() && leaf.is_pure_passthrough(args.len()) {
                let words: Vec<i64> = args.iter().map(|v| v.bits() as i64).collect();
                let mut out = 0i64;
                // SAFETY: `words` holds exactly the leaf's arity.
                let status = unsafe {
                    leaf.entry_call_raw_consts(
                        ctx.cast(),
                        bc.jit_constant_base(),
                        words.as_ptr(),
                        &mut out,
                    )
                };
                if status == STATUS_OK {
                    answers.push(crate::emacs_core::print::print_value(&Value::from_bits(
                        out as usize,
                    )));
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
