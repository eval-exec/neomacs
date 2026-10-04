//! Actual MIR/SSA parameter frontiers under the optional scalar preset.
//! Threading: contexts/sources/leaves belong to each test; only invocation-owned
//! compiler settings are overridden, with no process-environment mutation.

use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, function, mask_code_text,
};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    ByteCodeFunction, CompileRequest, Context, Inline2Mode, NativeRun, Op, RegallocPolicy,
    Tier2Knob, Value, compile_bytecode_function_requested, force_deopt_for_test,
    force_inline2_for_test, force_tier2_for_test, opt_mode_scope_for_test,
};
use crate::emacs_core::jit::stats::CompileOrigin;
use crate::emacs_core::jit::tier2::CompileTier;

/// This test owns these scalar settings for its compiler invocation; on drop it
/// returns the surrounding test thread to process configuration. No Lisp cache
/// or shared mutator assumption is introduced.
struct Settings;
impl Settings {
    fn enter() -> Self {
        force_inline2_for_test(Some(Inline2Mode::Off));
        force_tier2_for_test(Some(Tier2Knob::from_env(|_| None)));
        force_deopt_for_test(false);
        crate::emacs_core::jit::bg::force_mode_for_test(Some(
            crate::emacs_core::jit::bg::BgMode::Sync,
        ));
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_inline2_for_test(None);
        force_tier2_for_test(None);
        force_deopt_for_test(false);
        crate::emacs_core::jit::bg::force_mode_for_test(None);
    }
}

fn loop_body(list: bool) -> ByteCodeFunction {
    if list {
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
                Op::Return,
            ],
            vec![Value::make_int(0)],
            2,
        )
    } else {
        function(
            vec![
                Op::Constant(0),
                Op::StackRef(0),
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
        )
    }
}

fn compile(f: &ByteCodeFunction) -> super::super::CompiledLeaf {
    compile_bytecode_function_requested(
        f,
        None,
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: false,
            origin: CompileOrigin::Dispatch,
            tier: CompileTier::T1,
        },
    )
    .unwrap()
}

fn normalized(records: Vec<String>) -> Vec<String> {
    records
        .into_iter()
        .map(|text| mask_code_text(&text.replace('_', "")))
        .collect()
}

#[test]
fn opt_profile_lists20_keeps_numeric_mir_and_selects_hot_mutating_parameter_loop() {
    let _settings = Settings::enter();
    let _profile = scope_for_test(Profile::Lists20);
    let mut context = Context::new();
    let numeric = loop_body(false);
    let list = loop_body(true);
    numeric.jit_runtime().set_hot_for_test();
    list.jit_runtime().set_hot_for_test();
    assert_eq!(compile(&numeric).selected_tier(), SelectedTier::Mir);
    let selected = compile(&list);
    assert_eq!(selected.selected_tier(), SelectedTier::Opt);
    let pair = Value::cons(Value::make_int(-1), Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(pair);
    assert_eq!(
        selected.call(
            &mut context as *mut Context as *mut u8,
            &[pair, Value::make_int(4)]
        ),
        NativeRun::Ok(Value::make_int(4).bits())
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
}

#[test]
fn opt_profile_explicit_legacy_escape_keeps_numeric_and_list_clif() {
    let _settings = Settings::enter();
    let _context = Context::new();
    let _legacy = opt_mode_scope_for_test(OptMode::Legacy);
    for list in [false, true] {
        let source = loop_body(list);
        source.jit_runtime().set_hot_for_test();
        let selected = {
            let _preset = scope_for_test(Profile::Lists48);
            captured_clif(|| {
                assert_eq!(
                    compile(&source).selected_tier(),
                    if list {
                        SelectedTier::Baseline
                    } else {
                        SelectedTier::Mir
                    }
                );
            })
        };
        let original = {
            let _off = scope_for_test(Profile::Off);
            captured_clif(|| {
                compile(&source);
            })
        };
        assert!(!selected.is_empty());
        assert_eq!(normalized(selected), normalized(original));
    }
}
