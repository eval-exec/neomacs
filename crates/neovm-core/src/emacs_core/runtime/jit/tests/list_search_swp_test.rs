//! `memq`/`assq` under `symbols-with-pos-enabled`: the dispatch shims'
//! bounded walks must answer exactly GNU 31.1's `EQ` (`lisp.h` `EQ` ->
//! `data.c` `slow_eq`: bit identity first; else, with the flag on, each side
//! unwraps only if it is itself a symbol with position, and the two bare
//! sides compare by identity). Everything else -- a list with no match in
//! `LIST_SHIM_FAST_STEPS` cells, an improper tail before a match, a list a
//! cycle check would have to consider -- stays with the builtin, exactly as
//! before.
//!
//! The expected matrix below is GNU 31.1's answers, produced with
//!   tmp/v10x/sandbox-run.sh $NEOVM_FORCE_ORACLE_PATH -Q --batch -l tmp/memq/swp-oracle.el
//! inside `nix develop /home/exec/Projects/github.com/eval-exec/neomacs-main`
//! (rows `OP swp=N TARGET ELEMENT INDEX`, the index of the matched cell).
//! Each case is checked against GNU's expected value, against the builtin
//! that answered it before the walk learned positions, and (for the compiled
//! sites) against the interpreter's opcode arm.

use super::super::*;
use super::{
    ARRAY_SHIM_SLOW_CALLS, LIST_SEARCH_SHIM_CALLS, VALUE_SHIM_SIGNAL, neovm_jit_assq,
    neovm_jit_memq,
};
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

fn lexical_fn(nargs: u32, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(crate::emacs_core::intern::SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    f
}

/// `(lambda (a b) (OP a b))`.
fn search_fn(op: Op) -> ByteCodeFunction {
    lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return],
        vec![],
    )
}

/// Compile with both JIT knobs forced, then restore the environment's.
/// `memq`/`assq` have no leaf site, so the leaf knob only proves that.
fn compile_with(f: &ByteCodeFunction, leaf: LeafKnob, intrinsics: IntrinsicKnob) -> CompiledLeaf {
    force_profit_gate_for_test(false);
    force_leaf_knob_for_test(Some(leaf));
    force_intrinsic_knob_for_test(Some(intrinsics));
    let compiled = compile_bytecode_function(f).expect("compiles");
    force_intrinsic_knob_for_test(None);
    force_leaf_knob_for_test(None);
    compiled
}

fn flow_symbol(flow: crate::emacs_core::error::Flow) -> String {
    match flow.into_kind() {
        crate::emacs_core::error::FlowKind::Signal(sig) => sig.symbol_name().to_string(),
        other => format!("{other:?}"),
    }
}

fn interpret(eval: &mut Context, f: &ByteCodeFunction, args: Vec<Value>) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(eval);
    vm.execute(f, args)
}

fn native(
    ctx_ptr: *mut u8,
    leaf: &CompiledLeaf,
    args: &[Value],
    what: &str,
) -> Result<Value, Flow> {
    match leaf.call(ctx_ptr, args) {
        NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
        NativeRun::Signal => Err(take_pending_flow().expect("flow stashed")),
        other => panic!("{what} must not leave native code: {other:?}"),
    }
}

/// One direct call of the dispatch shim, decoded like [`native`].
fn shim(op: bool, ctx_ptr: *mut u8, target: Value, list: Value) -> Result<Value, Flow> {
    let word = if op {
        neovm_jit_assq(ctx_ptr, target.bits() as i64, list.bits() as i64)
    } else {
        neovm_jit_memq(ctx_ptr, target.bits() as i64, list.bits() as i64)
    };
    if word == VALUE_SHIM_SIGNAL {
        Err(take_pending_flow().expect("flow stashed"))
    } else {
        Ok(Value::from_bits(word as usize))
    }
}

