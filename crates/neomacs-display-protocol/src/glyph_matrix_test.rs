/// A surface point in a presentation composed with nothing in motion.
///
/// The production path builds the same value by mapping through the projection
/// the frame was drawn with; a settled projection maps by identity, so these
/// tests state the point they mean while still going through the real witness
/// rather than around it.
fn settled_point(
    presentation: crate::PresentationId,
    x: f32,
    y: f32,
) -> crate::PresentationFramePoint {
    crate::InteractionProjection::settled(presentation)
        .map(
            crate::GeometryPoint::<crate::RootSurfaceSpace, crate::LogicalPixels>::from_px(x, y)
                .expect("a finite surface point"),
        )
        .expect("a settled projection maps every finite point")
}

use super::*;
use crate::DisplayFrameId;
use crate::face::Face;
use crate::frame_chrome::PresentationId;
use crate::frame_glyphs::{DisplaySlotId, PhysCursor};

fn test_pointer_appearance() -> GlyphPointerAppearance {
    GlyphPointerAppearance {
        source: GlyphPointerSourceIdentity {
            kind: GlyphPointerSourceKind::Buffer,
            source_id: 7,
            range_start: 2,
            range_end: 5,
            property_owner: 0,
            occurrence: GlyphPointerOccurrenceIdentity::Source,
        },
        face_id: FaceId::new(9),
    }
}

fn install_complete_window_geometry(
    state: &mut FrameDisplayState,
    window_id: DisplayWindowId,
    text_body: Rect,
) {
    state.window_infos.push(crate::WindowInfo {
        window_id,
        buffer_id: 1,
        window_start: 1,
        window_end: 1,
        buffer_size: 1,
        buffer_modiff: crate::presentation_origin::BufferModiff::default(),
        bounds: text_body,
        geometry: crate::PresentedWindowGeometry::Complete {
            cell_origin: crate::PresentedCellOrigin::default(),
            regions: crate::PresentedWindowRegions {
                outer: text_body,
                text_body,
                ..crate::PresentedWindowRegions::default()
            },
        },
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: String::new(),
        buffer_file_name: String::new(),
        modified: false,
    });
}

#[test]
fn glyph_pointer_token_has_small_niche_sized_overhead() {
    assert_eq!(std::mem::size_of::<Option<GlyphPointerAppearanceId>>(), 4);
    assert_eq!(std::mem::size_of::<Option<GlyphStringSourceId>>(), 4);
    assert_eq!(std::mem::size_of::<super::GlyphImageMarginsId>(), 2);
    assert!(
        std::mem::size_of::<Glyph>() <= 80,
        "row-side provenance must keep Glyph compact; actual size is {}",
        std::mem::size_of::<Glyph>()
    );
}

#[test]
fn ordinary_glyph_and_row_omit_empty_pointer_metadata_from_json() {
    let glyph_json = serde_json::to_string(&Glyph::char('x', FaceId::new(0), 0)).unwrap();
    assert!(!glyph_json.contains("pointer_appearance"));

    let row_json = serde_json::to_string(&GlyphRow::new(GlyphRowRole::Text)).unwrap();
    assert!(!row_json.contains("pointer_appearances"));
    assert!(!row_json.contains("image_margins"));
    assert!(!row_json.contains("string_sources"));
    let round_trip: GlyphRow = serde_json::from_str(&row_json).unwrap();
    assert!(round_trip.pointer_appearances().is_empty());
    assert!(round_trip.image_margins_table().is_empty());
    assert!(round_trip.string_sources().is_empty());
}

#[test]
fn image_margin_tokens_hash_and_compare_by_row_local_geometry() {
    let image_row = |margins| {
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        let margins = row
            .intern_image_margins(margins)
            .expect("image-margin token");
        let mut glyph = Glyph::stretch(1, FaceId::new(0));
        glyph.glyph_type = GlyphType::Image {
            image_id: 7,
            width_cols: 1,
            source_rect: crate::ImageSourceRect::FULL,
            margins,
            opaque_background: crate::ImageOpaqueBackground::default(),
        };
        row.glyphs[GlyphArea::Text.index()].push(glyph);
        row
    };

    let symmetric = image_row(crate::ImageMargins::new(2.0, 1.0));
    let asymmetric = image_row(crate::ImageMargins::asymmetric(2.0, 4.0, 1.0, 3.0));

    assert_eq!(
        symmetric.glyphs[GlyphArea::Text.index()][0].glyph_type,
        asymmetric.glyphs[GlyphArea::Text.index()][0].glyph_type,
        "both rows deliberately reuse row-local token one"
    );
    assert_ne!(symmetric.compute_hash(), asymmetric.compute_hash());
    assert!(!symmetric.row_equal(&asymmetric));
}

#[test]
fn display_state_reports_image_dependencies_without_materializing() {
    let mut state = FrameDisplayState::new(2, 1, 8.0, 16.0);
    let mut matrix = GlyphMatrix::new(1, 2);
    let row = MatrixRow::make_mut(&mut matrix.rows[0]);
    row.enabled = true;
    let margins = row
        .intern_image_margins(crate::ImageMargins::default())
        .expect("image-margin token");
    for image_id in [17, 23, 17] {
        let mut glyph = Glyph::stretch(1, FaceId::new(0));
        glyph.glyph_type = GlyphType::Image {
            image_id,
            width_cols: 1,
            source_rect: crate::ImageSourceRect::FULL,
            margins,
            opaque_background: crate::ImageOpaqueBackground::default(),
        };
        row.glyphs[GlyphArea::Text.index()].push(glyph);
    }
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 16.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 16.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    #[cfg(debug_assertions)]
    reset_materialize_call_count_for_current_thread();
    let mut images = state.referenced_images().iter().collect::<Vec<_>>();
    images.sort_unstable();

    assert_eq!(images, [ImageId::new(17), ImageId::new(23)]);
    #[cfg(debug_assertions)]
    assert_eq!(materialize_call_count_for_current_thread(), 0);
}

#[test]
fn row_equality_resolves_token_identity_through_its_side_table() {
    let mut first = GlyphRow::new(GlyphRowRole::Text);
    let first_token = first
        .intern_pointer_appearance(test_pointer_appearance())
        .expect("first token");
    first.glyphs[GlyphArea::Text.index()].push(Glyph {
        pointer_appearance: Some(first_token),
        ..Glyph::char('x', FaceId::new(0), 0)
    });

    let mut changed_appearance = test_pointer_appearance();
    changed_appearance.source.range_end = 6;
    let mut second = GlyphRow::new(GlyphRowRole::Text);
    let second_token = second
        .intern_pointer_appearance(changed_appearance)
        .expect("second token");
    second.glyphs[GlyphArea::Text.index()].push(Glyph {
        pointer_appearance: Some(second_token),
        ..Glyph::char('x', FaceId::new(0), 0)
    });

    assert_eq!(first_token, second_token, "both rows use local token one");
    assert!(!first.row_equal(&second));
}

#[test]
fn interactive_row_interns_deduplicates_and_round_trips_pointer_appearance() {
    let appearance = test_pointer_appearance();
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let first = row
        .intern_pointer_appearance(appearance)
        .expect("appearance id");
    let duplicate = row
        .intern_pointer_appearance(appearance)
        .expect("deduplicated appearance id");
    assert_eq!(first, duplicate);
    row.glyphs[GlyphArea::Text.index()].push(Glyph {
        pointer_appearance: Some(first),
        ..Glyph::char('x', FaceId::new(0), 0)
    });

    let json = serde_json::to_string(&row).expect("serialize interactive row");
    let round_trip: GlyphRow = serde_json::from_str(&json).expect("deserialize interactive row");
    let token = round_trip.glyphs[GlyphArea::Text.index()][0]
        .pointer_appearance
        .expect("glyph pointer token");
    assert_eq!(round_trip.pointer_appearance(token), Some(&appearance));
    assert_eq!(round_trip.pointer_appearances().len(), 1);
}

#[test]
fn pointer_appearance_id_rejects_unrepresentable_table_index() {
    assert!(GlyphPointerAppearanceId::from_index(u32::MAX as usize).is_none());
}

#[test]
fn frame_display_state_carries_the_interaction_presentation_that_matches_its_pixels() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.presentation_id = PresentationId::new(23);

    let materialized = state.materialize();

    assert_eq!(materialized.presentation_id, PresentationId::new(23));
}

#[test]
fn spatial_projection_validation_rejects_window_and_hit_region_divergence() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state.presentation_id = PresentationId::new(23);
    install_complete_window_geometry(
        &mut state,
        DisplayWindowId::new(1),
        Rect::new(0.0, 0.0, 80.0, 16.0),
    );
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![crate::PresentedHitRegion::new(
            Some(DisplayWindowId::new(1)),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(8.0, 0.0, 72.0, 16.0).unwrap(),
            0,
        )],
        Vec::new(),
    )
    .unwrap();

    assert_eq!(
        state.validate_spatial_projections(),
        Err(crate::PresentedHitError::WindowGeometryMismatch {
            window: DisplayWindowId::new(1),
            region: crate::PresentedRegionKind::TextBody,
        })
    );
}

#[test]
fn spatial_projection_validation_rejects_resize_handle_detached_from_its_window_edge() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state.presentation_id = PresentationId::new(23);
    let window = DisplayWindowId::new(1);
    let outer = Rect::new(0.0, 0.0, 80.0, 16.0);
    install_complete_window_geometry(&mut state, window, outer);
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![crate::PresentedHitRegion::new(
            Some(window),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(outer.x, outer.y, outer.width, outer.height).unwrap(),
            0,
        )],
        Vec::new(),
    )
    .unwrap()
    .with_resize_handles(vec![crate::PresentedResizeHandle::new(
        window,
        crate::PresentedResizeAxis::Horizontal,
        crate::PresentedResizeEdge::Trailing,
        crate::FrameRect::new(0.0, 0.0, 8.0, 16.0).unwrap(),
    )])
    .unwrap();

    assert_eq!(
        state.validate_spatial_projections(),
        Err(crate::PresentedHitError::WindowGeometryMismatch {
            window,
            region: crate::PresentedRegionKind::RightDivider,
        })
    );
}

#[test]
fn spatial_projection_validation_rejects_body_outside_window_allocation() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state.presentation_id = PresentationId::new(23);
    let window = DisplayWindowId::new(1);
    let outer = Rect::new(0.0, 0.0, 80.0, 16.0);
    let body = Rect::new(72.0, 0.0, 16.0, 16.0);
    install_complete_window_geometry(&mut state, window, outer);
    let crate::PresentedWindowGeometry::Complete { regions, .. } =
        &mut state.window_infos[0].geometry
    else {
        panic!("complete window geometry");
    };
    regions.text_body = body;
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![crate::PresentedHitRegion::new(
            Some(window),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(body.x, body.y, body.width, body.height).unwrap(),
            0,
        )],
        Vec::new(),
    )
    .unwrap();

    assert_eq!(
        state.validate_spatial_projections(),
        Err(crate::PresentedHitError::WindowGeometryMismatch {
            window,
            region: crate::PresentedRegionKind::TextBody,
        })
    );
}

