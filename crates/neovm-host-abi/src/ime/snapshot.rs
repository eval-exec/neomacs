//! Owned, bounded Unicode observations of an editor insertion context.

/// VM-issued identity of one surrounding-text observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeSnapshotId(pub u64);

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
