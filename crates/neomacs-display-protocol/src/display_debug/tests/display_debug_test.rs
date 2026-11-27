use super::*;
use crate::frame_glyphs::PhysCursor;
use crate::types::{Color, DisplayWindowId};
use crate::{CursorStyle, DisplaySlotId};

#[test]
fn snapshot_prefers_last_presented_summary_and_falls_back_before_present() {
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = PresentationId::new(7);
    let win_id = DisplayWindowId::new(3);
    frame.set_phys_cursor(PhysCursor {
        window_id: win_id,
        charpos: 10,
        row: 1,
        col: 2,
        slot_id: DisplaySlotId {
            window_id: win_id,
            row: 1,
            col: 2,
        },
        x: 16.0,
        y: 20.0,
        width: 8.0,
        height: 17.0,
        ascent: 13.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });

    let before = PresentedFrameDebugSnapshot::from_frame_buffer(
        1,
        true,
        &frame,
        None,
        1.0,
        [800, 600],
        true,
        true,
    );
    assert_eq!(before.render_path, PresentedRenderPath::NotYetPresented);
    assert_eq!(before.frame_seq, 0);
    assert_eq!(before.presentation_id, 7);
    assert!(before.has_active_cursor);
    assert!(before.cursor_visible);
    assert!(before.inverse_video_active);

    let summary = LastPresentedFrameSummary {
        presentation_id: PresentationId::new(7),
        frame_seq: 42,
        render_path: PresentedRenderPath::RetainedStatic,
        scale_factor: 1.5,
        surface_width: 1200,
        surface_height: 900,
        logical_width: 800.0,
        logical_height: 600.0,
        has_active_cursor: true,
        cursor_visible: false,
        blink_enabled: true,
        inverse_video_active: false,
        is_animating: true,
        animated_cursor_rect: Some(DisplayDebugRect::from_array([12.0, 20.0, 8.0, 17.0])),
        input_scroll_active: true,
        input_scroll_offset: -14.5,
    };
    let after = PresentedFrameDebugSnapshot::from_frame_buffer(
        1,
        true,
        &frame,
        Some(summary),
        1.0,
        [800, 600],
        true,
        true,
    );
    assert_eq!(after.render_path, PresentedRenderPath::RetainedStatic);
    assert_eq!(after.frame_seq, 42);
    assert_eq!(after.scale_factor, 1.5);
    assert_eq!(after.surface_px, [1200, 900]);
    assert!(!after.cursor_visible);
    assert!(!after.inverse_video_active);
    assert!(after.is_animating);
    assert_eq!(
        after.animated_cursor_rect,
        Some(DisplayDebugRect {
            x: 12.0,
            y: 20.0,
            width: 8.0,
            height: 17.0,
        })
    );
    assert!(after.input_scroll_active);
    assert_eq!(after.input_scroll_offset, -14.5);
}

#[test]
fn diagnostics_flags_detect_lag_blink_animation_and_input_scroll() {
    let frame = PresentedFrameDebugSnapshot {
        frame_id: 1,
        is_primary: true,
        presentation_id: 12,
        frame_seq: 5,
        render_path: PresentedRenderPath::RetainedScroll,
        scale_factor: 2.0,
        surface_px: [1600, 1200],
        logical_px: [800.0, 600.0],
        has_active_cursor: true,
        cursor_visible: false,
        blink_enabled: true,
        inverse_video_active: false,
        is_animating: true,
        animated_cursor_rect: Some(DisplayDebugRect {
            x: 16.0,
            y: 48.0,
            width: 8.0,
            height: 16.0,
        }),
        input_scroll_active: true,
        input_scroll_offset: 6.0,
    };

    let flags = DisplayDebugFlags::compute(Some(1), 15, &[frame]);
    assert!(flags.presentation_lagging);
    assert!(flags.cursor_hidden_or_blinked_off);
    assert!(flags.cursor_animating);
    assert!(flags.input_scroll_active);
}
