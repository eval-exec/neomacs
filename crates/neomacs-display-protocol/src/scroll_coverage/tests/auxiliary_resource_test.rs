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

#[test]
fn scroll_surface_paint_preserves_sibling_char_fonts_shaped_clusters_and_base_bindings() {
    use crate::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont,
        ResolvedFontIdentity,
    };
    let mut state = base_state();
    let shared_face = crate::FaceId::new(0);
    let additive_face_id = crate::FaceId::new(77);
    let base_font_id = ResolvedFontId(41);
    let additive_font_id = ResolvedFontId(77);

    let make_font = |id: ResolvedFontId, family: &str| ResolvedFont {
        id,
        identity: ResolvedFontIdentity::from_file("/fixture/paint-merge.ttf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(
                FontFileAsset::new("/fixture/paint-merge.ttf", 0).unwrap(),
            ),
        },
        family: family.into(),
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

    let sibling_glyph = ResolvedCharGlyph {
        resolved_font_id: base_font_id,
        glyph_id: ResolvedGlyphId::new(9),
        advance_px: 8.0,
    };
    let cov_glyph = ResolvedCharGlyph {
        resolved_font_id: additive_font_id,
        glyph_id: ResolvedGlyphId::new(17),
        advance_px: 10.0,
    };
    let sibling_cluster = vec![ResolvedGlyph {
        resolved_font_id: base_font_id,
        glyph_id: ResolvedGlyphId::new(9),
        x: 0.0,
        y: 0.0,
        x_advance: 8.0,
        cluster_start: 0,
        cluster_end: 1,
    }];
    let cov_cluster = vec![ResolvedGlyph {
        resolved_font_id: additive_font_id,
        glyph_id: ResolvedGlyphId::new(17),
        x: 0.0,
        y: 0.0,
        x_advance: 10.0,
        cluster_start: 0,
        cluster_end: 1,
    }];

    // Populate coverage with its own char_fonts/shaped_clusters under shared_face
    // plus an additive face and font.
    {
        let cov = Arc::make_mut(&mut state.scroll_coverage[0]);
        let mut extra_face = crate::Face::new(additive_face_id);
        extra_face.default_resolved_font_id = Some(additive_font_id);
        cov.faces.insert(additive_face_id, extra_face);
        cov.fonts
            .insert(additive_font_id, make_font(additive_font_id, "Additive"));
        cov.char_fonts
            .entry(shared_face)
            .or_default()
            .insert('a', cov_glyph);
        cov.shaped_clusters
            .entry(shared_face)
            .or_default()
            .insert("cov".into(), cov_cluster.clone());
    }

    let surface = state
        .prepare_scroll_coverage(Arc::clone(&state.scroll_coverage[0]))
        .expect("prepare surface");

    // Materialize a base FrameGlyphBuffer that ALSO has sibling-window / chrome
    // char_fonts and shaped_clusters under the same shared_face.
    let mut current = state.materialize();
    current
        .fonts
        .insert(base_font_id, make_font(base_font_id, "Base"));
    current
        .char_fonts
        .entry(shared_face)
        .or_default()
        .insert('z', sibling_glyph);
    current
        .shaped_clusters
        .entry(shared_face)
        .or_default()
        .insert("sibling".into(), sibling_cluster.clone());

    surface
        .paint(&mut current, 0.0)
        .expect("paint scroll surface");

    // Sibling entries AND coverage entries under shared_face must BOTH survive!
    let merged_chars = current
        .char_fonts
        .get(&shared_face)
        .expect("shared face char_fonts");
    assert_eq!(
        merged_chars.get(&'z'),
        Some(&sibling_glyph),
        "paint must not clobber sibling char_fonts entries sharing a FaceId"
    );
    assert_eq!(
        merged_chars.get(&'a'),
        Some(&cov_glyph),
        "paint must merge coverage char_fonts entries"
    );

    let merged_clusters = current
        .shaped_clusters
        .get(&shared_face)
        .expect("shared face shaped_clusters");
    assert_eq!(
        merged_clusters.get("sibling"),
        Some(&sibling_cluster),
        "paint must not clobber sibling shaped_clusters entries sharing a FaceId"
    );
    assert_eq!(
        merged_clusters.get("cov"),
        Some(&cov_cluster),
        "paint must merge coverage shaped_clusters entries"
    );

    // Additive face and font are merged without clobbering base font.
    assert!(current.faces.contains_key(&additive_face_id));
    assert_eq!(
        current
            .fonts
            .get(&additive_font_id)
            .map(|f| f.family.as_str()),
        Some("Additive")
    );
    assert_eq!(
        current.fonts.get(&base_font_id).map(|f| f.family.as_str()),
        Some("Base")
    );
}
