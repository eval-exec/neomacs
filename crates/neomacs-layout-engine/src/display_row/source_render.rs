//! Source-render state facade.
//!
//! This module holds the state types that bridge the typed display-source
//! layer (`DisplayItemSource`) to the row renderer and output builder.  It
//! lives between `display_source.rs` / `display_row.rs` and the append-layer
//! helpers in `display_row_append.rs`, so that the append module does not
//! need to own the render-state facade.

use crate::display_current_row_output::{DisplayCurrentRowMutation, DisplayRowCurrentRowOutput};
use crate::display_face_policy::BaseFacePolicy;
use crate::display_item::{
    DisplayItem, DisplayPropertyReplacementDescriptor, RenderFaceRef, SourceSpan,
};
use crate::display_mock_frame::protocol_color_to_pixel;
use crate::display_origin::DisplayOrigin;
use crate::display_property::DisplayReplacementProperty;
use crate::display_row::append_context::{
    DisplayMarginAreaCapacity, DisplayRowAppendFrame, DisplayRowAppendSourceRenderRequest,
};
use crate::display_row::builder::{
    DisplayRowGlyphCheckpoint, DisplayRowPosition, DisplayRowVerticalMetrics,
};
use crate::display_row::face_environment::WindowFaces;
use crate::display_row::face_state::{
    DisplayRowActiveFaceState, DisplayRowMeasurementMode, DisplayRowMeasurementPolicy,
    DisplayRowResolvedMeasuredFace,
};
pub(crate) use crate::display_row::finalizer::RowExtendFill;
use crate::display_row::geometry::{DisplayRowExtendState, DisplayRowGeometryState};
use crate::display_row::line_end::{
    self, LineEndContext, LineEndExtend, LineEndFaceResolver, LineEndFillGeometry,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::render_policy::DisplayRowRenderPolicy;
use crate::display_row::render_state::{
    CurrentTextRowRenderOutcome, DisplayRowRenderIntoRowResult, display_row_output_end_position,
};
use crate::display_row::replacement::DisplayPropertyReplacementRowRenderRequest;
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_row::text_output::TextRowOutput;
use crate::display_row::{
    DisplayRowRenderContext, DisplayRowRenderExecutor, DisplayRowRenderer,
    DisplayRowSourceFragmentFrame, DisplayRowSourceRenderRequest,
};
use crate::display_source::{
    DisplayItemSegmentSource, DisplayItemSource, DisplayMarginEmission,
    DisplayMarginEmissionContent, DisplayNonTextAreaEmission, LispStringSourceCursor,
    LispStringSourceOrigin,
};
use crate::display_source_resolver::{
    ActiveDisplayStringBaseFace, DisplayDefaultFaceInstallPolicy, DisplayStringBaseFace,
    resolve_display_string_base_face,
};
use crate::display_spec::DisplayFringeSide;
use crate::display_text_output_install::TextWindowRowDecorationRequest;
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{FaceResolver, LayoutBufferView, ResolvedFace};
use crate::types::WindowParams;
use crate::window_output::{
    DisplayTextRowGeometryTransition, DisplayTextRowTransition, TextWindowOutputTarget,
    WindowOutputEmitter, install_text_window_row_decoration_request,
    transition_text_window_row_with_limit,
};
use neomacs_display_protocol::glyph_matrix::{FringeBitmapInfo, GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::types::Color;
use neomacs_display_protocol::types::FaceId;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::DisplayHost;
use neovm_core::window::DisplayRowSnapshot;

/// Current-row mutation that attaches a resolved fringe bitmap to the row's
/// left or right fringe slot.
struct SetRowFringeBitmapMutation {
    side: DisplayFringeSide,
    info: FringeBitmapInfo,
}

/// Reconcile a preallocated structural area's logical extent with the
/// concrete advances of its rendered content.
///
/// GNU line-number faces are documented to be monospaced, but Neomacs keeps
/// subpixel concrete glyph advances while the frame grid is integer/hinted.
/// Even a monospaced font can therefore expose a small measurement-domain
/// delta between digits and blank padding.  The owning structural area—not
/// the following buffer text—absorbs that delta by distributing the remaining
/// extent over its explicit space glyphs.
struct FitGlyphAreaPaddingToExtentMutation {
    area: GlyphArea,
    extent_px: f32,
}

/// Promote the materialized row after a structural source has been emitted.
///
/// Structural prefixes are rendered before the ordinary buffer source, so
/// their glyph metrics must update both the output row and the walk's geometry
/// authority.  Keeping the row half as a typed mutation prevents callers from
/// reaching into the output builder and assigning height/ascent independently.
struct IncludeCurrentRowVerticalMetricsMutation {
    metrics: DisplayRowVerticalMetrics,
}

impl DisplayCurrentRowMutation for IncludeCurrentRowVerticalMetricsMutation {
    type Output = ();

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        self.metrics.include_in_row(row);
    }
}

/// Complete a structural margin segment to its authoritative window-owned
/// extent.  Explicit margin content paints inside this segment; it must never
/// become extra horizontal flow that moves line numbers or buffer text.
struct FillMarginLaneToCapacityMutation {
    area: GlyphArea,
    capacity: DisplayMarginAreaCapacity,
    face_id: FaceId,
}

struct TakeGlyphAreaMutation {
    area: GlyphArea,
}

impl DisplayCurrentRowMutation for TakeGlyphAreaMutation {
    type Output = Vec<neomacs_display_protocol::glyph_matrix::Glyph>;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        std::mem::take(&mut row.glyphs[self.area.index()])
    }
}

struct AppendGlyphAreaMutation {
    area: GlyphArea,
    glyphs: Vec<neomacs_display_protocol::glyph_matrix::Glyph>,
}

struct FirstTextGlyphAfterCheckpointMutation {
    checkpoint: DisplayRowGlyphCheckpoint,
}

struct TrailingBoxRunTerminalMutation;

fn trailing_box_run_terminal_for_row(
    row: &GlyphRow,
) -> neomacs_display_protocol::face::BoxVerticalEdges {
    let owns_right = row.glyphs[GlyphArea::Text.index()]
        .iter()
        .rev()
        .find(|glyph| !glyph.padding)
        .is_some_and(|glyph| glyph.box_vertical_edges.owns_right());
    neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(false, owns_right)
}

impl DisplayCurrentRowMutation for TrailingBoxRunTerminalMutation {
    type Output = neomacs_display_protocol::face::BoxVerticalEdges;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        trailing_box_run_terminal_for_row(row)
    }
}

impl DisplayCurrentRowMutation for FirstTextGlyphAfterCheckpointMutation {
    type Output = Option<neomacs_display_protocol::glyph_matrix::Glyph>;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        self.checkpoint.first_new_text_glyph(row).cloned()
    }
}

impl DisplayCurrentRowMutation for AppendGlyphAreaMutation {
    type Output = ();

