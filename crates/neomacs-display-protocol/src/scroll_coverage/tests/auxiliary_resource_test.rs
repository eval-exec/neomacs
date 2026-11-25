use super::*;
use crate::font::{ResolvedCharGlyph, ResolvedFontId, ResolvedGlyph, ResolvedGlyphId};

fn base_state() -> FrameDisplayState {
    let mut state = super::prepared_surface_tests::frame();
    state.faces = state.scroll_coverage[0].faces.clone();
    state
}

#[test]
fn auxiliary_nested_bindings_require_each_exact_entry() {
    let mut state = base_state();
    let mut coverage = state.scroll_coverage[0].as_ref().clone();
    let face = crate::FaceId::new(0);
    let glyph = ResolvedCharGlyph {
        resolved_font_id: ResolvedFontId(91),
        glyph_id: ResolvedGlyphId::new(17),
        advance_px: 10.0,
    };
    let cluster = vec![ResolvedGlyph {
        resolved_font_id: glyph.resolved_font_id,
        glyph_id: glyph.glyph_id,
        x: 0.0,
        y: 0.0,
        x_advance: 10.0,
        cluster_start: 0,
        cluster_end: 1,
    }];
    coverage
        .char_fonts
        .entry(face)
        .or_default()
        .insert('a', glyph);
    coverage
        .shaped_clusters
        .entry(face)
        .or_default()
        .insert("a".into(), cluster.clone());
    state.char_fonts = coverage.char_fonts.clone();
    state.char_fonts.get_mut(&face).unwrap().insert('b', glyph);
    state.shaped_clusters = coverage.shaped_clusters.clone();
    state
        .shaped_clusters
        .get_mut(&face)
        .unwrap()
        .insert("b".into(), cluster);
    assert!(coverage.resource_subset_of_frame_state(&state));
    assert!(coverage.resource_subset_of_frame_buffer(&state.materialize()));
    state
        .char_fonts
        .get_mut(&face)
        .unwrap()
        .get_mut(&'a')
        .unwrap()
        .advance_px += 1.0;
    assert!(!coverage.resource_subset_of_frame_state(&state));
    state.char_fonts.get_mut(&face).unwrap().insert('a', glyph);
    state
        .shaped_clusters
        .get_mut(&face)
        .unwrap()
        .get_mut("a")
        .unwrap()[0]
        .x += 1.0;
    assert!(!coverage.resource_subset_of_frame_state(&state));
    state.shaped_clusters = coverage.shaped_clusters.clone();
    state.char_fonts.get_mut(&face).unwrap().remove(&'a');
    assert!(!coverage.resource_subset_of_frame_state(&state));
}

#[test]
fn auxiliary_faces_are_exact_subsets_and_extra_base_faces_are_allowed() {
    let mut state = base_state();
    let coverage = state.scroll_coverage[0].as_ref().clone();
    state.faces.insert(
        crate::FaceId::new(90),
        crate::Face::new(crate::FaceId::new(90)),
    );
    assert!(coverage.resource_subset_of_frame_state(&state));
    let own_face = *coverage.faces.keys().next().unwrap();
    state.faces.get_mut(&own_face).unwrap().background = crate::Color::RED;
    assert!(!coverage.resource_subset_of_frame_state(&state));
    state.faces.remove(&own_face);
    assert!(!coverage.resource_subset_of_frame_state(&state));
}

#[test]
fn auxiliary_admission_checks_catalog_metrics_materialized_bars_and_bitmaps() {
    let mut state = base_state();
    let surface = state
        .prepare_scroll_coverage(Arc::clone(&state.scroll_coverage[0]))
        .unwrap();
    let mut current = state.materialize();
    assert!(surface.frame_resources_match(&state));
    assert!(surface.frame_buffer_resources_match(&current));
    state.font_catalog_generation = state.font_catalog_generation.next();
    current.font_catalog_generation = state.font_catalog_generation;
    assert!(!surface.frame_resources_match(&state));
    assert!(!surface.frame_buffer_resources_match(&current));
    current = base_state().materialize();
    assert!(surface.frame_buffer_resources_match(&current));
    let bar = current
        .glyphs
        .iter_mut()
        .find(|glyph| matches!(glyph, FrameGlyph::ScrollBar { .. }))
        .unwrap();
    if let FrameGlyph::ScrollBar { x, .. } = bar {
        *x += 1.0;
    }
    assert!(!surface.frame_buffer_resources_match(&current));
    current = base_state().materialize();
    current.fringe_bitmaps.get_mut(&25).unwrap().bits[0] = 0;
    assert!(!surface.frame_buffer_resources_match(&current));
    current = base_state().materialize();
    current.char_width += 1.0;
    assert!(!surface.frame_buffer_resources_match(&current));
}

#[test]
fn auxiliary_font_table_requires_the_exact_resolved_binding() {
    use crate::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont,
        ResolvedFontIdentity,
    };
    let mut state = base_state();
    let mut coverage = state.scroll_coverage[0].as_ref().clone();
    let id = ResolvedFontId(91);
    let asset = FontFileAsset::new("/fixture/coverage-font.ttf", 0).unwrap();
    let font = ResolvedFont {
        id,
        identity: ResolvedFontIdentity::from_file("/fixture/coverage-font.ttf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(asset),
        },
        family: "Coverage fixture".into(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 10.0,
        ascent_px: 8.0,
        descent_px: 2.0,
        space_advance_px: 10.0,
        glyph_advance: Default::default(),
        spacing: None,
    };
    coverage.fonts.insert(id, font.clone());
    assert!(!coverage.resource_subset_of_frame_state(&state));
    state.fonts.insert(id, font);
    assert!(coverage.resource_subset_of_frame_state(&state));
    assert!(coverage.resource_subset_of_frame_buffer(&state.materialize()));
    state.fonts.get_mut(&id).unwrap().pixel_size += 1.0;
    assert!(!coverage.resource_subset_of_frame_state(&state));
    assert!(!coverage.resource_subset_of_frame_buffer(&state.materialize()));
}
