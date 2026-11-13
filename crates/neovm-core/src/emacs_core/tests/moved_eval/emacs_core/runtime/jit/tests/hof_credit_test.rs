//! Callback credit: a `mapc`/`mapcar` caller with a selectable HOF site gains
//! heat for the callbacks it drives; a caller without one, a short mapping,
//! an already compiled caller and the off knob gain nothing.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;

fn byte_compiled(ctx: &mut Context, name: &str, body: &str) -> neovm_value::value::Value {
    ctx.eval_str(&format!("(defun {name} (l) {body})"))
        .expect("defun");
    ctx.eval_str(&format!("(byte-compile '{name})"))
        .expect("byte-compile");
    ctx.eval_str(&format!("(symbol-function '{name})"))
        .expect("compiled function")
}

/// The splice fuser on (it is off in the inline-off family run): credit only
/// pays when the callback loop can be inlined. Threading: this test thread's
/// scalar override.
struct InlineOn;

impl InlineOn {
    #[doc(hidden)]
    pub fn enter() -> Self {
        neovm_eval::test_support::authority::force_inline_for_test(Some(true));
        Self
    }
}

impl Drop for InlineOn {
    fn drop(&mut self) {
        neovm_eval::test_support::authority::force_inline_for_test(None);
    }
}

fn heat_after(ctx: &mut Context, name: &str, list_len: usize) -> u32 {
    ctx.eval_str(&format!("({name} (number-sequence 1 {list_len}))"))
        .expect("call");
    let function = ctx
        .eval_str(&format!("(symbol-function '{name})"))
        .expect("function");
    let source = function.get_bytecode_data().expect("bytecode");
    neovm_eval::test_support::jit_heat_for_test(source)
}

#[test]
fn a_selectable_caller_gains_heat_per_32_callbacks() {
    let _inline = InlineOn::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    byte_compiled(
        &mut ctx,
        "hof-credit-add1",
        "(mapcar (lambda (x) (1+ x)) l)",
    );
    // One entry plus 3200 / 32 callbacks of credit.
    assert_eq!(heat_after(&mut ctx, "hof-credit-add1", 3200), 101);
}

#[test]
fn the_credit_per_mapping_is_capped_below_the_threshold() {
    let _inline = InlineOn::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    byte_compiled(&mut ctx, "hof-credit-long", "(mapc (lambda (x) (1+ x)) l)");
    let cap = crate::emacs_core::jit::hot_threshold() / 2;
    assert_eq!(heat_after(&mut ctx, "hof-credit-long", 200_000), 1 + cap);
}

#[test]
fn a_caller_whose_callback_calls_gains_no_heat() {
    let _inline = InlineOn::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    byte_compiled(
        &mut ctx,
        "hof-credit-calling",
        "(mapcar (lambda (x) (format \"%s\" x)) l)",
    );
    assert_eq!(heat_after(&mut ctx, "hof-credit-calling", 3200), 1);
}

#[test]
fn a_short_mapping_gains_no_heat() {
    let _inline = InlineOn::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    byte_compiled(
        &mut ctx,
        "hof-credit-short",
        "(mapcar (lambda (x) (1+ x)) l)",
    );
    assert_eq!(heat_after(&mut ctx, "hof-credit-short", 31), 1);
}

#[test]
fn a_caller_through_a_compile_gains_no_heat() {
    let _inline = InlineOn::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    let function = byte_compiled(
        &mut ctx,
        "hof-credit-seen",
        "(mapcar (lambda (x) (1+ x)) l)",
    );
    let source = function.get_bytecode_data().expect("bytecode");
    neovm_eval::test_support::authority::jit_compiled_id_or_assign_for_test(source);
    assert_eq!(heat_after(&mut ctx, "hof-credit-seen", 3200), 1);
}

#[test]
fn hof_inline_off_gives_no_credit() {
    let _inline = InlineOn::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    byte_compiled(&mut ctx, "hof-credit-off", "(mapcar (lambda (x) (1+ x)) l)");
    neovm_eval::test_support::authority::force_hof_inline_for_test(Some(false));
    let heat = heat_after(&mut ctx, "hof-credit-off", 3200);
    neovm_eval::test_support::authority::force_hof_inline_for_test(None);
    assert_eq!(heat, 1);
}

#[test]
fn a_mapping_in_an_interpreted_calls_argument_credits_no_caller() {
    let _inline = InlineOn::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    byte_compiled(
        &mut ctx,
        "hof-credit-arg",
        "(mapcar (lambda (x) (car x)) l)",
    );
    // The argument's mapping runs while the call's frame still evaluates
    // its arguments, before the callback below ever ran: no credit, and no
    // verdict cached for the caller.
    ctx.eval_str("(hof-credit-arg (mapcar #'list (number-sequence 1 3200)))")
        .expect("call");
    let heat_value = ctx
        .eval_str("(symbol-function 'hof-credit-arg)")
        .expect("function");
    let heat_source = heat_value.get_bytecode_data().expect("bytecode");
    let heat = neovm_eval::test_support::jit_heat_for_test(heat_source);
    assert_eq!(heat, 1 + 100, "only the body's own mapping credits");
    ctx.eval_str("(hof-credit-arg (mapcar #'list (number-sequence 1 3200)))")
        .expect("call");
    let heat_value = ctx
        .eval_str("(symbol-function 'hof-credit-arg)")
        .expect("function");
    let heat_source = heat_value.get_bytecode_data().expect("bytecode");
    let heat = neovm_eval::test_support::jit_heat_for_test(heat_source);
    assert_eq!(heat, 2 + 200);
}
