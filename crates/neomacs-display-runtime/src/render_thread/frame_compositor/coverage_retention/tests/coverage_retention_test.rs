use super::*;
use neomacs_display_protocol::scroll_coverage::{ScrollCoverage, ScrollCoverageRenewal};
use neomacs_display_protocol::*;

fn surface_frame(presentation: u64, window: u64, epoch: u64) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(100.0, 100.0);
    frame.presentation_id = PresentationId::new(presentation);
    frame.char_width = 10.0;
    frame.char_height = 10.0;
    frame.scroll_surfaces = vec![surface(window, epoch, presentation)];
    frame
}

fn surface(window: u64, epoch: u64, presentation: u64) -> Arc<ScrollSurface> {
    let mut state = FrameDisplayState::new(10, 2, 10.0, 10.0);
    state.presentation_id = PresentationId::new(presentation);
    let window = DisplayWindowId::new(window as i64);
    let viewport = Rect::new(0.0, 0.0, 100.0, 20.0);
    let bounds = Rect::new(0.0, 0.0, 100.0, 60.0);
    let mut matrix = GlyphMatrix::new(6, 10);
    let mut positions = Vec::new();
    for index in 0..6 {
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        row.pixel_y = index as f32 * 10.0;
        row.height_px = 10.0;
        row.ascent_px = 8.0;
        row.start_charpos = index * 2;
        row.end_charpos = index * 2 + 1;
        let mut glyph = Glyph::char(char::from(b'a' + index as u8), FaceId::new(0), index * 2);
        glyph.pixel_width = 10.0;
        row.glyphs[1].push(glyph);
        positions.push(PresentedTextPosition::new(
            window,
            FrameRect::new(0.0, row.pixel_y, 100.0, 10.0).unwrap(),
            index as i64 * 2 + 1,
            index as i64,
            0,
        ));
        matrix.rows[index] = MatrixRow::new(row);
    }
    let content = WindowMatrixEntry {
        window_id: window,
        matrix,
        pixel_bounds: viewport,
        text_pixel_bounds: viewport,
        text_clip_bounds: Some(bounds),
        selected: true,
    };
    let hit_index = PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![PresentedHitRegion::new(
            Some(window),
            PresentedRegionKind::TextBody,
            FrameRect::new(bounds.x, bounds.y, bounds.width, bounds.height).unwrap(),
            0,
        )],
        positions,
    )
    .unwrap();
    let mut faces = FrameFaceMap::default();
    faces.insert(FaceId::new(0), Face::new(FaceId::new(0)));
    state.window_matrices.push(content.clone());
    state.scroll_coverage.push(Arc::new(ScrollCoverage {
        epoch,
        publication: neomacs_display_protocol::scroll_coverage::CoveragePublicationId::allocate()
            .unwrap(),
        anchor_row: 0,
        predict_pixels: true,
        compositor_enabled: true,
        viewport,
        origin: 0.0,
        content,
        faces,
        fonts: Default::default(),
        char_fonts: Default::default(),
        shaped_clusters: Default::default(),
        hit_index,
        pointer_source: Default::default(),
    }));
    let mut frame = state.materialize();
    assert_eq!(frame.scroll_surfaces.len(), 1);
    frame.scroll_surfaces.pop().unwrap()
}

fn renewal_frame(
    presentation: u64,
    window: u64,
    epoch: u64,
    predict_pixels: bool,
) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(100.0, 100.0);
    frame.presentation_id = PresentationId::new(presentation);
    frame.scroll_coverage_renewals = vec![ScrollCoverageRenewal {
        window: DisplayWindowId::new(window as i64),
        epoch,
        predict_pixels,
        compositor_enabled: true,
    }];
    frame
}

#[test]
fn fresh_surfaces_install_and_silence_expires() {
    let mut retention = CoverageRetention::default();
    let first = surface_frame(1, 7, 3);
    retention.refresh(&first);
    let window = DisplayWindowId::new(7);
    assert!(
        retention
            .certified_epoch_at(PresentationId::new(1), window, 3)
            .is_some()
    );
    // A publish naming neither a surface nor a renewal revokes the coverage.
    let mut silent = first.clone();
    silent.presentation_id = PresentationId::new(2);
    silent.scroll_surfaces.clear();
    retention.refresh(&silent);
    assert!(
        retention
            .certified_epoch_at(PresentationId::new(2), window, 3)
            .is_none()
    );
}

