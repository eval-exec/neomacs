//! Tier-I tests.

#[path = "analyzer_test.rs"]
mod analyzer;
#[path = "census_test.rs"]
mod census;
#[path = "differential_test.rs"]
mod differential;
#[path = "knobs_test.rs"]
mod knobs;
#[cfg(test)]
#[path = "list_bindings_test.rs"]
mod list_bindings;
#[path = "soak_test.rs"]
mod soak;
