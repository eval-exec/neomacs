#![allow(dead_code)]
pub mod sync { pub use std::sync::atomic::{AtomicU64, Ordering}; }
#[path = "../../../../neovm-value/src/intern/shared/domain.rs"]
pub mod domain;
#[path = "../../../../neovm-value/src/intern/shared/slot.rs"]
pub mod slot;
