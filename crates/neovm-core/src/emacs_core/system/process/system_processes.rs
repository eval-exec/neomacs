//! `list-system-processes` and `process-attributes` (GNU `process.c`, defined
//! outside `#ifdef subprocesses`).
//!
//! GNU dispatches both through `find-file-name-handler` on `default-directory`
//! before touching the OS, so a remote directory answers from its handler on
//! every host. Only the OS query is backend-specific: each process backend
//! supplies `list_system_processes_leaf` / `process_attributes_leaf`, and GNU's
//! `sysdep.c` fallback for hosts without the facility is `nil`.

use crate::emacs_core::error::{EvalResult, Flow, expect_args};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

pub(super) fn visible_default_directory_lisp(eval: &Context) -> Option<LispString> {
    let visible = eval.visible_variable_value_or_nil("default-directory");
    if let Some(string) = visible.as_lisp_string() {
        return Some(string.clone());
    }
    crate::emacs_core::fileio::default_directory_lisp_in_state(&eval.obarray, &[], &eval.buffers)
}

/// GNU's `Ffind_file_name_handler (BVAR (current_buffer, directory), OPERATION)`
/// prologue: run the handler with OPERATION and ARGS when one claims the
/// directory.
fn dispatch_default_directory_handler(
    eval: &mut Context,
    operation: &str,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    let Some(default_directory) = visible_default_directory_lisp(eval) else {
        return Ok(None);
    };
    let operation = Value::symbol(operation);
    let handler = crate::emacs_core::fileio::find_file_name_handler_lisp_for_eval(
        eval,
        &default_directory,
        operation,
    );
    if handler.is_nil() {
        return Ok(None);
    }
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(operation);
    call_args.extend_from_slice(args);
    eval.funcall_general(handler, call_args).map(Some)
}

/// (list-system-processes) -> process-id-list
pub(crate) fn builtin_list_system_processes(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("list-system-processes", &args, 0)?;
    if let Some(result) = dispatch_default_directory_handler(eval, "list-system-processes", &[])? {
        return Ok(result);
    }
    super::list_system_processes_leaf()
}

/// (process-attributes PID) -> alist-or-nil
pub(crate) fn builtin_process_attributes(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("process-attributes", &args, 1)?;
    if let Some(result) =
        dispatch_default_directory_handler(eval, "process-attributes", &args[..1])?
    {
        return Ok(result);
    }
    super::process_attributes_leaf(args[0])
}
