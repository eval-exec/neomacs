//! Deopt classification and the release deopt census.

use super::*;
use crate::emacs_core::bytecode::vm::Vm as TestVm;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::compile::{force_deopt_for_test, lower_nullary_leaf};
use crate::emacs_core::jit::stats;
use crate::emacs_core::value::LambdaParams;

/// Any leaf: `classify` reads only its `inline_epoch`.
fn plain_leaf() -> CompiledLeaf {
    lower_nullary_leaf(&[Op::Constant(0), Op::Return], &[Value::make_int(1)]).expect("compiles")
}

fn bignum() -> Value {
    Value::make_integer_from_str_or_zero("100000000000000000000000")
}

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

fn census() -> [u64; stats::DEOPT_CAUSES] {
    stats::compile_stats_snapshot().deopt_causes
}

fn delta(before: [u64; stats::DEOPT_CAUSES]) -> Vec<(&'static str, u64)> {
    DeoptCause::CENSUS_NAMES
        .iter()
        .zip(census().iter().zip(before))
        .filter(|&(_, (now, then))| *now != then)
        .map(|(name, (now, then))| (*name, now - then))
        .collect()
}

/// Every arithmetic op with a generic fallback classifies its top `nargs`
/// operands: a float is `ArithOperands(Float)`, a bignum or a non-number is
/// `ArithOperands(Other)`, and all fixnums is an overflow.
#[test]
fn classify_arith_float_other_overflow() {
    let leaf = plain_leaf();
    let arith = [
        Op::Add,
        Op::Sub,
        Op::Mul,
        Op::Div,
        Op::Rem,
        Op::Max,
        Op::Min,
        Op::Eqlsign,
        Op::Lss,
        Op::Gtr,
        Op::Leq,
        Op::Geq,
        Op::Add1,
        Op::Sub1,
        Op::Negate,
    ];
    for op in arith {
        let (_, nargs) = Vm::arith_generic_kind(&op).expect("an arithmetic op");
        let ops = [Op::Nil, op.clone()];
        // One unrelated slot below the operands: only the top `nargs` count.
        let with = |v: Value| {
            let mut stack = vec![Value::string("below")];
            stack.extend(std::iter::repeat_n(Value::make_int(7), nargs - 1));
            stack.push(v);
            stack
        };
        let cls =
            |stack: &[Value]| classify(std::ptr::null(), &ops, &leaf, LeafOrigin::Entry, 1, stack);
        assert_eq!(
            cls(&with(Value::make_float(1.5))),
            DeoptCause::ArithOperands(NumericFeedback::Float),
            "{op:?}"
        );
        assert_eq!(
            cls(&with(bignum())),
            DeoptCause::ArithOperands(NumericFeedback::Other),
            "{op:?}"
        );
        assert_eq!(
            cls(&with(Value::NIL)),
            DeoptCause::ArithOperands(NumericFeedback::Other),
            "{op:?}"
        );
        assert_eq!(
            cls(&with(Value::make_int(Value::MOST_POSITIVE_FIXNUM))),
            DeoptCause::ArithOverflow,
            "{op:?}"
        );
    }
}