#[test]
fn spatial_projection_validation_rejects_overlapping_body_and_mode_line() {
    let mut state = FrameDisplayState::new(10, 4, 8.0, 16.0);
    state.presentation_id = PresentationId::new(23);
    let window = DisplayWindowId::new(1);
    let outer = Rect::new(0.0, 0.0, 80.0, 64.0);
    let body = Rect::new(0.0, 16.0, 80.0, 32.0);
    let mode_line = Rect::new(0.0, 44.0, 80.0, 20.0);
    install_complete_window_geometry(&mut state, window, outer);
    let crate::PresentedWindowGeometry::Complete { regions, .. } =
        &mut state.window_infos[0].geometry
    else {
        panic!("complete window geometry");
    };
    regions.text_body = body;
    regions.mode_line = Some(mode_line);
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![
            crate::PresentedHitRegion::new(
                Some(window),
                crate::PresentedRegionKind::TextBody,
                crate::FrameRect::new(body.x, body.y, body.width, body.height).unwrap(),
                0,
            ),
            crate::PresentedHitRegion::new(
                Some(window),
                crate::PresentedRegionKind::ModeLine,
                crate::FrameRect::new(mode_line.x, mode_line.y, mode_line.width, mode_line.height)
                    .unwrap(),
                0,
            ),
        ],
        Vec::new(),
    )
    .unwrap();

    assert_eq!(
        state.validate_spatial_projections(),
        Err(crate::PresentedHitError::WindowGeometryMismatch {
            window,
            region: crate::PresentedRegionKind::ModeLine,
        })
    );
}

#[test]
fn spatial_projection_validation_rejects_matrix_body_clip_divergence() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state.presentation_id = PresentationId::new(23);
    let window = DisplayWindowId::new(1);
    let body = Rect::new(0.0, 0.0, 80.0, 16.0);
    install_complete_window_geometry(&mut state, window, body);
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![crate::PresentedHitRegion::new(
            Some(window),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(body.x, body.y, body.width, body.height).unwrap(),
            0,
        )],
        Vec::new(),
    )
    .unwrap();
    state.window_matrices.push(WindowMatrixEntry {
        window_id: window,
        matrix: GlyphMatrix::new(1, 10),
        pixel_bounds: body,
        text_pixel_bounds: body,
        text_clip_bounds: Some(Rect::new(8.0, 0.0, 72.0, 16.0)),
        selected: true,
    });

    assert_eq!(
        state.validate_spatial_projections(),
        Err(crate::PresentedHitError::WindowGeometryMismatch {
            window,
            region: crate::PresentedRegionKind::TextBody,
        })
    );
}

#[test]
fn frame_display_state_carries_pointer_map_into_materialized_snapshot() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    install_complete_window_geometry(
        &mut state,
        DisplayWindowId::new(1),
        Rect::new(0.0, 0.0, 80.0, 16.0),
    );
    state.faces.insert(FaceId::new(0), Face::default());
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.height_px = 16.0;
    row.ascent_px = 12.0;
    row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('x', FaceId::new(0), 0).with_pixel_width(8.0));
    let mut matrix = GlyphMatrix::new(1, 10);
    matrix.rows[0] = crate::glyph_matrix::MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![crate::PresentedHitRegion::new(
            Some(DisplayWindowId::new(1)),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(0.0, 0.0, 80.0, 16.0).unwrap(),
            0,
        )],
        vec![],
    )
    .unwrap();

    let appearance = crate::PresentedPointerSourceAppearance::new(
        vec![crate::PresentedSourcePaintSpan::new(
            crate::PresentedPrimitiveKind::Glyph,
            GlyphRowRole::Text,
            DisplaySlotId {
                window_id: DisplayWindowId::new(1),
                row: 0,
                col: 0,
            },
            crate::FrameRect::new(0.0, 0.0, 8.0, 16.0).unwrap(),
        )],
        crate::PointerDrawMode::Face(FaceId::new(0)),
        crate::PointerDrawMode::Face(FaceId::new(0)),
    );
    state.presented_pointer_source = crate::PresentedPointerSourceMap::new(
        vec![crate::PresentedPointerRegion::new_owned(
            crate::PresentedRegionId::new(
                Some(DisplayWindowId::new(1)),
                crate::PresentedRegionKind::TextBody,
            ),
            crate::FrameRect::new(0.0, 0.0, 8.0, 16.0).unwrap(),
            Some(crate::InteractionId::new(7)),
            Some(crate::PointerAppearanceId::try_from(0usize).unwrap()),
        )],
        vec![appearance],
    );
    let materialized = state.materialize();

    let hit = materialized
        .presented_pointer()
        .hit_test(4.0, 8.0)
        .expect("transported pointer region");
    assert_eq!(hit.interaction(), Some(crate::InteractionId::new(7)));
}

#[test]
fn subcell_stretch_and_following_char_have_unique_materialized_pointer_slots() {
    let window_id = DisplayWindowId::new(6);
    let body = Rect::new(0.0, 0.0, 80.0, 16.0);
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    install_complete_window_geometry(&mut state, window_id, body);
    state.faces.insert(FaceId::new(0), Face::default());

    // `:align-to` can legitimately produce a positive pixel advance smaller
    // than half a cell.  Its rounded logical width is zero, but the stretch is
    // still a materialized primitive immediately followed by buffer text.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.height_px = 16.0;
    row.ascent_px = 12.0;
    row.glyphs[GlyphArea::Text.index()].extend([
        Glyph::stretch(0, FaceId::new(0)).with_pixel_width(2.0),
        Glyph::char('x', FaceId::new(0), 0).with_pixel_width(8.0),
    ]);
    let mut matrix = GlyphMatrix::new(1, 10);
    matrix.rows[0] = crate::glyph_matrix::MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id,
        matrix,
        pixel_bounds: body,
        text_pixel_bounds: body,
        text_clip_bounds: None,
        selected: true,
    });
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![crate::PresentedHitRegion::new(
            Some(window_id),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(body.x, body.y, body.width, body.height).unwrap(),
            0,
        )],
        vec![],
    )
    .unwrap();

    let paint_bounds = crate::FrameRect::new(0.0, 0.0, 10.0, 16.0).unwrap();
    let appearance = crate::PresentedPointerSourceAppearance::new(
        vec![crate::PresentedSourcePaintSpan::new_run(
            crate::PresentedPrimitiveKind::Glyph,
            GlyphRowRole::Text,
            DisplaySlotId {
                window_id,
                row: 0,
                col: 0,
            },
            2,
            paint_bounds,
        )],
        crate::PointerDrawMode::Face(FaceId::new(0)),
        crate::PointerDrawMode::Face(FaceId::new(0)),
    );
    state.presented_pointer_source = crate::PresentedPointerSourceMap::new(
        vec![crate::PresentedPointerRegion::new_owned(
            crate::PresentedRegionId::new(Some(window_id), crate::PresentedRegionKind::TextBody),
            paint_bounds,
            Some(crate::InteractionId::new(7)),
            Some(crate::PointerAppearanceId::try_from(0usize).unwrap()),
        )],
        vec![appearance],
    );

    let materialized = state.materialize();
    let slots = materialized
        .glyphs
        .iter()
        .filter_map(|glyph| glyph.slot_id())
        .collect::<Vec<_>>();
    assert_eq!(
        slots,
        vec![
            DisplaySlotId {
                window_id,
                row: 0,
                col: 0,
            },
            DisplaySlotId {
                window_id,
                row: 0,
                col: 1,
            },
        ]
    );
}

#[test]
fn face_fill_does_not_claim_the_pointer_slot_of_a_grid_glyph() {
    let window_id = DisplayWindowId::new(6);
    let window_bounds = Rect::new(0.0, 0.0, 80.0, 128.0);
    let row_bounds = Rect::new(0.0, 112.0, 80.0, 16.0);
    let mut state = FrameDisplayState::new(10, 8, 8.0, 16.0);
    install_complete_window_geometry(&mut state, window_id, window_bounds);
    state.faces.insert(FaceId::new(0), Face::default());
    state.face_fills.push(FaceFillItem {
        window_id,
        row_role: GlyphRowRole::Text,
        clip_rect: Some(window_bounds),
        bounds: row_bounds,
        face_id: FaceId::new(0),
    });

    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.height_px = 16.0;
    row.ascent_px = 12.0;
    row.pixel_y = 112.0;
    row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('2', FaceId::new(0), 0).with_pixel_width(8.0));
    let mut matrix = GlyphMatrix::new(8, 10);
    matrix.rows[7] = crate::glyph_matrix::MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id,
        matrix,
        pixel_bounds: window_bounds,
        text_pixel_bounds: window_bounds,
        text_clip_bounds: None,
        selected: true,
    });
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![crate::PresentedHitRegion::new(
            Some(window_id),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(
                window_bounds.x,
                window_bounds.y,
                window_bounds.width,
                window_bounds.height,
            )
            .unwrap(),
            0,
        )],
        vec![],
    )
    .unwrap();

    let paint_bounds = crate::FrameRect::new(0.0, 112.0, 8.0, 16.0).unwrap();
    let appearance = crate::PresentedPointerSourceAppearance::new(
        vec![crate::PresentedSourcePaintSpan::new(
            crate::PresentedPrimitiveKind::Glyph,
            GlyphRowRole::Text,
            DisplaySlotId {
                window_id,
                row: 7,
                col: 0,
            },
            paint_bounds,
        )],
        crate::PointerDrawMode::Face(FaceId::new(0)),
        crate::PointerDrawMode::Face(FaceId::new(0)),
    );
    state.presented_pointer_source = crate::PresentedPointerSourceMap::new(
        vec![crate::PresentedPointerRegion::new_owned(
            crate::PresentedRegionId::new(Some(window_id), crate::PresentedRegionKind::TextBody),
            paint_bounds,
            Some(crate::InteractionId::new(7)),
            Some(crate::PointerAppearanceId::try_from(0usize).unwrap()),
        )],
        vec![appearance],
    );

    let materialized = state.materialize();
    assert!(matches!(
        materialized.glyphs.as_slice(),
        [
            FrameGlyph::Background { .. },
            FrameGlyph::Char { char: '2', .. }
        ]
    ));
}

#[test]
fn glyph_type_kind_codes_match_gnu_glyph_type() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let image_margins = row
        .intern_image_margins(crate::ImageMargins::default())
        .expect("first image-margin token");
    let cases = [
        (GlyphTypeKind::Char, 0),
        (GlyphTypeKind::Composite, 1),
        (GlyphTypeKind::Glyphless, 2),
        (GlyphTypeKind::Image, 3),
        (GlyphTypeKind::Stretch, 4),
        (GlyphTypeKind::Xwidget, 5),
    ];

    for (kind, code) in cases {
        assert_eq!(kind.gnu_code(), code);
        assert_eq!(GlyphTypeKind::from_gnu_code(code), Some(kind));
    }

    assert_eq!(GlyphTypeKind::from_gnu_code(6), None);
    assert_eq!(
        Glyph::char('x', FaceId::new(0), 0).glyph_type.gnu_kind(),
        GlyphTypeKind::Char
    );
    assert_eq!(
        GlyphType::Composite { text: "xy".into() }.gnu_kind(),
        GlyphTypeKind::Composite
    );
    assert_eq!(
        GlyphType::Glyphless {
            ch: '\u{fffd}',
            presentation: GlyphlessPresentation::EmptyBox,
        }
        .gnu_kind(),
        GlyphTypeKind::Glyphless
    );
    assert_eq!(
        GlyphType::Image {
            source_rect: crate::ImageSourceRect::FULL,
            image_id: 7,
            width_cols: 1,
            margins: image_margins,
            opaque_background: crate::ImageOpaqueBackground::default(),
        }
        .gnu_kind(),
        GlyphTypeKind::Image
    );
    assert_eq!(
        Glyph::stretch(2, FaceId::new(0)).glyph_type.gnu_kind(),
        GlyphTypeKind::Stretch
    );
}

#[test]
fn glyph_area_codes_match_gnu_glyph_row_area() {
    let cases = [
        (GlyphArea::LeftMargin, 0),
        (GlyphArea::Text, 1),
        (GlyphArea::RightMargin, 2),
    ];

    for (area, code) in cases {
        assert_eq!(area.gnu_code(), code);
        assert_eq!(area.index(), usize::from(code));
        assert_eq!(GlyphArea::from_gnu_code(code), Some(area));
    }

    assert_eq!(GlyphArea::from_gnu_code(3), None);
}

#[test]
fn empty_row_has_zero_hash() {
    let row = GlyphRow::new(GlyphRowRole::Text);
    assert_eq!(row.compute_hash(), 0);
}

