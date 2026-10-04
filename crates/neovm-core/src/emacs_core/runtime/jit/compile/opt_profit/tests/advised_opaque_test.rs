//! Override-aware Aset cannot qualify a primitive-list Opt body.
//! Threading: immutable source objects and scalar compiler overrides belong to
//! this test invocation; no process environment or mutator cache is changed.

use super::frontend_tests::Settings;
use super::lists_frontend_tests::list_loop;
use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::{
    ByteCodeFunction, Context, OptEarlyMode, OptProfitMode, Value, force_opt_early_for_test,
    force_opt_max_ops_for_test, force_opt_profit_for_test,
};
use crate::emacs_core::jit::{NumericFeedback, inline, stats};

fn retained_aset_loop() -> ByteCodeFunction {
    // Four source arguments: vector, retained value, cons, iteration limit.
    // The list-containing loop is profitable; Aset returns an arbitrary value
    // when its live function cell is overridden, so it is not a safe primitive.
    function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(13),
            Op::StackRef(2),
            Op::StackRef(1),
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Pop,
            Op::StackRef(3),
            Op::Constant(0),
            Op::StackRef(4),
            Op::Aset,
            Op::Pop,
            Op::StackRef(2),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        4,
    )
}

#[test]
fn opt_profit_primitive_lists_aset_original_source_outside_fragment_stays_rejected() {
    let _settings = Settings::enter();
    let _context = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    force_opt_max_ops_for_test(Some(48));
    let source = retained_aset_loop();
    assert_eq!(source.executable_ops().len(), 21);
    let heavy = super::super::body_is_call_heavy(source.executable_ops(), &source.constants);
    assert!(!heavy, "the j17 gate alone must not exclude this witness");
    // An emission slice can retain the list backedge without the source's
    // Aset suffix. This policy input is not an assertion about OSR slicing.
    let mut fragment = source.executable_ops()[..13].to_vec();
    fragment.push(Op::Return);
    assert!(osr_admitted(
        &fragment,
        &source.constants,
        source.executable_ops().len()
    ));
    assert!(!primitive_osr_source_admitted(source.executable_ops()));
    assert!(!body_admitted(
        OptProfitMode::PrimitiveLists,
        source.executable_ops(),
        heavy,
        true
    ));
}

/// Compiler-thread fuser scalar override only; no Lisp/cache state is held.
/// Settings does not override this switch, so this invocation restores None.
struct FuserScope;
impl FuserScope {
    fn enter() -> Self {
        inline::force_inline_for_test(Some(true));
        Self
    }
}
impl Drop for FuserScope {
    fn drop(&mut self) {
        inline::force_inline_for_test(None);
    }
}

#[test]
fn opt_profit_primitive_lists_aset_post_fusion_slice_stays_rejected() {
    let _settings = Settings::enter();
    let _context = Context::new();
    let _fuser = FuserScope::enter();
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    force_opt_max_ops_for_test(Some(48));
    let callee = function(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![], 1);
    callee.jit_runtime().set_hot_for_test();
    let callee = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee);
    let source = function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(14),
            Op::StackRef(2),
            Op::StackRef(1),
            Op::Setcar,
            Op::Pop,
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::StackSet(1),
            Op::Goto(1),
            Op::Pop,
            Op::StackRef(3),
            Op::Constant(0),
            Op::StackRef(4),
            Op::Aset,
            Op::Pop,
            Op::StackRef(2),
            Op::Return,
        ],
        vec![Value::make_int(0), callee],
        4,
    );
    let feedback = vec![NumericFeedback::FixnumOnly; source.executable_ops().len()];
    let fused = inline::fuse_calls(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        4,
        &feedback,
    )
    .expect("forced legacy fuser must splice the pure counter increment");
    assert_eq!(fused.regions.len(), 1);
    assert!(!fused.is_v2());
    assert!(fused.ops.len() <= 48);
    assert!(fused.ops.iter().any(|op| matches!(op, Op::Aset)));
    assert!(!fused.ops.iter().any(|op| matches!(
        op,
        Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) | Op::CallBuiltinSym(..)
    )));
    let heavy = super::super::body_is_call_heavy(&fused.ops, &fused.constants);
    assert!(!heavy);
    assert!(body_admitted(OptProfitMode::Lists, &fused.ops, heavy, true));
    assert!(!osr_admitted(
        &fused.ops,
        &fused.constants,
        source.executable_ops().len()
    ));
}

#[test]
fn opt_profit_primitive_lists_aset_dead_source_rejects_before_frontier_and_keeps_other_modes() {
    let _settings = Settings::enter();
    let _context = Context::new();
    force_opt_max_ops_for_test(Some(48));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    let list = list_loop();
    let mut ops = list.executable_ops().to_vec();
    // No branch reaches this suffix; conservative source policy intentionally
    // rejects its callback evidence before any CFG/reachability construction.
    ops.extend([Op::Nil, Op::Constant(0), Op::Nil, Op::Aset, Op::Pop]);
    let source = function(ops, vec![Value::make_int(0)], 2);
    source.jit_runtime().set_hot_for_test();
    let heavy = super::super::body_is_call_heavy(source.executable_ops(), &source.constants);
    assert!(!heavy);
    for mode in [
        OptProfitMode::Loops,
        OptProfitMode::Lists,
        OptProfitMode::Kernels,
    ] {
        force_opt_profit_for_test(Some(mode));
        assert!(body_admitted(mode, source.executable_ops(), heavy, true));
        assert!(primitive_osr_source_admitted(source.executable_ops()));
    }
    force_opt_profit_for_test(Some(OptProfitMode::Off));
    assert!(body_admitted(
        OptProfitMode::Off,
        source.executable_ops(),
        true,
        false
    ));
    assert!(primitive_osr_source_admitted(source.executable_ops()));
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    let request = CompileRequest {
        regalloc: super::super::RegallocPolicy::Full,
        bypass_profit_gate: false,
        origin: stats::CompileOrigin::Dispatch,
        tier: CompileTier::T1,
    };
    assert_eq!(
        front(
            request,
            source.executable_ops(),
            heavy,
            source.jit_runtime()
        ),
        FrontChoice::Legacy,
        "dead Aset must not trigger selected construction followed by a refusal"
    );
    assert!(!primitive_osr_source_admitted(source.executable_ops()));
}
