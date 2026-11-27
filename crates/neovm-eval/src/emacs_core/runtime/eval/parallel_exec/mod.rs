//! E4 executor substrate: the E2-independent part of the owner-local parallel
//! evaluator (E4-DESIGN §3.1, §4 and §5), DORMANT until E4 stage 1.
//!
//! Six dormant pieces live here:
//!
//! - [`backtrace`]: read-only [`OwnerBacktraceView`] snapshot and stopped-only
//!   `set_debug_on_exit` adapter over `FrameArena`'s published frames (V6);
//! - [`dispatch`]: accessors over the generated cold dispatch rows (the
//!   Scalar/Sealed/World arm plus the folded green-guard re-entry axis);
//! - [`frames`]: the owned frame arena, handler/bind/slot records, typed
//!   marks and the root schema each detachable frame publishes;
//! - [`effective`]: the effective-Plain value API types and the one
//!   read/write rule (E4-DESIGN §4.1-§4.2) as pure functions;
//! - [`engine`]: the stage-1 tree-walk stepper; tests are its only caller;
//! - [`vm`]: the stage-1 bytecode stepper over one owned frame
//!   (VM-EXECUTOR-DESIGN r1). Scalar opcodes step the operand segment
//!   owner-locally, the sealed cons opcodes call a port, and every World
//!   arm is exactly one escape. Tests are its only caller.
//!
//! Nothing in this module is reachable from a serial dispatcher, constructs
//! an E1 capability (there is still no `ParallelExecution` constructor), or
//! allocates TLS. The real production stage 1 (a real serial escape and an
//! executor) stays gated on READY-E2B. This harness records the escape.

#[allow(dead_code)] // Harness-only until E5/E6-P wires the backtrace adapter.
pub(crate) mod backtrace;
#[allow(dead_code)] // Dormant E4-P2 Slice 2; E6-P wires CanonicalPlain.
pub(crate) mod canonical_plain;
pub(crate) mod dispatch;
pub(crate) mod effective;
#[allow(dead_code)] // Harness-only until a test target compiles the stepper.
pub(crate) mod engine;
#[doc(hidden)]
pub mod frames;
#[allow(dead_code)] // Harness-only until E6-P wires the executor.
pub(crate) mod vm;

#[cfg(test)]
#[path = "tests/canonical_plain_test.rs"]
mod canonical_plain_tests;
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
#[cfg(test)]
#[path = "tests/poll_test.rs"]
mod poll_tests;
#[cfg(test)]
#[path = "tests/region_test.rs"]
mod region_tests;
#[cfg(test)]
#[path = "tests/tier1_leaf_test.rs"]
mod tier1_leaf_tests;
#[cfg(test)]
#[path = "tests/vm_test.rs"]
mod vm_tests;
