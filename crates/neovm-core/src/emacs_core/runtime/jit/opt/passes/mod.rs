pub(crate) mod array_range_state;
pub(crate) mod array_reads;
pub(crate) mod bools;
pub(crate) mod cfg;
pub(crate) mod fold;
pub(crate) mod gvn;
pub(crate) mod licm;
pub(crate) mod range;
pub(crate) mod reps;
pub(crate) mod reps_lift;
pub(crate) mod sink;
pub(crate) mod static_fix;

#[cfg(test)]
#[path = "tests/bools_reference_test.rs"]
mod bools_reference_tests;
