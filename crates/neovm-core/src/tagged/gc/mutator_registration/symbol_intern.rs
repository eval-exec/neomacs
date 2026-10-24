//! C5 preparation-only rooted executor syntax; no legacy interner fallback.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InternError {
    MissingOperation(InternOpKind),
}

/// The production seam will be inherent sealed RunAccess operations. Keeping
/// this preparation trait test-only cannot accidentally enable serial intern.
pub(super) trait InitialInternApi<'era> {
    fn initial_intern_into(
        &mut self,
        name: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), InternError>;
    fn initial_intern_soft_into(
        &mut self,
        name: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), InternError>;
}
impl<'era> InitialInternApi<'era> for RunAccess<'_, 'era> {
    fn initial_intern_into(
        &mut self,
        _name: &OwnerRootSlot<'era>,
        _dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), InternError> {
        Err(InternError::MissingOperation(InternOpKind::Intern))
    }
    fn initial_intern_soft_into(
        &mut self,
        _name: &OwnerRootSlot<'era>,
        _dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), InternError> {
        Err(InternError::MissingOperation(InternOpKind::Soft))
    }
}

#[cfg(not(feature = "gc-memory-telemetry"))]
mod tests;
