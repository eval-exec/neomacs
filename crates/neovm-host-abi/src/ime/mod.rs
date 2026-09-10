//! Ordered input-method protocol. No editor pointers cross this interface.

mod offset;
mod request;
mod snapshot;

pub use offset::ImeUtf16Offset;
pub use request::{ImeOperation, ImeSelection, ImeSelectionOutcome, ImeSessionId};
pub use snapshot::{ImeSnapshotId, ImeTextSnapshot};

#[cfg(test)]
mod tests;