#[test]
fn exact_publication_replaces_within_a_presentation_and_survives_renewal() {
    let mut retention = CoverageRetention::default();
    let first = surface_frame(1, 7, 3);
    let second = surface_frame(1, 7, 3);
    let first_id = first.scroll_surfaces[0].coverage().publication;
    let second_id = second.scroll_surfaces[0].coverage().publication;
    let window = DisplayWindowId::new(7);
    let presentation = PresentationId::new(1);
    assert_ne!(first_id, second_id);

    retention.refresh(&first);
    let entry = retention
        .certified_publication_at(presentation, window, 3, first_id)
        .expect("the exact first export is certified");
    assert!(Arc::ptr_eq(entry.surface(), &first.scroll_surfaces[0]));

    // An auxiliary replacement can keep both the logical presentation and
    // content lease. Neither authorizes coordinates from the previous export.
    retention.refresh(&second);
    assert!(
        retention
            .certified_publication_at(presentation, window, 3, first_id)
            .is_none()
    );
    let entry = retention
        .certified_publication_at(presentation, window, 3, second_id)
        .expect("the replacement export is certified");
    assert!(Arc::ptr_eq(entry.surface(), &second.scroll_surfaces[0]));
    assert!(
        retention
            .certified_publication_at(presentation, window, 4, second_id)
            .is_none()
    );
    assert!(
        retention
            .certified_publication_at(presentation, DisplayWindowId::new(8), 3, second_id)
            .is_none()
    );

    // Renewal certifies the immutable export actually held by the renderer,
    // rather than inventing a new identity for the later logical frame.
    retention.refresh(&renewal_frame(2, 7, 3, true));
    let renewed = PresentationId::new(2);
    let entry = retention
        .certified_publication_at(renewed, window, 3, second_id)
        .expect("renewal retains the replacement's exact identity");
    assert!(Arc::ptr_eq(entry.surface(), &second.scroll_surfaces[0]));
    assert!(
        retention
            .certified_publication_at(renewed, window, 3, first_id)
            .is_none()
    );
    assert!(
        retention
            .certified_publication_at(presentation, window, 3, second_id)
            .is_none()
    );

    let mut silent = FrameGlyphBuffer::with_size(100.0, 100.0);
    silent.presentation_id = PresentationId::new(3);
    retention.refresh(&silent);
    for requested in [renewed, silent.presentation_id] {
        assert!(
            retention
                .certified_publication_at(requested, window, 3, second_id)
                .is_none()
        );
    }
}

#[test]
fn renewal_revalidates_and_refreshes_policy_bits() {
    let mut retention = CoverageRetention::default();
    let first = surface_frame(1, 7, 3);
    retention.refresh(&first);
    let renewed = renewal_frame(2, 7, 3, false);
    retention.refresh(&renewed);
    let window = DisplayWindowId::new(7);
    let entry = retention
        .certified_epoch_at(PresentationId::new(2), window, 3)
        .expect("renewal keeps the epoch certified");
    assert!(!entry.predict_pixels());
    assert!(entry.compositor_enabled());
    // The old presentation is no longer the certified one.
    assert!(
        retention
            .certified_epoch_at(PresentationId::new(1), window, 3)
            .is_none()
    );
}

#[test]
fn renewal_naming_a_foreign_epoch_expires_the_held_surface() {
    let mut retention = CoverageRetention::default();
    retention.refresh(&surface_frame(1, 7, 3));
    let window = DisplayWindowId::new(7);
    // The bookkeeping re-keyed (content revision) before any new surface was
    // exported: holding epoch 3 under a renewal for epoch 4 would paint rows
    // the evaluator no longer certifies.
    retention.refresh(&renewal_frame(2, 7, 4, true));
    assert!(
        retention
            .certified_at(PresentationId::new(2), window)
            .is_none()
    );
}

