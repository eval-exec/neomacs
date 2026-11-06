//! Slim parent for the restored cohort/hof fixture files (LEAD 21:5xZ).
//!
//! The execution fixture helper (`FixtureSeed`, `prepared_cell_fixture`,
//! `fixture`, `prepared_fixture`) and the seven activation tests now have
//! their single home in core's re-homed `execution/tests.rs`; the value copy
//! was deleted. The cohort/hof fixtures below are NOT duplicates: they are
//! the single home of the fixtures seven eval parallel test files use, and
//! they are restored here. Their later re-home (with those eval tests) to
//! eval's execution fixtures is a separate item.
//!
//! This parent keeps only what the cohort/hof children need: the shared
//! import preamble, `ActualMode` and the value-local `FixtureReceipt`.
#[path = "tests/cohort.rs"]
mod cohort;
#[path = "tests/hof.rs"]
mod hof;
pub(crate) use super::super::retry::{
    Deadline as ExecutionTestDeadline, Retry as ExecutionTestRetry, retry as execution_test_retry,
};
use super::super::symbol_functions::retry::{self as contention, Deadline, Retry, retry_owned};
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::ParallelHeapEraWorldExt;
use crate::tagged::gc::{Gen1Admission, Gen1AdmissionError};
pub(crate) use cohort::{
    CohortPhase, CohortSize, PublicationFacts, PublicationInputs, prepared_hof_cohort,
    prepared_publication_cohort,
};
pub(crate) use hof::{HofTestInputs, prepared_hof_fixture};
use std::ops::ControlFlow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActualMode {
    Gen0,
    Gen1,
}
/// Value-local receipt for the cohort/hof fixtures. The core fixture helper
/// has its own diverged receipt (it panics on a GEN0 refusal instead of
/// returning `Gen0Refused`); the two are kept separate so the cohort/hof
/// fixtures keep their exact pre-split behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FixtureReceipt {
    Gen0Refused,
    Gen1Completed,
    TelemetryRefused,
}
