//! What the harness knows about startup, independently of transport order.
//!
//! Neither legacy terminal predicates nor PTY fences certify completion of
//! the editor's native startup housekeeping. No startup protocol is installed.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StartupObservation {
    /// A generic launch has no editor startup observation contract.
    #[default]
    Unobserved,
    /// A known editor uses its existing readiness policy without certification.
    NotCertified,
}

/// A stable editor identity with startup context in its diagnostic rendering.
#[derive(Clone, Debug)]
pub struct SessionName {
    canonical: String,
    startup: StartupObservation,
}

impl SessionName {
    pub(crate) fn new(canonical: &str, startup: StartupObservation) -> Self {
        Self {
            canonical: canonical.to_owned(),
            startup,
        }
    }
}

impl AsRef<str> for SessionName {
    fn as_ref(&self) -> &str {
        &self.canonical
    }
}

impl<T: AsRef<str> + ?Sized> PartialEq<T> for SessionName {
    fn eq(&self, other: &T) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl std::fmt::Display for SessionName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [startup={:?}]", self.canonical, self.startup)
    }
}

#[cfg(test)]
#[path = "startup/tests.rs"]
mod tests;
