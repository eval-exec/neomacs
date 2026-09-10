//! Replacement coordinates belong to one observed text window.

use super::{ImeSnapshotId, ImeTextSnapshot};

/// Replace an observed range, never an inferred range in the current buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeReplacement {
    /// Observation from which all coordinates were computed.
    pub snapshot: ImeSnapshotId,
    /// Inclusive UTF-8 byte boundary in the original observation.
    pub start: usize,
    /// Exclusive UTF-8 byte boundary in the original observation.
    pub end: usize,
    /// Replacement text, not a keyboard sequence.
    pub text: String,
    /// UTF-8 byte boundary in the projected observation after replacement.
    pub cursor: usize,
}

/// Replacement validation never silently retargets stale input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImeReplacementOutcome {
    /// The requested text change was applied.
    Applied,
    /// The observation or its editor context is no longer authoritative.
    StaleSnapshot,
    /// The range is reversed, outside the observation, or splits a character.
    InvalidRange,
    /// The resulting cursor is outside the observation or splits a character.
    InvalidCursor,
}

/// Result and fresh observation captured during one ordered VM request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeReplacementAcknowledgement {
    /// Whether the requested change was applied.
    pub outcome: ImeReplacementOutcome,
    /// Current observation, absent when privacy or presentation disallows it.
    pub snapshot: Option<ImeTextSnapshot>,
}