#[test]
fn row_hash_changes_with_content() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let hash_empty = row.compute_hash();
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));
    let hash_a = row.compute_hash();
    assert_ne!(hash_empty, hash_a);
}

#[test]
fn row_hash_differs_for_different_chars() {
    let mut row_a = GlyphRow::new(GlyphRowRole::Text);
    row_a.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));

    let mut row_b = GlyphRow::new(GlyphRowRole::Text);
    row_b.glyphs[GlyphArea::Text as usize].push(Glyph::char('b', FaceId::new(0), 0));

    assert_ne!(row_a.compute_hash(), row_b.compute_hash());
}

#[test]
fn automatic_composition_plan_is_part_of_row_identity() {
    let automatic_row = |base: char, extenders: &str, width_cols: u8| {
        let terminal = TerminalComposition {
            cells: vec![TerminalCompositionCell {
                base,
                extenders: extenders.into(),
                width_cols,
                source_char_len: 2,
            }]
            .into_boxed_slice(),
            width_cols: u16::from(width_cols),
        };
        let mut glyph = Glyph::char(base, FaceId::new(0), 0);
        glyph.glyph_type = GlyphType::AutomaticComposite {
            text: "ab".into(),
            terminal,
        };
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        row.glyphs[GlyphArea::Text.index()].push(glyph);
        row
    };

    let one_cell = automatic_row('a', "b", 1);
    let two_cells = automatic_row('a', "b", 2);
    let different_cell_text = automatic_row('a', "c", 1);

    assert_ne!(one_cell.compute_hash(), two_cells.compute_hash());
    assert_ne!(one_cell.compute_hash(), different_cell_text.compute_hash());
}

#[test]
fn automatic_composition_materialized_span_comes_from_terminal_plan() {
    let mut glyph = Glyph::char('a', FaceId::new(0), 0);
    glyph.glyph_type = GlyphType::AutomaticComposite {
        text: "ab".into(),
        terminal: TerminalComposition {
            cells: vec![
                TerminalCompositionCell {
                    base: 'a',
                    extenders: "".into(),
                    width_cols: 1,
                    source_char_len: 1,
                },
                TerminalCompositionCell {
                    base: 'b',
                    extenders: "".into(),
                    width_cols: 1,
                    source_char_len: 1,
                },
            ]
            .into_boxed_slice(),
            width_cols: 2,
        },
    };

    assert_eq!(glyph.materialized_slot_span(), 2);
}

#[test]
fn row_hash_differs_for_different_faces() {
    let mut row_a = GlyphRow::new(GlyphRowRole::Text);
    row_a.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));

    let mut row_b = GlyphRow::new(GlyphRowRole::Text);
    row_b.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(1), 0));

    assert_ne!(row_a.compute_hash(), row_b.compute_hash());
}

#[test]
fn row_hash_differs_for_vertical_box_edge_ownership() {
    use crate::face::BoxVerticalEdges;

    let mut closed = GlyphRow::new(GlyphRowRole::Text);
    closed.glyphs[GlyphArea::Text.index()].push(Glyph::stretch(1, FaceId::new(3)));
    let mut open = closed.clone();
    open.glyphs[GlyphArea::Text.index()][0].box_vertical_edges = BoxVerticalEdges::Neither;

    assert_ne!(closed.compute_hash(), open.compute_hash());
}

#[test]
fn row_hash_differs_for_different_pixel_widths() {
    let mut row_a = GlyphRow::new(GlyphRowRole::Text);
    row_a.glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('a', FaceId::new(0), 0).with_pixel_width(8.0));

    let mut row_b = GlyphRow::new(GlyphRowRole::Text);
    row_b.glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('a', FaceId::new(0), 0).with_pixel_width(13.0));

    assert_ne!(row_a.compute_hash(), row_b.compute_hash());
}

#[test]
fn row_hash_differs_for_different_vertical_offsets() {
    let mut row_a = GlyphRow::new(GlyphRowRole::Text);
    row_a.glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('a', FaceId::new(0), 0).with_vertical_offset(-4.0));

    let mut row_b = GlyphRow::new(GlyphRowRole::Text);
    row_b.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));

    assert_ne!(row_a.compute_hash(), row_b.compute_hash());
}

#[test]
fn identical_rows_have_same_hash() {
    let mut row_a = GlyphRow::new(GlyphRowRole::Text);
    row_a.glyphs[GlyphArea::Text as usize].push(Glyph::char('x', FaceId::new(5), 100));

    let mut row_b = GlyphRow::new(GlyphRowRole::Text);
    row_b.glyphs[GlyphArea::Text as usize].push(Glyph::char('x', FaceId::new(5), 100));

    assert_eq!(row_a.compute_hash(), row_b.compute_hash());
}

#[test]
fn row_equal_uses_hash_fast_path() {
    let mut row_a = GlyphRow::new(GlyphRowRole::Text);
    row_a.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));
    row_a.hash = row_a.compute_hash();

    let mut row_b = GlyphRow::new(GlyphRowRole::Text);
    row_b.glyphs[GlyphArea::Text as usize].push(Glyph::char('b', FaceId::new(0), 0));
    row_b.hash = row_b.compute_hash();

    // Different hashes → rows are not equal (fast path, no cell comparison)
    assert!(!row_a.row_equal(&row_b));

    // Same content → equal
    let row_c = row_a.clone();
    assert!(row_a.row_equal(&row_c));
}

#[test]
fn new_row_has_empty_glyph_areas() {
    let row = GlyphRow::new(GlyphRowRole::ModeLine);
    assert!(row.glyphs[GlyphArea::LeftMargin as usize].is_empty());
    assert!(row.glyphs[GlyphArea::Text as usize].is_empty());
    assert!(row.glyphs[GlyphArea::RightMargin as usize].is_empty());
    assert_eq!(row.role, GlyphRowRole::ModeLine);
    assert!(row.enabled);
}

#[test]
fn matrix_new_has_correct_dimensions() {
    let matrix = GlyphMatrix::new(24, 80);
    assert_eq!(matrix.nrows, 24);
    assert_eq!(matrix.ncols, 80);
    assert_eq!(matrix.rows.len(), 24);
}

#[test]
fn matrix_rows_are_disabled_by_default() {
    // Rows in a freshly constructed GlyphMatrix start disabled,
    // matching GNU's MATRIX_ROW_ENABLED_P discipline: the walker
    // marks rows enabled as it populates them, and rows never
    // populated stay inert so scroll / clear-to-eob helpers
    // (e.g. overwrite_last_window_right_border) skip them.
    let matrix = GlyphMatrix::new(3, 10);
    for row in &matrix.rows {
        assert!(!row.enabled);
        assert_eq!(row.role, GlyphRowRole::Text);
    }
}

#[test]
fn matrix_clear_resets_all_rows() {
    let mut matrix = GlyphMatrix::new(2, 10);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('x', FaceId::new(0), 0));
    row_0.hash = 12345;
    row_0.cursor_col = Some(5);

    matrix.clear();

    assert!(matrix.rows[0].glyphs[GlyphArea::Text as usize].is_empty());
    assert_eq!(matrix.rows[0].hash, 0);
    assert_eq!(matrix.rows[0].cursor_col, None);
}

#[test]
fn matrix_resize_grows_and_shrinks() {
    let mut matrix = GlyphMatrix::new(10, 80);
    assert_eq!(matrix.rows.len(), 10);

    matrix.resize(20, 100);
    assert_eq!(matrix.nrows, 20);
    assert_eq!(matrix.ncols, 100);
    assert_eq!(matrix.rows.len(), 20);

    matrix.resize(5, 40);
    assert_eq!(matrix.nrows, 5);
    assert_eq!(matrix.ncols, 40);
    assert_eq!(matrix.rows.len(), 5);
}

#[test]
fn frame_display_state_new_has_correct_defaults() {
    let state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    assert_eq!(state.frame_cols, 80);
    assert_eq!(state.frame_rows, 24);
    assert_eq!(state.char_width, 8.0);
    assert_eq!(state.char_height, 16.0);
    assert!(state.window_matrices.is_empty());
    assert!(state.faces.is_empty());
}

#[test]
fn frame_display_state_add_window_matrix() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    let matrix = GlyphMatrix::new(20, 80);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 640.0, 320.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 640.0, 320.0),
        text_clip_bounds: None,
        selected: true,
    });
    assert_eq!(state.window_matrices.len(), 1);
    assert_eq!(state.window_matrices[0].window_id, DisplayWindowId::new(1));
    assert_eq!(state.window_matrices[0].matrix.nrows, 20);
}

// ---------------------------------------------------------------------------
// FrameDisplayState::materialize() tests
// ---------------------------------------------------------------------------

/// Helper: build a FrameDisplayState with one window containing `text` on row 0.
fn state_with_text(text: &str) -> FrameDisplayState {
    let cols = text.len().max(1);
    let rows = 1;
    let char_w = 8.0f32;
    let char_h = 16.0f32;
    let mut state = FrameDisplayState::new(cols, rows, char_w, char_h);

    // Insert a default face (id 0)
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, cols);
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).enabled = true;
    for (i, ch) in text.chars().enumerate() {
        crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs
            [GlyphArea::Text as usize]
            .push(Glyph::char(ch, FaceId::new(0), i));
    }

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * char_w, char_h),
        text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * char_w, char_h),
        text_clip_bounds: None,
        selected: true,
    });
    state
}

#[test]
fn materialize_produces_correct_glyph_count_from_grid() {
    let state = state_with_text("Hello");
    let buf = state.materialize();
    // 5 characters -> 5 FrameGlyph::Char entries
    assert_eq!(buf.glyphs.len(), 5);
    for g in &buf.glyphs {
        assert!(matches!(g, FrameGlyph::Char { .. }));
    }
}

#[test]
fn materialize_places_right_margin_glyphs_in_their_structural_area() {
    let char_w = 8.0;
    let char_h = 16.0;
    let window = DisplayWindowId::new(1);
    let outer = Rect::new(0.0, 0.0, 12.0 * char_w, char_h);
    let text_body = Rect::new(0.0, 0.0, 10.0 * char_w, char_h);
    let right_margin = Rect::new(10.0 * char_w, 0.0, 2.0 * char_w, char_h);
    let mut state = FrameDisplayState::new(12, 1, char_w, char_h);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 10);
    let row = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row.enabled = true;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('x', FaceId::new(0), 0));
    row.glyphs[GlyphArea::RightMargin.index()].extend([
        Glyph::char('R', FaceId::new(0), 0),
        Glyph::char('M', FaceId::new(0), 0),
    ]);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: window,
        matrix,
        pixel_bounds: outer,
        text_pixel_bounds: text_body,
        text_clip_bounds: Some(text_body),
        selected: true,
    });
    install_complete_window_geometry(&mut state, window, outer);
    let crate::PresentedWindowGeometry::Complete { regions, .. } =
        &mut state.window_infos[0].geometry
    else {
        panic!("complete window geometry");
    };
    regions.text_body = text_body;
    regions.right_margin = Some(right_margin);
    regions.right_margin_columns = 2;

    let chars: Vec<(char, f32)> = state
        .materialize()
        .glyphs
        .iter()
        .filter_map(|glyph| match glyph {
            FrameGlyph::Char { char, x, .. } => Some((*char, *x)),
            _ => None,
        })
        .collect();

    assert_eq!(
        chars,
        vec![('x', 0.0), ('R', 80.0), ('M', 88.0)],
        "right-margin placement must not depend on the text area's used width"
    );
}

