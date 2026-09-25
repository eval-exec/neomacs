//! Deopt-driven reoptimization end to end, through a real `Context`: the
//! cliff probes of the design (a leaf compiled on fixnum feedback then fed
//! floats; a multiply that keeps overflowing to a bignum) and the lifecycle
//! around them (the stale leaf, closures, AOT prewarm, the knobs). Every
//! Lisp result is checked against the interpreter.

use super::*;
use crate::emacs_core::bytecode::vm::Vm as TestVm;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::cache::{
    cache_entry_kind_for_test, compiled_leaf_ptr_for_test, try_run_compiled,
};
use crate::emacs_core::jit::compile::{force_deopt_for_test, force_profit_gate_for_test};
use crate::emacs_core::jit::stats;
use crate::emacs_core::value::{LambdaParams, eql_value};

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

/// Pc of the `*` in [`j4_add`].
const MUL_PC: usize = 9;
/// Pc of the `+` in [`j4_add`].
const ADD_PC: usize = 10;

/// `(lambda (a b) (let ((s 0)) (dotimes (_ 3) (setq s (+ s (* a b)))) s))`,
/// the `j4-add` of the design's `deopt6.el` probe. A loop, so its deopts are
/// precise whichever tier compiles it.
fn j4_add() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),   // 0: s = 0            [a b s]
            Op::Constant(0),   // 1: i = 0            [a b s i]
            Op::StackRef(0),   // 2: header: i
            Op::Constant(1),   // 3: 3
            Op::Lss,           // 4
            Op::GotoIfNil(16), // 5
            Op::StackRef(1),   // 6: s                [a b s i s]
            Op::StackRef(4),   // 7: a
            Op::StackRef(4),   // 8: b
            Op::Mul,           // 9: MUL_PC
            Op::Add,           // 10: ADD_PC
            Op::StackSet(2),   // 11: s = s + a*b      [a b s i]
            Op::StackRef(0),   // 12: i
            Op::Add1,          // 13
            Op::StackSet(1),   // 14: i = i + 1
            Op::Goto(2),       // 15
            Op::StackRef(1),   // 16: s
            Op::Return,        // 17
        ],
        vec![Value::make_int(0), Value::make_int(3)],
        2,
    )
}

/// A caller that reaches `sym` through a speculated call site:
/// `(lambda (x) (sym x x))`.
fn caller_of(sym: Value) -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![sym],
        1,
    )
}

fn install(ev: &mut Context, name: &str, f: ByteCodeFunction) -> Value {
    let sym = Value::symbol(name);
    ev.obarray
        .set_symbol_function_id(sym.as_symbol_id().unwrap(), Value::make_bytecode(f));
    sym
}

fn bytecode_of(ev: &Context, sym: Value) -> &'static ByteCodeFunction {
    ev.obarray
        .symbol_function_id(sym.as_symbol_id().unwrap())
        .and_then(Value::get_bytecode_data)
        .expect("bytecode")
}

/// The interpreter's answer for `(j4-add x x)` (a fresh, never-compiled copy).
fn interp_j4(ev: &mut Context, x: Value) -> Value {
    TestVm::from_context(ev)
        .execute(&j4_add(), vec![x, x])
        .expect("interp run")
}

/// A caller compiled into the cache, with the callee `name` installed.
struct Probe {
    ev: Box<Context>,
    sym: Value,
    caller: ByteCodeFunction,
    caller_val: Value,
}

impl Probe {
    fn new(name: &str) -> Self {
        force_deopt_for_test(false);
        // The caller is one call and no arithmetic.
        force_profit_gate_for_test(false);
        let mut ev = Box::new(Context::new());
        let sym = install(&mut ev, name, j4_add());
        let caller = caller_of(sym);
        let caller_val = Value::make_bytecode(caller.clone());
        Probe {
            ev,
            sym,
            caller,
            caller_val,
        }
    }

    fn ctx(&mut self) -> *mut Context {
        &mut *self.ev as *mut Context
    }

