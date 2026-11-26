use super::*;
use crate::font::{
    FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedCharGlyph, ResolvedFont,
    ResolvedFontId, ResolvedFontIdentity, ResolvedGlyph, ResolvedGlyphId,
};

fn base_state() -> FrameDisplayState {
    let mut state = super::prepared_surface_tests::frame();
    state.faces = state.scroll_coverage[0].faces.clone();
    state
}

fn fixture_font(id: ResolvedFontId, family: &str) -> ResolvedFont {
    let asset = FontFileAsset::new("/fixture/coverage-font.ttf", 0).unwrap();
    ResolvedFont {
        id,
        identity: ResolvedFontIdentity::from_file("/fixture/coverage-font.ttf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(asset),
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
    }
}

#[test]
fn auxiliary_nested_bindings_accept_additive_entries_and_refuse_conflicts() {
    let mut state = base_state();
    let mut coverage = state.scroll_coverage[0].as_ref().clone();
    let face = crate::FaceId::new(0);
    let font_id = ResolvedFontId(91);
    let glyph = ResolvedCharGlyph {
        resolved_font_id: font_id,
        glyph_id: ResolvedGlyphId::new(17),
        advance_px: 10.0,
    };
    let cluster = vec![ResolvedGlyph {
        resolved_font_id: font_id,
        glyph_id: glyph.glyph_id,
        x: 0.0,
        y: 0.0,
        x_advance: 10.0,
        cluster_start: 0,
        cluster_end: 1,
    }];
    coverage
        .fonts
        .insert(font_id, fixture_font(font_id, "Coverage fixture"));
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

    // Base lacks 'a' / "a" / font_id (additive in Aux v2), and has sibling 'b' / "b".
    state.char_fonts.entry(face).or_default().insert('b', glyph);
    state
        .shaped_clusters
        .entry(face)
        .or_default()
        .insert("b".into(), cluster.clone());
    assert!(coverage.resources_self_closed());
    assert!(coverage.resources_compatible_with_frame_state(&state));
    assert!(coverage.resources_compatible_with_frame_buffer(&state.materialize()));

    // Matching entries on base also remain compatible.
    state
        .fonts
        .insert(font_id, fixture_font(font_id, "Coverage fixture"));
    state.char_fonts.entry(face).or_default().insert('a', glyph);
    state
        .shaped_clusters
        .entry(face)
        .or_default()
        .insert("a".into(), cluster.clone());
    assert!(coverage.resources_compatible_with_frame_state(&state));
    assert!(coverage.resources_compatible_with_frame_buffer(&state.materialize()));

    // Genuine char_fonts conflict on the same (FaceId, char) must be refused.
    state
        .char_fonts
        .get_mut(&face)
        .unwrap()
        .get_mut(&'a')
        .unwrap()
        .advance_px += 1.0;
    assert!(!coverage.resources_compatible_with_frame_state(&state));
    assert!(!coverage.resources_compatible_with_frame_buffer(&state.materialize()));

    // Genuine shaped_clusters conflict on the same (FaceId, text) must be refused.
    state.char_fonts.get_mut(&face).unwrap().insert('a', glyph);
    state
        .shaped_clusters
        .get_mut(&face)
        .unwrap()
        .get_mut("a")
        .unwrap()[0]
        .x += 1.0;
    assert!(!coverage.resources_compatible_with_frame_state(&state));
    assert!(!coverage.resources_compatible_with_frame_buffer(&state.materialize()));
}

#[test]
fn auxiliary_faces_accept_self_closed_additions_and_refuse_conflicts() {
    let mut state = base_state();
    let mut coverage = state.scroll_coverage[0].as_ref().clone();

    // Extra base faces are allowed.
    state.faces.insert(
        crate::FaceId::new(90),
        crate::Face::new(crate::FaceId::new(90)),
    );
    assert!(coverage.resources_compatible_with_frame_state(&state));
    assert!(coverage.resources_compatible_with_frame_buffer(&state.materialize()));

    // Additive self-closed offscreen face absent from base is allowed in Aux v2.
    let additive_face_id = crate::FaceId::new(95);
    let additive_font_id = ResolvedFontId(95);
    let mut additive_face = crate::Face::new(additive_face_id);
    additive_face.foreground = crate::Color::GREEN;
    additive_face.default_resolved_font_id = Some(additive_font_id);
    coverage.faces.insert(additive_face_id, additive_face);
    coverage.fonts.insert(
        additive_font_id,
        fixture_font(additive_font_id, "Additive offscreen"),
    );
    assert!(coverage.resources_compatible_with_frame_state(&state));
    assert!(coverage.resources_compatible_with_frame_buffer(&state.materialize()));

    // Conflicting face binding for an existing base FaceId must be refused.
    let own_face = crate::FaceId::new(0);
    state.faces.get_mut(&own_face).unwrap().background = crate::Color::RED;
    assert!(!coverage.resources_compatible_with_frame_state(&state));
    assert!(!coverage.resources_compatible_with_frame_buffer(&state.materialize()));
}

#[test]
fn auxiliary_self_closure_refuses_every_dangling_face_or_font_reference() {
    let state = base_state();
    let face0 = crate::FaceId::new(0);
    let missing_face = crate::FaceId::new(404);
    let font_id = ResolvedFontId(91);
    let missing_font = ResolvedFontId(404);
    let glyph_with = |id| ResolvedCharGlyph {
        resolved_font_id: id,
        glyph_id: ResolvedGlyphId::new(17),
        advance_px: 10.0,
    };
    let cluster_with = |id| {
        vec![ResolvedGlyph {
            resolved_font_id: id,
            glyph_id: ResolvedGlyphId::new(17),
            x: 0.0,
            y: 0.0,
            x_advance: 10.0,
            cluster_start: 0,
            cluster_end: 1,
        }]
    };

    // 1. Matrix row referencing a FaceId absent from coverage.faces.
    let mut dangling_matrix_face = state.scroll_coverage[0].as_ref().clone();
    dangling_matrix_face.faces.remove(&face0);
    assert!(!dangling_matrix_face.resources_self_closed());
    assert!(!dangling_matrix_face.resources_compatible_with_frame_state(&state));
    assert!(!dangling_matrix_face.resources_compatible_with_frame_buffer(&state.materialize()));

    // 2. char_fonts referencing a FaceId absent from coverage.faces.
    let mut dangling_char_face = state.scroll_coverage[0].as_ref().clone();
    dangling_char_face
        .fonts
        .insert(font_id, fixture_font(font_id, "F"));
    dangling_char_face
        .char_fonts
        .entry(missing_face)
        .or_default()
        .insert('x', glyph_with(font_id));
    assert!(!dangling_char_face.resources_self_closed());
    assert!(!dangling_char_face.resources_compatible_with_frame_state(&state));
    assert!(!dangling_char_face.resources_compatible_with_frame_buffer(&state.materialize()));

    // 3. shaped_clusters referencing a FaceId absent from coverage.faces.
    let mut dangling_cluster_face = state.scroll_coverage[0].as_ref().clone();
    dangling_cluster_face
        .fonts
        .insert(font_id, fixture_font(font_id, "F"));
    dangling_cluster_face
        .shaped_clusters
        .entry(missing_face)
        .or_default()
        .insert("xy".into(), cluster_with(font_id));
    assert!(!dangling_cluster_face.resources_self_closed());
    assert!(!dangling_cluster_face.resources_compatible_with_frame_state(&state));
    assert!(!dangling_cluster_face.resources_compatible_with_frame_buffer(&state.materialize()));

    // 4. Face::default_resolved_font_id absent from coverage.fonts.
    let mut dangling_face_font = state.scroll_coverage[0].as_ref().clone();
    dangling_face_font
        .faces
        .get_mut(&face0)
        .unwrap()
        .default_resolved_font_id = Some(missing_font);
    assert!(!dangling_face_font.resources_self_closed());
    assert!(!dangling_face_font.resources_compatible_with_frame_state(&state));
    assert!(!dangling_face_font.resources_compatible_with_frame_buffer(&state.materialize()));

    // 5. char_fonts entry referencing a ResolvedFontId absent from coverage.fonts.
    let mut dangling_char_font = state.scroll_coverage[0].as_ref().clone();
    dangling_char_font
        .char_fonts
        .entry(face0)
        .or_default()
        .insert('x', glyph_with(missing_font));
    assert!(!dangling_char_font.resources_self_closed());
    assert!(!dangling_char_font.resources_compatible_with_frame_state(&state));
    assert!(!dangling_char_font.resources_compatible_with_frame_buffer(&state.materialize()));

    // 6. shaped_clusters entry referencing a ResolvedFontId absent from coverage.fonts.
    let mut dangling_cluster_font = state.scroll_coverage[0].as_ref().clone();
    dangling_cluster_font
        .shaped_clusters
        .entry(face0)
        .or_default()
        .insert("xy".into(), cluster_with(missing_font));
    assert!(!dangling_cluster_font.resources_self_closed());
    assert!(!dangling_cluster_font.resources_compatible_with_frame_state(&state));
    assert!(!dangling_cluster_font.resources_compatible_with_frame_buffer(&state.materialize()));
}

#[test]
fn auxiliary_self_closure_refuses_mismatched_face_and_font_map_keys() {
    let mut state = base_state();
    let base_font_id = ResolvedFontId(1);
    let aliased_font_key = ResolvedFontId(99);
    let additive_face_key = crate::FaceId::new(77);
    let base_face_id = crate::FaceId::new(0);

    state
        .fonts
        .insert(base_font_id, fixture_font(base_font_id, "DejaVu Sans Mono"));
    state
        .faces
        .get_mut(&base_face_id)
        .unwrap()
        .default_resolved_font_id = Some(base_font_id);

    // 1. Mismatched font key/id: fonts[99] carries embedded ResolvedFont::id == 1.
    // Neither key 77 nor key 99 collides with base's map keys, so map-key conflict
    // detection alone would accept this and GlyphAtlas::install_frame_fonts would
    // index by font.id (1), flushing the atlas and overwriting base font 1.
    let mut mismatched_font = state.scroll_coverage[0].as_ref().clone();
    mismatched_font
        .fonts
        .insert(base_font_id, fixture_font(base_font_id, "DejaVu Sans Mono"));
    mismatched_font
        .faces
        .get_mut(&base_face_id)
        .unwrap()
        .default_resolved_font_id = Some(base_font_id);
    mismatched_font.fonts.insert(
        aliased_font_key,
        fixture_font(base_font_id, "WRONG STALE FONT"),
    );
    let mut additive_face = crate::Face::new(additive_face_key);
    additive_face.default_resolved_font_id = Some(aliased_font_key);
    mismatched_font
        .faces
        .insert(additive_face_key, additive_face);
    assert!(!mismatched_font.resources_self_closed());
    assert!(!mismatched_font.resources_compatible_with_frame_state(&state));
    assert!(!mismatched_font.resources_compatible_with_frame_buffer(&state.materialize()));

    // 2. Mismatched face key/id: faces[77] carries embedded Face::id == 0 while
    // every font entry has matching key == id. Renderer char/cluster tables index
    // by face.id (0), which would alias base face 0 onto additive face 77.
    let mut mismatched_face = state.scroll_coverage[0].as_ref().clone();
    mismatched_face
        .fonts
        .insert(base_font_id, fixture_font(base_font_id, "DejaVu Sans Mono"));
    mismatched_face
        .faces
        .get_mut(&base_face_id)
        .unwrap()
        .default_resolved_font_id = Some(base_font_id);
    mismatched_face.fonts.insert(
        aliased_font_key,
        fixture_font(aliased_font_key, "Valid additive font"),
    );
    let mut aliased_face = crate::Face::new(base_face_id);
    aliased_face.default_resolved_font_id = Some(aliased_font_key);
    mismatched_face
        .faces
        .insert(additive_face_key, aliased_face);
    assert!(!mismatched_face.resources_self_closed());
    assert!(!mismatched_face.resources_compatible_with_frame_state(&state));
    assert!(!mismatched_face.resources_compatible_with_frame_buffer(&state.materialize()));
}

#[test]
fn auxiliary_realized_gui_frame_refuses_additive_face_without_resolved_font_and_accepts_resolved_or_tty()
 {
    let base_face_id = crate::FaceId::new(0);
    let additive_face_id = crate::FaceId::new(77);
    let base_font_id = ResolvedFontId(1);
    let additive_font_id = ResolvedFontId(95);

    // Realized GUI base (`!fonts.is_empty()`): layout's realize_frame_fonts sets
    // `default_resolved_font_id = Some(...)` on every GUI face.
    let mut gui_state = base_state();
    gui_state
        .fonts
        .insert(base_font_id, fixture_font(base_font_id, "DejaVu Sans Mono"));
    gui_state
        .faces
        .get_mut(&base_face_id)
        .unwrap()
        .default_resolved_font_id = Some(base_font_id);

    // 1. Negative GUI direction: additive face absent from base with
    // `default_resolved_font_id: None` must be refused so the render thread
    // cannot fall back to `record_emergency_font_fallback("layout published no font identity")`.
    let mut unresolved_additive = gui_state.scroll_coverage[0].as_ref().clone();
    unresolved_additive
        .fonts
        .insert(base_font_id, fixture_font(base_font_id, "DejaVu Sans Mono"));
    unresolved_additive
        .faces
        .get_mut(&base_face_id)
        .unwrap()
        .default_resolved_font_id = Some(base_font_id);
    unresolved_additive
        .faces
        .insert(additive_face_id, crate::Face::new(additive_face_id));
    assert!(unresolved_additive.resources_self_closed());
    assert!(!unresolved_additive.resources_compatible_with_frame_state(&gui_state));
    assert!(!unresolved_additive.resources_compatible_with_frame_buffer(&gui_state.materialize()));

    // 2. Positive GUI direction: realized GUI base + additive face carrying
    // `default_resolved_font_id: Some(id)` backed by `coverage.fonts` is accepted.
    let mut resolved_additive = unresolved_additive.clone();
    resolved_additive.fonts.insert(
        additive_font_id,
        fixture_font(additive_font_id, "Additive GUI font"),
    );
    resolved_additive
        .faces
        .get_mut(&additive_face_id)
        .unwrap()
        .default_resolved_font_id = Some(additive_font_id);
    assert!(resolved_additive.resources_self_closed());
    assert!(resolved_additive.resources_compatible_with_frame_state(&gui_state));
    assert!(resolved_additive.resources_compatible_with_frame_buffer(&gui_state.materialize()));

    // 3. Positive TTY/headless direction: both base.fonts and coverage.fonts are empty,
    // so an additive face with `default_resolved_font_id: None` remains valid.
    let tty_state = base_state();
    assert!(tty_state.fonts.is_empty());
    let mut tty_additive = tty_state.scroll_coverage[0].as_ref().clone();
    assert!(tty_additive.fonts.is_empty());
    tty_additive
        .faces
        .insert(additive_face_id, crate::Face::new(additive_face_id));
    assert!(tty_additive.resources_self_closed());
    assert!(tty_additive.resources_compatible_with_frame_state(&tty_state));
    assert!(tty_additive.resources_compatible_with_frame_buffer(&tty_state.materialize()));
}

#[test]
fn auxiliary_asymmetric_realized_fonts_refuses_unresolved_additive_face() {
    // Asymmetric escape (e.g. empty scalable catalog + fixed bitmap font in coverage, or a
    // deserialized coverage carrying realized fonts against an empty-fonts base):
    // `base.fonts.is_empty() && !coverage.fonts.is_empty()` must still refuse an additive face
    // whose `default_resolved_font_id` is `None`.
    let state = base_state();
    assert!(state.fonts.is_empty());
    let resolved_face_id = crate::FaceId::new(77);
    let unresolved_face_id = crate::FaceId::new(78);
    let bitmap_font_id = ResolvedFontId(95);

    let mut coverage = state.scroll_coverage[0].as_ref().clone();
    coverage
        .fonts
        .insert(bitmap_font_id, fixture_font(bitmap_font_id, "Spleen"));
    let mut resolved_face = crate::Face::new(resolved_face_id);
    resolved_face.default_resolved_font_id = Some(bitmap_font_id);
    coverage.faces.insert(resolved_face_id, resolved_face);
    coverage
        .faces
        .insert(unresolved_face_id, crate::Face::new(unresolved_face_id));

    assert!(coverage.resources_self_closed());
    assert!(!coverage.resources_compatible_with_frame_state(&state));
    assert!(!coverage.resources_compatible_with_frame_buffer(&state.materialize()));
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
fn auxiliary_font_table_accepts_additive_font_and_refuses_conflicting_binding() {
    let mut state = base_state();
    let mut coverage = state.scroll_coverage[0].as_ref().clone();
    let id = ResolvedFontId(91);
    let font = fixture_font(id, "Coverage fixture");
    coverage.fonts.insert(id, font.clone());
    // Additive font absent from base is compatible when self-closed.
    assert!(coverage.resources_compatible_with_frame_state(&state));
    assert!(coverage.resources_compatible_with_frame_buffer(&state.materialize()));
    // Matching binding on base is compatible.
    state.fonts.insert(id, font);
    assert!(coverage.resources_compatible_with_frame_state(&state));
    assert!(coverage.resources_compatible_with_frame_buffer(&state.materialize()));
    // Conflicting binding for the same ResolvedFontId is refused.
    state.fonts.get_mut(&id).unwrap().pixel_size += 1.0;
    assert!(!coverage.resources_compatible_with_frame_state(&state));
    assert!(!coverage.resources_compatible_with_frame_buffer(&state.materialize()));
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
