use neomacs_display_protocol::glyph_matrix::{
    GlyphPointerAppearance, GlyphPointerOccurrenceIdentity, GlyphPointerSourceIdentity,
    GlyphPointerSourceKind,
};
use neomacs_display_protocol::{
    DisplayWindowId, FaceId, FrameRect, GlyphRowRole, PresentedPrimitiveKind,
};

use super::{PresentedPointerMapBuildError, PresentedPointerMapBuilder};

fn rect(x: f32, y: f32, width: f32, height: f32) -> FrameRect {
    FrameRect::new(x, y, width, height).unwrap()
}

fn appearance(source_id: u64, face: u32) -> GlyphPointerAppearance {
    GlyphPointerAppearance {
        source: GlyphPointerSourceIdentity {
            kind: GlyphPointerSourceKind::Buffer,
            source_id,
            range_start: 0,
            range_end: 100,
            property_owner: 0,
            occurrence: GlyphPointerOccurrenceIdentity::Source,
        },
        face_id: FaceId::new(face),
    }
}

#[test]
fn production_builder_coalesces_adjacent_regions_and_source_spans() {
    let mut builder = PresentedPointerMapBuilder::new();
    for (col, x) in [(0, 0.0), (2, 20.0)] {
        builder.observe_glyph_run(
            DisplayWindowId::new(1),
            GlyphRowRole::Text,
            0,
            col,
            2,
            PresentedPrimitiveKind::Glyph,
            rect(x, 0.0, 20.0, 10.0),
            appearance(7, 9),
        );
    }

    let map = builder.finish().unwrap();
    assert_eq!(map.regions().len(), 1);
    assert_eq!(map.regions()[0].bounds(), rect(0.0, 0.0, 40.0, 10.0));
    assert_eq!(map.appearances().len(), 1);
    assert_eq!(map.appearances()[0].paint_spans().len(), 1);
    assert_eq!(map.appearances()[0].paint_spans()[0].len(), 4);
}

#[test]
fn production_builder_keeps_wrapped_rows_as_two_spans_of_one_appearance() {
    let mut builder = PresentedPointerMapBuilder::new();
    for (row, y) in [(0, 0.0), (1, 10.0)] {
        builder.observe_glyph_run(
            DisplayWindowId::new(1),
            GlyphRowRole::Text,
            row,
            0,
            3,
            PresentedPrimitiveKind::Glyph,
            rect(0.0, y, 30.0, 10.0),
            appearance(7, 9),
        );
    }

    let map = builder.finish().unwrap();
    assert_eq!(map.regions().len(), 2);
    assert_eq!(map.appearances().len(), 1);
    assert_eq!(map.appearances()[0].paint_spans().len(), 2);
}

#[test]
fn production_builder_rejects_conflicting_modes_for_one_source() {
    let mut builder = PresentedPointerMapBuilder::new();
    builder.observe_glyph_run(
        DisplayWindowId::new(1),
        GlyphRowRole::Text,
        0,
        0,
        1,
        PresentedPrimitiveKind::Glyph,
        rect(0.0, 0.0, 10.0, 10.0),
        appearance(7, 9),
    );
    builder.observe_glyph_run(
        DisplayWindowId::new(1),
        GlyphRowRole::Text,
        0,
        0,
        1,
        PresentedPrimitiveKind::Glyph,
        rect(0.0, 0.0, 10.0, 10.0),
        appearance(7, 10),
    );

    assert_eq!(
        builder.finish(),
        Err(PresentedPointerMapBuildError::ConflictingAppearanceModes)
    );
}

#[test]
fn production_builder_keeps_mixed_face_batches_in_one_logical_appearance() {
    let mut builder = PresentedPointerMapBuilder::new();
    for (col, face) in [(0, 9), (1, 10)] {
        builder.observe_glyph_run(
            DisplayWindowId::new(1),
            GlyphRowRole::Text,
            0,
            col,
            1,
            PresentedPrimitiveKind::Glyph,
            rect(f32::from(col) * 10.0, 0.0, 10.0, 10.0),
            appearance(7, face),
        );
    }

    let map = builder.finish().unwrap();
    assert_eq!(map.regions().len(), 1);
    assert_eq!(map.appearances().len(), 1);
    let spans = map.appearances()[0].paint_spans();
    assert_eq!(spans.len(), 2);
    assert_eq!(
        spans[0].hover(),
        Some(neomacs_display_protocol::PointerDrawMode::Face(
            FaceId::new(9)
        ))
    );
    assert_eq!(
        spans[1].hover(),
        Some(neomacs_display_protocol::PointerDrawMode::Face(
            FaceId::new(10)
        ))
    );
}
