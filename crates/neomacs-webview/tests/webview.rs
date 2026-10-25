//! Every neomacs-webview integration suite in one test binary.
//!
//! Each former per-file binary linked the crate dependency closure again;
//! nextest runs every test in its own process, so one binary is equivalent
//! at run time.

#[path = "policy.rs"]
mod policy;

#[path = "presentation.rs"]
mod presentation;

#[path = "system.rs"]
mod system;
