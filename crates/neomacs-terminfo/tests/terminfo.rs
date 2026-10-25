//! Every neomacs-terminfo integration suite in one test binary.
//!
//! Each former per-file binary linked the crate dependency closure again;
//! nextest runs every test in its own process, so one binary is equivalent
//! at run time.

#[path = "capabilities.rs"]
mod capabilities;

#[path = "gnu_numeric.rs"]
mod gnu_numeric;

#[path = "padding.rs"]
mod padding;
