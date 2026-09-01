use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{DisplayItem, DisplayItemKind, RenderFaceRef};
use crate::display_row::append_context::{
    DisplayRowActiveFaceAppendContext, DisplayRowAppendFrame, DisplayRowAppendKind,
    DisplayRowAppendSurface,
};
use crate::display_row::builder::{
    DisplayRowAppendProgress, DisplayRowAppendStartPolicy, DisplayRowPosition,
};
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::DisplayRowGeometryState;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::render_policy::DisplayRowRenderPolicy;
use crate::display_row::render_state::CurrentTextRowRenderOutcome;
use crate::display_row::source_render::{TextRowSourceMeasureState, TextRowSourceRenderState};
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_source::{DisplayItemSegmentSource, DisplayItemSource, SyntheticTextItemSource};
use crate::display_source_append_plan::{
    DisplaySourceAppendRenderPolicy, DisplaySourceFallbackWidth,
    NaturalDisplayRowAppendRenderPolicy,
};
use crate::frame_face_arena::{FrameFaceArena, FrameFaceAttempt};
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::face::BasicFaceId;
use neomacs_display_protocol::types::FaceId;

const SYNTHETIC_SOURCE_INVISIBLE_ELLIPSIS: u64 = 3;
const SYNTHETIC_SOURCE_HSCROLL_TRUNCATION: u64 = 4;
const SYNTHETIC_SOURCE_SELECTIVE_ELLIPSIS: u64 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntheticTextSource {
    source_id: u64,
    text: Box<str>,
}

#[derive(Clone, Debug)]
pub(crate) struct SyntheticTextAppendRequest {
    position: DisplayRowPosition,
    source: SyntheticTextSource,
    face: SyntheticTextAppendFace,
}

#[derive(Clone, Debug)]
enum SyntheticTextAppendFace {
    ActiveFace,
    TextRowMetrics {
        face_id: FaceId,
        base_face: ResolvedFace,
        metrics: DisplayRowFallbackMetrics,
    },
}

#[derive(Clone)]
struct SyntheticTextAppendContext<'a> {
    face_id: FaceId,
    base_face: &'a ResolvedFace,
    frame: DisplayRowAppendFrame,
    face_attempt: FrameFaceAttempt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyntheticTextMarker {
    InvisibleEllipsis,
    HscrollTruncation,
    SelectiveEllipsis,
}

#[derive(Clone)]
pub(crate) struct SyntheticTextRowAppendContext<'a> {
    active_face_context: DisplayRowActiveFaceAppendContext<'a, 'a>,
    face_attempt: FrameFaceAttempt,
}

#[derive(Clone)]
pub(crate) struct BufferSyntheticTextRenderContext<'a> {
    append_surface: &'a DisplayRowAppendSurface,
    active_face: &'a DisplayRowActiveFaceState,
    glyph_y_offset: f32,
    metrics: DisplayRowFallbackMetrics,
    face_attempt: FrameFaceAttempt,
}

struct PreparedSingleDisplayItemSourceAppend {
    item: DisplayItem,
    face_id: FaceId,
    kind: DisplayRowAppendKind,
    position: DisplayRowPosition,
}

impl PreparedSingleDisplayItemSourceAppend {
    fn into_parts(
        self,
    ) -> (
        DisplayItem,
        FaceId,
        DisplayRowAppendKind,
        DisplayRowPosition,
    ) {
        (self.item, self.face_id, self.kind, self.position)
    }
}

#[derive(Clone)]
pub(crate) struct SingleDisplayItemAppendContext<'face> {
    base_face: &'face ResolvedFace,
    face_id: FaceId,
    frame: DisplayRowAppendFrame,
    start_policy: DisplayRowAppendStartPolicy,
}

