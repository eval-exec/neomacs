//! Frame-glyph presentation over the portable surface lifecycle.

use std::sync::Arc;

use neomacs_display_protocol::{
    DeviceScale, DrawableSurface, FrameGlyphBuffer, GeometryError, GeometrySize, LogicalPixels,
    PresentMapping, PresentationExtent, SurfaceState,
};
use neomacs_renderer_wgpu::{WgpuGlyphAtlas, WgpuRenderer};
use thiserror::Error;
use winit::event_loop::OwnedDisplayHandle;

use crate::{
    PresentationOutcome, PresentationSkipReason, SurfaceExtent, SurfaceInitError,
    SurfacePresentError, SurfaceRuntime, SurfaceWindow,
};

/// Failure while creating a surface and the renderer bound to its device.
#[derive(Debug, Error)]
pub enum SurfaceFrameInitError {
    #[error(transparent)]
    Surface(#[from] SurfaceInitError),
    #[error("the host reported an invalid device scale: {0}")]
    InvalidDeviceScale(f64),
}

/// Failure while updating the host's logical-to-physical scale.
#[derive(Debug, Error, Clone, Copy, PartialEq)]
#[error("the host reported an invalid device scale: {0}")]
pub struct SurfaceScaleError(pub f64);

/// Failure while mapping or presenting one immutable frame.
#[derive(Debug, Error)]
pub enum SurfaceFramePresentError {
    #[error(transparent)]
    Geometry(#[from] GeometryError),
    #[error(transparent)]
    Surface(#[from] SurfacePresentError),
}

/// Owns every GPU object needed to present a [`FrameGlyphBuffer`].
///
/// Product adapters forward window lifecycle events here; they do not own
/// swapchain recovery, render pipelines, font replay, or device-scale math.
pub struct SurfaceFrameRenderer {
    surface: SurfaceRuntime,
    renderer: WgpuRenderer,
    glyph_atlas: WgpuGlyphAtlas,
    device_scale: DeviceScale,
}

impl SurfaceFrameRenderer {
    /// Create a renderer on the exact device selected for `window`'s surface.
    pub async fn new(
        display: OwnedDisplayHandle,
        window: SurfaceWindow,
    ) -> Result<Self, SurfaceFrameInitError> {
        let reported_scale = window.scale_factor();
        let device_scale = DeviceScale::new(reported_scale as f32)
            .map_err(|_| SurfaceFrameInitError::InvalidDeviceScale(reported_scale))?;
        let surface = SurfaceRuntime::new(display, window).await?;
        let (width, height) = surface.extent().dimensions().unwrap_or((1, 1));
        let device = Arc::new(surface.device());
        let queue = Arc::new(surface.queue());
        let renderer = WgpuRenderer::with_device(
            Arc::clone(&device),
            queue,
            width,
            height,
            surface.format(),
            device_scale.get(),
        );
        let glyph_atlas = WgpuGlyphAtlas::new_with_scale(&device, device_scale.get());

        Ok(Self {
            surface,
            renderer,
            glyph_atlas,
            device_scale,
        })
    }

    /// Current physical drawable state.
    pub const fn extent(&self) -> SurfaceExtent {
        self.surface.extent()
    }

    /// Current drawable size in logical pixels, or `None` while suspended.
    pub fn logical_size(&self) -> Result<Option<GeometrySize<LogicalPixels>>, GeometryError> {
        Ok(drawable_surface(self.surface.extent(), self.device_scale)?
            .map(DrawableSurface::logical_size))
    }

    /// Apply a physical resize to both the surface and render targets.
    pub fn resize_physical(&mut self, width: u32, height: u32) {
        self.surface.resize_physical(width, height);
        self.renderer.resize(width, height);
    }

    /// Apply a host scale-factor change at the window-system boundary.
    pub fn set_scale_factor(&mut self, reported_scale: f64) -> Result<(), SurfaceScaleError> {
        let device_scale = DeviceScale::new(reported_scale as f32)
            .map_err(|_| SurfaceScaleError(reported_scale))?;
        self.device_scale = device_scale;
        self.renderer.set_scale_factor(device_scale.get());
        self.glyph_atlas.set_scale_factor(device_scale.get());
        if let Some((width, height)) = self.surface.extent().dimensions() {
            self.renderer.resize(width, height);
        }
        Ok(())
    }

    /// Render and present one immutable display-protocol frame.
    pub fn present_frame(
        &mut self,
        frame: &FrameGlyphBuffer,
    ) -> Result<PresentationOutcome, SurfaceFramePresentError> {
        let Some(mapping) = frame_mapping(self.surface.extent(), self.device_scale, frame)? else {
            return Ok(PresentationOutcome::Skipped(
                PresentationSkipReason::Suspended,
            ));
        };

        self.glyph_atlas
            .set_current_frame_fonts(frame.font_bindings());
        let Self {
            surface,
            renderer,
            glyph_atlas,
            ..
        } = self;
        surface
            .present(|view| {
                renderer.render_frame_glyphs(
                    view,
                    frame,
                    glyph_atlas,
                    mapping,
                    false,
                    None,
                    (-1.0, -1.0),
                    None,
                    None,
                    None,
                );
            })
            .map_err(Into::into)
    }
}

fn frame_mapping(
    extent: SurfaceExtent,
    device_scale: DeviceScale,
    frame: &FrameGlyphBuffer,
) -> Result<Option<PresentMapping>, GeometryError> {
    let Some(surface) = drawable_surface(extent, device_scale)? else {
        return Ok(None);
    };
    let content = PresentationExtent::new(
        frame.presentation_id,
        GeometrySize::<LogicalPixels>::from_px(frame.width, frame.height)?,
    );
    Ok(Some(PresentMapping::top_left_clip(surface, content)))
}

fn drawable_surface(
    extent: SurfaceExtent,
    device_scale: DeviceScale,
) -> Result<Option<DrawableSurface>, GeometryError> {
    let Some((width, height)) = extent.dimensions() else {
        return Ok(None);
    };
    match SurfaceState::from_device_size(width, height, device_scale)? {
        SurfaceState::Drawable(surface) => Ok(Some(surface)),
        SurfaceState::Suspended => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspended_surface_has_no_frame_mapping() {
        let frame = FrameGlyphBuffer::with_size(320.0, 200.0);

        assert_eq!(
            frame_mapping(SurfaceExtent::Suspended, DeviceScale::ONE, &frame).unwrap(),
            None
        );
    }

    #[test]
    fn drawable_mapping_preserves_logical_frame_and_device_scale() {
        let frame = FrameGlyphBuffer::with_size(320.0, 200.0);
        let mapping = frame_mapping(
            SurfaceExtent::from_physical_size(640, 400),
            DeviceScale::new(2.0).unwrap(),
            &frame,
        )
        .unwrap()
        .expect("drawable surface");

        assert_eq!(mapping.content_logical_size().width(), 320.0);
        assert_eq!(mapping.content_logical_size().height(), 200.0);
        assert_eq!(mapping.surface().device_width().get(), 640);
        assert_eq!(mapping.surface().device_height().get(), 400);
        assert_eq!(mapping.surface().device_scale().get(), 2.0);
    }

    #[test]
    fn invalid_frame_extent_is_rejected_before_gpu_submission() {
        let frame = FrameGlyphBuffer::with_size(f32::NAN, 200.0);

        assert_eq!(
            frame_mapping(
                SurfaceExtent::from_physical_size(640, 400),
                DeviceScale::ONE,
                &frame,
            ),
            Err(GeometryError::InvalidGeometry)
        );
    }
}
