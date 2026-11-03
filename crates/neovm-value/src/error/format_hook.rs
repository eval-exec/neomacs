//! Lower formatter hook for [`EvalError`].
//!
//! `EvalError` moves to neovm-value while the rendering algorithm (symbol
//! names, signal payloads and the Lisp printer) stays in neovm-core. The upper
//! crate installs its one formatting algorithm through this typed table beside
//! the other one-time hooks; until then `Display` is the signed placeholder
//! `<EvalError>`. No print logic crosses the boundary, only this function
//! pointer.

use super::EvalError;
use std::fmt::{self, Formatter};
use std::sync::OnceLock;

/// The evaluator's `EvalError` formatting algorithm.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct EvalErrorFormatter {
    pub fmt: fn(&EvalError, &mut Formatter<'_>) -> fmt::Result,
}

static EVAL_ERROR_FORMATTER: OnceLock<EvalErrorFormatter> = OnceLock::new();

/// Install the evaluator's formatter. Idempotent like the other one-time hooks:
/// every `Context` construction and the pdump reconstruction call it.
pub(crate) fn install_eval_error_formatter(formatter: EvalErrorFormatter) {
    let _ = EVAL_ERROR_FORMATTER.set(formatter);
}

/// Whether the formatter hook has been installed (test-only observation).
#[cfg(test)]
#[doc(hidden)]
pub fn eval_error_formatter_installed() -> bool {
    EVAL_ERROR_FORMATTER.get().is_some()
}

/// Format with an explicitly supplied table so the signed fallback and the
/// delegation branch are both testable without process-global state.
pub(crate) fn format_with(
    error: &EvalError,
    formatter: &mut Formatter<'_>,
    table: Option<&EvalErrorFormatter>,
) -> fmt::Result {
    match table {
        Some(table) => (table.fmt)(error, formatter),
        None => formatter.write_str("<EvalError>"),
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        format_with(self, formatter, EVAL_ERROR_FORMATTER.get())
    }
}
