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
    #[error("renderer initialization failed: {0}")]
    RendererValidation(String),
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

/// Whether a direct surface presentation should paint its active cursor.
///
/// Direct product adapters do not run the desktop display runtime's cursor
/// blink state machine. Requiring this value at the presentation boundary
/// keeps bootstrap and editor frames explicit without reducing the decision
/// to an easy-to-invert boolean argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceCursorVisibility {
    Hidden,
    Visible,
}

impl SurfaceCursorVisibility {
    const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }
}

/// Owns every GPU object needed to present a [`FrameGlyphBuffer`].
///
/// Product adapters forward window lifecycle events here; they do not own
/// swapchain recovery, render pipelines, font replay, or device-scale math.
pub struct SurfaceFrameRenderer {
    cursor: neomacs_renderer_wgpu::cursor::CursorState,
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
        let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let renderer = WgpuRenderer::with_device(
            Arc::clone(&device),
            queue,
            width,
            height,
            surface.format(),
            device_scale.get(),
        );
        let glyph_atlas = WgpuGlyphAtlas::new_with_scale(&device, device_scale.get());
        if let Some(error) = validation_scope.pop().await {
            return Err(SurfaceFrameInitError::RendererValidation(error.to_string()));
        }

        Ok(Self {
            cursor: neomacs_renderer_wgpu::cursor::CursorState::new(
                neomacs_display_protocol::frame_time::observe_platform_now(),
            ),
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
        cursor_visibility: SurfaceCursorVisibility,
    ) -> Result<PresentationOutcome, SurfaceFramePresentError> {
        let Some(mapping) = frame_mapping(self.surface.extent(), self.device_scale, frame)? else {
            return Ok(PresentationOutcome::Skipped(
                PresentationSkipReason::Suspended,
            ));
        };

        self.glyph_atlas
            .set_current_frame_fonts(frame.font_bindings());
        // The portable adapter has no predicted presentation timestamp yet.
        // Date every visual in this frame to the same observed draw time.
        let at = neomacs_display_protocol::frame_time::observe_platform_now();
        self.renderer
            .set_frame_sample(neomacs_display_protocol::frame_time::FrameSample::new(
                at,
                std::time::Duration::ZERO,
            ));
        if cursor_visibility.is_visible()
            && let Some(cursor) = frame.active_cursor()
        {
            self.cursor.set_target(
                neomacs_renderer_wgpu::cursor::CursorTarget {
                    window_id: cursor.window_id.get(),
                    x: cursor.x,
                    y: cursor.y,
                    width: cursor.width,
                    height: cursor.height,
                    style: cursor.style,
                    frame_id: frame.frame_placement.frame().get(),
                },
                at,
            );
            self.cursor.tick_animation(at);
            self.cursor.tick_size_animation(at);
        } else {
            self.cursor.clear_target();
        }
        let animated_cursor = self.cursor.animated_cursor();
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
                    cursor_visibility.is_visible(),
                    animated_cursor,
                    None,
                    None,
                    None,
                    None,
                );
            })
            .map_err(Into::into)
    }

    /// Compositor-only work; never asks the VM for another layout frame.
    pub fn animation_interval(
        &self,
        frame: &FrameGlyphBuffer,
        visibility: SurfaceCursorVisibility,
    ) -> Option<std::time::Duration> {
        if !visibility.is_visible() {
            return None;
        }
        let motion_rate = self.cursor.is_animating().then_some(60);
        let cycle_rate = frame
            .window_cursors
            .iter()
            .filter(|cursor| !cursor.style.is_hollow())
            .filter_map(|cursor| {
                let cycle = &frame
                    .effective_window_cursor_effects(cursor.window_id, &self.renderer.effects)
                    .cursor_color_cycle;
                cycle.enabled.then_some(u32::from(cycle.fps.get()).min(60))
            })
            .max();
        motion_rate
            .into_iter()
            .chain(cycle_rate)
            .max()
            .map(|rate| std::time::Duration::from_secs_f64(1.0 / f64::from(rate)))
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

    #[test]
    fn renderer_validation_is_an_initialization_error() {
        let error = SurfaceFrameInitError::RendererValidation("invalid pipeline".into());

        assert_eq!(
            error.to_string(),
            "renderer initialization failed: invalid pipeline"
        );
    }

    #[test]
    fn direct_surface_cursor_visibility_is_explicit() {
        assert!(SurfaceCursorVisibility::Visible.is_visible());
        assert!(!SurfaceCursorVisibility::Hidden.is_visible());
    }
}
