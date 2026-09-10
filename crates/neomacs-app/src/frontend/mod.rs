//! Frontend behavior shared by native-window and direct-surface adapters.
//!
//! This module owns no evaluator, native window, or GPU resource. Platform
//! adapters supply owned display data and translate their input locally.

pub mod menu;