#[test]
fn mode_line_reserves_the_final_window_cell_for_a_right_border() {
    // GNU `build_frame_matrix_from_leaf_window` installs a vertical border in
    // LAST_AREA for every enabled row, including mode lines.  Window-wide
    // chrome therefore needs a structural LAST_AREA origin even though it does
    // not use the buffer text area's left/right margins.
    let mut state = FrameDisplayState::new(5, 1, 8.0, 16.0);
    let entry = WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix: GlyphMatrix::new(1, 5),
        pixel_bounds: Rect::new(0.0, 0.0, 40.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 40.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    };
    state.window_matrices.push(entry);

    let layout = state.glyph_row_area_layout(&state.window_matrices[0], GlyphRowRole::ModeLine);
    let GlyphAreaPlacement::Structural(right) = layout.placement(GlyphArea::RightMargin) else {
        panic!("window-wide rows must publish a structural right-border cell");
    };
    assert_eq!(right.bounds(), Rect::new(32.0, 0.0, 8.0, 16.0));
}

#[test]
fn materialize_emits_tab_line_row_at_window_top() {
    // Regression guard for the GUI "empty tab-line" bug: a window with a
    // tab-line (row 0, role TabLine) above its text (row 1, role Text) must
    // materialize the tab-line glyphs at the window's TOP edge, tagged with
    // GlyphRowRole::TabLine so the renderer treats them as top chrome.
    let char_w = 8.0f32;
    let char_h = 16.0f32;
    let cols = 4;
    let win = Rect::new(10.0, 20.0, cols as f32 * char_w, 2.0 * char_h);
    let text_area = Rect::new(10.0, 20.0 + char_h, cols as f32 * char_w, char_h);

    let mut state = FrameDisplayState::new(cols, 2, char_w, char_h);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(2, cols);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.role = GlyphRowRole::TabLine;
    row_0.enabled = true;
    row_0.height_px = char_h;
    row_0.pixel_y = 0.0;
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('T', FaceId::new(0), 0));
    let row_1 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]);
    row_1.role = GlyphRowRole::Text;
    row_1.enabled = true;
    row_1.height_px = char_h;
    row_1.pixel_y = 0.0;
    row_1.glyphs[GlyphArea::Text as usize].push(Glyph::char('B', FaceId::new(0), 0));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: win,
        text_pixel_bounds: text_area,
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();

    let tab_glyphs: Vec<_> = buf
        .glyphs
        .iter()
        .filter_map(|g| match g {
            FrameGlyph::Char {
                row_role,
                char: c,
                y,
                ..
            } if *row_role == GlyphRowRole::TabLine => Some((*c, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(tab_glyphs.len(), 1, "tab-line glyph must be materialized");
    assert_eq!(tab_glyphs[0].0, 'T');
    assert!(
        (tab_glyphs[0].1 - win.y).abs() < 0.5,
        "tab-line glyph y={} must sit at window top {}",
        tab_glyphs[0].1,
        win.y
    );

    let body: Vec<_> = buf
        .glyphs
        .iter()
        .filter_map(|g| match g {
            FrameGlyph::Char {
                row_role,
                char: c,
                y,
                ..
            } if *row_role == GlyphRowRole::Text => Some((*c, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0].0, 'B');
    assert!(
        (body[0].1 - text_area.y).abs() < 0.5,
        "body glyph y={} must sit in the text area at {}",
        body[0].1,
        text_area.y
    );
}

#[test]
fn materialize_right_aligns_reversed_row() {
    // A reversed_p (right-to-left) row is flush to the right margin: its glyphs
    // start where the content ends at the right edge, not at column 0.
    let char_w = 8.0f32;
    let char_h = 16.0f32;
    let cols = 10; // 80px-wide text area
    let mut state = FrameDisplayState::new(cols, 1, char_w, char_h);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));
    let mut matrix = GlyphMatrix::new(1, cols);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.reversed_p = true;
    // Two cells, no recorded pixel width -> one column (8px) each => 16px used.
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('\u{05d0}', FaceId::new(0), 0));
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('\u{05d1}', FaceId::new(0), 1));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * char_w, char_h),
        text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * char_w, char_h),
        text_clip_bounds: None,
        selected: true,
    });
    let buf = state.materialize();
    let first_x = buf
        .glyphs
        .iter()
        .find_map(|g| match g {
            FrameGlyph::Char { x, .. } => Some(*x),
            _ => None,
        })
        .expect("a char glyph");
    // 80px area minus 16px content => content flush-right starting at x=64.
    assert!(
        (first_x - 64.0).abs() < 0.01,
        "expected flush-right x=64, got {first_x}"
    );
}

#[test]
fn materialize_empty_grid_produces_no_glyphs() {
    let state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    let buf = state.materialize();
    assert!(buf.glyphs.is_empty());
}

#[test]
fn materialize_includes_backgrounds() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.backgrounds.push(BackgroundItem {
        bounds: Rect::new(0.0, 0.0, 640.0, 384.0),
        color: Color::RED,
    });
    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Background { bounds, color } => {
            assert_eq!(bounds.x, 0.0);
            assert_eq!(bounds.width, 640.0);
            assert_eq!(*color, Color::RED);
        }
        other => panic!("expected Background, got {:?}", other),
    }
}

#[test]
fn materialize_includes_borders() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.borders.push(BorderItem {
        window_id: DisplayWindowId::new(42),
        x: 100.0,
        y: 0.0,
        width: 1.0,
        height: 384.0,
        color: Color::WHITE,
    });
    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Border {
            window_id,
            x,
            width,
            color,
            ..
        } => {
            assert_eq!(window_id.get(), 42);
            assert_eq!(*x, 100.0);
            assert_eq!(*width, 1.0);
            assert_eq!(*color, Color::WHITE);
        }
        other => panic!("expected Border, got {:?}", other),
    }
}

#[test]
fn materialize_includes_cursors() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.cursors.push(CursorItem {
        window_id: DisplayWindowId::new(7),
        role: CursorItemRole::Decorative,
        slot_id: DisplaySlotId::from_pixels(
            DisplayWindowId::new(7),
            Px(40.0),
            Px(0.0),
            Px(8.0),
            Px(16.0),
        ),
        x: 40.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        style: CursorStyle::FilledBox,
        color: Color::GREEN,
        cursor_fg: Color::WHITE,
        ascent: 12.0,
    });
    let buf = state.materialize();
    assert!(buf.glyphs.is_empty());
    assert_eq!(buf.window_cursors.len(), 1);
    assert!(buf.active_cursor().is_none());
    let cursor = &buf.window_cursors[0];
    assert_eq!(cursor.window_id.get(), 7);
    assert_eq!(cursor.x, 40.0);
    assert_eq!(cursor.style, CursorStyle::FilledBox);
    assert_eq!(cursor.color, Color::GREEN);
    assert_eq!(cursor.cursor_fg, Color::WHITE);
    // The CursorItem's ascent must flow into the WindowCursor. A hardcoded 0
    // here dropped a non-selected window's cursor one text row too low
    // (`cursor_draw_rect` places the top at `glyph_baseline - ascent`).
    assert_eq!(cursor.ascent, 12.0);
}

#[test]
fn presented_cursor_for_window_selects_typed_caret_after_decoration() {
    let window_id = DisplayWindowId::new(7);
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.cursors.push(CursorItem {
        window_id,
        role: CursorItemRole::Decorative,
        slot_id: DisplaySlotId::from_pixels(window_id, Px(8.0), Px(0.0), Px(8.0), Px(16.0)),
        x: 8.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        style: CursorStyle::FilledBox,
        color: Color::RED,
        cursor_fg: Color::WHITE,
        ascent: 12.0,
    });
    state.cursors.push(CursorItem {
        window_id,
        role: CursorItemRole::WindowCaret { charpos: 42 },
        slot_id: DisplaySlotId::from_pixels(window_id, Px(40.0), Px(16.0), Px(8.0), Px(16.0)),
        x: 40.0,
        y: 16.0,
        width: 9.0,
        height: 17.0,
        style: CursorStyle::Hollow,
        color: Color::BLUE,
        cursor_fg: Color::BLACK,
        ascent: 13.0,
    });

    let cursor = state
        .presented_cursor_for_window(window_id)
        .expect("typed window caret");

    assert_eq!(cursor.charpos, 42);
    assert_eq!(cursor.slot_id.col, 5);
    assert_eq!(cursor.x, 40.0);
    assert_eq!(cursor.y, 16.0);
    assert_eq!(cursor.width, 9.0);
    assert_eq!(cursor.height, 17.0);
    assert_eq!(cursor.ascent, 13.0);
}

#[test]
fn decorative_cursor_paint_round_trips_without_losing_inverse_foreground() {
    let window_id = DisplayWindowId::new(7);
    let slot_id = DisplaySlotId::from_pixels(window_id, Px(40.0), Px(0.0), Px(8.0), Px(16.0));
    let mut input = FrameGlyphBuffer::new();
    input.window_cursors.push(WindowCursor {
        window_id,
        slot_id,
        x: 40.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        style: CursorStyle::FilledBox,
        color: Color::BLUE,
        cursor_fg: Color::WHITE,
        ascent: 12.0,
        active: false,
    });

    let state = FrameDisplayState::from_frame_glyph_buffer(&input);
    let output = state.materialize();

    assert_eq!(output.window_cursors.len(), 1);
    assert_eq!(output.window_cursors[0].cursor_fg, Color::WHITE);
}

#[test]
fn materialize_preserves_phys_cursor() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(11),
        charpos: 42,
        row: 3,
        col: 5,
        x: 80.0,
        y: 48.0,
        width: 9.0,
        height: 18.0,
        ascent: 13.0,
        style: CursorStyle::Hollow,
        color: Color::BLUE,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(11),
            row: 3,
            col: 5,
        },
        cursor_fg: Color::WHITE,
    });

    let buf = state.materialize();
    let phys = buf.active_cursor().expect("preserved active cursor");
    assert_eq!(phys.window_id.get(), 11);
    assert_eq!(phys.slot_id.row, 3);
    assert_eq!(phys.slot_id.col, 5);
    assert!(phys.active);
    assert_eq!(phys.style, CursorStyle::Hollow);
}

#[test]
fn materialize_includes_scroll_bars() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.scroll_bars.push(ScrollBarItem {
        window_id: DisplayWindowId::new(42),
        row_role: GlyphRowRole::Text,
        clip_rect: Some(Rect::new(0.0, 0.0, 640.0, 384.0)),
        horizontal: false,
        x: 632.0,
        y: 0.0,
        width: 8.0,
        height: 384.0,
        position: 10,
        portion: 50,
        whole: 200,
        thumb_start: 10.0,
        thumb_size: 50.0,
        track_color: Color::BLACK,
        thumb_color: Color::WHITE,
    });
    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 1);
    assert!(matches!(&buf.glyphs[0], FrameGlyph::ScrollBar { .. }));
}

