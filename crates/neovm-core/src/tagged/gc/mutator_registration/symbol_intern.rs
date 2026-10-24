//! Test-only closed intern executor syntax; no legacy interner fallback.
use super::{OwnerRootSlot, RunAccess};

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
#[cfg(not(feature = "gc-memory-telemetry"))]
mod tests;
