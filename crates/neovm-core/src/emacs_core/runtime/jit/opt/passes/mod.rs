pub(crate) mod bools;
pub(crate) mod cfg;
pub(crate) mod fold;
pub(crate) mod reps;
pub(crate) mod reps_lift;

#[cfg(test)]
#[path = "tests/bools_reference.rs"]
mod bools_reference_tests;
