//! Ordered input-method protocol. No editor pointers cross this interface.

mod request;
mod snapshot;

pub use request::{ImeOperation, ImeSelection, ImeSelectionOutcome, ImeSessionId};
pub use snapshot::{ImeSnapshotId, ImeTextSnapshot};

#[cfg(test)]
mod tests;
