//! Window-finalization selection of the chrome paragraph direction.

use super::*;
use crate::bidi::{ParagraphDirection, ParagraphDirections};
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphProvenance, GlyphRow};
use neomacs_display_protocol::types::{FaceId, Rect};

/// The reporter's Arabic row, `JA` = U+062C U+0627, as buffer glyphs.
fn arabic_row(role: GlyphRowRole) -> GlyphRow {
    let mut row = GlyphRow::new(role);
    row.glyphs[GlyphArea::Text.index()] = "جا"
        .chars()
        .enumerate()
        .map(|(position, ch)| {
            Glyph::char_with_provenance(ch, FaceId::new(1), GlyphProvenance::buffer(position))
        })
        .collect();
    row
}

/// The window finalizer must pick `chrome` for a ModeLine row even when the
/// buffer's `text` direction is RightToLeft; the same Arabic glyphs in a Text
/// row prove the text direction really is R2L in this window.  GNU pins
/// `display_mode_line` to L2R (`src/xdisp.c:28165`).
#[test]
fn finalization_picks_the_chrome_direction_for_chrome_rows() {
    let mut state = OutputWindowBuildState::new();
    let bounds = Rect::new(0.0, 0.0, 80.0, 32.0);
    state.install_window_lifecycle(
        OutputWindowLifecycleRequest::begin(1, 2, 10, bounds, bounds, bounds, false)
            .with_bidi_paragraph_directions(ParagraphDirections {
                text: ParagraphDirection::RightToLeft,
                chrome: ParagraphDirection::LeftToRight,
            }),
    );
    state.install_row_lifecycle(
        OutputRowLifecycleRequest::complete(
            0,
            GlyphRowRole::Text,
            false,
            arabic_row(GlyphRowRole::Text),
        ),
        None,
    );
    state.install_row_lifecycle(
        OutputRowLifecycleRequest::complete(
            1,
            GlyphRowRole::ModeLine,
            true,
            arabic_row(GlyphRowRole::ModeLine),
        ),
        None,
    );

    let text_row = state.current_window_row(0).expect("text row was installed");
    let chrome_row = state
        .current_window_row(1)
        .expect("mode-line row was installed");
    assert!(
        text_row.reversed_p,
        "the Text row must follow the window's text=RightToLeft direction"
    );
    assert!(
        !chrome_row.reversed_p,
        "the ModeLine row must follow chrome=LeftToRight, not the buffer text direction"
    );
    assert!(
        chrome_row.glyphs[GlyphArea::Text.index()]
            .iter()
            .all(|glyph| glyph.bidi_level == 1),
        "an LTR paragraph resolves its Arabic run at level 1"
    );
}
