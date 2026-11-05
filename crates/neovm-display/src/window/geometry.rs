//! Coordinate-safe views over redisplay's existing window snapshots.
//!
//! A `PresentationGeometry` owns one immutable frame publication and exposes typed
//! pixel and cell views so consumers cannot silently combine body-local,
//! window-local, frame-local, and cell-grid values.
//!
//! Layer boundary: this module answers "where was this window PRESENTED"
//! from renderer publications (post-redisplay). The Lisp-facing edge
//! builtins in emacs_core/window_cmds deliberately do NOT route through
//! the presentation queries below -- window-edges and friends answer from
//! the live Window tree (Window::bounds), matching GNU, which computes
//! window.c edges from struct window fields without consulting the glyph
//! matrix. Body, chrome and scroll-bar area queries do read the same
//! publication, as they always have, through [`PublishedWindowRegions`]:
//! a borrowed, per-rectangle-validated view, so a query that needs one
//! width does not pay for the fourteen-rectangle owned conversion.

use super::{FrameId, LispCharPos1, WindowChromeLine, WindowId};
use neomacs_display_protocol::frame_glyphs::PresentedWindowRegions;
use neomacs_display_protocol::types::Rect as TransportRect;
use std::marker::PhantomData;

mod positions;

pub use neovm_eval::emacs_core::eval::data::window_state::PresentationId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationPrepareError {
    ReusedPresentation(PresentationId),
    InvalidGeometry(GeometryError),
}

pub use neovm_eval::emacs_core::eval::data::window_state::PresentationActivateError;
pub use neovm_eval::emacs_core::eval::data::window_state::{
    CellOrigin, FrameLogicalSpace, GeometryError, LogicalPx, PixelPoint, PixelRect,
    PresentationFrame, PresentationGeometry, PresentationWindow, WindowBodySpace, WindowLocalSpace,
    WindowRegions,
};
pub(crate) use neovm_eval::emacs_core::eval::data::window_state::{
    PresentationPosition, PresentedBufferPositionMatch,
};

/// Borrowed view of the rectangle table one redisplay publication carries.
///
/// The Lisp geometry primitives (`window-body-width`, `window-fringes`,
/// `window-mode-line-height`, …) each read one or two rectangles out of this
/// table.  Converting the whole publication into [`WindowRegions`] validates
/// and copies fourteen rectangles for every such query -- 24 conversions per
/// C-n/C-p pair in the profile.  This view validates exactly the rectangle it
/// returns, with the same predicate, so a malformed publication still
/// surfaces the same [`GeometryError`] for the value that was requested.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PublishedWindowRegions<'a> {
    regions: &'a PresentedWindowRegions,
}

impl<'a> PublishedWindowRegions<'a> {
    pub(crate) const fn new(regions: &'a PresentedWindowRegions) -> Self {
        Self { regions }
    }

    /// Validate one published rectangle and report its integer extent.
    fn extent_pixels(rect: &TransportRect) -> Result<(i64, i64), GeometryError> {
        let rect = PixelRect::<FrameLogicalSpace>::from_transport(rect)?;
        Ok((rect.width().get() as i64, rect.height().get() as i64))
    }

    pub(crate) fn text_body_width_pixels(self) -> Result<i64, GeometryError> {
        Self::extent_pixels(&self.regions.text_body).map(|(width, _)| width)
    }

    pub(crate) fn text_body_height_pixels(self) -> Result<i64, GeometryError> {
        Self::extent_pixels(&self.regions.text_body).map(|(_, height)| height)
    }

    /// Margin columns are stored scalars in the publication, not a projection
    /// of a rectangle, so reading them needs no geometry conversion.
    pub(crate) fn left_margin_columns(self) -> i64 {
        self.regions.left_margin_columns
    }

    pub(crate) fn right_margin_columns(self) -> i64 {
        self.regions.right_margin_columns
    }

