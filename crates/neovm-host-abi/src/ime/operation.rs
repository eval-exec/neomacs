//! Ordered editing operations, independent of platform connection objects.

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
