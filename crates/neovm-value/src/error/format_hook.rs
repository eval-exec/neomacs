//! Lower formatter hook for [`EvalError`].
//!
//! `EvalError` moves to neovm-value while the rendering algorithm (symbol
//! names, signal payloads and the Lisp printer) stays in neovm-core. The upper
//! crate installs its one formatting algorithm through this typed table beside
//! the other one-time hooks; until then `Display` is the signed placeholder
//! `<EvalError>`. No print logic crosses the boundary, only this function
//! pointer.

use super::EvalError;
use crate::value::hook_publication::{self, HookInstallError, HookInstallStatus};
use std::fmt::{self, Formatter};
use std::sync::OnceLock;

/// The evaluator's `EvalError` formatting algorithm.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct EvalErrorFormatter {
    fmt: fn(&EvalError, &mut Formatter<'_>) -> fmt::Result,
}

impl EvalErrorFormatter {
    /// Describe inert formatting metadata; only the unsafe installer publishes it.
    pub const fn new(fmt: fn(&EvalError, &mut Formatter<'_>) -> fmt::Result) -> Self {
        Self { fmt }
    }
}

static EVAL_ERROR_FORMATTER: OnceLock<EvalErrorFormatter> = OnceLock::new();
static EVAL_ERROR_FORMATTER_IDENTITY: OnceLock<&'static EvalErrorFormatter> = OnceLock::new();

/// Publish the canonical formatter, accepting repeated installation of that
/// exact descriptor and refusing a different descriptor without replacement.
///
/// # Safety
/// Only the core printer's `install_eval_error_format_hook` adapter may call
/// this function, with its process-lifetime canonical formatting descriptor.
/// Only `Context::new_inner` and `Context::from_dump` may call the adapter,
/// before exposing the constructed or restored evaluator. Its callback must
/// preserve the existing signal/throw formatting algorithm. Publication
/// refusal must be surfaced at that construction or image-restore seam.
#[must_use = "handle a conflicting EvalError formatter publication"]
#[cold]
#[inline(never)]
pub unsafe fn install_eval_error_formatter(
    formatter: &'static EvalErrorFormatter,
) -> Result<HookInstallStatus, HookInstallError> {
    hook_publication::publish(
        &EVAL_ERROR_FORMATTER,
        &EVAL_ERROR_FORMATTER_IDENTITY,
        formatter,
    )
}

/// Whether the formatter hook has been installed (test-only observation).
#[cfg(any(test, feature = "test-support"))]
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
