//! `RowAppearance`: the identity of what a glyph row draws.

use super::super::*;

fn row_with(text: &str) -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (index, ch) in text.chars().enumerate() {
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char(ch, FaceId::new(0), index));
    }
    row
}

#[test]
fn every_row_and_every_copy_has_its_own_appearance() {
    let first = row_with("abc");
    let second = row_with("abc");
    assert_ne!(first.appearance_id(), second.appearance_id());
    let copy = first.clone();
    assert_ne!(
        copy.appearance_id(),
        first.appearance_id(),
        "a copy may be changed, so it starts as a new appearance"
    );
    assert_eq!(copy, first, "the appearance is not part of a row's content");
}

#[test]
fn a_declared_copy_keeps_the_appearance_until_cleared() {
    let original = row_with("abc");
    let mut shifted = original.clone();
    shifted.pixel_y += 1.0;
    shifted.start_charpos += 5;
    shifted.keep_appearance_of(&original);
    assert_eq!(shifted.appearance_id(), original.appearance_id());
    shifted.clear();
    assert_ne!(
        shifted.appearance_id(),
        original.appearance_id(),
        "a cleared row draws something else"
    );
}

#[test]
fn a_shared_row_changed_through_make_mut_is_a_new_appearance() {
    let mut row = MatrixRow::new(row_with("abc"));
    let painted = row.clone();
    let before = row.appearance_id();
    MatrixRow::make_mut(&mut row).glyphs[GlyphArea::Text.index()].clear();
    assert_ne!(row.appearance_id(), before);
    assert_eq!(painted.appearance_id(), before);
}

#[test]
fn the_appearance_is_not_serialized() {
    let row = row_with("abc");
    let json = serde_json::to_string(&row).expect("serialize row");
    assert!(!json.contains("appearance"), "{json}");
    let decoded: GlyphRow = serde_json::from_str(&json).expect("deserialize row");
    assert_eq!(decoded, row);
    assert_ne!(decoded.appearance_id(), row.appearance_id());
}
