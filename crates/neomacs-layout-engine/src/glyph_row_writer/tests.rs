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
