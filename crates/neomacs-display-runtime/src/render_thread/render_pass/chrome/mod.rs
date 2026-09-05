//! Everything drawn *over* the composed editor picture, in window
//! coordinates, and the order it goes on in.
//!
//! Owns: the sequence, and only the sequence. Two kinds of thing are drawn
//! here and they differ in what decides they exist:
//!
//! - `frame_bands` — the frame's own furniture. The custom titlebar, the menu
//!   / tool / compact bars and the rounded corner mask are there because the
//!   frame is configured that way, they occupy reserved geometry, and they do
//!   not animate.
//! - `transient_overlays` — the popup menu, the tooltip, the IME pre-edit box,
//!   the visual bell, the FPS readout and the typing-speed readout. Each is
//!   there because of something that just happened, and three of them are
//!   still moving while they are drawn.
//!
//! The two interleave, which is why neither half owns a run of the order: the
//! corner mask is a band and it goes on *after* the visual bell, because it
//! has to cut the corners out of the flash as well as out of the picture.
//! Later passes draw over earlier ones, so this sequence is load-bearing in
//! both directions and belongs in one file that does nothing else.
//!
//! Must not: decide *whether* the frame is drawn, acquire or present a
//! surface, or choose a composition strategy. This pass runs once the target
//! view is already chosen, and it draws onto whatever view it is handed —
//! the offscreen composition slot on a transition frame, the swapchain view
//! otherwise. It is the last content-producing phase of the draw order, which
//! is why the overlays that must keep animating report by setting the
//! frame-dirty flag rather than by scheduling anything themselves.

mod frame_bands;
mod transient_overlays;

/// Production code reaches this through `frame_bands` itself; the re-export
/// exists so `input_test` can assert that the tool bar's origin comes from the
/// authoritative band bounds rather than from a second layout calculation.
#[cfg(test)]
pub(in crate::render_thread) use frame_bands::frame_chrome_toolbar_bounds;

use crate::render_thread::frame_windows::{GuiFrameNativeWindowState, GuiFrameRenderState};
use crate::render_thread::state::{ChildFrameStyle, ToolbarResources};
use neomacs_renderer_wgpu::WgpuRenderer;

#[allow(clippy::too_many_arguments)]
pub(super) fn render_frame_window_overlays_with_toolbar_resources(
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    cursor_visible: bool,
    animated_cursor: Option<crate::core::types::AnimatedCursor>,
    child_frame_style: &ChildFrameStyle,
    scroll_indicators_enabled: bool,
    toolbar: &ToolbarResources,
) {
    // Content overlays are the *editor's* picture — child frames, breadcrumbs,
    // scroll indicators, watermarks — and belong to `scene`, which is where
    // they are drawn. They open this pass rather than closing `scene` because
    // both composition strategies call straight from their glyph pass into
    // this one; moving the call up to them is a change to those two files.
    super::scene::render_frame_content_overlays(
        renderer,
        native,
        render,
        surface_view,
        frame,
        cursor_visible,
        animated_cursor,
        child_frame_style,
        scroll_indicators_enabled,
    );

    frame_bands::draw(renderer, native, render, surface_view, frame, toolbar);
    transient_overlays::draw_panels(renderer, render, surface_view, native.width, native.height);
    transient_overlays::draw_visual_bell(
        renderer,
        render,
        surface_view,
        native.width,
        native.height,
    );
    frame_bands::draw_corner_mask(
        renderer,
        surface_view,
        &native.chrome,
        native.width,
        native.height,
    );
    transient_overlays::draw_fps(
        renderer,
        render,
        surface_view,
        frame,
        native.width,
        native.height,
    );
    transient_overlays::draw_typing_speed(renderer, render, surface_view, frame);
}
