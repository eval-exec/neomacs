//! mode_line_dynamic_symbols_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use crate::emacs_core::xdisp::{ModeLinePercentContext, ModeLineRendered};
use neovm_display::test_support::authority::{
    mode_line_dynamic_symbols_context_for_test as context,
    mode_line_dynamic_symbols_render_for_test as render,
    mode_line_dynamic_symbols_render_into_for_test as render_into,
};
use neovm_display::test_utils::mode_line_dynamic_symbols_test_upper_layer_helpers::Walker;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::heap_types::{GnuDeclaredCharacterCount, LispString};
use neovm_value::intern::intern;
use neovm_value::intern::{
    intern_call_names, intern_uninterned, reset_intern_calls, restore_runtime_interner,
    unintern_canonical_id,
};
use neovm_value::value::Value;

#[test]
fn mode_line_symbol_object_risk_property_matches_gnu() {
    let mut ctx = context(crate::test_utils::new_context);
    let name = "mode-line-symbol-object-risk";
    let canonical = intern(name);
    let actual = intern_uninterned(name);
    let value = ctx
        .eval_str("'(:eval (propertize \"RISK-EVAL\" 'ep33-symbol-probe-property 'present))")
        .expect("GNU probe value");
    ctx.obarray_mut().set_symbol_value_id(canonical, value);
    ctx.obarray_mut().set_symbol_value_id(actual, value);
    let risk = crate::emacs_core::xdisp::risky_local_variable_symbol_id();
    ctx.obarray_mut()
        .put_property_id(actual, risk, Value::T)
        .expect("valid risk plist");
    ctx.obarray_mut()
        .put_property_id(canonical, risk, Value::NIL)
        .expect("valid risk plist");
    let format = Value::from_sym_id(actual);
    let mut rendered = ModeLineRendered::default();
    render_into(
        &mut ctx,
        &ModeLinePercentContext::default(),
        format,
        Walker::Context,
        &mut rendered,
    );
    assert_eq!(
        rendered.text().to_vec(),
        "RISK-EVAL".chars().map(u32::from).collect::<Vec<_>>()
    );
    assert_eq!(
        rendered.text_props().get_property_at_char_pos(
            neovm_edit::buffer::CharPos0::new(0),
            Value::symbol("ep33-symbol-probe-property"),
        ),
        Some(Value::symbol("present"))
    );
    ctx.obarray_mut()
        .put_property_id(actual, risk, Value::NIL)
        .expect("valid risk plist");
    ctx.obarray_mut()
        .put_property_id(canonical, risk, Value::T)
        .expect("valid risk plist");
    assert_eq!(
        render(
            &mut ctx,
            &ModeLinePercentContext::default(),
            format,
            Walker::Context
        ),
        ""
    );
}
