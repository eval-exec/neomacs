//! GNU `fileio.c` stream binary-mode compatibility.

use crate::emacs_core::error::{EvalResult, LispCondition, expect_args, signal};
use crate::emacs_core::value::Value;

/// (set-binary-mode STREAM MODE) -> t
///
/// GNU `fileio.c` `Fset_binary_mode`. On POSIX hosts the call only flushes and
/// always answers non-nil; it is defined on every host, so both process
/// backends re-export this one implementation.
pub(crate) fn builtin_set_binary_mode(args: Vec<Value>) -> EvalResult {
    expect_args("set-binary-mode", &args, 2)?;
    let stream = args[0].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;

    match stream {
        "stdin" | "stdout" | "stderr" => Ok(Value::T),
        _ => Err(signal(
            "error",
            vec![Value::string("unsupported stream"), args[0]],
        )),
    }
}
