use super::*;
fn source(window: i64, y: f32) -> PresentedPointerSourceMap {
    let window = DisplayWindowId::new(window);
    let bounds = FrameRect::new(10.0, y, 20.0, 10.0).unwrap();
    let mode = PointerDrawMode::Face(FaceId::new(1));
    PresentedPointerSourceMap::new(
        vec![PresentedPointerRegion::new_owned(
            PresentedRegionId::new(Some(window), PresentedRegionKind::TextBody),
            bounds,
            None,
            Some(PointerAppearanceId::try_from(0usize).unwrap()),
        )],
        vec![PresentedPointerSourceAppearance::new(
            vec![PresentedSourcePaintSpan::new(
                PresentedPrimitiveKind::Glyph,
                crate::GlyphRowRole::Text,
                DisplaySlotId {
                    window_id: window,
                    row: 1,
                    col: 0,
                },
                bounds,
            )],
            mode,
            mode,
        )],
    )
}

#[test]
fn scroll_pointer_replacement_preserves_other_panes_without_accumulating_history() {
    let window = DisplayWindowId::new(1);
    let mut original = source(1, 10.0);
    let other = source(2, 30.0);
    original.append(other.clone()).unwrap();
    let coverage = source(1, 20.0);
    let viewport = Rect::new(10.0, 10.0, 30.0, 20.0);
    let mut projected = original;
    for offset in [15.5, 10.0, 0.0, 15.5].into_iter().cycle().take(100) {
        projected = projected
            .replace_scrolled_body(window, &coverage, viewport, offset)
            .unwrap();
        assert_eq!(projected.regions.len(), 2);
        assert_eq!(projected.appearances.len(), 2);
        let unchanged = &projected.regions[0];
        assert_eq!(unchanged.owner, other.regions[0].owner);
        assert_eq!(unchanged.bounds, other.regions[0].bounds);
        assert_eq!(projected.appearances[0], other.appearances[0]);
        let moved = &projected.regions[1];
        assert_eq!(moved.bounds.y(), (20.0 - offset).max(10.0));
        assert_eq!(moved.bounds.bottom(), (30.0 - offset).min(30.0));
        assert_eq!(projected.appearances[1].paint_spans[0].clip, moved.bounds);
    }
}

#[test]
fn scroll_pointer_transport_rejects_foreign_or_evaluator_owned_regions() {
    let bounds = FrameRect::new(0.0, 0.0, 100.0, 100.0).unwrap();
    let mut map = source(1, 10.0);
    assert!(
        map.validate_scroll_source(DisplayWindowId::new(1), bounds)
            .is_ok()
    );
    assert!(
        map.validate_scroll_source(DisplayWindowId::new(2), bounds)
            .is_err()
    );
    map.regions[0].interaction = Some(InteractionId::new(1));
    assert!(
        map.validate_scroll_source(DisplayWindowId::new(1), bounds)
            .is_err()
    );
}
