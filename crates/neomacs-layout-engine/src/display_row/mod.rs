use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{DisplayItemKind, RenderFaceRef};
use crate::display_origin::DisplayOrigin;
use crate::display_pixel_calc::PixelCalcContext;
use crate::display_row::builder::{
    DisplayRowAppendStartPolicy, DisplayRowAppendStatus, DisplayRowItemMeasurement,
    DisplayRowLayout, DisplayRowPosition, DisplayRowProgressWriter, DisplayTabPolicy,
    new_display_row_for_role,
};
use crate::display_row::finalizer::DisplayRowLineEndFinalizer;
use crate::display_row::geometry::DisplayRowGeometryState;
pub(crate) use crate::display_row::geometry::{DisplayRowGeometry, DisplayRowMaxX};
#[cfg(test)]
pub(crate) use crate::display_row::measured_state::{
    DisplayRowBoundsPolicy, DisplayRowOwner, FrameChromeKind, MeasuredDisplayRow, WindowChromeKind,
};
pub(crate) use crate::display_row::metrics::DisplayRowFallbackMetrics;
#[cfg(test)]
pub(crate) use crate::display_row::metrics::DisplayRowMeasuredFaceMetrics;
use crate::display_row::render_item::DisplayRowRenderItem;
use crate::display_row::render_policy::NaturalDisplayRowRenderPolicy;
pub(crate) use crate::display_row::render_policy::{
    DisplayRowRenderClipBehavior, DisplayRowRenderPolicy,
};
#[cfg(test)]
pub(crate) use crate::display_row::render_state::{
    CurrentTextRowRenderOutcome, DisplayRowOutputProgress,
};
pub(crate) use crate::display_row::render_state::{
    DisplayRowRenderBounds, DisplayRowRenderIntoRowResult, DisplayRowRenderStop,
    RenderedDisplayRow, display_row_progress,
};
pub(crate) use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_source::{DisplayItemSource, LispStringSourceCursor, LispStringSourceOrigin};
use crate::display_source_resolver::{
    DisplaySourceFaceBasis, DisplaySourceFaceScope, DisplaySourceResolveParams,
};
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::FaceResolver;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{GlyphArea, GlyphRow, GlyphStringId};
use neomacs_display_protocol::types::FaceId;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::DisplayHost;
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;

#[cfg(test)]
pub(crate) use crate::display_row::face_state::{
    DisplayRowActiveFaceState, DisplayRowGlyphMeasurementFace, DisplayRowMeasurementPolicy,
};
pub(crate) use crate::display_row::face_state::{
    DisplayRowFace, DisplayRowFaceRealizer, DisplayRowGlyphMeasurer, DisplayRowMeasurementMode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DisplayRowLispStringSourceId(u64);

impl DisplayRowLispStringSourceId {
    const ROOT: Self = Self(1);

    fn raw(self) -> u64 {
        self.0
    }
}

pub(crate) const fn root_lisp_string_id() -> GlyphStringId {
    GlyphStringId::new(DisplayRowLispStringSourceId::ROOT.0)
}

fn include_display_row_face_metrics(layout: &mut DisplayRowLayout, face: &DisplayRowFace) {
    face.metrics.include_in_layout(layout);
}

#[derive(Clone)]
pub(crate) struct DisplayRowSourceFragmentFrame<'face> {
    policy: DisplayRowSourceRequestPolicy,
    base_face_id: FaceId,
    base_face: &'face ResolvedFace,
}

impl<'face> DisplayRowSourceFragmentFrame<'face> {
    pub(crate) fn new(
        geometry: DisplayRowGeometry,
        role: GlyphRowRole,
        base_face_id: FaceId,
        base_face: &'face ResolvedFace,
    ) -> Self {
        Self {
            policy: DisplayRowSourceRequestPolicy::from_display_row_geometry(geometry, role),
            base_face_id,
            base_face,
        }
    }

