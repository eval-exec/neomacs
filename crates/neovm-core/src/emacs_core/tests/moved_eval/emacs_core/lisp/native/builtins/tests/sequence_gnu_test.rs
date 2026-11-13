//! GNU 31.1 pins for allocation failures, list prefixes and numeric value ordering.
//! Frozen expectations come from retained bounded GNU 31.1 oracle receipts.
use crate::test_utils::runtime_startup_eval_one;

pub(super) fn assert_gnu(_name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = fixture.trim_end().to_owned();
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[cfg(test)]
#[path = "sequence_gnu/prefix_test.rs"]
mod prefix;

#[cfg(test)]
#[path = "sequence_gnu/ntake_test.rs"]
mod ntake;

#[cfg(test)]
#[path = "sequence_gnu/comparison_test.rs"]
mod comparison;

#[cfg(test)]
#[path = "sequence_gnu/allocation_test.rs"]
mod allocation;

#[cfg(test)]
#[path = "sequence_gnu/compiled_test.rs"]
mod compiled;