    fn apply(mut self, row: &mut GlyphRow) -> Self::Output {
        row.glyphs[self.area.index()].append(&mut self.glyphs);
    }
}

/// Source-order relation between an explicit margin spec and structural
/// decorations already emitted for the row (notably display line numbers).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayStructuralAreaOrder {
    BeforeExisting,
    AfterExisting,
}

impl DisplayCurrentRowMutation for FitGlyphAreaPaddingToExtentMutation {
    type Output = bool;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        if !self.extent_px.is_finite() || self.extent_px <= 0.0 {
            return false;
        }
        let glyphs = &mut row.glyphs[self.area.index()];
        let mut padding_count = 0usize;
        let mut content_width = 0.0f32;
        for glyph in glyphs.iter() {
            if !glyph.padding && matches!(glyph.glyph_type, GlyphType::Char { ch: ' ' }) {
                padding_count += 1;
            } else if !glyph.padding {
                content_width += glyph.pixel_width.max(0.0);
            }
        }
        if padding_count == 0 || content_width >= self.extent_px {
            return false;
        }
        let padding_width = (self.extent_px - content_width) / padding_count as f32;
        if !padding_width.is_finite() || padding_width <= 0.0 {
            return false;
        }
        for glyph in glyphs.iter_mut() {
            if !glyph.padding && matches!(glyph.glyph_type, GlyphType::Char { ch: ' ' }) {
                glyph.pixel_width = padding_width;
            }
        }
        // Keep the structural boundary bit-stable across rows with different
        // digit counts. Repeating one fractional padding width can accumulate
        // to either side of `extent_px`; let the final explicit blank absorb
        // that floating-point residual so following buffer text always starts
        // at the exact window-planned X.
        let fitted_width = glyphs
            .iter()
            .filter(|glyph| !glyph.padding)
            .map(|glyph| glyph.pixel_width.max(0.0))
            .sum::<f32>();
        let residual = self.extent_px - fitted_width;
        if residual != 0.0
            && let Some(last_padding) = glyphs.iter_mut().rfind(|glyph| {
                !glyph.padding && matches!(glyph.glyph_type, GlyphType::Char { ch: ' ' })
            })
        {
            let corrected = last_padding.pixel_width + residual;
            if corrected.is_finite() && corrected > 0.0 {
                last_padding.pixel_width = corrected;
            }
        }
        true
    }
}

impl DisplayCurrentRowMutation for FillMarginLaneToCapacityMutation {
    type Output = bool;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        if self.capacity.is_empty() {
            return false;
        }
        let glyphs = &mut row.glyphs[self.area.index()];
        let content_width = glyphs
            .iter()
            .filter(|glyph| !glyph.padding)
            .map(|glyph| glyph.pixel_width.max(0.0))
            .sum::<f32>();
        let remaining_width = (self.capacity.width_px() - content_width).max(0.0);
        if !remaining_width.is_finite() || remaining_width <= f32::EPSILON {
            return false;
        }
        let used_columns = glyphs
            .iter()
            .filter(|glyph| !glyph.padding)
            .map(|glyph| match glyph.glyph_type {
                GlyphType::Stretch { width_cols } => usize::from(width_cols),
                _ if glyph.wide => 2,
                _ => 1,
            })
            .sum::<usize>();
        let remaining_columns = self.capacity.columns().saturating_sub(used_columns);
        glyphs.push(
            neomacs_display_protocol::glyph_matrix::Glyph::stretch(
                remaining_columns.min(u16::MAX as usize) as u16,
                self.face_id,
            )
            .with_pixel_width(remaining_width),
        );
        true
    }
}

impl DisplayCurrentRowMutation for SetRowFringeBitmapMutation {
    type Output = ();

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        match self.side {
            DisplayFringeSide::Left => row.left_fringe_bitmap = Some(self.info),
            DisplayFringeSide::Right => row.right_fringe_bitmap = Some(self.info),
        }
    }
}

pub(crate) struct TextRowOutputRenderState<'a> {
    output: TextWindowOutputTarget<'a>,
    output_emitter: &'a mut WindowOutputEmitter,
    evaluator: &'a mut Context,
}

struct DisplayRowCurrentTextSourceState<'face, 'emit> {
    row_output: DisplayRowCurrentRowOutput<'emit>,
    evaluator: &'emit mut Context,
    font_metrics: &'emit mut Option<FontMetricsService>,
    measurement_mode: DisplayRowMeasurementMode,
    face_resolver: &'face FaceResolver,
    face_ids: &'emit mut FrameFaceAttempt,
}

struct DisplayRowCurrentSourceFragmentRenderState<'face, 'emit> {
    row_output: DisplayRowCurrentRowOutput<'emit>,
    font_metrics: &'emit mut Option<FontMetricsService>,
    measurement_mode: DisplayRowMeasurementMode,
    face_resolver: &'face FaceResolver,
    display_host: Option<&'emit dyn DisplayHost>,
    face_ids: &'emit mut FrameFaceAttempt,
}

struct DisplayRowCurrentTextSourceStepResult {
    result: DisplayRowRenderIntoRowResult,
    row_height_px: f32,
    row_ascent_px: f32,
}

struct DisplayRowCurrentSourceStepMutation<'a, 'request, 'renderer, 'face, 'host, S, P> {
    row_request: DisplayRowSourceRenderRequest<'request>,
    renderer: DisplayRowRenderer<'renderer>,
    source: &'a mut S,
    source_state: &'a mut DisplayRowSourceState,
    context: DisplayRowRenderContext<'face, 'host>,
    render_policy: &'a mut P,
}

struct DisplayRowNaturalSourceFragmentMutation<'a, 'request, 'metrics, 'face, 'host, S> {
    request: DisplayRowSourceRenderRequest<'request>,
    render_executor: &'a mut DisplayRowRenderExecutor<'metrics, 'face, 'host>,
    source: &'a mut S,
    source_state: &'a mut DisplayRowSourceState,
}

/// Mutation that appends the trailing extend-face stretch to the current row's
/// TEXT area, without emitting any output span. Mirrors GNU
/// `extend_face_to_end_of_line`: an empty row first gets a leading space glyph
/// (xdisp.c:24420) so the row `displays_text` and carries a face anchor; the
/// fill stretch is then pushed to the text-area right edge. Returns `true` when
/// a fill was applied.
struct RowExtendFillMutation {
    fill: RowExtendFill,
}

impl DisplayCurrentRowMutation for RowExtendFillMutation {
    type Output = bool;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        self.fill.apply_to(row)
    }
}

/// Mutation that rolls the current row's drawn glyphs back to a previously
/// captured `DisplayRowGlyphCheckpoint`. Used by the word-wrap break to drop the
/// partial-word glyphs that fit on the current row but belong on the next
/// continuation row. GNU keeps whole words by rewinding its iterator to the word
/// boundary; we mirror that by truncating the glyph row here while the source
/// position is rewound to the same boundary.
struct DisplayRowGlyphCheckpointRestoreMutation {
    checkpoint: DisplayRowGlyphCheckpoint,
}

