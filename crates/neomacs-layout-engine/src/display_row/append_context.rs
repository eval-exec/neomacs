use crate::composition::{
    base_width_cols, continues_cluster, continues_complex_run, last_text_cluster_tail_in_glyphs,
};
use crate::display_row::DisplayRowSourceRenderRequest;
use crate::display_row::builder::{
    DisplayRowAppendStartPolicy, DisplayRowPosition, DisplayTabPolicy,
};
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::{
    DisplayRowGeometry, DisplayRowGeometryState, DisplayRowMaxX, DisplayRowTextAreaOrigin,
};
use crate::display_row::metrics::{DisplayRowFallbackMetrics, DisplayRowMeasuredFaceMetrics};
use crate::display_row::render_state::DisplayRowRenderBounds;
use crate::display_row::text_output::TextRowOutput;
use crate::font::metrics::FontMetricsService;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::Glyph;
use neomacs_display_protocol::glyph_matrix::GlyphArea;
use neomacs_display_protocol::types::FaceId;
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowAppendPlacement {
    row: usize,
    y: f32,
    glyph_y: f32,
}

impl DisplayRowAppendPlacement {
    fn new(row: usize, y: f32, glyph_y: f32) -> Self {
        Self { row, y, glyph_y }
    }

    pub(crate) fn from_geometry_state(
        geometry: &DisplayRowGeometryState,
        glyph_y_offset: f32,
    ) -> Self {
        Self::new(
            geometry.row(),
            geometry.y(),
            geometry.glyph_y(glyph_y_offset),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowAppendArea {
    content_x: f32,
    width: f32,
    text_width: f32,
    line_number_width: f32,
}

impl DisplayRowAppendArea {
    pub(crate) fn new(content_x: f32, width: f32, text_width: f32, line_number_width: f32) -> Self {
        Self {
            content_x,
            width,
            text_width,
            line_number_width,
        }
    }

    pub(crate) fn content_x(self) -> f32 {
        self.content_x
    }

    pub(crate) fn width(self) -> f32 {
        self.width
    }

    pub(crate) fn text_width(self) -> f32 {
        self.text_width
    }

    pub(crate) fn line_number_width(self) -> f32 {
        self.line_number_width
    }

    pub(crate) fn right_edge(self) -> f32 {
        self.content_x() + self.width()
    }

    fn full_text_width(self) -> Self {
        Self {
            width: (self.text_width() - self.line_number_width()).max(0.0),
            ..self
        }
    }
}

/// Capacity of one structural margin glyph area.  Both logical columns and
/// the authoritative pixel extent are kept because font advances are concrete
/// while window geometry is allocated in canonical frame columns.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DisplayMarginAreaCapacity {
    columns: usize,
    width_px: f32,
}

impl DisplayMarginAreaCapacity {
    pub(crate) fn new(columns: usize, width_px: f32) -> Self {
        Self {
            columns,
            width_px: width_px.max(0.0),
        }
    }

    pub(crate) fn columns(self) -> usize {
        self.columns
    }

    pub(crate) fn width_px(self) -> f32 {
        self.width_px
    }

    pub(crate) fn is_empty(self) -> bool {
        self.columns == 0 || self.width_px <= 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct DisplayRowMarginAreas {
    left: DisplayMarginAreaCapacity,
    right: DisplayMarginAreaCapacity,
}

impl DisplayRowMarginAreas {
    fn new(left: DisplayMarginAreaCapacity, right: DisplayMarginAreaCapacity) -> Self {
        Self { left, right }
    }

    fn capacity(self, area: GlyphArea) -> Option<DisplayMarginAreaCapacity> {
        match area {
            GlyphArea::LeftMargin => Some(self.left),
            GlyphArea::RightMargin => Some(self.right),
            GlyphArea::Text => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayRowAppendSurface {
    area: DisplayRowAppendArea,
    margin_areas: DisplayRowMarginAreas,
    tab_policy: DisplayTabPolicy,
    image_scale_environment: ImageScaleEnvironment,
    right_edge_marker_column: RightEdgeMarkerColumn,
}

/// Whether this surface kept the row's last column free for a truncation `$`
/// or continuation `\`, instead of letting body text reach the window edge.
///
/// GNU never reserves: `display_line` fills every column and then OVERWRITES
/// the last glyph with the special one (`produce_special_glyphs` at
/// src/xdisp.c:26611-26632), so the character under the marker is one the row
/// really produced.  This port reserves on a terminal frame with no right
/// fringe, so the character under the marker is one the row STOPPED before --
/// and it is the same character, which is why the reservation is exactly the
/// condition under which the marker's column needs a slot of its own.  Where
/// nothing is reserved (window-system frames, where GNU marks truncation in the
/// fringe instead), the last column carries a real glyph and answers for itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RightEdgeMarkerColumn {
    #[default]
    NotReserved,
    Reserved,
}

impl RightEdgeMarkerColumn {
    pub(crate) fn is_reserved(self) -> bool {
        matches!(self, Self::Reserved)
    }
}

impl DisplayRowAppendSurface {
    pub(crate) fn new(area: DisplayRowAppendArea, tab_policy: DisplayTabPolicy) -> Self {
        Self {
            area,
            margin_areas: DisplayRowMarginAreas::default(),
            tab_policy,
            image_scale_environment: ImageScaleEnvironment::default(),
            right_edge_marker_column: RightEdgeMarkerColumn::NotReserved,
        }
    }

    pub(crate) fn with_right_edge_marker_column(
        mut self,
        right_edge_marker_column: RightEdgeMarkerColumn,
    ) -> Self {
        self.right_edge_marker_column = right_edge_marker_column;
        self
    }

    pub(crate) fn right_edge_marker_column(&self) -> RightEdgeMarkerColumn {
        self.right_edge_marker_column
    }

    pub(crate) fn with_margin_areas(
        mut self,
        left_columns: usize,
        left_width_px: f32,
        right_columns: usize,
        right_width_px: f32,
    ) -> Self {
        self.margin_areas = DisplayRowMarginAreas::new(
            DisplayMarginAreaCapacity::new(left_columns, left_width_px),
            DisplayMarginAreaCapacity::new(right_columns, right_width_px),
        );
        self
    }

    pub(crate) fn with_image_scale_environment(
        mut self,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        self.image_scale_environment = image_scale_environment;
        self
    }

    pub(crate) fn content_x(&self) -> f32 {
        self.area.content_x()
    }

    /// Left edge of the complete TEXT_AREA, including a structural display
    /// line-number prefix.  `content_x` is the ordinary buffer body's start.
    pub(crate) fn text_area_left(&self) -> f32 {
        self.area.content_x() - self.area.line_number_width()
    }

    pub(crate) fn right_edge(&self) -> f32 {
        self.area.right_edge()
    }

    /// The surface's tab policy (buffer `tab-width` / `tab-stop-list`), the
    /// one every append frame built from this surface expands tabs with.
    pub(crate) fn tab_policy(&self) -> &DisplayTabPolicy {
        &self.tab_policy
    }

    pub(crate) fn full_text_right_edge(&self) -> f32 {
        self.area.full_text_width().right_edge()
    }

    #[cfg(test)]
    pub(crate) fn full_text_width_surface(&self) -> Self {
        Self {
            area: self.area.full_text_width(),
            margin_areas: self.margin_areas,
            tab_policy: self.tab_policy.clone(),
            image_scale_environment: self.image_scale_environment,
            // A full-text-width surface deliberately spans the reserved column
            // too, so nothing on it is a marker column any more.
            right_edge_marker_column: RightEdgeMarkerColumn::NotReserved,
        }
    }

    pub(crate) fn frame(
        &self,
        placement: DisplayRowAppendPlacement,
        metrics: DisplayRowAppendMetrics,
    ) -> DisplayRowAppendFrame {
        DisplayRowAppendFrame::from_parts(
            placement,
            self.area,
            self.margin_areas,
            metrics,
            self.tab_policy.clone(),
            self.image_scale_environment,
        )
    }

    pub(crate) fn frame_from_geometry_state(
        &self,
        geometry: &DisplayRowGeometryState,
        glyph_y_offset: f32,
        metrics: DisplayRowAppendMetrics,
    ) -> DisplayRowAppendFrame {
        self.frame(
            DisplayRowAppendPlacement::from_geometry_state(geometry, glyph_y_offset),
            metrics,
        )
    }

    fn text_row_frame_from_geometry_state(
        &self,
        geometry: &DisplayRowGeometryState,
        glyph_y_offset: f32,
        height: f32,
        ascent: f32,
        char_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> DisplayRowAppendFrame {
        self.frame_from_geometry_state(
            geometry,
            glyph_y_offset,
            DisplayRowAppendMetrics::text_row(height, ascent, char_width, fallback_metrics),
        )
    }

    fn frame_for_active_face_from_geometry_state(
        &self,
        geometry: &DisplayRowGeometryState,
        glyph_y_offset: f32,
        active_face: &DisplayRowActiveFaceState,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> DisplayRowAppendFrame {
        self.frame_from_geometry_state(
            geometry,
            glyph_y_offset,
            DisplayRowAppendMetrics::from_active_face_state(active_face, fallback_metrics),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayRowTextNaturalAdvanceKind {
    Tab,
    ClusterContinuation,
    ComplexRunMember,
    /// A source character with GNU `CHARACTER_WIDTH` zero that is not a
    /// member of a selected composition.  It may carry graphical ink, but it
    /// advances neither pixels nor terminal cells and must not be merged into
    /// an adjacent glyph.
    StandaloneZeroWidth,
    FaceColumns {
        columns: usize,
    },
}

impl DisplayRowTextNaturalAdvanceKind {
    fn for_face_columns(ch: char) -> Self {
        match usize::from(base_width_cols(ch)) {
            0 => Self::StandaloneZeroWidth,
            columns => Self::FaceColumns { columns },
        }
    }

    pub(crate) fn for_tail(ch: char, tail: Option<(char, bool)>) -> Self {
        if ch == '\t' {
            Self::Tab
        } else if continues_cluster(ch, tail) {
            Self::ClusterContinuation
        } else if continues_complex_run(ch, tail) {
            Self::ComplexRunMember
        } else {
            Self::for_face_columns(ch)
        }
    }

    pub(crate) fn for_source_char(ch: char, is_cluster_continuation: bool) -> Self {
        if ch == '\t' {
            Self::Tab
        } else if is_cluster_continuation {
            Self::ClusterContinuation
        } else {
            Self::for_face_columns(ch)
        }
    }

    pub(crate) fn resolve_to_text_row(
        self,
        font_metrics: &mut Option<FontMetricsService>,
        active_face_state: &DisplayRowActiveFaceState,
        frame: &DisplayRowAppendFrame,
        position: DisplayRowPosition,
        ch: char,
    ) -> f32 {
        let request = DisplayRowTextNaturalAdvanceRequest::new(
            self,
            position,
            ch,
            active_face_state.face_id(),
        );
        frame
            .natural_text_advance_policy()
            .resolve_with(request, |ch, _face_id, columns| {
                active_face_state.advance_for_columns(font_metrics, ch, columns)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowTextNaturalAdvanceRequest {
    kind: DisplayRowTextNaturalAdvanceKind,
    position: DisplayRowPosition,
    ch: char,
    face_id: FaceId,
}

impl DisplayRowTextNaturalAdvanceRequest {
    pub(crate) fn new(
        kind: DisplayRowTextNaturalAdvanceKind,
        position: DisplayRowPosition,
        ch: char,
        face_id: FaceId,
    ) -> Self {
        Self {
            kind,
            position,
            ch,
            face_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayRowTextNaturalAdvancePolicy {
    tab_policy: DisplayTabPolicy,
}

impl DisplayRowTextNaturalAdvancePolicy {
    pub(crate) fn new(tab_policy: DisplayTabPolicy) -> Self {
        Self { tab_policy }
    }

    pub(crate) fn resolve_with(
        &self,
        request: DisplayRowTextNaturalAdvanceRequest,
        mut glyph_advance_px: impl FnMut(char, FaceId, usize) -> f32,
    ) -> f32 {
        match request.kind {
            DisplayRowTextNaturalAdvanceKind::Tab => {
                let space_width_px = glyph_advance_px(' ', request.face_id, 1);
                self.tab_policy
                    .advance_from(request.position, space_width_px)
                    .pixel_width
            }
            DisplayRowTextNaturalAdvanceKind::ClusterContinuation => 0.0,
            DisplayRowTextNaturalAdvanceKind::ComplexRunMember => {
                glyph_advance_px(request.ch, request.face_id, 1)
            }
            DisplayRowTextNaturalAdvanceKind::StandaloneZeroWidth => 0.0,
            DisplayRowTextNaturalAdvanceKind::FaceColumns { columns } => {
                glyph_advance_px(request.ch, request.face_id, columns)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplayRowTextCharState {
    ch: char,
    kind: DisplayRowTextNaturalAdvanceKind,
}

impl DisplayRowTextCharState {
    pub(crate) fn for_tail(ch: char, tail: Option<(char, bool)>) -> Self {
        Self {
            ch,
            kind: DisplayRowTextNaturalAdvanceKind::for_tail(ch, tail),
        }
    }

    pub(crate) fn for_glyphs(ch: char, glyphs: &[Glyph]) -> Self {
        Self::for_tail(ch, last_text_cluster_tail_in_glyphs(glyphs))
    }

    pub(crate) fn independent(ch: char) -> Self {
        Self {
            ch,
            kind: DisplayRowTextNaturalAdvanceKind::for_source_char(ch, false),
        }
    }

    pub(crate) fn ch(self) -> char {
        self.ch
    }

    pub(crate) fn kind(self) -> DisplayRowTextNaturalAdvanceKind {
        self.kind
    }

    pub(crate) fn natural_advance_request(
        self,
        position: DisplayRowPosition,
        face_id: FaceId,
    ) -> DisplayRowTextNaturalAdvanceRequest {
        DisplayRowTextNaturalAdvanceRequest::new(self.kind, position, self.ch, face_id)
    }
}

#[derive(Clone, Copy)]
struct DisplayRowTextAppendContext<'a> {
    append_surface: &'a DisplayRowAppendSurface,
    geometry: &'a DisplayRowGeometryState,
    glyph_y_offset: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
}

impl<'a> DisplayRowTextAppendContext<'a> {
    fn new(
        append_surface: &'a DisplayRowAppendSurface,
        geometry: &'a DisplayRowGeometryState,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            append_surface,
            geometry,
            glyph_y_offset,
            fallback_metrics,
        }
    }

    fn text_row_frame(self, height: f32, ascent: f32, char_width: f32) -> DisplayRowAppendFrame {
        self.append_surface.text_row_frame_from_geometry_state(
            self.geometry,
            self.glyph_y_offset,
            height,
            ascent,
            char_width,
            self.fallback_metrics,
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DisplayRowActiveFaceAppendContext<'row, 'face> {
    text_context: DisplayRowTextAppendContext<'row>,
    active_face: &'face DisplayRowActiveFaceState,
}

impl<'row, 'face> DisplayRowActiveFaceAppendContext<'row, 'face> {
    pub(crate) fn new(
        append_surface: &'row DisplayRowAppendSurface,
        geometry: &'row DisplayRowGeometryState,
        active_face: &'face DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            text_context: DisplayRowTextAppendContext::new(
                append_surface,
                geometry,
                glyph_y_offset,
                fallback_metrics,
            ),
            active_face,
        }
    }

    pub(crate) fn active_face_frame(self) -> DisplayRowAppendFrame {
        self.text_context
            .append_surface
            .frame_for_active_face_from_geometry_state(
                self.text_context.geometry,
                self.text_context.glyph_y_offset,
                self.active_face,
                self.text_context.fallback_metrics,
            )
    }

    pub(crate) fn active_face(self) -> &'face DisplayRowActiveFaceState {
        self.active_face
    }

    #[cfg(test)]
    pub(crate) fn full_text_width_active_face_frame(self) -> DisplayRowAppendFrame {
        self.text_context
            .append_surface
            .full_text_width_surface()
            .frame_for_active_face_from_geometry_state(
                self.text_context.geometry,
                self.text_context.glyph_y_offset,
                self.active_face,
                self.text_context.fallback_metrics,
            )
    }

    pub(crate) fn text_row_frame(
        self,
        height: f32,
        ascent: f32,
        char_width: f32,
    ) -> DisplayRowAppendFrame {
        self.text_context.text_row_frame(height, ascent, char_width)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowAppendMetrics {
    height: f32,
    ascent: f32,
    char_width: f32,
    space_width: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
}

impl DisplayRowAppendMetrics {
    pub(crate) fn new(
        height: f32,
        ascent: f32,
        char_width: f32,
        space_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            height,
            ascent,
            char_width,
            space_width,
            fallback_metrics,
        }
    }

    pub(crate) fn height(self) -> f32 {
        self.height
    }

    pub(crate) fn ascent(self) -> f32 {
        self.ascent
    }

    pub(crate) fn char_width(self) -> f32 {
        self.char_width
    }

    pub(crate) fn space_width(self) -> f32 {
        self.space_width
    }

    pub(crate) fn fallback_metrics(self) -> DisplayRowFallbackMetrics {
        self.fallback_metrics
    }

    pub(crate) fn text_row(
        height: f32,
        ascent: f32,
        char_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self::new(height, ascent, char_width, char_width, fallback_metrics)
    }

    pub(crate) fn from_active_face_state(
        active_face: &DisplayRowActiveFaceState,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self::from_measured_face_metrics(active_face.metrics(), fallback_metrics)
    }

    pub(crate) fn display_box_from_active_face_state(
        active_face: &DisplayRowActiveFaceState,
        height: f32,
        ascent: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        let metrics = active_face.metrics();
        Self::new(
            height,
            ascent,
            metrics.char_width(),
            metrics.space_width(),
            fallback_metrics,
        )
    }

    pub(crate) fn from_measured_face_metrics(
        metrics: DisplayRowMeasuredFaceMetrics,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self::new(
            metrics.row_height(),
            metrics.ascent(),
            metrics.char_width(),
            metrics.space_width(),
            fallback_metrics,
        )
    }

    pub(crate) fn text_row_frame(
        self,
        append_surface: &DisplayRowAppendSurface,
        geometry: &DisplayRowGeometryState,
        glyph_y_offset: f32,
    ) -> DisplayRowAppendFrame {
        DisplayRowTextAppendContext::new(
            append_surface,
            geometry,
            glyph_y_offset,
            self.fallback_metrics(),
        )
        .text_row_frame(self.height(), self.ascent(), self.char_width())
    }
}

#[derive(Clone)]
pub(crate) struct DisplayRowAppendFrame {
    row: usize,
    glyph_y: f32,
    geometry: DisplayRowGeometry,
    default_row_height: f32,
    content_x: f32,
    text_width: f32,
    line_number_width: f32,
    margin_areas: DisplayRowMarginAreas,
    face_space_width: f32,
    image_scale_environment: ImageScaleEnvironment,
}

pub(crate) struct DisplayRowAppendSourceRenderRequest<'face> {
    row_request: DisplayRowSourceRenderRequest<'face>,
    output: TextRowOutput,
}

impl<'face> DisplayRowAppendSourceRenderRequest<'face> {
    fn new(row_request: DisplayRowSourceRenderRequest<'face>, output: TextRowOutput) -> Self {
        Self {
            row_request,
            output,
        }
    }

    #[cfg(test)]
    pub(crate) fn row_request(self) -> DisplayRowSourceRenderRequest<'face> {
        self.row_request
    }

    #[cfg(test)]
    pub(crate) fn output(&self) -> TextRowOutput {
        self.output
    }

    pub(crate) fn render_with_row_request<R>(
        self,
        render: impl FnOnce(DisplayRowSourceRenderRequest<'face>) -> R,
    ) -> (R, TextRowOutput) {
        (render(self.row_request), self.output)
    }

    pub(crate) fn with_append_start_policy(
        mut self,
        append_start_policy: DisplayRowAppendStartPolicy,
    ) -> Self {
        self.row_request = self
            .row_request
            .with_append_start_policy(append_start_policy);
        self
    }
}

impl DisplayRowAppendFrame {
    pub(crate) fn row(&self) -> usize {
        self.row
    }

    pub(crate) fn glyph_y(&self) -> f32 {
        self.glyph_y
    }

    pub(crate) fn geometry(&self) -> &DisplayRowGeometry {
        &self.geometry
    }

    pub(crate) fn default_row_height(&self) -> f32 {
        self.default_row_height
    }

    pub(crate) fn content_x(&self) -> f32 {
        self.content_x
    }

    pub(crate) fn text_width(&self) -> f32 {
        self.text_width
    }

    pub(crate) fn line_number_width(&self) -> f32 {
        self.line_number_width
    }

    pub(crate) fn face_space_width(&self) -> f32 {
        self.face_space_width
    }

    pub(crate) fn width_for_columns(&self, columns: usize) -> f32 {
        columns as f32 * self.geometry().char_width().max(1.0)
    }

    pub(crate) fn natural_text_advance_policy(&self) -> DisplayRowTextNaturalAdvancePolicy {
        DisplayRowTextNaturalAdvancePolicy::new(self.geometry().tab_policy().clone())
    }

    fn right_edge(&self) -> f32 {
        self.content_x() + self.geometry().width()
    }

    /// The window's text area starts where the content does, less the
    /// line-number prefix that `content_x` already skips: GNU's
    /// `it->current_x` counts that prefix (it is produced as glyphs), so the
    /// origin GNU measures from is the text area's own left edge.
    fn text_area_origin(&self) -> DisplayRowTextAreaOrigin {
        DisplayRowTextAreaOrigin::at_frame_x(self.content_x() - self.line_number_width())
    }

    fn text_right_edge_excluding_line_number(&self) -> f32 {
        self.content_x() + (self.text_width() - self.line_number_width()).max(0.0)
    }

    fn from_parts(
        placement: DisplayRowAppendPlacement,
        area: DisplayRowAppendArea,
        margin_areas: DisplayRowMarginAreas,
        metrics: DisplayRowAppendMetrics,
        tab_policy: DisplayTabPolicy,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        Self {
            row: placement.row,
            glyph_y: placement.glyph_y,
            geometry: DisplayRowGeometry::new(
                placement.y,
                area.width(),
                metrics.height(),
                metrics.char_width(),
                metrics.ascent(),
                tab_policy,
            ),
            default_row_height: metrics.fallback_metrics().row_height(),
            content_x: area.content_x(),
            text_width: area.text_width(),
            line_number_width: area.line_number_width(),
            margin_areas,
            face_space_width: metrics.space_width(),
            image_scale_environment,
        }
    }

    pub(crate) fn margin_capacity(&self, area: GlyphArea) -> Option<DisplayMarginAreaCapacity> {
        self.margin_areas.capacity(area)
    }

    pub(crate) fn source_append_render_request<'face>(
        &self,
        position: DisplayRowPosition,
        face_id: FaceId,
        base_face: &'face ResolvedFace,
        kind: DisplayRowAppendKind,
    ) -> DisplayRowAppendSourceRenderRequest<'face> {
        DisplayRowAppendSourceRenderRequest::new(
            self.source_render_request(position, face_id, base_face, kind),
            self.text_row_output(kind),
        )
    }

    pub(crate) fn source_append_measure_request<'face>(
        &self,
        position: DisplayRowPosition,
        face_id: FaceId,
        base_face: &'face ResolvedFace,
        kind: DisplayRowAppendKind,
    ) -> DisplayRowSourceRenderRequest<'face> {
        self.source_render_request(position, face_id, base_face, kind)
            .with_render_bounds(DisplayRowRenderBounds::unbounded_from(position))
    }

    fn source_render_request<'face>(
        &self,
        position: DisplayRowPosition,
        face_id: FaceId,
        base_face: &'face ResolvedFace,
        kind: DisplayRowAppendKind,
    ) -> DisplayRowSourceRenderRequest<'face> {
        DisplayRowSourceRenderRequest::from_display_row_geometry_for_base_face_id(
            self.source_render_geometry(kind),
            face_id,
            base_face,
            GlyphRowRole::Text,
        )
        .with_image_scale_environment(self.image_scale_environment)
        .with_render_bounds(DisplayRowRenderBounds::in_window_text_area(
            position,
            kind.max_x(self),
            self.text_area_origin(),
        ))
        .with_line_end_right_edge_x(self.text_right_edge_excluding_line_number())
    }

    fn source_render_geometry(&self, kind: DisplayRowAppendKind) -> DisplayRowGeometry {
        self.geometry()
            .clone()
            .with_char_width(kind.char_width(self))
    }

    fn text_row_output(&self, kind: DisplayRowAppendKind) -> TextRowOutput {
        TextRowOutput::new(
            self.row(),
            self.geometry().y(),
            self.glyph_y(),
            kind.output_height(self),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayRowAppendKind {
    SourceText,
    Tab,
    ControlChar,
    SourceMappedText,
    Glyphless,
    DisplayReplacement,
    DisplayReplacementString,
}

impl DisplayRowAppendKind {
    pub(crate) fn char_width(self, frame: &DisplayRowAppendFrame) -> f32 {
        match self {
            Self::Tab | Self::DisplayReplacementString => frame.face_space_width(),
            Self::SourceText
            | Self::ControlChar
            | Self::SourceMappedText
            | Self::Glyphless
            | Self::DisplayReplacement => frame.geometry().char_width(),
        }
    }

    pub(crate) fn max_x(self, frame: &DisplayRowAppendFrame) -> DisplayRowMaxX {
        match self {
            Self::Tab => DisplayRowMaxX::Unbounded,
            Self::ControlChar => {
                DisplayRowMaxX::Bounded(frame.text_right_edge_excluding_line_number())
            }
            Self::SourceText
            | Self::SourceMappedText
            | Self::Glyphless
            | Self::DisplayReplacement
            | Self::DisplayReplacementString => DisplayRowMaxX::Bounded(frame.right_edge()),
        }
    }

    pub(crate) fn output_height(self, frame: &DisplayRowAppendFrame) -> f32 {
        match self {
            Self::SourceText
            | Self::Glyphless
            | Self::DisplayReplacement
            | Self::DisplayReplacementString => frame.geometry().height(),
            Self::Tab | Self::ControlChar | Self::SourceMappedText => frame.default_row_height(),
        }
    }
}
