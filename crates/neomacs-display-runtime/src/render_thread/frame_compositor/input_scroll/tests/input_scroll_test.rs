use super::*;
use neomacs_display_protocol::input_progress::{InputProgress, InputStream};
use neomacs_display_protocol::*;

fn frame(presentation: u64, displacement: f32, epoch: u64) -> FrameGlyphBuffer {
    frame_with_prediction(presentation, displacement, epoch, true)
}

fn frame_with_prediction(
    presentation: u64,
    displacement: f32,
    epoch: u64,
    predict_pixels: bool,
) -> FrameGlyphBuffer {
    let mut state = FrameDisplayState::new(10, 2, 10.0, 10.0);
    state.presentation_id = PresentationId::new(presentation);
    let window = DisplayWindowId::new(1);
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
    state.scroll_coverage.push(Arc::new(
        neomacs_display_protocol::scroll_coverage::ScrollCoverage {
            epoch,
            anchor_row: 0,
            predict_pixels,
            compositor_enabled: true,
            viewport,
            origin: displacement,
            content,
            faces: Default::default(),
            fonts: Default::default(),
            char_fonts: Default::default(),
            shaped_clusters: Default::default(),
            hit_index,
            pointer_source: Default::default(),
        },
    ));
    let frame = state.materialize();
    assert_eq!(frame.scroll_surfaces.len(), 1);
    frame
}

/// A publish that re-exports nothing: the same window's rows certified for a
/// later presentation without carrying a fresh surface.
fn cursor_only_successor(source: &FrameGlyphBuffer, epoch: u64) -> FrameGlyphBuffer {
    let mut frame = source.clone();
    frame.presentation_id = PresentationId::new(source.presentation_id.get() + 1);
    frame.scroll_surfaces.clear();
    frame.scroll_coverage_renewals = vec![
        neomacs_display_protocol::scroll_coverage::ScrollCoverageRenewal {
            window: DisplayWindowId::new(1),
            epoch,
            predict_pixels: true,
            compositor_enabled: true,
        },
    ];
    frame
}

/// Successive installs: each frame refreshes the map, latest wins.
fn install_retention(frames: &[&FrameGlyphBuffer]) -> CoverageRetention {
    let mut retention = CoverageRetention::default();
    for frame in frames {
        retention.refresh(frame);
    }
    retention
}

fn point(frame: &FrameGlyphBuffer, y: f32) -> PresentationFramePoint {
    InteractionProjection::settled(frame.presentation_id)
        .map(GeometryPoint::<RootSurfaceSpace, LogicalPixels>::from_px(1.0, y).unwrap())
        .unwrap()
}

