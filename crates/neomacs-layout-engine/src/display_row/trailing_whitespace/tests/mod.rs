//! Unit tests for the trailing-whitespace highlight mutation (GNU
//! `highlight_trailing_whitespace`, xdisp.c).
//!
//! These exercise `HighlightTrailingWhitespaceMutation` directly against a
//! freshly built `GlyphRow`: the run of space `Char` glyphs and tab `Stretch`
//! glyphs at the END of the TEXT area is re-faced with the trailing-whitespace
//! face; earlier glyphs and any interior (non-trailing) whitespace keep their
//! original face. Mirroring GNU's object checks: glyphs without a buffer
//! position (the appended newline space, face-extension stretches) are
//! SKIPPED at the row end, and only buffer-position glyphs are ever re-faced.

use super::HighlightTrailingWhitespaceMutation;
use crate::output::row_request::DisplayCurrentRowMutation;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphProvenance, GlyphRow};
use neomacs_display_protocol::types::FaceId;

const TEXT_FACE: FaceId = FaceId::new(1);
const TWS_FACE: FaceId = FaceId::new(42);

fn push_char(row: &mut GlyphRow, ch: char, charpos: usize) {
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char(ch, TEXT_FACE, charpos));
}

fn push_tab(row: &mut GlyphRow) {
    row.glyphs[GlyphArea::Text.index()].push(Glyph::stretch(8, TEXT_FACE));
}

fn push_positionless_space(row: &mut GlyphRow) {
    row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char(' ', TEXT_FACE, 0).with_provenance(GlyphProvenance::mark()));
}

fn push_positionless_stretch(row: &mut GlyphRow) {
    let mut glyph = Glyph::stretch(4, TEXT_FACE);
    glyph.provenance = GlyphProvenance::mark();
    row.glyphs[GlyphArea::Text.index()].push(glyph);
}

fn face_ids(row: &GlyphRow) -> Vec<FaceId> {
    row.glyphs[GlyphArea::Text.index()]
        .iter()
        .map(|g| g.face_id)
        .collect()
}

#[test]
fn trailing_spaces_are_refaced_leading_text_kept() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "ab  ".chars().enumerate() {
        push_char(&mut row, ch, i);
    }

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TEXT_FACE, TEXT_FACE, TWS_FACE, TWS_FACE],
        "only the two trailing spaces are re-faced"
    );
}

#[test]
fn trailing_tab_stretch_is_refaced() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_char(&mut row, 'x', 0);
    push_tab(&mut row);

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TEXT_FACE, TWS_FACE],
        "a trailing tab (Stretch) counts as trailing whitespace"
    );
}

#[test]
fn interior_whitespace_is_not_refaced() {
    // "ab  cd  " — only the FINAL run of spaces is trailing.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "ab  cd  ".chars().enumerate() {
        push_char(&mut row, ch, i);
    }

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![
            TEXT_FACE, TEXT_FACE, // a b
            TEXT_FACE, TEXT_FACE, // interior "  " — NOT trailing
            TEXT_FACE, TEXT_FACE, // c d
            TWS_FACE, TWS_FACE, // trailing "  "
        ],
        "interior whitespace between words must be left untouched"
    );
}

#[test]
fn line_with_no_trailing_whitespace_is_unchanged() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "abc".chars().enumerate() {
        push_char(&mut row, ch, i);
    }

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TEXT_FACE, TEXT_FACE, TEXT_FACE],
        "a line ending in a non-space glyph keeps every face"
    );
}

#[test]
fn appended_positionless_newline_space_is_skipped_but_buffer_run_refaced() {
    // GNU skips row-end glyphs with a nil object (its appended newline space)
    // before deciding, then re-faces only buffer glyphs — the appended space
    // itself keeps its face.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "a  ".chars().enumerate() {
        push_char(&mut row, ch, i);
    }
    push_positionless_space(&mut row);

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TEXT_FACE, TWS_FACE, TWS_FACE, TEXT_FACE],
        "the appended positionless newline space must be skipped, not re-faced"
    );
}

#[test]
fn positionless_extend_stretch_at_row_end_is_skipped() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_char(&mut row, 'a', 0);
    push_char(&mut row, ' ', 1);
    push_positionless_stretch(&mut row);

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TEXT_FACE, TWS_FACE, TEXT_FACE],
        "a face-extension stretch (no buffer position) is skipped, not re-faced"
    );
}

#[test]
fn refacing_stops_at_glyph_without_buffer_position() {
    // Only buffer-position glyphs are re-faced: a positionless whitespace
    // glyph inside the trailing run stops the backward walk, exactly as GNU's
    // `BUFFERP (glyph->object)` condition does.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_char(&mut row, 'a', 0);
    push_positionless_space(&mut row);
    push_char(&mut row, ' ', 2);

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TEXT_FACE, TEXT_FACE, TWS_FACE],
        "the re-face run must stop at the first glyph without a buffer position"
    );
}

#[test]
fn row_of_only_positionless_glyphs_is_unchanged() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_positionless_space(&mut row);
    push_positionless_stretch(&mut row);

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TEXT_FACE, TEXT_FACE],
        "a row holding only appended glyphs has no buffer whitespace to re-face"
    );
}

#[test]
fn all_whitespace_row_is_fully_refaced() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_char(&mut row, ' ', 0);
    push_tab(&mut row);
    push_char(&mut row, ' ', 1);

    HighlightTrailingWhitespaceMutation { face_id: TWS_FACE }.apply(&mut row);

    assert_eq!(
        face_ids(&row),
        vec![TWS_FACE, TWS_FACE, TWS_FACE],
        "an all-whitespace row is entirely trailing whitespace"
    );
}