    /// Run the compiled caller on `x`; the result, checked against the
    /// interpreter.
    fn call(&mut self, x: Value) -> Value {
        let ctx = self.ctx();
        let bits = try_run_compiled(ctx, &self.caller, self.caller_val, &[x])
            .expect("no signal")
            .expect("the caller runs native");
        let got = Value::from_bits(bits);
        let want = interp_j4(&mut self.ev, x);
        assert!(
            eql_value(&got, &want),
            "native {got:?} != interp {want:?} for x={x:?}"
        );
        got
    }

    fn callee(&self) -> &'static ByteCodeFunction {
        bytecode_of(&self.ev, self.sym)
    }

    fn callee_id(&self) -> u64 {
        self.callee()
            .jit_runtime()
            .compiled_id()
            .expect("callee id")
    }

    fn kind(&self) -> &'static str {
        cache_entry_kind_for_test(self.callee_id())
    }

    /// The caller's speculated-call slot.
    fn caller_slot(&self) -> &'static crate::emacs_core::jit::compile::SpecSlot {
        let id = self.caller.jit_runtime().compiled_id().expect("caller id");
        let leaf = compiled_leaf_ptr_for_test(id).expect("caller cached");
        // SAFETY: cached (and later at worst retired) leaves stay allocated
        // for the test's duration.
        unsafe { &(*leaf).spec_slots[0] }
    }
}

fn knobs_with_heat(heat: u32) -> ReoptKnobs {
    ReoptKnobs {
        heat,
        ..ReoptKnobs::defaults()
    }
}

/// The design's P1 cliff: a leaf compiled on fixnum feedback and then fed
/// floats deopted on every call, forever. Now the first float deopt widens
/// the site, retires the leaf, clears the caller's spec slot, and the source
/// recompiles against the widened feedback after the re-profile window; the
/// floats then run native with no deopt.
#[test]
fn float_after_fixnum_warmup_widens_invalidates_and_recompiles() {
    let mut p = Probe::new("reopt-p1-add");
    force_reopt_for_test(Some(knobs_with_heat(5)));
    for _ in 0..3 {
        assert_eq!(p.call(Value::make_int(3)), Value::make_int(27));
    }
    assert_eq!(p.kind(), "compiled");
    let rt = p.callee().jit_runtime();
    let old = compiled_leaf_ptr_for_test(p.callee_id()).expect("compiled");
    assert_eq!(
        p.caller_slot().leaf_ptr(),
        old,
        "the caller's slot caches it"
    );
    assert_eq!(rt.numeric_feedback(MUL_PC), NumericFeedback::FixnumOnly);
    let invalidations = stats::compile_stats_snapshot().reopt_levels[0];

    // The first float: one precise deopt, conclusive.
    assert_eq!(p.call(Value::make_float(3.0)).as_float(), Some(27.0));
    assert_eq!(rt.numeric_feedback(MUL_PC), NumericFeedback::Float);
    assert_eq!(p.kind(), "deferred-reopt");
    assert_eq!(rt.reopt_count(), 1);
    assert_eq!(rt.reopt_level(), ReoptLevel::Speculative);
    assert!(
        p.caller_slot().leaf_ptr().is_null(),
        "the caller was unlinked"
    );
    // SAFETY: retired leaves stay allocated.
    assert!(unsafe { (*old).retired.get() }, "the stale leaf is retired");
    assert!(rt.wants_numeric_feedback(), "the interpreter records again");
    assert_eq!(
        stats::compile_stats_snapshot().reopt_levels[0],
        invalidations + 1
    );

    // The re-profile window, then the recompile.
    for _ in 0..10 {
        assert_eq!(p.call(Value::make_float(3.0)).as_float(), Some(27.0));
    }
    assert_eq!(p.kind(), "compiled", "recompiled after the window");
    let new = compiled_leaf_ptr_for_test(p.callee_id()).expect("compiled");
    assert_ne!(new, old);
    // SAFETY: the cache holds it.
    assert_eq!(unsafe { (*new).compiled_level }, ReoptLevel::Speculative);
    assert_eq!(
        rt.numeric_feedback(ADD_PC),
        NumericFeedback::Float,
        "the window recorded the float sum too"
    );
    assert!(!rt.wants_numeric_feedback(), "the recompile consumed it");

    // Floats now run native without a deopt, through the re-armed slot.
    let deopts = stats::compile_stats_snapshot().deopts();
    for _ in 0..20 {
        assert_eq!(p.call(Value::make_float(3.0)).as_float(), Some(27.0));
    }
    assert_eq!(stats::compile_stats_snapshot().deopts(), deopts);
    assert_eq!(p.caller_slot().leaf_ptr(), new, "the caller re-armed");
    // Fixnums still run correctly (the float lowering promotes them).
    assert_eq!(p.call(Value::make_int(3)), Value::make_int(27));
    assert_eq!(rt.reopt_count(), 1, "one invalidation in all");
    force_reopt_for_test(None);
}

