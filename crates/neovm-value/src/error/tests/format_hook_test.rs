//! Deterministic tests for the two `EvalError` formatter branches.
//!
//! The process-global OnceLock cannot be reset, so the fallback and delegation
//! branches are exercised through an explicit table probe with a real
//! `Formatter`; the installed path is covered by
//! `runtime/eval/tests/coherence_hooks_test.rs`.

use super::EvalError;
use super::format_hook::{EvalErrorFormatter, format_with};
use crate::intern::intern;
use std::fmt::{self, Formatter};

struct Probe<'a> {
    error: &'a EvalError,
    table: Option<&'a EvalErrorFormatter>,
}

impl fmt::Display for Probe<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        format_with(self.error, formatter, self.table)
    }
}

fn hooked(_error: &EvalError, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter.write_str("HOOKED")
}

#[test]
fn uninstalled_fallback_is_the_signed_placeholder() {
    let error = EvalError::signal(intern("error"), Vec::new(), None);
    assert_eq!(
        format!(
            "{}",
            Probe {
                error: &error,
                table: None
            }
        ),
        "<EvalError>"
    );
}

#[test]
fn installed_table_receives_the_error_and_formatter() {
    let error = EvalError::signal(intern("error"), Vec::new(), None);
    let table = EvalErrorFormatter::new(hooked);
    assert_eq!(
        format!(
            "{}",
            Probe {
                error: &error,
                table: Some(&table)
            }
        ),
        "HOOKED"
    );
}
