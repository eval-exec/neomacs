use super::{
    CursorColorPolicy, CursorShape, FilledBoxPresentation, PresentedCursorPaint,
    ResolvedCursorPaint,
};
use neomacs_display_protocol::CursorColorCycleConfig;
use neomacs_display_protocol::frame_glyphs::DisplaySlotId;
use neomacs_display_protocol::types::{
    AnimatedCursor, Color, DisplayFrameId, DisplayWindowId, Rect,
};

#[test]
fn color_cycle_origin_inherits_the_gnu_resolved_cursor_paint() {
    let cycle = CursorColorCycleConfig::default();
    let resolved = ResolvedCursorPaint::new(Color::WHITE, Color::BLACK);
    let origin = std::time::Instant::now();

    let presented = PresentedCursorPaint::resolve(
        resolved,
        CursorColorPolicy::Cycle {
            config: &cycle,
            origin,
        },
        origin,
    );

    assert_eq!(presented.body_background, Color::WHITE);
    assert_eq!(presented.glyph_foreground, Color::BLACK);
}

#[test]
fn color_cycle_depends_on_target_time_not_delivered_tick_count() {
    let mut cycle = CursorColorCycleConfig::default();
    cycle.speed = 0.5;
    cycle.saturation = 1.0;
    cycle.lightness = 0.5;
    let resolved = ResolvedCursorPaint::new(Color::BLACK, Color::WHITE);
    let origin = std::time::Instant::now();
    let target = origin + std::time::Duration::from_millis(500);
    let policy = CursorColorPolicy::Cycle {
        config: &cycle,
        origin,
    };

    let direct = PresentedCursorPaint::resolve(resolved, policy, target);
    for skipped_sample in [40, 80, 120, 250] {
        let _ = PresentedCursorPaint::resolve(
            resolved,
            policy,
            origin + std::time::Duration::from_millis(skipped_sample),
        );
    }
    let after_intermediate_samples = PresentedCursorPaint::resolve(resolved, policy, target);

    assert_eq!(direct, after_intermediate_samples);
}

#[test]
fn color_cycle_retains_frame_scale_precision_after_a_year() {
    let mut cycle = CursorColorCycleConfig::default();
    cycle.speed = 0.5;
    cycle.saturation = 1.0;
    cycle.lightness = 0.5;
    let resolved = ResolvedCursorPaint::new(Color::WHITE, Color::BLACK);
    let origin = std::time::Instant::now();
    let after_a_year = origin + std::time::Duration::from_secs(365 * 24 * 60 * 60);
    let policy = CursorColorPolicy::Cycle {
        config: &cycle,
        origin,
    };

    let first = PresentedCursorPaint::resolve(resolved, policy, after_a_year);
    let next = PresentedCursorPaint::resolve(
        resolved,
        policy,
        after_a_year + std::time::Duration::from_secs_f64(1.0 / 24.0),
    );

    assert_ne!(
        first, next,
        "adjacent 24 Hz samples must remain distinct after a year of uptime"
    );
    assert_eq!(first.body_background, Color::WHITE);
}

#[test]
fn vertical_motion_does_not_inverse_the_destination_glyph_before_the_box_arrives() {
    let window_id = DisplayWindowId::new(7);
    let destination_slot = DisplaySlotId {
        window_id,
        row: 1,
        col: 0,
    };
    let destination = Rect::new(10.0, 28.0, 12.0, 18.0);
    let animated_at_source = AnimatedCursor {
        window_id,
        x: 10.0,
        y: 10.0,
        width: 12.0,
        height: 18.0,
        corners: None,
        frame_id: DisplayFrameId::new(0),
    };
    let paint = PresentedCursorPaint::resolve(
        ResolvedCursorPaint::new(Color::WHITE, Color::BLACK),
        CursorColorPolicy::Inherit,
        std::time::Instant::now(),
    );

    let presentation = FilledBoxPresentation::resolve(
        window_id,
        destination_slot,
        destination,
        Some(&animated_at_source),
        paint,
    );

    assert_eq!(presentation.inverse_video(), None);
    assert_eq!(
        presentation,
        FilledBoxPresentation::InFlight {
            shape: CursorShape::Rect(Rect::new(10.0, 10.0, 12.0, 18.0)),
            paint,
        }
    );
}

#[test]
fn horizontal_and_vertical_motion_share_one_in_flight_contract() {
    let window_id = DisplayWindowId::new(7);
    let destination_slot = DisplaySlotId {
        window_id,
        row: 1,
        col: 1,
    };
    let destination = Rect::new(22.0, 28.0, 12.0, 18.0);
    let paint = PresentedCursorPaint::resolve(
        ResolvedCursorPaint::new(Color::WHITE, Color::BLACK),
        CursorColorPolicy::Inherit,
        std::time::Instant::now(),
    );
    let moving_from = |x, y| AnimatedCursor {
        window_id,
        x,
        y,
        width: destination.width,
        height: destination.height,
        corners: None,
        frame_id: DisplayFrameId::new(0),
    };

    let horizontal = FilledBoxPresentation::resolve(
        window_id,
        destination_slot,
        destination,
        Some(&moving_from(10.0, destination.y)),
        paint,
    );
    let vertical = FilledBoxPresentation::resolve(
        window_id,
        destination_slot,
        destination,
        Some(&moving_from(destination.x, 10.0)),
        paint,
    );

    assert!(matches!(horizontal, FilledBoxPresentation::InFlight { .. }));
    assert!(matches!(vertical, FilledBoxPresentation::InFlight { .. }));
    assert_eq!(horizontal.inverse_video(), None);
    assert_eq!(vertical.inverse_video(), None);
}

#[test]
fn settled_box_owns_the_matching_inverse_video_cell() {
    let window_id = DisplayWindowId::new(7);
    let destination_slot = DisplaySlotId {
        window_id,
        row: 1,
        col: 1,
    };
    let destination = Rect::new(22.0, 28.0, 12.0, 18.0);
    let paint = PresentedCursorPaint::resolve(
        ResolvedCursorPaint::new(Color::WHITE, Color::BLACK),
        CursorColorPolicy::Inherit,
        std::time::Instant::now(),
    );

    let presentation =
        FilledBoxPresentation::resolve(window_id, destination_slot, destination, None, paint);

    assert_eq!(
        presentation.inverse_video(),
        Some(super::InverseVideoCell {
            slot_id: destination_slot,
            paint,
        })
    );
    assert!(matches!(
        presentation,
        FilledBoxPresentation::Settled { rect, .. } if rect == destination
    ));
}
