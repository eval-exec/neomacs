use super::*;
use crate::glyph_matrix::{Glyph, MatrixRow};
use crate::{DisplayWindowId, FaceId, FrameRect, PresentedTextPosition};

fn enabled_row(
    matrix: &mut GlyphMatrix,
    index: usize,
    y: f32,
) -> &mut crate::glyph_matrix::GlyphRow {
    let row = MatrixRow::make_mut(&mut matrix.rows[index]);
    row.enabled = true;
    row.pixel_y = y;
    row.height_px = 1.0;
    row
}

#[test]
fn stored_glyph_row_fallback_and_undrawn_are_distinct_from_physical_metrics() {
    let mut matrix = GlyphMatrix::new(3, 12);
    let row = enabled_row(&mut matrix, 1, 2.0);
    // Physical glyph height remains a layout fact. Terminal object extents
    // follow term.c::append_glyph, which writes neither ascent nor descent.
    let mut glyph = Glyph::char('x', FaceId::new(0), 7);
    glyph.pixel_width = 1.0;
    glyph.pixel_height = 19.0;
    glyph.pixel_ascent = 13.0;
    row.glyphs[GlyphArea::Text.index()].push(glyph);
    let retained = PosnMatrixSnapshot::from_terminal_matrix(&matrix, |_| 1);
    assert_eq!(
        retained.at(1, 0, GlyphArea::Text),
        PosnObjectExtent::Glyph {
            width: 1,
            height: 0
        }
    );
    assert_eq!(
        retained.at(1, 1, GlyphArea::Text),
        PosnObjectExtent::RowFallback { height: 1 }
    );
    for (row, col) in [(0, 0), (2, 0), (3, 0), (-1, 0), (i64::MAX, 0)] {
        assert_eq!(
            retained.at(row, col, GlyphArea::Text),
            PosnObjectExtent::Undrawn
        );
    }
    assert_eq!(
        matrix.rows[1].glyphs[GlyphArea::Text.index()][0].pixel_height,
        19.0
    );
    assert_eq!(matrix.rows[1].height_px, 1.0);
}

#[test]
fn materialized_cells_include_wide_tab_stretch_padding_and_nil_line_end() {
    let mut matrix = GlyphMatrix::new(1, 16);
    let row = enabled_row(&mut matrix, 0, 0.0);
    let face = FaceId::new(0);
    let mut wide = Glyph::char('界', face, 3);
    wide.wide = true;
    let mut padding = Glyph::char(' ', face, 3);
    padding.padding = true;
    // A zero-column stretch still owns one addressable terminal slot. The
    // nil-object cursor space is an actual glyph, independent of point role.
    let eol = Glyph::char_with_provenance(' ', face, crate::glyph_matrix::GlyphProvenance::LineEnd);
    row.glyphs[GlyphArea::Text.index()] = vec![
        Glyph::char('a', face, 1),
        wide,
        padding,
        Glyph::stretch(4, face),
        Glyph::stretch(0, face),
        eol,
    ];
    row.glyphs[GlyphArea::LeftMargin.index()] = vec![Glyph::char('L', face, 0)];
    row.glyphs[GlyphArea::RightMargin.index()] = vec![Glyph::stretch(2, face)];
    let retained = PosnMatrixSnapshot::from_terminal_matrix(&matrix, |_| 1);
    assert_eq!(retained.rows[0].areas, [1, 9, 2]);
    assert_eq!(retained.at(0, 8, GlyphArea::Text).dimensions(), (1, 0));
    assert_eq!(retained.at(0, 9, GlyphArea::Text).dimensions(), (0, 1));
    assert_eq!(
        retained.at(0, 1, GlyphArea::LeftMargin).dimensions(),
        (0, 1)
    );
    assert_eq!(
        retained.at(0, 1, GlyphArea::RightMargin).dimensions(),
        (1, 0)
    );
}

