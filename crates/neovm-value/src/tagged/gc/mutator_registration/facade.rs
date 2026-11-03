//! Crate-local registration vocabulary, reexported by the heap facade.
//! Keep this list with its domain so new bounded operations do not expand gc.rs.
pub(crate) use super::symbol_authority::{PreparedHofBuiltin, PreparedUnaryFunction};
pub(crate) use super::{
    CellRead, CellReadError, EraAdmissionFailure, EraFinishFailure, EraKeeper, ExecutionActivation,
    FrozenFunctionRoots, FrozenLegacyRoots, HofError, HofScratch, HofSingleton, ImmediateResult,
    ImmediateResultError, OwnerRootAdmission, StoppedRootAdmission, SymbolReadToken,
    SymbolWriteToken,
};

#[cfg(test)]
pub(crate) use super::{
    CohortPhase, CohortSize, ExecutionFixtureReceipt, ExecutionTestDeadline, ExecutionTestHomes,
    ExecutionTestRetry, ExecutionTestSeed, GenericProofEra, GenericProofEraFinishFailure,
    HofTestInputs, PublicationFacts, PublicationInputs, execution_test_retry,
    with_test_prepared_cell_execution, with_test_prepared_execution, with_test_prepared_hof_cohort,
    with_test_prepared_hof_execution, with_test_prepared_publication_cohort,
};