    pub(crate) fn render_request(
        self,
        render_bounds: DisplayRowRenderBounds,
    ) -> DisplayRowSourceRenderRequest<'face> {
        DisplayRowSourceRenderRequest::from_base_face_id_policy_with_render_bounds(
            self.policy,
            self.base_face_id,
            self.base_face,
            render_bounds,
        )
    }

    pub(crate) fn render_request_for_area(
        self,
        render_bounds: DisplayRowRenderBounds,
        area: GlyphArea,
    ) -> DisplayRowSourceRenderRequest<'face> {
        self.render_request(render_bounds).with_glyph_area(area)
    }

    pub(crate) fn from_glyph_row_columns(
        row: &GlyphRow,
        matrix_cols: usize,
        char_width: f32,
        role: GlyphRowRole,
        base_face_id: FaceId,
        base_face: &'face ResolvedFace,
    ) -> Self {
        let char_width = char_width.max(1.0);
        let height = row.height_px.max(1.0);
        Self::new(
            DisplayRowGeometry::new(
                row.pixel_y,
                matrix_cols.max(1) as f32 * char_width,
                height,
                char_width,
                row.ascent_px.max(0.0).min(height),
                DisplayTabPolicy::every(8),
            ),
            role,
            base_face_id,
            base_face,
        )
    }

    pub(crate) fn from_row_geometry_columns(
        row_geometry: &DisplayRowGeometryState,
        columns: usize,
        char_width: f32,
        role: GlyphRowRole,
        base_face_id: FaceId,
        base_face: &'face ResolvedFace,
    ) -> Self {
        let char_width = char_width.max(1.0);
        Self::new(
            DisplayRowGeometry::new(
                row_geometry.y(),
                columns.max(1) as f32 * char_width,
                row_geometry.height(),
                char_width,
                row_geometry.ascent(),
                DisplayTabPolicy::every(8),
            ),
            role,
            base_face_id,
            base_face,
        )
    }

    pub(crate) fn render_request_from_column(
        self,
        start_col: usize,
        max_col: usize,
    ) -> DisplayRowSourceRenderRequest<'face> {
        let char_width = self.policy.geometry.char_width;
        self.render_request(DisplayRowRenderBounds::new(
            DisplayRowPosition::new(start_col as f32 * char_width, start_col),
            DisplayRowMaxX::Bounded(max_col as f32 * char_width),
        ))
    }

    pub(crate) fn render_request_from_column_for_area(
        self,
        start_col: usize,
        max_col: usize,
        area: GlyphArea,
    ) -> DisplayRowSourceRenderRequest<'face> {
        self.render_request_from_column(start_col, max_col)
            .with_glyph_area(area)
    }
}

pub(crate) struct DisplayRowLispStringSourceSessionRequest {
    source_id: DisplayRowLispStringSourceId,
    value: Value,
    base_face_id: FaceId,
    face_scope: DisplaySourceFaceScope,
}

pub(crate) struct DisplayRowLispStringSourceRenderRequest<'a> {
    row_request: DisplayRowSourceRenderRequest<'a>,
    session_request: DisplayRowLispStringSourceSessionRequest,
}

/// All policy and ownership inputs needed to turn one Lisp string into a
/// display row. Keeping this as one typed request prevents chrome callers from
/// unpacking the source into an order-sensitive scalar argument list at the
/// `display_row` boundary.
pub(crate) struct DisplayRowLispStringSourceRequest<'a> {
    geometry: DisplayRowGeometry,
    origin: DisplayOrigin,
    base_face: &'a ResolvedFace,
    value: Value,
    face_scope: DisplaySourceFaceScope,
    symbol_values: std::collections::HashMap<String, Value>,
    image_scale_environment: ImageScaleEnvironment,
}

impl<'a> DisplayRowLispStringSourceRequest<'a> {
    pub(crate) fn new(
        geometry: DisplayRowGeometry,
        origin: DisplayOrigin,
        base_face: &'a ResolvedFace,
        value: Value,
        face_scope: DisplaySourceFaceScope,
    ) -> Self {
        Self {
            geometry,
            origin,
            base_face,
            value,
            face_scope,
            symbol_values: std::collections::HashMap::new(),
            image_scale_environment: ImageScaleEnvironment::default(),
        }
    }

    pub(crate) fn with_symbol_values(
        mut self,
        symbol_values: std::collections::HashMap<String, Value>,
    ) -> Self {
        self.symbol_values = symbol_values;
        self
    }

    pub(crate) fn with_image_scale_environment(
        mut self,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        self.image_scale_environment = image_scale_environment;
        self
    }

    pub(crate) fn into_render_request(
        self,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayRowLispStringSourceRenderRequest<'a> {
        let role = self
            .origin
            .glyph_row_role()
            .expect("display row source origin must map to a glyph row role");
        let row_request =
            DisplayRowSourceRequestPolicy::from_display_row_geometry(self.geometry, role)
                .with_symbol_values(self.symbol_values)
                .source_request_from_base_face(face_ids, self.base_face);
        DisplayRowLispStringSourceRenderRequest::from_value(
            row_request,
            self.value,
            self.face_scope,
        )
        .with_image_scale_environment(self.image_scale_environment)
    }

    #[cfg(test)]
    pub(crate) fn geometry(&self) -> DisplayRowGeometry {
        self.geometry.clone()
    }

    #[cfg(test)]
    pub(crate) fn origin(&self) -> &DisplayOrigin {
        &self.origin
    }

    #[cfg(test)]
    pub(crate) fn symbol_values(&self) -> &std::collections::HashMap<String, Value> {
        &self.symbol_values
    }
}

impl<'a> DisplayRowLispStringSourceRenderRequest<'a> {
    pub(crate) fn from_value(
        row_request: DisplayRowSourceRenderRequest<'a>,
        value: Value,
        face_scope: DisplaySourceFaceScope,
    ) -> Self {
        let session_request = DisplayRowLispStringSourceSessionRequest::for_base_face_id(
            value,
            row_request.base_face_id(),
            face_scope,
        );
        Self {
            row_request,
            session_request,
        }
    }

    pub(crate) fn with_chrome_text_area_left_px(mut self, text_area_left_px: f32) -> Self {
        self.row_request = self
            .row_request
            .with_chrome_text_area_left_px(text_area_left_px);
        self
    }

    pub(crate) fn base_face_id(&self) -> FaceId {
        self.row_request.base_face_id()
    }

