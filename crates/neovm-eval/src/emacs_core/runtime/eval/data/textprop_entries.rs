//! `get-char-property` entries whose WINDOW/OBJECT resolution runs in the
//! evaluator's frame/window state before the edit-owned property scan.
//!
//! Moved down from the core command layer: every input here is evaluator-
//! owned (`Context::frame_manager`, `data::window_state`'s window lookup, the
//! evaluator's variables, and the edit buffer manager), and the property scan
//! itself already lives in `neovm-edit`. The core crate re-exports these
//! entries for the command-seam installer that names them.

use neovm_value::error::EvalResult;
use neovm_value::value::Value;

/// (get-char-property POS PROP &optional OBJECT)
/// For strings, same as get-text-property (no overlays).
#[doc(hidden)]
pub fn builtin_get_char_property(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    neovm_value::error::expect_args_range("get-char-property", &args, 2, 3)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_get_char_property_3(eval, arg(0), arg(1), arg(2))
}

/// `get-char-property` as registered: fixed arity 3, called straight off the
/// bytecode stack like GNU `funcall_subr`'s `a3` case (absent optionals
/// arrive as nil). The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_get_char_property_3(
    eval: &mut crate::emacs_core::eval::Context,
    position: Value,
    prop: Value,
    object: Value,
) -> EvalResult {
    let args: [Value; 3] = [position, prop, object];
    neovm_edit::emacs_core::textprop::builtin_get_char_property_with_window_lookup(
        eval.variables(),
        &eval.edit.buffers,
        crate::emacs_core::eval::data::window_state::char_property_window_lookup(Some(
            &eval.frame_manager(),
        )),
        &args,
    )
}

/// (get-char-property-and-overlay POS PROP &optional OBJECT)
#[doc(hidden)]
pub fn builtin_get_char_property_and_overlay(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    neovm_edit::emacs_core::textprop::builtin_get_char_property_and_overlay_with_window_lookup(
        eval.variables(),
        &eval.edit.buffers,
        crate::emacs_core::eval::data::window_state::char_property_window_lookup(Some(
            &eval.frame_manager(),
        )),
        args,
    )
}
