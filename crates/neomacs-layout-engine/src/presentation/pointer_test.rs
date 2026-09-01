use super::*;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphMatrix, GlyphPointerAppearance, GlyphPointerOccurrenceIdentity,
    GlyphPointerSourceIdentity, GlyphPointerSourceKind, GlyphRow, GlyphType, WindowMatrixEntry,
};
use neomacs_display_protocol::{
    DisplayWindowId, FaceId, ImageMargins, ImageOpaqueBackground, ImageSourceRect,
    PresentedPrimitiveKind, PresentedRegionKind, Rect,
};

fn pointer() -> GlyphPointerAppearance {
    GlyphPointerAppearance {
        source: GlyphPointerSourceIdentity {
            kind: GlyphPointerSourceKind::Buffer,
            source_id: 7,
            range_start: 0,
            range_end: 1,
            property_owner: 0,
            occurrence: GlyphPointerOccurrenceIdentity::Source,
        },
        face_id: FaceId::new(9),
    }
}

#[test]
fn pointer_projection_uses_authoritative_row_origin_and_start_column() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.pixel_x = 5.0;
    row.start_col = 3;
    row.height_px = 16.0;
    let token = row.intern_pointer_appearance(pointer()).unwrap();
    row.glyphs[GlyphArea::Text.index()].push(Glyph {
        pointer_appearance: Some(token),
        ..Glyph::char('x', FaceId::new(0), 0).with_pixel_width(8.0)
    });
    row.rebuild_pointer_runs(8.0, 80.0);
    let mut matrix = GlyphMatrix::new(1, 10);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(4),
        matrix,
        pixel_bounds: Rect::new(10.0, 20.0, 80.0, 16.0),
        text_pixel_bounds: Rect::new(10.0, 20.0, 80.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let source = window_pointer_source_map(&state).unwrap();

    assert_eq!(source.regions()[0].bounds().x(), 15.0);
    assert_eq!(source.appearances()[0].paint_spans()[0].slot().col, 3);
}

#[test]
fn pointer_projection_supports_window_chrome_rows_from_the_same_matrix() {
    let mut row = GlyphRow::new(GlyphRowRole::HeaderLine);
    row.pixel_y = 3.0;
    row.height_px = 12.0;
    let token = row.intern_pointer_appearance(pointer()).unwrap();
    row.glyphs[GlyphArea::Text.index()].push(Glyph {
        pointer_appearance: Some(token),
        ..Glyph::char('h', FaceId::new(0), 0).with_pixel_width(8.0)
    });
    row.rebuild_pointer_runs(8.0, 80.0);
    let mut matrix = GlyphMatrix::new(1, 10);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(4),
        matrix,
        pixel_bounds: Rect::new(10.0, 20.0, 80.0, 32.0),
        text_pixel_bounds: Rect::new(10.0, 32.0, 80.0, 20.0),
        text_clip_bounds: None,
        selected: true,
    });

    let source = window_pointer_source_map(&state).unwrap();

    let region = &source.regions()[0];
    assert_eq!(
        region.owner().unwrap().kind(),
        PresentedRegionKind::HeaderLine
    );
    assert_eq!(region.bounds().y(), 23.0);
    assert_eq!(region.bounds().height(), 12.0);
    assert_eq!(
        source.appearances()[0].paint_spans()[0].row_role(),
        GlyphRowRole::HeaderLine
    );
}

#[test]
fn pointer_projection_preserves_image_primitive_kind_in_window_chrome() {
    let mut row = GlyphRow::new(GlyphRowRole::TabLine);
    row.height_px = 16.0;
    let token = row.intern_pointer_appearance(pointer()).unwrap();
    let image_margins = row
        .intern_image_margins(ImageMargins::default())
        .expect("image-margin token");
    let mut image = Glyph::char(' ', FaceId::new(0), 0).with_pixel_width(16.0);
    image.glyph_type = GlyphType::Image {
        image_id: 7,
        width_cols: 2,
        source_rect: ImageSourceRect::FULL,
        margins: image_margins,
        opaque_background: ImageOpaqueBackground::default(),
    };
    image.pointer_appearance = Some(token);
    row.glyphs[GlyphArea::Text.index()].push(image);
    row.rebuild_pointer_runs(8.0, 80.0);

    let mut matrix = GlyphMatrix::new(1, 10);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(4),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 16.0, 80.0, 0.0),
        text_clip_bounds: None,
        selected: true,
    });

    let source = window_pointer_source_map(&state).unwrap();

    assert_eq!(
        source.appearances()[0].paint_spans()[0].kind(),
        PresentedPrimitiveKind::Image
    );
}