#[test]
fn replacement_surface_supersedes_the_renewed_epoch() {
    let mut retention = CoverageRetention::default();
    retention.refresh(&surface_frame(1, 7, 3));
    retention.refresh(&renewal_frame(2, 7, 3, true));
    retention.refresh(&surface_frame(3, 7, 9));
    let window = DisplayWindowId::new(7);
    assert!(
        retention
            .certified_epoch_at(PresentationId::new(3), window, 9)
            .is_some()
    );
    assert!(
        retention
            .certified_epoch_at(PresentationId::new(3), window, 3)
            .is_none()
    );
}

#[test]
fn windows_expire_independently() {
    let mut retention = CoverageRetention::default();
    let mut first = surface_frame(1, 7, 3);
    first.scroll_surfaces.push(surface(8, 5, 1));
    retention.refresh(&first);
    // Only window 7 is renewed by the next publish; window 8 loses its entry.
    retention.refresh(&renewal_frame(2, 7, 3, true));
    assert!(
        retention
            .certified_epoch_at(PresentationId::new(2), DisplayWindowId::new(7), 3)
            .is_some()
    );
    assert!(
        retention
            .certified_epoch_at(PresentationId::new(2), DisplayWindowId::new(8), 5)
            .is_none()
    );
}

fn auxiliary(base: u64, surfaces: Vec<Arc<ScrollSurface>>) -> AuxiliaryScrollCoverage {
    AuxiliaryScrollCoverage {
        frame: DisplayFrameId::new(0),
        base: PresentationId::new(base),
        surfaces,
    }
}

#[test]
fn auxiliary_exact_base_installs_new_publications_without_a_logical_frame() {
    let first = surface_frame(1, 7, 3);
    let second = surface_frame(1, 7, 3);
    let first_id = first.scroll_surfaces[0].coverage().publication;
    let second_id = second.scroll_surfaces[0].coverage().publication;
    let window = DisplayWindowId::new(7);
    let mut retention = CoverageRetention::default();
    retention.refresh(&first);
    let changes =
        retention.apply_auxiliary(auxiliary(1, second.scroll_surfaces.clone()), Some(&first));
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 1,
            revoked: 0,
            conservative_revocations: 0,
        }
    );
    assert!(
        retention
            .certified_publication_at(first.presentation_id, window, 3, first_id)
            .is_none()
    );
    assert!(
        retention
            .certified_publication_at(first.presentation_id, window, 3, second_id)
            .is_some()
    );
    assert!(
        !retention
            .apply_auxiliary(auxiliary(1, second.scroll_surfaces.clone()), Some(&first))
            .changed()
    );
}

#[test]
fn auxiliary_wrong_base_revokes_absent_newer_epochs_but_never_installs() {
    let stale = surface_frame(1, 7, 3);
    let mut current = surface_frame(2, 7, 9);
    current.scroll_surfaces.push(surface(8, 10, 2));
    let current_id = current.scroll_surfaces[0].coverage().publication;
    let mut retention = CoverageRetention::default();
    retention.refresh(&current);
    let changes = retention.apply_auxiliary(auxiliary(1, stale.scroll_surfaces), Some(&current));
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 0,
            revoked: 1,
            conservative_revocations: 1,
        }
    );
    assert!(
        retention
            .certified_publication_at(
                current.presentation_id,
                DisplayWindowId::new(7),
                9,
                current_id
            )
            .is_some()
    );
    assert!(
        retention
            .certified_at(current.presentation_id, DisplayWindowId::new(8))
            .is_none()
    );
    let empty = retention.apply_auxiliary(auxiliary(1, Vec::new()), Some(&current));
    assert_eq!(
        empty,
        AuxiliaryCoverageChanges {
            installed: 0,
            revoked: 1,
            conservative_revocations: 1,
        }
    );
    assert!(retention.entries.is_empty());
}

#[test]
fn auxiliary_resource_refusal_revokes_its_previous_surface() {
    let first = surface_frame(1, 7, 3);
    let mut incompatible = first.clone();
    incompatible.char_width += 1.0;
    let mut retention = CoverageRetention::default();
    retention.refresh(&first);
    let changes = retention.apply_auxiliary(
        auxiliary(1, first.scroll_surfaces.clone()),
        Some(&incompatible),
    );
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 0,
            revoked: 1,
            conservative_revocations: 0,
        }
    );
    assert!(retention.entries.is_empty());
}

