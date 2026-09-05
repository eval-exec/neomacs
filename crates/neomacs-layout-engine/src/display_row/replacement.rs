use crate::display_cursor::{CapturedCursorInfo, display_property_replacement_cursor_info};
#[cfg(test)]
use crate::display_face_policy::BaseFacePolicy;
use crate::display_item::{
    BufferDisplayReplacementSource, DisplayItem, DisplayItemKind, DisplayLineHeightPolicy,
    DisplayLineSpacingPolicy, DisplayPointerAppearance, DisplayPropertyReplacementDescriptor,
    DisplayStringBoxBoundaries,
};
#[cfg(test)]
use crate::display_origin::DisplayOrigin;
use crate::display_row::append_context::{
    DisplayRowAppendFrame, DisplayRowAppendKind, DisplayRowAppendMetrics,
    DisplayRowAppendPlacement, DisplayRowAppendSurface,
};
use crate::display_row::builder::{
    DisplayRowAppendProgress, DisplayRowItemMeasurement, DisplayRowPosition,
};
use crate::display_row::face_state::{DisplayRowActiveFaceState, DisplayRowExtendFace};
use crate::display_row::geometry::{DisplayRowGeometryState, DisplayRowTextPosition};
#[cfg(test)]
use crate::display_row::lisp_string::LispStringSourceId;
use crate::display_row::metrics::{DisplayRowFallbackMetrics, DisplayRowMeasuredFaceMetrics};
use crate::display_row::render_policy::{DisplayRowRenderClipBehavior, DisplayRowRenderPolicy};
use crate::display_row::render_state::DisplayRowRenderStop;
use crate::display_row::source_append::SingleDisplayItemAppendContext;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_source::{
    BufferDisplayReplacementStringRequest, BufferDisplayReplacementStringSource,
    DisplayItemSegmentSource, DisplayMarginEmission, DisplayNonTextAreaEmission,
    DisplayPropertyReplacementCursorPolicy, DisplayPropertyReplacementSourceItem,
    DisplayReplacementMediaSourceItem, DisplayReplacementMediaSourceResolution,
    DisplayReplacementSourceMappedTextItem, DisplayReplacementStretchSourceItem,
    DisplayReplacementStringSourceItem, LispStringSourceCursor,
};
use crate::display_source_append_plan::NaturalDisplayRowAppendRenderPolicy;
use crate::display_source_resolver::{
    DisplayPropertyReplacementSourceResolveRequest, DisplayStringBaseFace,
};
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{LayoutBufferView, ResolvedFace};
use crate::types::WindowParams;
use neomacs_display_protocol::types::{Color, FaceId};
#[cfg(test)]
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::DisplayHost;

pub(crate) struct DisplayReplacementStringItemMeasurer {
    active_face_state: DisplayRowActiveFaceState,
}

impl DisplayRowRenderPolicy for DisplayReplacementStringItemMeasurer {
    fn measurement_for(
        &mut self,
        item: &DisplayItem,
        _face_id: FaceId,
        font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowItemMeasurement {
        let DisplayItemKind::SourceMappedText(text) = &item.kind else {
            return DisplayRowItemMeasurement::Default;
        };
        DisplayRowItemMeasurement::TextRun(
            self.active_face_state
                .text_run_measurement(font_metrics, text.text.as_ref()),
        )
    }
}

struct DisplayReplacementStringRenderPolicy<'a, M> {
    item_policy: &'a mut M,
    fallback_metrics: DisplayRowMeasuredFaceMetrics,
    /// Set when the display string contains a newline (a `RowBreak` item). GNU
    /// (xdisp.c `display_line`) treats a '\n' inside a `display` string as a
    /// row terminator, exactly like a buffer newline; the caller must emit a
    /// row break so the following buffer text starts on a fresh row.
    produced_row_break: Option<DisplayReplacementStringLineBreak>,
}

impl<M: DisplayRowRenderPolicy> DisplayRowRenderPolicy
    for DisplayReplacementStringRenderPolicy<'_, M>
{
    fn stop_before_item(
        &mut self,
        item: &DisplayItem,
        face_id: FaceId,
        face: &ResolvedFace,
    ) -> bool {
        if let DisplayItemKind::RowBreak(row_break) = item.kind {
            self.produced_row_break = Some(DisplayReplacementStringLineBreak::from_resolved_face(
                face_id,
                face,
                self.fallback_metrics,
                row_break.line_height,
                row_break.line_spacing,
                item.box_vertical_edges,
            ));
            true
        } else {
            false
        }
    }

    fn measurement_for(
        &mut self,
        item: &DisplayItem,
        face_id: FaceId,
        font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowItemMeasurement {
        self.item_policy
            .measurement_for(item, face_id, font_metrics)
    }

    fn clipped_behavior(&mut self, item: &DisplayItem) -> DisplayRowRenderClipBehavior {
        if matches!(item.kind, DisplayItemKind::SourceMappedText(_)) {
            // A display string is a pushed GNU iterator frame, not an atomic
            // row decoration.  Preserve the unrendered suffix so a wrapping
            // caller can resume the same typed source on the next glyph row.
            DisplayRowRenderClipBehavior::PreserveRemainderAndStop
        } else {
            DisplayRowRenderClipBehavior::Continue
        }
    }
}

/// Why one pass of a display-property string session stopped.
///
/// Keeping these cases closed prevents a caller from treating a clipped
/// (therefore resumable) string as exhausted and silently losing its suffix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayReplacementStringRowStop {
    SourceExhausted,
    Clipped,
    RowBreak(DisplayReplacementStringLineBreak),
}

/// Exact semantic newline produced by a pushed display string.
///
/// Buffer position is intentionally absent: the string newline consumes no
/// buffer character.  Its row-break policy, realized face, and affine box-run
/// terminals stay together so the buffer-row transition cannot accidentally
/// substitute the covered buffer character's face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayReplacementStringLineBreak {
    face_id: FaceId,
    metrics: DisplayRowMeasuredFaceMetrics,
    extend_face: Option<DisplayRowExtendFace>,
    line_height: DisplayLineHeightPolicy,
    line_spacing: DisplayLineSpacingPolicy,
    box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    box_run_membership: neomacs_display_protocol::face::BoxRunMembership,
}

