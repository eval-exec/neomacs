//! One-time hook installation and byte-identical `EvalError`/`Value` rendering.
//!
//! `Context::new_inner` and the pdump reconstruction install the value layer's
//! `EvalError` formatter and its `Display for TaggedValue` formatter; these
//! tests pin both install paths and the output contract of the installed
//! algorithms.

use crate::emacs_core::Context;
use crate::emacs_core::print::print_value;
use neovm_value::error::EvalError;
use neovm_value::error::format_hook::eval_error_formatter_installed;
use neovm_value::intern::intern;
use neovm_value::value::Value;
use neovm_value::value::display_hook::value_display_formatter_installed;
use neovm_value::value::flow_vocab::ShutdownRequest;

#[test]
fn context_new_installs_the_eval_error_formatter() {
    let _context = crate::test_utils::new_context();
    assert!(
        eval_error_formatter_installed(),
        "Context::new_inner must install the EvalError formatter"
    );
    #[cfg(feature = "sqlite")]
    assert!(
        neovm_value::tagged::header::sqlite_obj_finalizer_installed(),
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
        neovm_value::tagged::header::sqlite_obj_finalizer_installed(),
        "pdump reconstruction must install the SqliteObj finalizer"
    );
}

#[test]
fn installed_formatter_renders_the_historical_strings() {
    let _context = crate::test_utils::new_context();
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

#[test]
fn context_new_installs_the_value_display_formatter() {
    let _context = crate::test_utils::new_context();
    assert!(
        value_display_formatter_installed(),
        "Context::new_inner must install the TaggedValue Display formatter"
    );
}

#[test]
fn pdump_reconstruction_installs_the_value_display_formatter() {
    let _context = crate::test_utils::runtime_startup_context();
    assert!(
        value_display_formatter_installed(),
        "pdump reconstruction must install the TaggedValue Display formatter"
    );
}

/// After installation, `Display` must be byte-identical to `print_value` for
/// every value, including the printer's string escaping. The corpus covers
/// each representation the printer branches on by kind.
#[test]
fn installed_display_is_byte_identical_to_print_value() {
    let _context = crate::test_utils::new_context();
    let corpus = [
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::fixnum(42),
        Value::fixnum(-7),
        Value::symbol("foo"),
        Value::symbol("bar"),
        Value::string("plain"),
        Value::string("line\nbreak"),
        Value::string("quote\"and\\slash"),
        Value::string("non-ascii \u{00e9}"),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        Value::cons(
            Value::symbol("a"),
            Value::cons(Value::symbol("b"), Value::NIL),
        ),
        Value::vector(vec![Value::fixnum(1), Value::symbol("x")]),
    ];
    for value in corpus {
        assert_eq!(
            format!("{value}"),
            print_value(&value),
            "Display for {value:?} must match print_value"
        );
    }
}
