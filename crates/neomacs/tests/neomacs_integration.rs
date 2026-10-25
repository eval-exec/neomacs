//! Every neomacs integration suite in one test binary.
//!
//! Each former per-file binary linked the crate dependency closure again;
//! nextest runs every test in its own process, so one binary is equivalent
//! at run time.

mod common;

#[path = "argv_parity.rs"]
mod argv_parity;

#[path = "batch_startup.rs"]
mod batch_startup;

#[path = "jit_observability.rs"]
mod jit_observability;

#[path = "main_stack_overflow.rs"]
mod main_stack_overflow;

#[path = "neomacsclient_cli.rs"]
mod neomacsclient_cli;