impl DisplayReplacementStringLineBreak {
    pub(crate) fn from_resolved_face(
        face_id: FaceId,
        face: &ResolvedFace,
        fallback_metrics: DisplayRowMeasuredFaceMetrics,
        line_height: DisplayLineHeightPolicy,
        line_spacing: DisplayLineSpacingPolicy,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    ) -> Self {
        let char_width = face.measured_char_width_px();
        let metrics = DisplayRowMeasuredFaceMetrics::new(
            if char_width > 0.0 {
                char_width
            } else {
                fallback_metrics.char_width()
            },
            if face.font_line_height > 0.0 {
                face.font_line_height
            } else {
                fallback_metrics.row_height()
            },
            if face.font_ascent > 0.0 {
                face.font_ascent
            } else {
                fallback_metrics.ascent()
            },
            if char_width > 0.0 {
                char_width
            } else {
                fallback_metrics.space_width()
            },
        );
        Self {
            face_id,
            metrics,
            extend_face: face
                .extend
                .then(|| DisplayRowExtendFace::new(Color::from_pixel(face.bg), face_id, metrics)),
            line_height,
            line_spacing,
            box_vertical_edges,
            box_run_membership: neomacs_display_protocol::face::BoxRunMembership::from_boxed(
                face.box_type > 0,
            ),
        }
    }

    pub(crate) fn face_id(self) -> FaceId {
        self.face_id
    }

    pub(crate) fn metrics(self) -> DisplayRowMeasuredFaceMetrics {
        self.metrics
    }

    pub(crate) fn extend_face(self) -> Option<DisplayRowExtendFace> {
        self.extend_face
    }

    pub(crate) fn line_height(self) -> DisplayLineHeightPolicy {
        self.line_height
    }

    pub(crate) fn line_spacing(self, inherited: f32) -> f32 {
        if self.line_height == DisplayLineHeightPolicy::ContentOnly {
            0.0
        } else {
            self.line_spacing
                .resolve(self.metrics.row_height(), inherited)
        }
    }

    pub(crate) fn box_vertical_edges(self) -> neomacs_display_protocol::face::BoxVerticalEdges {
        self.box_vertical_edges
    }

    pub(crate) fn box_run_membership(self) -> neomacs_display_protocol::face::BoxRunMembership {
        self.box_run_membership
    }
}

pub(crate) struct DisplayReplacementStringRowOutcome {
    stop: DisplayReplacementStringRowStop,
    end_position: DisplayRowPosition,
}

impl DisplayReplacementStringRowOutcome {
    pub(crate) fn stop(&self) -> DisplayReplacementStringRowStop {
        self.stop
    }

    pub(crate) fn end_position(&self) -> DisplayRowPosition {
        self.end_position
    }
}

/// A pushed display-string iterator that can outlive one glyph row.
///
/// GNU stores this state in `struct it` across `display_line` calls.  The
/// source cursor and its pending clipped item are deliberately owned together
/// here, so Rust makes it impossible to resume one without the other.
pub(crate) struct DisplayReplacementStringRowSession {
    source: BufferDisplayReplacementStringSource<LispStringSourceCursor>,
    source_state: DisplayRowSourceState,
    base_face: DisplayStringBaseFace,
    item_policy: DisplayReplacementStringItemMeasurer,
}

impl DisplayReplacementStringRowSession {
    fn new(
        request: DisplayReplacementStringAppendRequest,
        replacement_source: BufferDisplayReplacementSource,
        pointer_appearance: Option<DisplayPointerAppearance>,
        box_boundaries: DisplayStringBoxBoundaries,
    ) -> Option<Self> {
        let face_scope = request.face_scope;
        let source = request
            .source_request(replacement_source, pointer_appearance)
            .with_box_boundaries(box_boundaries)
            .into_source(request.replacement_base_face.as_ref()?.face_id())?;
        let item_policy = request.string_item_measurer();
        let base_face = request.replacement_base_face?;
        Some(Self {
            source,
            source_state: DisplayRowSourceState::with_face_scope(face_scope),
            base_face,
            item_policy,
        })
    }

    pub(crate) fn render_to_text_row_and_emit(
        &mut self,
        replacement_append_context: DisplayReplacementRowAppendContext<'_>,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        row_geometry: &mut DisplayRowGeometryState,
        position: DisplayRowPosition,
    ) -> Option<DisplayReplacementStringRowOutcome> {
        let append_context = DisplayReplacementAppendContext::new(
            self.base_face.face_id(),
            self.base_face.face(),
            replacement_append_context.active_face_frame(),
        );
        let mut render_policy = DisplayReplacementStringRenderPolicy {
            item_policy: &mut self.item_policy,
            fallback_metrics: replacement_append_context.active_face.metrics(),
            produced_row_break: None,
        };
        let outcome = append_context.single_item.render_source_with_policy(
            state,
            face_ids,
            &mut self.source,
            &mut self.source_state,
            position,
            DisplayRowAppendKind::DisplayReplacementString,
            &mut render_policy,
            self.base_face.face_id(),
        )?;
        let stop = if let Some(line_break) = render_policy.produced_row_break {
            match line_break.line_height() {
                DisplayLineHeightPolicy::Default => {
                    outcome.include_vertical_metrics(row_geometry);
                    let metrics = line_break.metrics();
                    row_geometry
                        .include_glyph_vertical_metrics(metrics.row_height(), metrics.ascent());
                }
                DisplayLineHeightPolicy::ContentOnly => {
                    let metrics = state.current_row_visible_content_metrics(
                        face_ids,
                        crate::display_row::metrics::DisplayRowFallbackMetrics::from_measured_face(
                            replacement_append_context.active_face.metrics(),
                        ),
                    );
                    row_geometry
                        .replace_current_row_metrics(metrics.height_px(), metrics.ascent_px());
                }
            }
            DisplayReplacementStringRowStop::RowBreak(line_break)
        } else {
            outcome.include_vertical_metrics(row_geometry);
            match outcome.stop() {
                DisplayRowRenderStop::SourceExhausted => {
                    DisplayReplacementStringRowStop::SourceExhausted
                }
                DisplayRowRenderStop::Clipped => DisplayReplacementStringRowStop::Clipped,
                DisplayRowRenderStop::RowBreak(_) => unreachable!(
                    "replacement-string policy stops before a row-break item and captures its box terminals"
                ),
            }
        };
        Some(DisplayReplacementStringRowOutcome {
            stop,
            end_position: outcome.end_position(),
        })
    }
}