/// The design's P3 cliff: `*` of fixnums overflowing to a bignum deopted on
/// every call, and even a correct warm-up could not fix it (an overflow
/// records nothing). One overflow must not make a fixnum-hot site generic,
/// so only `site_limit` overflows at the site widen it to `Other`; the
/// recompile takes the generic fallback and never deopts again.
#[test]
fn overflow_widens_to_generic_only_at_the_site_limit() {
    let mut p = Probe::new("reopt-p3-sq");
    force_reopt_for_test(Some(knobs_with_heat(5)));
    for _ in 0..3 {
        assert_eq!(p.call(Value::make_int(3)), Value::make_int(27));
    }
    let rt = p.callee().jit_runtime();
    let big = Value::make_int(3_037_000_500); // big * big > most-positive-fixnum
    let limit = ReoptKnobs::SITE_LIMIT;
    for _ in 1..limit {
        assert!(!p.call(big).is_fixnum(), "a bignum");
        assert_eq!(p.kind(), "compiled", "below the site limit");
        assert_eq!(rt.numeric_feedback(MUL_PC), NumericFeedback::FixnumOnly);
    }
    assert!(!p.call(big).is_fixnum());
    assert_eq!(rt.numeric_feedback(MUL_PC), NumericFeedback::Other);
    assert_eq!(p.kind(), "deferred-reopt");
    for _ in 0..10 {
        assert!(!p.call(big).is_fixnum());
    }
    assert_eq!(p.kind(), "compiled");
    assert_eq!(rt.numeric_feedback(ADD_PC), NumericFeedback::Other);
    let deopts = stats::compile_stats_snapshot().deopts();
    for _ in 0..20 {
        assert!(!p.call(big).is_fixnum());
    }
    assert_eq!(p.call(Value::make_int(3)), Value::make_int(27));
    assert_eq!(
        stats::compile_stats_snapshot().deopts(),
        deopts,
        "the generic fallback never deopts"
    );
    force_reopt_for_test(None);
}

/// A leaf already retired that still runs through some caller's spec slot
/// deopts: its callers are unlinked, and the current leaf stays cached.
#[test]
fn stale_leaf_deopt_unlinks_but_does_not_evict_successor() {
    let mut p = Probe::new("reopt-stale-add");
    force_reopt_for_test(Some(knobs_with_heat(5)));
    for _ in 0..3 {
        p.call(Value::make_int(3));
    }
    let old = compiled_leaf_ptr_for_test(p.callee_id()).expect("compiled");
    for _ in 0..12 {
        p.call(Value::make_float(3.0));
    }
    assert_eq!(p.kind(), "compiled");
    let new = compiled_leaf_ptr_for_test(p.callee_id()).expect("recompiled");
    assert_ne!(new, old);
    // Point the caller's slot back at the stale fixnum leaf (a test hook for
    // what a missed unlink would leave behind).
    let slot = p.caller_slot();
    slot.arm_leaf(old, p.callee().constants.as_ptr(), false, false);
    let stale = stats::compile_stats_snapshot().reopt_stale;
    let count = p.callee().jit_runtime().reopt_count();
    assert_eq!(p.call(Value::make_float(3.0)).as_float(), Some(27.0));
    assert_eq!(stats::compile_stats_snapshot().reopt_stale, stale + 1);
    assert_eq!(p.kind(), "compiled", "the successor is not evicted");
    assert_eq!(compiled_leaf_ptr_for_test(p.callee_id()), Some(new));
    assert_eq!(
        p.callee().jit_runtime().reopt_count(),
        count,
        "a stale deopt is no invalidation"
    );
    assert!(slot.leaf_ptr().is_null() || slot.leaf_ptr() == new);
    assert_eq!(p.call(Value::make_float(3.0)).as_float(), Some(27.0));
    assert_eq!(
        slot.leaf_ptr(),
        new,
        "the caller re-resolved the current leaf"
    );
    force_reopt_for_test(None);
}