fn try_surface_with_coverage_edit(
    window: u64,
    epoch: u64,
    presentation: u64,
    edit_state: impl FnOnce(&mut FrameDisplayState),
    edit_cov: impl FnOnce(&mut ScrollCoverage),
) -> Option<Arc<ScrollSurface>> {
    let base = surface(window, epoch, presentation);
    let mut cov = base.coverage().clone();
    let mut state = FrameDisplayState::new(10, 2, 10.0, 10.0);
    state.presentation_id = PresentationId::new(presentation);
    state.faces = cov.faces.clone();
    edit_state(&mut state);
    edit_cov(&mut cov);
    state.window_matrices.push(cov.content.clone());
    state.scroll_coverage.push(Arc::new(cov));
    state.materialize().scroll_surfaces.pop()
}

fn surface_with_coverage_edit(
    window: u64,
    epoch: u64,
    presentation: u64,
    edit: impl FnOnce(&mut ScrollCoverage),
) -> Arc<ScrollSurface> {
    try_surface_with_coverage_edit(window, epoch, presentation, |_| {}, edit)
        .expect("coverage fixture should materialize")
}

#[test]
fn auxiliary_accepts_self_closed_additive_resources_without_modifying_base() {
    use neomacs_display_protocol::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedCharGlyph,
        ResolvedFont, ResolvedFontId, ResolvedFontIdentity, ResolvedGlyph, ResolvedGlyphId,
    };
    let mut base = surface_frame(1, 7, 3);
    base.faces.insert(FaceId::new(0), Face::new(FaceId::new(0)));
    let additive_face = FaceId::new(77);
    let additive_font = ResolvedFontId(77);
    let font = ResolvedFont {
        id: additive_font,
        identity: ResolvedFontIdentity::from_file("/fixture/aux-v2.ttf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(FontFileAsset::new("/fixture/aux-v2.ttf", 0).unwrap()),
        },
        family: "AuxV2".into(),
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
    let enriched = surface_with_coverage_edit(7, 3, 1, |cov| {
        let mut face = Face::new(additive_face);
        face.default_resolved_font_id = Some(additive_font);
        cov.faces.insert(additive_face, face);
        cov.fonts.insert(additive_font, font);
        cov.char_fonts.entry(additive_face).or_default().insert(
            'q',
            ResolvedCharGlyph {
                resolved_font_id: additive_font,
                glyph_id: ResolvedGlyphId::new(5),
                advance_px: 10.0,
            },
        );
        cov.shaped_clusters
            .entry(additive_face)
            .or_default()
            .insert(
                "qr".into(),
                vec![ResolvedGlyph {
                    resolved_font_id: additive_font,
                    glyph_id: ResolvedGlyphId::new(5),
                    x: 0.0,
                    y: 0.0,
                    x_advance: 10.0,
                    cluster_start: 0,
                    cluster_end: 2,
                }],
            );
    });
    let pub_id = enriched.coverage().publication;
    let mut retention = CoverageRetention::default();
    retention.refresh(&base);
    let changes = retention.apply_auxiliary(auxiliary(1, vec![enriched]), Some(&base));
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 1,
            revoked: 0,
            conservative_revocations: 0,
        }
    );
    assert!(
        retention
            .certified_publication_at(base.presentation_id, DisplayWindowId::new(7), 3, pub_id)
            .is_some()
    );
}

