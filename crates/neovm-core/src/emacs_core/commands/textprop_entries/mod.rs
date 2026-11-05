//! Text-property Lisp entries that stay in the core command layer.
//!
//! These entries resolve an optional WINDOW/OBJECT through the frame manager
//! before the edit-owned property scan, so under R0 they stay in core and
//! call the editor's in-state machinery downward.

use crate::emacs_core::error::EvalResult;
use crate::emacs_core::textprop::{
    builtin_get_char_property_and_overlay_with_window_lookup,
    builtin_get_char_property_with_window_lookup,
};
use crate::emacs_core::value::Value;

/// (get-char-property POS PROP &optional OBJECT)
/// For strings, same as get-text-property (no overlays).
pub(crate) fn builtin_get_char_property(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::error::expect_args_range("get-char-property", &args, 2, 3)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_get_char_property_3(eval, arg(0), arg(1), arg(2))
}

/// `get-char-property` as registered: fixed arity 3, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a3` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_get_char_property_3(
    eval: &mut crate::emacs_core::eval::Context,
    position: Value,
    prop: Value,
    object: Value,
) -> EvalResult {
    let args: [Value; 3] = [position, prop, object];
    builtin_get_char_property_with_window_lookup(
        eval.variables(),
        &eval.edit.buffers,
        crate::emacs_core::eval::data::window_state::char_property_window_lookup(Some(
            &eval.frame_manager(),
        )),
        &args,
    )
}

/// (get-char-property-and-overlay POS PROP &optional OBJECT)
pub(crate) fn builtin_get_char_property_and_overlay(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_char_property_and_overlay_with_window_lookup(
        eval.variables(),
        &eval.edit.buffers,
        crate::emacs_core::eval::data::window_state::char_property_window_lookup(Some(
            &eval.frame_manager(),
        )),
        args,
    )
}