/// The index of the matched cell (`memq`: the tail, `assq`: the entry),
/// `-1` for nil, or the signal's symbol name.
fn index_answer(op: bool, list: Value, result: Result<Value, Flow>) -> String {
    match result {
        Ok(value) if value.is_nil() => "-1".to_string(),
        Ok(value) => {
            let mut tail = list;
            let mut i = 0i64;
            while tail.is_cons() && i < 4096 {
                let cell = if op { tail.cons_car() } else { tail };
                if cell.bits() == value.bits() {
                    return i.to_string();
                }
                tail = tail.cons_cdr();
                i += 1;
            }
            format!("no cell matches {}", print_value(&value))
        }
        Err(flow) => format!("signal {}", flow_symbol(flow)),
    }
}

/// `(let* ((a 'alpha) (b 'beta) (s1 (copy-sequence "s")) (s2 (copy-sequence "s"))
///         (tg TARGET)) (cons tg LIST))`: the target and a one-cell list built
/// with the same `alpha`/`s1` objects, so `same-target`, `bare-a` and
/// `same-string` share identity with the target where the oracle says so.
fn case_operands(eval: &mut Context, op: bool, target: &str, element: &str) -> (Value, Value) {
    let list = if op {
        format!("(list (cons {element} 'entry-marker))")
    } else {
        format!("(list {element})")
    };
    let pair = eval
        .eval_str(&format!(
            "(let* ((a 'alpha) (b 'beta) \
                     (s1 (copy-sequence \"s\")) (s2 (copy-sequence \"s\")) \
                     (tg {target})) (cons tg {list}))"
        ))
        .expect("case operands");
    crate::emacs_core::eval::push_scratch_gc_root(pair);
    (pair.cons_car(), pair.cons_cdr())
}

const TARGETS: [&str; 6] = [
    "a",
    "(position-symbol 'alpha 1)",
    "(position-symbol 'beta 2)",
    "42",
    "s1",
    "nil",
];

/// `same-target` is `tg` itself; `same-string` is the target's `s1`, while
/// `string` is a distinct string with the same contents.
const ELEMENTS: [&str; 8] = [
    "tg",
    "a",
    "(position-symbol 'alpha 3)",
    "(position-symbol 'beta 4)",
    "42",
    "s2",
    "nil",
    "s1",
];

/// GNU 31.1 `(memq TARGET (list ELEMENT))`, `[swp][target][element]`.
const GNU_MEMQ: [[[i8; 8]; 6]; 2] = [
    [
        [0, 0, -1, -1, -1, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, -1],
        [0, -1, -1, -1, 0, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, 0],
        [0, -1, -1, -1, -1, -1, 0, -1],
    ],
    [
        [0, 0, 0, -1, -1, -1, -1, -1],
        [0, 0, 0, -1, -1, -1, -1, -1],
        [0, -1, -1, 0, -1, -1, -1, -1],
        [0, -1, -1, -1, 0, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, 0],
        [0, -1, -1, -1, -1, -1, 0, -1],
    ],
];

/// GNU 31.1 `(assq TARGET (list (cons ELEMENT 'entry-marker)))`, same axes.
const GNU_ASSQ: [[[i8; 8]; 6]; 2] = [
    [
        [0, 0, -1, -1, -1, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, -1],
        [0, -1, -1, -1, 0, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, 0],
        [0, -1, -1, -1, -1, -1, 0, -1],
    ],
    [
        [0, 0, 0, -1, -1, -1, -1, -1],
        [0, 0, 0, -1, -1, -1, -1, -1],
        [0, -1, -1, 0, -1, -1, -1, -1],
        [0, -1, -1, -1, 0, -1, -1, -1],
        [0, -1, -1, -1, -1, -1, -1, 0],
        [0, -1, -1, -1, -1, -1, 0, -1],
    ],
];

