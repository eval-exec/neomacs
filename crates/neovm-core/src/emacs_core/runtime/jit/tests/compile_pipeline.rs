//! The compile pipeline's fixed-cost machinery (P2.4 B0-B4): compile origins
//! and phase timers through the real cache seams.

use super::*;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::stats::{self, CompileOrigin, CompilePhase};
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    f
}

fn observe_stats() {
    stats::force_observe_for_test(stats::ObserveOverride {
        stats: true,
        naming: false,
        entry_count: false,
    });
}

/// `(lambda (x) (+ x 1))`.
fn add1() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    )
}

/// A dispatch-seam compile lands in the `dispatch` origin row, and its phase
/// split covers the backend (setup, codegen, finalize) and sums to the
/// stall aggregate.
#[test]
fn jit_pipeline_dispatch_compile_is_split_by_phase() {
    force_deopt_for_test(false);
    observe_stats();
    stats::reset_compile_stats();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let f_val = Value::make_bytecode(f.clone());
    let got = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(41)])
        .expect("no signal");
    assert_eq!(got, Some(Value::make_int(42).bits()));
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.total_compiles, 1);
    let row = s.origins[CompileOrigin::Dispatch as usize];
    assert_eq!((row.count, row.ok, row.us), (1, 1, s.total_us), "{s:?}");
    for phase in [
        CompilePhase::Gate,
        CompilePhase::Lower,
        CompilePhase::Codegen,
        CompilePhase::Finalize,
    ] {
        assert!(s.phase_ns[phase as usize] > 0, "{phase:?} unclaimed: {s:?}");
    }
    let split_us = s.phase_ns.iter().sum::<u64>() / 1_000;
    assert!(
        split_us.abs_diff(s.total_us) <= 1,
        "split {split_us}us vs stall {}us",
        s.total_us
    );
}

/// A spec site's first call into an uncompiled callee is a `first_sight`
/// compile; a compile outside the cache is `direct` and is not a stall.
#[test]
fn jit_pipeline_first_sight_and_direct_origins() {
    force_deopt_for_test(false);
    observe_stats();
    stats::reset_compile_stats();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert!(cache::resolve_compiled_leaf_ptr(ctx, &f).is_some());
    let g = add1();
    compile_bytecode_function_with(&g, None).expect("compiles");
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.origins[CompileOrigin::FirstSight as usize].count, 1);
    assert_eq!(
        s.origins[CompileOrigin::Direct as usize].count,
        0,
        "only the cache seams run a compile clock"
    );
    assert_eq!(s.total_compiles, 1);
}

