//! Input-method requests and their explicit outcomes.

use super::ImeSnapshotId;

/// A selection request relative to exactly one observed text snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeSelection {
    /// Observation on which the input method based this request.
    pub snapshot: ImeSnapshotId,
    /// Requested point, in UTF-8 bytes within that observation.
    pub cursor: usize,
    /// Requested mark, in UTF-8 bytes within that observation.
    pub anchor: usize,
}

/// A selection request never silently falls back to the current buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImeSelectionOutcome {
    /// The selection was applied using the editor's point/mark operations.
    Applied,
    /// The observation was retired or its editor context has changed.
    StaleSnapshot,
    /// An offset is outside the observed text or splits a Unicode character.
    InvalidSelection,
}