#[test]
fn auxiliary_rejects_conflicting_or_dangling_resources_and_revokes_previous() {
    let mut conflicting_face = Face::new(FaceId::new(0));
    conflicting_face.background = Color::RED;
    let base_surface = try_surface_with_coverage_edit(
        7,
        3,
        1,
        |state| {
            state.faces.insert(FaceId::new(0), conflicting_face.clone());
        },
        |cov| {
            cov.faces.insert(FaceId::new(0), conflicting_face.clone());
        },
    )
    .expect("base primary surface matching FrameDisplayState should materialize");
    let mut base = FrameGlyphBuffer::with_size(100.0, 100.0);
    base.presentation_id = PresentationId::new(1);
    base.char_width = 10.0;
    base.char_height = 10.0;
    base.faces.insert(FaceId::new(0), conflicting_face);
    base.scroll_surfaces = vec![base_surface];

    // 1. Conflicting FaceId binding against base FrameGlyphBuffer is rejected.
    let mut retention = CoverageRetention::default();
    retention.refresh(&base);
    assert_eq!(retention.entries.len(), 1);
    let candidate = surface(7, 3, 1);
    let changes = retention.apply_auxiliary(auxiliary(1, vec![candidate]), Some(&base));
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 0,
            revoked: 1,
            conservative_revocations: 0,
        }
    );
    assert!(retention.entries.is_empty());

    // 2. Dangling FaceId in coverage.content.matrix (missing from coverage.faces)
    // is now unrepresentable: ScrollCoverage::materialize refuses it before any
    // ScrollSurface can be constructed.
    assert!(
        try_surface_with_coverage_edit(
            7,
            3,
            1,
            |_| {},
            |cov| {
                cov.faces.remove(&FaceId::new(0));
            },
        )
        .is_none()
    );
}