impl SyntheticTextSource {
    #[cfg(test)]
    pub(crate) fn new(source_id: u64, text: impl Into<Box<str>>) -> Self {
        Self {
            source_id,
            text: text.into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn source_id(&self) -> u64 {
        self.source_id
    }

    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    fn marker(marker: SyntheticTextMarker) -> Self {
        Self {
            source_id: marker.source_id(),
            text: marker.text().into(),
        }
    }

    /// Build an ellipsis source for `marker` using `text` when present (e.g.
    /// the buffer display table's selective-display glyphs), falling back to
    /// the marker's hard-coded default text otherwise.
    fn marker_with_text(marker: SyntheticTextMarker, text: Option<&str>) -> Self {
        match text {
            Some(text) if !text.is_empty() => Self {
                source_id: marker.source_id(),
                text: text.into(),
            },
            _ => Self::marker(marker),
        }
    }

    fn into_item_source(self, face_id: FaceId) -> SyntheticTextItemSource {
        SyntheticTextItemSource::new(self.source_id, self.text, RenderFaceRef::FaceId(face_id), 0)
    }
}

impl SyntheticTextAppendRequest {
    #[cfg(test)]
    pub(crate) fn active_source(position: DisplayRowPosition, source: SyntheticTextSource) -> Self {
        Self {
            position,
            source,
            face: SyntheticTextAppendFace::ActiveFace,
        }
    }

    #[cfg(test)]
    pub(crate) fn active_marker(position: DisplayRowPosition, marker: SyntheticTextMarker) -> Self {
        Self {
            position,
            source: SyntheticTextSource::marker(marker),
            face: SyntheticTextAppendFace::ActiveFace,
        }
    }

    /// Like `active_marker`, but uses `text` (the buffer display table's
    /// selective-display glyphs) for the ellipsis when present, otherwise the
    /// marker's default text.
    pub(crate) fn active_marker_with_text(
        position: DisplayRowPosition,
        marker: SyntheticTextMarker,
        text: Option<&str>,
    ) -> Self {
        Self {
            position,
            source: SyntheticTextSource::marker_with_text(marker, text),
            face: SyntheticTextAppendFace::ActiveFace,
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_row_metrics_source(
        position: DisplayRowPosition,
        source: SyntheticTextSource,
        face_id: FaceId,
        base_face: &ResolvedFace,
        metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            position,
            source,
            face: SyntheticTextAppendFace::TextRowMetrics {
                face_id,
                base_face: base_face.clone(),
                metrics,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_row_metrics_marker(
        position: DisplayRowPosition,
        marker: SyntheticTextMarker,
        face_id: FaceId,
        base_face: &ResolvedFace,
        metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            position,
            source: SyntheticTextSource::marker(marker),
            face: SyntheticTextAppendFace::TextRowMetrics {
                face_id,
                base_face: base_face.clone(),
                metrics,
            },
        }
    }

    fn into_parts(
        self,
    ) -> (
        DisplayRowPosition,
        SyntheticTextSource,
        SyntheticTextAppendFace,
    ) {
        (self.position, self.source, self.face)
    }
}

impl<'a> SyntheticTextAppendContext<'a> {
    fn with_face_attempt(
        face_id: FaceId,
        base_face: &'a ResolvedFace,
        frame: DisplayRowAppendFrame,
        face_attempt: FrameFaceAttempt,
    ) -> Self {
        Self {
            face_id,
            base_face,
            frame,
            face_attempt,
        }
    }

    #[cfg(test)]
    fn new(face_id: FaceId, base_face: &'a ResolvedFace, frame: DisplayRowAppendFrame) -> Self {
        Self::with_face_attempt(
            face_id,
            base_face,
            frame,
            crate::frame_face_arena::FrameFaceArena::default().begin_attempt(),
        )
    }

    fn append_to_text_row_and_emit(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        position: DisplayRowPosition,
        source: SyntheticTextSource,
    ) -> Option<DisplayRowAppendProgress> {
        append_synthetic_text_to_display_row(
            state,
            &mut self.face_attempt.clone(),
            self.base_face,
            self.frame.clone(),
            position,
            source,
            self.face_id,
        )
    }
}

impl SyntheticTextMarker {
    pub(crate) fn source_id(self) -> u64 {
        match self {
            Self::InvisibleEllipsis => SYNTHETIC_SOURCE_INVISIBLE_ELLIPSIS,
            Self::HscrollTruncation => SYNTHETIC_SOURCE_HSCROLL_TRUNCATION,
            Self::SelectiveEllipsis => SYNTHETIC_SOURCE_SELECTIVE_ELLIPSIS,
        }
    }

    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::InvisibleEllipsis | Self::SelectiveEllipsis => "...",
            Self::HscrollTruncation => "$",
        }
    }
}

impl<'a> SyntheticTextRowAppendContext<'a> {
    fn with_face_attempt(
        append_surface: &'a DisplayRowAppendSurface,
        geometry: &'a DisplayRowGeometryState,
        active_face: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        face_attempt: FrameFaceAttempt,
    ) -> Self {
        Self {
            active_face_context: DisplayRowActiveFaceAppendContext::new(
                append_surface,
                geometry,
                active_face,
                glyph_y_offset,
                fallback_metrics,
            ),
            face_attempt,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(
        append_surface: &'a DisplayRowAppendSurface,
        geometry: &'a DisplayRowGeometryState,
        active_face: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self::with_face_attempt(
            append_surface,
            geometry,
            active_face,
            glyph_y_offset,
            fallback_metrics,
            FrameFaceArena::default().begin_attempt(),
        )
    }

    fn active_face(
        self,
        face_id: FaceId,
        base_face: &'a ResolvedFace,
    ) -> SyntheticTextAppendContext<'a> {
        SyntheticTextAppendContext::with_face_attempt(
            face_id,
            base_face,
            self.active_face_context.active_face_frame(),
            self.face_attempt.clone(),
        )
    }

    fn text_row<'face>(
        self,
        face_id: FaceId,
        base_face: &'face ResolvedFace,
        height_px: f32,
        ascent_px: f32,
        char_width_px: f32,
    ) -> SyntheticTextAppendContext<'face> {
        SyntheticTextAppendContext::with_face_attempt(
            face_id,
            base_face,
            self.active_face_context
                .text_row_frame(height_px, ascent_px, char_width_px),
            self.face_attempt.clone(),
        )
    }

    pub(crate) fn append_request_to_text_row_and_emit(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        request: SyntheticTextAppendRequest,
    ) -> Option<DisplayRowAppendProgress> {
        let (position, source, face) = request.into_parts();
        match face {
            SyntheticTextAppendFace::ActiveFace => {
                let active_face = self.active_face_context.active_face();
                self.active_face(active_face.face_id(), active_face.resolved_face())
                    .append_to_text_row_and_emit(state, position, source)
            }
            SyntheticTextAppendFace::TextRowMetrics {
                face_id,
                base_face,
                metrics,
            } => self
                .text_row(
                    face_id,
                    &base_face,
                    metrics.row_height(),
                    metrics.ascent(),
                    metrics.char_width(),
                )
                .append_to_text_row_and_emit(state, position, source),
        }
    }
}

impl<'a> BufferSyntheticTextRenderContext<'a> {
    pub(crate) fn with_face_attempt(
        append_surface: &'a DisplayRowAppendSurface,
        active_face: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        metrics: DisplayRowFallbackMetrics,
        face_attempt: FrameFaceAttempt,
    ) -> Self {
        Self {
            append_surface,
            active_face,
            glyph_y_offset,
            metrics,
            face_attempt,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(
        append_surface: &'a DisplayRowAppendSurface,
        active_face: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self::with_face_attempt(
            append_surface,
            active_face,
            glyph_y_offset,
            metrics,
            crate::frame_face_arena::FrameFaceArena::default().begin_attempt(),
        )
    }

    pub(crate) fn active_face(&self) -> &'a DisplayRowActiveFaceState {
        self.active_face
    }

    pub(crate) fn metrics(&self) -> DisplayRowFallbackMetrics {
        self.metrics
    }

    fn row_context(
        self,
        geometry: &'a DisplayRowGeometryState,
    ) -> SyntheticTextRowAppendContext<'a> {
        SyntheticTextRowAppendContext::with_face_attempt(
            self.append_surface,
            geometry,
            self.active_face,
            self.glyph_y_offset,
            self.metrics,
            self.face_attempt,
        )
    }

    pub(crate) fn render_request_to_text_row(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        geometry: &'a DisplayRowGeometryState,
        request: SyntheticTextAppendRequest,
    ) -> Option<DisplayRowAppendProgress> {
        self.row_context(geometry)
            .append_request_to_text_row_and_emit(state, request)
    }

    #[cfg(test)]
    pub(crate) fn render_active_marker_to_text_row(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        geometry: &'a DisplayRowGeometryState,
        position: DisplayRowPosition,
        marker: SyntheticTextMarker,
    ) -> Option<DisplayRowPosition> {
        self.render_request_to_text_row(
            state,
            geometry,
            SyntheticTextAppendRequest::active_marker(position, marker),
        )
        .map(|progress| progress.end())
    }

    pub(crate) fn hscroll_truncation_request(
        &self,
        base_face: ResolvedFace,
        content_x: f32,
    ) -> SyntheticTextAppendRequest {
        SyntheticTextAppendRequest::text_row_metrics_marker(
            DisplayRowPosition::new(content_x, 0),
            SyntheticTextMarker::HscrollTruncation,
            BasicFaceId::Default.into(),
            &base_face,
            self.metrics,
        )
    }

    #[cfg(test)]
    pub(crate) fn render_hscroll_truncation_marker_to_text_row(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        geometry: &'a DisplayRowGeometryState,
        content_x: f32,
    ) -> Option<DisplayRowPosition> {
        let request = self.hscroll_truncation_request(state.default_face(), content_x);
        self.render_request_to_text_row(state, geometry, request)
            .map(|progress| progress.end())
    }
}

impl<'face> SingleDisplayItemAppendContext<'face> {
    pub(crate) fn new(
        base_face: &'face ResolvedFace,
        face_id: FaceId,
        frame: DisplayRowAppendFrame,
    ) -> Self {
        Self {
            base_face,
            face_id,
            frame,
            start_policy: DisplayRowAppendStartPolicy::ReconcileWithRowTail,
        }
    }

    /// Buffer-source progress already starts after structural TEXT_AREA
    /// prefixes such as line numbers.  Keep that coordinate authority instead
    /// of deriving a second start from the row's materialized glyph tail.
    pub(crate) fn for_source_walk(
        base_face: &'face ResolvedFace,
        face_id: FaceId,
        frame: DisplayRowAppendFrame,
    ) -> Self {
        Self {
            base_face,
            face_id,
            frame,
            start_policy: DisplayRowAppendStartPolicy::SourcePosition,
        }
    }

    pub(crate) fn face_id(&self) -> FaceId {
        self.face_id
    }

    #[cfg(test)]
    pub(crate) fn frame(&self) -> &DisplayRowAppendFrame {
        &self.frame
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_source_with_policy<S, P>(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
        render_policy: &mut P,
        face_id: FaceId,
    ) -> Option<CurrentTextRowRenderOutcome>
    where
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    {
        let row_request = self
            .frame
            .source_append_render_request(position, face_id, self.base_face, kind)
            .with_append_start_policy(self.start_policy);
        let outcome = state.render_display_item_source_into_current_text_row_and_emit(
            face_ids,
            source,
            source_state,
            row_request,
            render_policy,
        );
        // Route every placement collected while walking nested Lisp/overlay
        // strings. The frame carries the structural-area capacities, so margin
        // content cannot leak into inline text geometry.
        let pending_non_text_area = source_state.take_pending_non_text_area();
        let face_scope = source_state.face_scope();
        let structural_order = if position.col() == 0 {
            crate::display_row::source_render::DisplayStructuralAreaOrder::BeforeExisting
        } else {
            crate::display_row::source_render::DisplayStructuralAreaOrder::AfterExisting
        };
        for emission in pending_non_text_area {
            state.render_non_text_area_emission(
                emission,
                face_scope,
                &self.frame,
                face_ids,
                face_id,
                structural_order,
            );
        }
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    fn measure_source_with_policy<S, P>(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        face_ids: &mut FrameFaceAttempt,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
        render_policy: &mut P,
        face_id: FaceId,
    ) -> Option<CurrentTextRowRenderOutcome>
    where
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    {
        let row_request = self
            .frame
            .source_append_measure_request(position, face_id, self.base_face, kind)
            .with_append_start_policy(self.start_policy);
        state.measure_display_item_source_against_current_text_row(
            face_ids,
            source,
            source_state,
            row_request,
            render_policy,
        )
    }

    fn prepare_item(
        &self,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
    ) -> PreparedSingleDisplayItemSourceAppend {
        prepare_single_display_item_source_append(item, self.face_id, position, kind)
    }

    pub(crate) fn render_with_policy<P: DisplayRowRenderPolicy>(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
        render_policy: &mut P,
    ) -> Option<DisplayRowAppendProgress> {
        let prepared = self.prepare_item(item, position, kind);
        let (item, face_id, kind, position) = prepared.into_parts();
        face_ids.reserve_after(face_id);
        let mut source = DisplayItemSegmentSource::new(item);
        let mut source_state = DisplayRowSourceState::frame_local();
        let outcome = self.render_source_with_policy(
            state,
            face_ids,
            &mut source,
            &mut source_state,
            position,
            kind,
            render_policy,
            face_id,
        )?;
        Some(outcome.into_append_progress(position))
    }

    pub(crate) fn render_naturally(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
    ) -> Option<DisplayRowAppendProgress> {
        let mut render_policy = NaturalDisplayRowAppendRenderPolicy;
        self.render_with_policy(state, face_ids, item, position, kind, &mut render_policy)
    }

    fn measure_progress_with_policy<P: DisplayRowRenderPolicy>(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
        render_policy: &mut P,
    ) -> Option<DisplayRowAppendProgress> {
        let prepared = self.prepare_item(item, position, kind);
        let (item, face_id, kind, position) = prepared.into_parts();
        // Measurement identities never escape into frame output. Keep them in
        // an isolated arena so measuring cannot consume or publish frame IDs.
        let mut face_ids = FrameFaceArena::default().begin_attempt();
        face_ids.reserve_after(face_id);
        let mut source = DisplayItemSegmentSource::new(item);
        let mut source_state = DisplayRowSourceState::frame_local();
        let outcome = self.measure_source_with_policy(
            state,
            &mut face_ids,
            &mut source,
            &mut source_state,
            position,
            kind,
            render_policy,
            face_id,
        )?;
        Some(outcome.into_append_progress(position))
    }

    pub(crate) fn measure_width_with_policy<P: DisplayRowRenderPolicy>(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
        render_policy: &mut P,
    ) -> Option<f32> {
        self.measure_progress_with_policy(state, item, position, kind, render_policy)
            .map(|progress| progress.metrics().width_px())
    }

    pub(crate) fn measure_width_naturally(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
    ) -> Option<f32> {
        let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
        self.measure_width_with_policy(state, item, position, kind, &mut render_policy)
    }

    /// Natural measurement returning the END position (x AND col) the append
    /// would reach — what a position-dependent advance (tab stops, 2-col
    /// chars) needs to seed a following measurement at the exact pen the
    /// pipeline's own walk would use.
    pub(crate) fn measure_advance_naturally(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
    ) -> Option<DisplayRowPosition> {
        let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
        self.measure_progress_with_policy(state, item, position, kind, &mut render_policy)
            .map(|progress| progress.end())
    }

    pub(crate) fn measure_width_with_source_fallback(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        item: DisplayItem,
        position: DisplayRowPosition,
        kind: DisplayRowAppendKind,
        fallback_width: DisplaySourceFallbackWidth,
    ) -> f32 {
        let fallback_width_px = fallback_width.resolve_to_text_row(&self.frame);
        self.measure_width_naturally(state, item, position, kind)
            .unwrap_or(fallback_width_px)
    }
}

fn append_synthetic_text_to_display_row(
    state: &mut TextRowSourceRenderState<'_>,
    face_ids: &mut FrameFaceAttempt,
    base_face: &ResolvedFace,
    frame: DisplayRowAppendFrame,
    position: DisplayRowPosition,
    source: SyntheticTextSource,
    face_id: FaceId,
) -> Option<DisplayRowAppendProgress> {
    let mut source = source.into_item_source(face_id);
    let mut render_policy = NaturalDisplayRowAppendRenderPolicy;
    let start = position;
    face_ids.reserve_after(face_id);
    let context = SingleDisplayItemAppendContext::new(base_face, face_id, frame);
    let mut source_state = DisplayRowSourceState::frame_local();
    let outcome = context.render_source_with_policy(
        state,
        face_ids,
        &mut source,
        &mut source_state,
        position,
        DisplayRowAppendKind::SourceText,
        &mut render_policy,
        face_id,
    )?;
    Some(outcome.into_append_progress(start))
}

fn prepare_single_display_item_source_append(
    item: DisplayItem,
    fallback_face_id: FaceId,
    position: DisplayRowPosition,
    fallback_kind: DisplayRowAppendKind,
) -> PreparedSingleDisplayItemSourceAppend {
    let kind = display_item_append_kind(&item, fallback_kind);
    let face_id = render_face_ref_id(item.face, fallback_face_id);
    let mut item = item;
    item.face = RenderFaceRef::FaceId(face_id);
    PreparedSingleDisplayItemSourceAppend {
        item,
        face_id,
        kind,
        position,
    }
}

pub(crate) fn display_item_append_kind(
    item: &DisplayItem,
    fallback: DisplayRowAppendKind,
) -> DisplayRowAppendKind {
    match &item.kind {
        DisplayItemKind::TextRun(run) if run.text.as_ref() == "\t" => DisplayRowAppendKind::Tab,
        DisplayItemKind::TextRun(_) => DisplayRowAppendKind::SourceText,
        DisplayItemKind::SourceMappedText(_) => DisplayRowAppendKind::SourceMappedText,
        DisplayItemKind::ControlChar { .. } => DisplayRowAppendKind::ControlChar,
        DisplayItemKind::Glyphless(_) => DisplayRowAppendKind::Glyphless,
        _ => fallback,
    }
}

#[cfg(test)]
#[path = "append_test.rs"]
mod tests;