/// The non-arithmetic causes: a call site, a stale MIR inline epoch, car/cdr,
/// an OSR entry guard (pc == header, stack == snapshot) and the rest.
#[test]
fn classify_call_car_osr_entry_unattributed() {
    let mut ev = Context::new();
    let ctx = &mut ev as *const Context;
    let mut leaf = plain_leaf();
    let ops = [
        Op::StackRef(0),
        Op::Call(1),
        Op::Car,
        Op::Cdr,
        Op::Add,
        Op::VarRef(0),
    ];
    let stack = [Value::make_int(1), Value::make_int(2)];
    let cls = |leaf: &CompiledLeaf, origin: LeafOrigin<'_>, pc: usize, stack: &[Value]| {
        classify(ctx, &ops, leaf, origin, pc, stack)
    };
    assert_eq!(
        cls(&leaf, LeafOrigin::Entry, 1, &stack),
        DeoptCause::InlinedCall
    );
    // A MIR leaf that inlined at an epoch the obarray has since left.
    let now = ev.obarray.function_epoch();
    leaf.inline_epoch = Some(now.wrapping_sub(1));
    assert_eq!(
        cls(&leaf, LeafOrigin::Entry, 1, &stack),
        DeoptCause::InlineEpochMoved
    );
    leaf.inline_epoch = Some(now);
    assert_eq!(
        cls(&leaf, LeafOrigin::Entry, 1, &stack),
        DeoptCause::InlinedCall,
        "an unmoved epoch is a guard failure inside the inlined call"
    );
    // A null Context cannot read the epoch: a plain call-site cause.
    leaf.inline_epoch = Some(now.wrapping_sub(1));
    assert_eq!(
        classify(std::ptr::null(), &ops, &leaf, LeafOrigin::Entry, 1, &stack),
        DeoptCause::InlinedCall
    );
    leaf.inline_epoch = None;
    assert_eq!(
        cls(&leaf, LeafOrigin::Entry, 2, &stack),
        DeoptCause::TypeError
    );
    assert_eq!(
        cls(&leaf, LeafOrigin::Entry, 3, &stack),
        DeoptCause::TypeError
    );
    assert_eq!(
        cls(&leaf, LeafOrigin::Entry, 5, &stack),
        DeoptCause::Unattributed
    );
    assert_eq!(
        cls(&leaf, LeafOrigin::Entry, 99, &stack),
        DeoptCause::Unattributed,
        "a pc outside the body"
    );
    // OSR entry guard: the header pc with the untouched snapshot.
    let osr = LeafOrigin::Osr {
        header_pc: 0,
        snapshot: &stack,
    };
    assert_eq!(cls(&leaf, osr, 0, &stack), DeoptCause::OsrEntry);
    assert_eq!(
        cls(&leaf, osr, 0, &[Value::make_int(1), Value::make_int(3)]),
        DeoptCause::Unattributed,
        "a different stack at the header is not the entry snapshot"
    );
    // An arithmetic header whose operand is a float is still the operand
    // cause, not the entry guard.
    let fl = [Value::make_int(1), Value::make_float(2.0)];
    let osr_add = LeafOrigin::Osr {
        header_pc: 4,
        snapshot: &fl,
    };
    assert_eq!(
        cls(&leaf, osr_add, 4, &fl),
        DeoptCause::ArithOperands(NumericFeedback::Float)
    );
    // ...and an all-fixnum arithmetic header is the entry guard.
    let osr_add = LeafOrigin::Osr {
        header_pc: 4,
        snapshot: &stack,
    };
    assert_eq!(cls(&leaf, osr_add, 4, &stack), DeoptCause::OsrEntry);
}

/// Every census bucket has a distinct index and a name.
#[test]
fn census_buckets_are_distinct() {
    let causes = [
        DeoptCause::ArithOperands(NumericFeedback::Float),
        DeoptCause::ArithOperands(NumericFeedback::Other),
        DeoptCause::ArithOverflow,
        DeoptCause::InlinedCall,
        DeoptCause::InlineEpochMoved,
        DeoptCause::TypeError,
        DeoptCause::OsrEntry,
        DeoptCause::Rerun,
        DeoptCause::Unattributed,
    ];
    let mut seen: Vec<usize> = causes.iter().map(|c| c.census_index()).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), DeoptCause::CENSUS_NAMES.len());
    assert_eq!(*seen.last().unwrap(), DeoptCause::CENSUS_NAMES.len() - 1);
}

/// Through the tier-up seam: a leaf compiled on fixnum feedback and fed a
/// float is counted once, whichever deopt (precise or rerun) its tier takes.
#[test]
fn census_counts_a_tier_up_seam_deopt_once() {
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    // (lambda (x) (+ x 1))
    let f = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    let f_val = Value::make_bytecode(f.clone());
    let run = |arg: Value| {
        crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[arg]).expect("no signal")
    };
    assert_eq!(run(Value::make_int(41)), Some(Value::make_int(42).bits()));
    let before = census();
    let _ = run(Value::make_float(1.5));
    let d = delta(before);
    assert!(
        d == vec![("arith_float", 1)] || d == vec![("rerun", 1)],
        "exactly one deopt, precise with a float operand or a MIR rerun: {d:?}"
    );
    assert_eq!(stats::compile_stats_snapshot().deopt_osr, 0);
}

/// A native-to-native (speculated) call whose callee deopts precisely is
/// counted once, on the callee's path, per failing run.
#[test]
fn census_counts_a_direct_path_deopt_once() {
    force_deopt_for_test(false);
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // A multi-block callee, so the MIR inliner leaves the call in place:
    // (lambda (n) (if n (1- n) 0))
    let step = Value::symbol("reopt-census-step");
    ev.obarray.set_symbol_function_id(
        step.as_symbol_id().unwrap(),
        Value::make_bytecode(function(
            vec![
                Op::StackRef(0),
                Op::GotoIfNil(5),
                Op::StackRef(0),
                Op::Sub1,
                Op::Return,
                Op::Constant(0),
                Op::Return,
            ],
            vec![Value::make_int(0)],
            1,
        )),
    );
    // (lambda (x) (reopt-census-step x))
    let f = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![step],
        1,
    );
    let leaf =
        crate::emacs_core::jit::compile::compile_bytecode_function_with(&f, Some(&ev.obarray))
            .expect("compiles");
    let ctx = &mut ev as *mut Context as *mut u8;
    for _ in 0..3 {
        assert_eq!(
            leaf.call(ctx, &[Value::make_int(5)]),
            crate::emacs_core::jit::compile::NativeRun::Ok(Value::make_int(4).bits())
        );
    }
    let before = census();
    match leaf.call(ctx, &[Value::make_float(5.0)]) {
        crate::emacs_core::jit::compile::NativeRun::Ok(bits) => {
            assert_eq!(Value::from_bits(bits).as_float(), Some(4.0))
        }
        other => panic!("expected the resumed result, got {other:?}"),
    }
    let d = delta(before);
    assert!(
        d == vec![("arith_float", 1)] || d == vec![("rerun", 1)],
        "one deopt for one failing callee run: {d:?}"
    );
}

