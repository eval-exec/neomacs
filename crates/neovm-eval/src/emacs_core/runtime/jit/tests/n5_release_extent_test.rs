//! Release-reading witnesses for the native extent cell.
//!
//! This module is compiled only with `release-native-depth`, so
//! `native_depth` reads `CURRENT_LEAF_BASES` and the test counter cannot
//! mask the hole. Each test asks the serial mint from Lisp that runs while
//! native code is still live, and requires the mint to be refused.
//!
//! (t1) is an unwind form in an outermost framed leaf's cold tail.
//! (t2) is a raw body entered through `run_armed_leaf`.

use std::cell::Cell;

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, ByteCodeFunctionExt, Op, StackDepth};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use crate::emacs_core::subr::SubrSpec;
use neovm_value::error::EvalResult;
use neovm_value::intern::intern;
use neovm_value::value::{LambdaParams, Value};

thread_local! {
    /// `Some(true)` when the probe's mint was refused.
    static MINT_REFUSED: Cell<Option<bool>> = const { Cell::new(None) };
}

fn probe(_ctx: &mut Context) -> EvalResult {
    let refused = super::cache::JitGracePeriod::mint_serial().is_err();
    MINT_REFUSED.with(|cell| cell.set(Some(refused)));
    Ok(Value::NIL)
}

fn bytecode(ops: Vec<Op>, constants: Vec<Value>) -> Value {
    let mut source = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    source.lexical = true;
    source.set_ops(ops);
    source.set_constants(constants.into());
    source.set_max_stack(StackDepth::for_test(8));
    source.seal_hand_assembled_ops();
    source
        .jit_runtime()
        .set_heat_for_test(super::hot_threshold().saturating_mul(64));
    source.jit_runtime().set_hot_for_test();
    Value::make_bytecode(source)
}

struct Guard<B> {
    profit: super::compile::ProfitGateTestPolicy,
    jit_off: bool,
    _backend: B,
    _bg: super::bg::TestModeScope,
}

impl Guard<()> {
    fn enter() -> Guard<impl Drop> {
        let profit = super::compile::replace_profit_gate_for_test(
            super::compile::ProfitGateTestPolicy::Bypass,
        );
        let jit_off = super::jit_forced_off_for_test();
        super::force_jit_off_for_test(false);
        super::compile::force_inline2_for_test(Some(super::compile::Inline2Mode::Off));
        let backend = super::compile::opt_mode_scope_for_test(super::compile::OptMode::Off);
        let bg = super::bg::TestModeScope::enter(super::bg::BgMode::Sync);
        Guard {
            profit,
            jit_off,
            _backend: backend,
            _bg: bg,
        }
    }
}

impl<B> Drop for Guard<B> {
    fn drop(&mut self) {
        super::compile::replace_profit_gate_for_test(self.profit);
        super::force_jit_off_for_test(self.jit_off);
        super::compile::force_inline2_for_test(None);
    }
}

fn prepare(ctx: &mut Context, value: Value) -> &'static ByteCodeFunction {
    let function = value
        .get_bytecode_data()
        .expect("bytecode value keeps its source");
    super::test_support::prepare_entry(ctx, function);
    let id = function.jit_runtime().compiled_id().expect("id assigned");
    assert!(
        super::cache::is_compiled_for_test(id),
        "prepare did not install a compiled leaf"
    );
    function
}

/// (t1) The cold tail of an outermost framed leaf runs the unwind form after
/// the native body has returned. The mint must still be refused.
#[test]
fn n5_release_cold_tail_refuses_mint() {
    let _guard = Guard::enter();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.set_lexical_binding(true);
    ctx.register_subr(SubrSpec::fixed0("n5-release-tail-probe", probe));
    let probe_sym = intern("n5-release-tail-probe");
    let cleanup = Value::list(vec![Value::list(vec![Value::from_sym_id(probe_sym)])]);
    ctx.push_specpdl_root(cleanup);

    super::cache::clear_with_serial_receipt().expect("serial jit grace");
    MINT_REFUSED.with(|cell| cell.set(None));
    let body = bytecode(
        vec![
            Op::Constant(0),
            Op::UnwindProtectPop,
            Op::Constant(1),
            Op::Call(0),
            Op::Return,
        ],
        vec![cleanup, Value::symbol("n5-release-no-such")],
    );
    ctx.obarray.set_symbol_function("n5-release-tail", body);
    let function = prepare(&mut ctx, body);
    // SAFETY: `ctx` is the live Context this test owns. The call stays on this thread.
    let _returned = unsafe { super::try_run_compiled(&mut ctx, function, Value::NIL, &[]) };
    assert_eq!(
        MINT_REFUSED.with(Cell::get),
        Some(true),
        "cold-tail mint must be refused while the outermost frame is live"
    );
}

/// (t2) A raw body entered by `run_armed_leaf` re-enters Lisp. The mint from
/// that re-entry must be refused. This path does not use `try_run_compiled`.
#[test]
fn n5_release_armed_raw_refuses_mint() {
    let _guard = Guard::enter();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.set_lexical_binding(true);
    ctx.register_subr(SubrSpec::fixed0("n5-release-raw-probe", probe));
    let probe_sym = intern("n5-release-raw-probe");

    super::cache::clear_with_serial_receipt().expect("serial jit grace");
    MINT_REFUSED.with(|cell| cell.set(None));
    let body = bytecode(
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![Value::from_sym_id(probe_sym)],
    );
    ctx.obarray.set_symbol_function("n5-release-raw", body);
    let function = prepare(&mut ctx, body);
    let id = function.jit_runtime().compiled_id().expect("id assigned");
    let ptr = super::cache::compiled_leaf_ptr_for_test(id).expect("raw leaf pointer");
    let shape = unsafe { (*ptr).entry_shape };
    assert!(
        matches!(
            shape,
            super::compile::EntryShape::RawMemory | super::compile::EntryShape::RawRegister
        ),
        "t2 must enter a raw body, shape={shape:?}"
    );
    let args: [i64; 0] = [];
    let returned = super::cache::run_armed_leaf(
        std::ptr::from_mut(&mut ctx),
        function,
        body,
        ptr,
        args.as_ptr(),
    );
    assert!(
        returned.is_ok(),
        "raw leaf should return after the probe, got {returned:?}"
    );
    assert_eq!(
        MINT_REFUSED.with(Cell::get),
        Some(true),
        "armed raw re-entry mint must be refused while native code is live"
    );
}