/// Mask what legitimately differs between two compiles of one body: baked
/// addresses (hex literals of 6+ digits) and module-local external-name
/// indices (`userextname3`).
fn mask_code_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if text[i..].starts_with("0x") {
            let digits = text[i + 2..]
                .bytes()
                .take_while(u8::is_ascii_hexdigit)
                .count();
            if digits >= 6 {
                out.push_str("0xADDR");
                i += 2 + digits;
                continue;
            }
        }
        if text[i..].starts_with("userextname") {
            out.push_str("userextname");
            i += "userextname".len();
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        let ch = text[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The register-allocated code of every leaf `compile` builds on this
/// thread, one entry per leaf: `size=` plus the masked disassembly (the
/// `NEOVM_JIT_DUMP_ASM` capture; addresses and bytes are dropped).
fn captured_code(compile: impl FnOnce()) -> Vec<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("leaves.asm");
    stats::asm_dump::force_asm_dump_for_test(Some(path.clone()));
    compile();
    stats::asm_dump::force_asm_dump_for_test(None);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    text.split(";; ==== ")
        .filter(|chunk| !chunk.trim().is_empty())
        .map(|chunk| {
            let (header, rest) = chunk.split_once('\n').expect("header line");
            let size = header
                .split_whitespace()
                .find(|field| field.starts_with("size="))
                .expect("size field");
            let code = rest.split(";; bytes:").next().expect("disassembly");
            format!("{size}\n{}", mask_code_text(code))
        })
        .collect()
}

/// A body mix that reaches both tiers and most shim families: a pure MIR
/// leaf, a baseline leaf with calls, a loop, a condition-case and float
/// arithmetic.
fn corpus() -> Vec<(Vec<Op>, Vec<Value>, usize)> {
    let sym = |name: &str| Value::symbol(name);
    vec![
        // (lambda (x) (+ x 1)): MIR.
        (
            vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
            vec![Value::make_int(1)],
            1,
        ),
        // (lambda (x) (car (cons x x))): cons + car.
        (
            vec![Op::StackRef(0), Op::Dup, Op::Cons, Op::Car, Op::Return],
            vec![],
            1,
        ),
        // (lambda (x) (list x (symbol-value 'foo))): a varref and a list.
        (
            vec![Op::StackRef(0), Op::VarRef(1), Op::List(2), Op::Return],
            vec![Value::NIL, sym("jit-pipeline-foo")],
            1,
        ),
        // (lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i)): loop.
        (
            vec![
                Op::Constant(0),  // 0: i = 0         [n i]
                Op::StackRef(0),  // 1: i             [n i i]
                Op::StackRef(2),  // 2: n             [n i i n]
                Op::Lss,          // 3: (< i n)       [n i b]
                Op::GotoIfNil(9), // 4:               [n i]
                Op::StackRef(0),  // 5: i             [n i i]
                Op::Add1,         // 6:               [n i i+1]
                Op::StackSet(1),  // 7: i = i+1       [n i]
                Op::Goto(1),      // 8
                Op::Return,       // 9: i
            ],
            vec![Value::make_int(0)],
            1,
        ),
        // (lambda (x y) (* x y)) on floats would need feedback; plain `*`.
        (
            vec![Op::StackRef(1), Op::StackRef(1), Op::Mul, Op::Return],
            vec![],
            2,
        ),
        // (lambda (x) (eq x 'a)): eq + symbol constant.
        (
            vec![Op::StackRef(0), Op::Constant(0), Op::Eq, Op::Return],
            vec![sym("jit-pipeline-a")],
            1,
        ),
        // (lambda (f x) (+ 1 (funcall f x))): a generic call (baseline, call
        // shim) with enough arithmetic to pass the profitability gate.
        (
            vec![
                Op::Constant(0),
                Op::StackRef(2),
                Op::StackRef(2),
                Op::Call(1),
                Op::Add,
                Op::Return,
            ],
            vec![Value::make_int(1)],
            2,
        ),
    ]
}

/// Compile every corpus body through the cache (a fresh function object per
/// body, so each compiles) and return its captured code.
fn compile_corpus() -> Vec<String> {
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    captured_code(|| {
        for (ops, constants, arity) in corpus() {
            let f = function(ops, constants, arity);
            assert!(
                cache::resolve_compiled_leaf_ptr(ctx, &f).is_some(),
                "every corpus body compiles: {:?}",
                f.ops
            );
        }
    })
}

/// T-S3: the cached ISA carries exactly the flags a fresh build has, for
/// both allocators, and is shared rather than rebuilt.
#[test]
fn jit_pipeline_cached_isa_matches_a_fresh_build() {
    use lowering::{
        RegallocChoice, RegallocScope, build_jit_isa, force_isa_cache_for_test, jit_isa,
    };
    force_isa_cache_for_test(true);
    for choice in [RegallocChoice::Fast, RegallocChoice::Full] {
        let _scope = RegallocScope::enter(choice);
        let cached = jit_isa().expect("isa");
        let again = jit_isa().expect("isa");
        assert!(
            std::sync::Arc::ptr_eq(&cached, &again),
            "{choice:?}: one ISA per allocator"
        );
        let fresh = build_jit_isa(choice).expect("isa");
        let shared = |isa: &cranelift_codegen::isa::OwnedTargetIsa| {
            isa.flags()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        };
        let specific = |isa: &cranelift_codegen::isa::OwnedTargetIsa| {
            isa.isa_flags()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(shared(&cached), shared(&fresh), "{choice:?}");
        assert_eq!(specific(&cached), specific(&fresh), "{choice:?}");
        assert_eq!(cached.triple(), fresh.triple());
        assert!(
            shared(&cached).contains(&format!(
                "regalloc_algorithm={}",
                choice.cranelift_setting()
            )),
            "{choice:?}: {:?}",
            shared(&cached)
        );
    }
    force_isa_cache_for_test(false);
    let _scope = RegallocScope::enter(RegallocChoice::Full);
    assert!(
        !std::sync::Arc::ptr_eq(&jit_isa().expect("isa"), &jit_isa().expect("isa")),
        "NEOVM_JIT_ISA_CACHE=off builds per compile"
    );
}

/// The ISA cache changes no generated code: the corpus compiles to the same
/// machine code (addresses masked) with the cache on and off.
#[test]
fn jit_pipeline_isa_cache_is_code_identical() {
    lowering::force_isa_cache_for_test(false);
    let fresh = compile_corpus();
    lowering::force_isa_cache_for_test(true);
    let cached = compile_corpus();
    assert_eq!(fresh.len(), corpus().len(), "{fresh:?}");
    assert_eq!(fresh, cached);
}
