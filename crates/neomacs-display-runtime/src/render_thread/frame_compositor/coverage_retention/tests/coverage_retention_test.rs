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

fn surface_with_coverage_edit(
    window: u64,
    epoch: u64,
    presentation: u64,
    edit: impl FnOnce(&mut ScrollCoverage),
) -> Arc<ScrollSurface> {
    let base = surface(window, epoch, presentation);
    let mut cov = base.coverage().clone();
    edit(&mut cov);
    let mut state = FrameDisplayState::new(10, 2, 10.0, 10.0);
    state.presentation_id = PresentationId::new(presentation);
    state.window_matrices.push(cov.content.clone());
    state.scroll_coverage.push(Arc::new(cov));
    let mut frame = state.materialize();
    frame.scroll_surfaces.pop().unwrap()
}

#[test]
fn auxiliary_accepts_self_closed_additive_resources_without_modifying_base() {
    use neomacs_display_protocol::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedCharGlyph,
        ResolvedFont, ResolvedFontId, ResolvedFontIdentity, ResolvedGlyph, ResolvedGlyphId,
    };
    let base = surface_frame(1, 7, 3);
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
    let mut base = surface_frame(1, 7, 3);
    let mut conflicting_face = Face::new(FaceId::new(0));
    conflicting_face.background = Color::RED;
    base.faces.insert(FaceId::new(0), conflicting_face);

    // 1. Conflicting FaceId binding against base FrameGlyphBuffer is rejected.
    let mut retention = CoverageRetention::default();
    retention.refresh(&base);
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

    // 2. Dangling FaceId in coverage.content.matrix (missing from coverage.faces) is rejected.
    let clean_base = surface_frame(1, 7, 3);
    retention.refresh(&clean_base);
    let dangling = surface_with_coverage_edit(7, 3, 1, |cov| {
        cov.faces.remove(&FaceId::new(0));
    });
    let changes = retention.apply_auxiliary(auxiliary(1, vec![dangling]), Some(&clean_base));
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

    let mut base = surface_frame(1, 7, 3);
    base.fonts
        .insert(base_font_id, make_font(base_font_id, "DejaVu Sans Mono"));
    if let Some(face) = base.faces.get_mut(&base_face_id) {
        face.default_resolved_font_id = Some(base_font_id);
    }

    // 1. Mismatched font key/id: fonts[99] has embedded id=1; must be rejected and revoke prior entry.
    let mut retention = CoverageRetention::default();
    retention.refresh(&base);
    let bad_font_key = surface_with_coverage_edit(7, 3, 1, |cov| {
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
    });
    let bad_font_pub = bad_font_key.coverage().publication;
    let changes = retention.apply_auxiliary(auxiliary(1, vec![bad_font_key]), Some(&base));
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
                base.presentation_id,
                DisplayWindowId::new(7),
                3,
                bad_font_pub
            )
            .is_none()
    );

    // 2. Mismatched face key/id: faces[77] has embedded id=0; must be rejected and revoke prior entry.
    retention.refresh(&base);
    let bad_face_key = surface_with_coverage_edit(7, 3, 1, |cov| {
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
    });
    let bad_face_pub = bad_face_key.coverage().publication;
    let changes = retention.apply_auxiliary(auxiliary(1, vec![bad_face_key]), Some(&base));
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
                base.presentation_id,
                DisplayWindowId::new(7),
                3,
                bad_face_pub
            )
            .is_none()
    );
}
