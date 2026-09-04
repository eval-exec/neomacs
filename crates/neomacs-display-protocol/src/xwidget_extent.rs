//! The intrinsic size of an inline xwidget, kept apart from the glyph that
//! places it.
//!
//! GNU keeps three extents for one xwidget glyph and never conflates them:
//!
//! - the widget's own size, `xw->width` / `xw->height`, which is what the
//!   native view is sized to (`x_draw_xwidget_glyph_string` sizes the view
//!   from `xww->width` and only clips it, src/xwidget.c:2841-2849 in
//!   emacs-31.0.90);
//! - the glyph's layout advance, `glyph->pixel_width`, which
//!   `produce_xwidget_glyph` may crop at the right edge of the text area
//!   (src/xdisp.c:32577-32579);
//! - the visible clip, the window's text area (`window_box (s->w, xv->area,
//!   …)`, src/xwidget.c:2841).
//!
//! [`XwidgetContentExtent`] is the first of these.  The glyph matrix and the
//! frame glyph carry it next to the cropped advance so the native placement
//! reads the widget size from here and never from the layout width.

use crate::{
    GeometryError, GeometryPoint, GeometryRect, GeometrySize, LogicalPixels, Px, SpaceTranslation,
};

/// GNU `xw->width` / `xw->height`: the pixel size the xwidget was created
/// with, and therefore the size of the native web view's content area.
///
/// Both dimensions are finite and strictly positive; a widget with no area
/// has no native view to place.  [`XwidgetContentExtent::new`] is the only
/// way in: deserialization goes through it as well, so a serialized frame
/// cannot carry an extent the constructor would refuse.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "XwidgetContentExtentWire")]
pub struct XwidgetContentExtent {
    width_px: f32,
    height_px: f32,
}

/// The serialized shape of [`XwidgetContentExtent`]: the two raw
/// dimensions, validated by [`XwidgetContentExtent::new`] on the way in.
#[derive(serde::Deserialize)]
struct XwidgetContentExtentWire {
    width_px: f32,
    height_px: f32,
}

/// A serialized extent that [`XwidgetContentExtent::new`] refuses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidXwidgetContentExtent {
    /// The rejected width.
    pub width_px: f32,
    /// The rejected height.
    pub height_px: f32,
}

impl std::fmt::Display for InvalidXwidgetContentExtent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "xwidget content extent {} x {} px is not finite and strictly positive",
            self.width_px, self.height_px
        )
    }
}

impl std::error::Error for InvalidXwidgetContentExtent {}

impl TryFrom<XwidgetContentExtentWire> for XwidgetContentExtent {
    type Error = InvalidXwidgetContentExtent;

    fn try_from(wire: XwidgetContentExtentWire) -> Result<Self, Self::Error> {
        Self::new(wire.width_px, wire.height_px).ok_or(InvalidXwidgetContentExtent {
            width_px: wire.width_px,
            height_px: wire.height_px,
        })
    }
}

impl XwidgetContentExtent {
    /// `None` unless both dimensions are finite and strictly positive.
    #[must_use]
    pub fn new(width_px: f32, height_px: f32) -> Option<Self> {
        let valid = |value: f32| value.is_finite() && value > 0.0;
        (valid(width_px) && valid(height_px)).then_some(Self {
            width_px,
            height_px,
        })
    }

    #[must_use]
    pub const fn width_px(self) -> f32 {
        self.width_px
    }

    #[must_use]
    pub const fn height_px(self) -> f32 {
        self.height_px
    }

    fn geometry_size(self) -> GeometrySize<LogicalPixels> {
        GeometrySize::<LogicalPixels>::from_px(self.width_px, self.height_px)
            .expect("an xwidget content extent is finite and positive")
    }
}

/// The horizontal row advance occupied by one xwidget glyph.
///
/// This is deliberately distinct from [`XwidgetContentExtent`].  GNU may
/// crop the advance while keeping the native widget at its intrinsic size;
/// carrying a branded value prevents a caller from accidentally passing the
/// content width where a cropped glyph width is required.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XwidgetLayoutAdvance(Px);

impl XwidgetLayoutAdvance {
    /// Construct a finite, strictly positive glyph advance.
    #[must_use]
    pub fn new(px: Px) -> Option<Self> {
        (px.get().is_finite() && px.get() > 0.0).then_some(Self(px))
    }

    #[must_use]
    pub const fn px(self) -> Px {
        self.0
    }
}

/// Coordinates inside the xwidget's own content surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum XwidgetContentSpace {}

/// The complete portable geometry contract for one presented xwidget.
///
/// `Space` brands the coordinate system of the origin and clip.  Layout emits
/// frame-space geometry; child-frame ingestion translates it into root-surface
/// geometry.  The intrinsic content extent and cropped layout advance remain
/// different types throughout that conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XwidgetPresentationGeometry<Space> {
    origin: GeometryPoint<Space, LogicalPixels>,
    content: XwidgetContentExtent,
    layout_advance: XwidgetLayoutAdvance,
    clip: Option<GeometryRect<Space, LogicalPixels>>,
}

impl<Space: Copy> XwidgetPresentationGeometry<Space> {
    #[must_use]
    pub const fn new(
        origin: GeometryPoint<Space, LogicalPixels>,
        content: XwidgetContentExtent,
        layout_advance: XwidgetLayoutAdvance,
        clip: Option<GeometryRect<Space, LogicalPixels>>,
    ) -> Self {
        Self {
            origin,
            content,
            layout_advance,
            clip,
        }
    }

    #[must_use]
    pub const fn origin(&self) -> &GeometryPoint<Space, LogicalPixels> {
        &self.origin
    }

    #[must_use]
    pub const fn content_extent(self) -> XwidgetContentExtent {
        self.content
    }