impl DisplayCurrentRowMutation for DisplayRowGlyphCheckpointRestoreMutation {
    type Output = ();

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        self.checkpoint.restore(row);
    }
}

impl<S, P> DisplayCurrentRowMutation
    for DisplayRowCurrentSourceStepMutation<'_, '_, '_, '_, '_, S, P>
where
    S: DisplayItemSource,
    P: DisplayRowRenderPolicy,
{
    type Output = Option<(DisplayRowRenderIntoRowResult, f32, f32)>;

    fn apply(self, row: &mut neomacs_display_protocol::glyph_matrix::GlyphRow) -> Self::Output {
        let mut renderer = self.renderer;
        let mut context = self.context;
        let result = self.row_request.render_fragment_step_into_row_with_policy(
            &mut renderer,
            row,
            self.source,
            self.source_state,
            &mut context,
            self.render_policy,
        )?;
        result.apply_current_row_effects_to(row);
        Some((result, row.height_px, row.ascent_px))
    }
}

impl<S> DisplayCurrentRowMutation for DisplayRowNaturalSourceFragmentMutation<'_, '_, '_, '_, '_, S>
where
    S: DisplayItemSource,
{
    type Output = Option<DisplayRowRenderIntoRowResult>;

    fn apply(self, row: &mut neomacs_display_protocol::glyph_matrix::GlyphRow) -> Self::Output {
        let result = self.render_executor.render_item_source_fragment_into_row(
            self.request,
            row,
            self.source,
            self.source_state,
        )?;
        result.apply_current_row_effects_to(row);
        Some(result)
    }
}

impl<'face, 'emit> DisplayRowCurrentTextSourceState<'face, 'emit> {
    fn new(
        row_output: DisplayRowCurrentRowOutput<'emit>,
        evaluator: &'emit mut Context,
        font_metrics: &'emit mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        face_resolver: &'face FaceResolver,
        face_ids: &'emit mut FrameFaceAttempt,
    ) -> Self {
        Self {
            row_output,
            evaluator,
            font_metrics,
            measurement_mode,
            face_resolver,
            face_ids,
        }
    }

    fn render_source_with_policy<S, P>(
        &mut self,
        row_request: DisplayRowSourceRenderRequest<'_>,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        render_policy: &mut P,
    ) -> Option<DisplayRowCurrentTextSourceStepResult>
    where
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    {
        let mutation = DisplayRowCurrentSourceStepMutation {
            row_request,
            renderer: DisplayRowRenderer::new(self.font_metrics, self.measurement_mode),
            source,
            source_state,
            context: DisplayRowRenderContext::new(
                self.face_resolver,
                self.evaluator.display_host.as_deref(),
                self.face_ids,
            ),
            render_policy,
        };
        let (result, row_height_px, row_ascent_px) =
            self.row_output.apply_current_row_mutation(mutation)??;
        Some(DisplayRowCurrentTextSourceStepResult {
            result,
            row_height_px,
            row_ascent_px,
        })
    }
    fn measure_source_with_policy<S, P>(
        &mut self,
        row_request: DisplayRowSourceRenderRequest<'_>,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        render_policy: &mut P,
    ) -> Option<DisplayRowCurrentTextSourceStepResult>
    where
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    {
        let mutation = DisplayRowCurrentSourceStepMutation {
            row_request,
            renderer: DisplayRowRenderer::new(self.font_metrics, self.measurement_mode),
            source,
            source_state,
            context: DisplayRowRenderContext::new(
                self.face_resolver,
                self.evaluator.display_host.as_deref(),
                self.face_ids,
            ),
            render_policy,
        };
        let (result, row_height_px, row_ascent_px) = self
            .row_output
            .apply_current_row_scratch_mutation(mutation)??;
        Some(DisplayRowCurrentTextSourceStepResult {
            result,
            row_height_px,
            row_ascent_px,
        })
    }
}

impl<'face, 'emit> DisplayRowCurrentSourceFragmentRenderState<'face, 'emit> {
    fn new(
        row_output: DisplayRowCurrentRowOutput<'emit>,
        font_metrics: &'emit mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        face_resolver: &'face FaceResolver,
        display_host: Option<&'emit dyn DisplayHost>,
        face_ids: &'emit mut FrameFaceAttempt,
    ) -> Self {
        Self {
            row_output,
            font_metrics,
            measurement_mode,
            face_resolver,
            display_host,
            face_ids,
        }
    }

    fn render_natural_fragment_into_current_row<S: DisplayItemSource>(
        &mut self,
        request: DisplayRowSourceRenderRequest<'_>,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        let mut render_executor = DisplayRowRenderExecutor::new(
            self.font_metrics,
            self.measurement_mode,
            self.face_resolver,
            self.display_host,
            self.face_ids,
        );
        let result = self.row_output.apply_current_row_mutation(
            DisplayRowNaturalSourceFragmentMutation {
                request,
                render_executor: &mut render_executor,
                source,
                source_state,
            },
        )??;
        Some(result)
    }
}

impl DisplayRowCurrentTextSourceStepResult {
    fn into_measure_outcome(self) -> CurrentTextRowRenderOutcome {
        let (progress, source_slots, _faces, stop) = self.result.into_current_row_parts();
        let end = display_row_output_end_position(progress);
        CurrentTextRowRenderOutcome::new(
            stop,
            source_slots,
            end,
            self.row_height_px,
            self.row_ascent_px,
        )
    }
}

fn render_display_item_source_into_current_text_row<S, P>(
    state: &mut DisplayRowCurrentTextSourceState<'_, '_>,
    source: &mut S,
    source_state: &mut DisplayRowSourceState,
    request: DisplayRowSourceRenderRequest<'_>,
    render_policy: &mut P,
) -> Option<DisplayRowCurrentTextSourceStepResult>
where
    S: DisplayItemSource,
    P: DisplayRowRenderPolicy,
{
    state.render_source_with_policy(request, source, source_state, render_policy)
}

fn measure_display_item_source_against_current_text_row<S, P>(
    state: &mut DisplayRowCurrentTextSourceState<'_, '_>,
    source: &mut S,
    source_state: &mut DisplayRowSourceState,
    request: DisplayRowSourceRenderRequest<'_>,
    render_policy: &mut P,
) -> Option<CurrentTextRowRenderOutcome>
where
    S: DisplayItemSource,
    P: DisplayRowRenderPolicy,
{
    state
        .measure_source_with_policy(request, source, source_state, render_policy)
        .map(DisplayRowCurrentTextSourceStepResult::into_measure_outcome)
}

impl<'a> TextRowOutputRenderState<'a> {
    pub(crate) fn from_parts(
        output: TextWindowOutputTarget<'a>,
        output_emitter: &'a mut WindowOutputEmitter,
        evaluator: &'a mut Context,
    ) -> Self {
        Self {
            output,
            output_emitter,
            evaluator,
        }
    }

