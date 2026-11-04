//! dired.c bootstrap DEFVARs owned by the evaluator (stage-4 K2).

use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

/// dired.c:1206/1212 — DEFVAR_LISP completion-ignored-extensions, nil.
pub(crate) fn register_bootstrap_vars(obarray: &mut Obarray) {
    obarray.seed_value("completion-ignored-extensions", Value::NIL);
    obarray.make_special("completion-ignored-extensions");
}