#[test]
fn auxiliary_rejects_mismatched_face_or_font_key_id_and_revokes_previous() {
    use neomacs_display_protocol::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont, ResolvedFontId,
        ResolvedFontIdentity,
    };
    let make_font = |id: ResolvedFontId, family: &str| ResolvedFont {
        id,
        identity: ResolvedFontIdentity::from_file("/fixture/key-id-mismatch.ttf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(
                FontFileAsset::new("/fixture/key-id-mismatch.ttf", 0).unwrap(),
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

    let base_font_id = ResolvedFontId(1);
    let aliased_font_key = ResolvedFontId(99);
    let additive_face_key = FaceId::new(77);
    let base_face_id = FaceId::new(0);

    // 1. Mismatched font key/id: fonts[99] has embedded id=1; refused at
    // ScrollCoverage::materialize so no invalid ScrollSurface can exist.
    let bad_font_key = try_surface_with_coverage_edit(
        7,
        3,
        1,
        |state| {
            state
                .fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            state
                .faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
        },
        |cov| {
            cov.fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            cov.faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
            cov.fonts.insert(
                aliased_font_key,
                make_font(base_font_id, "WRONG STALE FONT"),
            );
            let mut additive_face = Face::new(additive_face_key);
            additive_face.default_resolved_font_id = Some(aliased_font_key);
            cov.faces.insert(additive_face_key, additive_face);
        },
    );
    assert!(bad_font_key.is_none());

    // 2. Mismatched face key/id: faces[77] has embedded id=0; refused at
    // ScrollCoverage::materialize so no invalid ScrollSurface can exist.
    let bad_face_key = try_surface_with_coverage_edit(
        7,
        3,
        1,
        |state| {
            state
                .fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            state
                .faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
        },
        |cov| {
            cov.fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            cov.faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
            cov.fonts.insert(
                aliased_font_key,
                make_font(aliased_font_key, "Valid additive font"),
            );
            let mut aliased_face = Face::new(base_face_id);
            aliased_face.default_resolved_font_id = Some(aliased_font_key);
            cov.faces.insert(additive_face_key, aliased_face);
        },
    );
    assert!(bad_face_key.is_none());
}

#[test]
fn auxiliary_realized_gui_frame_refuses_unresolved_additive_face_and_accepts_resolved_or_tty() {
    use neomacs_display_protocol::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont, ResolvedFontId,
        ResolvedFontIdentity,
    };
    let make_font = |id: ResolvedFontId, family: &str| ResolvedFont {
        id,
        identity: ResolvedFontIdentity::from_file("/fixture/f2-realizer.ttf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(
                FontFileAsset::new("/fixture/f2-realizer.ttf", 0).unwrap(),
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

    let base_face_id = FaceId::new(0);
    let additive_face_id = FaceId::new(77);
    let base_font_id = ResolvedFontId(1);
    let additive_font_id = ResolvedFontId(77);

    // 1a. Negative GUI direction at materialize: realized GUI state refuses
    // an additive face with `default_resolved_font_id: None`.
    assert!(
        try_surface_with_coverage_edit(
            7,
            3,
            1,
            |state| {
                state
                    .fonts
                    .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
                state
                    .faces
                    .get_mut(&base_face_id)
                    .unwrap()
                    .default_resolved_font_id = Some(base_font_id);
            },
            |cov| {
                cov.fonts
                    .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
                cov.faces
                    .get_mut(&base_face_id)
                    .unwrap()
                    .default_resolved_font_id = Some(base_font_id);
                cov.faces
                    .insert(additive_face_id, Face::new(additive_face_id));
            },
        )
        .is_none()
    );

    // 1b. Negative GUI direction at apply_auxiliary: a surface materialized validly
    // on an empty-fonts state (carrying matching base `FaceId(0)` with `None` and an
    // unresolved additive `FaceId(77)`) delivered against a realized-font base
    // `FrameGlyphBuffer` is refused by `apply_auxiliary` and revokes the previous entry.
    let mut gui_base_for_unresolved = surface_frame(1, 7, 3);
    gui_base_for_unresolved
        .fonts
        .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
    gui_base_for_unresolved
        .faces
        .insert(base_face_id, Face::new(base_face_id));
    let mut retention = CoverageRetention::default();
    retention.refresh(&gui_base_for_unresolved);
    assert_eq!(retention.entries.len(), 1);
    let unresolved_gui = surface_with_coverage_edit(7, 3, 1, |cov| {
        cov.faces
            .insert(additive_face_id, Face::new(additive_face_id));
    });
    let unresolved_pub = unresolved_gui.coverage().publication;
    let changes = retention.apply_auxiliary(
        auxiliary(1, vec![unresolved_gui]),
        Some(&gui_base_for_unresolved),
    );
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 0,
            revoked: 1,
            conservative_revocations: 0,
        }
    );
    assert!(retention.entries.is_empty());
    assert!(
        retention
            .certified_publication_at(
                gui_base_for_unresolved.presentation_id,
                DisplayWindowId::new(7),
                3,
                unresolved_pub,
            )
            .is_none()
    );

    // 2. Positive GUI direction: realized GUI base + additive face with
    // `default_resolved_font_id: Some(additive_font_id)` installs cleanly.
    let gui_primary_surface = try_surface_with_coverage_edit(
        7,
        3,
        1,
        |state| {
            state
                .fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            state
                .faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
        },
        |cov| {
            cov.fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            cov.faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
        },
    )
    .expect("realized GUI primary surface should materialize");
    let mut gui_base = FrameGlyphBuffer::with_size(100.0, 100.0);
    gui_base.presentation_id = PresentationId::new(1);
    gui_base.char_width = 10.0;
    gui_base.char_height = 10.0;
    gui_base
        .fonts
        .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
    let mut base_face = Face::new(base_face_id);
    base_face.default_resolved_font_id = Some(base_font_id);
    gui_base.faces.insert(base_face_id, base_face);
    gui_base.scroll_surfaces = vec![gui_primary_surface];

    retention.refresh(&gui_base);
    assert_eq!(retention.entries.len(), 1);
    let resolved_gui = try_surface_with_coverage_edit(
        7,
        3,
        1,
        |state| {
            state
                .fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            state
                .faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
        },
        |cov| {
            cov.fonts
                .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
            cov.faces
                .get_mut(&base_face_id)
                .unwrap()
                .default_resolved_font_id = Some(base_font_id);
            cov.fonts.insert(
                additive_font_id,
                make_font(additive_font_id, "Additive GUI font"),
            );
            let mut additive_face = Face::new(additive_face_id);
            additive_face.default_resolved_font_id = Some(additive_font_id);
            cov.faces.insert(additive_face_id, additive_face);
        },
    )
    .expect("resolved additive GUI surface should materialize");
    let resolved_pub = resolved_gui.coverage().publication;
    let changes = retention.apply_auxiliary(auxiliary(1, vec![resolved_gui]), Some(&gui_base));
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 1,
            revoked: 0,
            conservative_revocations: 0,
        }
    );
    assert!(
        retention
            .certified_publication_at(
                gui_base.presentation_id,
                DisplayWindowId::new(7),
                3,
                resolved_pub,
            )
            .is_some()
    );

    // 3. Positive TTY/headless direction: both `tty_base.fonts` and `coverage.fonts`
    // are empty, so an additive face with `default_resolved_font_id: None` installs cleanly.
    let mut tty_base = surface_frame(1, 7, 3);
    tty_base.faces.insert(base_face_id, Face::new(base_face_id));
    assert!(tty_base.fonts.is_empty());
    retention.refresh(&tty_base);
    let tty_additive = surface_with_coverage_edit(7, 3, 1, |cov| {
        cov.faces
            .insert(additive_face_id, Face::new(additive_face_id));
    });
    assert!(tty_additive.coverage().fonts.is_empty());
    let tty_pub = tty_additive.coverage().publication;
    let changes = retention.apply_auxiliary(auxiliary(1, vec![tty_additive]), Some(&tty_base));
    assert_eq!(
        changes,
        AuxiliaryCoverageChanges {
            installed: 1,
            revoked: 0,
            conservative_revocations: 0,
        }
    );
    assert!(
        retention
            .certified_publication_at(
                tty_base.presentation_id,
                DisplayWindowId::new(7),
                3,
                tty_pub
            )
            .is_some()
    );
}