    pub(crate) fn reborrow(&mut self) -> TextRowOutputRenderState<'_> {
        TextRowOutputRenderState {
            output: self.output.reborrow(),
            output_emitter: self.output_emitter,
            evaluator: self.evaluator,
        }
    }

    pub(crate) fn with_output_target_parts<R>(
        self,
        f: impl FnOnce(TextWindowOutputTarget<'_>, &mut WindowOutputEmitter, &mut Context) -> R,
    ) -> R {
        f(self.output, self.output_emitter, self.evaluator)
    }

    pub(crate) fn transition_text_row_with_limit(
        self,
        transition: DisplayTextRowGeometryTransition,
        max_rows: usize,
    ) -> DisplayTextRowTransition {
        transition_text_window_row_with_limit(
            self.output,
            self.output_emitter,
            self.evaluator,
            transition,
            max_rows,
        )
    }

    pub(crate) fn install_row_decoration(self, request: TextWindowRowDecorationRequest) {
        install_text_window_row_decoration_request(self.output, request);
    }

    fn insert_resolved_face(&mut self, face_id: FaceId, face: &ResolvedFace) {
        self.output.install_resolved_face(face_id, face, None);
    }

    fn install_resolved_measured_face(&mut self, face: &DisplayRowResolvedMeasuredFace) {
        self.output.install_resolved_face(
            face.face_id(),
            face.resolved_face(),
            face.font_metrics(),
        );
    }

    fn display_host(&self) -> Option<&dyn DisplayHost> {
        self.evaluator.display_host.as_deref()
    }

    fn evaluator(&self) -> &Context {
        self.evaluator
    }

    fn current_row_output(&mut self) -> DisplayRowCurrentRowOutput<'_> {
        self.output.current_row_output()
    }

    fn output_emitter(&mut self) -> &mut WindowOutputEmitter {
        self.output_emitter
    }

    fn output_emitter_ref(&self) -> &WindowOutputEmitter {
        self.output_emitter
    }

    fn output_rows(&self) -> &[DisplayRowSnapshot] {
        self.output_emitter.rows()
    }

    fn output_rows_len(&self) -> usize {
        self.output_emitter.rows().len()
    }

    fn measure_state<'emit>(
        &'emit mut self,
        font_metrics: &'emit mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        face_resolver: &'emit FaceResolver,
    ) -> TextRowSourceMeasureState<'emit> {
        TextRowSourceMeasureState {
            row_output: self.output.current_row_output(),
            evaluator: self.evaluator,
            font_metrics,
            measurement_mode,
            face_resolver,
        }
    }

    fn current_text_render_state<'emit>(
        &'emit mut self,
        font_metrics: &'emit mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        face_resolver: &'emit FaceResolver,
        face_ids: &'emit mut FrameFaceAttempt,
    ) -> DisplayRowCurrentTextSourceState<'emit, 'emit> {
        DisplayRowCurrentTextSourceState::new(
            self.output.current_row_output(),
            self.evaluator,
            font_metrics,
            measurement_mode,
            face_resolver,
            face_ids,
        )
    }

    fn current_source_fragment_render_state<'emit>(
        &'emit mut self,
        font_metrics: &'emit mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        face_resolver: &'emit FaceResolver,
        face_ids: &'emit mut FrameFaceAttempt,
    ) -> DisplayRowCurrentSourceFragmentRenderState<'emit, 'emit> {
        DisplayRowCurrentSourceFragmentRenderState::new(
            self.output.current_row_output(),
            font_metrics,
            measurement_mode,
            face_resolver,
            self.evaluator.display_host.as_deref(),
            face_ids,
        )
    }

    /// Capture the current output row's glyph counts for a word-wrap candidate.
    fn capture_current_row_glyph_checkpoint(&self) -> DisplayRowGlyphCheckpoint {
        self.output.capture_current_row_glyph_checkpoint()
    }

    /// Truncate the current output row's drawn glyphs back to `checkpoint`,
    /// dropping the partial-word glyphs that the word-wrap break rewinds past.
    fn restore_current_row_glyph_checkpoint(&mut self, checkpoint: DisplayRowGlyphCheckpoint) {
        self.output
            .current_row_output()
            .apply_current_row_mutation(DisplayRowGlyphCheckpointRestoreMutation { checkpoint });
    }

    /// Append a trailing `:extend` fill stretch to the current row's TEXT area
    /// without emitting an output span. Returns `true` when a fill was applied.
    fn extend_current_row_face_to_end_of_line(&mut self, fill: RowExtendFill) -> bool {
        self.output
            .current_row_output()
            .apply_current_row_mutation(RowExtendFillMutation { fill })
            .unwrap_or(false)
    }

    fn finish_current_text_row_render(
        &mut self,
        output: TextRowOutput,
        result: DisplayRowCurrentTextSourceStepResult,
    ) -> CurrentTextRowRenderOutcome {
        let DisplayRowCurrentTextSourceStepResult {
            result,
            row_height_px,
            row_ascent_px,
        } = result;
        let (progress, source_slots, faces, stop) = result.into_current_row_parts();
        let end = display_row_output_end_position(progress);
        self.output.install_rendered_fragment_assets(&faces);
        let output_spans = output.spans_for_source_slots(&source_slots);
        self.output_emitter
            .emit_text_output_spans(self.evaluator, output, output_spans, end);
        CurrentTextRowRenderOutcome::new(stop, source_slots, end, row_height_px, row_ascent_px)
    }
}

/// Face-resolution services for the line-end seam: resolves the named
/// `trailing-whitespace` / `fill-column-indicator` faces against the frame
/// face resolver, interns a stable face id, and installs the resolved face on
/// the output, exactly as the pre-seam inline sequence did.
struct TextRowLineEndFaceResolver<'render, 'a, 'ids> {
    render: &'render mut TextRowSourceRenderState<'a>,
    face_ids: &'ids mut FrameFaceAttempt,
}

impl TextRowLineEndFaceResolver<'_, '_, '_> {
    fn install_named_face(&mut self, face: &ResolvedFace) -> FaceId {
        let face_id =
            crate::display_row::face_state::stable_face_id_for_resolved(self.face_ids, face);
        self.render.insert_resolved_face(face_id, face);
        face_id
    }
}

impl LineEndFaceResolver for TextRowLineEndFaceResolver<'_, '_, '_> {
    fn trailing_whitespace_face_id(&mut self) -> FaceId {
        let face = self.render.resolve_named_face("trailing-whitespace");
        self.install_named_face(&face)
    }

    fn fill_column_indicator_face_id(&mut self, extend_bg: Option<Color>) -> FaceId {
        // GNU: it->face_id = merge_faces (w, Qfill_column_indicator, 0,
        // saved_face_id). With an active `:extend` face the indicator keeps
        // that fill's background so the highlight is continuous.
        let mut face = self.render.resolve_named_face("fill-column-indicator");
        if let Some(extend_bg) = extend_bg {
            face.bg = protocol_color_to_pixel(extend_bg);
            face.use_default_background = false;
        }
        self.install_named_face(&face)
    }
}