    pub(crate) fn with_image_scale_environment(
        mut self,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        self.row_request = self
            .row_request
            .with_image_scale_environment(image_scale_environment);
        self
    }

    fn into_render_parts(
        self,
    ) -> (
        DisplayRowRenderPlan<'a>,
        DisplayRowLispStringSourceSessionRequest,
    ) {
        (self.row_request.into_render_plan(), self.session_request)
    }
}

impl DisplayRowLispStringSourceSessionRequest {
    fn for_base_face_id(
        value: Value,
        base_face_id: FaceId,
        face_scope: DisplaySourceFaceScope,
    ) -> Self {
        Self {
            source_id: DisplayRowLispStringSourceId::ROOT,
            value,
            base_face_id,
            face_scope,
        }
    }
}

pub(crate) struct DisplayRowLispStringSourceSession {
    source: LispStringSourceCursor,
    state: DisplayRowSourceState,
}

impl DisplayRowLispStringSourceSession {
    pub(crate) fn new(request: DisplayRowLispStringSourceSessionRequest) -> Option<Self> {
        let source = LispStringSourceCursor::new(
            request.source_id.raw(),
            request.value,
            RenderFaceRef::FaceId(request.base_face_id),
            LispStringSourceOrigin::Normal,
        )?;
        Some(Self {
            source,
            state: DisplayRowSourceState::with_face_scope(request.face_scope),
        })
    }

    fn render_next_row_plan_with_context(
        &mut self,
        renderer: &mut DisplayRowRenderer<'_>,
        plan: DisplayRowRenderPlan<'_>,
        context: &mut DisplayRowRenderContext<'_, '_>,
    ) -> Option<RenderedDisplayRow> {
        renderer.render_display_item_source_row_step_with_context(
            plan,
            &mut self.source,
            &mut self.state,
            context,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DisplayRowSourceRequestPolicy {
    geometry: DisplayRowGeometry,
    role: GlyphRowRole,
    symbol_values: std::collections::HashMap<String, Value>,
    image_scale_environment: ImageScaleEnvironment,
}

impl DisplayRowSourceRequestPolicy {
    #[cfg(test)]
    fn new(
        y: f32,
        width: f32,
        height: f32,
        char_width: f32,
        ascent: f32,
        tab_policy: DisplayTabPolicy,
        role: GlyphRowRole,
    ) -> Self {
        Self {
            geometry: DisplayRowGeometry::new(y, width, height, char_width, ascent, tab_policy),
            role,
            symbol_values: std::collections::HashMap::new(),
            image_scale_environment: ImageScaleEnvironment::default(),
        }
    }

    fn from_display_row_geometry(geometry: DisplayRowGeometry, role: GlyphRowRole) -> Self {
        Self {
            geometry,
            role,
            symbol_values: std::collections::HashMap::new(),
            image_scale_environment: ImageScaleEnvironment::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_origin(
        y: f32,
        width: f32,
        height: f32,
        char_width: f32,
        ascent: f32,
        tab_policy: DisplayTabPolicy,
        origin: DisplayOrigin,
    ) -> Self {
        let role = origin
            .glyph_row_role()
            .expect("display row source origin must map to a glyph row role");
        Self::new(y, width, height, char_width, ascent, tab_policy, role)
    }

    pub(crate) fn with_symbol_values(
        mut self,
        symbol_values: std::collections::HashMap<String, Value>,
    ) -> Self {
        self.symbol_values = symbol_values;
        self
    }

    pub(crate) fn source_request_from_base_face<'face>(
        self,
        face_ids: &mut FrameFaceAttempt,
        base_face: &'face ResolvedFace,
    ) -> DisplayRowSourceRenderRequest<'face> {
        let image_scale_environment = self.image_scale_environment;
        DisplayRowSourceRenderRequest::from_base_face(
            self.geometry,
            face_ids,
            base_face,
            self.role,
            self.symbol_values,
            image_scale_environment,
        )
    }

    pub(crate) fn source_request_for_base_face_id<'face>(
        self,
        base_face_id: FaceId,
        base_face: &'face ResolvedFace,
    ) -> DisplayRowSourceRenderRequest<'face> {
        debug_assert!(self.symbol_values.is_empty());
        DisplayRowSourceRenderRequest::whole_row(self.geometry, base_face_id, base_face, self.role)
            .with_image_scale_environment(self.image_scale_environment)
    }
}

struct DisplayRowRenderPlan<'a> {
    geometry: DisplayRowGeometry,
    render_bounds: DisplayRowRenderBounds,
    append_start_policy: DisplayRowAppendStartPolicy,
    line_end_right_edge_x: f32,
    area: GlyphArea,
    base_face_id: FaceId,
    base_face: &'a ResolvedFace,
    role: GlyphRowRole,
    chrome_text_area_left_px: f32,
    symbol_values: std::collections::HashMap<String, Value>,
    image_scale_environment: ImageScaleEnvironment,
}

