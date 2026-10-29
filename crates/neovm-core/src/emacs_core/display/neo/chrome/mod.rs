//! Lisp surface for renderer chrome controls that are not part of
//! `VisualConfig`.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

#[cfg(test)]
#[path = "tests/chrome_test.rs"]
mod tests;

use crate::emacs_core::error::{EvalResult, Flow, expect_args, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

fn chrome_error(function: &str, message: impl std::fmt::Display) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("{function}: {message}"))],
    )
}

fn number_argument(value: Value) -> Result<f32, Flow> {
    value
        .as_number_f64()
        .filter(|number| number.is_finite())
        .map(|number| number as f32)
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("numberp"), value]))
}

/// `(neomacs-set-scroll-indicators ENABLED)` -- toggle the scroll-position
/// indicators and active-window focus ring in the render thread.
fn set_scroll_indicators(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-set-scroll-indicators", &args, 1)?;
    let enabled = !args[0].is_nil();
    if let Some(host) = eval.display_host.as_mut() {
        host.set_scroll_indicators(enabled)
            .map_err(|message| chrome_error("neomacs-set-scroll-indicators", message))?;
    }
    Ok(Value::NIL)
}

/// `(neomacs-set-extra-spacing LINE-SPACING LETTER-SPACING)` -- set the
/// renderer's additional spacing in logical pixels.
fn set_extra_spacing(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-set-extra-spacing", &args, 2)?;
    let line_spacing = number_argument(args[0])?;
    let letter_spacing = number_argument(args[1])?;
    if let Some(host) = eval.display_host.as_mut() {
        host.set_extra_spacing(line_spacing, letter_spacing)
            .map_err(|message| chrome_error("neomacs-set-extra-spacing", message))?;
    }
    Ok(Value::NIL)
}
