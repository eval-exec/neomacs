//! Owner pin: a cache-owned leaf's last `Rc` must not drop while
//! `native_depth` is non-zero. The nested callee is the case the parent
//! drops: its only owner is the cache, and a clear runs inside the leaf.
//! The serial receipt refuses that clear. The observer records a drop if
//! one still happens. This test does not catch a panic.

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
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn probe(_ctx: &mut Context) -> EvalResult {
    DEPTH.with(|cell| cell.set(super::cache::native_depth()));
    let _ = super::cache::clear_with_serial_receipt();
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

fn body_calls_probe_then_seven(probe_sym: neovm_value::intern::SymId) -> Value {
    bytecode(
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::from_sym_id(probe_sym), Value::make_int(7)],
    )
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

#[test]
fn n5_no_last_owner_drop_under_live_frame() {
    let _guard = Guard::enter();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.set_lexical_binding(true);
    ctx.register_subr(SubrSpec::fixed0("n5-owner-pin-probe", probe));
    let probe_sym = intern("n5-owner-pin-probe");

    super::cache::clear_with_serial_receipt().expect("serial jit grace");
    let inner_sym = intern("n5-owner-pin-inner");
    let inner = body_calls_probe_then_seven(probe_sym);
    ctx.obarray.set_symbol_function_id(inner_sym, inner);
    let _inner_fn = prepare(&mut ctx, inner);
    let outer = bytecode(
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![Value::from_sym_id(inner_sym)],
    );
    ctx.obarray.set_symbol_function("n5-owner-pin-outer", outer);
    let outer_fn = prepare(&mut ctx, outer);

    super::cache::reset_live_frame_owner_drops_for_test();
    DEPTH.with(|cell| cell.set(0));
    // SAFETY: `ctx` is the live Context this test owns. The call stays on this thread.
    let returned = unsafe { super::try_run_compiled(&mut ctx, outer_fn, Value::NIL, &[]) };
    let depth = DEPTH.with(Cell::get);
    let drops = super::cache::live_frame_owner_drops_for_test();
    assert_eq!(
        returned.ok().flatten(),
        Some(Value::make_int(7).bits()),
        "nested leaf depth={depth} live_frame_owner_drops={drops}"
    );
    assert!(
        depth >= 2,
        "nested callee must run inside the outer native frame, depth={depth}"
    );
    assert_eq!(
        drops, 0,
        "a cache-owned leaf was dropped under a live frame at native depth {depth}"
    );
}