pub(crate) struct DisplayRowSourceRenderRequest<'a> {
    geometry: DisplayRowGeometry,
    render_bounds: DisplayRowRenderBounds,
    append_start_policy: DisplayRowAppendStartPolicy,
    line_end_right_edge_x: f32,
    area: GlyphArea,
    base_face_id: FaceId,
    base_face: &'a ResolvedFace,
    role: GlyphRowRole,
    chrome_text_area_left_px: f32,
    symbol_values: std::collections::HashMap<String, Value>,
    image_scale_environment: ImageScaleEnvironment,
}

impl<'a> DisplayRowSourceRenderRequest<'a> {
    fn whole_row(
        geometry: DisplayRowGeometry,
        base_face_id: FaceId,
        base_face: &'a ResolvedFace,
        role: GlyphRowRole,
    ) -> Self {
        let render_bounds = DisplayRowRenderBounds::whole_row(geometry.width());
        Self {
            geometry,
            render_bounds,
            append_start_policy: DisplayRowAppendStartPolicy::ReconcileWithRowTail,
            line_end_right_edge_x: render_bounds.max_x().to_f32(),
            area: GlyphArea::Text,
            base_face_id,
            base_face,
            role,
            chrome_text_area_left_px: 0.0,
            symbol_values: std::collections::HashMap::new(),
            image_scale_environment: ImageScaleEnvironment::default(),
        }
    }

    fn from_base_face(
        geometry: DisplayRowGeometry,
        face_ids: &mut FrameFaceAttempt,
        base_face: &'a ResolvedFace,
        role: GlyphRowRole,
        symbol_values: std::collections::HashMap<String, Value>,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        let base_face_id = if base_face.face_id != 0 {
            base_face.display_face_id()
        } else {
            crate::display_row::face_state::stable_face_id_for_resolved(face_ids, base_face)
        };
        let render_bounds = DisplayRowRenderBounds::whole_row(geometry.width());
        Self {
            geometry,
            render_bounds,
            append_start_policy: DisplayRowAppendStartPolicy::ReconcileWithRowTail,
            line_end_right_edge_x: render_bounds.max_x().to_f32(),
            area: GlyphArea::Text,
            base_face_id,
            base_face,
            role,
            chrome_text_area_left_px: 0.0,
            symbol_values,
            image_scale_environment,
        }
    }

    pub(crate) fn from_display_row_geometry_for_base_face_id(
        geometry: DisplayRowGeometry,
        base_face_id: FaceId,
        base_face: &'a ResolvedFace,
        role: GlyphRowRole,
    ) -> Self {
        DisplayRowSourceRequestPolicy::from_display_row_geometry(geometry, role)
            .source_request_for_base_face_id(base_face_id, base_face)
    }

    #[cfg(test)]
    pub(crate) fn from_display_row_geometry(
        geometry: DisplayRowGeometry,
        face_ids: &mut FrameFaceAttempt,
        base_face: &'a ResolvedFace,
        role: GlyphRowRole,
        symbol_values: std::collections::HashMap<String, Value>,
    ) -> Self {
        DisplayRowSourceRequestPolicy::from_display_row_geometry(geometry, role)
            .with_symbol_values(symbol_values)
            .source_request_from_base_face(face_ids, base_face)
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_origin(
        y: f32,
        width: f32,
        height: f32,
        char_width: f32,
        ascent: f32,
        tab_policy: DisplayTabPolicy,
        origin: DisplayOrigin,
        face_ids: &mut FrameFaceAttempt,
        base_face: &'a ResolvedFace,
        symbol_values: std::collections::HashMap<String, Value>,
    ) -> Self {
        DisplayRowSourceRequestPolicy::from_origin(
            y, width, height, char_width, ascent, tab_policy, origin,
        )
        .with_symbol_values(symbol_values)
        .source_request_from_base_face(face_ids, base_face)
    }

    pub(crate) fn with_render_bounds(mut self, render_bounds: DisplayRowRenderBounds) -> Self {
        self.render_bounds = render_bounds;
        self.line_end_right_edge_x = render_bounds.max_x().to_f32();
        self
    }

    pub(crate) fn with_append_start_policy(
        mut self,
        append_start_policy: DisplayRowAppendStartPolicy,
    ) -> Self {
        self.append_start_policy = append_start_policy;
        self
    }

    /// Sets the absolute edge used only by newline face extension.
    ///
    /// TTY text sources reserve the final column for a continuation marker,
    /// so ordinary glyphs must clip before it.  A real newline may still paint
    /// that column with an extending face, matching GNU redisplay.
    pub(crate) fn with_line_end_right_edge_x(mut self, right_edge_x: f32) -> Self {
        self.line_end_right_edge_x = right_edge_x;
        self
    }

    pub(crate) fn with_chrome_text_area_left_px(mut self, text_area_left_px: f32) -> Self {
        self.chrome_text_area_left_px = text_area_left_px.max(0.0);
        self
    }

    pub(crate) fn with_image_scale_environment(
        mut self,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        self.image_scale_environment = image_scale_environment;
        self
    }

    fn with_glyph_area(mut self, area: GlyphArea) -> Self {
        self.area = area;
        self
    }

    #[cfg(test)]
    pub(crate) fn base_face_ref(&self) -> RenderFaceRef {
        RenderFaceRef::FaceId(self.base_face_id)
    }

    pub(crate) fn base_face_id(&self) -> FaceId {
        self.base_face_id
    }

    #[cfg(test)]
    pub(crate) fn base_face(&self) -> &'a ResolvedFace {
        self.base_face
    }

