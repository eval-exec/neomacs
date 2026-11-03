//! Crate-local registration vocabulary, reexported by the heap facade.
//! Keep this list with its domain so new bounded operations do not expand gc.rs.
#[doc(hidden)]
pub use super::symbol_authority::{PreparedHofBuiltin, PreparedUnaryFunction};
#[doc(hidden)]
pub use super::{
    CellRead, CellReadError, ExecutionActivation, FrozenFunctionRoots, FrozenLegacyRoots, HofError,
    HofScratch, HofSingleton, ImmediateResult, ImmediateResultError, SymbolWriteToken,
};
pub(crate) use super::{
    EraAdmissionFailure, EraFinishFailure, EraKeeper, OwnerRootAdmission, StoppedRootAdmission,
    SymbolReadToken,
};

#[cfg(test)]
pub(crate) use super::{
    CohortPhase, CohortSize, ExecutionFixtureReceipt, ExecutionTestDeadline, ExecutionTestHomes,
    ExecutionTestRetry, ExecutionTestSeed, GenericProofEra, GenericProofEraFinishFailure,
    HofTestInputs, PublicationFacts, PublicationInputs, execution_test_retry,
    with_test_prepared_cell_execution, with_test_prepared_execution, with_test_prepared_hof_cohort,
    with_test_prepared_hof_execution, with_test_prepared_publication_cohort,
};
