//! Closed intern executor syntax; no legacy interner fallback.
//! Present for the crate's own tests and for `test-support`, so re-homed
//! core tests can name `InternOp`. Absent from production builds.
#![cfg(any(test, feature = "test-support"))]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InternOp {
    InitialIntern { name: u16, dst: u16 },
    InitialInternSoft { name: u16, dst: u16 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InternOpKind {
    Intern,
    Soft,
}
impl InternOpKind {
    pub(super) fn method(self) -> &'static str {
        match self {
            Self::Intern => "initial_intern_into",
            Self::Soft => "initial_intern_soft_into",
        }
    }
}