    #[cfg(test)]
    pub(crate) fn geometry(&self) -> &DisplayRowGeometry {
        &self.geometry
    }

    #[cfg(test)]
    pub(crate) fn render_bounds(&self) -> DisplayRowRenderBounds {
        self.render_bounds
    }

    #[cfg(test)]
    pub(crate) fn line_end_right_edge_x(&self) -> f32 {
        self.line_end_right_edge_x
    }

    #[cfg(test)]
    pub(crate) fn role(&self) -> GlyphRowRole {
        self.role
    }

    pub(crate) fn render_fragment_step_into_row_with_policy<
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    >(
        self,
        renderer: &mut DisplayRowRenderer<'_>,
        row: &mut GlyphRow,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        context: &mut DisplayRowRenderContext<'_, '_>,
        render_policy: &mut P,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        renderer.render_display_item_source_row_fragment_step_into_row_with_policy(
            self.into_render_plan(),
            row,
            source,
            source_state,
            context,
            render_policy,
        )
    }

    fn from_base_face_id_policy_with_render_bounds(
        policy: DisplayRowSourceRequestPolicy,
        base_face_id: FaceId,
        base_face: &'a ResolvedFace,
        render_bounds: DisplayRowRenderBounds,
    ) -> Self {
        policy
            .source_request_for_base_face_id(base_face_id, base_face)
            .with_render_bounds(render_bounds)
    }

    #[cfg(test)]
    pub(crate) fn glyph_area(&self) -> GlyphArea {
        self.area
    }

    #[cfg(test)]
    pub(crate) fn render<S: DisplayItemSource>(
        self,
        renderer: &mut DisplayRowRenderer<'_>,
        source: &mut S,
        face_resolver: &FaceResolver,
        face_ids: &mut FrameFaceAttempt,
    ) -> Option<RenderedDisplayRow> {
        let mut context = DisplayRowRenderContext::new(face_resolver, None, face_ids);
        self.render_with_context(renderer, source, &mut context)
    }

    #[cfg(test)]
    fn render_with_context<S: DisplayItemSource>(
        self,
        renderer: &mut DisplayRowRenderer<'_>,
        source: &mut S,
        context: &mut DisplayRowRenderContext<'_, '_>,
    ) -> Option<RenderedDisplayRow> {
        let mut state = DisplayRowSourceState::frame_local();
        self.render_step_with_context(renderer, source, &mut state, context)
    }

    #[cfg(test)]
    pub(crate) fn render_step_with_context<S: DisplayItemSource>(
        self,
        renderer: &mut DisplayRowRenderer<'_>,
        source: &mut S,
        state: &mut DisplayRowSourceState,
        context: &mut DisplayRowRenderContext<'_, '_>,
    ) -> Option<RenderedDisplayRow> {
        renderer.render_display_item_source_row_step_with_context(
            self.into_render_plan(),
            source,
            state,
            context,
        )
    }

    #[cfg(test)]
    pub(crate) fn render_fragment_step_with_display_host<S: DisplayItemSource>(
        self,
        renderer: &mut DisplayRowRenderer<'_>,
        source: &mut S,
        state: &mut DisplayRowSourceState,
        face_resolver: &FaceResolver,
        display_host: Option<&dyn DisplayHost>,
        face_ids: &mut FrameFaceAttempt,
    ) -> Option<RenderedDisplayRow> {
        let mut context = DisplayRowRenderContext::new(face_resolver, display_host, face_ids);
        renderer.render_display_item_source_row_fragment_step_with_context(
            self.into_render_plan(),
            source,
            state,
            &mut context,
        )
    }

    #[cfg(test)]
    pub(crate) fn render_fragment_step_into_row_with_display_host<S: DisplayItemSource>(
        self,
        renderer: &mut DisplayRowRenderer<'_>,
        row: &mut GlyphRow,
        source: &mut S,
        state: &mut DisplayRowSourceState,
        face_resolver: &FaceResolver,
        display_host: Option<&dyn DisplayHost>,
        face_ids: &mut FrameFaceAttempt,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        let mut context = DisplayRowRenderContext::new(face_resolver, display_host, face_ids);
        renderer.render_display_item_source_row_fragment_step_into_row_with_context(
            self.into_render_plan(),
            row,
            source,
            state,
            &mut context,
        )
    }

    #[cfg(test)]
    pub(crate) fn symbol_values(&self) -> &std::collections::HashMap<String, Value> {
        &self.symbol_values
    }

    fn into_render_plan(self) -> DisplayRowRenderPlan<'a> {
        DisplayRowRenderPlan {
            geometry: self.geometry,
            render_bounds: self.render_bounds,
            append_start_policy: self.append_start_policy,
            line_end_right_edge_x: self.line_end_right_edge_x,
            area: self.area,
            base_face_id: self.base_face_id,
            base_face: self.base_face,
            role: self.role,
            chrome_text_area_left_px: self.chrome_text_area_left_px,
            symbol_values: self.symbol_values,
            image_scale_environment: self.image_scale_environment,
        }
    }
}

