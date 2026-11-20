//! E4 executor substrate: the E2-independent part of the owner-local parallel
//! evaluator (E4-DESIGN §3.1, §4 and §5), DORMANT until E4 stage 1.
//!
//! Four dormant pieces live here:
//!
//! - [`dispatch`]: accessors over the generated cold dispatch rows (the
//!   Scalar/Sealed/World arm plus the folded green-guard re-entry axis);
//! - [`frames`]: the owned frame arena, handler/bind/slot records, typed
//!   marks and the root schema each detachable frame publishes;
//! - [`effective`]: the effective-Plain value API types and the one
//!   read/write rule (E4-DESIGN §4.1-§4.2) as pure functions;
//! - [`engine`]: the stage-1 tree-walk stepper. Tests are its only caller.
//!
//! Nothing in this module is reachable from a serial dispatcher, constructs
//! an E1 capability (there is still no `ParallelExecution` constructor), or
//! allocates TLS. The real production stage 1 (a real serial escape and an
//! executor) stays gated on READY-E2B. This harness records the escape.

pub(crate) mod dispatch;
pub(crate) mod effective;
#[allow(dead_code)] // Harness-only until a test target compiles the stepper.
pub(crate) mod engine;
#[doc(hidden)]
pub mod frames;

#[cfg(test)]
#[path = "tests/dispatch_test.rs"]
mod dispatch_tests;
#[cfg(test)]
#[path = "tests/effective_test.rs"]
mod effective_tests;
#[cfg(test)]
#[path = "tests/engine_test.rs"]
mod engine_tests;
#[cfg(test)]
#[path = "tests/frames_test.rs"]
mod frames_tests;
#[cfg(test)]
#[path = "tests/join_test.rs"]
mod join_tests;