    pub(crate) fn fringe_widths_pixels(self) -> Result<(i64, i64), GeometryError> {
        let width = |rect: Option<TransportRect>| {
            rect.map(|rect| Self::extent_pixels(&rect).map(|(width, _)| width))
                .transpose()
                .map(Option::unwrap_or_default)
        };
        Ok((
            width(self.regions.left_fringe)?,
            width(self.regions.right_fringe)?,
        ))
    }

    pub(crate) fn scroll_bar_width_pixels(self) -> Result<i64, GeometryError> {
        match (self.regions.left_scroll_bar, self.regions.right_scroll_bar) {
            (Some(rect), _) | (None, Some(rect)) => {
                Self::extent_pixels(&rect).map(|(width, _)| width)
            }
            (None, None) => Ok(0),
        }
    }

    pub(crate) fn horizontal_scroll_bar_height_pixels(self) -> Result<i64, GeometryError> {
        self.regions
            .horizontal_scroll_bar
            .map(|rect| Self::extent_pixels(&rect).map(|(_, height)| height))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    /// Height of one window-chrome line, or zero when redisplay published no
    /// such row for this window.
    pub(crate) fn chrome_height_pixels(self, line: WindowChromeLine) -> Result<i64, GeometryError> {
        let rect = match line {
            WindowChromeLine::ModeLine => self.regions.mode_line,
            WindowChromeLine::HeaderLine => self.regions.header_line,
            WindowChromeLine::TabLine => self.regions.tab_line,
        };
        rect.map(|rect| Self::extent_pixels(&rect).map(|(_, height)| height))
            .transpose()
            .map(Option::unwrap_or_default)
    }
}

mod query_seal {
    pub trait Sealed {}
}

/// A closed semantic request against one immutable geometry publication.
pub trait GeometryQuery: query_seal::Sealed {
    type Output<'a>;

    #[doc(hidden)]
    fn presentation(&self) -> PresentationId;

    #[doc(hidden)]
    fn resolve_presented<'a>(
        self,
        geometry: &'a PresentationGeometry,
    ) -> Result<Self::Output<'a>, GeometryQueryError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryQueryError {
    NotYetActive {
        frame: FrameId,
    },
    StalePresentation {
        requested: PresentationId,
        available: PresentationId,
    },
    MissingWindow(WindowId),
    MissingMaterializedGeometry(WindowId),
    MissingRegion {
        window: WindowId,
        region: WindowRegion,
    },
    PositionNotVisible {
        window: WindowId,
        position: LispCharPos1,
    },
    CoordinateNotVisible {
        window: WindowId,
        x: i64,
        y: i64,
    },
    VisualAnchorUnavailable(VisualAnchor),
    InvalidGeometry(GeometryError),
}

/// Semantic edge used to attach a popup or child frame to active geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnchorEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// A durable description of what should be anchored, independent of pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualAnchor {
    CursorBottom {
        window: WindowId,
    },
    BufferPositionBottom {
        window: WindowId,
        position: LispCharPos1,
    },
    WindowRegionEdge {
        window: WindowId,
        region: WindowRegion,
        edge: AnchorEdge,
    },
}

impl VisualAnchor {
    const fn window(self) -> WindowId {
        match self {
            Self::CursorBottom { window }
            | Self::BufferPositionBottom { window, .. }
            | Self::WindowRegionEdge { window, .. } => window,
        }
    }
}

/// Resolve one semantic anchor against one explicitly named presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisualAnchorQuery {
    presentation: PresentationId,
    anchor: VisualAnchor,
}

impl VisualAnchorQuery {
    pub const fn new(presentation: PresentationId, anchor: VisualAnchor) -> Self {
        Self {
            presentation,
            anchor,
        }
    }

    pub const fn presentation(self) -> PresentationId {
        self.presentation
    }
}

impl query_seal::Sealed for VisualAnchorQuery {}

impl GeometryQuery for VisualAnchorQuery {
    type Output<'a> = VisualAnchorGeometry;

    fn presentation(&self) -> PresentationId {
        self.presentation
    }