pub(crate) struct DisplayRowRenderContext<'a, 'ids> {
    face_resolver: &'a FaceResolver,
    display_host: Option<&'a dyn DisplayHost>,
    face_ids: &'ids mut FrameFaceAttempt,
}

impl<'a, 'ids> DisplayRowRenderContext<'a, 'ids> {
    pub(crate) fn new(
        face_resolver: &'a FaceResolver,
        display_host: Option<&'a dyn DisplayHost>,
        face_ids: &'ids mut FrameFaceAttempt,
    ) -> Self {
        Self {
            face_resolver,
            display_host,
            face_ids,
        }
    }

    pub(crate) fn source_resolve_params<'b>(
        &self,
        base_face_id: FaceId,
        base_face: &'b ResolvedFace,
        fallback: DisplayRowFallbackMetrics,
        image_scale_environment: ImageScaleEnvironment,
    ) -> DisplaySourceResolveParams<'b>
    where
        'a: 'b,
    {
        DisplaySourceResolveParams::new(
            DisplaySourceFaceBasis::new(self.face_resolver, base_face_id, base_face, fallback),
            self.display_host.map(|host| host as &'b dyn DisplayHost),
            image_scale_environment,
        )
    }

    fn face_ids(&mut self) -> &mut FrameFaceAttempt {
        self.face_ids
    }
}

pub(crate) struct DisplayRowRenderer<'metrics> {
    font_metrics: &'metrics mut Option<FontMetricsService>,
    measurement_mode: DisplayRowMeasurementMode,
}

pub(crate) struct DisplayRowRenderExecutor<'metrics, 'context, 'ids> {
    renderer: DisplayRowRenderer<'metrics>,
    context: DisplayRowRenderContext<'context, 'ids>,
}

impl<'metrics, 'context, 'ids> DisplayRowRenderExecutor<'metrics, 'context, 'ids> {
    pub(crate) fn new(
        font_metrics: &'metrics mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        face_resolver: &'context FaceResolver,
        display_host: Option<&'context dyn DisplayHost>,
        face_ids: &'ids mut FrameFaceAttempt,
    ) -> Self {
        Self {
            renderer: DisplayRowRenderer::new(font_metrics, measurement_mode),
            context: DisplayRowRenderContext::new(face_resolver, display_host, face_ids),
        }
    }

    pub(crate) fn render_lisp_string_source_request(
        &mut self,
        request: DisplayRowLispStringSourceRenderRequest<'_>,
    ) -> Option<RenderedDisplayRow> {
        let (plan, session_request) = request.into_render_parts();
        self.renderer
            .render_lisp_string_plan_with_context(plan, session_request, &mut self.context)
    }

    pub(crate) fn render_item_source_fragment_into_row<S: DisplayItemSource>(
        &mut self,
        request: DisplayRowSourceRenderRequest<'_>,
        row: &mut GlyphRow,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        request.render_fragment_step_into_row_with_policy(
            &mut self.renderer,
            row,
            source,
            source_state,
            &mut self.context,
            &mut NaturalDisplayRowRenderPolicy,
        )
    }
}

impl<'metrics> DisplayRowRenderer<'metrics> {
    pub(crate) fn new(
        font_metrics: &'metrics mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
    ) -> Self {
        Self {
            font_metrics,
            measurement_mode,
        }
    }

    fn render_lisp_string_plan_with_context(
        &mut self,
        plan: DisplayRowRenderPlan<'_>,
        session_request: DisplayRowLispStringSourceSessionRequest,
        context: &mut DisplayRowRenderContext<'_, '_>,
    ) -> Option<RenderedDisplayRow> {
        let mut session = DisplayRowLispStringSourceSession::new(session_request)?;
        session.render_next_row_plan_with_context(self, plan, context)
    }

    fn render_display_item_source_row_step_with_context(
        &mut self,
        plan: DisplayRowRenderPlan<'_>,
        source: &mut impl DisplayItemSource,
        state: &mut DisplayRowSourceState,
        context: &mut DisplayRowRenderContext<'_, '_>,
    ) -> Option<RenderedDisplayRow> {
        let mut result = self.render_display_item_source_row_fragment_step_with_context(
            plan, source, state, context,
        )?;
        result.normalize_external_row();
        Some(result)
    }

    fn render_display_item_source_row_fragment_step_with_context(
        &mut self,
        plan: DisplayRowRenderPlan<'_>,
        source: &mut impl DisplayItemSource,
        state: &mut DisplayRowSourceState,
        context: &mut DisplayRowRenderContext<'_, '_>,
    ) -> Option<RenderedDisplayRow> {
        let mut row = new_display_row_for_role(plan.role);
        let result = self.render_display_item_source_row_fragment_step_into_row_with_context(
            plan, &mut row, source, state, context,
        )?;
        Some(result.with_row(row))
    }

