//! Bare facade seam for the evaluator's construction-time PC2 installs
//! (stage-4 K2 batch K9).
//!
//! The evaluator's `Context::new_inner`/`Context::from_dump` must publish the
//! core command-layer cold tables before any form can reach them, but the
//! evaluator manifest must not name the command-layer modules themselves. This
//! module is the one bare `emacs_core::` root the eval construction files
//! reference; each entry delegates to the command module that owns the
//! canonical descriptor static (`commands::interactive`,
//! `commands::casetab_entries`, `commands::kmacro`).
//!
//! Frequency: construction / dump restore, once. Not a hot path.

/// Publish the `call-interactively` acquisition descriptor.
pub(crate) fn install_call_interactively_hook() {
    crate::emacs_core::interactive::install_call_interactively_hook()
}

/// Publish the case-table read/sync descriptor.
pub(crate) fn install_case_table_hooks() {
    crate::emacs_core::casetab_entries::install_case_table_hooks()
}

/// Publish the keyboard-macro entry descriptor.
pub(crate) fn install_kmacro_hooks() {
    crate::emacs_core::kmacro::install_kmacro_hooks()
}