#[test]
fn right_fringe_bitmap_column_stops_at_the_scroll_bar_not_the_window_edge() {
    // Regression: the right-fringe column used to span text_right..window_right,
    // swallowing the vertical scroll-bar column. Centering the arrow inside that
    // wide span dropped it behind the (opaque, later-drawn) bar, hiding every
    // truncation / continuation arrow whenever the window had a right scroll bar.
    // GNU keeps the scroll bar OUTSIDE the fringe (fringe.c draw_fringe_bitmap_1
    // positions bitmaps against window_box_right(TEXT_AREA)), so the column must
    // stop at the bar's left edge.
    let build = |with_bar: bool| -> FrameGlyph {
        let mut state = FrameDisplayState::new(4, 1, 8.0, 16.0);
        let window = DisplayWindowId::new(1);
        let mut matrix = GlyphMatrix::new(1, 4);
        let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
        row_0.enabled = true;
        row_0.height_px = 16.0;
        row_0.right_fringe_bitmap = Some(FringeBitmapInfo {
            bitmap_index: 4, // right-arrow (truncation)
            face_id: FaceId::new(0),
        });
        state.window_matrices.push(WindowMatrixEntry {
            window_id: window,
            matrix,
            // window [0,640]; text area [16,624] -> right fringe+bar span = 16px.
            pixel_bounds: Rect::new(0.0, 0.0, 640.0, 16.0),
            text_pixel_bounds: Rect::new(16.0, 0.0, 608.0, 16.0),
            text_clip_bounds: None,
            selected: true,
        });
        if with_bar {
            // Vertical bar occupying the outer 8px: [632,640].
            state.scroll_bars.push(ScrollBarItem {
                window_id: window,
                row_role: GlyphRowRole::Text,
                clip_rect: None,
                horizontal: false,
                x: 632.0,
                y: 0.0,
                width: 8.0,
                height: 16.0,
                position: 0,
                portion: 1,
                whole: 1,
                thumb_start: 0.0,
                thumb_size: 16.0,
                track_color: Color::BLACK,
                thumb_color: Color::WHITE,
            });
        }
        let mut fringes = Vec::new();
        state.for_each_glyph(|g| {
            if matches!(
                &g,
                FrameGlyph::FringeBitmap {
                    side: FringeSide::Right,
                    ..
                }
            ) {
                fringes.push(g);
            }
        });
        assert_eq!(fringes.len(), 1, "exactly one right-fringe glyph");
        fringes.pop().unwrap()
    };

    // With a right scroll bar: column starts at text_right (624) and stops at the
    // bar's left edge (632) — width 8, NOT 16 (which would run to the window edge
    // and hide the arrow under the bar).
    match build(true) {
        FrameGlyph::FringeBitmap { x, width, .. } => {
            assert_eq!(x, 624.0);
            assert_eq!(width, 8.0);
        }
        other => panic!("expected right FringeBitmap, got {other:?}"),
    }

    // Without a scroll bar: the column runs to the window edge (width 16).
    match build(false) {
        FrameGlyph::FringeBitmap { x, width, .. } => {
            assert_eq!(x, 624.0);
            assert_eq!(width, 16.0);
        }
        other => panic!("expected right FringeBitmap, got {other:?}"),
    }
}

#[test]
fn for_each_glyph_matches_materialize_glyphs() {
    // Build a state exercising several glyph kinds at once: a background, a
    // window row with both a Char and a Stretch slot, and a scroll bar. This
    // pins down that `for_each_glyph` walks the matrix in the exact same order
    // and with the exact same constructions as `materialize()` builds
    // `buf.glyphs`.
    let char_w = 8.0f32;
    let char_h = 16.0f32;
    let cols = 4;
    let mut state = FrameDisplayState::new(cols, 1, char_w, char_h);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    // One background (emits FrameGlyph::Background).
    state.backgrounds.push(BackgroundItem {
        bounds: Rect::new(0.0, 0.0, cols as f32 * char_w, char_h),
        color: Color::RED,
    });

    // One window matrix row: two chars then a 2-col stretch
    // (emits FrameGlyph::Char x2 and FrameGlyph::Stretch).
    let mut matrix = GlyphMatrix::new(1, cols);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('A', FaceId::new(0), 0));
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('B', FaceId::new(0), 1));
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::stretch(2, FaceId::new(0)));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * char_w, char_h),
        text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * char_w, char_h),
        text_clip_bounds: None,
        selected: true,
    });

    // One scroll bar (emits FrameGlyph::ScrollBar).
    state.scroll_bars.push(ScrollBarItem {
        window_id: DisplayWindowId::new(1),
        row_role: GlyphRowRole::Text,
        clip_rect: Some(Rect::new(0.0, 0.0, cols as f32 * char_w, char_h)),
        horizontal: false,
        x: 24.0,
        y: 0.0,
        width: 8.0,
        height: char_h,
        position: 10,
        portion: 50,
        whole: 200,
        thumb_start: 10.0,
        thumb_size: 50.0,
        track_color: Color::BLACK,
        thumb_color: Color::WHITE,
    });

    let buf = state.materialize();
    let mut walked = Vec::new();
    state.for_each_glyph(|g| walked.push(g));

    // Sanity: all four kinds actually appeared, so the comparison is meaningful.
    assert!(
        buf.glyphs
            .iter()
            .any(|g| matches!(g, FrameGlyph::Background { .. }))
    );
    assert!(
        buf.glyphs
            .iter()
            .any(|g| matches!(g, FrameGlyph::Char { .. }))
    );
    assert!(
        buf.glyphs
            .iter()
            .any(|g| matches!(g, FrameGlyph::Stretch { .. }))
    );
    assert!(
        buf.glyphs
            .iter()
            .any(|g| matches!(g, FrameGlyph::ScrollBar { .. }))
    );

    // FrameGlyph has no PartialEq, so compare via Debug strings.
    assert_eq!(format!("{:?}", buf.glyphs), format!("{:?}", walked));
}

#[test]
fn materialize_pixel_positions_from_grid() {
    let char_w = 10.0f32;
    let char_h = 20.0f32;
    let cols = 3;
    let rows = 2;
    let mut state = FrameDisplayState::new(cols, rows, char_w, char_h);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(2, cols);
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).enabled = true;
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]).enabled = true;
    // Row 0: "AB"
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('A', FaceId::new(0), 0));
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('B', FaceId::new(0), 1));
    // Row 1: "C"
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]).glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('C', FaceId::new(0), 2));

    let win_x = 5.0f32;
    let win_y = 3.0f32;
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(win_x, win_y, cols as f32 * char_w, rows as f32 * char_h),
        text_pixel_bounds: Rect::new(win_x, win_y, cols as f32 * char_w, rows as f32 * char_h),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 3);

    // Glyph 'A' at (win_x + 0*char_w, win_y + 0*char_h)
    match &buf.glyphs[0] {
        FrameGlyph::Char {
            char: ch,
            x,
            y,
            width,
            height,
            ..
        } => {
            assert_eq!(*ch, 'A');
            assert_eq!(*x, win_x);
            assert_eq!(*y, win_y);
            assert_eq!(*width, char_w);
            assert_eq!(*height, char_h);
        }
        other => panic!("expected Char, got {:?}", other),
    }

    // Glyph 'B' at (win_x + 1*char_w, win_y + 0*char_h)
    match &buf.glyphs[1] {
        FrameGlyph::Char { char: ch, x, y, .. } => {
            assert_eq!(*ch, 'B');
            assert_eq!(*x, win_x + char_w);
            assert_eq!(*y, win_y);
        }
        other => panic!("expected Char, got {:?}", other),
    }

    // Glyph 'C' at (win_x + 0*char_w, win_y + 1*char_h)
    match &buf.glyphs[2] {
        FrameGlyph::Char { char: ch, x, y, .. } => {
            assert_eq!(*ch, 'C');
            assert_eq!(*x, win_x);
            assert_eq!(*y, win_y + char_h);
        }
        other => panic!("expected Char, got {:?}", other),
    }
}

#[test]
fn materialize_preserves_char_bidi_level() {
    let mut state = FrameDisplayState::new(1, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 1);
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).enabled = true;
    let mut glyph = Glyph::char('א', FaceId::new(0), 1);
    glyph.bidi_level = 1;
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs[GlyphArea::Text as usize]
        .push(glyph);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 8.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 8.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    match &buf.glyphs[0] {
        FrameGlyph::Char {
            char: ch,
            bidi_level,
            ..
        } => {
            assert_eq!(*ch, 'א');
            assert_eq!(*bidi_level, 1);
            assert_eq!(buf.glyphs[0].bidi_level(), Some(1));
        }
        other => panic!("expected Char, got {:?}", other),
    }
}

#[test]
fn materialize_preserves_stretch_bidi_level() {
    let mut state = FrameDisplayState::new(4, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 4);
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).enabled = true;
    let mut glyph = Glyph::stretch(3, FaceId::new(0));
    glyph.bidi_level = 1;
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs[GlyphArea::Text as usize]
        .push(glyph);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 32.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 32.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    match &buf.glyphs[0] {
        FrameGlyph::Stretch {
            bidi_level, width, ..
        } => {
            assert_eq!(*bidi_level, 1);
            assert_eq!(*width, 24.0);
            assert_eq!(buf.glyphs[0].bidi_level(), Some(1));
        }
        other => panic!("expected Stretch, got {:?}", other),
    }
}

#[test]
fn materialize_uses_explicit_row_metrics() {
    let mut state = FrameDisplayState::new(2, 1, 10.0, 20.0);
    let mut face = Face::new(FaceId::new(0));
    face.font_ascent = 14;
    state.faces.insert(FaceId::new(0), face);

    let mut matrix = GlyphMatrix::new(1, 2);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.pixel_y = 7.0;
    row_0.height_px = 18.0;
    row_0.ascent_px = 13.0;
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('A', FaceId::new(0), 0));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(5.0, 3.0, 20.0, 18.0),
        text_pixel_bounds: Rect::new(5.0, 3.0, 20.0, 18.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Char {
            char,
            x,
            y,
            baseline,
            height,
            ascent,
            ..
        } => {
            assert_eq!(*char, 'A');
            assert_eq!(*x, 5.0);
            assert_eq!(*y, 10.0);
            assert_eq!(*baseline, 23.0);
            assert_eq!(*height, 18.0);
            assert_eq!(*ascent, 14.0);
        }
        other => panic!("expected Char, got {:?}", other),
    }
}

#[test]
fn materialize_applies_glyph_vertical_offset_to_char_baseline() {
    let mut state = FrameDisplayState::new(2, 1, 10.0, 20.0);
    let mut matrix = GlyphMatrix::new(1, 2);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.height_px = 20.0;
    row_0.ascent_px = 15.0;
    row_0.glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('A', FaceId::new(0), 0).with_vertical_offset(-4.0));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Char { baseline, .. } => {
            assert_eq!(*baseline, 11.0);
        }
        other => panic!("expected Char, got {:?}", other),
    }
}

#[test]
fn materialize_copies_metadata() {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.frame_placement = crate::PresentedFramePlacement::new(
        DisplayFrameId::new(123),
        state.presentation_id,
        Some(DisplayFrameId::new(456)),
        crate::ParentFrameRect::new(
            10.0,
            20.0,
            state.frame_pixel_width,
            state.frame_pixel_height,
        )
        .unwrap(),
        5,
    );
    state.background = Color::BLUE;

    let mut face = Face::new(FaceId::new(1));
    face.foreground = Color::RED;
    state.faces.insert(FaceId::new(1), face);

    let buf = state.materialize();
    assert_eq!(buf.frame_placement.frame().get(), 123);
    assert_eq!(buf.frame_placement.parent().unwrap().get(), 456);
    assert_eq!(buf.frame_placement.outer_in_parent().x(), 10.0);
    assert_eq!(buf.frame_placement.outer_in_parent().y(), 20.0);
    assert_eq!(buf.frame_placement.z_order(), 5);
    assert_eq!(buf.background, Color::BLUE);
    assert!(buf.faces.contains_key(&FaceId::new(1)));
    assert_eq!(buf.faces[&FaceId::new(1)].foreground, Color::RED);
}

#[test]
fn materialize_disabled_rows_are_skipped() {
    let mut state = FrameDisplayState::new(3, 2, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(2, 3);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('A', FaceId::new(0), 0));
    // Row 1 stays disabled (default), so its glyph is filtered out.
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]).glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('B', FaceId::new(0), 1));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 24.0, 32.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 24.0, 32.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    // Only row 0's glyph should be materialized
    assert_eq!(buf.glyphs.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Char { char: ch, .. } => assert_eq!(*ch, 'A'),
        other => panic!("expected Char, got {:?}", other),
    }
}

