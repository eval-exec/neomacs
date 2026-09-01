//! Coordinate-safe views over redisplay's existing window snapshots.
//!
//! A `PresentationGeometry` owns one immutable frame publication and exposes typed
//! pixel and cell views so consumers cannot silently combine body-local,
//! window-local, frame-local, and cell-grid values.
//!
//! Layer boundary: this module answers "where was this window PRESENTED"
//! from renderer publications (post-redisplay). The Lisp-facing edge
//! builtins in emacs_core/window_cmds deliberately do NOT route through
//! here -- window-edges and friends answer from the live Window tree
//! (Window::bounds), matching GNU, which computes window.c edges from
//! struct window fields without consulting the glyph matrix. The two
//! systems are different layers, not duplication.

use super::{FrameId, LispCharPos1, WindowDisplaySnapshot, WindowId};
use neomacs_display_protocol::frame_glyphs::PresentedWindowRegions;
use neomacs_display_protocol::types::Rect as TransportRect;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::num::NonZeroU64;

/// Evaluator-owned identity of one immutable displayed geometry publication.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresentationId(NonZeroU64);

impl PresentationId {
    pub const fn new(value: u64) -> Self {
        match Self::try_new(value) {
            Some(id) => id,
            None => panic!("presentation identity must be nonzero"),
        }
    }

