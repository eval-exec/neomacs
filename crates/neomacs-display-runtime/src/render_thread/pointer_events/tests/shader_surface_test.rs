use super::*;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_display_protocol::{
    DisplayWindowId, GeometryPoint, GeometryRect, InteractionProjection, LiveDisplayWindowId,
    LogicalPixels, PaneProjection, PresentationFrameSpace, PresentationId, RootSurfaceSpace,
    SurfaceId,
    types::{Color, DisplayFrameId},
};

const FRAME: u64 = 0x42;
/// The surface whose destination rect the pointer is really over.
const POINTED_AT: u32 = 1;
/// The surface sitting at the pointer's *unprojected* coordinates.
const AT_RAW_COORDINATES: u32 = 2;

fn frame_with_two_surfaces(presentation: PresentationId) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.set_frame_identity(
        DisplayFrameId::new(FRAME),
        DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    frame.presentation_id = presentation;
    frame.add_surface(SurfaceId::new(POINTED_AT), 40.0, 20.0, 40.0, 40.0);
    frame.add_surface(SurfaceId::new(AT_RAW_COORDINATES), 240.0, 20.0, 80.0, 40.0);
    frame
}

fn render_showing_two_surfaces(presentation: PresentationId) -> GuiFrameRenderState {
    let mut render = GuiFrameRenderState::new_without_device(
        FRAME,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(
        Some(frame_with_two_surfaces(presentation)),
        None,
        Default::default(),
        Default::default(),
    );
    render
}

/// A pane drawn 200px right of where its content belongs, as one is
/// partway through a horizontal `split-window`.
fn pane_displaced_by_200px(presentation: PresentationId) -> InteractionProjection {
    let pane = PaneProjection::new(
        LiveDisplayWindowId::try_from(DisplayWindowId::new(4))
            .expect("a non-zero window id is live"),
        GeometryRect::<RootSurfaceSpace, LogicalPixels>::new(200.0, 0.0, 400.0, 600.0)
            .expect("a fixture's pane rect is valid geometry"),
        GeometryPoint::<PresentationFrameSpace, LogicalPixels>::from_px(0.0, 0.0)
            .expect("the frame origin is a valid content origin"),
    )
    .expect("a 200px translation is representable");
    InteractionProjection::new(presentation, vec![pane])
}

#[test]
fn the_shader_surface_a_pointer_reaches_is_the_one_drawn_under_it_while_its_pane_moves() {
    // Both `iMouse` routes — the press in `handle_mouse_input` and the
    // hover in `handle_cursor_moved` — come through
    // `surface_target_for_frame_window`. If it stopped projecting, a click
    // or hover mid-`split-window` would drive the shader sitting at the
    // pointer's raw coordinates instead of the one under the pointer, and
    // every other test here would still pass: they run against settled
    // frames, where the projection is the identity.
    let presentation = PresentationId::new(11);
    let mut render = render_showing_two_surfaces(presentation);
    render.compositor.interaction = Some(pane_displaced_by_200px(presentation));

    assert_eq!(
        RenderApp::surface_target_for_frame_window(&render, FRAME, 260.0, 40.0),
        Some((POINTED_AT, 0.5, 0.5)),
        "surface x=260 is 60px into the destination, the middle of the first surface"
    );
}

#[test]
fn a_settled_frame_reaches_the_shader_surface_at_the_pointers_own_coordinates() {
    // The other half of the claim: projecting must displace nothing while
    // nothing is moving, which is every frame outside a layout morph.
    let render = render_showing_two_surfaces(PresentationId::new(11));

    assert_eq!(
        RenderApp::surface_target_for_frame_window(&render, FRAME, 260.0, 40.0),
        Some((AT_RAW_COORDINATES, 0.25, 0.5)),
        "with no pane in motion the pointer is over the surface its own coordinates name"
    );
}
