//! Ordered text-conversion sessions. No editor pointers cross this interface.

/// Host-issued identity, never reused during an editor session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeSessionId(pub u64);

/// VM-issued identity of one surrounding-text observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeSnapshotId(pub u64);

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

/// Bounded, owned Unicode text; no Lisp values or editor pointers cross threads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeTextSnapshot {
    id: ImeSnapshotId,
    text: String,
    cursor: usize,
    anchor: usize,
}

impl ImeTextSnapshot {
    /// Stay below winit's 4000-byte surrounding-text transport limit.
    pub const MAX_BYTES: usize = 3999;

    /// Validate UTF-8 selection offsets and the transport size bound.
    pub fn new(id: ImeSnapshotId, text: String, cursor: usize, anchor: usize) -> Option<Self> {
        if text.len() > Self::MAX_BYTES
            || !text.is_char_boundary(cursor)
            || !text.is_char_boundary(anchor)
        {
            return None;
        }
        Some(Self {
            id,
            text,
            cursor,
            anchor,
        })
    }

    /// Identity to echo with a request based on this observation.
    pub fn id(&self) -> ImeSnapshotId {
        self.id
    }

    /// Accessible Unicode excerpt, without a lossy encoding conversion.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Point as a UTF-8 byte offset within the excerpt.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Active mark as a UTF-8 byte offset; equals cursor without a selection.
    pub fn anchor(&self) -> usize {
        self.anchor
    }
}

/// One ordered operation in an input-method session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeOperation {
    /// Capture the editor's current insertion context.
    Begin,
    /// Replace surrounding text and insert the accepted composition together.
    Replace {
        /// UTF-8 bytes before the captured cursor.
        before_bytes: usize,
        /// UTF-8 bytes after the captured cursor.
        after_bytes: usize,
        /// Accepted text, not an editor key sequence.
        text: String,
    },
    /// Retire the context; later operations using this identity are stale.
    End,
}