    fn resolve_presented<'a>(
        self,
        geometry: &'a PresentationGeometry,
    ) -> Result<Self::Output<'a>, GeometryQueryError> {
        let window_id = self.anchor.window();
        let (bounds, edge) = match self.anchor {
            VisualAnchor::CursorBottom { window } => {
                let view = geometry.resolve(WindowGeometryQuery::new(self.presentation, window))?;
                let cursor = view
                    .window_geometry
                    .cursor
                    .ok_or(GeometryQueryError::VisualAnchorUnavailable(self.anchor))?;
                let body = view
                    .text_body_origin_in_frame()
                    .map_err(GeometryQueryError::InvalidGeometry)?;
                (
                    PixelRect {
                        origin: PixelPoint::new(
                            body.x().get() + cursor.x as f32,
                            body.y().get() + cursor.body_y as f32,
                        )
                        .map_err(GeometryQueryError::InvalidGeometry)?,
                        width: LogicalPx::from_i64(cursor.width.max(0)),
                        height: LogicalPx::from_i64(cursor.height.max(0)),
                    },
                    AnchorEdge::Bottom,
                )
            }
            VisualAnchor::BufferPositionBottom { window, position } => {
                let point = geometry.resolve(BufferPositionQuery::new(
                    self.presentation,
                    window,
                    position,
                ))?;
                (
                    PixelRect {
                        origin: point.in_frame().point,
                        width: point.width(),
                        height: point.height(),
                    },
                    AnchorEdge::Bottom,
                )
            }
            VisualAnchor::WindowRegionEdge {
                window,
                region,
                edge,
            } => (
                geometry.resolve(WindowRegionBoundsQuery::new(
                    self.presentation,
                    window,
                    region,
                ))?,
                edge,
            ),
        };
        Ok(VisualAnchorGeometry {
            presentation: self.presentation,
            frame: geometry.frame.id,
            window: window_id,
            bounds,
            edge,
        })
    }
}

/// Presentation-qualified, frame-local placement result for a semantic anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualAnchorGeometry {
    presentation: PresentationId,
    frame: FrameId,
    window: WindowId,
    bounds: PixelRect<FrameLogicalSpace>,
    edge: AnchorEdge,
}

impl VisualAnchorGeometry {
    pub const fn presentation(self) -> PresentationId {
        self.presentation
    }

    pub const fn frame(self) -> FrameId {
        self.frame
    }

    pub const fn window(self) -> WindowId {
        self.window
    }

    pub const fn bounds_in_frame(self) -> PixelRect<FrameLogicalSpace> {
        self.bounds
    }

    pub const fn edge(self) -> AnchorEdge {
        self.edge
    }

    /// Edge attachment point. Horizontal edges attach at their left endpoint;
    /// vertical edges attach at their top endpoint.
    pub const fn x(self) -> LogicalPx {
        match self.edge {
            AnchorEdge::Left | AnchorEdge::Top | AnchorEdge::Bottom => self.bounds.origin.x,
            AnchorEdge::Right => LogicalPx(self.bounds.origin.x.0 + self.bounds.width.0),
        }
    }

