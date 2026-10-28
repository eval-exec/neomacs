//! Cursor synchronization without an active cursor must not manufacture repaint demand.

use super::*;
use crate::render_thread::cursor::CursorState;
use neomacs_display_protocol::frame_time::observe_platform_now;

#[test]
fn no_active_cursor_sync_only_repaints_when_clearing_a_preedit() {
    let now = observe_platform_now();
    let mut render = GuiFrameRenderState::new_without_device(0x42, false, now);
    let config = CursorState::new(now).config_snapshot();
    // A cursor-less frame exercises the same no-active-cursor branch as a hidden cursor.
    let frame = crate::core::frame_glyphs::FrameGlyphBuffer::with_size(800.0, 600.0);
    render.set_current_frame(Some(frame), None, Default::default(), Default::default());
    render.set_dirty(false);

    // poll_frame synchronizes the cursor even without a new editor scene.
    for _ in 0..3 {
        assert!(RenderApp::sync_render_cursor(&mut render, config).is_none());
        assert!(!render.has_presentable_dirty_content());
    }

    render.set_ime_preedit("ni".into(), Some((2, 2)));
    render.set_dirty(false); // The composition has already been painted.
    assert!(RenderApp::sync_render_cursor(&mut render, config).is_none());
    assert!(!render.has_ime_preedit());
    assert!(
        render.has_presentable_dirty_content(),
        "erase the old preedit"
    );

    render.set_dirty(false); // The erasure has reached the screen.
    for _ in 0..3 {
        assert!(RenderApp::sync_render_cursor(&mut render, config).is_none());
        assert!(!render.has_presentable_dirty_content());
    }
}

#[test]
fn no_active_cursor_sync_preserves_existing_repaint_demand() {
    let now = observe_platform_now();
    let mut render = GuiFrameRenderState::new_without_device(0x42, false, now);
    let config = CursorState::new(now).config_snapshot();
    let frame = crate::core::frame_glyphs::FrameGlyphBuffer::with_size(800.0, 600.0);
    render.set_current_frame(Some(frame), None, Default::default(), Default::default());
    assert!(!render.has_ime_preedit());
    render.set_dirty(true);

    for _ in 0..3 {
        assert!(RenderApp::sync_render_cursor(&mut render, config).is_none());
        assert!(
            render.has_presentable_dirty_content(),
            "preserve repaint demand from other content changes"
        );
    }
}