#[derive(Clone)]
struct DisplayReplacementStringAppendRequest {
    item: DisplayReplacementStringSourceItem,
    face_scope: crate::display_source_resolver::DisplaySourceFaceScope,
    replacement_base_face: Option<DisplayStringBaseFace>,
    active_face_state: DisplayRowActiveFaceState,
}

#[cfg(test)]
pub(crate) struct DisplayReplacementStringSourceSnapshot {
    value: Value,
    source_id: LispStringSourceId,
    position: DisplayRowPosition,
    origin: DisplayOrigin,
    base_face_policy: BaseFacePolicy,
    cursor_slot_width_px: f32,
    is_empty: bool,
}

#[cfg(test)]
pub(crate) struct DisplayPropertyReplacementStringPlanSnapshot {
    origin: DisplayOrigin,
    base_face_policy: BaseFacePolicy,
    has_replacement_base_face: bool,
}

#[cfg(test)]
impl DisplayReplacementStringSourceSnapshot {
    pub(crate) fn value(&self) -> Value {
        self.value
    }

    pub(crate) fn source_id(&self) -> LispStringSourceId {
        self.source_id
    }

    pub(crate) fn position(&self) -> DisplayRowPosition {
        self.position
    }

    pub(crate) fn origin(&self) -> DisplayOrigin {
        self.origin
    }

    pub(crate) fn base_face_policy(&self) -> BaseFacePolicy {
        self.base_face_policy
    }

    pub(crate) fn cursor_slot_width_px(&self) -> f32 {
        self.cursor_slot_width_px
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.is_empty
    }
}

#[cfg(test)]
impl DisplayPropertyReplacementStringPlanSnapshot {
    pub(crate) fn origin(&self) -> DisplayOrigin {
        self.origin
    }

    pub(crate) fn base_face_policy(&self) -> BaseFacePolicy {
        self.base_face_policy
    }

    pub(crate) fn has_replacement_base_face(&self) -> bool {
        self.has_replacement_base_face
    }
}

impl DisplayReplacementStringAppendRequest {
    fn new(
        buffer: &impl LayoutBufferView,
        item: DisplayReplacementStringSourceItem,
        replacement_base_face: Option<DisplayStringBaseFace>,
        active_face_state: DisplayRowActiveFaceState,
    ) -> Self {
        Self {
            item,
            face_scope: crate::display_source_resolver::DisplaySourceFaceScope::for_buffer(buffer),
            replacement_base_face,
            active_face_state,
        }
    }

    fn string_item_measurer(&self) -> DisplayReplacementStringItemMeasurer {
        DisplayReplacementStringItemMeasurer {
            active_face_state: self.active_face_state.clone(),
        }
    }

    #[cfg(test)]
    fn plan_snapshot(&self) -> DisplayPropertyReplacementStringPlanSnapshot {
        DisplayPropertyReplacementStringPlanSnapshot {
            origin: self.item.origin(),
            base_face_policy: self.item.base_face_policy(),
            has_replacement_base_face: self.replacement_base_face.is_some(),
        }
    }

    fn source_request(
        &self,
        replacement_source: BufferDisplayReplacementSource,
        pointer_appearance: Option<DisplayPointerAppearance>,
    ) -> BufferDisplayReplacementStringRequest {
        BufferDisplayReplacementStringRequest::new(
            self.item.source_id(),
            self.item.value(),
            replacement_source,
        )
        .with_pointer_appearance(pointer_appearance)
    }
}

#[cfg(test)]
impl DisplayReplacementStringSourceItem {
    pub(crate) fn append_source_snapshot(
        &self,
        position: DisplayRowPosition,
    ) -> DisplayReplacementStringSourceSnapshot {
        DisplayReplacementStringSourceSnapshot {
            value: self.value(),
            source_id: LispStringSourceId::display_replacement(self.source_id()),
            position,
            origin: self.origin(),
            base_face_policy: self.base_face_policy(),
            cursor_slot_width_px: self.cursor_slot_width_px(),
            is_empty: self.is_empty(),
        }
    }