    pub const fn try_new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationPrepareError {
    ReusedPresentation(PresentationId),
    InvalidGeometry(GeometryError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationActivateError {
    UnknownPresentation(PresentationId),
}

/// One immutable, presentation-owned publication of all evaluator window geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct PresentationGeometry {
    presentation: PresentationId,
    frame: PresentationFrame,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationFrame {
    id: FrameId,
    placement: neomacs_display_protocol::PresentedFramePlacement,
    windows: HashMap<WindowId, PresentationWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationWindow {
    id: WindowId,
    cell_origin: CellOrigin,
    outer: PixelRect<FrameLogicalSpace>,
    regions: Option<WindowRegions>,
    positions: Vec<PresentationPosition>,
    cursor: Option<PresentationCursor>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowRegions {
    outer: PixelRect<FrameLogicalSpace>,
    text_body: PixelRect<FrameLogicalSpace>,
    left_margin_columns: i64,
    right_margin_columns: i64,
    left_margin: Option<PixelRect<FrameLogicalSpace>>,
    right_margin: Option<PixelRect<FrameLogicalSpace>>,
    left_fringe: Option<PixelRect<FrameLogicalSpace>>,
    right_fringe: Option<PixelRect<FrameLogicalSpace>>,
    left_scroll_bar: Option<PixelRect<FrameLogicalSpace>>,
    right_scroll_bar: Option<PixelRect<FrameLogicalSpace>>,
    horizontal_scroll_bar: Option<PixelRect<FrameLogicalSpace>>,
    tab_line: Option<PixelRect<FrameLogicalSpace>>,
    header_line: Option<PixelRect<FrameLogicalSpace>>,
    mode_line: Option<PixelRect<FrameLogicalSpace>>,
    right_divider: Option<PixelRect<FrameLogicalSpace>>,
    bottom_divider: Option<PixelRect<FrameLogicalSpace>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PresentationPosition {
    buffer_pos: LispCharPos1,
    x: i64,
    body_y: i64,
    width: i64,
    height: i64,
    body_row: i64,
    col: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PresentationCursor {
    x: i64,
    body_y: i64,
    width: i64,
    height: i64,
}

/// Result of resolving a buffer position against one immutable presentation.
///
/// GNU redisplay can advance past text hidden by invisibility or a replacing
/// `display` property.  Keep that compatibility case distinct from an exact
/// glyph match so the row-boundary rule cannot be lost in a generic fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresentedBufferPositionMatch<'a> {
    Exact(&'a PresentationPosition),
    HiddenOnRow { next: &'a PresentationPosition },
    NotVisible,
}

impl PresentationGeometry {
    pub(crate) fn new(
        frame: FrameId,
        presentation: PresentationId,
        snapshots: impl IntoIterator<Item = WindowDisplaySnapshot>,
    ) -> Result<Self, GeometryError> {
        let mut windows = HashMap::new();
        for snapshot in snapshots {
            let window = PresentationWindow::from_snapshot(snapshot)?;
            let id = window.id;
            if windows.insert(id, window).is_some() {
                return Err(GeometryError::DuplicateWindow(id));
            }
        }
        let placement = neomacs_display_protocol::PresentedFramePlacement::new(
            neomacs_display_protocol::DisplayFrameId::new(frame.0),
            neomacs_display_protocol::PresentationId::new(presentation.get()),
            None,
            neomacs_display_protocol::ParentFrameRect::new(0.0, 0.0, 0.0, 0.0)
                .expect("zero root extent is valid for compatibility construction"),
            0,
        );
        Ok(Self {
            presentation,
            frame: PresentationFrame {
                id: frame,
                placement,
                windows,
            },
        })
    }

    #[allow(clippy::too_many_arguments)] // constructor receives the complete immutable frame placement
    pub(crate) fn new_with_frame_placement(
        frame: FrameId,
        presentation: PresentationId,
        parent: Option<FrameId>,
        left: i64,
        top: i64,
        width: u32,
        height: u32,
        z_order: i32,
        snapshots: impl IntoIterator<Item = WindowDisplaySnapshot>,
    ) -> Result<Self, GeometryError> {
        let mut geometry = Self::new(frame, presentation, snapshots)?;
        let (left, top) = if parent.is_some() {
            (left as f32, top as f32)
        } else {
            (0.0, 0.0)
        };
        geometry.frame.placement = neomacs_display_protocol::PresentedFramePlacement::new(
            neomacs_display_protocol::DisplayFrameId::new(frame.0),
            neomacs_display_protocol::PresentationId::new(presentation.get()),
            parent.map(|parent| neomacs_display_protocol::DisplayFrameId::new(parent.0)),
            neomacs_display_protocol::ParentFrameRect::new(left, top, width as f32, height as f32)
                .map_err(|_| GeometryError::InvalidExtent)?,
            z_order,
        );
        Ok(geometry)
    }

    pub const fn frame_placement(&self) -> neomacs_display_protocol::PresentedFramePlacement {
        self.frame.placement
    }

    pub const fn presentation(&self) -> PresentationId {
        self.presentation
    }

    /// Logical source extent captured by this immutable presentation.
    ///
    /// Compatibility publications built by [`Self::new`] predate explicit
    /// frame placement and carry a zero extent.  `None` preserves that unknown
    /// state instead of conflating it with empty content.
    pub fn content_extent(
        &self,
    ) -> Option<neomacs_display_protocol::GeometrySize<neomacs_display_protocol::LogicalPixels>>
    {
        let outer = self.frame.placement.outer_in_parent();
        (outer.width() > 0.0 && outer.height() > 0.0).then_some(*outer.size())
    }

    fn window(&self, window: WindowId) -> Option<&PresentationWindow> {
        self.frame.windows.get(&window)
    }

    /// Resolve one of the closed set of semantic geometry queries against this
    /// immutable publication.
    pub fn resolve<Q: GeometryQuery>(&self, query: Q) -> Result<Q::Output<'_>, GeometryQueryError> {
        if query.presentation() != self.presentation {
            return Err(GeometryQueryError::StalePresentation {
                requested: query.presentation(),
                available: self.presentation,
            });
        }
        query.resolve_presented(self)
    }
}

impl PresentationWindow {
    fn from_snapshot(snapshot: WindowDisplaySnapshot) -> Result<Self, GeometryError> {
        let outer = PixelRect::from_transport(&snapshot.regions.outer)?;
        let regions = snapshot
            .regions_materialized
            .then(|| WindowRegions::from_transport(&snapshot.regions))
            .transpose()?;
        let mut body_rows = HashMap::new();
        for row in &snapshot.body_rows {
            if body_rows.insert(row.output_row, *row).is_some() {
                return Err(GeometryError::DuplicateBodyRow {
                    window: snapshot.window_id,
                    output_row: row.output_row,
                });
            }
        }
        let positions = snapshot
            .points
            .iter()
            .map(|point| {
                let body_row = body_rows
                    .get(&point.row)
                    .ok_or(GeometryError::MissingBodyRow {
                        window: snapshot.window_id,
                        output_row: point.row,
                    })?;
                Ok(PresentationPosition {
                    buffer_pos: point.buffer_pos,
                    x: point.x,
                    body_y: body_row.body_y,
                    width: point.width,
                    height: point.height,
                    body_row: body_row.body_row,
                    col: point.col,
                })
            })
            .collect::<Result<Vec<_>, GeometryError>>()?;
        let cursor = snapshot.logical_cursor_pos().and_then(|cursor| {
            let point = positions
                .iter()
                .find(|point| point.body_row == cursor.row && point.col == cursor.col);
            let physical = snapshot.phys_cursor.as_ref();
            let width = physical
                .map(|cursor| cursor.width)
                .or_else(|| point.map(|p| p.width))?;
            let height = physical
                .map(|cursor| cursor.height)
                .or_else(|| point.map(|p| p.height))?;
            Some(PresentationCursor {
                x: cursor.x,
                body_y: cursor.y,
                width,
                height,
            })
        });
        Ok(Self {
            id: snapshot.window_id,
            cell_origin: snapshot.cell_origin,
            outer,
            regions,
            positions,
            cursor,
        })
    }
}

impl WindowRegions {
    pub(crate) fn from_transport(regions: &PresentedWindowRegions) -> Result<Self, GeometryError> {
        let optional = |rect: Option<TransportRect>| {
            rect.map(|rect| PixelRect::from_transport(&rect))
                .transpose()
        };
        Ok(Self {
            outer: PixelRect::from_transport(&regions.outer)?,
            text_body: PixelRect::from_transport(&regions.text_body)?,
            left_margin_columns: regions.left_margin_columns,
            right_margin_columns: regions.right_margin_columns,
            left_margin: optional(regions.left_margin)?,
            right_margin: optional(regions.right_margin)?,
            left_fringe: optional(regions.left_fringe)?,
            right_fringe: optional(regions.right_fringe)?,
            left_scroll_bar: optional(regions.left_scroll_bar)?,
            right_scroll_bar: optional(regions.right_scroll_bar)?,
            horizontal_scroll_bar: optional(regions.horizontal_scroll_bar)?,
            tab_line: optional(regions.tab_line)?,
            header_line: optional(regions.header_line)?,
            mode_line: optional(regions.mode_line)?,
            right_divider: optional(regions.right_divider)?,
            bottom_divider: optional(regions.bottom_divider)?,
        })
    }

    pub const fn outer(self) -> PixelRect<FrameLogicalSpace> {
        self.outer
    }
    pub const fn text_body(self) -> PixelRect<FrameLogicalSpace> {
        self.text_body
    }
    pub const fn left_margin_columns(self) -> i64 {
        self.left_margin_columns
    }
    pub const fn right_margin_columns(self) -> i64 {
        self.right_margin_columns
    }
    pub fn matches_transport(self, regions: &PresentedWindowRegions) -> bool {
        Self::from_transport(regions).is_ok_and(|other| other == self)
    }
    pub const fn left_margin(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.left_margin
    }
    pub const fn right_margin(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.right_margin
    }
    pub const fn left_fringe(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.left_fringe
    }
    pub const fn right_fringe(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.right_fringe
    }
    pub const fn left_scroll_bar(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.left_scroll_bar
    }
    pub const fn right_scroll_bar(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.right_scroll_bar
    }
    pub const fn horizontal_scroll_bar(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.horizontal_scroll_bar
    }
    pub const fn tab_line(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.tab_line
    }
    pub const fn header_line(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.header_line
    }
    pub const fn mode_line(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.mode_line
    }
    pub const fn right_divider(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.right_divider
    }
    pub const fn bottom_divider(self) -> Option<PixelRect<FrameLogicalSpace>> {
        self.bottom_divider
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

/// A logical-pixel coordinate or extent.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct LogicalPx(f32);

impl LogicalPx {
    pub const fn get(self) -> f32 {
        self.0
    }

    fn from_i64(value: i64) -> Self {
        Self(value as f32)
    }
}

/// A stored character-column coordinate, distinct from pixels.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Column(i64);

impl Column {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

/// A stored character-line coordinate, distinct from pixels.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Line(i64);

impl Line {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameLogicalSpace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowLocalSpace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowBodySpace;

/// A logical-pixel point whose coordinate space is part of its type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PixelPoint<Space> {
    x: LogicalPx,
    y: LogicalPx,
    space: PhantomData<Space>,
}

impl<Space> PixelPoint<Space> {
    fn new(x: f32, y: f32) -> Result<Self, GeometryError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(GeometryError::NonFiniteCoordinate);
        }
        Ok(Self {
            x: LogicalPx(x),
            y: LogicalPx(y),
            space: PhantomData,
        })
    }

    pub const fn x(self) -> LogicalPx {
        self.x
    }

    pub const fn y(self) -> LogicalPx {
        self.y
    }
}

/// A finite, nonnegative-extent logical-pixel rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PixelRect<Space> {
    origin: PixelPoint<Space>,
    width: LogicalPx,
    height: LogicalPx,
}

impl<Space> PixelRect<Space> {
    fn from_transport(rect: &TransportRect) -> Result<Self, GeometryError> {
        if rect.x < 0.0
            || rect.y < 0.0
            || !rect.width.is_finite()
            || !rect.height.is_finite()
            || rect.width < 0.0
            || rect.height < 0.0
        {
            return Err(GeometryError::InvalidExtent);
        }
        Ok(Self {
            origin: PixelPoint::new(rect.x, rect.y)?,
            width: LogicalPx(rect.width),
            height: LogicalPx(rect.height),
        })
    }

    pub const fn origin(self) -> PixelPoint<Space> {
        self.origin
    }

    pub const fn width(self) -> LogicalPx {
        self.width
    }

    pub const fn height(self) -> LogicalPx {
        self.height
    }
}

/// Independent stored cell-grid origin for one window.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellOrigin {
    column: Column,
    line: Line,
}

impl CellOrigin {
    pub const fn new(column: i64, line: i64) -> Self {
        Self {
            column: Column::new(column),
            line: Line::new(line),
        }
    }

    pub const fn column(self) -> Column {
        self.column
    }

    pub const fn line(self) -> Line {
        self.line
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryError {
    SnapshotWindowMismatch,
    DuplicateWindow(WindowId),
    MissingBodyRow { window: WindowId, output_row: i64 },
    DuplicateBodyRow { window: WindowId, output_row: i64 },
    NonFiniteCoordinate,
    InvalidExtent,
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
        let idx = self
            .window_geometry
            .positions
            .partition_point(|point| point.buffer_pos < buffer_pos);
        let next = self.window_geometry.positions.get(idx);
        let previous = idx
            .checked_sub(1)
            .and_then(|previous| self.window_geometry.positions.get(previous));
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
            .iter()
            .filter(|point| {
                body_y >= point.body_y && body_y < point.body_y.saturating_add(point.height.max(1))
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
