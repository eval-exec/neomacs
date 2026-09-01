//! GNU `highlight_trailing_whitespace` (xdisp.c:24839) as a standalone row
//! mutation.
//!
//! GNU's routine operates on the completed glyph row regardless of which
//! source produced it, but it can only ever re-face BUFFER glyphs: it first
//! skips row-end `Char`/`Stretch` glyphs carrying a nil object (the appended
//! newline space, face-extension stretches, truncation and continuation
//! glyphs), then walks backward re-facing while the glyph's object is the
//! buffer and the glyph is a space `Char` or a `Stretch` (a tab). Neomacs
//! glyph provenance carries the same discriminator, so this mutation can
//! require the `Buffer` arm directly instead of inferring it from a sentinel.
//!
//! WHEN the highlight runs is not decided here: [`super::line_end::plan`] is
//! the one place that orders it (first, at a true line end only) for every
//! row producer.

use crate::display_current_row_output::DisplayCurrentRowMutation;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::types::FaceId;

/// Re-face the current row's trailing whitespace glyphs with the
/// `trailing-whitespace` face (GNU `highlight_trailing_whitespace`, xdisp.c).
/// Walks the TEXT-area glyphs from the end backward — past appended
/// positionless glyphs first — over space `Char` glyphs and `Stretch` glyphs
/// (tabs) that map to a buffer position, stamping each with `face_id` until
/// the first non-whitespace or positionless glyph. A `Glyph`'s background is
/// resolved from its `face_id`, so this paints the trailing run through the
/// same per-glyph background path the `region` face uses. Called only at true
/// line ends (before a real newline / at ZV), never at a visual wrap.
pub(super) struct HighlightTrailingWhitespaceMutation {
    pub(super) face_id: FaceId,
}

/// GNU also accepts only space `Char` glyphs (tabs render as `Stretch`); the
/// literal `'\t'` arm is kept from the pre-seam behavior in case a producer
/// ever emits a raw tab character glyph.
fn is_whitespace_glyph(glyph: &Glyph) -> bool {
    match glyph.glyph_type {
        GlyphType::Char { ch } => ch == ' ' || ch == '\t',
        GlyphType::Stretch { .. } => true,
        _ => false,
    }
}

fn has_buffer_position(glyph: &Glyph) -> bool {
    glyph.provenance.buffer_charpos().is_some()
}

impl DisplayCurrentRowMutation for HighlightTrailingWhitespaceMutation {
    type Output = ();

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        let glyphs = &mut row.glyphs[GlyphArea::Text.index()];
        // GNU: "Skip over glyphs inserted to display the cursor at the end of
        // a line, for extending the face of the last glyph to the end of the
        // line on terminals, and for truncation and continuation glyphs" —
        // Char/Stretch glyphs with a nil object.
        let mut end = glyphs.len();
        while end > 0 {
            let glyph = &glyphs[end - 1];
            let skippable = matches!(
                glyph.glyph_type,
                GlyphType::Char { .. } | GlyphType::Stretch { .. }
            );
            if !skippable || has_buffer_position(glyph) {
                break;
            }
            end -= 1;
        }
        // Re-face backward while the glyph is buffer whitespace (GNU's
        // `BUFFERP (glyph->object)` plus space/stretch condition).
        let mut start = end;
        while start > 0 {
            let glyph = &glyphs[start - 1];
            if !has_buffer_position(glyph) || !is_whitespace_glyph(glyph) {
                break;
            }
            start -= 1;
        }
        for glyph in &mut glyphs[start..end] {
            glyph.face_id = self.face_id;
        }
    }
}

#[cfg(test)]
#[path = "trailing_whitespace_test.rs"]
mod tests;