    #[must_use]
    pub const fn layout_advance(self) -> XwidgetLayoutAdvance {
        self.layout_advance
    }

    #[must_use]
    pub const fn clip_rect(self) -> Option<GeometryRect<Space, LogicalPixels>> {
        self.clip
    }

    /// Replace the presentation clip while preserving intrinsic content and
    /// layout geometry.  Frame builders use this to install their current
    /// draw-context clip exactly once.
    #[must_use]
    pub const fn with_clip(mut self, clip: Option<GeometryRect<Space, LogicalPixels>>) -> Self {
        self.clip = clip;
        self
    }

    #[must_use]
    pub fn content_rect(self) -> GeometryRect<Space, LogicalPixels> {
        GeometryRect::from_origin_and_size(self.origin, self.content.geometry_size())
    }

    #[must_use]
    pub fn layout_slot_rect(self) -> GeometryRect<Space, LogicalPixels> {
        GeometryRect::from_origin_and_size(
            self.origin,
            GeometrySize::<LogicalPixels>::from_px(
                self.layout_advance.px().get(),
                self.content.height_px(),
            )
            .expect("an xwidget advance and content height are finite and positive"),
        )
    }

    /// Resolve the native/GPU-visible part of the widget through its text-area
    /// clip and an optional enclosing-frame clip.
    pub fn resolve_visible(
        self,
        enclosing_clip: Option<GeometryRect<Space, LogicalPixels>>,
    ) -> Result<Option<XwidgetVisibleGeometry<Space>>, GeometryError> {
        let content = self.content_rect();
        let visible = match self.clip {
            Some(clip) => content.try_intersection(clip)?,
            None => Some(content),
        };
        let visible = match (visible, enclosing_clip) {
            (Some(visible), Some(clip)) => visible.try_intersection(clip)?,
            (visible, None) => visible,
            (None, _) => None,
        };
        Ok(visible.map(|visible| XwidgetVisibleGeometry { content, visible }))
    }

    /// Translate the presentation while changing its coordinate-space brand.
    pub fn translated<To: Copy>(
        self,
        translation: SpaceTranslation<Space, To, LogicalPixels>,
    ) -> Result<XwidgetPresentationGeometry<To>, GeometryError> {
        let origin = translation.map_point(&self.origin)?;
        let clip = self
            .clip
            .map(|clip| translation.map_rect(clip))
            .transpose()?;
        Ok(XwidgetPresentationGeometry::new(
            origin,
            self.content,
            self.layout_advance,
            clip,
        ))
    }

    /// Move the glyph within the same coordinate space without moving its
    /// enclosing window clip.
    ///
    /// This is distinct from [`Self::translated`], which maps the complete
    /// presentation into another coordinate space and therefore translates
    /// both origin and clip.  Post-layout glyph spacing moves only the glyph.
    pub fn translated_origin(
        self,
        displacement: SpaceTranslation<Space, Space, LogicalPixels>,
    ) -> Result<Self, GeometryError> {
        Ok(Self::new(
            displacement.map_point(&self.origin)?,
            self.content,
            self.layout_advance,
            self.clip,
        ))
    }
}

/// Intrinsic and clipped rectangles after resolving one presentation.
///
/// All consumers use this result: native placement reads both rectangles,
/// Linux composition additionally reads the texture coordinates, and pointer
/// dispatch maps a visible point into [`XwidgetContentSpace`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XwidgetVisibleGeometry<Space> {
    content: GeometryRect<Space, LogicalPixels>,
    visible: GeometryRect<Space, LogicalPixels>,
}

impl<Space: Copy> XwidgetVisibleGeometry<Space> {
    #[must_use]
    pub const fn content_rect(self) -> GeometryRect<Space, LogicalPixels> {
        self.content
    }

    #[must_use]
    pub const fn visible_rect(self) -> GeometryRect<Space, LogicalPixels> {
        self.visible
    }

    #[must_use]
    pub fn texture_coordinates(self) -> XwidgetTextureCoordinates {
        XwidgetTextureCoordinates {
            u_min: (self.visible.x() - self.content.x()) / self.content.width(),
            u_max: (self.visible.x() + self.visible.width() - self.content.x())
                / self.content.width(),
            v_min: (self.visible.y() - self.content.y()) / self.content.height(),
            v_max: (self.visible.y() + self.visible.height() - self.content.y())
                / self.content.height(),
        }
    }

    /// Map a visible point in the presentation's coordinate space into the
    /// widget's intrinsic content coordinates.
    #[must_use]
    pub fn content_point_at(
        self,
        point: GeometryPoint<Space, LogicalPixels>,
    ) -> Option<GeometryPoint<XwidgetContentSpace, LogicalPixels>> {
        let inside = point.x() >= self.visible.x()
            && point.x() < self.visible.x() + self.visible.width()
            && point.y() >= self.visible.y()
            && point.y() < self.visible.y() + self.visible.height();
        inside.then(|| {
            GeometryPoint::<XwidgetContentSpace, LogicalPixels>::from_px(
                point.x() - self.content.x(),
                point.y() - self.content.y(),
            )
            .expect("the difference of finite visible and content coordinates is finite")
        })
    }
}

/// Normalized source coordinates for the visible part of an xwidget texture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XwidgetTextureCoordinates {
    u_min: f32,
    u_max: f32,
    v_min: f32,
    v_max: f32,
}

impl XwidgetTextureCoordinates {
    #[must_use]
    pub const fn as_array(self) -> [f32; 4] {
        [self.u_min, self.u_max, self.v_min, self.v_max]
    }
}

#[cfg(test)]
#[path = "xwidget_extent_test.rs"]
mod tests;
