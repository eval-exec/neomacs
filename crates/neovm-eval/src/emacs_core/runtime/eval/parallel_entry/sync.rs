//! The keeper driver's synchronization seam (L6.7 K9). The driver takes its
//! channel types only from here, so the Loom copy substitutes Loom's and the
//! std and Loom builds share one driver body.
#![forbid(unsafe_code)]

pub(crate) use std::sync::mpsc;