#[test]
fn auxiliary_asymmetric_realized_fonts_rejects_unresolved_additive_face_and_revokes_previous() {
    use neomacs_display_protocol::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont, ResolvedFontId,
        ResolvedFontIdentity,
    };
    let bitmap_font_id = ResolvedFontId(77);
    let bitmap_font = ResolvedFont {
        id: bitmap_font_id,
        identity: ResolvedFontIdentity::from_file("/fixture/spleen.pcf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(FontFileAsset::new("/fixture/spleen.pcf", 0).unwrap()),
        },
        family: "Spleen".into(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 16.0,
        ascent_px: 12.0,
        descent_px: 4.0,
        space_advance_px: 8.0,
        glyph_advance: Default::default(),
        spacing: None,
    };

    let resolved_face_id = FaceId::new(77);
    let unresolved_face_id = FaceId::new(78);

    // Base has `fonts.is_empty()` while coverage carries a realized font (`!cov.fonts.is_empty()`)
    // alongside an unresolved additive face (`FaceId(78)` with `default_resolved_font_id: None`).
    // ScrollCoverage::materialize now refuses this asymmetric invalid coverage directly.
    assert!(
        try_surface_with_coverage_edit(
            7,
            3,
            1,
            |_| {},
            |cov| {
                cov.fonts.insert(bitmap_font_id, bitmap_font);
                let mut resolved_face = Face::new(resolved_face_id);
                resolved_face.default_resolved_font_id = Some(bitmap_font_id);
                cov.faces.insert(resolved_face_id, resolved_face);
                cov.faces
                    .insert(unresolved_face_id, Face::new(unresolved_face_id));
            },
        )
        .is_none()
    );
}

