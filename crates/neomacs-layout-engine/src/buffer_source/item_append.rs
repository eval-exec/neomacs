use crate::display_item::{DisplayItem, DisplayItemKind, RenderFaceRef};
use crate::display_row::append_context::{
    DisplayRowActiveFaceAppendContext, DisplayRowAppendKind, DisplayRowAppendSurface,
};
use crate::display_row::builder::{DisplayRowAppendProgress, DisplayRowPosition};
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::DisplayRowGeometryState;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::source_append::SingleDisplayItemAppendContext;
use crate::display_row::source_render::{TextRowSourceMeasureState, TextRowSourceRenderState};
#[cfg(test)]
use crate::display_source::DisplaySourceTextItemRequest;
use crate::display_source::{
    DisplaySourceItemRequest, DisplaySourceRangeItemAppendRequest, DisplaySourceRenderPlanRequest,
    DisplaySourceSpecialDisplayKind, DisplaySourceTextChar, DisplaySourceTextRequest,
    DisplaySpecialSourceCharRequest,
};
use crate::display_source_append_plan::{
    DisplaySourceAppendRenderPlan, DisplaySourceAppendRenderPolicy,
};
use crate::display_source_item_append::{
    DisplaySourceCharAppendContext, DisplaySourceItemAppendContext,
    DisplaySourcePreparedCharAppend, DisplaySourceRowAppendState,
    DisplaySourceSpecialCharAppendPlan, DisplaySourceSpecialCharPreparedAppend,
    DisplaySourceTextCharAppendPlan, DisplaySourceTextCharPreparedAppend,
};
#[cfg(test)]
use crate::frame_face_arena::FrameFaceArena;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{LayoutBufferView, ResolvedFace};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::BufferId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferSourceActiveFaceRowMetrics {
    fallback_metrics: DisplayRowFallbackMetrics,
}

impl BufferSourceActiveFaceRowMetrics {
    pub(crate) fn from_active_face_row(
        active_face: &DisplayRowActiveFaceState,
        row_height_px: f32,
    ) -> Self {
        let active_face_metrics = active_face.metrics();
        Self {
            fallback_metrics: DisplayRowFallbackMetrics::from_measured_face(active_face_metrics)
                .with_row_height(row_height_px),
        }
    }

    pub(crate) fn fallback_metrics(self) -> DisplayRowFallbackMetrics {
        self.fallback_metrics
    }
}

impl DisplaySourceTextRequest {
    #[cfg(test)]
    pub(crate) fn append_request<B: LayoutBufferView + ?Sized>(
        self,
        buffer_id: BufferId,
        buffer: &B,
        face_id: FaceId,
    ) -> Option<DisplaySourceRangeItemAppendRequest> {
        buffer_source_text_item_append_request(self.source_item(), buffer_id, buffer, face_id)
    }
}

#[cfg(test)]
pub(crate) fn buffer_source_text_item_append_request<B: LayoutBufferView + ?Sized>(
    source_item: DisplaySourceTextItemRequest,
    buffer_id: BufferId,
    buffer: &B,
    face_id: FaceId,
) -> Option<DisplaySourceRangeItemAppendRequest> {
    let append_kind = source_item.append_kind();
    let item = source_item.into_display_item(buffer_id, buffer, RenderFaceRef::FaceId(face_id))?;
    Some(DisplaySourceRangeItemAppendRequest::new(item, append_kind))
}

#[derive(Clone)]
pub(crate) struct BufferSourceRowAppendContext<'source, 'surface, B: LayoutBufferView + ?Sized> {
    buffer: &'source B,
    buffer_id: BufferId,
    append_surface: &'surface DisplayRowAppendSurface,
    active_face: &'source DisplayRowActiveFaceState,
    resolved_item_face: Option<BufferSourceResolvedItemFace>,
    glyph_y_offset: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
    face_attempt: FrameFaceAttempt,
}

#[derive(Clone, Debug)]
struct BufferSourceResolvedItemFace {
    face_id: FaceId,
    face: ResolvedFace,
}

impl BufferSourceResolvedItemFace {
    fn new(face_id: FaceId, face: ResolvedFace) -> Self {
        Self { face_id, face }
    }
}

