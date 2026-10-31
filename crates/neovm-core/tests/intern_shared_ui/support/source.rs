#![allow(dead_code)]
pub mod sync { pub use std::sync::atomic::{AtomicU64, Ordering}; }
#[path = "../../../src/emacs_core/runtime/intern/shared/domain.rs"]
pub mod domain;
#[path = "../../../src/emacs_core/runtime/intern/shared/slot.rs"]
pub mod slot;