#[test]
fn materialize_padding_glyphs_are_skipped() {
    let mut state = FrameDisplayState::new(4, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 4);
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).enabled = true;
    // Wide char 'W' followed by padding
    let mut wide_glyph = Glyph::char('W', FaceId::new(0), 0);
    wide_glyph.wide = true;
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.glyphs[GlyphArea::Text as usize].push(wide_glyph);
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::padding_for(FaceId::new(0), 0));
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('x', FaceId::new(0), 1));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 32.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 32.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    // Should have 2 visible glyphs: wide 'W' and 'x'; padding is skipped
    assert_eq!(buf.glyphs.len(), 2);
    match &buf.glyphs[0] {
        FrameGlyph::Char {
            char: ch, width, ..
        } => {
            assert_eq!(*ch, 'W');
            assert_eq!(*width, 16.0); // 2 * char_w for wide
        }
        other => panic!("expected wide Char, got {:?}", other),
    }
    match &buf.glyphs[1] {
        FrameGlyph::Char { char: ch, x, .. } => {
            assert_eq!(*ch, 'x');
            // col = 2 (wide took 2 cols), so x = 2 * 8.0 = 16.0
            assert_eq!(*x, 16.0);
        }
        other => panic!("expected Char, got {:?}", other),
    }
}

#[test]
fn materialize_uses_realized_pixel_width_for_text_positions() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 10);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('N', FaceId::new(0), 0).with_pixel_width(13.0));
    crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs[GlyphArea::Text as usize]
        .push(Glyph::char('E', FaceId::new(0), 1).with_pixel_width(12.0));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 2);
    match (&buf.glyphs[0], &buf.glyphs[1]) {
        (
            FrameGlyph::Char {
                char: first,
                x: first_x,
                width: first_width,
                ..
            },
            FrameGlyph::Char {
                char: second,
                x: second_x,
                width: second_width,
                ..
            },
        ) => {
            assert_eq!((*first, *second), ('N', 'E'));
            assert_eq!(*first_x, 0.0);
            assert_eq!(*first_width, 13.0);
            assert_eq!(*second_x, 13.0);
            assert_eq!(*second_width, 12.0);
        }
        other => panic!("expected two chars, got {:?}", other),
    }
}

#[test]
fn chrome_completion_moves_box_run_terminal_to_final_stretch() {
    use crate::face::{BoxLineWidth, BoxType, BoxVerticalEdges};

    let face_id = FaceId::new(1);
    let mut state = FrameDisplayState::new(3, 1, 8.0, 16.0);
    let mut face = Face::new(face_id);
    face.box_type = BoxType::Line;
    face.box_line_width = BoxLineWidth::from_gnu(1);
    state.faces.insert(face_id, face);

    let mut matrix = GlyphMatrix::new(1, 3);
    let row = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row.enabled = true;
    row.role = GlyphRowRole::ModeLine;
    let mut glyph = Glyph::char('x', face_id, 0).with_pixel_width(8.0);
    glyph.box_vertical_edges = BoxVerticalEdges::Both;
    row.glyphs[GlyphArea::Text.index()].push(glyph);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 24.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 24.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let glyphs = state.materialize().glyphs;
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0].box_vertical_edges(), Some(BoxVerticalEdges::Left));
    assert!(matches!(glyphs[1], FrameGlyph::Stretch { .. }));
    assert_eq!(
        glyphs[1].box_vertical_edges(),
        Some(BoxVerticalEdges::Right)
    );
}

#[test]
fn materialize_clips_overlong_window_rows_to_pixel_bounds() {
    let mut state = FrameDisplayState::new(6, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 3);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.role = GlyphRowRole::ModeLine;
    row_0.mode_line = true;
    for (idx, ch) in "abcdef".chars().enumerate() {
        crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs
            [GlyphArea::Text as usize]
            .push(Glyph::char(ch, FaceId::new(0), idx));
    }

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 24.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 24.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    let chars: Vec<(char, f32, f32)> = buf
        .glyphs
        .iter()
        .filter_map(|glyph| match glyph {
            FrameGlyph::Char {
                char: ch, x, width, ..
            } => Some((*ch, *x, *width)),
            _ => None,
        })
        .collect();

    assert_eq!(
        chars,
        vec![('a', 0.0, 8.0), ('b', 8.0, 8.0), ('c', 16.0, 8.0)]
    );
    assert!(
        buf.glyphs.iter().all(|glyph| match glyph {
            FrameGlyph::Char { x, width, .. } | FrameGlyph::Stretch { x, width, .. } =>
                *x + *width <= 24.0,
            _ => true,
        }),
        "materialized row glyphs must stay inside their owning window"
    );
}

#[test]
fn materialize_text_rows_from_text_area_but_chrome_from_window_area() {
    let mut state = FrameDisplayState::new(10, 2, 8.0, 16.0);
    let mut matrix = GlyphMatrix::new(2, 4);

    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.role = GlyphRowRole::Text;
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('t', FaceId::new(0), 0));

    let row_1 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]);
    row_1.enabled = true;
    row_1.role = GlyphRowRole::ModeLine;
    row_1.glyphs[GlyphArea::Text as usize].push(Glyph::char('m', FaceId::new(0), 1));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 32.0),
        text_pixel_bounds: Rect::new(8.0, 0.0, 64.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    let text = buf
        .glyphs
        .iter()
        .find(|glyph| matches!(glyph, FrameGlyph::Char { char: 't', .. }))
        .expect("text glyph");
    let chrome = buf
        .glyphs
        .iter()
        .find(|glyph| matches!(glyph, FrameGlyph::Char { char: 'm', .. }))
        .expect("mode-line glyph");

    assert!(matches!(
        text,
        FrameGlyph::Char {
            x: 8.0,
            width: 16.0,
            ..
        }
    ));
    assert!(matches!(
        chrome,
        FrameGlyph::Char {
            x: 0.0,
            width: 20.0,
            ..
        }
    ));
}

#[test]
fn text_area_clip_rect_narrows_to_band_between_chrome_rows() {
    // Window 32x100 with a 20px header line on top and a 20px mode line on the
    // bottom: the buffer-text clip band is the 60px in between.  A vscroll shifts
    // the buffer rows but NOT the chrome anchors, so the band stays put.
    let mut matrix = GlyphMatrix::new(3, 4);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.role = GlyphRowRole::HeaderLine;
    row_0.pixel_y = 0.0;
    row_0.height_px = 20.0;
    // A vscroll'd buffer row whose top pokes up into the header band.
    let row_1 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]);
    row_1.enabled = true;
    row_1.role = GlyphRowRole::Text;
    row_1.pixel_y = 15.0;
    row_1.height_px = 20.0;
    let row_2 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[2]);
    row_2.enabled = true;
    row_2.role = GlyphRowRole::ModeLine;
    row_2.pixel_y = 80.0;
    row_2.height_px = 20.0;

    let entry = WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 32.0, 100.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 32.0, 100.0),
        text_clip_bounds: None,
        selected: true,
    };

    // header bottom (20) .. mode-line top (80).
    assert_eq!(
        entry.text_area_clip_rect(),
        Rect::new(0.0, 20.0, 32.0, 60.0)
    );
}

#[test]
fn text_area_clip_rect_equals_text_pixel_bounds_without_chrome() {
    // With no header/tab/mode-line rows the band spans the full window height at
    // the text-area's horizontal extent -- byte-identical to the historical
    // `Some(text_pixel_bounds)` clip, so windows without chrome are unaffected.
    let mut matrix = GlyphMatrix::new(1, 4);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.role = GlyphRowRole::Text;
    row_0.pixel_y = 0.0;
    row_0.height_px = 16.0;

    // Production invariant: text_pixel_bounds spans the full window height with
    // y == window top; only x/width are inset by fringes/margins.
    let text_pixel_bounds = Rect::new(8.0, 0.0, 64.0, 32.0);
    let entry = WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 32.0),
        text_pixel_bounds,
        text_clip_bounds: None,
        selected: true,
    };

    assert_eq!(entry.text_area_clip_rect(), text_pixel_bounds);
}

#[test]
fn materialize_clips_vscrolled_text_row_to_text_band() {
    // End-to-end: a Text row shifted UP by a vscroll (its top pokes above the
    // header line) is materialized with the text-area BAND as its clip_rect, so
    // the renderer's per-glyph vertical clip hides the overflow instead of
    // letting it bleed over the header/mode-line chrome.
    let mut state = FrameDisplayState::new(32, 100, 8.0, 20.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(3, 4);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.role = GlyphRowRole::HeaderLine;
    row_0.pixel_y = 0.0;
    row_0.height_px = 20.0;
    row_0.glyphs[GlyphArea::Text as usize].push(Glyph::char('h', FaceId::new(0), 0));
    // vscroll'd buffer row: top at y=15, inside the header band (0..20).
    let row_1 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]);
    row_1.enabled = true;
    row_1.role = GlyphRowRole::Text;
    row_1.pixel_y = 15.0;
    row_1.height_px = 20.0;
    row_1.glyphs[GlyphArea::Text as usize].push(Glyph::char('x', FaceId::new(0), 0));
    let row_2 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[2]);
    row_2.enabled = true;
    row_2.role = GlyphRowRole::ModeLine;
    row_2.pixel_y = 80.0;
    row_2.height_px = 20.0;
    row_2.glyphs[GlyphArea::Text as usize].push(Glyph::char('m', FaceId::new(0), 0));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 32.0, 100.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 32.0, 100.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    let band = Rect::new(0.0, 20.0, 32.0, 60.0);

    // The buffer glyph clips to the text-area band, and its top (y=15) sits
    // ABOVE the band top (20): the renderer's vertical clip hides the 5px that
    // would otherwise overlap the header line.
    let (text_clip, text_y) = buf
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Char {
                char: 'x',
                clip_rect,
                y,
                ..
            } => Some((*clip_rect, *y)),
            _ => None,
        })
        .expect("text glyph 'x'");
    assert_eq!(text_clip, Some(band));
    assert!(
        text_y < band.y,
        "vscroll'd row top {text_y} must be above band top {}",
        band.y
    );

    // Chrome glyphs use their measured row as the authoritative clip inside
    // the window partition, so tall content cannot bleed into adjacent bands.
    let header_clip = buf
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Char {
                char: 'h',
                clip_rect,
                ..
            } => Some(*clip_rect),
            _ => None,
        })
        .expect("header glyph 'h'");
    assert_eq!(header_clip, Some(Rect::new(0.0, 0.0, 32.0, 20.0)));
}

#[test]
fn materialize_stretch_glyph() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 10);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.glyphs[GlyphArea::Text as usize].push(
        Glyph::stretch(4, FaceId::new(0))
            .with_box_vertical_edges(crate::face::BoxVerticalEdges::Neither),
    );

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    assert_eq!(buf.glyphs.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Stretch {
            width,
            height,
            box_vertical_edges,
            ..
        } => {
            assert_eq!(*width, 4.0 * 8.0); // 4 cols * 8px
            assert_eq!(*height, 16.0);
            assert_eq!(*box_vertical_edges, crate::face::BoxVerticalEdges::Neither);
        }
        other => panic!("expected Stretch, got {:?}", other),
    }
}

#[test]
fn materialize_stretch_paint_uses_row_geometry_after_explicit_layout_metrics() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 10);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.height_px = 30.0;
    row_0.ascent_px = 20.0;
    row_0.glyphs[GlyphArea::Text as usize]
        .push(Glyph::stretch(4, FaceId::new(0)).with_pixel_geometry(24.0, 12.0, 5.0));

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 10.0, 80.0, 40.0),
        text_pixel_bounds: Rect::new(0.0, 10.0, 80.0, 40.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    match &buf.glyphs[0] {
        FrameGlyph::Stretch {
            y, width, height, ..
        } => {
            assert_eq!(*y, 10.0);
            assert_eq!(*width, 24.0);
            assert_eq!(*height, 30.0);
        }
        other => panic!("expected Stretch, got {:?}", other),
    }
}

