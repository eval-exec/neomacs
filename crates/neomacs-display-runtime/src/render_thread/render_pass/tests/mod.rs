use super::*;
use neomacs_display_protocol::{DeviceScale, SurfaceState};

fn drawable() -> SurfaceState {
    SurfaceState::from_device_size(800, 600, DeviceScale::new(1.0).unwrap()).unwrap()
}

fn render_with_frame() -> GuiFrameRenderState {
    let mut render = GuiFrameRenderState::new_without_device(
        42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(
        Some(crate::core::frame_glyphs::FrameGlyphBuffer::with_size(
            800.0, 600.0,
        )),
        None,
        Default::default(),
        Default::default(),
    );
    render
}

#[test]
fn suspended_native_surface_is_window_not_ready_with_or_without_content() {
    let mut render = render_with_frame();
    render.set_surface_state(SurfaceState::Suspended);
    assert_eq!(
        composition_surface(&mut render, SurfaceState::Suspended),
        Err(FrameRenderFailure::WindowNotReady)
    );
    assert!(render.present_mapping().is_none());
    let mut empty = GuiFrameRenderState::new_without_device(
        42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    assert_eq!(
        composition_surface(&mut empty, SurfaceState::Suspended),
        Err(FrameRenderFailure::WindowNotReady)
    );
}

#[test]
fn drawable_native_surface_refreshes_suspended_cached_mapping() {
    let mut render = render_with_frame();
    render.set_surface_state(SurfaceState::Suspended);
    let SurfaceState::Drawable(surface) = drawable() else {
        unreachable!()
    };
    assert_eq!(composition_surface(&mut render, drawable()), Ok(surface));
    assert!(render.present_mapping().is_some());
}

#[test]
fn drawable_surface_without_editor_content_is_still_awaiting_content() {
    let mut render = GuiFrameRenderState::new_without_device(
        42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    assert_eq!(
        composition_surface(&mut render, drawable()),
        Err(FrameRenderFailure::AwaitingContent)
    );
}
