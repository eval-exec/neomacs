//! GNU xsettings.c's Lisp queries, delegated to the current display host.

use super::{EvalResult, Value, expect_args};
use crate::emacs_core::{display_host::SystemFontRole, eval::Context};

fn system_font(eval: &Context, role: SystemFontRole) -> Value {
    eval.display_host
        .as_ref()
        .and_then(|host| host.system_font(role))
        .map_or(Value::NIL, |font| Value::string(font.as_str()))
}

pub(crate) fn builtin_font_get_system_font(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("font-get-system-font", &args, 0)?;
    Ok(system_font(eval, SystemFontRole::Monospace))
}

pub(crate) fn builtin_font_get_system_normal_font(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("font-get-system-normal-font", &args, 0)?;
    Ok(system_font(eval, SystemFontRole::Application))
}