#[test]
fn materialize_stretch_face_paints_the_full_mixed_height_row() {
    let mut state = FrameDisplayState::new(10, 1, 8.0, 16.0);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));

    let mut matrix = GlyphMatrix::new(1, 10);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.pixel_y = 4.0;
    row_0.height_px = 20.0;
    row_0.ascent_px = 15.0;
    row_0.glyphs[GlyphArea::Text as usize].extend([
        Glyph::char('\u{f401}', FaceId::new(0), 0).with_pixel_geometry(7.0, 16.0, 12.0),
        Glyph::stretch(2, FaceId::new(0)).with_pixel_geometry(10.0, 13.0, 10.0),
        Glyph::char('n', FaceId::new(0), 2).with_pixel_geometry(9.0, 20.0, 15.0),
    ]);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 20.0, 80.0, 40.0),
        text_pixel_bounds: Rect::new(0.0, 20.0, 80.0, 40.0),
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    match &buf.glyphs[1] {
        FrameGlyph::Stretch {
            x,
            y,
            width,
            height,
            ..
        } => {
            assert_eq!(*x, 7.0);
            assert_eq!(*width, 10.0);
            assert_eq!(*y, 24.0, "GNU stretch backgrounds start at row->y");
            assert_eq!(
                *height, 20.0,
                "GNU stretch backgrounds use row->height, not glyph height"
            );
        }
        other => panic!("expected Stretch, got {other:?}"),
    }
}

#[test]
fn materialize_new_fields_default_to_empty() {
    let state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    assert!(state.backgrounds.is_empty());
    assert!(state.borders.is_empty());
    assert!(state.cursors.is_empty());
    assert!(state.scroll_bars.is_empty());
}

#[test]
fn frame_chrome_materializes_nonzero_tab_origin_once() {
    use crate::frame_chrome::{
        BandRect, ChromeAction, ChromeBandRequest, ChromeDisplayRow, ChromeHitRegion, FrameChrome,
        FrameChromeContent, FrameChromeKind, FrameSize, MenuBarContent, ToolBarContent,
    };

    let mut state = FrameDisplayState::new(80, 36, 7.8, 18.0);
    state.frame_pixel_width = 624.0;
    state.frame_pixel_height = 648.0;

    let mut tab_row = GlyphRow::new(GlyphRowRole::TabBar);
    tab_row.enabled = true;
    tab_row.pixel_y = 52.0;
    tab_row.height_px = 18.0;
    tab_row.ascent_px = 14.0;
    tab_row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('T', FaceId::new(0), 0).with_pixel_width(7.8));
    tab_row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::stretch(2, FaceId::new(0)).with_pixel_geometry(16.0, 18.0, 14.0));
    let image_margins = tab_row
        .intern_image_margins(crate::ImageMargins::new(2.0, 1.0))
        .expect("image-margin token");
    let mut image = Glyph::char(' ', FaceId::new(0), 1);
    image.glyph_type = GlyphType::Image {
        source_rect: crate::ImageSourceRect::FULL,
        image_id: 77,
        width_cols: 2,
        margins: image_margins,
        opaque_background: crate::ImageOpaqueBackground::default(),
    };
    image.box_vertical_edges = crate::face::BoxVerticalEdges::Left;
    tab_row.glyphs[GlyphArea::Text.index()].push(image.with_pixel_geometry(16.0, 16.0, 14.0));

    let tab_content = ChromeDisplayRow::new(tab_row);
    assert_eq!(tab_content.row().pixel_y, 0.0);

    state.frame_chrome = FrameChrome::layout(
        FrameSize::new(624.0, 648.0).expect("valid frame"),
        vec![
            ChromeBandRequest::new(
                FrameChromeKind::MenuBar,
                18.0,
                FrameChromeContent::MenuBar(MenuBarContent::empty()),
            ),
            ChromeBandRequest::new(
                FrameChromeKind::ToolBar,
                34.0,
                FrameChromeContent::ToolBar(ToolBarContent::empty()),
            ),
            ChromeBandRequest::new(
                FrameChromeKind::TabBar,
                18.0,
                FrameChromeContent::DisplayRow(tab_content),
            )
            .with_hit_regions(vec![ChromeHitRegion::new(
                BandRect::new(0.0, 0.0, 24.0, 18.0).expect("local hit bounds"),
                ChromeAction::Presented {
                    interaction: crate::frame_chrome::InteractionId::new(0),
                },
            )]),
        ],
    )
    .expect("valid chrome");

    let materialized = state.materialize();
    let tab_char = materialized
        .glyphs
        .iter()
        .find(|glyph| {
            matches!(
                glyph,
                FrameGlyph::Char {
                    row_role: GlyphRowRole::TabBar,
                    ..
                }
            )
        })
        .expect("tab character");
    let tab_stretch = materialized
        .glyphs
        .iter()
        .find(|glyph| {
            matches!(
                glyph,
                FrameGlyph::Stretch {
                    row_role: GlyphRowRole::TabBar,
                    ..
                }
            )
        })
        .expect("tab stretch");
    let tab_image = materialized
        .glyphs
        .iter()
        .find(|glyph| {
            matches!(
                glyph,
                FrameGlyph::Image {
                    row_role: GlyphRowRole::TabBar,
                    image_id,
                    ..
                } if *image_id == ImageId::new(77)
            )
        })
        .expect("tab image");

    assert_eq!(tab_char.geometry().map(|rect| rect.y), Some(52.0));
    assert_eq!(tab_stretch.geometry().map(|rect| rect.y), Some(52.0));
    assert_eq!(
        tab_image.geometry(),
        Some(Rect::new(25.8, 53.0, 12.0, 14.0))
    );
    assert_eq!(tab_image.cell_rect(), Some((23.8, 52.0, 16.0, 16.0)));
    assert_eq!(tab_image.face_id(), Some(FaceId::new(0)));
    assert_eq!(
        tab_image.box_vertical_edges(),
        Some(crate::face::BoxVerticalEdges::Left)
    );
    assert_eq!(
        tab_char.clip_rect(),
        Some(Rect::new(0.0, 52.0, 624.0, 18.0))
    );

    let tab_band = materialized
        .frame_chrome
        .band(FrameChromeKind::TabBar)
        .expect("tab band");
    let hits = tab_band.materialized_hit_regions().expect("valid hits");
    assert_eq!(hits[0].bounds().y(), 52.0);
}

#[test]
fn frame_tab_bar_materialization_does_not_invent_a_trailing_face() {
    use crate::frame_chrome::{
        ChromeBandRequest, ChromeDisplayRow, FrameChrome, FrameChromeContent, FrameChromeKind,
        FrameSize,
    };

    let mut state = FrameDisplayState::new(10, 2, 8.0, 16.0);
    state.frame_pixel_width = 80.0;
    state.frame_pixel_height = 32.0;

    let blue_face_id = FaceId::new(20);
    let mut blue_face = Face::new(blue_face_id);
    blue_face.background = Color::from_pixel(0x000000ff);
    state.faces.insert(blue_face_id, blue_face);

    let mut row = GlyphRow::new(GlyphRowRole::TabBar);
    row.enabled = true;
    row.height_px = 16.0;
    row.ascent_px = 12.0;
    row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('x', blue_face_id, 0).with_pixel_width(8.0));
    state.frame_chrome = FrameChrome::layout(
        FrameSize::new(80.0, 32.0).expect("valid frame"),
        vec![ChromeBandRequest::new(
            FrameChromeKind::TabBar,
            16.0,
            FrameChromeContent::DisplayRow(ChromeDisplayRow::new(row)),
        )],
    )
    .expect("valid chrome");

    let materialized = state.materialize();
    let tab_bar_glyphs: Vec<_> = materialized
        .glyphs
        .iter()
        .filter(|glyph| glyph.row_role() == Some(GlyphRowRole::TabBar))
        .collect();

    assert_eq!(
        tab_bar_glyphs.len(),
        1,
        "materialization must preserve the complete frame-tab-bar scene instead of guessing a tail fill"
    );
    assert!(matches!(tab_bar_glyphs[0], FrameGlyph::Char { .. }));
}

#[test]
fn materialize_mixed_grid_and_nongrid_items() {
    let mut state = state_with_text("Hi");

    // Add one background and one cursor
    state.backgrounds.push(BackgroundItem {
        bounds: Rect::new(0.0, 0.0, 16.0, 16.0),
        color: Color::BLACK,
    });
    state.cursors.push(CursorItem {
        window_id: DisplayWindowId::new(1),
        role: CursorItemRole::Decorative,
        slot_id: DisplaySlotId::from_pixels(
            DisplayWindowId::new(1),
            Px(0.0),
            Px(0.0),
            Px(8.0),
            Px(16.0),
        ),
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
        ascent: 12.0,
    });

    let buf = state.materialize();
    // 1 background + 2 chars = 3 glyphs, plus 1 decorative window cursor
    assert_eq!(buf.glyphs.len(), 3);
    assert_eq!(buf.window_cursors.len(), 1);

    // Backgrounds come first
    assert!(matches!(&buf.glyphs[0], FrameGlyph::Background { .. }));
    // Then grid chars
    assert!(matches!(&buf.glyphs[1], FrameGlyph::Char { .. }));
    assert!(matches!(&buf.glyphs[2], FrameGlyph::Char { .. }));
    assert_eq!(buf.window_cursors[0].style, CursorStyle::FilledBox);
}

#[test]
fn materialize_emits_left_fringe_bitmap_glyph_from_row() {
    // A buffer-text row that carries a left-fringe bitmap (magit section-heading
    // fold arrow) must materialize one FrameGlyph::FringeBitmap positioned in the
    // window's left fringe column (between the window edge and the text area),
    // with the row's bitmap index and resolved face id.
    use crate::frame_glyphs::{FringeBitmapData, FringeSide};
    use crate::glyph_matrix::FringeBitmapInfo;

    let char_w = 8.0f32;
    let char_h = 16.0f32;
    let cols = 4;
    let left_fringe = 8.0f32;
    // Window spans from x=10; the text area starts after an 8px left fringe.
    let win = Rect::new(10.0, 20.0, left_fringe + cols as f32 * char_w, char_h);
    let text_area = Rect::new(10.0 + left_fringe, 20.0, cols as f32 * char_w, char_h);

    let mut state = FrameDisplayState::new(cols, 1, char_w, char_h);
    state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));
    state
        .faces
        .insert(FaceId::new(7), Face::new(FaceId::new(7)));

    // Register the bitmap bits once per frame.
    state.fringe_bitmaps.insert(
        25,
        FringeBitmapData {
            bits: vec![0x6000, 0x3000, 0x1800, 0x0C00],
            width: 8,
            height: 4,
            period: 0,
            align: 0,
        },
    );

    let mut matrix = GlyphMatrix::new(1, cols);
    let row_0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row_0.enabled = true;
    row_0.height_px = char_h;
    row_0.pixel_y = 0.0;
    row_0.left_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 25,
        face_id: FaceId::new(7),
    });

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: win,
        text_pixel_bounds: text_area,
        text_clip_bounds: None,
        selected: true,
    });

    let buf = state.materialize();
    let fringes: Vec<_> = buf
        .glyphs
        .iter()
        .filter(|g| matches!(g, FrameGlyph::FringeBitmap { .. }))
        .collect();
    assert_eq!(fringes.len(), 1, "exactly one fringe bitmap glyph");
    match fringes[0] {
        FrameGlyph::FringeBitmap {
            x,
            y,
            width,
            height,
            bitmap_index,
            face_id,
            side,
            ..
        } => {
            assert_eq!(*bitmap_index, 25);
            assert_eq!(*face_id, FaceId::new(7));
            assert_eq!(*side, FringeSide::Left);
            // Fringe column: from window left edge to text area left edge.
            assert_eq!(*x, 10.0);
            assert_eq!(*width, left_fringe);
            assert_eq!(*y, 20.0);
            assert_eq!(*height, char_h);
        }
        other => panic!("expected FringeBitmap, got {other:?}"),
    }

    // The bits round-trip into the materialized buffer.
    assert!(buf.fringe_bitmaps.contains_key(&25));
}

