//! Exercise the real typed shifter with both clone-area ownership classes.
use super::*;
use neomacs_display_protocol::cursor::CursorStyle;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphPointerAppearance, GlyphPointerSourceIdentity, GlyphProvenance,
    GlyphStringBufferRange, GlyphStringId, GlyphStringSource, GlyphType, TerminalComposition,
    TerminalCompositionCell,
};
use neomacs_display_protocol::types::FaceId;

fn row_after_optional_shift(shifted: bool, cursor: bool) -> GlyphRow {
    let delta = if shifted { 3 } else { 0 };
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.start_charpos = 12 + delta;
    row.end_charpos = 20 + delta;
    row.pixel_y = 40.0;
    row.height_px = 16.0;
    row.ascent_px = 12.0;
    row.hash = 0x1234;
    if cursor {
        row.cursor_col = Some(2);
        row.cursor_type = Some(CursorStyle::Hbar(1.5));
    }
    let string = row
        .push_string_source(GlyphStringSource::replacement(
            GlyphStringId::new(19),
            GlyphStringBufferRange::new(10 + delta, 20 + delta),
        ))
        .unwrap();
    let pointer = row
        .intern_pointer_appearance(GlyphPointerAppearance {
            source: GlyphPointerSourceIdentity {
                kind: GlyphPointerSourceKind::Buffer,
                source_id: 23,
                range_start: (12 + delta) as u64,
                range_end: (20 + delta) as u64,
                property_owner: 29,
                occurrence: GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                    buffer_id: 23,
                    anchor: (12 + delta) as u64,
                },
            },
            face_id: FaceId::new(31),
        })
        .unwrap();
    let mut before = Glyph::char('a', FaceId::new(31), 3);
    before.pointer_appearance = Some(pointer);
    row.glyphs[GlyphArea::LeftMargin.index()].push(before);
    row.glyphs[GlyphArea::LeftMargin.index()].push(Glyph::char_with_provenance(
        'm',
        FaceId::new(31),
        GlyphProvenance::Mark,
    ));
    let mut owned = Glyph::char('b', FaceId::new(31), 12 + delta);
    owned.glyph_type = GlyphType::Composite {
        text: "owned".into(),
    };
    owned.pointer_appearance = Some(pointer);
    row.glyphs[GlyphArea::Text.index()].push(owned);
    let mut automatic =
        Glyph::char_with_provenance('e', FaceId::new(31), GlyphProvenance::string(string, 2));
    automatic.glyph_type = GlyphType::AutomaticComposite {
        text: "e\u{301}".into(),
        terminal: TerminalComposition {
            cells: vec![TerminalCompositionCell {
                base: 'e',
                extenders: "\u{301}".into(),
                width_cols: 1,
                source_char_len: 2,
            }]
            .into_boxed_slice(),
            width_cols: 1,
        }
        .into(),
    };
    row.glyphs[GlyphArea::RightMargin.index()].push(automatic);
    row
}

#[test]
fn clone_area_replay_keeps_typed_shift_and_source_immutability() {
    let source = MatrixRow::new(row_after_optional_shift(false, true));
    // Independent expected construction does not call GlyphRow::clone or the
    // shifter, and explicitly keeps string indexes / marker provenance fixed.
    let expected = row_after_optional_shift(true, false);
    let actual = shift_row_positions(&source, 10, 3);
    assert_eq!(&*actual, &expected);
    assert_eq!(actual.appearance_id(), source.appearance_id());
    assert_eq!(&*source, &row_after_optional_shift(false, true));
    for area in GlyphArea::ALL {
        assert_ne!(
            actual.glyphs[area.index()].as_ptr(),
            source.glyphs[area.index()].as_ptr()
        );
    }
}

#[test]
fn clone_area_zero_delta_still_strips_cursor_and_keeps_appearance() {
    let source = MatrixRow::new(row_after_optional_shift(false, true));
    let actual = shift_row_positions(&source, 10, 0);
    assert_eq!(&*actual, &row_after_optional_shift(false, false));
    assert_eq!(actual.appearance_id(), source.appearance_id());
    assert_eq!(source.cursor_col, Some(2));
    assert_eq!(source.cursor_type, Some(CursorStyle::Hbar(1.5)));
}
