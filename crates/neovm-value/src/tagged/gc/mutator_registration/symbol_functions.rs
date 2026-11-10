//! Closed C4 instruction syntax and native twins of the production rooted API.
use super::{FunctionError, RunAccess, SymbolHandle};
use std::num::NonZeroU32;

pub(crate) mod retry;

/// Crate-private seam for the authority REGWAIT-GAP `function` classifier
/// fence (peer #1482 condition 3 audit). Only this module may name the
/// restricted `retry::function`, so the public route is the authority fence
/// and the module chain stays private.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn function_retry_for_fence(error: &FunctionError) -> retry::Retry {
    retry::function(error)
}

/// Closed syntax only: handles and resident operand homes, no raw callee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FunctionOp {
    Function { symbol: u16, dst: u16 },
    Fset { symbol: u16, src: u16, dst: u16 },
    Fmakunbound { symbol: u16, dst: u16 },
    CallBegin { symbol: u16 },
    CallStep { dst: u16, fuel: NonZeroU32 },
}

#[cfg(all(
    test,
    not(feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
pub(crate) mod boundary;
#[cfg(all(feature = "test-support", not(feature = "gc-memory-telemetry")))]
pub mod boundary;