/// `make-closure` instances share their source's runtime: a deopt through
/// one instance invalidates the leaf every instance runs, and all of them
/// run the recompiled one.
#[test]
fn closure_instances_share_the_reopt_state() {
    force_deopt_for_test(false);
    force_reopt_for_test(Some(knobs_with_heat(3)));
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f1 = j4_add();
    let f2 = f1.clone(); // shares the Runtime, as make-closure's copy does
    let (v1, v2) = (
        Value::make_bytecode(f1.clone()),
        Value::make_bytecode(f2.clone()),
    );
    f1.jit_runtime().set_hot_for_test();
    let run = |f: &ByteCodeFunction, v: Value, x: Value| {
        try_run_compiled(ctx, f, v, &[x, x]).expect("no signal")
    };
    assert_eq!(
        run(&f1, v1, Value::make_int(3)),
        Some(Value::make_int(27).bits())
    );
    let id = f1.jit_runtime().compiled_id().expect("compiled");
    assert_eq!(
        f2.jit_runtime().compiled_id(),
        Some(id),
        "one source, one id"
    );
    let fl = Value::make_float(3.0);
    let got = run(&f2, v2, fl).expect("a precise resume returns the value");
    assert_eq!(Value::from_bits(got).as_float(), Some(27.0));
    assert_eq!(cache_entry_kind_for_test(id), "deferred-reopt");
    assert_eq!(f1.jit_runtime().reopt_count(), 1, "seen through instance 1");
    assert_eq!(run(&f1, v1, fl), None, "interpreted during the window");
    // Past the window the next native attempt recompiles, for both.
    f1.jit_runtime()
        .set_heat_for_test(f1.jit_runtime().heat() + 10);
    assert!(run(&f1, v1, fl).is_some());
    assert_eq!(cache_entry_kind_for_test(id), "compiled");
    let deopts = stats::compile_stats_snapshot().deopts();
    assert!(run(&f2, v2, fl).is_some());
    assert!(run(&f1, v1, fl).is_some());
    assert_eq!(stats::compile_stats_snapshot().deopts(), deopts);
    force_reopt_for_test(None);
}

/// An AOT-prewarmed source's dispatcher answers `Compiled` from call 1; an
/// invalidation clears that, so the re-profile window holds.
#[test]
fn aot_prewarmed_source_honours_the_reopt_deferral() {
    force_deopt_for_test(false);
    force_reopt_for_test(Some(knobs_with_heat(1_000)));
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = j4_add();
    let v = Value::make_bytecode(f.clone());
    let rt = f.jit_runtime();
    rt.mark_aot_prewarmed();
    assert!(
        try_run_compiled(ctx, &f, v, &[Value::make_int(3), Value::make_int(3)])
            .expect("no signal")
            .is_some()
    );
    let len = f.executable_ops().len();
    assert!(matches!(
        rt.dispatch_sized(len),
        crate::emacs_core::jit::Plan::Compiled
    ));
    let fl = Value::make_float(3.0);
    try_run_compiled(ctx, &f, v, &[fl, fl]).expect("no signal");
    assert_eq!(rt.reopt_count(), 1);
    assert!(
        matches!(
            rt.dispatch_sized(len),
            crate::emacs_core::jit::Plan::Interpret
        ),
        "the window holds for a prewarmed source too"
    );
    force_reopt_for_test(None);
}