/// Every target kind x element kind, with positions enabled and disabled:
/// the shim's answer and the builtin's answer must both be GNU's.
#[test]
fn shim_answers_gnus_positioned_symbol_matrix() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for op in [false, true] {
        let expected = if op { &GNU_ASSQ } else { &GNU_MEMQ };
        for (swp, gnu) in [false, true].into_iter().zip(expected) {
            eval.symbols_with_pos_enabled = swp;
            for (ti, target_src) in TARGETS.iter().enumerate() {
                for (ei, element_src) in ELEMENTS.iter().enumerate() {
                    let (target, list) = case_operands(&mut eval, op, target_src, element_src);
                    let what = format!(
                        "({} {target_src} ({element_src})) swp={swp}",
                        if op { "assq" } else { "memq" }
                    );
                    let want = gnu[ti][ei].to_string();
                    let builtin = if op {
                        super::b::builtin_assq_values(target, list, swp)
                    } else {
                        super::b::builtin_memq_values(target, list, swp)
                    };
                    assert_eq!(index_answer(op, list, builtin), want, "{what}: the builtin");
                    assert_eq!(
                        index_answer(op, list, shim(op, ctx_ptr, target, list)),
                        want,
                        "{what}: the shim"
                    );
                }
            }
        }
    }
    eval.symbols_with_pos_enabled = false;
}

/// The shapes the walk refuses: a list longer than `LIST_SHIM_FAST_STEPS`
/// with no match inside it, an improper tail before a match, and a circular
/// list. A match inside the bound (including one through a position), the
/// end of a short proper list and an improper tail after a match stay on the
/// fast walk. `ARRAY_SHIM_SLOW_CALLS` counts the shim's builtin fallbacks.
#[test]
fn swp_walk_refuses_only_the_unbounded_shapes() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    eval.symbols_with_pos_enabled = true;
    // target, list, builtin fallback -- `memq` matches list cells, `assq`
    // matches the keys of cons entries, so the shapes differ.
    let memq_cases: &[(&str, &str, bool)] = &[
        ("'alpha", "(list 'alpha 'beta)", false),
        ("'zeta", "(list 'alpha 'beta)", false),
        (
            "(position-symbol 'alpha 9)",
            "(list 'qa 'alpha 'beta)",
            false,
        ),
        ("'alpha", "(list (position-symbol 'alpha 8) 'q)", false),
        ("'q", "(make-list 70 'q)", false),
        ("'zulu", "(append (make-list 63 'q) '(zulu))", false),
        ("'zulu", "(append (make-list 64 'q) '(zulu))", true),
        ("'zulu", "(make-list 70 'q)", true),
        ("'alpha", "'(alpha beta . tail)", false),
        ("'zulu", "'(alpha beta . tail)", true),
        (
            "'beta",
            "(let ((l (list 'q 'beta))) (setcdr (cdr l) l) l)",
            false,
        ),
        (
            "'zulu",
            "(let ((l (list 'q 'q))) (setcdr (cdr l) l) l)",
            true,
        ),
        (
            "'zulu",
            "(let ((l (make-list 100 'q))) (setcdr (nthcdr 99 l) l) l)",
            true,
        ),
        ("'alpha", "nil", false),
    ];
    let assq_cases: &[(&str, &str, bool)] = &[
        ("'alpha", "(list (cons 'alpha 1) (cons 'beta 2))", false),
        ("'zeta", "(list (cons 'alpha 1))", false),
        (
            "(position-symbol 'alpha 9)",
            "(list (cons 'qa 1) (cons 'alpha 2))",
            false,
        ),
        (
            "'alpha",
            "(list (cons (position-symbol 'alpha 8) 1))",
            false,
        ),
        (
            "0",
            "(let ((l nil) (i 70)) (while (> i 0) (setq i (1- i) l (cons (cons i i) l))) l)",
            false,
        ),
        (
            "'zulu",
            "(let ((l nil) (i 70)) (while (> i 0) (setq i (1- i) l (cons (cons i i) l))) l)",
            true,
        ),
        (
            "63",
            "(let ((l nil) (i 64)) (while (> i 0) (setq i (1- i) l (cons (cons i i) l))) l)",
            false,
        ),
        (
            "64",
            "(let ((l nil) (i 65)) (while (> i 0) (setq i (1- i) l (cons (cons i i) l))) l)",
            true,
        ),
        ("'alpha", "(cons (cons 'alpha 1) 'tail)", false),
        ("'zeta", "(cons (cons 'alpha 1) 'tail)", true),
        (
            "'beta",
            "(let ((l (list (cons 'q 1) (cons 'beta 2)))) (setcdr (cdr l) l) l)",
            false,
        ),
        (
            "'zulu",
            "(let ((l (list (cons 'q 1) (cons 'q 2)))) (setcdr (cdr l) l) l)",
            true,
        ),
        (
            "'zulu",
            "(let ((l (make-list 100 (cons 'q 0)))) (setcdr (nthcdr 99 l) l) l)",
            true,
        ),
        ("'alpha", "nil", false),
    ];
    for op in [false, true] {
        let f = search_fn(if op { Op::Assq } else { Op::Memq });
        let cases = if op { assq_cases } else { memq_cases };
        for &(target_src, list_src, fallback) in cases {
            let pair = eval
                .eval_str(&format!("(cons (cons {target_src} nil) {list_src})"))
                .expect("operands");
            crate::emacs_core::eval::push_scratch_gc_root(pair);
            let target = pair.cons_car().cons_car();
            let list = pair.cons_cdr();
            let what = format!(
                "({} {target_src} {list_src})",
                if op { "assq" } else { "memq" }
            );
            let want = index_answer(op, list, interpret(&mut eval, &f, vec![target, list]));
            ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
            let got = index_answer(op, list, shim(op, ctx_ptr, target, list));
            assert_eq!(got, want, "{what}: the shim");
            assert_eq!(
                ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()),
                usize::from(fallback),
                "{what}: builtin fallback = {fallback}"
            );
        }
    }
    eval.symbols_with_pos_enabled = false;
}

