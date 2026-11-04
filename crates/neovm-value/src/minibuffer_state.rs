//! Minibuffer state vocabulary shared with the evaluator.
//!
//! Kept in the value layer so the evaluator's minibuffer-window state can
//! name the entry level without depending on the core command layer.

use std::num::NonZeroUsize;

/// Whether an admitted read enters the first minibuffer or recurses from one.
///
/// GNU uses `minibuf_level == 1` when deciding whether to replace
/// `minibuf_selected_window`.  Keeping that distinction typed prevents callers
/// from reconstructing the lifecycle rule from an unvalidated integer depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinibufferEntryLevel {
    Outermost,
    Recursive,
}

impl MinibufferEntryLevel {
    pub fn from_depth(depth: NonZeroUsize) -> Self {
        if depth.get() == 1 {
            Self::Outermost
        } else {
            Self::Recursive
        }
    }
}
