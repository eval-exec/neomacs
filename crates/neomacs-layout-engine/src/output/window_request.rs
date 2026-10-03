//! Typed output window lifecycle requests.

use neomacs_display_protocol::types::Rect;

/// Owned numeric row-storage capacity. Full mini measurement grows only
/// emitted rows; presentation and ordinary query grids remain fixed.
/// One exclusive output builder owns each selector; independent mutators share no mutable state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputWindowRowCapacity {
    #[default]
    Fixed,
    Growing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutputWindowBeginRequest {
    pub(crate) window_id: u64,
    pub(crate) nrows: usize,
    pub(crate) ncols: usize,
    pub(crate) row_capacity: OutputWindowRowCapacity,
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
            row_capacity: OutputWindowRowCapacity::Fixed,
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

    pub(crate) fn with_row_capacity(self, capacity: OutputWindowRowCapacity) -> Self {
        match self {
            Self::Begin(mut request) => {
                request.row_capacity = capacity;
                Self::Begin(request)
            }
            Self::End => Self::End,
        }
    }

    pub(crate) fn end() -> Self {
        Self::End
    }
}