/// The compiled site: under `symbols-with-pos-enabled` the emitted intrinsic
/// misses (its walk is bit identity only), the shim runs, and its own walk
/// answers a short list -- so `LIST_SEARCH_SHIM_CALLS` rises while
/// `ARRAY_SHIM_SLOW_CALLS` stays put. With the flag off the intrinsic
/// answers short lists when it is on, and the shim's walk answers them when
/// it is off. Both intrinsic modes and both leaf knobs.
#[test]
fn compiled_sites_reach_the_shims_fast_walk_under_positions() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let short = eval.eval_str("'(x y z)").expect("short");
    let alist = eval.eval_str("'((x . 1) (y . 2))").expect("alist");
    let long = eval.eval_str("(make-list 70 'q)").expect("long");
    let swp_target = eval
        .eval_str("(position-symbol 'y 3)")
        .expect("symbol with pos");
    crate::emacs_core::eval::push_scratch_gc_root(short);
    crate::emacs_core::eval::push_scratch_gc_root(alist);
    crate::emacs_core::eval::push_scratch_gc_root(long);
    crate::emacs_core::eval::push_scratch_gc_root(swp_target);
    for op in [Op::Memq, Op::Assq] {
        let f = search_fn(op.clone());
        for intrinsics in [IntrinsicKnob::ALL, IntrinsicKnob::OFF] {
            for leaf_knob in [LeafKnob::DEFAULT, LeafKnob::OFF] {
                let leaf = compile_with(&f, leaf_knob, intrinsics);
                let inline = intrinsics.memq;
                let list = if op == Op::Assq { alist } else { short };
                let target = Value::symbol("y");
                // The flag off: the intrinsic (when on) or the shim's walk.
                eval.symbols_with_pos_enabled = false;
                let what = format!("{op:?} {intrinsics:?} {leaf_knob:?} flag off");
                let want = index_answer(
                    op == Op::Assq,
                    list,
                    interpret(&mut eval, &f, vec![target, list]),
                );
                LIST_SEARCH_SHIM_CALLS.with(|c| c.set(0));
                ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
                let got = native(ctx_ptr, &leaf, &[target, list], &what);
                assert_eq!(index_answer(op == Op::Assq, list, got), want, "{what}");
                assert_eq!(
                    LIST_SEARCH_SHIM_CALLS.with(|c| c.get()),
                    usize::from(!inline),
                    "{what}: compiled site reached the shim"
                );
                assert_eq!(
                    ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()),
                    0,
                    "{what}: short list stays on a fast path"
                );
                // The flag on, short list: the intrinsic misses, the walk hits.
                eval.symbols_with_pos_enabled = true;
                let what = format!("{op:?} {intrinsics:?} {leaf_knob:?} flag on");
                let target = swp_target;
                let want = index_answer(
                    op == Op::Assq,
                    list,
                    interpret(&mut eval, &f, vec![target, list]),
                );
                LIST_SEARCH_SHIM_CALLS.with(|c| c.set(0));
                ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
                let got = native(ctx_ptr, &leaf, &[target, list], &what);
                assert_eq!(index_answer(op == Op::Assq, list, got), want, "{what}");
                assert_eq!(
                    LIST_SEARCH_SHIM_CALLS.with(|c| c.get()),
                    1,
                    "{what}: the flag on is the shim's"
                );
                assert_eq!(
                    ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()),
                    0,
                    "{what}: the shim's walk answered"
                );
                // The flag on, 70 cells with no match: the walk gives up.
                let what = format!("{op:?} {intrinsics:?} {leaf_knob:?} flag on long");
                let missing = Value::symbol("zulu");
                let want = index_answer(
                    op == Op::Assq,
                    long,
                    interpret(&mut eval, &f, vec![missing, long]),
                );
                LIST_SEARCH_SHIM_CALLS.with(|c| c.set(0));
                ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
                let got = native(ctx_ptr, &leaf, &[missing, long], &what);
                assert_eq!(index_answer(op == Op::Assq, long, got), want, "{what}");
                assert_eq!(LIST_SEARCH_SHIM_CALLS.with(|c| c.get()), 1, "{what}");
                assert_eq!(
                    ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()),
                    1,
                    "{what}: the builtin answered"
                );
            }
        }
    }
    eval.symbols_with_pos_enabled = false;
}

