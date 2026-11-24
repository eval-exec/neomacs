//! The keeper driver's synchronization seam (L6.7 K9). The driver takes its
//! channel types and its backoff yield and sleep only from here, so the Loom
//! copy substitutes Loom's and the std and Loom builds share one driver body.
#![forbid(unsafe_code)]

pub(crate) use std::sync::mpsc;

/// The driver's backoff yields here.
pub(crate) fn yield_now() {
    std::thread::yield_now();
}

/// The driver's backoff sleeps here between later attempts.
pub(crate) fn sleep(duration: std::time::Duration) {
    std::thread::sleep(duration);
}
