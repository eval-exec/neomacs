//! Test the actual lower owner-step source without widening production APIs.
#![allow(dead_code)]

#[path = "../../../../neovm-eval/src/host_effect/contracts.rs"]
pub(crate) mod contracts;
#[path = "../../../../neovm-eval/src/host_effect/owner_step.rs"]
pub(crate) mod owner_step;
