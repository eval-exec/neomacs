//! Closed C4 instruction syntax and native twins of the production rooted API.
use super::{
    FunctionActivation, FunctionCallCache, FunctionError, FunctionOpKind, FunctionStep,
    OwnerRootSlot, RunAccess, SymbolHandle,
};
use std::num::NonZeroU32;

pub(super) mod retry;

/// Closed syntax only: handles and resident operand homes, no raw callee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FunctionOp {
    Function { symbol: u16, dst: u16 },
    Fset { symbol: u16, src: u16, dst: u16 },
    Fmakunbound { symbol: u16, dst: u16 },
    CallBegin { symbol: u16 },
    CallStep { dst: u16, fuel: NonZeroU32 },
}

#[cfg(not(any(test, feature = "test-support")))]
#[cfg(not(feature = "gc-memory-telemetry"))]
mod tests;
#[cfg(any(test, feature = "test-support"))]
#[cfg(not(feature = "gc-memory-telemetry"))]
pub mod tests;

#[cfg(not(feature = "gc-memory-telemetry"))]
pub(crate) mod boundary;

#[cfg(not(any(test, feature = "test-support")))]
#[cfg(not(feature = "gc-memory-telemetry"))]
pub(super) mod names;
#[cfg(any(test, feature = "test-support"))]
#[cfg(not(feature = "gc-memory-telemetry"))]
pub mod names;
