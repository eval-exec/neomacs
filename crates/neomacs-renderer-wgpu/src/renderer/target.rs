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

/// A bounded sample of a pooled texture, placed in logical destination pixels.
/// Source coordinates are texture texels, not normalized or logical pixels;
/// retaining f32 precision avoids quantizing a scrolling crop to a u16 UV grid.
#[derive(Clone, Copy)]
pub struct SnapshotRegion<'a> {
    pub(super) source: &'a crate::SnapshotLease,
    pub(super) uv: neomacs_display_protocol::Rect,
    pub(super) destination: neomacs_display_protocol::FrameRect,
}

impl<'a> SnapshotRegion<'a> {
    pub fn new(
        source: &'a crate::SnapshotLease,
        source_pixels: neomacs_display_protocol::Rect,
        destination: neomacs_display_protocol::FrameRect,
    ) -> Option<Self> {
        let size = source.size();
        let region = source_pixels;
        let dst = destination.raw();
        if [region.x, region.y, region.width, region.height]
            .iter()
            .any(|v| !v.is_finite())
            || region.x < 0.0
            || region.y < 0.0
            || region.width <= 0.0
            || region.height <= 0.0
            || region.right() > size.width() as f32
            || region.bottom() > size.height() as f32
            || dst.width <= 0.0
            || dst.height <= 0.0
        {
            return None;
        }
        Some(Self {
            source,
            uv: neomacs_display_protocol::Rect::new(
                region.x / size.width() as f32,
                region.y / size.height() as f32,
                region.width / size.width() as f32,
                region.height / size.height() as f32,
            ),
            destination,
        })
    }
}
