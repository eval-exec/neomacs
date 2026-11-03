//! Test-only C3 executor syntax. Operations use the real sealed authority.
use super::{GlobalError, OwnerRootSlot, RunAccess, SymbolHandle};

/// Closed executor surface. `symbol` indexes serially admitted real World
/// handles, not arbitrary symbol values; all other indices name owner roots.
/// restricted_lisp::Op/Flow::Lease uses these without opening an executor
/// route to legacy Obarray/Context accessors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GlobalOp {
    Value { symbol: u16, dst: u16 },
    Set { symbol: u16, src: u16, dst: u16 },
    Boundp { symbol: u16, dst: u16 },
    Makunbound { symbol: u16, dst: u16 },
}

#[cfg(not(feature = "gc-memory-telemetry"))]
mod tests;
#[cfg(not(feature = "gc-memory-telemetry"))]
mod transitions;

#[cfg(not(feature = "gc-memory-telemetry"))]
mod histories;
