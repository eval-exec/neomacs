//! GNU Baset is a direct primitive and can qualify a primitive-list Opt body.
//! Threading: source objects, scalar compiler overrides and existing mutator
//! caches belong to this test invocation; no process environment is changed.

use super::frontend_tests::Settings;
use super::lists_frontend_tests::list_loop;
use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::{
    ByteCodeFunction, Context, OptEarlyMode, OptProfitMode, Value, force_opt_early_for_test,
    force_opt_max_ops_for_test, force_opt_profit_for_test,
};
use crate::emacs_core::jit::{NumericFeedback, inline, stats};

fn retained_aset_loop() -> ByteCodeFunction {
    // Four source arguments: vector, retained value, cons, iteration limit.
    // The list-containing loop is profitable. GNU Baset always returns its
    // stored operand without resolving the symbol's live function cell.
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
fn opt_profit_primitive_lists_aset_original_source_outside_fragment_is_admitted() {
    let _settings = Settings::enter();
    let _context = Context::new(crate::test_utils::unit_runtime());
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
    assert!(primitive_osr_source_admitted(source.executable_ops()));
    assert!(body_admitted(
        OptProfitMode::PrimitiveLists,
        source.executable_ops(),
        CallDensity::from(heavy),
        KernelHeat::Hot
    ));
}

/// Compiler-thread fuser scalar override only; no Lisp/cache state is held.
/// Settings does not override this switch, so this invocation restores None.
#[derive(Debug)]
#[must_use = "dropping the guard restores this compiler thread's test state"]
struct FuserScope(std::marker::PhantomData<*const ()>);

static_assertions::assert_not_impl_any!(FuserScope: Send, Sync);
const _: () = assert!(std::mem::size_of::<FuserScope>() == 0);
impl FuserScope {
    #[doc(hidden)]
    pub fn enter() -> Self {
        inline::force_inline_for_test(Some(true));
        Self(std::marker::PhantomData)
    }
}
impl Drop for FuserScope {
    fn drop(&mut self) {
        inline::force_inline_for_test(None);
    }
}

#[test]
fn opt_profit_primitive_lists_aset_post_fusion_slice_is_admitted_but_original_call_is_not() {
    let _settings = Settings::enter();
    let _context = Context::new(crate::test_utils::unit_runtime());
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
    assert!(body_admitted(
        OptProfitMode::Lists,
        &fused.ops,
        CallDensity::from(heavy),
        KernelHeat::Hot
    ));
    assert!(osr_admitted(
        &fused.ops,
        &fused.constants,
        source.executable_ops().len()
    ));
    assert!(primitive_osr_source_admitted(&fused.ops));
    assert!(
        !primitive_osr_source_admitted(source.executable_ops()),
        "the original genuine call still excludes this source"
    );
}

#[test]
fn opt_profit_primitive_lists_aset_dead_source_keeps_selected_frontier_and_other_modes() {
    let _settings = Settings::enter();
    let _context = Context::new(crate::test_utils::unit_runtime());
    force_opt_max_ops_for_test(Some(48));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    let list = list_loop();
    let mut ops = list.executable_ops().to_vec();
    // No branch reaches this suffix. A direct primitive cannot add callback
    // evidence, whether the source's CFG eventually removes it or not.
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
        assert!(body_admitted(
            mode,
            source.executable_ops(),
            CallDensity::from(heavy),
            KernelHeat::Hot
        ));
        assert!(primitive_osr_source_admitted(source.executable_ops()));
    }
    force_opt_profit_for_test(Some(OptProfitMode::Off));
    assert!(body_admitted(
        OptProfitMode::Off,
        source.executable_ops(),
        CallDensity::Heavy,
        KernelHeat::Cold
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
            CallDensity::from(heavy),
            source.jit_runtime()
        ),
        FrontChoice::SelectedAfterMir,
        "dead GNU Baset must not exclude an otherwise selected primitive loop"
    );
    assert!(primitive_osr_source_admitted(source.executable_ops()));
}
