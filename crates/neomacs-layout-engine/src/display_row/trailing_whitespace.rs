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
//! A stretch can also represent a non-whitespace character (for example an
//! org-modern table border). The last candidate must first pass GNU's source
//! predicate; string-owned glyphs block the walk instead of being skipped.
//!
//! WHEN the highlight runs is not decided here: [`super::line_end::plan`] is
//! the one place that orders it (first, at a true line end only) for every
//! row producer.

use crate::display_current_row_output::DisplayCurrentRowMutation;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphProvenance, GlyphRow, GlyphType,
};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{CharPos0, EmacsBytePos};

/// GNU `trailing_whitespace_p`: inspect actual accessible buffer bytes, not
/// the shape of a display replacement, and suppress the run ending at point.
/// The predicate is asked once for the last candidate, before the row walk.
pub(crate) fn buffer_trailing_whitespace_p(
    buffer: &impl crate::neovm_bridge::LayoutBufferView,
    point: i64,
    from: CharPos0,
) -> bool {
    if from > buffer.layout_point_max_char_pos() {
        return false;
    }
    let end = buffer.layout_point_max_emacs_byte_pos();
    let mut pos = buffer.layout_char_pos_to_emacs_byte_pos(from);
    if pos < buffer.layout_point_min_emacs_byte_pos() {
        return false;
    }
    while pos < end && matches!(buffer.layout_emacs_byte_at_pos(pos), Some(b' ' | b'\t')) {
        pos = EmacsBytePos::new(pos.get() + 1);
    }
    (pos == end || matches!(buffer.layout_emacs_byte_at_pos(pos), Some(b'\n' | b'\r')))
        && buffer.layout_emacs_byte_pos_to_char_pos(pos).get() as i64 != point
}

/// Re-face the current row's trailing whitespace glyphs with the
/// `trailing-whitespace` face (GNU `highlight_trailing_whitespace`, xdisp.c).
/// Walks the TEXT-area glyphs from the end backward — past appended
/// positionless glyphs first — over space `Char` glyphs and `Stretch` glyphs
/// (tabs) that map to a buffer position, stamping each with `face_id` until
/// the first non-whitespace or positionless glyph. A `Glyph`'s background is
/// resolved from its `face_id`, so this paints the trailing run through the
/// same per-glyph background path the `region` face uses. Called only at true
/// line ends (before a real newline / at ZV), never at a visual wrap.
pub(super) struct HighlightTrailingWhitespaceMutation<'a> {
    pub(super) face_id: FaceId,
    pub(super) source_is_trailing: &'a dyn Fn(CharPos0) -> bool,
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

impl DisplayCurrentRowMutation for HighlightTrailingWhitespaceMutation<'_> {
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
            let nil_object = match glyph.provenance {
                GlyphProvenance::Buffer { .. } | GlyphProvenance::Str { .. } => false,
                GlyphProvenance::LineEnd
                | GlyphProvenance::Mark
                | GlyphProvenance::LeftTruncation
                | GlyphProvenance::EmptyLineNewline { .. } => true,
            };
            if !skippable || !nil_object {
                break;
            }
            end -= 1;
        }
        let Some(last) = end.checked_sub(1).map(|index| &glyphs[index]) else {
            return;
        };
        let Some(charpos) = last.provenance.buffer_charpos() else {
            return;
        };
        if !is_whitespace_glyph(last) || !(self.source_is_trailing)(CharPos0::new(charpos)) {
            return;
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
#[path = "trailing_whitespace/tests/trailing_whitespace_test.rs"]
mod tests;
