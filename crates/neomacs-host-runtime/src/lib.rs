//! Compile-target-selected host services shared by Neomacs runtimes.
//!
//! This crate owns target facts, not runtime policy. Native builds use the
//! operating system directly; browser WebAssembly builds expose narrow
//! composition-root adapters for services that the browser must provide.

#![deny(missing_docs)]

pub mod process;
pub mod network;
pub mod time;