#[test]
fn matrix_y_is_window_local_and_reserves_tab_and_header_rows() {
    let mut matrix = GlyphMatrix::new(5, 16);
    let face = FaceId::new(0);
    for (index, y) in [(0, 0.0), (1, 1.0), (2, 2.0), (4, 4.0)] {
        let row = enabled_row(&mut matrix, index, y);
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char('x', face, index));
        row.glyphs[GlyphArea::LeftMargin.index()].push(Glyph::char('L', face, index));
    }
    let retained = PosnMatrixSnapshot::from_terminal_matrix(&matrix, |_| 1);
    // A lower split window may start at frame y=20. The captured row origins
    // have already been normalized by from_window_absolute_row; never add20.
    assert_eq!(
        retained.at_y(2, 0, GlyphArea::LeftMargin).dimensions(),
        (1, 0)
    );
    assert_eq!(
        retained.at_y(22, 0, GlyphArea::LeftMargin),
        PosnObjectExtent::Undrawn
    );
    assert_eq!(
        retained.at_y(3, 0, GlyphArea::LeftMargin),
        PosnObjectExtent::Undrawn
    );
    // GNU's marginal lookup stops at the first disabled row, even if a later
    // enabled chrome row covers Y. Direct chrome lookup still uses its index.
    assert_eq!(
        retained.at_y(4, 0, GlyphArea::LeftMargin),
        PosnObjectExtent::Undrawn
    );
    assert_eq!(retained.at(2, 0, GlyphArea::Text).dimensions(), (1, 0));
    assert_eq!(retained.at(4, 1, GlyphArea::Text).dimensions(), (0, 1));
}

#[test]
fn retained_matrix_facts_are_independent_of_later_copy_on_write_edits() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<PosnMatrixSnapshot>();
    let mut matrix = GlyphMatrix::new(1, 4);
    enabled_row(&mut matrix, 0, 0.0).glyphs[GlyphArea::Text.index()].push(Glyph::char(
        'x',
        FaceId::new(0),
        1,
    ));
    let old_matrix = matrix.clone();
    let old = std::sync::Arc::new(PosnMatrixSnapshot::from_terminal_matrix(&old_matrix, |_| 1));
    MatrixRow::make_mut(&mut matrix.rows[0]).enabled = false;
    let new = PosnMatrixSnapshot::from_terminal_matrix(&matrix, |_| 1);
    assert_eq!(new.at(0, 0, GlyphArea::Text), PosnObjectExtent::Undrawn);
    let reader = std::sync::Arc::clone(&old);
    assert_eq!(
        std::thread::spawn(move || reader.at(0, 0, GlyphArea::Text).dimensions())
            .join()
            .unwrap(),
        (1, 0)
    );
}

#[test]
fn point_roles_transport_without_changing_physical_hit_bounds_or_legacy_json() {
    let bounds = FrameRect::new(5.0, 8.0, 2.0, 1.0).unwrap();
    let base = PresentedTextPosition::new(DisplayWindowId::new(3), bounds, 7, 0, 4);
    let legacy_json = serde_json::to_string(&base).unwrap();
    assert!(!legacy_json.contains("point_role"));
    let legacy: PresentedTextPosition = serde_json::from_str(&legacy_json).unwrap();
    assert_eq!(legacy.point_role(), PosnPointRole::Glyph);
    for role in [
        PosnPointRole::Glyph,
        PosnPointRole::OverlaidMarker,
        PosnPointRole::InsertionBoundary,
        PosnPointRole::SyntheticBoundary,
    ] {
        let point = base.with_point_role(role);
        let restored: PresentedTextPosition =
            serde_json::from_str(&serde_json::to_string(&point).unwrap()).unwrap();
        assert_eq!(restored, point);
        assert_eq!(restored.bounds(), bounds);
        assert_eq!(restored.buffer_position(), 7);
    }
}

#[test]
fn terminal_used_cells_do_not_count_bare_zero_width_or_duplicate_composition_owners() {
    use crate::glyph_matrix::{GlyphType, TerminalComposition, TerminalCompositionCell};
    let face = FaceId::new(0);
    let mut composite = Glyph::char('a', face, 1);
    composite.glyph_type = GlyphType::Composite { text: "abc".into() };
    let mut b = Glyph::char('b', face, 2);
    b.padding = true;
    let mut c = Glyph::char('c', face, 3);
    c.padding = true;
    let mut automatic = Glyph::char('e', face, 4);
    automatic.glyph_type = GlyphType::AutomaticComposite {
        text: "e\u{301}".into(),
        terminal: std::sync::Arc::new(TerminalComposition {
            cells: vec![TerminalCompositionCell {
                base: 'e',
                extenders: "\u{301}".into(),
                width_cols: 1,
                source_char_len: 2,
            }]
            .into_boxed_slice(),
            width_cols: 1,
        }),
    };
    let mut extender = Glyph::char('\u{301}', face, 5);
    extender.padding = true;
    let glyphs = vec![
        Glyph::char('\u{301}', face, 0),
        composite,
        b,
        c,
        automatic,
        extender,
        Glyph::stretch(0, face),
    ];
    assert_eq!(
        terminal_area_used_cells(&glyphs, |ch| if ch == '\u{301}' { 0 } else { 1 }),
        5
    );
    // No terminal cell is emitted for the bare extender; three contextual
    // cells, one automatic cell and one minimum stretch remain.
}
