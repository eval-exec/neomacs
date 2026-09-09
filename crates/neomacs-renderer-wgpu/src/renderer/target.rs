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

/// Proof that one editor composition exactly fits a native content viewport.
/// Native geometry is checked once; painters cannot supply a different source
/// or destination after validation.
#[derive(Clone, Copy)]
pub struct NativeContentPlacement<'a> {
    pub(super) target: RenderTarget<'a>,
    pub(super) source: &'a crate::SnapshotLease,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NativePlacementError {
    #[error("native surface has no drawable content viewport")]
    Obscured,
    #[error("editor texture does not match the native content viewport")]
    ExtentMismatch,
    #[error("editor texture and native surface use different formats")]
    FormatMismatch,
}

impl<'a> NativeContentPlacement<'a> {
    pub fn new(
        target: RenderTarget<'a>,
        source: &'a crate::SnapshotLease,
    ) -> Result<Self, NativePlacementError> {
        let content = target
            .surface
            .content_surface()
            .ok_or(NativePlacementError::Obscured)?;
        if source.size().width() != content.device_width().get()
            || source.size().height() != content.device_height().get()
        {
            return Err(NativePlacementError::ExtentMismatch);
        }
        if target.view.texture().format() != source.view().texture().format() {
            return Err(NativePlacementError::FormatMismatch);
        }
        Ok(Self { target, source })
    }
}
