//! A destination and its authoritative geometry; no native-window ownership.

use neomacs_display_protocol::DrawableSurface;

#[derive(Clone, Copy)]
pub struct RenderTarget<'a> {
    pub(super) view: &'a wgpu::TextureView,
    pub(super) surface: DrawableSurface,
}

impl<'a> RenderTarget<'a> {
    /// Requires a full-size, default-format, single-sample view. These checks
    /// inspect the backing texture, not the view descriptor. Mip/subresource
    /// targets require their own extent-aware interface.
    pub fn new(view: &'a wgpu::TextureView, surface: DrawableSurface) -> Self {
        assert_eq!(
            view.texture().sample_count(),
            1,
            "draw target must be single-sample"
        );
        assert_eq!(
            view.texture().width(),
            surface.device_width().get(),
            "draw target width must match its texture"
        );
        assert_eq!(
            view.texture().height(),
            surface.device_height().get(),
            "draw target height must match its texture"
        );
        Self { view, surface }
    }
}