#[derive(Clone, Copy)]
enum BufferSourceItemFace<'a> {
    Active(&'a DisplayRowActiveFaceState),
    Resolved(&'a BufferSourceResolvedItemFace),
}

impl<'a> BufferSourceItemFace<'a> {
    fn face_id(self) -> FaceId {
        match self {
            Self::Active(face) => face.face_id(),
            Self::Resolved(face) => face.face_id,
        }
    }

    fn resolved_face(self) -> &'a ResolvedFace {
        match self {
            Self::Active(face) => face.resolved_face(),
            Self::Resolved(face) => &face.face,
        }
    }

    fn bind_item(self, mut item: DisplayItem) -> DisplayItem {
        item.face = RenderFaceRef::FaceId(self.face_id());
        item
    }
}

impl<'source, 'surface, B: LayoutBufferView + ?Sized>
    BufferSourceRowAppendContext<'source, 'surface, B>
{
    fn new_with_face_attempt(
        buffer: &'source B,
        buffer_id: BufferId,
        append_surface: &'surface DisplayRowAppendSurface,
        active_face: &'source DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        face_attempt: FrameFaceAttempt,
    ) -> Self {
        Self {
            buffer,
            buffer_id,
            append_surface,
            active_face,
            resolved_item_face: None,
            glyph_y_offset,
            fallback_metrics,
            face_attempt,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(
        buffer: &'source B,
        buffer_id: BufferId,
        append_surface: &'surface DisplayRowAppendSurface,
        active_face: &'source DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self::new_with_face_attempt(
            buffer,
            buffer_id,
            append_surface,
            active_face,
            glyph_y_offset,
            fallback_metrics,
            FrameFaceArena::default().begin_attempt(),
        )
    }

    pub(crate) fn from_active_face_row(
        buffer: &'source B,
        buffer_id: BufferId,
        append_surface: &'surface DisplayRowAppendSurface,
        active_face: &'source DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        row_height_px: f32,
        face_attempt: FrameFaceAttempt,
    ) -> Self {
        Self::new_with_face_attempt(
            buffer,
            buffer_id,
            append_surface,
            active_face,
            glyph_y_offset,
            BufferSourceActiveFaceRowMetrics::from_active_face_row(active_face, row_height_px)
                .fallback_metrics(),
            face_attempt,
        )
    }

    pub(crate) fn with_resolved_item_face(mut self, face_id: FaceId, face: ResolvedFace) -> Self {
        self.resolved_item_face = Some(BufferSourceResolvedItemFace::new(face_id, face));
        self
    }

    fn active_face_context<'row>(
        &self,
        geometry: &'row DisplayRowGeometryState,
    ) -> DisplayRowActiveFaceAppendContext<'row, 'source>
    where
        'surface: 'row,
    {
        DisplayRowActiveFaceAppendContext::new(
            self.append_surface,
            geometry,
            self.active_face,
            self.glyph_y_offset,
            self.fallback_metrics,
        )
    }

    fn item_active_face(
        &self,
        geometry: &DisplayRowGeometryState,
    ) -> DisplaySourceItemAppendContext<'source> {
        let frame = self.active_face_context(geometry).active_face_frame();
        DisplaySourceItemAppendContext::with_face_attempt(
            self.active_face.face_id(),
            self.active_face.resolved_face(),
            frame,
            self.face_attempt.clone(),
        )
    }

    fn source_item_face(&self, item: &DisplayItem) -> BufferSourceItemFace<'_> {
        let RenderFaceRef::FaceId(item_face_id) = item.face else {
            return BufferSourceItemFace::Active(self.active_face);
        };
        if let Some(face) = &self.resolved_item_face
            && face.face_id == item_face_id
        {
            return BufferSourceItemFace::Resolved(face);
        }
        BufferSourceItemFace::Active(self.active_face)
    }

    fn source_display_item_for_special_source_char(
        &self,
        request: &DisplaySpecialSourceCharRequest,
        source_item: &DisplayItem,
    ) -> DisplayItem {
        if let Some(source_item) = matching_special_display_item(source_item, request.kind()) {
            return source_item.clone();
        }

        buffer_source_item_append_request(
            request.source_item_request(),
            self.buffer_id,
            self.buffer,
            self.active_face.face_id(),
        )
        .map(DisplaySourceRangeItemAppendRequest::into_item)
        .unwrap_or_else(|| source_item.clone())
    }

    pub(crate) fn prepare_special_source_char_at(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceMeasureState<'_>,
        request: DisplaySpecialSourceCharRequest,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> DisplaySourceSpecialCharPreparedAppend {
        let display_item = self.source_display_item_for_special_source_char(&request, source_item);
        let measured_width_px = request.requires_overflow_measurement().then(|| {
            self.item_active_face(geometry)
                .measure_source_display_item_width_to_text_row(
                    state,
                    &display_item,
                    request.source_item_request(),
                    position,
                )
        });
        request.prepared_append_at(position, measured_width_px, display_item)
    }

    fn append_special_source_char_plan_to_text_row_and_emit(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        plan: DisplaySourceSpecialCharAppendPlan,
    ) -> Option<DisplayRowAppendProgress> {
        let (display_item, position, fallback_kind) = plan.into_append_request();
        self.item_active_face(geometry)
            .append_display_item_to_text_row_and_emit(state, display_item, position, fallback_kind)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_source_render_plan_to_text_row(
        &self,
        geometry: &DisplayRowGeometryState,
        append_state: &mut DisplaySourceRowAppendState,
        measure_state: &mut TextRowSourceMeasureState<'_>,
        source: DisplaySourceRenderPlanRequest<'_>,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> DisplaySourceAppendRenderPlan {
        let frame = self.active_face_context(geometry).active_face_frame();
        append_state.resolve_source_render_plan_to_text_row(
            measure_state,
            self.active_face,
            frame,
            source,
            position,
            source_item,
        )
    }

    fn prepare_source_char_append_plan(
        &self,
        geometry: &DisplayRowGeometryState,
        append_state: &mut DisplaySourceRowAppendState,
        measure_state: &mut TextRowSourceMeasureState<'_>,
        source: DisplaySourceRenderPlanRequest<'_>,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> DisplaySourceTextCharAppendPlan {
        let render_plan = self.resolve_source_render_plan_to_text_row(
            geometry,
            append_state,
            measure_state,
            source,
            position,
            source_item,
        );
        DisplaySourceTextCharAppendPlan::from_render_plan(
            source,
            position,
            source_item,
            render_plan,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_text_source_item_char_at(
        &self,
        geometry: &DisplayRowGeometryState,
        append_state: &mut DisplaySourceRowAppendState,
        measure_state: &mut TextRowSourceMeasureState<'_>,
        source_char: &DisplaySourceTextChar,
        text: &[u8],
        byte_idx: usize,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
        cluster_tail: Option<(char, bool)>,
    ) -> DisplaySourceTextCharPreparedAppend {
        let source = source_char.advance_request(text, byte_idx, cluster_tail);
        DisplaySourceTextCharPreparedAppend::new(self.prepare_source_char_append_plan(
            geometry,
            append_state,
            measure_state,
            source,
            position,
            source_item,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_source_item_char_at(
        &self,
        geometry: &DisplayRowGeometryState,
        append_state: &mut DisplaySourceRowAppendState,
        measure_state: &mut TextRowSourceMeasureState<'_>,
        source_char: &DisplaySourceTextChar,
        text: &[u8],
        byte_idx: usize,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
        cluster_tail: Option<(char, bool)>,
    ) -> DisplaySourcePreparedCharAppend {
        if let Some(request) = source_char.special_request(cluster_tail) {
            return DisplaySourcePreparedCharAppend::Special(self.prepare_special_source_char_at(
                geometry,
                measure_state,
                request,
                position,
                source_item,
            ));
        }
        DisplaySourcePreparedCharAppend::Text(self.prepare_text_source_item_char_at(
            geometry,
            append_state,
            measure_state,
            source_char,
            text,
            byte_idx,
            position,
            source_item,
            cluster_tail,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_source_item_for_current_text_row(
        &self,
        geometry: DisplayRowGeometryState,
        append_state: &mut DisplaySourceRowAppendState,
        source_render: &mut TextRowSourceRenderState<'_>,
        source_char: &DisplaySourceTextChar,
        text: &[u8],
        byte_idx: usize,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> DisplaySourcePreparedCharAppend {
        let mut measure = source_render.measure_state();
        let cluster_tail = measure.current_cluster_tail();
        self.prepare_source_item_char_at(
            &geometry,
            append_state,
            &mut measure,
            source_char,
            text,
            byte_idx,
            position,
            source_item,
            cluster_tail,
        )
    }

    /// Prepare a display-table vector as displayed text regardless of the
    /// source character's ordinary classification. GNU enters
    /// `DISP_CHAR_VECTOR` before its control/tab/nobreak branches and then
    /// classifies the vector's glyphs, not the replaced buffer character.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_display_vector_for_current_text_row(
        &self,
        geometry: DisplayRowGeometryState,
        append_state: &mut DisplaySourceRowAppendState,
        source_render: &mut TextRowSourceRenderState<'_>,
        source_char: &DisplaySourceTextChar,
        text: &[u8],
        byte_idx: usize,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> DisplaySourceTextCharPreparedAppend {
        let mut measure = source_render.measure_state();
        let cluster_tail = measure.current_cluster_tail();
        self.prepare_text_source_item_char_at(
            &geometry,
            append_state,
            &mut measure,
            source_char,
            text,
            byte_idx,
            position,
            source_item,
            cluster_tail,
        )
    }

    #[cfg(test)]
    pub(crate) fn append_source_text_request_to_text_row(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        source_text: DisplaySourceTextRequest,
        position: DisplayRowPosition,
    ) -> Option<DisplayRowAppendProgress> {
        let frame = self.active_face_context(geometry).active_face_frame();
        let face_id = self.active_face.face_id();
        let append_item = source_text.append_request(self.buffer_id, self.buffer, face_id)?;
        let kind = append_item.append_kind();
        let item = append_item.into_item();
        let mut render_policy = source_text.append_render_policy();
        let mut face_ids = self.face_attempt.clone();
        SingleDisplayItemAppendContext::for_source_walk(
            self.active_face.resolved_face(),
            face_id,
            frame,
        )
        .render_with_policy(
            state,
            &mut face_ids,
            item,
            position,
            kind,
            &mut render_policy,
        )
    }

    pub(crate) fn append_source_display_item_to_text_row(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        item: DisplayItem,
        position: DisplayRowPosition,
        fallback_kind: DisplayRowAppendKind,
        render_policy: &mut DisplaySourceAppendRenderPolicy,
    ) -> Option<DisplayRowAppendProgress> {
        let frame = self.active_face_context(geometry).active_face_frame();
        let item_face = self.source_item_face(&item);
        let item = item_face.bind_item(item);
        let mut face_ids = self.face_attempt.clone();
        SingleDisplayItemAppendContext::for_source_walk(
            item_face.resolved_face(),
            item_face.face_id(),
            frame,
        )
        .render_with_policy(
            state,
            &mut face_ids,
            item,
            position,
            fallback_kind,
            render_policy,
        )
    }

    /// Render every item a `DisplayItemSource` yields into the current text
    /// row through the unified item renderer — the same
    /// `SingleDisplayItemAppendContext` seam every other source consumer
    /// (Lisp strings, overlay strings, display strings) uses. The routed
    /// plain-row acquisition path drives this with a
    /// [`crate::buffer_source::row_route::BufferPlainItemSource`]; items are
    /// expected to carry their realized `FaceId` (the active face for a
    /// classified row).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_display_item_source_to_text_row<
        S: crate::display_source::DisplayItemSource,
        P: crate::display_row::render_policy::DisplayRowRenderPolicy,
    >(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        source: &mut S,
        source_state: &mut crate::display_row::source_state::DisplayRowSourceState,
        position: DisplayRowPosition,
        fallback_kind: DisplayRowAppendKind,
        render_policy: &mut P,
    ) -> Option<DisplayRowAppendProgress> {
        let frame = self.active_face_context(geometry).active_face_frame();
        let (face_id, base_face) = self
            .resolved_item_face
            .as_ref()
            .map(|face| (face.face_id, &face.face))
            .unwrap_or_else(|| (self.active_face.face_id(), self.active_face.resolved_face()));
        let mut face_ids = self.face_attempt.clone();
        face_ids.reserve_after(face_id);
        let outcome = SingleDisplayItemAppendContext::for_source_walk(base_face, face_id, frame)
            .render_source_with_policy(
                state,
                &mut face_ids,
                source,
                source_state,
                position,
                fallback_kind,
                render_policy,
                face_id,
            )?;
        Some(outcome.into_append_progress(position))
    }

    pub(crate) fn measure_source_display_item_width_naturally(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceMeasureState<'_>,
        item: &DisplayItem,
        position: DisplayRowPosition,
        fallback_kind: DisplayRowAppendKind,
    ) -> Option<f32> {
        let frame = self.active_face_context(geometry).active_face_frame();
        let item_face = self.source_item_face(item);
        SingleDisplayItemAppendContext::for_source_walk(
            item_face.resolved_face(),
            item_face.face_id(),
            frame,
        )
        .measure_width_naturally(
            state,
            item_face.bind_item(item.clone()),
            position,
            fallback_kind,
        )
    }

    /// Natural measurement returning the END position (x AND col) the append
    /// would reach; see
    /// [`SingleDisplayItemAppendContext::measure_advance_naturally`].
    pub(crate) fn measure_source_display_item_advance_naturally(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceMeasureState<'_>,
        item: &DisplayItem,
        position: DisplayRowPosition,
        fallback_kind: DisplayRowAppendKind,
    ) -> Option<DisplayRowPosition> {
        let frame = self.active_face_context(geometry).active_face_frame();
        let item_face = self.source_item_face(item);
        SingleDisplayItemAppendContext::for_source_walk(
            item_face.resolved_face(),
            item_face.face_id(),
            frame,
        )
        .measure_advance_naturally(
            state,
            item_face.bind_item(item.clone()),
            position,
            fallback_kind,
        )
    }

    fn append_source_char_plan_to_text_row(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        plan: DisplaySourceTextCharAppendPlan,
    ) -> Option<DisplayRowAppendProgress> {
        let (source_item, position, fallback_kind, mut render_policy) = plan.into_render_request();
        self.append_source_display_item_to_text_row(
            geometry,
            state,
            source_item,
            position,
            fallback_kind,
            &mut render_policy,
        )
    }
}

impl<'source, 'surface, B: LayoutBufferView + ?Sized> DisplaySourceCharAppendContext
    for BufferSourceRowAppendContext<'source, 'surface, B>
{
    fn append_source_char_plan_to_text_row(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        plan: DisplaySourceTextCharAppendPlan,
    ) -> Option<DisplayRowAppendProgress> {
        BufferSourceRowAppendContext::append_source_char_plan_to_text_row(
            self, geometry, state, plan,
        )
    }

    fn append_special_source_char_plan_to_text_row_and_emit(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        plan: DisplaySourceSpecialCharAppendPlan,
    ) -> Option<DisplayRowAppendProgress> {
        BufferSourceRowAppendContext::append_special_source_char_plan_to_text_row_and_emit(
            self, geometry, state, plan,
        )
    }
}

fn matching_special_display_item(
    source_item: &DisplayItem,
    kind: DisplaySourceSpecialDisplayKind,
) -> Option<&DisplayItem> {
    match (&source_item.kind, kind) {
        (DisplayItemKind::ControlChar { .. }, DisplaySourceSpecialDisplayKind::Control)
        | (DisplayItemKind::Glyphless(_), DisplaySourceSpecialDisplayKind::Glyphless)
        | (DisplayItemKind::SourceMappedText(_), DisplaySourceSpecialDisplayKind::Nobreak) => {
            Some(source_item)
        }
        _ => None,
    }
}

pub(crate) fn buffer_source_item_append_request<B: LayoutBufferView + ?Sized>(
    source_item: DisplaySourceItemRequest,
    buffer_id: BufferId,
    buffer: &B,
    face_id: FaceId,
) -> Option<DisplaySourceRangeItemAppendRequest> {
    let append_kind = source_item.append_kind();
    let item = source_item.into_display_item(buffer_id, buffer, RenderFaceRef::FaceId(face_id))?;
    Some(DisplaySourceRangeItemAppendRequest::new(item, append_kind))
}
