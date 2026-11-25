//! The keeper driver's synchronization seam (L6.7 K9). The driver takes its
//! channel types, its backoff yield and sleep, and the clock that bounds the
//! backoff only from here, so the Loom copy substitutes its own and the std
//! and Loom builds share one driver body.
#![forbid(unsafe_code)]

pub(crate) use std::sync::mpsc;
/// The clock the backoff measures its contention bound on.
pub(crate) use std::time::Instant;

/// The driver's backoff yields here.
pub(crate) fn yield_now() {
    std::thread::yield_now();
}

/// The driver's backoff sleeps here between later attempts.
pub(crate) fn sleep(duration: std::time::Duration) {
    std::thread::sleep(duration);
}
