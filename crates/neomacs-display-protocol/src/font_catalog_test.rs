use crate::font::FontCatalogGeneration;

#[test]
fn font_catalog_generation_never_uses_the_uninitialized_wire_value() {
    let initial = FontCatalogGeneration::default();
    assert_ne!(initial.get(), 0);
    assert_ne!(initial.next().get(), 0);
}

#[test]
fn font_catalog_generation_advances_and_wraps_to_initial() {
    let initial = FontCatalogGeneration::default();
    assert_eq!(initial.next().get(), initial.get() + 1);
    assert_eq!(FontCatalogGeneration::from_raw(u64::MAX).next(), initial);
}

#[test]
fn frame_materialization_preserves_font_catalog_generation() {
    let mut state = crate::FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.font_catalog_generation = FontCatalogGeneration::default().next();

    let materialized = state.materialize();
    assert_eq!(
        materialized.font_catalog_generation,
        state.font_catalog_generation
    );
    assert_eq!(
        crate::FrameDisplayState::from_frame_glyph_buffer(&materialized).font_catalog_generation,
        state.font_catalog_generation
    );
}

#[test]
fn mini_frame_clones_the_complete_font_binding_snapshot() {
    use crate::font::{ResolvedCharGlyph, ResolvedFontId, ResolvedGlyphId};
    use crate::types::FaceId;

    let mut source = crate::FrameGlyphBuffer::default();
    source.font_catalog_generation = FontCatalogGeneration::default().next();
    source.char_fonts.entry(FaceId::new(7)).or_default().insert(
        'x',
        ResolvedCharGlyph {
            resolved_font_id: ResolvedFontId(11),
            glyph_id: ResolvedGlyphId::new(13),
            advance_px: 5.0,
        },
    );

    let mut mini = crate::FrameGlyphBuffer::default();
    mini.clone_font_bindings_from(&source);

    assert_eq!(mini.font_catalog_generation, source.font_catalog_generation);
    assert_eq!(mini.faces, source.faces);
    assert_eq!(mini.fonts, source.fonts);
    assert_eq!(mini.char_fonts, source.char_fonts);
    assert_eq!(mini.shaped_clusters, source.shaped_clusters);
}