/// `NEOVM_JIT_REOPT=off` is the pre-reoptimization behaviour: the stale leaf
/// stays cached and deopts on every call, and nothing is widened.
#[test]
fn reopt_off_keeps_the_stale_leaf() {
    let mut p = Probe::new("reopt-off-add");
    force_reopt_for_test(Some(ReoptKnobs::off()));
    for _ in 0..3 {
        p.call(Value::make_int(3));
    }
    let leaf = compiled_leaf_ptr_for_test(p.callee_id()).expect("compiled");
    let deopts = stats::compile_stats_snapshot().deopts();
    for _ in 0..5 {
        assert_eq!(p.call(Value::make_float(3.0)).as_float(), Some(27.0));
    }
    assert_eq!(
        stats::compile_stats_snapshot().deopts(),
        deopts + 5,
        "one per call"
    );
    assert_eq!(p.kind(), "compiled");
    assert_eq!(compiled_leaf_ptr_for_test(p.callee_id()), Some(leaf));
    let rt = p.callee().jit_runtime();
    assert_eq!(rt.reopt_count(), 0);
    assert_eq!(rt.numeric_feedback(MUL_PC), NumericFeedback::FixnumOnly);
    force_reopt_for_test(None);
}

/// Under the every-guard-fails harness alone, reoptimization is inert: the
/// harness keeps exercising every deopt path.
#[test]
fn force_deopt_harness_keeps_reopt_inert() {
    let mut p = Probe::new("reopt-forced-add");
    force_reopt_for_test(Some(knobs_with_heat(5)));
    force_deopt_for_test(true);
    for _ in 0..8 {
        assert_eq!(p.call(Value::make_int(3)), Value::make_int(27));
    }
    assert_eq!(p.kind(), "compiled");
    assert_eq!(p.callee().jit_runtime().reopt_count(), 0);
    force_deopt_for_test(false);
    force_reopt_for_test(None);
}

// --- OSR: an invalidated OSR leaf may be re-entered. ---

use std::sync::atomic::Ordering as AtomicOrdering;

/// Pc of the `*` in [`osr_switch`].
const OSR_MUL_PC: usize = 16;
/// The loop header of [`osr_switch`].
const OSR_HEADER: usize = 3;

/// The design's P2 probe: `(lambda (n k) (let ((i 0) (s 0) (x 1))
///   (while (< i n) (when (= i k) (setq x 1.5)) (setq s (+ s (* x 2)))
///     (setq i (1+ i))) s))` — a loop whose operands switch from fixnums to
/// floats at iteration `k`.
fn osr_switch() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),   // 0: i = 0              [n k i]
            Op::Constant(0),   // 1: s = 0              [n k i s]
            Op::Constant(1),   // 2: x = 1              [n k i s x]
            Op::StackRef(2),   // 3: OSR_HEADER: i
            Op::StackRef(5),   // 4: n
            Op::Lss,           // 5
            Op::GotoIfNil(23), // 6
            Op::StackRef(2),   // 7: i
            Op::StackRef(4),   // 8: k
            Op::Eqlsign,       // 9
            Op::GotoIfNil(13), // 10
            Op::Constant(2),   // 11: 1.5
            Op::StackSet(1),   // 12: x = 1.5
            Op::StackRef(1),   // 13: s
            Op::StackRef(1),   // 14: x
            Op::Constant(3),   // 15: 2
            Op::Mul,           // 16: OSR_MUL_PC
            Op::Add,           // 17
            Op::StackSet(2),   // 18: s = s + x*2
            Op::StackRef(2),   // 19: i
            Op::Add1,          // 20
            Op::StackSet(3),   // 21: i = i + 1
            Op::Goto(3),       // 22
            Op::StackRef(1),   // 23: s
            Op::Return,        // 24
        ],
        vec![
            Value::make_int(0),
            Value::make_int(1),
            Value::make_float(1.5),
            Value::make_int(2),
        ],
        2,
    )
}

fn osr_transfers() -> u64 {
    crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(AtomicOrdering::Relaxed)
}