    fn render_display_item_source_row_fragment_step_into_row_with_context(
        &mut self,
        plan: DisplayRowRenderPlan<'_>,
        row: &mut GlyphRow,
        source: &mut impl DisplayItemSource,
        state: &mut DisplayRowSourceState,
        context: &mut DisplayRowRenderContext<'_, '_>,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        let mut policy = NaturalDisplayRowRenderPolicy;
        self.render_display_item_source_row_fragment_step_into_row_with_policy(
            plan,
            row,
            source,
            state,
            context,
            &mut policy,
        )
    }

    fn render_display_item_source_row_fragment_step_into_row_with_policy<
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    >(
        &mut self,
        plan: DisplayRowRenderPlan<'_>,
        row: &mut GlyphRow,
        source: &mut S,
        state: &mut DisplayRowSourceState,
        context: &mut DisplayRowRenderContext<'_, '_>,
        policy: &mut P,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        if state.is_finished() {
            return None;
        }

        let DisplayRowRenderPlan {
            geometry,
            render_bounds,
            append_start_policy,
            line_end_right_edge_x,
            area,
            base_face_id,
            base_face,
            role,
            chrome_text_area_left_px,
            symbol_values,
            image_scale_environment,
        } = plan;
        context.face_ids().reserve_after(base_face_id);
        let mut face_realizer = DisplayRowFaceRealizer::new(&mut *self.font_metrics);
        let row_face = face_realizer.realize_face(
            base_face_id,
            base_face,
            geometry.char_width(),
            geometry.ascent(),
            geometry.height(),
        );
        let char_width = face_realizer
            .char_width(&row_face, geometry.char_width())
            .max(1.0);
        let mut row_faces = vec![row_face.clone()];

        // Build the chrome row's pixel-calc context from its own geometry so
        // `(space :width/:align-to …)` forms resolve through the single
        // GNU-faithful evaluator (`calc_pixel_width_or_height`), the same
        // authority the buffer text path uses. Region symbols (`text`,
        // `right`, …) now reach real window-region positions instead of the
        // 0.0 the retired `length_expr_pixels` evaluator returned.
        let mut pixel_calc = PixelCalcContext::for_chrome_row(
            geometry.width(),
            char_width,
            geometry.height(),
            symbol_values,
        );
        if matches!(
            role,
            GlyphRowRole::ModeLine | GlyphRowRole::HeaderLine | GlyphRowRole::TabLine
        ) {
            pixel_calc.text_area_left = f64::from(chrome_text_area_left_px.max(0.0));
        }
        let row_ascent = row_face
            .metrics
            .ascent_px()
            .max(geometry.ascent())
            .min(geometry.height().max(1.0));
        // `(space :align-to …)` on this row may embed an `(image …)` operand
        // whose intrinsic size decides the result (GNU resolves it inline with
        // `lookup_image`). Carry the inputs so the row builder can resolve it.
        let space_image_params =
            context
                .display_host
                .map(|host| crate::display_pixel_calc::PixelCalcImageInputs {
                    catalog: host
                        .image_catalog_shared()
                        .map(crate::types::SharedImageCatalog),
                    scale: image_scale_environment,
                    dimensions: crate::display_spec::DisplayImageDimensionEnvironment::new(
                        row_face.font_size,
                        geometry.height(),
                        char_width,
                    ),
                    default_fg: base_face.fg,
                    default_bg: base_face.bg,
                });
        let mut row_layout = geometry.to_layout(
            role,
            char_width,
            row_ascent,
            RenderFaceRef::FaceId(row_face.face_id),
            pixel_calc,
            space_image_params,
        );
        let mut position = render_bounds.start();
        let mut source_slots = Vec::new();
        let fallback_metrics = DisplayRowFallbackMetrics::from_default_face_extents(
            char_width,
            geometry.height(),
            geometry.ascent(),
        );
        let mut row_break_face = None;
        let stop = loop {
            let params = context.source_resolve_params(
                row_face.face_id,
                base_face,
                fallback_metrics,
                image_scale_environment,
            );
            let resolved = state.next_resolved_item(source, params, context.face_ids());
            let (item, pending_faces) = resolved.into_parts();
            for pending in pending_faces {
                let (face_id, resolved) = pending.into_parts();
                let row_face = face_realizer.realize_face(
                    face_id,
                    &resolved,
                    char_width,
                    geometry.ascent(),
                    geometry.height(),
                );
                if !self.measurement_mode.uses_concrete_font_geometry()
                    || !face_realizer.has_font_metrics()
                {
                    include_display_row_face_metrics(&mut row_layout, &row_face);
                }
                row_faces.push(row_face);
            }
            let Some(item) = item else {
                break DisplayRowRenderStop::SourceExhausted;
            };
            let item_face_id = render_face_ref_id(item.face, row_face.face_id);
            let item_resolved_face = state.resolved_face(item_face_id).unwrap_or(base_face);
            if policy.stop_before_item(&item, item_face_id, item_resolved_face) {
                break DisplayRowRenderStop::SourceExhausted;
            }
            if let RenderFaceRef::FaceId(face_id) = item.face
                && face_id != row_face.face_id
                && !row_faces.iter().any(|face| face.face_id == face_id)
                && let Some(resolved) = state.resolved_face(face_id).cloned()
            {
                let realized = face_realizer.realize_face(
                    face_id,
                    &resolved,
                    char_width,
                    geometry.ascent(),
                    geometry.height(),
                );
                if !self.measurement_mode.uses_concrete_font_geometry()
                    || !face_realizer.has_font_metrics()
                {
                    include_display_row_face_metrics(&mut row_layout, &realized);
                }
                row_faces.push(realized);
            }
            let item_edges = item.box_vertical_edges;
            let item = item.with_box_run_topology(item_resolved_face.box_type > 0, item_edges);
            let render_item = DisplayRowRenderItem::from_source_item(item);
            let measurement = policy.measurement_for(
                render_item.row_item(),
                item_face_id,
                face_realizer.font_metrics_mut(),
            );
            let progress = match measurement {
                DisplayRowItemMeasurement::Default => {
                    let mut glyph_measurer = DisplayRowGlyphMeasurer::with_mode(
                        &row_faces,
                        face_realizer.font_metrics_service_mut(),
                        char_width,
                        crate::glyph_advance::GlyphAdvanceQuantization::PreserveLogicalPixels,
                        self.measurement_mode,
                    );
                    let mut row_writer =
                        DisplayRowProgressWriter::with_glyph_measurer_for_area_and_start_policy(
                            &row_layout,
                            &mut *row,
                            &mut glyph_measurer,
                            position,
                            render_bounds.max_x().to_f32(),
                            area,
                            append_start_policy,
                        );
                    row_writer.push_item(render_item.row_item_for_write())
                }
                DisplayRowItemMeasurement::TextRun(measurement) => {
                    let mut glyph_measurer = DisplayRowGlyphMeasurer::with_mode(
                        &row_faces,
                        face_realizer.font_metrics_service_mut(),
                        char_width,
                        crate::glyph_advance::GlyphAdvanceQuantization::PreserveLogicalPixels,
                        self.measurement_mode,
                    );
                    let mut row_writer = DisplayRowProgressWriter::
                        with_text_run_measurement_and_glyph_measurer_for_area_and_start_policy(
                            &row_layout,
                            &mut *row,
                            measurement,
                            &mut glyph_measurer,
                            position,
                            render_bounds.max_x().to_f32(),
                            area,
                            append_start_policy,
                        );
                    row_writer.push_item(render_item.row_item_for_write())
                }
            };
            position = progress.end();
            source_slots.extend(progress.slots().iter().cloned());
            match progress.status() {
                DisplayRowAppendStatus::Complete => {}
                DisplayRowAppendStatus::Clipped => {
                    match policy.clipped_behavior(render_item.source_item()) {
                        DisplayRowRenderClipBehavior::PreserveRemainderAndStop => {
                            state.remember_pending_item(render_item.clipped_remainder(&progress));
                            break DisplayRowRenderStop::Clipped;
                        }
                        DisplayRowRenderClipBehavior::Continue => {}
                    }
                }
                DisplayRowAppendStatus::RowBreak => {
                    let DisplayItemKind::RowBreak(row_break) = render_item.source_item().kind
                    else {
                        unreachable!("row-break status requires a row-break display item")
                    };
                    row_break_face = Some((
                        item_face_id,
                        render_item.source_item().box_vertical_edges,
                        render_item.source_item().box_run_membership,
                    ));
                    break DisplayRowRenderStop::RowBreak(row_break);
                }
            }
        };
        if let (
            DisplayRowRenderStop::RowBreak(row_break),
            Some((row_break_face_id, box_edges, box_membership)),
        ) = (stop, row_break_face)
        {
            DisplayRowLineEndFinalizer::new(
                row_break,
                row_break_face_id,
                line_end_right_edge_x - position.x_px(),
                fallback_metrics,
                row_face.background,
                self.measurement_mode,
                box_edges,
                box_membership,
            )
            .finalize(row, &row_faces);
        }
        let progress_height = if row.height_px > 0.0 {
            row.height_px
        } else {
            row_layout.height_px
        };
        let progress = display_row_progress(position, geometry.y(), progress_height);
        let faces = row_faces
            .into_iter()
            .map(|face| face.render_face())
            .collect();
        Some(DisplayRowRenderIntoRowResult::new(
            progress,
            source_slots,
            faces,
            stop,
        ))
    }
}

#[cfg(test)]
#[path = "test.rs"]
mod tests;

// Submodules of the display_row family (moved from flat crate root).
pub(crate) mod append_context;
pub(crate) mod builder;
pub(crate) mod face_environment;
pub(crate) mod face_state;
pub(crate) mod finalizer;
pub(crate) mod geometry;
pub(crate) mod line_end;
pub(crate) mod line_number_prefix;
pub(crate) mod lisp_string;
pub(crate) mod measured_state;
pub(crate) mod metrics;
pub(crate) mod overlay_string;
pub(crate) mod render_item;
pub(crate) mod render_policy;
pub(crate) mod render_state;
pub(crate) mod replacement;
pub(crate) mod source_append;
pub(crate) mod source_render;
pub(crate) mod source_state;
pub(crate) mod special_glyphs;
pub(crate) mod text_output;
pub(crate) mod trailing_whitespace;
pub(crate) mod transition;
pub(crate) mod walk_state;
pub(crate) mod width;
