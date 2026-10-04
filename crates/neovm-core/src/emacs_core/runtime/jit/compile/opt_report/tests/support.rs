//! Invocation-owned opt-report fixtures; no process environment changes.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

use super::{Ledger, Mutex, lock};

/// Threading: one test invocation owns the path and scalar-only ledger; Arc
/// keeps an active compiler guard valid across nested test reporting scopes.
/// This contains no Lisp values, source cache or executing-mutator state.
pub(super) struct Reporter {
    pub(super) ledger: Mutex<Ledger>,
    pub(super) path: Option<PathBuf>,
}

thread_local! {
    /// Test-owned reporting configuration/diagnostics only, never Lisp state.
    static REPORTER: RefCell<Option<Arc<Reporter>>> = const { RefCell::new(None) };
}

pub(super) fn current() -> Option<Arc<Reporter>> {
    REPORTER.with(|reporter| reporter.borrow().clone())
}

/// Threading: installs/restores reporting diagnostics on its owning test
/// thread; each nextest invocation has a private file and ledger. Guards retain
/// their own Arc so nested scopes cannot redirect pending completion counts.
pub(super) struct Scope {
    previous: Option<Arc<Reporter>>,
    reporter: Arc<Reporter>,
}

impl Scope {
    pub(super) fn enter(path: Option<PathBuf>) -> Self {
        let reporter = Arc::new(Reporter {
            ledger: Mutex::new(Ledger::new()),
            path,
        });
        let previous = REPORTER.with(|current| current.replace(Some(reporter.clone())));
        Self { previous, reporter }
    }

    pub(super) fn snapshot(&self) -> Ledger {
        lock(&self.reporter.ledger).clone()
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        REPORTER.with(|current| current.replace(self.previous.take()));
    }
}
