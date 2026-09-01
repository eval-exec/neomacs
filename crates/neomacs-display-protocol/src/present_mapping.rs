//! Mapping between an immutable evaluator presentation and a live native surface.
//!
//! A presentation and a surface advance on different clocks during resize.  This
//! module keeps their dimensions distinct and resolves the only supported text-UI
//! policy: preserve logical size at the top-left and clip to the drawable surface.

use std::num::NonZeroU32;

use crate::frame_chrome::PresentationId;
use crate::geometry::{
    DeviceScale, FrameSpace, GeometryError, GeometryPoint, GeometryRect, GeometrySize,
    GeometryUnit, LogicalPixels, RootSurfaceSpace,
};

/// Device-pixel coordinates on the live native surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevicePixels(f32);

impl DevicePixels {
    pub fn new(value: f32) -> Result<Self, GeometryError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(GeometryError::InvalidGeometry)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl GeometryUnit for DevicePixels {
    fn valid_coordinate(self) -> bool {
        self.0.is_finite()
    }

    fn valid_extent(self) -> bool {
        self.valid_coordinate() && self.0 >= 0.0
    }
}

/// The coordinate space of a winit/wgpu surface, before conversion to logical pixels.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeviceSurfaceSpace {}

pub type DeviceSurfacePoint = GeometryPoint<DeviceSurfaceSpace, DevicePixels>;
pub type PresentedFramePoint = GeometryPoint<FrameSpace, LogicalPixels>;

impl DeviceSurfacePoint {
    pub fn from_px(x: f32, y: f32) -> Result<Self, GeometryError> {
        Self::try_from_units(DevicePixels::new(x)?, DevicePixels::new(y)?)
    }

    #[must_use]
    pub const fn x(&self) -> f32 {
        self.x_unit().get()
    }

    #[must_use]
    pub const fn y(&self) -> f32 {
        self.y_unit().get()
    }
}

/// A non-zero native surface whose logical extent is known to be finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawableSurface {
    device_width: NonZeroU32,
    device_height: NonZeroU32,
    device_scale: DeviceScale,
    logical_size: GeometrySize<LogicalPixels>,
}

impl DrawableSurface {
    pub fn new(
        device_width: NonZeroU32,
        device_height: NonZeroU32,
        device_scale: DeviceScale,
    ) -> Result<Self, GeometryError> {
        let logical_size = GeometrySize::<LogicalPixels>::from_px(
            device_width.get() as f32 / device_scale.get(),
            device_height.get() as f32 / device_scale.get(),
        )?;
        Ok(Self {
            device_width,
            device_height,
            device_scale,
            logical_size,
        })
    }

    #[must_use]
    pub const fn device_width(self) -> NonZeroU32 {
        self.device_width
    }

    #[must_use]
    pub const fn device_height(self) -> NonZeroU32 {
        self.device_height
    }

    #[must_use]
    pub const fn device_scale(self) -> DeviceScale {
        self.device_scale
    }

    #[must_use]
    pub const fn logical_size(self) -> GeometrySize<LogicalPixels> {
        self.logical_size
    }
}

/// Native surface lifecycle. A zero extent is suspension, not drawable geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceState {
    Suspended,
    Drawable(DrawableSurface),
}

impl SurfaceState {
    pub fn from_device_size(
        width: u32,
        height: u32,
        scale: DeviceScale,
    ) -> Result<Self, GeometryError> {
        let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return Ok(Self::Suspended);
        };
        DrawableSurface::new(width, height, scale).map(Self::Drawable)
    }
}

/// The immutable source extent attached to one evaluator presentation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentationExtent {
    presentation: PresentationId,
    logical_size: GeometrySize<LogicalPixels>,
}

impl PresentationExtent {
    #[must_use]
    pub const fn new(
        presentation: PresentationId,
        logical_size: GeometrySize<LogicalPixels>,
    ) -> Self {
        Self {
            presentation,
            logical_size,
        }
    }

    #[must_use]
    pub const fn presentation(self) -> PresentationId {
        self.presentation
    }

    #[must_use]
    pub const fn logical_size(self) -> GeometrySize<LogicalPixels> {
        self.logical_size
    }
}

/// A resolved top-left, one-logical-pixel-to-one-logical-pixel presentation.
///
/// There is deliberately no stretch policy and no scalar convenience
/// constructor: callers must name both the live surface and immutable source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentMapping {
    surface: DrawableSurface,
    content: PresentationExtent,
    visible_content: Option<GeometryRect<RootSurfaceSpace, LogicalPixels>>,
}

impl PresentMapping {
    #[must_use]
    pub fn top_left_clip(surface: DrawableSurface, content: PresentationExtent) -> Self {
        let surface_size = surface.logical_size();
        let content_size = content.logical_size();
        let width = surface_size.width().min(content_size.width());
        let height = surface_size.height().min(content_size.height());
        let visible_content = if width == 0.0 || height == 0.0 {
            None
        } else {
            Some(
                GeometryRect::<RootSurfaceSpace, LogicalPixels>::new(0.0, 0.0, width, height)
                    .expect("minimums of validated extents remain valid geometry"),
            )
        };
        Self {
            surface,
            content,
            visible_content,
        }
    }

    #[must_use]
    pub const fn presentation(self) -> PresentationId {
        self.content.presentation()
    }

    #[must_use]
    pub const fn content_logical_size(self) -> GeometrySize<LogicalPixels> {
        self.content.logical_size()
    }

    #[must_use]
    pub const fn surface(self) -> DrawableSurface {
        self.surface
    }

    #[must_use]
    pub const fn surface_logical_size(self) -> GeometrySize<LogicalPixels> {
        self.surface.logical_size()
    }

    #[must_use]
    pub const fn visible_content_rect(
        self,
    ) -> Option<GeometryRect<RootSurfaceSpace, LogicalPixels>> {
        self.visible_content
    }

    pub fn device_from_frame(
        self,
        point: PresentedFramePoint,
    ) -> Result<DeviceSurfacePoint, GeometryError> {
        let scale = self.surface.device_scale().get();
        DeviceSurfacePoint::from_px(point.x() * scale, point.y() * scale)
    }

    #[must_use]
    pub fn frame_from_device(self, point: DeviceSurfacePoint) -> Option<PresentedFramePoint> {
        let scale = self.surface.device_scale().get();
        let surface_point = GeometryPoint::<RootSurfaceSpace, LogicalPixels>::from_px(
            point.x() / scale,
            point.y() / scale,
        )
        .ok()?;
        self.frame_from_surface(surface_point)
    }

    #[must_use]
    pub fn frame_from_surface(
        self,
        point: GeometryPoint<RootSurfaceSpace, LogicalPixels>,
    ) -> Option<PresentedFramePoint> {
        let x = point.x();
        let y = point.y();
        let visible = self.visible_content?;
        if x < visible.x()
            || y < visible.y()
            || x >= visible.x() + visible.width()
            || y >= visible.y() + visible.height()
        {
            return None;
        }
        PresentedFramePoint::from_px(x - visible.x(), y - visible.y()).ok()
    }
}
