//! Owned glyph dependencies beneath an active display table.
use crate::emacs_core::{chartable, value::Value};
use rustc_hash::FxHashSet;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutDisplayTableInput {
    table: usize,
    vectors: Arc<[GlyphVectorInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GlyphVectorInput {
    identity: usize,
    glyphs: Box<[GlyphInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GlyphInput {
    Packed(usize),
    CharacterAndFace { character: usize, face: usize },
}

impl LayoutDisplayTableInput {
    /// Whether this immutable snapshot has no effective display table.
    /// This borrows only captured numeric state; independent mutators may
    /// inspect their snapshots concurrently without reading Lisp objects.
    #[inline]
    pub fn is_unset(&self) -> bool {
        self.table == 0
    }

    pub(super) fn capture(table: Value) -> Self {
        if !chartable::is_char_table(&table) {
            return Self::default();
        }
        let mut pending = vec![table];
        let mut seen = FxHashSet::default();
        let mut vectors = Vec::new();
        while let Some(value) = pending.pop() {
            if !seen.insert(value.bits()) {
                continue;
            }
            if let Some(children) = chartable::display_dependency_children(value) {
                pending.extend(children);
            } else if let Some(glyphs) = value.as_vector_data() {
                vectors.push(GlyphVectorInput {
                    identity: value.bits(),
                    glyphs: glyphs
                        .iter()
                        .map(|glyph| {
                            if glyph.is_cons() {
                                GlyphInput::CharacterAndFace {
                                    character: glyph.cons_car().bits(),
                                    face: glyph.cons_cdr().bits(),
                                }
                            } else {
                                GlyphInput::Packed(glyph.bits())
                            }
                        })
                        .collect(),
                });
            }
        }
        // Cache population and alias traversal order are not glyph changes.
        vectors.sort_unstable_by_key(|vector| vector.identity);
        Self {
            table: table.bits(),
            vectors: vectors.into(),
        }
    }
}
