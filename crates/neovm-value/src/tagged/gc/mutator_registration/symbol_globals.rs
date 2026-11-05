//! Test-only C3 executor syntax. Operations use the real sealed authority.
use super::{GlobalError, OwnerRootSlot, RunAccess, SymbolHandle};

/// Closed executor surface. `symbol` indexes serially admitted real World
/// handles, not arbitrary symbol values; all other indices name owner roots.
/// restricted_lisp::Op/Flow::Lease uses these without opening an executor
/// route to legacy Obarray/Context accessors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(super) enum GlobalOp {
    Value { symbol: u16, dst: u16 },
    Set { symbol: u16, src: u16, dst: u16 },
    Boundp { symbol: u16, dst: u16 },
    Makunbound { symbol: u16, dst: u16 },
}