    pub const fn y(self) -> LogicalPx {
        match self.edge {
            AnchorEdge::Top | AnchorEdge::Left | AnchorEdge::Right => self.bounds.origin.y,
            AnchorEdge::Bottom => LogicalPx(self.bounds.origin.y.0 + self.bounds.height.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowGeometryQuery {
    presentation: PresentationId,
    window: WindowId,
}

impl WindowGeometryQuery {
    pub const fn new(presentation: PresentationId, window: WindowId) -> Self {
        Self {
            presentation,
            window,
        }
    }
}

impl query_seal::Sealed for WindowGeometryQuery {}

impl GeometryQuery for WindowGeometryQuery {
    type Output<'a> = SnapshotWindowGeometry<'a>;

    fn presentation(&self) -> PresentationId {
        self.presentation
    }

    fn resolve_presented<'a>(
        self,
        geometry: &'a PresentationGeometry,
    ) -> Result<Self::Output<'a>, GeometryQueryError> {
        let window = geometry
            .window(self.window)
            .ok_or(GeometryQueryError::MissingWindow(self.window))?;
        SnapshotWindowGeometry::new(
            geometry.presentation,
            geometry.frame.id,
            self.window,
            window,
        )
        .ok_or(GeometryQueryError::MissingMaterializedGeometry(self.window))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnownWindowGeometryQuery {
    presentation: PresentationId,
    window: WindowId,
}

impl KnownWindowGeometryQuery {
    pub const fn new(presentation: PresentationId, window: WindowId) -> Self {
        Self {
            presentation,
            window,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnownWindowGeometry {
    outer: PixelRect<FrameLogicalSpace>,
    cell_origin: CellOrigin,
}

impl KnownWindowGeometry {
    pub const fn outer(self) -> PixelRect<FrameLogicalSpace> {
        self.outer
    }
    pub const fn cell_origin(self) -> CellOrigin {
        self.cell_origin
    }
}

impl query_seal::Sealed for KnownWindowGeometryQuery {}

impl GeometryQuery for KnownWindowGeometryQuery {
    type Output<'a> = KnownWindowGeometry;

    fn presentation(&self) -> PresentationId {
        self.presentation
    }

    fn resolve_presented<'a>(
        self,
        geometry: &'a PresentationGeometry,
    ) -> Result<Self::Output<'a>, GeometryQueryError> {
        let window = geometry
            .window(self.window)
            .ok_or(GeometryQueryError::MissingWindow(self.window))?;
        Ok(KnownWindowGeometry {
            outer: window.outer,
            cell_origin: window.cell_origin,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowRegion {
    Outer,
    TextBody,
    LeftMargin,
    RightMargin,
    LeftFringe,
    RightFringe,
    LeftScrollBar,
    RightScrollBar,
    HorizontalScrollBar,
    TabLine,
    HeaderLine,
    ModeLine,
    RightDivider,
    BottomDivider,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowRegionBoundsQuery {
    presentation: PresentationId,
    window: WindowId,
    region: WindowRegion,
}

impl WindowRegionBoundsQuery {
    pub const fn new(presentation: PresentationId, window: WindowId, region: WindowRegion) -> Self {
        Self {
            presentation,
            window,
            region,
        }
    }
}

impl query_seal::Sealed for WindowRegionBoundsQuery {}

impl GeometryQuery for WindowRegionBoundsQuery {
    type Output<'a> = PixelRect<FrameLogicalSpace>;

    fn presentation(&self) -> PresentationId {
        self.presentation
    }

    fn resolve_presented<'a>(
        self,
        geometry: &'a PresentationGeometry,
    ) -> Result<Self::Output<'a>, GeometryQueryError> {
        let window = geometry
            .window(self.window)
            .ok_or(GeometryQueryError::MissingWindow(self.window))?;
        let regions = window
            .regions
            .as_ref()
            .ok_or(GeometryQueryError::MissingMaterializedGeometry(self.window))?;
        let rect = match self.region {
            WindowRegion::Outer => Some(regions.outer),
            WindowRegion::TextBody => Some(regions.text_body),
            WindowRegion::LeftMargin => regions.left_margin,
            WindowRegion::RightMargin => regions.right_margin,
            WindowRegion::LeftFringe => regions.left_fringe,
            WindowRegion::RightFringe => regions.right_fringe,
            WindowRegion::LeftScrollBar => regions.left_scroll_bar,
            WindowRegion::RightScrollBar => regions.right_scroll_bar,
            WindowRegion::HorizontalScrollBar => regions.horizontal_scroll_bar,
            WindowRegion::TabLine => regions.tab_line,
            WindowRegion::HeaderLine => regions.header_line,
            WindowRegion::ModeLine => regions.mode_line,
            WindowRegion::RightDivider => regions.right_divider,
            WindowRegion::BottomDivider => regions.bottom_divider,
        }
        .ok_or(GeometryQueryError::MissingRegion {
            window: self.window,
            region: self.region,
        })?;
        Ok(rect)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferPositionQuery {
    presentation: PresentationId,
    window: WindowId,
    position: LispCharPos1,
}

impl BufferPositionQuery {
    pub const fn new(
        presentation: PresentationId,
        window: WindowId,
        position: LispCharPos1,
    ) -> Self {
        Self {
            presentation,
            window,
            position,
        }
    }
}

impl query_seal::Sealed for BufferPositionQuery {}

impl GeometryQuery for BufferPositionQuery {
    type Output<'a> = SnapshotPointGeometry;

    fn presentation(&self) -> PresentationId {
        self.presentation
    }

    fn resolve_presented<'a>(
        self,
        geometry: &'a PresentationGeometry,
    ) -> Result<Self::Output<'a>, GeometryQueryError> {
        let window = geometry.resolve(WindowGeometryQuery::new(self.presentation, self.window))?;
        window
            .point_for_buffer_pos(self.position)
            .map_err(GeometryQueryError::InvalidGeometry)?
            .ok_or(GeometryQueryError::PositionNotVisible {
                window: self.window,
                position: self.position,
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CoordinateInput {
    TextBody { x: i64, y: i64 },
    WholeWindow { x: i64, y: i64 },
    Frame(neomacs_display_protocol::PresentedFramePoint),
}

/// Resolve one pixel coordinate against the exact immutable presentation that
/// supplied the window regions and visible glyph positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCoordinateQuery {
    presentation: PresentationId,
    window: WindowId,
    input: CoordinateInput,
}

impl WindowCoordinateQuery {
    pub const fn in_text_body(
        presentation: PresentationId,
        window: WindowId,
        x: i64,
        window_y: i64,
    ) -> Self {
        Self {
            presentation,
            window,
            input: CoordinateInput::TextBody { x, y: window_y },
        }
    }

    pub const fn in_whole_window(
        presentation: PresentationId,
        window: WindowId,
        x: i64,
        y: i64,
    ) -> Self {
        Self {
            presentation,
            window,
            input: CoordinateInput::WholeWindow { x, y },
        }
    }

    pub const fn in_frame(
        presentation: PresentationId,
        window: WindowId,
        point: neomacs_display_protocol::PresentedFramePoint,
    ) -> Self {
        Self {
            presentation,
            window,
            input: CoordinateInput::Frame(point),
        }
    }
}

impl query_seal::Sealed for WindowCoordinateQuery {}

impl GeometryQuery for WindowCoordinateQuery {
    type Output<'a> = SnapshotPointGeometry;

    fn presentation(&self) -> PresentationId {
        self.presentation
    }

    fn resolve_presented<'a>(
        self,
        geometry: &'a PresentationGeometry,
    ) -> Result<Self::Output<'a>, GeometryQueryError> {
        let window = geometry.resolve(WindowGeometryQuery::new(self.presentation, self.window))?;
        let body_origin = window.text_body_origin_in_window();
        let outer_origin = window.outer_in_frame().origin();
        let (source_x, source_y, body_x, window_y) = match self.input {
            CoordinateInput::TextBody { x, y } => (x, y, x, y),
            CoordinateInput::WholeWindow { x, y } => (x, y, x - body_origin.x().get() as i64, y),
            CoordinateInput::Frame(point) => {
                let x = point.x() as i64;
                let y = point.y() as i64;
                (
                    x,
                    y,
                    x - outer_origin.x().get() as i64 - body_origin.x().get() as i64,
                    y - outer_origin.y().get() as i64,
                )
            }
        };
        window
            .point_at_window_coords(body_x, window_y)
            .map_err(GeometryQueryError::InvalidGeometry)?
            .ok_or(GeometryQueryError::CoordinateNotVisible {
                window: self.window,
                x: source_x,
                y: source_y,
            })
    }
}

/// A frame-owned point.  The owner prevents points from different frames from
/// being treated as interchangeable merely because both are frame-relative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramePoint {
    frame: FrameId,
    point: PixelPoint<FrameLogicalSpace>,
}

impl FramePoint {
    pub const fn frame(self) -> FrameId {
        self.frame
    }

    pub const fn x(self) -> LogicalPx {
        self.point.x()
    }

    pub const fn y(self) -> LogicalPx {
        self.point.y()
    }
}

/// A window-owned point in a statically named window coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPoint<Space> {
    window: WindowId,
    point: PixelPoint<Space>,
}

impl<Space> WindowPoint<Space> {
    pub const fn window(self) -> WindowId {
        self.window
    }

    pub const fn x(self) -> LogicalPx {
        self.point.x()
    }

    pub const fn y(self) -> LogicalPx {
        self.point.y()
    }
}

/// Typed geometry for one visible source position in a window snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapshotPointGeometry {
    buffer_pos: LispCharPos1,
    body_point: WindowPoint<WindowBodySpace>,
    frame_point: FramePoint,
    width: LogicalPx,
    height: LogicalPx,
    row: i64,
    column: i64,
}

impl SnapshotPointGeometry {
    pub const fn buffer_pos(self) -> LispCharPos1 {
        self.buffer_pos
    }

    pub const fn in_text_body(self) -> WindowPoint<WindowBodySpace> {
        self.body_point
    }

    pub const fn in_frame(self) -> FramePoint {
        self.frame_point
    }

    pub const fn width(self) -> LogicalPx {
        self.width
    }

    pub const fn height(self) -> LogicalPx {
        self.height
    }

    pub const fn row(self) -> i64 {
        self.row
    }

    pub const fn column(self) -> i64 {
        self.column
    }
}

/// A borrowed, coordinate-safe view into one immutable presentation geometry.
#[derive(Debug)]
pub struct SnapshotWindowGeometry<'a> {
    presentation: PresentationId,
    frame: FrameId,
    window: WindowId,
    window_geometry: &'a PresentationWindow,
    regions: &'a WindowRegions,
    outer: PixelRect<FrameLogicalSpace>,
}

impl<'a> SnapshotWindowGeometry<'a> {
    fn new(
        presentation: PresentationId,
        frame: FrameId,
        window: WindowId,
        window_geometry: &'a PresentationWindow,
    ) -> Option<Self> {
        let regions = window_geometry.regions.as_ref()?;
        Some(Self {
            presentation,
            frame,
            window,
            window_geometry,
            regions,
            outer: regions.outer,
        })
    }

    pub const fn presentation(&self) -> PresentationId {
        self.presentation
    }

    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    pub fn window(&self) -> WindowId {
        self.window
    }

    pub const fn outer_in_frame(&self) -> PixelRect<FrameLogicalSpace> {
        self.outer
    }

    pub const fn regions(&self) -> WindowRegions {
        *self.regions
    }

    pub fn cell_origin(&self) -> CellOrigin {
        self.window_geometry.cell_origin
    }

    pub fn text_body_origin_in_window(&self) -> WindowPoint<WindowLocalSpace> {
        WindowPoint {
            window: self.window,
            point: PixelPoint {
                x: LogicalPx(
                    self.regions.text_body.origin.x.get() - self.regions.outer.origin.x.get(),
                ),
                y: LogicalPx(
                    self.regions.text_body.origin.y.get() - self.regions.outer.origin.y.get(),
                ),
                space: PhantomData,
            },
        }
    }

    pub fn text_body_origin_in_frame(&self) -> Result<FramePoint, GeometryError> {
        let local = self.text_body_origin_in_window();
        Ok(FramePoint {
            frame: self.frame,
            point: PixelPoint::new(
                self.outer.origin().x().get() + local.x().get(),
                self.outer.origin().y().get() + local.y().get(),
            )?,
        })
    }

    pub fn point_for_buffer_pos(
        &self,
        buffer_pos: LispCharPos1,
    ) -> Result<Option<SnapshotPointGeometry>, GeometryError> {
        let positions = self.window_geometry.positions.as_slice();
        let idx = positions.partition_point(|point| point.buffer_pos < buffer_pos);
        let next = positions.get(idx);
        let previous = idx
            .checked_sub(1)
            .and_then(|previous| positions.get(previous));
        let matched = match (previous, next) {
            (_, Some(point)) if point.buffer_pos == buffer_pos => {
                PresentedBufferPositionMatch::Exact(point)
            }
            (Some(previous), Some(next))
                if previous.buffer_pos < buffer_pos
                    && buffer_pos < next.buffer_pos
                    && previous.body_row == next.body_row =>
            {
                PresentedBufferPositionMatch::HiddenOnRow { next }
            }
            _ => PresentedBufferPositionMatch::NotVisible,
        };
        let point = match matched {
            PresentedBufferPositionMatch::Exact(point)
            | PresentedBufferPositionMatch::HiddenOnRow { next: point } => Some(point),
            PresentedBufferPositionMatch::NotVisible => None,
        };
        point.map(|point| self.materialize_point(point)).transpose()
    }

    /// Resolve coordinates in GNU's current snapshot convention: X is
    /// text-body-local while Y is window-local.
    pub fn point_at_window_coords(
        &self,
        body_x: i64,
        window_y: i64,
    ) -> Result<Option<SnapshotPointGeometry>, GeometryError> {
        let body_y = window_y.saturating_sub(self.text_body_origin_in_window().y().get() as i64);
        let mut points: Vec<_> = self
            .window_geometry
            .positions
            .as_slice()
            .iter()
            .filter(|point| {
                body_y >= point.body_y
                    && body_y < point.body_y.saturating_add(point.row_height.max(1))
            })
            .collect();
        points.sort_by_key(|point| (point.x, point.col, point.buffer_pos));
        let point = points
            .iter()
            .copied()
            .find(|point| body_x < point.x.saturating_add(point.width.max(1)))
            .or_else(|| points.last().copied());
        point.map(|point| self.materialize_point(point)).transpose()
    }

    fn materialize_point(
        &self,
        point: &PresentationPosition,
    ) -> Result<SnapshotPointGeometry, GeometryError> {
        let body_point = WindowPoint {
            window: self.window,
            point: PixelPoint::new(
                LogicalPx::from_i64(point.x).get(),
                LogicalPx::from_i64(point.body_y).get(),
            )?,
        };
        let body_origin = self.text_body_origin_in_frame()?;
        let frame_point = FramePoint {
            frame: self.frame,
            point: PixelPoint::new(
                body_origin.x().get() + body_point.x().get(),
                body_origin.y().get() + body_point.y().get(),
            )?,
        };
        Ok(SnapshotPointGeometry {
            buffer_pos: point.buffer_pos,
            body_point,
            frame_point,
            width: LogicalPx::from_i64(point.width),
            height: LogicalPx::from_i64(point.height),
            row: point.body_row,
            column: point.col,
        })
    }
}

#[cfg(test)]
#[path = "tests/geometry_positions_test.rs"]
mod geometry_positions_test;

/// Resolve the closed set of semantic geometry queries against an
/// evaluator-owned presentation publication (stage-4 W2/E5 FM-6b2).
pub trait DisplayGeometryExt {
    fn resolve<Q: GeometryQuery>(&self, query: Q) -> Result<Q::Output<'_>, GeometryQueryError>;
}

impl DisplayGeometryExt for PresentationGeometry {
    /// Resolve one of the closed set of semantic geometry queries against this
    /// immutable publication.
    fn resolve<Q: GeometryQuery>(&self, query: Q) -> Result<Q::Output<'_>, GeometryQueryError> {
        if query.presentation() != self.presentation {
            return Err(GeometryQueryError::StalePresentation {
                requested: query.presentation(),
                available: self.presentation,
            });
        }
        query.resolve_presented(self)
    }
}
