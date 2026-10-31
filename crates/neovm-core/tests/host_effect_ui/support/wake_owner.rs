//! Import the real wake owner without changing production visibility or adding
//! a fixture constructor. The existing thin target already supplies polling and
//! static_assertions. ReadDeadline is its only parent-module dependency.
#![allow(dead_code)]

mod contracts {
    pub use neovm_core::host_effect::contracts::ReadDeadline;
}

#[path = "../../../src/host_effect/wake.rs"]
pub(crate) mod wake;
