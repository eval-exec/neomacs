//! Owned glyph dependencies beneath an active display table.
use neovm_text::emacs_core::chartable;
use neovm_value::value::Value;
use rustc_hash::FxHashSet;

pub use neovm_eval::emacs_core::eval::data::layout_input::display_table_input::LayoutDisplayTableInput;
pub(crate) use neovm_eval::emacs_core::eval::data::layout_input::display_table_input::{
    GlyphInput, GlyphVectorInput,
};

pub(crate) trait LayoutDisplayTableInputExt: Sized {
    fn capture(table: Value) -> Self;
}

impl LayoutDisplayTableInputExt for LayoutDisplayTableInput {
    /// Whether this immutable snapshot has no effective display table.
    /// This borrows only captured numeric state; independent mutators may

    fn capture(table: Value) -> Self {
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
