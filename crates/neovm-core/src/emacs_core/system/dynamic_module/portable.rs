//! Dynamic-module boundary for hosts without native shared libraries.

use crate::emacs_core::error::{EvalResult, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

pub fn load_module(_ctx: &mut Context, path: std::path::PathBuf) -> EvalResult {
    Err(signal(
        "module-open-failed",
        vec![
            Value::string(path.display().to_string()),
            Value::string("native dynamic modules are unavailable on this host"),
        ],
    ))
}

pub fn apply_module_function(_ctx: &mut Context, func: Value, _args: Vec<Value>) -> EvalResult {
    Err(signal(
        "invalid-function",
        vec![
            func,
            Value::string("native dynamic modules are unavailable on this host"),
        ],
    ))
}

pub(crate) fn collect_dynamic_module_gc_roots(_roots: &mut Vec<Value>) {}

pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<opaque panic payload>".to_owned()
    }
}
