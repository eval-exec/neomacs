use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use crate::emacs_core::jit::cache;
use neovm_value::value::{LambdaParams, Value};

// Keep the original helper paths while using the existing signed fences.
mod compile {
    pub(super) use neovm_eval::test_support::authority::{
        TestOptMode as OptMode, TestProfitPolicy as ProfitGateTestPolicy,
        force_inline_arith_for_test, jit_inline_arith_on_for_test as jit_inline_arith_on,
        opt_mode_token_scope_for_test as opt_mode_scope_for_test,
        replace_profit_gate_token_for_test as replace_profit_gate_for_test,
    };
}
mod bg {
    pub(super) use crate::emacs_core::jit::BgMode;
    pub(super) struct TestModeScope;
    impl TestModeScope {
        pub(super) fn enter(mode: BgMode) -> impl Drop {
            neovm_eval::test_support::authority::enter_test_mode_for_test(mode)
        }
    }
}

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut source = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    source.lexical = true;
    source.ops = ops;
    source.constants = constants.into();
    source.max_stack = StackDepth::for_test(16);
    source.seal_hand_assembled_ops();
    source
        .jit_runtime()
        .set_heat_for_test(crate::emacs_core::jit::hot_threshold().saturating_mul(64));
    source
}

fn witness_caller() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0), // 'green-release-helper
            Op::Call(0),     // <- the release point
            Op::Pop,
            Op::Constant(1), // 'logand
            Op::Constant(2), // 7
            Op::Constant(3), // 3
            Op::Call(2),     // <- must deliver the NEW definition post-deopt
            Op::Return,
        ],
        vec![
            Value::symbol("green-release-helper"),
            Value::symbol("logand"),
            Value::fixnum(7),
            Value::fixnum(3),
        ],
    )
}

struct ArithScope {
    profit: compile::ProfitGateTestPolicy,
    jit_off: bool,
    arithmetic: bool,
}

impl Drop for ArithScope {
    fn drop(&mut self) {
        compile::replace_profit_gate_for_test(self.profit);
        crate::emacs_core::jit::force_jit_off_for_test(self.jit_off);
        compile::force_inline_arith_for_test(self.arithmetic);
    }
}

fn arith_scope() -> ArithScope {
    let scope = ArithScope {
        // The witness bodies are one real call plus an intrinsified `logand` —
        // `calls > arith` under the default gate, so they would never tier.
        // green_arith's `exercise` bypasses the gate for the same body shape.
        profit: compile::replace_profit_gate_for_test(compile::ProfitGateTestPolicy::Bypass),
        jit_off: crate::emacs_core::jit::jit_forced_off_for_test(),
        arithmetic: compile::jit_inline_arith_on(),
    };
    crate::emacs_core::jit::force_jit_off_for_test(false);
    compile::force_inline_arith_for_test(true);
    scope
}

fn run_witness(context: &mut Context) -> (Value, u64) {
    let source = Value::make_bytecode(witness_caller());
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("green-release-caller", source)
    });
    let _root = context.retain_gc_roots([source]);
    let source_data = source.get_bytecode_data().unwrap();
    neovm_eval::test_support::authority::prepare_entry_for_test(context, source_data);
    let before = cache::native_run_counters().2;
    let result = context
        .apply(Value::symbol("green-release-caller"), Vec::new())
        .expect("the witness caller completes");
    let deopts = cache::native_run_counters().2 - before;
    (result, deopts)
}

#[test]
fn release_check_passes_when_epoch_is_unchanged() {
    let _scope = arith_scope();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let mut context = crate::test_utils::new_green_context();
    context.set_lexical_binding(true);
    context
        .eval_str(
            "(fset 'green-release-helper (lambda ()
              (sleep-for 0 5)))",
        )
        .expect("setup");
    let (result, deopts) = run_witness(&mut context);
    assert_eq!(
        result,
        Value::fixnum(3),
        "an unchanged epoch keeps the baked intrinsic result"
    );
    assert_eq!(deopts, 0, "no revalidation may fire without an epoch move");
    neovm_eval::test_support::authority::green_teardown_for_test(&mut context);
    assert_eq!(
        neovm_eval::test_support::authority::jit_root_stack_top_for_test(&context),
        0
    );
}