/// The observed walk (`reads_need_observation`) answers exactly like the
/// unobserved one, for both flags and on both sides of the positioned-symbol
/// rule.
#[test]
fn observed_walks_answer_like_unobserved_ones() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let cases: &[(&str, &str)] = &[
        ("'alpha", "(list 'qa 'alpha 'qb)"),
        ("(position-symbol 'alpha 9)", "(list 'qa 'alpha 'qb)"),
        (
            "(position-symbol 'alpha 9)",
            "(list 'qa (position-symbol 'alpha 8) 'qb)",
        ),
        ("'alpha", "(list (position-symbol 'alpha 8) 'qb)"),
        ("(position-symbol 'zeta 9)", "(list 'qa 'qb)"),
        ("'zeta", "'(qa qb . tail)"),
        ("'beta", "(let ((l (list 'q 'beta))) (setcdr (cdr l) l) l)"),
        // Beyond the bounded walk: the observed slow path continues at the
        // spill tail (`builtin_memq_values_from`), and must still answer
        // like the unobserved one.
        ("'zulu", "(append (make-list 64 'q) '(zulu))"),
        ("'zulu", "(make-list 100 'q)"),
    ];
    for op in [false, true] {
        for swp in [false, true] {
            eval.symbols_with_pos_enabled = swp;
            for &(target_src, list_src) in cases {
                let pair = eval
                    .eval_str(&format!("(cons (cons {target_src} nil) {list_src})"))
                    .expect("operands");
                crate::emacs_core::eval::push_scratch_gc_root(pair);
                let target = pair.cons_car().cons_car();
                let list = pair.cons_cdr();
                let what = format!(
                    "({} {target_src} {list_src}) swp={swp}",
                    if op { "assq" } else { "memq" }
                );
                let plain = index_answer(op, list, shim(op, ctx_ptr, target, list));
                let (observed, _) = crate::tagged::collection_reads::capture(|| {
                    index_answer(op, list, shim(op, ctx_ptr, target, list))
                });
                assert_eq!(observed, plain, "{what}: observed walk");
            }
        }
    }
    eval.symbols_with_pos_enabled = false;
}