/// An OSR leaf's precise deopt is counted once, as an OSR deopt: here a
/// `1+` that leaves the fixnum range mid-loop (an overflow).
#[test]
fn census_counts_an_osr_deopt_once() {
    force_deopt_for_test(false);
    // (lambda (start n) (let ((i 0) (x start))
    //   (while (< i n) (setq i (1+ i)) (setq x (1+ x))) i))
    let f = function(
        vec![
            Op::Constant(0), // i = 0            [start n i]
            Op::StackRef(2), // x = start        [start n i x]
            Op::StackRef(1), // 2: header -- i
            Op::StackRef(3), // n
            Op::Lss,
            Op::GotoIfNil(13),
            Op::StackRef(1), // i
            Op::Add1,
            Op::StackSet(2),
            Op::StackRef(0), // x
            Op::Add1,        // 10: leaves the fixnum range mid-loop
            Op::StackSet(1),
            Op::Goto(2),
            Op::StackRef(1), // 13: i
            Op::Return,
        ],
        vec![Value::make_int(0)],
        2,
    );
    let mut ev = Context::new();
    crate::emacs_core::jit::force_osr_for_test(true);
    f.jit_runtime().set_hot_for_test();
    let before = census();
    let osr_before = stats::compile_stats_snapshot().deopt_osr;
    let start = Value::make_int(Value::MOST_POSITIVE_FIXNUM - 1500);
    let got = TestVm::from_context(&mut ev)
        .execute(&f, vec![start, Value::make_int(2000)])
        .expect("runs");
    crate::emacs_core::jit::force_osr_for_test(false);
    assert_eq!(got, Value::make_int(2000));
    assert_eq!(delta(before), vec![("overflow", 1)]);
    assert_eq!(stats::compile_stats_snapshot().deopt_osr - osr_before, 1);
}

/// The census renders in the summary line.
#[test]
fn census_renders_in_the_summary() {
    let mut s = stats::CompileStats::default();
    assert!(stats::format_summary(&s).contains("deopts[total=0 osr=0]"));
    s.deopt_causes[DeoptCause::ArithOverflow.census_index()] = 3;
    s.deopt_causes[DeoptCause::Rerun.census_index()] = 2;
    s.deopt_osr = 1;
    let line = stats::format_summary(&s);
    assert!(
        line.contains("deopts[total=5 osr=1 overflow=3 rerun=2]"),
        "{line}"
    );
}

/// `NEOVM_JIT_FORCE_DEOPT=1` alone makes reoptimization inert; the stress
/// harness keeps it on; `NEOVM_JIT_REOPT=off` wins over both.
#[test]
fn reopt_inert_under_force_deopt_unless_stressed() {
    force_deopt_for_test(false);
    force_reopt_for_test(Some(ReoptKnobs::defaults()));
    assert!(reopt_enabled());
    force_deopt_for_test(true);
    assert!(
        !reopt_enabled(),
        "the every-guard-fails harness keeps its deopts"
    );
    force_reopt_for_test(Some(ReoptKnobs::stress()));
    assert!(reopt_enabled(), "unless the stress harness asks for both");
    force_reopt_for_test(Some(ReoptKnobs {
        enabled: false,
        ..ReoptKnobs::stress()
    }));
    assert!(!reopt_enabled(), "off wins");
    force_deopt_for_test(false);
    force_reopt_for_test(Some(ReoptKnobs::off()));
    assert!(!reopt_enabled());
    let stress = ReoptKnobs::stress();
    assert_eq!(
        (stress.heat, stress.max_reopts, stress.site_limit),
        (1, 1, 1)
    );
    let d = ReoptKnobs::defaults();
    assert_eq!(
        (d.max_reopts, d.site_limit),
        (ReoptKnobs::MAX_REOPTS, ReoptKnobs::SITE_LIMIT)
    );
    force_reopt_for_test(None);
}