    pub(crate) fn measurement_from_active_face(
        &self,
        active_face_state: &DisplayRowActiveFaceState,
        item: &DisplayItem,
        face_id: FaceId,
        font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowItemMeasurement {
        let mut measurer = DisplayReplacementStringItemMeasurer {
            active_face_state: active_face_state.clone(),
        };
        DisplayRowRenderPolicy::measurement_for(&mut measurer, item, face_id, font_metrics)
    }
}

#[derive(Clone, Debug)]
struct DisplayReplacementItemAppendRequest {
    kind: DisplayItemKind,
    frame: DisplayReplacementItemAppendFrame,
    position: DisplayRowPosition,
    pointer_appearance: Option<DisplayPointerAppearance>,
    box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
}

#[derive(Clone, Debug)]
struct DisplayReplacementItemAppendPlan {
    item: DisplayItem,
    frame: DisplayReplacementItemAppendFrame,
    position: DisplayRowPosition,
}

#[derive(Clone, Debug)]
struct DisplayReplacementItemAppendTemplate {
    kind: DisplayItemKind,
    frame: DisplayReplacementItemAppendFrame,
    row_geometry_update: DisplayReplacementItemRowGeometryUpdate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DisplayReplacementItemAppendFrame {
    ActiveFace,
    DisplayBox { height_px: f32, ascent_px: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DisplayReplacementItemRowGeometryUpdate {
    None,
    BeforeAppendGlyphMetrics { height_px: f32, ascent_px: f32 },
    AfterCompleteRowExtents { height_px: f32, ascent_px: f32 },
}

impl DisplayReplacementItemAppendRequest {
    #[cfg(test)]
    fn active_face(kind: DisplayItemKind, position: DisplayRowPosition) -> Self {
        Self {
            kind,
            frame: DisplayReplacementItemAppendFrame::ActiveFace,
            position,
            pointer_appearance: None,
            box_vertical_edges: Default::default(),
        }
    }

    #[cfg(test)]
    fn display_box(
        kind: DisplayItemKind,
        height_px: f32,
        ascent_px: f32,
        position: DisplayRowPosition,
    ) -> Self {
        Self {
            kind,
            frame: DisplayReplacementItemAppendFrame::DisplayBox {
                height_px,
                ascent_px,
            },
            position,
            pointer_appearance: None,
            box_vertical_edges: Default::default(),
        }
    }

    fn into_plan(
        self,
        replacement_source: BufferDisplayReplacementSource,
        face_id: FaceId,
    ) -> DisplayReplacementItemAppendPlan {
        DisplayReplacementItemAppendPlan {
            item: replacement_source
                .display_item(face_id, self.kind)
                .with_box_vertical_edges(self.box_vertical_edges)
                .with_pointer_appearance(self.pointer_appearance),
            frame: self.frame,
            position: self.position,
        }
    }
}

impl DisplayReplacementItemAppendPlan {
    fn frame(&self) -> DisplayReplacementItemAppendFrame {
        self.frame
    }

    fn into_parts(self) -> (DisplayItem, DisplayRowPosition) {
        (self.item, self.position)
    }
}

impl DisplayReplacementItemAppendTemplate {
    fn active_face(
        kind: DisplayItemKind,
        row_geometry_update: DisplayReplacementItemRowGeometryUpdate,
    ) -> Self {
        Self {
            kind,
            frame: DisplayReplacementItemAppendFrame::ActiveFace,
            row_geometry_update,
        }
    }

    fn display_box(
        kind: DisplayItemKind,
        height_px: f32,
        ascent_px: f32,
        row_geometry_update: DisplayReplacementItemRowGeometryUpdate,
    ) -> Self {
        Self {
            kind,
            frame: DisplayReplacementItemAppendFrame::DisplayBox {
                height_px,
                ascent_px,
            },
            row_geometry_update,
        }
    }

    fn from_stretch(item: DisplayReplacementStretchSourceItem) -> Option<Self> {
        (item.width_px() > 0.0).then(|| {
            Self::active_face(
                item.display_item_kind(),
                DisplayReplacementItemRowGeometryUpdate::BeforeAppendGlyphMetrics {
                    height_px: item.height_px(),
                    ascent_px: item.ascent_px(),
                },
            )
        })
    }

    fn from_media_resolution(item: DisplayReplacementMediaSourceResolution) -> Self {
        match item {
            DisplayReplacementMediaSourceResolution::Media(media_item) => Self::display_box(
                DisplayItemKind::MediaReplacement(media_item.media()),
                media_item.display_height_px(),
                media_item.display_ascent_px(),
                DisplayReplacementItemRowGeometryUpdate::AfterCompleteRowExtents {
                    height_px: media_item.display_height_px(),
                    ascent_px: media_item.display_ascent_px(),
                },
            ),
            DisplayReplacementMediaSourceResolution::Placeholder(placeholder_item) => {
                Self::active_face(
                    DisplayItemKind::SourceMappedText(
                        crate::display_item::DisplaySourceMappedText::new(
                            placeholder_item.into_text(),
                        ),
                    ),
                    DisplayReplacementItemRowGeometryUpdate::None,
                )
            }
        }
    }

    fn into_request(
        self,
        position: DisplayRowPosition,
        pointer_appearance: Option<DisplayPointerAppearance>,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    ) -> DisplayReplacementItemAppendRequest {
        DisplayReplacementItemAppendRequest {
            kind: self.kind,
            frame: self.frame,
            position,
            pointer_appearance,
            box_vertical_edges,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn append_to_text_row(
        self,
        replacement_append_context: DisplayReplacementRowAppendContext<'_>,
        row_geometry: &mut DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        position: DisplayRowPosition,
        pointer_appearance: Option<DisplayPointerAppearance>,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    ) -> DisplayRowPosition {
        let geometry_update = self.row_geometry_update;
        if let DisplayReplacementItemRowGeometryUpdate::BeforeAppendGlyphMetrics {
            height_px,
            ascent_px,
        } = geometry_update
        {
            row_geometry.include_glyph_vertical_metrics(height_px, ascent_px);
        }
        let Some(progress) = replacement_append_context.append_item_request_to_text_row_and_emit(
            state,
            face_ids,
            self.into_request(position, pointer_appearance, box_vertical_edges),
        ) else {
            return position;
        };
        if let DisplayReplacementItemRowGeometryUpdate::AfterCompleteRowExtents {
            height_px,
            ascent_px,
        } = geometry_update
            && progress.is_complete_with_positive_width()
        {
            row_geometry.include_row_extents(height_px, ascent_px);
        }
        progress.end()
    }
}

#[derive(Clone)]
struct DisplayPropertyReplacementAppendRequest {
    replacement_source: BufferDisplayReplacementSource,
    item: DisplayPropertyReplacementSourceItem,
    glyph_y_offset: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
    start_position: DisplayRowPosition,
    pointer_appearance: Option<DisplayPointerAppearance>,
    box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    box_boundaries: DisplayStringBoxBoundaries,
}

impl DisplayPropertyReplacementAppendRequest {
    #[allow(clippy::too_many_arguments)]
    fn new(
        replacement_source: BufferDisplayReplacementSource,
        item: DisplayPropertyReplacementSourceItem,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        start_position: DisplayRowPosition,
        pointer_appearance: Option<DisplayPointerAppearance>,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
        box_boundaries: DisplayStringBoxBoundaries,
    ) -> Self {
        Self {
            replacement_source,
            item,
            glyph_y_offset,
            fallback_metrics,
            start_position,
            pointer_appearance,
            box_vertical_edges,
            box_boundaries,
        }
    }

    fn cursor_policy(&self) -> DisplayPropertyReplacementCursorPolicy {
        self.item.cursor_policy()
    }

    #[allow(clippy::too_many_arguments)]
    fn from_typed_replacement_descriptor(
        descriptor: &DisplayPropertyReplacementDescriptor,
        source_text: &[u8],
        active_face_state: &DisplayRowActiveFaceState,
        font_metrics: &mut Option<FontMetricsService>,
        current_x: f32,
        content_x: f32,
        params: &WindowParams,
        display_host: Option<&dyn DisplayHost>,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        start_position: DisplayRowPosition,
    ) -> Option<Self> {
        let item = DisplayPropertyReplacementSourceResolveRequest::from_typed_replacement(
            descriptor.classification(),
            descriptor.anchor_charpos(),
            source_text,
            active_face_state,
            font_metrics,
            current_x,
            content_x,
            params,
            display_host,
        )
        .resolve()?;
        Some(Self::new(
            descriptor.replacement_source(),
            item,
            glyph_y_offset,
            fallback_metrics,
            start_position,
            descriptor.pointer_appearance().cloned(),
            descriptor.box_vertical_edges(),
            descriptor.box_boundaries(),
        ))
    }

    fn into_plan<B: LayoutBufferView>(
        self,
        buffer: &B,
        state: &mut TextRowSourceRenderState<'_>,
        active_face_state: &DisplayRowActiveFaceState,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayPropertyReplacementAppendPlan {
        let item = DisplayPropertyReplacementAppendPlanItemRequest::new(self.item).resolve(
            buffer,
            state,
            active_face_state,
            face_ids,
        );
        DisplayPropertyReplacementAppendPlan {
            replacement_source: self.replacement_source,
            item,
            glyph_y_offset: self.glyph_y_offset,
            fallback_metrics: self.fallback_metrics,
            start_position: self.start_position,
            pointer_appearance: self.pointer_appearance,
            box_vertical_edges: self.box_vertical_edges,
            box_boundaries: self.box_boundaries,
        }
    }
}

#[derive(Clone)]
pub(crate) struct DisplayPropertyReplacementRowRenderRequest {
    append_request: DisplayPropertyReplacementAppendRequest,
}

impl DisplayPropertyReplacementRowRenderRequest {
    #[cfg(test)]
    pub(crate) fn from_resolved_source_item(
        replacement_source: BufferDisplayReplacementSource,
        item: DisplayPropertyReplacementSourceItem,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        start_position: DisplayRowPosition,
    ) -> Self {
        Self {
            append_request: DisplayPropertyReplacementAppendRequest::new(
                replacement_source,
                item,
                glyph_y_offset,
                fallback_metrics,
                start_position,
                None,
                Default::default(),
                Default::default(),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_typed_replacement_descriptor(
        descriptor: &DisplayPropertyReplacementDescriptor,
        source_text: &[u8],
        active_face_state: &DisplayRowActiveFaceState,
        font_metrics: &mut Option<FontMetricsService>,
        current_x: f32,
        content_x: f32,
        params: &WindowParams,
        display_host: Option<&dyn DisplayHost>,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        start_position: DisplayRowPosition,
    ) -> Option<Self> {
        DisplayPropertyReplacementAppendRequest::from_typed_replacement_descriptor(
            descriptor,
            source_text,
            active_face_state,
            font_metrics,
            current_x,
            content_x,
            params,
            display_host,
            glyph_y_offset,
            fallback_metrics,
            start_position,
        )
        .map(|append_request| Self { append_request })
    }

    #[cfg(test)]
    pub(crate) fn cursor_policy(&self) -> DisplayPropertyReplacementCursorPolicy {
        self.append_request.cursor_policy()
    }

    /// The resolved stretch width of a `(space …)` replacement, for the
    /// routed acquisition's fit probe (increment 2i rung 3): the exact pixel
    /// width the session's append will advance by. `None` for non-stretch
    /// replacement kinds.
    pub(crate) fn stretch_width_px(&self) -> Option<f32> {
        match &self.append_request.item {
            DisplayPropertyReplacementSourceItem::Stretch(item) => Some(item.width_px()),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn start_position(&self) -> DisplayRowPosition {
        self.append_request.start_position
    }

    #[cfg(test)]
    pub(crate) fn into_item(self) -> DisplayPropertyReplacementSourceItem {
        self.append_request.item
    }

    #[cfg(test)]
    pub(crate) fn string_plan_snapshot<B: LayoutBufferView>(
        self,
        buffer: &B,
        state: &mut TextRowSourceRenderState<'_>,
        active_face_state: &DisplayRowActiveFaceState,
        face_ids: &mut FrameFaceAttempt,
    ) -> Option<DisplayPropertyReplacementStringPlanSnapshot> {
        self.append_request
            .into_plan(buffer, state, active_face_state, face_ids)
            .string_plan_snapshot()
    }

    pub(crate) fn begin_render_to_text_rows<B: LayoutBufferView>(
        self,
        buffer: &B,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        append_surface: &DisplayRowAppendSurface,
        row_geometry: &mut DisplayRowGeometryState,
        active_face_state: &DisplayRowActiveFaceState,
    ) -> DisplayPropertyReplacementRowRender {
        let cursor_policy = self.append_request.cursor_policy();
        self.append_request
            .into_plan(buffer, state, active_face_state, face_ids)
            .begin_render_to_text_rows(
                state,
                face_ids,
                append_surface,
                row_geometry,
                active_face_state,
                cursor_policy,
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayPropertyReplacementAppendOutcome {
    start_position: DisplayRowPosition,
    end_position: DisplayRowPosition,
    cursor_policy: DisplayPropertyReplacementCursorPolicy,
}

impl DisplayPropertyReplacementAppendOutcome {
    pub(crate) fn new(
        start_position: DisplayRowPosition,
        end_position: DisplayRowPosition,
        cursor_policy: DisplayPropertyReplacementCursorPolicy,
    ) -> Self {
        Self {
            start_position,
            end_position,
            cursor_policy,
        }
    }

    pub(crate) fn start_position(self) -> DisplayRowPosition {
        self.start_position
    }

    pub(crate) fn end_position(self) -> DisplayRowPosition {
        self.end_position
    }

    pub(crate) fn cursor_info(
        self,
        active_face_state: &DisplayRowActiveFaceState,
        position: DisplayRowTextPosition,
        preceding_charpos: Option<i64>,
    ) -> CapturedCursorInfo {
        display_property_replacement_cursor_info(
            self.cursor_policy,
            active_face_state,
            position,
            preceding_charpos,
        )
    }
}

/// A replacement either completed atomically on the current row or opened a
/// stateful string iterator.  Callers must handle both variants explicitly;
/// there is no lossy "append and forget a clipped suffix" representation.
pub(crate) enum DisplayPropertyReplacementRowRender {
    Applied(DisplayPropertyReplacementAppendOutcome),
    String(DisplayPropertyReplacementStringRender),
}

pub(crate) struct DisplayPropertyReplacementStringRender {
    session: DisplayReplacementStringRowSession,
    replacement_source: BufferDisplayReplacementSource,
    glyph_y_offset: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
    start_position: DisplayRowPosition,
    cursor_policy: DisplayPropertyReplacementCursorPolicy,
}

impl DisplayPropertyReplacementStringRender {
    pub(crate) fn opening_slot(&self) -> DisplayPropertyReplacementAppendOutcome {
        DisplayPropertyReplacementAppendOutcome::new(
            self.start_position,
            self.start_position,
            self.cursor_policy,
        )
    }

    pub(crate) fn render_next_row(
        &mut self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        append_surface: &DisplayRowAppendSurface,
        row_geometry: &mut DisplayRowGeometryState,
        active_face_state: &DisplayRowActiveFaceState,
        position: DisplayRowPosition,
    ) -> Option<DisplayReplacementStringRowOutcome> {
        let append_context = DisplayReplacementRowAppendContext::new(
            self.replacement_source,
            append_surface,
            row_geometry,
            active_face_state,
            self.glyph_y_offset,
            self.fallback_metrics,
        );
        self.session.render_to_text_row_and_emit(
            append_context,
            state,
            face_ids,
            row_geometry,
            position,
        )
    }

    pub(crate) fn finish(
        self,
        end_position: DisplayRowPosition,
    ) -> DisplayPropertyReplacementAppendOutcome {
        DisplayPropertyReplacementAppendOutcome::new(
            self.start_position,
            end_position,
            self.cursor_policy,
        )
    }
}

struct DisplayPropertyReplacementAppendPlan {
    replacement_source: BufferDisplayReplacementSource,
    item: DisplayPropertyReplacementAppendPlanItem,
    glyph_y_offset: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
    start_position: DisplayRowPosition,
    pointer_appearance: Option<DisplayPointerAppearance>,
    box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    box_boundaries: DisplayStringBoxBoundaries,
}

impl DisplayPropertyReplacementAppendPlan {
    #[cfg(test)]
    fn string_plan_snapshot(&self) -> Option<DisplayPropertyReplacementStringPlanSnapshot> {
        match &self.item {
            DisplayPropertyReplacementAppendPlanItem::String(request) => {
                Some(request.plan_snapshot())
            }
            _ => None,
        }
    }

    fn begin_render_to_text_rows(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        append_surface: &DisplayRowAppendSurface,
        row_geometry: &mut DisplayRowGeometryState,
        active_face_state: &DisplayRowActiveFaceState,
        cursor_policy: DisplayPropertyReplacementCursorPolicy,
    ) -> DisplayPropertyReplacementRowRender {
        let Self {
            replacement_source,
            item,
            glyph_y_offset,
            fallback_metrics,
            start_position,
            pointer_appearance,
            box_vertical_edges,
            box_boundaries,
        } = self;
        match item {
            DisplayPropertyReplacementAppendPlanItem::String(request) => {
                let Some(session) = DisplayReplacementStringRowSession::new(
                    request,
                    replacement_source,
                    pointer_appearance,
                    box_boundaries,
                ) else {
                    return DisplayPropertyReplacementRowRender::Applied(
                        DisplayPropertyReplacementAppendOutcome::new(
                            start_position,
                            start_position,
                            cursor_policy,
                        ),
                    );
                };
                DisplayPropertyReplacementRowRender::String(
                    DisplayPropertyReplacementStringRender {
                        session,
                        replacement_source,
                        glyph_y_offset,
                        fallback_metrics,
                        start_position,
                        cursor_policy,
                    },
                )
            }
            DisplayPropertyReplacementAppendPlanItem::Atomic(item) => {
                let append_context = DisplayReplacementRowAppendContext::new(
                    replacement_source,
                    append_surface,
                    row_geometry,
                    active_face_state,
                    glyph_y_offset,
                    fallback_metrics,
                );
                let end_position = item.append_to_text_row(
                    append_context,
                    row_geometry,
                    state,
                    face_ids,
                    start_position,
                    pointer_appearance,
                    box_vertical_edges,
                );
                DisplayPropertyReplacementRowRender::Applied(
                    DisplayPropertyReplacementAppendOutcome::new(
                        start_position,
                        end_position,
                        cursor_policy,
                    ),
                )
            }
        }
    }
}

#[derive(Clone)]
// Large payload variant; boxing is a perf hint deferred out of the lint gate.
#[allow(clippy::large_enum_variant)]
enum DisplayPropertyReplacementAppendPlanItem {
    String(DisplayReplacementStringAppendRequest),
    Atomic(DisplayPropertyReplacementAtomicAppendPlanItem),
}

#[derive(Clone)]
enum DisplayPropertyReplacementAtomicAppendPlanItem {
    Empty,
    Margin {
        emission: DisplayMarginEmission,
        face_scope: crate::display_source_resolver::DisplaySourceFaceScope,
    },
    Item(DisplayReplacementItemAppendTemplate),
}

struct DisplayPropertyReplacementAppendPlanItemRequest {
    item: DisplayPropertyReplacementSourceItem,
}

impl DisplayPropertyReplacementAppendPlanItemRequest {
    fn new(item: DisplayPropertyReplacementSourceItem) -> Self {
        Self { item }
    }

    fn resolve<B: LayoutBufferView>(
        self,
        buffer: &B,
        state: &mut TextRowSourceRenderState<'_>,
        active_face_state: &DisplayRowActiveFaceState,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayPropertyReplacementAppendPlanItem {
        match self.item {
            DisplayPropertyReplacementSourceItem::Empty => {
                DisplayPropertyReplacementAppendPlanItem::Atomic(
                    DisplayPropertyReplacementAtomicAppendPlanItem::Empty,
                )
            }
            DisplayPropertyReplacementSourceItem::Margin(emission) => {
                DisplayPropertyReplacementAppendPlanItem::Atomic(
                    DisplayPropertyReplacementAtomicAppendPlanItem::Margin {
                        emission,
                        face_scope:
                            crate::display_source_resolver::DisplaySourceFaceScope::for_buffer(
                                buffer,
                            ),
                    },
                )
            }
            DisplayPropertyReplacementSourceItem::String(item) => {
                let replacement_base_face = (!item.is_empty()).then(|| {
                    state.default_display_string_base_face_for_active_row(
                        buffer,
                        item.origin(),
                        active_face_state,
                        face_ids,
                    )
                });
                DisplayPropertyReplacementAppendPlanItem::String(
                    DisplayReplacementStringAppendRequest::new(
                        buffer,
                        item,
                        replacement_base_face,
                        active_face_state.clone(),
                    ),
                )
            }
            DisplayPropertyReplacementSourceItem::Stretch(item) => {
                let item = DisplayReplacementItemAppendTemplate::from_stretch(item)
                    .map(DisplayPropertyReplacementAtomicAppendPlanItem::Item)
                    .unwrap_or(DisplayPropertyReplacementAtomicAppendPlanItem::Empty);
                DisplayPropertyReplacementAppendPlanItem::Atomic(item)
            }
            DisplayPropertyReplacementSourceItem::Media(item) => {
                DisplayPropertyReplacementAppendPlanItem::Atomic(
                    DisplayPropertyReplacementAtomicAppendPlanItem::Item(
                        DisplayReplacementItemAppendTemplate::from_media_resolution(item),
                    ),
                )
            }
        }
    }
}

impl DisplayPropertyReplacementAtomicAppendPlanItem {
    #[allow(clippy::too_many_arguments)]
    fn append_to_text_row(
        self,
        replacement_append_context: DisplayReplacementRowAppendContext<'_>,
        row_geometry: &mut DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        position: DisplayRowPosition,
        pointer_appearance: Option<DisplayPointerAppearance>,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    ) -> DisplayRowPosition {
        match self {
            Self::Empty => position,
            Self::Margin {
                emission,
                face_scope,
            } => {
                let face_id = replacement_append_context.active_face.face_id();
                let frame = replacement_append_context.active_face_frame();
                state.render_non_text_area_emission(
                    DisplayNonTextAreaEmission::Margin(emission),
                    face_scope,
                    &frame,
                    face_ids,
                    face_id,
                    if position.col() == 0 {
                        crate::display_row::source_render::DisplayStructuralAreaOrder::BeforeExisting
                    } else {
                        crate::display_row::source_render::DisplayStructuralAreaOrder::AfterExisting
                    },
                );
                position
            }
            Self::Item(item) => item.append_to_text_row(
                replacement_append_context,
                row_geometry,
                state,
                face_ids,
                position,
                pointer_appearance,
                box_vertical_edges,
            ),
        }
    }
}

impl DisplayReplacementMediaSourceItem {
    #[cfg(test)]
    pub(crate) fn row_extents_after_append(
        self,
        progress: &DisplayRowAppendProgress,
    ) -> Option<(f32, f32)> {
        if progress.is_complete_with_positive_width() {
            Some((self.display_height_px(), self.display_ascent_px()))
        } else {
            None
        }
    }

    #[cfg(test)]
    fn append_request(self, position: DisplayRowPosition) -> DisplayReplacementItemAppendRequest {
        DisplayReplacementItemAppendRequest::display_box(
            DisplayItemKind::MediaReplacement(self.media()),
            self.display_height_px(),
            self.display_ascent_px(),
            position,
        )
    }
}

impl DisplayReplacementSourceMappedTextItem {
    #[cfg(test)]
    fn append_request(self, position: DisplayRowPosition) -> DisplayReplacementItemAppendRequest {
        DisplayReplacementItemAppendRequest::active_face(
            DisplayItemKind::SourceMappedText(crate::display_item::DisplaySourceMappedText::new(
                self.into_text(),
            )),
            position,
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DisplayReplacementRowAppendContext<'a> {
    replacement_source: BufferDisplayReplacementSource,
    append_surface: &'a DisplayRowAppendSurface,
    placement: DisplayRowAppendPlacement,
    active_face: &'a DisplayRowActiveFaceState,
    fallback_metrics: DisplayRowFallbackMetrics,
}

impl<'a> DisplayReplacementRowAppendContext<'a> {
    pub(crate) fn new(
        replacement_source: BufferDisplayReplacementSource,
        append_surface: &'a DisplayRowAppendSurface,
        geometry: &DisplayRowGeometryState,
        active_face: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            replacement_source,
            append_surface,
            placement: DisplayRowAppendPlacement::from_geometry_state(geometry, glyph_y_offset),
            active_face,
            fallback_metrics,
        }
    }

    fn active_face_frame(self) -> DisplayRowAppendFrame {
        self.append_surface.frame(
            self.placement,
            DisplayRowAppendMetrics::from_active_face_state(
                self.active_face,
                self.fallback_metrics,
            ),
        )
    }

    fn display_box_frame(self, height_px: f32, ascent_px: f32) -> DisplayRowAppendFrame {
        self.append_surface.frame(
            self.placement,
            DisplayRowAppendMetrics::display_box_from_active_face_state(
                self.active_face,
                height_px,
                ascent_px,
                self.fallback_metrics,
            ),
        )
    }

    fn active_face(
        self,
        face_id: FaceId,
        base_face: &'a ResolvedFace,
    ) -> DisplayReplacementAppendContext<'a> {
        DisplayReplacementAppendContext::new(face_id, base_face, self.active_face_frame())
    }

    fn display_box(
        self,
        face_id: FaceId,
        base_face: &'a ResolvedFace,
        height_px: f32,
        ascent_px: f32,
    ) -> DisplayReplacementAppendContext<'a> {
        DisplayReplacementAppendContext::new(
            face_id,
            base_face,
            self.display_box_frame(height_px, ascent_px),
        )
    }

    fn append_item_request_to_text_row_and_emit(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        request: DisplayReplacementItemAppendRequest,
    ) -> Option<DisplayRowAppendProgress> {
        let plan = request.into_plan(self.replacement_source, self.active_face.face_id());
        let append_context = match plan.frame() {
            DisplayReplacementItemAppendFrame::ActiveFace => {
                self.active_face(self.active_face.face_id(), self.active_face.resolved_face())
            }
            DisplayReplacementItemAppendFrame::DisplayBox {
                height_px,
                ascent_px,
            } => self.display_box(
                self.active_face.face_id(),
                self.active_face.resolved_face(),
                height_px,
                ascent_px,
            ),
        };
        append_context.append_replacement_item_plan_to_text_row_and_emit(state, face_ids, plan)
    }

    #[cfg(test)]
    pub(crate) fn append_stretch_source_item_to_text_row_and_emit(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        item: DisplayReplacementStretchSourceItem,
        position: DisplayRowPosition,
    ) -> Option<DisplayRowAppendProgress> {
        let request = DisplayReplacementItemAppendTemplate::from_stretch(item)?.into_request(
            position,
            None,
            Default::default(),
        );
        self.append_item_request_to_text_row_and_emit(state, face_ids, request)
    }

    #[cfg(test)]
    pub(crate) fn append_media_source_item_to_text_row_and_emit(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        item: DisplayReplacementMediaSourceItem,
        position: DisplayRowPosition,
    ) -> Option<DisplayRowAppendProgress> {
        self.append_item_request_to_text_row_and_emit(
            state,
            face_ids,
            item.append_request(position),
        )
    }

    #[cfg(test)]
    pub(crate) fn append_source_mapped_text_item_to_text_row_and_emit(
        self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        item: DisplayReplacementSourceMappedTextItem,
        position: DisplayRowPosition,
    ) -> Option<DisplayRowAppendProgress> {
        self.append_item_request_to_text_row_and_emit(
            state,
            face_ids,
            item.append_request(position),
        )
    }
}

#[derive(Clone)]
pub(crate) struct DisplayReplacementAppendContext<'a> {
    single_item: SingleDisplayItemAppendContext<'a>,
}

impl<'a> DisplayReplacementAppendContext<'a> {
    pub(crate) fn new(
        face_id: FaceId,
        base_face: &'a ResolvedFace,
        frame: DisplayRowAppendFrame,
    ) -> Self {
        Self {
            single_item: SingleDisplayItemAppendContext::new(base_face, face_id, frame),
        }
    }

    fn append_replacement_item_plan_to_text_row_and_emit(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        plan: DisplayReplacementItemAppendPlan,
    ) -> Option<DisplayRowAppendProgress> {
        let (item, position) = plan.into_parts();
        let mut source = DisplayItemSegmentSource::new(item);
        let mut source_state = DisplayRowSourceState::frame_local();
        let mut render_policy = NaturalDisplayRowAppendRenderPolicy;
        let outcome = self.single_item.render_source_with_policy(
            state,
            face_ids,
            &mut source,
            &mut source_state,
            position,
            DisplayRowAppendKind::DisplayReplacement,
            &mut render_policy,
            self.single_item.face_id(),
        )?;
        Some(outcome.into_append_progress(position))
    }

    #[cfg(test)]
    pub(crate) fn append_replacement_item_kind_to_text_row_and_emit(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        replacement_source: BufferDisplayReplacementSource,
        kind: DisplayItemKind,
        position: DisplayRowPosition,
    ) -> Option<DisplayRowAppendProgress> {
        let plan = DisplayReplacementItemAppendRequest::active_face(kind, position)
            .into_plan(replacement_source, self.single_item.face_id());
        self.append_replacement_item_plan_to_text_row_and_emit(state, face_ids, plan)
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_replacement_string_source_to_text_row_and_emit(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        replacement_source: BufferDisplayReplacementSource,
        source_id: LispStringSourceId,
        value: Value,
        position: DisplayRowPosition,
        item_policy: &mut impl DisplayRowRenderPolicy,
    ) -> DisplayRowPosition {
        let Some(mut source) =
            BufferDisplayReplacementStringRequest::new(source_id.raw(), value, replacement_source)
                .into_source(self.single_item.face_id())
        else {
            return position;
        };
        let mut source_state = DisplayRowSourceState::frame_local();
        let frame = self.single_item.frame();
        let geometry = frame.geometry();
        let mut render_policy = DisplayReplacementStringRenderPolicy {
            item_policy,
            fallback_metrics: DisplayRowMeasuredFaceMetrics::new(
                geometry.char_width(),
                geometry.height(),
                geometry.ascent(),
                frame.face_space_width(),
            ),
            produced_row_break: None,
        };
        self.single_item
            .render_source_with_policy(
                state,
                face_ids,
                &mut source,
                &mut source_state,
                position,
                DisplayRowAppendKind::DisplayReplacementString,
                &mut render_policy,
                self.single_item.face_id(),
            )
            .map(|outcome| outcome.end_position())
            .unwrap_or(position)
    }
}