// ---------------------------------------------------------------------------
// serde snapshot serialization tests
// ---------------------------------------------------------------------------

/// The frame snapshot contract: serializing the real `FrameDisplayState` must
/// be lossless (serialize → deserialize → serialize is a fixed point) so the
/// JSON artifact is a faithful display oracle.
#[test]
fn frame_display_state_serde_round_trip() {
    let state = state_with_text("hello serde");
    let json = serde_json::to_string(&state).expect("serialize");
    let back: FrameDisplayState = serde_json::from_str(&json).expect("deserialize");
    let json2 = serde_json::to_string(&back).expect("re-serialize");
    assert_eq!(json, json2, "serde round-trip must be lossless");
    // Each Char glyph serializes individually: {"Char":{"ch":"h"}}.
    assert!(
        json.contains(r#""ch":"h""#),
        "glyph chars must appear in JSON: {json}"
    );
}

/// Integer-keyed maps (faces, fringe bitmaps, per-window effects) must
/// survive JSON, where keys are strings.
#[test]
fn frame_display_state_integer_map_keys_round_trip() {
    let mut state = state_with_text("k");
    state
        .faces
        .insert(FaceId::new(42), Face::new(FaceId::new(42)));
    state
        .cursor_effects_by_window
        .insert(crate::types::DisplayWindowId::new(7), Default::default());
    let json = serde_json::to_string(&state).expect("serialize");
    let back: FrameDisplayState = serde_json::from_str(&json).expect("deserialize");
    assert!(back.faces.contains_key(&FaceId::new(42)));
    assert!(
        back.cursor_effects_by_window
            .contains_key(&crate::types::DisplayWindowId::new(7))
    );
}

/// The resolved font table and `Face::default_resolved_font_id` must survive
/// both directions of the frame IR conversion (grid state -> glyph buffer ->
/// grid state) and JSON snapshots, so the render thread always sees the exact
/// fonts layout resolved.
#[test]
fn resolved_fonts_survive_materialize_and_round_trip() {
    use crate::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontResolutionSource, FontSlantKind,
        ResolvedFont, ResolvedFontId, ResolvedFontIdentity,
    };

    let mut state = state_with_text("f");
    let font_id = ResolvedFontId(3);
    let mut face = Face::new(FaceId::new(0));
    face.default_resolved_font_id = Some(font_id);
    state.faces.insert(FaceId::new(0), face);
    state.fonts.insert(
        font_id,
        ResolvedFont {
            id: font_id,
            identity: ResolvedFontIdentity::from_file("/fonts/mono.ttf", 0, None),
            replay: FontReplay::Swash {
                asset: FontOutlineAsset::File(FontFileAsset::new("/fonts/mono.ttf", 0).unwrap()),
            },
            family: "Mono".to_string(),
            full_name: None,
            postscript_name: None,
            weight: 400,
            slant: FontSlantKind::Normal,
            width: 5,
            pixel_size: 15.0,
            ascent_px: 12.0,
            descent_px: 3.0,
            space_advance_px: 8.0,
            glyph_advance: Default::default(),
            source: FontResolutionSource::FacePrimary,
        },
    );

    // grid -> buffer
    let buf = state.materialize();
    assert_eq!(buf.fonts.get(&font_id), state.fonts.get(&font_id));
    assert_eq!(
        buf.faces
            .get(&FaceId::new(0))
            .unwrap()
            .default_resolved_font_id,
        Some(font_id)
    );

    // buffer -> grid
    let back = FrameDisplayState::from_frame_glyph_buffer(&buf);
    assert_eq!(back.fonts.get(&font_id), state.fonts.get(&font_id));

    // JSON snapshot
    let json = serde_json::to_string(&state).expect("serialize");
    let parsed: FrameDisplayState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.fonts.get(&font_id), state.fonts.get(&font_id));
    assert_eq!(
        parsed
            .faces
            .get(&FaceId::new(0))
            .unwrap()
            .default_resolved_font_id,
        Some(font_id)
    );
}

/// `char_fonts` (per-char fallback font table) must survive materialize and
/// JSON snapshots like the faces/fonts tables do.
#[test]
fn char_fonts_survive_materialize_and_serde() {
    use crate::font::{ResolvedCharGlyph, ResolvedFontId, ResolvedGlyphId};

    let expected = ResolvedCharGlyph {
        resolved_font_id: ResolvedFontId(3),
        glyph_id: ResolvedGlyphId::new(91_000),
        advance_px: 12.5,
    };

    let mut state = state_with_text("x");
    state
        .char_fonts
        .entry(FaceId::new(7))
        .or_default()
        .insert('好', expected);

    let buf = state.materialize();
    assert_eq!(
        buf.char_fonts
            .get(&FaceId::new(7))
            .and_then(|m| m.get(&'好')),
        Some(&expected)
    );

    let back = FrameDisplayState::from_frame_glyph_buffer(&buf);
    assert_eq!(
        back.char_fonts
            .get(&FaceId::new(7))
            .and_then(|m| m.get(&'好')),
        Some(&expected)
    );

    let json = serde_json::to_string(&state).expect("serialize");
    let parsed: FrameDisplayState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        parsed
            .char_fonts
            .get(&FaceId::new(7))
            .and_then(|m| m.get(&'好')),
        Some(&expected)
    );
}

/// `shaped_clusters` must survive materialize and JSON snapshots like the
/// other font tables.
#[test]
fn shaped_clusters_survive_materialize_and_serde() {
    use crate::font::{ResolvedFontId, ResolvedGlyph};

    let glyphs = vec![ResolvedGlyph {
        resolved_font_id: ResolvedFontId(4),
        glyph_id: crate::font::ResolvedGlyphId::new(99),
        x: 0.0,
        y: 0.0,
        x_advance: 8.5,
        cluster_start: 0,
        cluster_end: 3,
    }];
    let mut state = state_with_text("x");
    state
        .shaped_clusters
        .entry(FaceId::new(2))
        .or_default()
        .insert("e\u{301}".into(), glyphs.clone());

    let buf = state.materialize();
    assert_eq!(
        buf.shaped_clusters
            .get(&FaceId::new(2))
            .and_then(|m| m.get("e\u{301}")),
        Some(&glyphs)
    );
    let back = FrameDisplayState::from_frame_glyph_buffer(&buf);
    assert_eq!(
        back.shaped_clusters
            .get(&FaceId::new(2))
            .and_then(|m| m.get("e\u{301}")),
        Some(&glyphs)
    );
    let json = serde_json::to_string(&state).expect("serialize");
    let parsed: FrameDisplayState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        parsed
            .shaped_clusters
            .get(&FaceId::new(2))
            .and_then(|m| m.get("e\u{301}")),
        Some(&glyphs)
    );
}

#[test]
fn semantic_hit_index_survives_transport_and_materialization() {
    let presentation = PresentationId::new(19);
    let window = crate::DisplayWindowId::new(4);
    let mut state = FrameDisplayState::new(20, 10, 8.0, 16.0);
    state.presentation_id = presentation;
    install_complete_window_geometry(&mut state, window, Rect::new(8.0, 16.0, 80.0, 32.0));
    state.presented_hit_index = crate::PresentedHitIndex::from_parts(
        presentation,
        vec![crate::PresentedHitRegion::new(
            Some(window),
            crate::PresentedRegionKind::TextBody,
            crate::FrameRect::new(8.0, 16.0, 80.0, 32.0).unwrap(),
            0,
        )],
        vec![crate::PresentedTextPosition::new(
            window,
            crate::FrameRect::new(8.0, 16.0, 8.0, 16.0).unwrap(),
            42,
            0,
            0,
        )],
    )
    .unwrap();

    let wire = serde_json::to_string(&state).unwrap();
    let decoded: FrameDisplayState = serde_json::from_str(&wire).unwrap();
    let frame = decoded.materialize();
    let hit = frame
        .presented_hit_index()
        .resolve(crate::PresentedHitQuery::new(settled_point(
            presentation,
            10.0,
            20.0,
        )))
        .unwrap()
        .unwrap();
    assert_eq!(hit.region().kind(), crate::PresentedRegionKind::TextBody);
    assert_eq!(hit.text_position().unwrap().buffer_position(), 42);

    let round_trip = FrameDisplayState::from_frame_glyph_buffer(&frame);
    assert_eq!(round_trip.presented_hit_index, state.presented_hit_index);
}

#[test]
fn glyph_provenance_keeps_string_and_buffer_coordinates_disjoint() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let string = GlyphStringId::new(7);
    let covered = GlyphStringBufferRange::new(20, 24);
    let source = row
        .push_string_source(GlyphStringSource::replacement(string, covered))
        .expect("row-local string source");
    let provenance = GlyphProvenance::string(source, 2);

    assert_eq!(provenance.string_index(), Some((source, 2)));
    assert_eq!(provenance.buffer_charpos(), None);
    let source = row
        .string_source(source)
        .expect("source resolves in its row");
    assert_eq!(source.string(), string);
    assert_eq!(source.covered_buffer_range(), Some(covered));
    assert!(source.covers_buffer_charpos(22));
    assert!(!source.covers_buffer_charpos(24));
}

#[test]
fn identical_string_objects_can_have_distinct_row_occurrences() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let string = GlyphStringId::new(7);
    let first = row
        .push_string_source(GlyphStringSource::replacement(
            string,
            GlyphStringBufferRange::new(10, 12),
        ))
        .expect("first occurrence");
    let second = row
        .push_string_source(GlyphStringSource::replacement(
            string,
            GlyphStringBufferRange::new(20, 22),
        ))
        .expect("second occurrence");

    assert_ne!(first, second);
    assert_eq!(
        row.string_source(first).map(|source| source.string()),
        Some(string)
    );
    assert_eq!(
        row.string_source(second).map(|source| source.string()),
        Some(string)
    );
    assert!(
        row.string_source(first)
            .is_some_and(|source| source.covers_buffer_charpos(10))
    );
    assert!(
        row.string_source(second)
            .is_some_and(|source| source.covers_buffer_charpos(20))
    );
}

#[test]
fn rebasing_a_row_moves_string_coverage_but_not_glyph_string_indices() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let string = GlyphStringId::new(7);
    let source = row
        .push_string_source(GlyphStringSource::replacement(
            string,
            GlyphStringBufferRange::new(20, 24),
        ))
        .expect("row-local string source");
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char_with_provenance(
        'S',
        FaceId::new(0),
        GlyphProvenance::string(source, 2),
    ));

    row.shift_string_source_buffer_positions(10, 3);
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()][0].provenance,
        GlyphProvenance::string(source, 2),
        "a string index is not a buffer position"
    );
    assert_eq!(
        row.string_source(source)
            .and_then(|source| source.covered_buffer_range()),
        Some(GlyphStringBufferRange::new(23, 27))
    );
}

#[test]
fn row_string_provenance_survives_json_without_losing_its_coordinate_space() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let source = row
        .push_string_source(GlyphStringSource::replacement(
            GlyphStringId::new(9),
            GlyphStringBufferRange::new(3, 5),
        ))
        .expect("row-local string source");
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char_with_provenance(
        'x',
        FaceId::new(0),
        GlyphProvenance::string(source, 1),
    ));

    let json = serde_json::to_string(&row).expect("serialize row provenance");
    let decoded: GlyphRow = serde_json::from_str(&json).expect("deserialize row provenance");

    assert_eq!(decoded, row);
    let GlyphProvenance::Str { source, index } =
        decoded.glyphs[GlyphArea::Text.index()][0].provenance
    else {
        panic!("decoded glyph lost string coordinate space")
    };
    assert_eq!(index, 1);
    assert_eq!(
        decoded.string_source(source).copied(),
        Some(GlyphStringSource::replacement(
            GlyphStringId::new(9),
            GlyphStringBufferRange::new(3, 5),
        ))
    );
}
