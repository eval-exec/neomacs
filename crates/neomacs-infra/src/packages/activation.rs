//! The activation vocabulary: how a session loads a prepared package.
//!
//! Moved here with the acquisition core so the cache layout and the forms
//! that mount it cannot drift apart; `neomacs-melpa-test-support` re-exports
//! these names for its existing callers.

/// Which of the package's entry points a session loads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageActivation {
    /// `(load (getenv "NEOMACS_PACKAGE_SOURCE") nil t t)` — the parity
    /// default: both engines read the same source file, so neither engine's
    /// byte-compiler is in the comparison.
    SourceFile,
    /// No activation form: the session uses the package's installed
    /// autoloads (performance measurement wants this — it is what a user's
    /// session does).
    InstalledAutoloads,
}

pub fn package_activation_elisp(activation: PackageActivation) -> &'static str {
    match activation {
        PackageActivation::SourceFile => r#"(load (getenv "NEOMACS_PACKAGE_SOURCE") nil t t)"#,
        PackageActivation::InstalledAutoloads => "nil",
    }
}

/// Which file-name suffixes `load` prefers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadSuffixes {
    /// `load-suffixes '(".el")` — read the source, skipping any `.elc`.
    Source,
    /// The editor's default suffix order (`.elc` before `.el`).
    EmacsDefault,
}
