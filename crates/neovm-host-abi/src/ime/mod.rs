//! Ordered input-method protocol. No editor pointers cross this interface.

mod identity;
mod offset;
mod operation;
mod replacement;
mod request;
mod snapshot;

pub use identity::{ImeSessionId, ImeSnapshotId};
pub use offset::ImeUtf16Offset;
pub use operation::ImeOperation;
pub use replacement::{ImeReplacement, ImeReplacementAcknowledgement, ImeReplacementOutcome};
pub use request::{ImeSelection, ImeSelectionAcknowledgement, ImeSelectionOutcome};
pub use snapshot::ImeTextSnapshot;

#[cfg(test)]
mod tests;
