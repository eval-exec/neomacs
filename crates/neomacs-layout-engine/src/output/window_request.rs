//! Typed output window lifecycle requests.

use neomacs_display_protocol::types::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutputWindowBeginRequest {
    pub(crate) window_id: u64,
    pub(crate) nrows: usize,
    pub(crate) ncols: usize,
    pub(crate) pixel_bounds: Rect,
    pub(crate) text_pixel_bounds: Rect,
    pub(crate) text_clip_bounds: Rect,
    pub(crate) selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum OutputWindowLifecycleRequest {
    Begin(OutputWindowBeginRequest),
    End,
}

impl OutputWindowBeginRequest {
    pub(crate) fn new(
        window_id: u64,
        nrows: usize,
        ncols: usize,
        pixel_bounds: Rect,
        text_pixel_bounds: Rect,
        text_clip_bounds: Rect,
        selected: bool,
    ) -> Self {
        Self {
            window_id,
            nrows,
            ncols,
            pixel_bounds,
            text_pixel_bounds,
            text_clip_bounds,
            selected,
        }
    }
}

impl OutputWindowLifecycleRequest {
    pub(crate) fn begin(
        window_id: u64,
        nrows: usize,
        ncols: usize,
        pixel_bounds: Rect,
        text_pixel_bounds: Rect,
        text_clip_bounds: Rect,
        selected: bool,
    ) -> Self {
        Self::Begin(OutputWindowBeginRequest::new(
            window_id,
            nrows,
            ncols,
            pixel_bounds,
            text_pixel_bounds,
            text_clip_bounds,
            selected,
        ))
    }

    pub(crate) fn end() -> Self {
        Self::End
    }
}
