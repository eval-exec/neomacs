//! Actual native ownership, wake, pending registry and service transport.
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

#[path = "../../../src/host_effect/contracts.rs"]
pub(crate) mod contracts;
#[path = "../../../src/host_effect/native_outcome.rs"]
pub(crate) mod native_outcome;
#[path = "../../../src/host_effect/native_pending.rs"]
pub(crate) mod native_pending;
#[path = "../../../src/host_effect/wake.rs"]
pub(crate) mod wake;

mod host_effect {
    pub(crate) use crate::{contracts, native_outcome, native_pending, wake};
}

mod emacs_core;