#[test]
fn primary_refresh_rejects_incompatible_or_unresolved_surfaces_and_revokes_previous() {
    use neomacs_display_protocol::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont, ResolvedFontId,
        ResolvedFontIdentity,
    };
    let make_font = |id: ResolvedFontId, family: &str| ResolvedFont {
        id,
        identity: ResolvedFontIdentity::from_file("/fixture/primary-refresh.ttf", 0, None),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(
                FontFileAsset::new("/fixture/primary-refresh.ttf", 0).unwrap(),
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

    let base_face_id = FaceId::new(0);
    let additive_face_id = FaceId::new(77);
    let base_font_id = ResolvedFontId(1);
    let additive_font_id = ResolvedFontId(77);

    let make_gui_frame = |presentation: u64, scroll_surface: Arc<ScrollSurface>| {
        let mut frame = FrameGlyphBuffer::with_size(100.0, 100.0);
        frame.presentation_id = PresentationId::new(presentation);
        frame.char_width = 10.0;
        frame.char_height = 10.0;
        frame
            .fonts
            .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
        let mut base_face = Face::new(base_face_id);
        base_face.default_resolved_font_id = Some(base_font_id);
        frame.faces.insert(base_face_id, base_face);
        frame.scroll_surfaces = vec![scroll_surface];
        frame
    };

    let valid_gui_surface = |presentation: u64| {
        let base = surface(7, 3, presentation);
        let mut cov = base.coverage().clone();
        cov.fonts
            .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
        cov.faces
            .get_mut(&base_face_id)
            .unwrap()
            .default_resolved_font_id = Some(base_font_id);
        cov.fonts.insert(
            additive_font_id,
            make_font(additive_font_id, "Additive GUI font"),
        );
        let mut additive_face = Face::new(additive_face_id);
        additive_face.default_resolved_font_id = Some(additive_font_id);
        cov.faces.insert(additive_face_id, additive_face);
        let mut state = FrameDisplayState::new(10, 2, 10.0, 10.0);
        state.presentation_id = PresentationId::new(presentation);
        state
            .fonts
            .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
        let mut base_face = Face::new(base_face_id);
        base_face.default_resolved_font_id = Some(base_font_id);
        state.faces.insert(base_face_id, base_face);
        state.window_matrices.push(cov.content.clone());
        state.scroll_coverage.push(Arc::new(cov));
        state.materialize().scroll_surfaces.pop().unwrap()
    };

    // 1. Positive realized GUI primary surface installs cleanly.
    let mut retention = CoverageRetention::default();
    let good_surface = valid_gui_surface(1);
    let good_pub = good_surface.coverage().publication;
    let good_frame = make_gui_frame(1, good_surface);
    retention.refresh(&good_frame);
    assert!(
        retention
            .certified_publication_at(PresentationId::new(1), DisplayWindowId::new(7), 3, good_pub)
            .is_some()
    );

    // 2. Negative: a surface materialized validly on an empty-fonts state (carrying
    // matching base `FaceId(0)` with `None` and an additive `FaceId(77)` with
    // `default_resolved_font_id: None`) delivered on a realized-font GUI primary
    // `FrameGlyphBuffer` must be rejected by F2 and revoke the previous entry.
    let unresolved_surface = surface_with_coverage_edit(7, 3, 2, |cov| {
        cov.faces
            .insert(additive_face_id, Face::new(additive_face_id));
    });
    let mut bad_unresolved = make_gui_frame(2, unresolved_surface);
    bad_unresolved
        .faces
        .insert(base_face_id, Face::new(base_face_id));
    retention.refresh(&bad_unresolved);
    assert!(retention.entries.is_empty());
    assert!(
        retention
            .certified_at(PresentationId::new(2), DisplayWindowId::new(7))
            .is_none()
    );

    // 3. Negative: conflicting base face binding on primary `refresh` rejects and revokes.
    let good_surface = valid_gui_surface(3);
    retention.refresh(&make_gui_frame(3, good_surface));
    assert_eq!(retention.entries.len(), 1);
    let mut conflicting = make_gui_frame(4, valid_gui_surface(4));
    conflicting.faces.get_mut(&base_face_id).unwrap().background = Color::RED;
    retention.refresh(&conflicting);
    assert!(retention.entries.is_empty());

    // 4. Negative: stale `hit_index.presentation()` on primary `refresh` rejects and revokes.
    let good_surface = valid_gui_surface(5);
    retention.refresh(&make_gui_frame(5, good_surface));
    assert_eq!(retention.entries.len(), 1);
    let stale_pres_frame = make_gui_frame(6, valid_gui_surface(5));
    retention.refresh(&stale_pres_frame);
    assert!(retention.entries.is_empty());

    // 5. Positive TTY/headless primary surface (`fonts.is_empty()` on both) installs cleanly.
    let mut tty_frame = surface_frame(7, 7, 3);
    tty_frame
        .faces
        .insert(base_face_id, Face::new(base_face_id));
    let tty_surface = surface_with_coverage_edit(7, 3, 7, |cov| {
        cov.faces
            .insert(additive_face_id, Face::new(additive_face_id));
    });
    let tty_pub = tty_surface.coverage().publication;
    tty_frame.scroll_surfaces = vec![tty_surface];
    retention.refresh(&tty_frame);
    assert!(
        retention
            .certified_publication_at(PresentationId::new(7), DisplayWindowId::new(7), 3, tty_pub)
            .is_some()
    );
}