pub(crate) struct TextRowSourceRenderState<'a> {
    output_render: TextRowOutputRenderState<'a>,
    font_metrics: &'a mut Option<FontMetricsService>,
    measurement_mode: DisplayRowMeasurementMode,
    faces: WindowFaces<'a>,
}

impl<'a> TextRowSourceRenderState<'a> {
    pub(crate) fn from_output_render(
        output_render: TextRowOutputRenderState<'a>,
        font_metrics: &'a mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        faces: WindowFaces<'a>,
    ) -> Self {
        Self {
            output_render,
            font_metrics,
            measurement_mode,
            faces,
        }
    }

    pub(crate) fn reborrow(&mut self) -> TextRowSourceRenderState<'_> {
        TextRowSourceRenderState {
            output_render: self.output_render.reborrow(),
            font_metrics: self.font_metrics,
            measurement_mode: self.measurement_mode,
            faces: self.faces,
        }
    }

    pub(crate) fn output_render(&mut self) -> TextRowOutputRenderState<'_> {
        self.output_render.reborrow()
    }

    pub(crate) fn measurement_mode(&self) -> DisplayRowMeasurementMode {
        self.measurement_mode
    }

    pub(crate) fn current_row_visible_content_metrics(
        &mut self,
        face_ids: &FrameFaceAttempt,
        fallback: DisplayRowFallbackMetrics,
    ) -> DisplayRowVerticalMetrics {
        let Some(row) = self
            .output_render
            .current_row_output()
            .current_row_snapshot()
        else {
            return DisplayRowVerticalMetrics::new(1.0, 1.0);
        };
        let (height, ascent) = crate::display_row::finalizer::display_row_visible_content_metrics(
            &row,
            fallback.row_height(),
            fallback.ascent(),
            |face_id| face_ids.face_vertical_metrics(face_id),
        );
        DisplayRowVerticalMetrics::new(height, ascent)
    }

    /// Reconcile a just-rendered structural prefix with both row authorities.
    /// GNU's display iterator does this while producing each glyph by updating
    /// `max_ascent`/`max_descent`; Neomacs renders the prefix as a fragment, so
    /// the equivalent operation happens once from the fragment's visible
    /// content metrics.
    pub(crate) fn include_current_row_visible_content_metrics(
        &mut self,
        face_ids: &FrameFaceAttempt,
        fallback: DisplayRowFallbackMetrics,
        geometry: &mut DisplayRowGeometryState,
    ) {
        let metrics = self.current_row_visible_content_metrics(face_ids, fallback);
        self.output_render
            .current_row_output()
            .apply_current_row_mutation(IncludeCurrentRowVerticalMetricsMutation { metrics });
        geometry.include_glyph_vertical_metrics(metrics.height_px(), metrics.ascent_px());
    }

    pub(crate) fn measure_state(&mut self) -> TextRowSourceMeasureState<'_> {
        self.output_render.measure_state(
            self.font_metrics,
            self.measurement_mode,
            self.faces.pipeline_resolver(),
        )
    }

    pub(crate) fn insert_resolved_face(&mut self, face_id: FaceId, face: &ResolvedFace) {
        self.output_render.insert_resolved_face(face_id, face);
    }

    fn install_pending_display_string_base_face(&mut self, base_face: &DisplayStringBaseFace) {
        if let Some(pending_face) = base_face.pending_face() {
            self.insert_resolved_face(pending_face.face_id(), pending_face.resolved());
        }
    }

    fn resolved_measured_face(
        &mut self,
        measurement_policy: DisplayRowMeasurementPolicy,
        face_id: FaceId,
        face: ResolvedFace,
        fallback_char_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> DisplayRowResolvedMeasuredFace {
        let metrics = if measurement_policy.uses_concrete_font_geometry() {
            self.font_metrics.as_mut().map(|svc| {
                svc.font_metrics(
                    &face.font_family,
                    face.font_weight,
                    face.italic,
                    face.font_size,
                )
            })
        } else {
            None
        };
        measurement_policy.resolved_measured_face(
            face_id,
            face,
            metrics,
            fallback_char_width,
            fallback_metrics,
            self.font_metrics,
        )
    }

    /// [`Self::resolve_and_install_measured_face`] WITHOUT the install: a
    /// probe for the routed row acquisition, which must measure candidate
    /// segments against their measured faces before deciding whether to
    /// commit (and only then installs, through the checkpoint seam).
    pub(crate) fn resolve_measured_face_without_install(
        &mut self,
        measurement_policy: DisplayRowMeasurementPolicy,
        face_id: FaceId,
        face: ResolvedFace,
        fallback_char_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> DisplayRowActiveFaceState {
        self.resolved_measured_face(
            measurement_policy,
            face_id,
            face,
            fallback_char_width,
            fallback_metrics,
        )
        .into_active_face_state()
    }

    pub(crate) fn resolve_and_install_measured_face(
        &mut self,
        measurement_policy: DisplayRowMeasurementPolicy,
        face_id: FaceId,
        face: ResolvedFace,
        fallback_char_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> DisplayRowActiveFaceState {
        let resolved_face = self.resolved_measured_face(
            measurement_policy,
            face_id,
            face,
            fallback_char_width,
            fallback_metrics,
        );
        self.output_render
            .install_resolved_measured_face(&resolved_face);
        resolved_face.into_active_face_state()
    }

    pub(crate) fn resolve_named_face(&self, face_name: &str) -> ResolvedFace {
        self.faces.resolve_named_face(face_name)
    }

    /// Merge a named face over a base resolved face, GNU
    /// `merge_faces(w, <named-face>, 0, base_face_id)`: start from `base`'s full
    /// attribute set, overlay only the attributes the named face specifies
    /// (resolving its `:inherit` chain), and return the realized face. Returns
    /// `base` unchanged when the named face contributes nothing.
    pub(crate) fn merge_named_face_over(
        &self,
        base: &ResolvedFace,
        face_name: &str,
    ) -> ResolvedFace {
        self.faces.merge_named_face_over(base, face_name)
    }

    pub(crate) fn default_face(&self) -> ResolvedFace {
        self.faces.default_face()
    }

    pub(crate) fn display_string_base_face<B: LayoutBufferView>(
        &mut self,
        buffer: &B,
        origin: DisplayOrigin,
        policy: BaseFacePolicy,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayStringBaseFace {
        let base_face = resolve_display_string_base_face(
            buffer,
            self.faces.pipeline_resolver(),
            origin,
            policy,
            None,
            DisplayDefaultFaceInstallPolicy::InstallDefaultFace,
            face_ids,
        );
        self.install_pending_display_string_base_face(&base_face);
        base_face
    }

    pub(crate) fn default_display_string_base_face<B: LayoutBufferView>(
        &mut self,
        buffer: &B,
        origin: DisplayOrigin,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayStringBaseFace {
        self.display_string_base_face(buffer, origin, origin.default_base_face_policy(), face_ids)
    }

    pub(crate) fn display_string_base_face_for_active_row<B: LayoutBufferView>(
        &mut self,
        buffer: &B,
        origin: DisplayOrigin,
        policy: BaseFacePolicy,
        active_face_state: &DisplayRowActiveFaceState,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayStringBaseFace {
        let base_face = resolve_display_string_base_face(
            buffer,
            self.faces.pipeline_resolver(),
            origin,
            policy,
            Some(ActiveDisplayStringBaseFace::new(
                active_face_state.face_id(),
                active_face_state.resolved_face(),
            )),
            DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace,
            face_ids,
        );
        self.install_pending_display_string_base_face(&base_face);
        base_face
    }

    pub(crate) fn default_display_string_base_face_for_active_row<B: LayoutBufferView>(
        &mut self,
        buffer: &B,
        origin: DisplayOrigin,
        active_face_state: &DisplayRowActiveFaceState,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayStringBaseFace {
        self.display_string_base_face_for_active_row(
            buffer,
            origin,
            origin.default_base_face_policy(),
            active_face_state,
            face_ids,
        )
    }

    pub(crate) fn render_natural_fragment_into_current_row<S: DisplayItemSource>(
        &mut self,
        request: DisplayRowSourceRenderRequest<'_>,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        face_ids: &mut FrameFaceAttempt,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        let result = self
            .output_render
            .current_source_fragment_render_state(
                self.font_metrics,
                self.measurement_mode,
                self.faces.pipeline_resolver(),
                face_ids,
            )
            .render_natural_fragment_into_current_row(request, source, source_state);
        if let Some(result) = result.as_ref() {
            self.output_render
                .output
                .install_rendered_fragment_assets(result.faces());
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_natural_fragment_from_row_geometry_columns<S: DisplayItemSource>(
        &mut self,
        row_geometry: &DisplayRowGeometryState,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        cols: usize,
        char_width: f32,
        role: neomacs_display_protocol::frame_glyphs::GlyphRowRole,
        face_id: FaceId,
        base_face: &ResolvedFace,
        start_col: usize,
        max_col: usize,
        area: neomacs_display_protocol::glyph_matrix::GlyphArea,
        face_ids: &mut FrameFaceAttempt,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        let request = DisplayRowSourceFragmentFrame::from_row_geometry_columns(
            row_geometry,
            cols,
            char_width,
            role,
            face_id,
            base_face,
        )
        .render_request_from_column_for_area(start_col, max_col, area);
        self.render_natural_fragment_into_current_row(request, source, source_state, face_ids)
    }

    /// Make a structural glyph area consume exactly its preallocated extent by
    /// assigning all residual width to the area's explicit blank padding.
    /// Concrete nonblank glyph advances remain untouched.
    pub(crate) fn fit_current_row_area_padding_to_extent(
        &mut self,
        area: GlyphArea,
        extent_px: f32,
    ) -> bool {
        self.output_render
            .current_row_output()
            .apply_current_row_mutation(FitGlyphAreaPaddingToExtentMutation { area, extent_px })
            .unwrap_or(false)
    }

    /// Route one resolved non-text emission into its structural glyph area.
    /// Inline text progress is deliberately untouched: GNU changes
    /// `it->area`, emits the margin glyphs, then resumes `TEXT_AREA` at the same
    /// buffer position.
    pub(crate) fn render_non_text_area_emission(
        &mut self,
        emission: DisplayNonTextAreaEmission,
        face_scope: crate::display_source_resolver::DisplaySourceFaceScope,
        frame: &DisplayRowAppendFrame,
        face_ids: &mut FrameFaceAttempt,
        fallback_face_id: FaceId,
        structural_order: DisplayStructuralAreaOrder,
    ) {
        match emission {
            DisplayNonTextAreaEmission::Fringe(layout) => {
                self.record_fringe_bitmap_layout(&layout, face_ids, fallback_face_id);
            }
            DisplayNonTextAreaEmission::Margin(margin) => {
                self.render_margin_emission(margin, face_scope, frame, face_ids, structural_order);
            }
        }
    }

    fn render_margin_emission(
        &mut self,
        emission: DisplayMarginEmission,
        face_scope: crate::display_source_resolver::DisplaySourceFaceScope,
        frame: &DisplayRowAppendFrame,
        face_ids: &mut FrameFaceAttempt,
        structural_order: DisplayStructuralAreaOrder,
    ) {
        let area = emission.side().glyph_area();
        let Some(capacity) = frame.margin_capacity(area) else {
            return;
        };
        if capacity.is_empty() {
            return;
        }

        // GNU initializes marginal display strings from the named `margin`
        // face, then lets the string's own face properties refine it.
        let margin_face = self.resolve_named_face("margin");
        let margin_face_id =
            crate::display_row::face_state::stable_face_id_for_resolved(face_ids, &margin_face);
        self.insert_resolved_face(margin_face_id, &margin_face);

        let columns = capacity.columns();
        let char_width = (capacity.width_px() / columns as f32).max(1.0);
        let fragment = DisplayRowSourceFragmentFrame::new(
            crate::display_row::geometry::DisplayRowGeometry::new(
                frame.geometry().y(),
                capacity.width_px(),
                frame.geometry().height(),
                char_width,
                frame.geometry().ascent(),
                crate::display_row::builder::DisplayTabPolicy::every(8),
            ),
            neomacs_display_protocol::frame_glyphs::GlyphRowRole::Text,
            margin_face_id,
            &margin_face,
        )
        .render_request_from_column_for_area(0, columns, area);

        // GNU encounters a BOL before-string before it calls
        // `maybe_produce_line_number`. Neomacs' prelude currently materializes
        // the line number eagerly, so temporarily detach existing structural
        // glyphs and restore them after the explicit margin content. The enum
        // makes this source-order choice explicit; later-in-line margin specs
        // retain append order.
        let trailing_glyphs = match structural_order {
            DisplayStructuralAreaOrder::BeforeExisting => self
                .output_render
                .current_row_output()
                .apply_current_row_mutation(TakeGlyphAreaMutation { area })
                .unwrap_or_default(),
            DisplayStructuralAreaOrder::AfterExisting => Vec::new(),
        };

        let mut source_state = DisplayRowSourceState::with_face_scope(face_scope);
        match emission.content() {
            DisplayMarginEmissionContent::String(value) => {
                if let Some(mut source) = LispStringSourceCursor::new(
                    0x6d61_7267,
                    *value,
                    RenderFaceRef::FaceId(margin_face_id),
                    LispStringSourceOrigin::MarginDisplayReplacement,
                ) {
                    let _ = self.render_natural_fragment_into_current_row(
                        fragment,
                        &mut source,
                        &mut source_state,
                        face_ids,
                    );
                }
            }
            DisplayMarginEmissionContent::Item(kind) => {
                let item = DisplayItem::new(
                    SourceSpan::synthetic(0x6d61_7267, 0, 1),
                    RenderFaceRef::FaceId(margin_face_id),
                    kind.clone(),
                );
                let mut source = DisplayItemSegmentSource::new(item);
                let _ = self.render_natural_fragment_into_current_row(
                    fragment,
                    &mut source,
                    &mut source_state,
                    face_ids,
                );
            }
        }
        if structural_order == DisplayStructuralAreaOrder::BeforeExisting {
            self.output_render
                .current_row_output()
                .apply_current_row_mutation(FillMarginLaneToCapacityMutation {
                    area,
                    capacity,
                    face_id: margin_face_id,
                });
        }
        if !trailing_glyphs.is_empty() {
            self.output_render
                .current_row_output()
                .apply_current_row_mutation(AppendGlyphAreaMutation {
                    area,
                    glyphs: trailing_glyphs,
                });
        }
    }

    pub(crate) fn render_display_item_source_into_current_text_row_and_emit<
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    >(
        &mut self,
        face_ids: &mut FrameFaceAttempt,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        request: DisplayRowAppendSourceRenderRequest<'_>,
        render_policy: &mut P,
    ) -> Option<CurrentTextRowRenderOutcome> {
        let mut state = current_text_render_state(self, face_ids);
        let (result, output) = request.render_with_row_request(|row_request| {
            render_display_item_source_into_current_text_row(
                &mut state,
                source,
                source_state,
                row_request,
                render_policy,
            )
        });
        let result = result?;
        Some(
            self.output_render
                .finish_current_text_row_render(output, result),
        )
    }

    pub(crate) fn mark_current_text_row_truncated_left(&mut self) {
        self.output_render()
            .install_row_decoration(TextWindowRowDecorationRequest::MarkCurrentTruncatedLeft);
    }

    /// Fill the current row's background from the current pen `x` to the
    /// text-area `right_edge` with the active `:extend` face (GNU
    /// `extend_face_to_end_of_line`). No-op (returns `false`) when the row is
    /// width is non-positive, or when [`line_end::extend_fill_runs`] declines
    /// — which, per GNU's
    /// `FRAME_WINDOW_P` guard (xdisp.c:24388), can happen for an
    /// invisible background only on a window-system row, never on a terminal
    /// one.
    pub(crate) fn extend_face_to_end_of_line(
        &mut self,
        row_extend: &DisplayRowExtendState,
        row_geometry: &DisplayRowGeometryState,
        current_x: f32,
        right_edge: f32,
        frame_background: Color,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    ) -> bool {
        let fill_px = right_edge - current_x;
        if fill_px <= 0.0 {
            return false;
        }
        let extend_face = row_extend.value_on(row_geometry).copied();
        let extend = extend_face.map(|face| LineEndExtend {
            bg: face.background(),
            face_id: face.face_id(),
        });
        // Same rule as the line-end seam, encoded once: the frame-background
        // skip is GNU's FRAME_WINDOW_P-guarded early return (xdisp.c:24388) and
        // never applies to a terminal row.
        if !line_end::extend_fill_runs(self.measurement_mode(), extend, frame_background) {
            return false;
        }
        let LineEndExtend { bg, face_id } =
            extend.expect("extend_fill_runs returns false without an extend face");
        let metrics = extend_face
            .expect("extend_fill_runs returns false without an extend face")
            .metrics();
        self.output_render.extend_current_row_face_to_end_of_line(
            RowExtendFill::new(
                bg,
                face_id,
                fill_px,
                metrics.row_height(),
                metrics.ascent(),
                metrics.char_width(),
            )
            .with_box_vertical_edges(box_vertical_edges),
        )
    }

    /// Render the GNU line-end sequence for the current row through the
    /// shared [`crate::display_row::line_end`] seam: the caller builds the
    /// [`LineEndContext`], [`line_end::plan`] decides the ordered effects,
    /// this method resolves their faces against the frame face services and
    /// applies them to the current row.
    pub(crate) fn render_line_end(
        &mut self,
        ctx: &LineEndContext,
        geometry: LineEndFillGeometry,
        face_ids: &mut FrameFaceAttempt,
    ) {
        let plan = line_end::plan(ctx);
        if plan.steps().is_empty() {
            return;
        }
        let resolved = {
            let mut resolver = TextRowLineEndFaceResolver {
                render: self,
                face_ids,
            };
            plan.resolve(ctx, geometry, &mut resolver)
        };
        self.output_render
            .current_row_output()
            .apply_current_row_mutation(resolved);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_display_property_replacement_row_request(
        &mut self,
        descriptor: &DisplayPropertyReplacementDescriptor,
        source_text: &[u8],
        active_face_state: &DisplayRowActiveFaceState,
        current_x: f32,
        content_x: f32,
        params: &WindowParams,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        start_position: DisplayRowPosition,
    ) -> Option<DisplayPropertyReplacementRowRenderRequest> {
        DisplayPropertyReplacementRowRenderRequest::from_typed_replacement_descriptor(
            descriptor,
            source_text,
            active_face_state,
            self.font_metrics,
            current_x,
            content_x,
            params,
            self.output_render.display_host(),
            glyph_y_offset,
            fallback_metrics,
            start_position,
        )
    }

    /// Record a `(left-fringe …)` / `(right-fringe …)` fringe-bitmap descriptor
    /// on the current row, if the typed replacement is a fringe spec. The text
    /// area still shows nothing (the replacement resolves to `Empty`); this only
    /// attaches the bitmap so the frame-output bridge can draw it in the fringe.
    pub(crate) fn record_fringe_bitmap_for_descriptor(
        &mut self,
        descriptor: &DisplayPropertyReplacementDescriptor,
        face_ids: &mut FrameFaceAttempt,
        active_face_state: &DisplayRowActiveFaceState,
    ) {
        if let Some(DisplayReplacementProperty::Fringe(layout)) =
            descriptor.classification().replacement()
        {
            let layout = *layout;
            self.record_fringe_bitmap_layout(&layout, face_ids, active_face_state.face_id());
        }
    }

    /// Record a parsed fringe layout (from any source path) on the current row.
    /// `fallback_face_id` is the row's active face, used only when neither a
    /// `set-fringe-bitmap-face` override nor the spec's FACE resolves.
    ///
    /// Resolution honors GNU's `set-fringe-bitmap-face` override: the face name
    /// stored on the registry entry wins over the spec's FACE argument.
    pub(crate) fn record_fringe_bitmap_layout(
        &mut self,
        layout: &crate::display_spec::DisplayFringeLayout,
        face_ids: &mut FrameFaceAttempt,
        fallback_face_id: FaceId,
    ) {
        let layout = *layout;

        // Resolve the bitmap symbol -> registry index, and capture the registry
        // face override name (GC-safe String) before borrowing self mutably.
        let evaluator = self.output_render.evaluator();
        let (bitmap_index, registry_face_name) =
            match evaluator.fringe_bitmap_for_symbol(layout.bitmap) {
                Some((index, bitmap)) => {
                    if index > u32::from(u16::MAX) {
                        return;
                    }
                    (index as u16, bitmap.face.clone())
                }
                // No registered user bitmap (e.g. a standard built-in we don't
                // implement yet): nothing to draw.
                None => return,
            };

        // The face id: prefer the `set-fringe-bitmap-face` override, then the
        // spec's FACE, then the row's active face.
        let face_id = self.resolve_fringe_face_id(
            registry_face_name.as_deref(),
            layout.face,
            face_ids,
            fallback_face_id,
        );

        let info = FringeBitmapInfo {
            bitmap_index,
            face_id,
        };
        let side = layout.side;
        self.output_render
            .current_row_output()
            .apply_current_row_mutation(SetRowFringeBitmapMutation { side, info });
    }

    /// Resolve the face id used for a fringe bitmap. `override_name` is the
    /// `set-fringe-bitmap-face` registry override (highest priority); `spec_face`
    /// is the FACE from the display spec; the active row face is the fallback.
    fn resolve_fringe_face_id(
        &mut self,
        override_name: Option<&str>,
        spec_face: Option<Value>,
        face_ids: &mut FrameFaceAttempt,
        fallback_face_id: FaceId,
    ) -> FaceId {
        if let Some(name) = override_name {
            let resolved = self.faces.resolve_named_face(name);
            let face_id =
                crate::display_row::face_state::stable_face_id_for_resolved(face_ids, &resolved);
            self.insert_resolved_face(face_id, &resolved);
            return face_id;
        }
        if let Some(face_value) = spec_face
            && let Some(resolved) = self
                .faces
                .resolve_face_value_over(&self.faces.default_face(), &face_value)
        {
            let face_id =
                crate::display_row::face_state::stable_face_id_for_resolved(face_ids, &resolved);
            self.insert_resolved_face(face_id, &resolved);
            return face_id;
        }
        fallback_face_id
    }

    pub(crate) fn output_emitter(&mut self) -> &mut WindowOutputEmitter {
        self.output_render.output_emitter()
    }

    pub(crate) fn output_emitter_ref(&self) -> &WindowOutputEmitter {
        self.output_render.output_emitter_ref()
    }

    /// Capture the current row's drawn-glyph counts at a word-wrap candidate so
    /// the eventual break can roll the partial word off the row.
    pub(crate) fn capture_glyph_checkpoint(&self) -> DisplayRowGlyphCheckpoint {
        self.output_render.capture_current_row_glyph_checkpoint()
    }

    pub(crate) fn first_text_glyph_after_checkpoint(
        &mut self,
        checkpoint: DisplayRowGlyphCheckpoint,
    ) -> Option<neomacs_display_protocol::glyph_matrix::Glyph> {
        self.output_render
            .current_row_output()
            .apply_current_row_mutation(FirstTextGlyphAfterCheckpointMutation { checkpoint })?
    }

    /// Roll the current row's drawn glyphs back to `checkpoint` when the
    /// word-wrap break rewinds to a word boundary.
    pub(crate) fn restore_glyph_checkpoint(&mut self, checkpoint: DisplayRowGlyphCheckpoint) {
        self.output_render
            .restore_current_row_glyph_checkpoint(checkpoint);
    }

    /// The source-derived end terminal owned by the last glyph that remains
    /// on the current visual row. GNU restores the iterator to that element
    /// before extending a wrapped row; the filler may inherit its right side,
    /// but never starts a second run of its own.
    pub(crate) fn trailing_box_run_terminal(
        &mut self,
    ) -> neomacs_display_protocol::face::BoxVerticalEdges {
        self.output_render
            .current_row_output()
            .apply_current_row_mutation(TrailingBoxRunTerminalMutation)
            .unwrap_or(neomacs_display_protocol::face::BoxVerticalEdges::Neither)
    }

    pub(crate) fn output_rows(&self) -> &[DisplayRowSnapshot] {
        self.output_render.output_rows()
    }

    pub(crate) fn output_rows_len(&self) -> usize {
        self.output_render.output_rows_len()
    }
}

fn current_text_render_state<'emit>(
    state: &'emit mut TextRowSourceRenderState<'_>,
    face_ids: &'emit mut FrameFaceAttempt,
) -> DisplayRowCurrentTextSourceState<'emit, 'emit> {
    state.output_render.current_text_render_state(
        state.font_metrics,
        state.measurement_mode,
        state.faces.pipeline_resolver(),
        face_ids,
    )
}

pub(crate) struct TextRowSourceMeasureState<'a> {
    row_output: DisplayRowCurrentRowOutput<'a>,
    evaluator: &'a mut Context,
    font_metrics: &'a mut Option<FontMetricsService>,
    measurement_mode: DisplayRowMeasurementMode,
    face_resolver: &'a FaceResolver,
}

impl<'a> TextRowSourceMeasureState<'a> {
    #[cfg(test)]
    pub(crate) fn from_current_row(
        row_output: DisplayRowCurrentRowOutput<'a>,
        evaluator: &'a mut Context,
        font_metrics: &'a mut Option<FontMetricsService>,
        face_resolver: &'a FaceResolver,
    ) -> Self {
        Self {
            row_output,
            evaluator,
            font_metrics,
            measurement_mode: DisplayRowMeasurementMode::LogicalCells,
            face_resolver,
        }
    }

    pub(crate) fn font_metrics(&mut self) -> &mut Option<FontMetricsService> {
        self.font_metrics
    }

    pub(crate) fn current_cluster_tail(&self) -> Option<(char, bool)> {
        self.row_output.cluster_tail()
    }

    pub(crate) fn measure_display_item_source_against_current_text_row<
        S: DisplayItemSource,
        P: DisplayRowRenderPolicy,
    >(
        &mut self,
        face_ids: &mut FrameFaceAttempt,
        source: &mut S,
        source_state: &mut DisplayRowSourceState,
        row_request: DisplayRowSourceRenderRequest<'_>,
        render_policy: &mut P,
    ) -> Option<CurrentTextRowRenderOutcome> {
        let mut state = current_text_measure_state(self, face_ids);
        measure_display_item_source_against_current_text_row(
            &mut state,
            source,
            source_state,
            row_request,
            render_policy,
        )
    }
}

fn current_text_measure_state<'emit>(
    state: &'emit mut TextRowSourceMeasureState<'_>,
    face_ids: &'emit mut FrameFaceAttempt,
) -> DisplayRowCurrentTextSourceState<'emit, 'emit> {
    DisplayRowCurrentTextSourceState::new(
        state.row_output.reborrow(),
        state.evaluator,
        state.font_metrics,
        state.measurement_mode,
        state.face_resolver,
        face_ids,
    )
}

#[cfg(test)]
#[path = "source_render_test.rs"]
mod tests;
