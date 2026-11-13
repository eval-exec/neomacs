//! Layout input records for display table input (stage-4 W2/E5).

use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutDisplayTableInput {
    pub table: usize,
    pub vectors: Arc<[GlyphVectorInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct GlyphVectorInput {
    pub identity: usize,
    pub glyphs: Box<[GlyphInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum GlyphInput {
    Packed(usize),
    CharacterAndFace { character: usize, face: usize },
}

impl LayoutDisplayTableInput {
    /// Whether this immutable snapshot has no effective display table.
    #[inline]
    pub fn is_unset(&self) -> bool {
        self.table == 0
    }
}
