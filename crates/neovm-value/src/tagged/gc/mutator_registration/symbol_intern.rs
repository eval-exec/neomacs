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
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn method(self) -> &'static str {
        match self {
            Self::Intern => "initial_intern_into",
            Self::Soft => "initial_intern_soft_into",
        }
    }
}
mod fixture;
#[path = "symbol_intern/tests/publication.rs"]
mod publication;
#[path = "symbol_intern/tests/table_failures.rs"]
mod table_failures;
#[cfg(not(any(test, feature = "test-support")))]
#[cfg(not(feature = "gc-memory-telemetry"))]
mod tests;
#[cfg(any(test, feature = "test-support"))]
#[cfg(not(feature = "gc-memory-telemetry"))]
pub mod tests;

pub(super) fn nmjit_n3_retired_publication_manifest() -> Result<(), String> {
    publication::manifest()
}
