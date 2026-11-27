//! The layout stage of the `neomacs::cursor_geometry` bug-report telemetry.
//!
//! The draw stage (logged where the render thread receives the frame) has the
//! cursor, row and font metrics. The scroll state the rows were laid out
//! under lives only on this side: window start, hscroll and vscroll, and the
//! incremental class of the window's walk. A point row never takes the plain
//! row route (`buffer_source::row_route::planning` refuses it with
//! `PointInRow` before any routed commit). So the path that produced the
//! cursor row is the row's damage in the draw stage: `New` is a buffer
//! pipeline walk this frame, and `Reused`/`ReusedShifted` is a retained row
//! from an earlier walk. The window class says why the walk was cut that way.

use neomacs_display_protocol::DisplayWindowId;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_display_protocol::glyph_matrix::cursor_geometry::TARGET;
use neomacs_display_protocol::input_latency::ScrollViewport;

use crate::incremental_layout::LayoutClass;

/// One line for the window that owns the active cursor, or nothing when the
/// frame published none.
pub(super) fn log_layout_stage(
    frame_id: u64,
    state: &FrameDisplayState,
    viewports: impl FnOnce() -> Vec<ScrollViewport>,
    class_of: impl Fn(DisplayWindowId) -> Option<LayoutClass>,
) {
    if !tracing::enabled!(target: TARGET, tracing::Level::DEBUG) {
        return;
    }
    let Some(cursor) = state.phys_cursor.as_ref() else {
        return;
    };
    let window = cursor.window_id;
    let viewport = viewports()
        .into_iter()
        .find(|viewport| viewport.window as i64 == window.get());
    let class = class_of(window).map_or_else(|| "none".to_owned(), |class| format!("{class:?}"));
    match viewport {
        Some(viewport) => tracing::debug!(
            target: TARGET,
            "frame={frame_id} stage=layout presentation={} window={} buffer={} start={} \
             hscroll={} vscroll={} class={class}",
            state.presentation_id.get(),
            window.get(),
            viewport.buffer,
            viewport.start,
            viewport.hscroll,
            viewport.vscroll,
        ),
        None => tracing::debug!(
            target: TARGET,
            "frame={frame_id} stage=layout presentation={} window={} viewport=missing class={class}",
            state.presentation_id.get(),
            window.get(),
        ),
    }
}
