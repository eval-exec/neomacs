use super::*;
use neomacs_display_protocol::scroll_coverage::{ScrollCoverage, ScrollCoverageRenewal};
use neomacs_display_protocol::*;

fn surface_frame(presentation: u64, window: u64, epoch: u64) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(100.0, 100.0);
    frame.presentation_id = PresentationId::new(presentation);
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
    state.window_matrices.push(content.clone());
    state.scroll_coverage.push(Arc::new(ScrollCoverage {
        epoch,
        anchor_row: 0,
        predict_pixels: true,
        compositor_enabled: true,
        viewport,
        origin: 0.0,
        content,
        faces: Default::default(),
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
