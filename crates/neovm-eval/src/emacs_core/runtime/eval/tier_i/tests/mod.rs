//! Tier-I tests.
//!
//! The Context-constructing suites (analyzer, census, differential, soak and
//! the fresh-context knob oracle) re-homed to core moved_eval; the
//! context-free parser tests stay here with the remaining file fixtures.

#[path = "knobs_test.rs"]
mod knobs;
#[cfg(test)]
#[path = "list_bindings_test.rs"]
mod list_bindings;
