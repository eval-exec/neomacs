//! Crate-local registration vocabulary, reexported by the heap facade.
//! Keep this list with its domain so new bounded operations do not expand gc.rs.
#[doc(hidden)]
pub use super::symbol_authority::{PreparedHofBuiltin, PreparedUnaryFunction};
#[doc(hidden)]
pub use super::{
    CellRead, CellReadError, EraAdmissionFailure, EraKeeper, ExecutionActivation,
    FrozenFunctionRoots, FrozenLegacyRoots, HofError, HofScratch, HofSingleton, ImmediateResult,
    ImmediateResultError, SymbolWriteToken,
};
pub(crate) use super::{
    EraFinishFailure, OwnerRootAdmission, StoppedRootAdmission, SymbolReadToken,
};

#[cfg(any(test, feature = "test-support"))]
pub(crate) use super::{GenericProofEra, GenericProofEraFinishFailure};
