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
/// `right-to-left` paragraph right-aligns it (`reversed_p`) and resolves even
/// its L characters at level 2, and a nil `bidi-display-reordering` leaves the
/// logical order untouched.
#[test]
fn paragraph_direction_pins_the_base_level_and_alignment() {
    use crate::bidi::ParagraphDirection;

    // Reporter text: JA = U+062C U+0627.  "ab" is the all-LTR row that an
    // explicit right-to-left paragraph must still reverse and right-align.
    let cases = [
        ("جا", ParagraphDirection::Auto, true, "اج", 1),
        ("جا", ParagraphDirection::LeftToRight, false, "اج", 1),
        ("جا", ParagraphDirection::RightToLeft, true, "اج", 1),
        ("جا", ParagraphDirection::Off, false, "جا", 0),
        ("ab", ParagraphDirection::RightToLeft, true, "ab", 2),
    ];
    for (text, direction, expected_reversed, expected_order, expected_level) in cases {
        let mut row = GlyphRow::new(neomacs_display_protocol::GlyphRowRole::Text);
        row.glyphs[GlyphArea::Text.index()] = text
            .chars()
            .enumerate()
            .map(|(position, ch)| {
                Glyph::char_with_provenance(ch, FaceId::new(1), GlyphProvenance::buffer(position))
            })
            .collect();
        reorder_row_bidi_with_direction(&mut row, None, direction);
        assert_eq!(
            row.reversed_p, expected_reversed,
            "{direction:?} {text:?}: reversed_p must follow the paragraph base level"
        );
        let rendered: String = row.glyphs[GlyphArea::Text.index()]
            .iter()
            .filter_map(|glyph| match glyph.glyph_type {
                GlyphType::Char { ch } => Some(ch),
                _ => None,
            })
            .collect();
        assert_eq!(
            rendered, expected_order,
            "{direction:?} {text:?}: visual order"
        );
        assert!(
            row.glyphs[GlyphArea::Text.index()]
                .iter()
                .all(|glyph| glyph.bidi_level == expected_level),
            "{direction:?} {text:?}: every glyph must resolve to level {expected_level}"
        );
    }
}

/// Mode, header and tab-line rows are strings drawn by `display_mode_line`
/// (src/xdisp.c:28165) and `display_tab_bar` (src/xdisp.c:14727), which set
/// `it->paragraph_embedding = L2R`.  Their first strong character must not
/// turn the row into a right-aligned paragraph.
#[test]
fn chrome_rows_are_pinned_left_to_right_like_gnu() {
    use crate::bidi::ParagraphDirection;

    let mut row = GlyphRow::new(neomacs_display_protocol::GlyphRowRole::ModeLine);
    row.glyphs[GlyphArea::Text.index()] = "جا"
        .chars()
        .enumerate()
        .map(|(position, ch)| {
            Glyph::char_with_provenance(ch, FaceId::new(1), GlyphProvenance::buffer(position))
        })
        .collect();
    reorder_row_bidi_with_direction(&mut row, None, ParagraphDirection::LeftToRight);
    assert!(
        !row.reversed_p,
        "a chrome row must stay left-to-right even when its first strong char is Arabic"
    );
    assert!(
        row.glyphs[GlyphArea::Text.index()]
            .iter()
            .all(|glyph| glyph.bidi_level == 1),
        "an LTR paragraph resolves its RTL run at level 1"
    );
}
