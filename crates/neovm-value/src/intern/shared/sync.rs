//! The production atomic backend. Isolated Loom tests include domain.rs with
//! a Loom backend at this same seam; the admission algorithm is not copied.
pub(super) use std::sync::atomic::{AtomicU64, Ordering};