/// P2: an OSR'd loop whose operands switch to floats deopted once and then
/// interpreted the rest of the loop. Now the deopt widens the site and
/// retires the OSR leaf, and the next hot back-edge transfers into a leaf
/// compiled from the widened feedback, which finishes the loop natively.
#[test]
fn osr_float_switch_reenters_native() {
    force_deopt_for_test(false);
    force_reopt_for_test(Some(knobs_with_heat(5)));
    let mut ev = Context::new();
    let (n, k) = (Value::make_int(20_000), Value::make_int(1_000));
    let want = TestVm::from_context(&mut ev)
        .execute(&osr_switch(), vec![n, k])
        .expect("interp run");
    crate::emacs_core::jit::force_osr_for_test(true);
    let f = osr_switch();
    f.jit_runtime().set_hot_for_test();
    let before = osr_transfers();
    let got = TestVm::from_context(&mut ev)
        .execute(&f, vec![n, k])
        .expect("OSR run");
    let transfers = osr_transfers() - before;
    crate::emacs_core::jit::force_osr_for_test(false);
    assert!(eql_value(&got, &want), "OSR {got:?} != interp {want:?}");
    let rt = f.jit_runtime();
    assert_eq!(rt.numeric_feedback(OSR_MUL_PC), NumericFeedback::Float);
    assert_eq!(rt.reopt_count(), 1, "one invalidation");
    assert_eq!(transfers, 2, "the first transfer and the re-entry");
    assert!(
        crate::emacs_core::jit::cache::osr_entry_cached(&f, OSR_HEADER),
        "the re-entered leaf is cached"
    );
    force_reopt_for_test(None);
}

/// An OSR deopt that invalidates nothing (an overflow below the site limit)
/// still latches the frame onto the interpreter: one transfer only.
#[test]
fn osr_deopt_without_invalidation_latches() {
    force_deopt_for_test(false);
    force_reopt_for_test(Some(knobs_with_heat(5)));
    // (lambda (start n) (let ((i 0) (x start))
    //   (while (< i n) (setq i (1+ i)) (setq x (1+ x))) i))
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(1), // 2: header
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(13),
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::StackRef(0),
            Op::Add1, // 10: overflows mid-loop
            Op::StackSet(1),
            Op::Goto(2),
            Op::StackRef(1),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        2,
    );
    let mut ev = Context::new();
    crate::emacs_core::jit::force_osr_for_test(true);
    f.jit_runtime().set_hot_for_test();
    let before = osr_transfers();
    let start = Value::make_int(Value::MOST_POSITIVE_FIXNUM - 1500);
    let got = TestVm::from_context(&mut ev)
        .execute(&f, vec![start, Value::make_int(2000)])
        .expect("runs");
    crate::emacs_core::jit::force_osr_for_test(false);
    assert_eq!(got, Value::make_int(2000));
    assert_eq!(osr_transfers() - before, 1, "latched after the deopt");
    assert_eq!(f.jit_runtime().reopt_count(), 0);
    force_reopt_for_test(None);
}

/// An OSR entry guard that keeps refusing the live stack at the header
/// reopens recording at the first refusal and retires the OSR leaf at the
/// site limit, so the next transfer compiles against what the loop records.
#[test]
fn osr_entry_guard_failures_reprofile_after_limit() {
    force_deopt_for_test(false);
    force_reopt_for_test(Some(knobs_with_heat(5)));
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    // (lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i)): `i` is
    // proven fixnum at the header, so the entry guards it.
    let lp = function(
        vec![
            Op::Constant(0),
            Op::StackRef(0), // 1: header
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(9),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        1,
    );
    let rt = lp.jit_runtime();
    let snapshot = [Value::make_int(10), Value::make_float(1.5)];
    let transfer = || match crate::emacs_core::jit::cache::try_run_osr(ctx, &lp, 1, &snapshot, &[])
    {
        Some(crate::emacs_core::jit::compile::NativeRun::DeoptAt(resume)) => {
            assert_eq!(resume.pc, 1, "rejected at the header");
            assert_eq!(resume.stack, snapshot, "with the untouched snapshot");
        }
        other => panic!("expected the entry guard's deopt, got {other:?}"),
    };
    rt.note_numeric_feedback_consumed();
    transfer();
    assert!(
        rt.wants_numeric_feedback(),
        "the first refusal reopens recording"
    );
    assert_eq!(rt.reopt_count(), 0);
    for _ in 2..ReoptKnobs::SITE_LIMIT {
        transfer();
        assert!(crate::emacs_core::jit::cache::osr_entry_cached(&lp, 1));
    }
    transfer();
    assert_eq!(rt.reopt_count(), 1, "invalidated at the limit");
    assert!(
        !crate::emacs_core::jit::cache::osr_entry_cached(&lp, 1),
        "the OSR leaf is gone"
    );
    force_reopt_for_test(None);
}