/// The direct shim answers agree with the interpreter's opcode arm as well.
#[test]
fn shim_answers_match_the_opcode_arm() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for op in [false, true] {
        let f = search_fn(if op { Op::Assq } else { Op::Memq });
        for swp in [false, true] {
            eval.symbols_with_pos_enabled = swp;
            for target_src in TARGETS {
                for element_src in ELEMENTS {
                    let (target, list) = case_operands(&mut eval, op, target_src, element_src);
                    let what = format!(
                        "({} {target_src} ({element_src})) swp={swp}",
                        if op { "assq" } else { "memq" }
                    );
                    let interpreted =
                        index_answer(op, list, interpret(&mut eval, &f, vec![target, list]));
                    let got = index_answer(op, list, shim(op, ctx_ptr, target, list));
                    assert_eq!(got, interpreted, "{what}");
                }
            }
        }
    }
    eval.symbols_with_pos_enabled = false;
}

/// The slow path continues at the tail the bounded walk reached instead of
/// re-walking the prefix from the head. That continuation must answer like
/// the walk from the head for every shape that spills: a match at or beyond
/// the bound, no match, and -- through the exact fallback, which always
/// restarts at the original head -- the same improper/circular signal with
/// the same data.
#[test]
fn memq_continuation_answers_like_the_full_scan() {
    use crate::emacs_core::builtins::{builtin_memq_values, builtin_memq_values_from};
    let mut eval = Context::new();
    // `LIST_SHIM_FAST_STEPS` is the dispatch module's constant (64, asserted
    // by `swp_walk_refuses_only_the_unbounded_shapes`' 63/64 cases).
    const FAST_STEPS: usize = 64;
    // (target, list, symbols-with-pos-enabled). Every list spills: either a
    // match at/after cell 64, no match, or an improper/circular tail.
    let cases: &[(&str, &str, bool)] = &[
        ("'zulu", "(append (make-list 64 'q) '(zulu))", true),
        ("'zulu", "(append (make-list 200 'q) '(zulu))", true),
        ("'zulu", "(make-list 100 'q)", true),
        ("'zulu", "'(alpha beta . tail)", true),
        (
            "'zulu",
            "(let ((l (make-list 100 'q))) (setcdr (nthcdr 99 l) l) l)",
            true,
        ),
        (
            "(position-symbol 'zulu 3)",
            "(append (make-list 70 'q) (list (position-symbol 'zulu 1)))",
            true,
        ),
        ("5", "(append (make-list 70 'q) '(5))", false),
        ("'zulu", "(append (make-list 70 'q) '(zulu))", false),
    ];
    for &(target_src, list_src, swp) in cases {
        eval.symbols_with_pos_enabled = swp;
        let pair = eval
            .eval_str(&format!("(cons (cons {target_src} nil) {list_src})"))
            .expect("operands");
        crate::emacs_core::eval::push_scratch_gc_root(pair);
        let target = pair.cons_car().cons_car();
        let list = pair.cons_cdr();
        // The tail the shim's fast walk would hand the builtin: 64 cells, or
        // the first improper cell if the list ends before that.
        let mut tail = list;
        let mut steps = 0;
        while tail.is_cons() && steps < FAST_STEPS {
            tail = tail.cons_cdr();
            steps += 1;
        }
        let what = format!("(memq {target_src} {list_src}) swp={swp}");
        let full = outcome(builtin_memq_values(target, list, swp));
        let continued = outcome(builtin_memq_values_from(target, list, tail, swp));
        assert_eq!(continued, full, "{what}: continuation from the spill tail");
    }
    eval.symbols_with_pos_enabled = false;
}

/// A comparable rendering of an answer: the value, or the signal's symbol
/// and every datum (the improper/circular signal carries the original list,
/// so the tail object is part of what must match).
fn outcome(result: Result<Value, Flow>) -> (String, Vec<String>) {
    match result {
        Ok(value) => ("ok".to_string(), vec![print_value(&value)]),
        Err(flow) => match flow.into_kind() {
            crate::emacs_core::error::FlowKind::Signal(sig) => (
                format!("signal {}", sig.symbol_name()),
                sig.data.iter().map(print_value).collect(),
            ),
            other => (format!("{other:?}"), Vec::new()),
        },
    }
}
