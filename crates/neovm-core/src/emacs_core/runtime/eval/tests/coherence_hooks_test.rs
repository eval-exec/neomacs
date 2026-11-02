//! One-time hook installation and byte-identical `EvalError` rendering.
//!
//! `Context::new_inner` and the pdump reconstruction install the value layer's
//! `EvalError` formatter; these tests pin both install paths and the output
//! contract of the installed algorithm.

use super::Context;
use crate::emacs_core::error::EvalError;
use crate::emacs_core::error::format_hook::eval_error_formatter_installed;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;
use crate::emacs_core::value::flow_vocab::ShutdownRequest;

#[test]
fn context_new_installs_the_eval_error_formatter() {
    let _context = Context::new();
    assert!(
        eval_error_formatter_installed(),
        "Context::new_inner must install the EvalError formatter"
    );
    #[cfg(feature = "sqlite")]
    assert!(
        crate::tagged::header::sqlite_finalizer::sqlite_obj_finalizer_installed(),
        "Context::new_inner must install the SqliteObj finalizer"
    );
}

#[test]
fn pdump_reconstruction_installs_the_eval_error_formatter() {
    let _context = crate::test_utils::runtime_startup_context();
    assert!(
        eval_error_formatter_installed(),
        "pdump reconstruction must install the EvalError formatter"
    );
    #[cfg(feature = "sqlite")]
    assert!(
        crate::tagged::header::sqlite_finalizer::sqlite_obj_finalizer_installed(),
        "pdump reconstruction must install the SqliteObj finalizer"
    );
}

#[test]
fn installed_formatter_renders_the_historical_strings() {
    let _context = Context::new();
    assert_eq!(
        EvalError::signal(intern("error"), Vec::new(), None).to_string(),
        "signal error nil"
    );
    assert_eq!(
        EvalError::signal(
            intern("wrong-type-argument"),
            vec![Value::symbol("plistp"), Value::NIL],
            None,
        )
        .to_string(),
        "signal wrong-type-argument (plistp nil)"
    );
    assert_eq!(
        EvalError::uncaught_throw(Value::symbol("tag"), Value::symbol("payload")).to_string(),
        "uncaught throw tag=tag value=payload"
    );
    assert_eq!(
        EvalError::Shutdown(ShutdownRequest {
            exit_code: 7,
            restart: false,
        })
        .to_string(),
        "kill-emacs 7"
    );
}
