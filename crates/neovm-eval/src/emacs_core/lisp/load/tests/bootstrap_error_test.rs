//! F1: a bootstrap failure owns its diagnostic, so a caller may hold, log or
//! format it after the failed evaluator's heap is gone.
//!
//! Before this type the bootstrap paths returned an `EvalError::Signal` whose
//! payload named heap strings; the caller's cleanup dropped the evaluator
//! first, so reading the error afterwards walked a dead heap. The bootstrap
//! failure now renders while the heap is alive and owns that rendering.

use super::{BootstrapError, bootstrap_error_in_state};
use crate::emacs_core::eval::Context;
use neovm_value::error::EvalError;
use neovm_value::intern::intern;
use neovm_value::value::Value;

#[test]
fn bootstrap_error_outlives_the_failed_evaluator_heap() {
    let owned = {
        let eval = Context::new(crate::test_utils::unit_runtime());
        // A signal payload with a heap string: the exact shape that forced
        // callers to render before teardown.
        let error = EvalError::signal(
            intern("error"),
            vec![Value::string("loadup failed in an owned test")],
            None,
        );
        bootstrap_error_in_state(&eval, &error)
        // The evaluator (and the heap the payload names) is dropped here.
    };
    let rendered = owned.to_string();
    assert!(
        rendered.contains("loadup failed in an owned test"),
        "owned rendering must contain the failing message: {rendered}"
    );
    assert_eq!(owned.message(), rendered);
}

#[test]
fn bootstrap_error_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<BootstrapError>();
}
