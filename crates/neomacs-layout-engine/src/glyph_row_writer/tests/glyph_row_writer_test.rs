use super::*;

#[test]
fn bidi_finalization_preserves_owned_glyph_storage_and_cursor_mapping() {
    for (text, expected, cursor) in [("abc", "abc", 0), ("אבג", "גבא", 2)] {
        let mut row = GlyphRow::new(neomacs_display_protocol::GlyphRowRole::Text);
        row.glyphs[GlyphArea::Text.index()] = text
            .chars()
            .enumerate()
            .map(|(position, ch)| {
                Glyph::char_with_provenance(ch, FaceId::new(1), GlyphProvenance::buffer(position))
            })
            .collect();
        row.cursor_col = Some(0);
        let allocation = row.glyphs[GlyphArea::Text.index()].as_ptr();
        assert_eq!(reorder_row_bidi(&mut row, Some(0)), Some(cursor));
        assert_eq!(row.cursor_col, Some(cursor));
        let rendered: String = row.glyphs[GlyphArea::Text.index()]
            .iter()
            .filter_map(|glyph| match glyph.glyph_type {
                GlyphType::Char { ch } => Some(ch),
                _ => None,
            })
            .collect();
        assert_eq!(rendered, expected);
        assert_eq!(
            row.glyphs[GlyphArea::Text.index()].as_ptr(),
            allocation,
            "bidi finalization must reorder owned glyphs without copying their storage"
        );
    }
}

/// GNU reads the buffer's `bidi-display-reordering` / `bidi-paragraph-direction`
/// once per window at start_display (src/xdisp.c:3600-3611) and hands that
/// `it->paragraph_embedding` to `bidi_paragraph_init` (src/bidi.c:1713): a
/// `left-to-right` paragraph keeps the RTL run reversed but left-aligned, a
/// `right-to-left` paragraph right-aligns it (`reversed_p`), and a nil
/// `bidi-display-reordering` leaves the logical order untouched.
#[test]
fn paragraph_direction_pins_the_base_level_and_alignment() {
    use crate::bidi::ParagraphDirection;

    // Reporter text: JA = U+062C U+0627.
    let cases = [
        (ParagraphDirection::Auto, true, "اج"),
        (ParagraphDirection::LeftToRight, false, "اج"),
        (ParagraphDirection::RightToLeft, true, "اج"),
        (ParagraphDirection::Off, false, "جا"),
    ];
    for (direction, expected_reversed, expected_order) in cases {
        let mut row = GlyphRow::new(neomacs_display_protocol::GlyphRowRole::Text);
        row.glyphs[GlyphArea::Text.index()] = "جا"
            .chars()
            .enumerate()
            .map(|(position, ch)| {
                Glyph::char_with_provenance(ch, FaceId::new(1), GlyphProvenance::buffer(position))
            })
            .collect();
        reorder_row_bidi_with_direction(&mut row, None, direction);
        assert_eq!(
            row.reversed_p, expected_reversed,
            "{direction:?}: reversed_p must follow the paragraph base level"
        );
        let rendered: String = row.glyphs[GlyphArea::Text.index()]
            .iter()
            .filter_map(|glyph| match glyph.glyph_type {
                GlyphType::Char { ch } => Some(ch),
                _ => None,
            })
            .collect();
        assert_eq!(rendered, expected_order, "{direction:?}: visual order");
        let expected_level = match direction {
            ParagraphDirection::LeftToRight | ParagraphDirection::RightToLeft => 1,
            ParagraphDirection::Auto | ParagraphDirection::Off => {
                // Auto resolves RTL here; Off leaves every glyph at level 0.
                u8::from(direction == ParagraphDirection::Auto)
            }
        };
        assert!(
            row.glyphs[GlyphArea::Text.index()]
                .iter()
                .all(|glyph| glyph.bidi_level == expected_level),
            "{direction:?}: every Arabic glyph must resolve to level {expected_level}"
        );
    }
}
