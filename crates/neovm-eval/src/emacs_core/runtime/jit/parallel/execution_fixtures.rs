//! Execution fixtures shared by the eval parallel test files.
//!
//! These files were re-homed from neovm-value's cfg(test) tree by peer
//! #1482 item C: value cannot name the evaluator, and this module sits
//! beside the eval tests that use it. The activation helper and its seven
//! tests have their single home in core's re-homed execution/tests.rs. The
//! cohort/hof fixtures below are the single home of the fixtures the eval
//! parallel tests use; the value parent, its children and its `mod tests`
//! declaration are gone, so no file exists in both crates.
//!
//! This parent keeps only what the cohort/hof children need: the shared
//! import preamble, `ActualMode` and the value-local `FixtureReceipt`.
mod cohort;
mod hof;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::ParallelHeapEraWorldCollectExt;
#[allow(unused_imports)]
pub(crate) use cohort::{
    CohortPhase, CohortSize, PublicationFacts, PublicationInputs, prepared_hof_cohort,
    prepared_publication_cohort,
};
#[allow(unused_imports)]
pub(crate) use hof::{HofTestInputs, prepared_hof_fixture};
use neovm_value::tagged::gc::mutator_registration::*;
use neovm_value::tagged::gc::{Gen1Admission, Gen1AdmissionError, HeapIdentity};
use neovm_value::tagged::value::{Fixnum, TaggedValue};
use neovm_value::test_support::authority::{
    Deadline, Retry, SPIN_WAIT, collector_generational_enabled_for_test,
    era_prepare_hof_builtin_for_test, era_prepare_unary_function_symbol_for_test, function, global,
    hof_scratch_new_for_test, hof_singleton_new_for_test, nil_root_for_test, registration,
    registry_first_chunk_word_for_test, registry_publication_facts_for_test, rendezvous, retry,
    retry_owned, suppress_for_test, take_legacy_roots_for_test,
    with_registry_admission_gate_for_test,
};
// The retry helpers the consumers need are named directly at their
// authority fence path (TA2: a rename must not resolve to a fence module).
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