#[test]
fn input_scroll_publishes_hits_only_with_pixels_and_preserves_them_on_retained_submissions() {
    let mut frame = frame(1, 0.0, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let retention = install_retention(&[&frame]);
    let mut scroll = InputScroll::default();
    assert!(scroll.push(&retention, &frame, 1.0, 1.0, 12.0, delivery.receipt(), None));
    assert!(scroll.hit(point(&frame, 1.0)).is_none());
    scroll.paint(&mut frame);
    assert!(scroll.hit(point(&frame, 1.0)).is_none());
    scroll.submit();
    let hit = scroll.hit(point(&frame, 1.0)).unwrap().unwrap().unwrap();
    assert_eq!(hit.text_position().unwrap().buffer_position(), 3);
    assert_eq!(hit.text_position().unwrap().row(), 0);
    scroll.submit(); // cursor-only retained-static submission
    assert!(scroll.hit(point(&frame, 1.0)).is_some());
    assert!(scroll.hit(point(&frame, 21.0)).is_none());
}

#[test]
fn input_scroll_holds_target_during_mid_command_layout_and_finishes_on_frame_ack() {
    let first = frame(1, 0.0, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    let mut scroll = InputScroll::default();
    assert!(scroll.push(
        &install_retention(&[&first]),
        &first,
        1.0,
        1.0,
        12.0,
        delivery.receipt(),
        None
    ));
    progress.consumed(delivery);
    let mut mid = frame(2, 5.0, 1);
    mid.input_checkpoint = progress.checkpoint();
    scroll.reconcile(&install_retention(&[&first, &mid]), Some(&mid));
    assert_eq!(scroll.active.as_ref().unwrap().offset, 7.0);
    drop(command);
    // A late completion cannot mutate the checkpoint already in `mid`.
    scroll.reconcile(&install_retention(&[&first, &mid]), Some(&mid));
    assert!(scroll.active());
    let mut completed = frame(3, 12.0, 1);
    completed.input_checkpoint = progress.checkpoint();
    scroll.reconcile(
        &install_retention(&[&first, &mid, &completed]),
        Some(&completed),
    );
    assert!(!scroll.active());
}

#[test]
fn input_scroll_cancels_on_revision_and_drop_and_clamps_reversal_at_coverage() {
    let first = frame(1, 0.0, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let retention = install_retention(&[&first]);
    let mut scroll = InputScroll::default();
    for _ in 0..4 {
        assert!(scroll.push(&retention, &first, 1.0, 1.0, 18.0, delivery.receipt(), None));
    }
    assert_eq!(scroll.active.as_ref().unwrap().offset, 40.0);
    scroll.push(&retention, &first, 1.0, 1.0, -4.0, delivery.receipt(), None);
    assert_eq!(scroll.active.as_ref().unwrap().offset, 36.0);
    let revision = frame(2, 0.0, 2);
    scroll.reconcile(&install_retention(&[&first, &revision]), Some(&revision));
    assert!(!scroll.active());
    scroll.push(&retention, &first, 1.0, 1.0, 8.0, delivery.receipt(), None);
    drop(delivery);
    scroll.paint(&mut first.clone());
    assert!(!scroll.active());
}

#[test]
fn resolved_scroll_uses_authoritative_destination_without_predictable_lisp_binding() {
    let mut first = frame_with_prediction(1, 0.0, 1, false);
    // Permission for raw input is independent of evaluator-resolved movement.
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let make_intent =
        |presentation, epoch| neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent {
            frame: 1,
            window: DisplayWindowId::new(1),
            presentation: PresentationId::new(presentation),
            epoch,
            offset: 12.0,
            inputs: vec![delivery.receipt()],
        };
    let retention = install_retention(&[&first]);
    let mut scroll = InputScroll::default();
    // Presentation equality is no longer the gate: an intent names a coverage
    // epoch, and the retention map decides whether that epoch is certified for
    // the installed presentation. Presentation mismatch with a certified epoch
    // is a renewal (the rows are unchanged), so the intent still applies.
    assert!(scroll.resolve(&retention, &first, make_intent(2, 1)));
    assert!(!scroll.resolve(&retention, &first, make_intent(1, 2)));
    assert!(scroll.resolve(&retention, &first, make_intent(1, 1)));
    assert!(!scroll.push(&retention, &first, 1.0, 1.0, 4.0, delivery.receipt(), None));
    scroll.paint(&mut first);
    scroll.submit();
    assert_eq!(
        scroll
            .hit(point(&first, 1.0))
            .unwrap()
            .unwrap()
            .unwrap()
            .text_position()
            .unwrap()
            .buffer_position(),
        3
    );
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    progress.consumed(delivery);
    let mut mid = frame_with_prediction(2, 5.0, 1, false);
    mid.input_checkpoint = progress.checkpoint();
    scroll.reconcile(&install_retention(&[&first, &mid]), Some(&mid));
    assert_eq!(scroll.active.as_ref().unwrap().offset, 7.0);
    drop(command);
    let mut done = frame(3, 12.0, 1);
    done.input_checkpoint = progress.checkpoint();
    scroll.reconcile(&install_retention(&[&first, &mid, &done]), Some(&done));
    assert!(!scroll.active());
}

#[test]
fn raw_prediction_survives_cursor_only_publishes_that_renew_the_epoch() {
    // The 120Hz stream failure: every cursor-only redisplay published a frame
    // with no surface, so prediction stalled until the worker exported again.
    let first = frame(1, 0.0, 1);
    let second = cursor_only_successor(&first, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let retention = install_retention(&[&first, &second]);
    let mut scroll = InputScroll::default();
    assert!(scroll.push(
        &retention,
        &second,
        1.0,
        1.0,
        12.0,
        delivery.receipt(),
        None
    ));
    // Silence — neither a surface nor a renewal — revokes the coverage.
    let mut silent = second.clone();
    silent.presentation_id = PresentationId::new(3);
    silent.scroll_coverage_renewals.clear();
    let retention = install_retention(&[&first, &second, &silent]);
    assert!(!scroll.push(
        &retention,
        &silent,
        1.0,
        1.0,
        12.0,
        delivery.receipt(),
        None
    ));
    // Reconcile against the silent frame drops the unacknowledged active.
    scroll.reconcile(&retention, Some(&silent));
    assert!(!scroll.active());
}

#[test]
fn raw_prediction_projects_and_submits_against_the_renewed_presentation() {
    let first = frame(1, 0.0, 1);
    let mut second = cursor_only_successor(&first, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let retention = install_retention(&[&first, &second]);
    let mut scroll = InputScroll::default();
    assert!(scroll.push(
        &retention,
        &second,
        1.0,
        1.0,
        12.0,
        delivery.receipt(),
        None
    ));
    scroll.reconcile(&retention, Some(&second));
    assert!(scroll.active());
    scroll.paint(&mut second);
    scroll.submit();
    // The submitted projection answers hits at the renewed presentation even
    // though its surface's hit index keeps the original export's stamp.
    let hit = scroll.hit(point(&second, 1.0)).unwrap().unwrap().unwrap();
    assert_eq!(hit.text_position().unwrap().buffer_position(), 3);
}

#[test]
fn resolved_intent_applies_across_intervening_cursor_only_publish() {
    // The wheel-stall failure: the intent named the presentation that exported
    // the coverage; a cursor-only publish superseded it before the command was
    // processed, and the intent was dropped even though the epoch stayed
    // certified.
    let first = frame(1, 0.0, 1);
    let second = cursor_only_successor(&first, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let intent = neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent {
        frame: 1,
        window: DisplayWindowId::new(1),
        presentation: first.presentation_id,
        epoch: 1,
        offset: 12.0,
        inputs: vec![delivery.receipt()],
    };
    let retention = install_retention(&[&first, &second]);
    let mut scroll = InputScroll::default();
    assert!(scroll.resolve(&retention, &second, intent));
    let mut projected = second.clone();
    scroll.paint(&mut projected);
    scroll.submit();
    assert_eq!(
        scroll
            .hit(point(&second, 1.0))
            .unwrap()
            .unwrap()
            .unwrap()
            .text_position()
            .unwrap()
            .buffer_position(),
        3
    );
}

#[test]
fn renewal_policy_flip_blocks_raw_prediction_but_keeps_resolved_authority() {
    let first = frame(1, 0.0, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    // Renewal with prediction revoked: raw pushes stop, a resolved intent from
    // the evaluator keeps its authority (compositor scrolling stays enabled).
    let mut revoked = cursor_only_successor(&first, 1);
    revoked.scroll_coverage_renewals[0].predict_pixels = false;
    let retention = install_retention(&[&first, &revoked]);
    let mut scroll = InputScroll::default();
    assert!(!scroll.push(
        &retention,
        &revoked,
        1.0,
        1.0,
        12.0,
        delivery.receipt(),
        None
    ));
    let intent = neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent {
        frame: 1,
        window: DisplayWindowId::new(1),
        presentation: revoked.presentation_id,
        epoch: 1,
        offset: 12.0,
        inputs: vec![delivery.receipt()],
    };
    assert!(scroll.resolve(&retention, &revoked, intent));
}

#[test]
fn renewal_naming_unknown_epoch_expires_the_retained_surface() {
    let first = frame(1, 0.0, 1);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let mut rekeyed = cursor_only_successor(&first, 2);
    rekeyed.scroll_coverage_renewals[0].epoch = 2;
    let retention = install_retention(&[&first, &rekeyed]);
    let mut scroll = InputScroll::default();
    assert!(!scroll.push(
        &retention,
        &rekeyed,
        1.0,
        1.0,
        12.0,
        delivery.receipt(),
        None
    ));
}
