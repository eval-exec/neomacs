//! E4 executor substrate: the E2-independent part of the owner-local parallel
//! evaluator (E4-DESIGN §3.1, §4 and §5), DORMANT until E4 stage 1.
//!
//! Three plain-data pieces live here:
//!
//! - [`dispatch`]: accessors over the generated cold dispatch rows (the
//!   Scalar/Sealed/World arm plus the folded green-guard re-entry axis);
//! - [`frames`]: the owned frame arena, handler/bind/slot records, typed
//!   marks and the root schema each detachable frame publishes;
//! - [`effective`]: the effective-Plain value API types and the one
//!   read/write rule (E4-DESIGN §4.1-§4.2) as pure functions.
//!
//! Nothing in this module is reachable from a serial dispatcher, constructs
//! an E1 capability (there is still no `ParallelExecution` constructor), or
//! allocates TLS. E4 stage 1 supplies the E2-minted execution value and the
//! serial escape; these structures are its owned state.

pub(crate) mod dispatch;
pub(crate) mod effective;
pub(crate) mod frames;

#[cfg(test)]
#[path = "tests/dispatch_test.rs"]
mod dispatch_tests;
#[cfg(test)]
#[path = "tests/effective_test.rs"]
mod effective_tests;
#[cfg(test)]
#[path = "tests/frames_test.rs"]
mod frames_tests;
