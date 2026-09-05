//! Buffer source render-attempt state, retry, and publish lifecycle.

use crate::buffer_source::tail_render::{
    BufferSourcePostLoopRenderOutcome, BufferSourceRetryBounds,
};
use crate::buffer_source::window_source::ResolvedWindowStart;
use crate::coords::layout_i64_char_pos_to_lisp_char_pos;
use crate::display_face_policy::EffectiveWindowDefaultFace;
use crate::display_frame_output::FrameOutputOwner;
use crate::display_row::face_environment::WindowFaces;
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::display_row::source_render::{TextRowOutputRenderState, TextRowSourceRenderState};
use crate::display_text_window_row_lifecycle::{
    TextWindowBeginRequest, TextWindowCursorEffectsRequest, TextWindowVisibilityRetryOutcome,
};
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::incremental_layout::ReusedMatrixRows;
use crate::layout_effect::{LayoutEffect, WindowScrollEffect, WindowScrollHookSite};
use crate::neovm_bridge::FaceResolver;
use crate::types::WindowParams;
use crate::viewport_resolution::ViewportDecision;
use crate::window_output::{
    TextWindowOutputRetryCheckpoint, TextWindowOutputTarget, TextWindowRedisplayPositions,
    WindowOutputEmitter, capture_text_window_retry_checkpoint,
    restore_text_window_retry_checkpoint,
};
use neovm_core::buffer::TextPositionAnchor;
use neovm_core::emacs_core::Context;
use neovm_core::window::{FrameId, WindowId, WindowPresentationSnapshot};

pub(crate) struct BufferSourceOutputState<'emit> {
    output: TextWindowOutputTarget<'emit>,
    evaluator: &'emit mut Context,
}

pub(crate) struct BufferSourceRenderAttemptContext<'a, 'face> {
    output: BufferSourceOutputState<'a>,
    font_metrics: &'a mut Option<FontMetricsService>,
    face_resolver: &'face FaceResolver,
    face_attempt: FrameFaceAttempt,
    window_snapshots: &'a mut Vec<WindowPresentationSnapshot>,
}

/// Which live window state may be published by a completed row walk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WindowPositionPublication {
    #[default]
    Redisplay,
    /// This logical redisplay already acknowledged the window's scroll hook.
    /// Physical convergence retries may still commit a corrected live start,
    /// but they must not replay the Lisp callback.
    RedisplayResumedScrollHook,
    /// Redisplay's preliminary GNU `resize_mini_window` measurement.
    ///
    /// The row walk is lifted to `max-mini-window-height`. If it reaches ZV,
    /// point visibility relative to the old physical one-line allocation must
    /// not scroll the start chosen by the resize measurement.
    RedisplayMinibufferMeasurement,
    /// GNU's inactive echo-area walk temporarily substitutes the echo buffer
    /// without entering redisplay_window. It publishes neither the temporary
    /// start/end nor a window-scroll-functions effect to the live minibuffer.
    InactiveEchoArea,
    SynchronousQueryEnd,
}

impl WindowPositionPublication {
    /// A logical `window-end` query walks from the live start marker exactly.
    ///
    /// Redisplay may resolve a different source start to keep point visible;
    /// GNU `Fwindow_end` does not run that viewport policy.
    pub(crate) const fn uses_exact_window_start(self) -> bool {
        matches!(self, Self::InactiveEchoArea | Self::SynchronousQueryEnd)
    }

    /// A synchronous GNU `window-end` query walks only the text area using
    /// the chrome dimensions accepted by the last redisplay.  It must not
    /// evaluate mode/header/tab-line Lisp while answering the query.
    pub(crate) const fn is_synchronous_query(self) -> bool {
        matches!(self, Self::SynchronousQueryEnd)
    }

    pub(crate) const fn keeps_complete_minibuffer_measurement_start(self) -> bool {
        matches!(self, Self::RedisplayMinibufferMeasurement)
    }

    /// Commit the candidate start before entering any Lisp-backed layout
    /// service, matching GNU `redisplay_window`'s force/scroll/recenter sites.
    pub(crate) fn publish_window_start(
        self,
        evaluator: &mut Context,
        frame_id: FrameId,
        window_id: WindowId,
        window_start: ResolvedWindowStart,
    ) -> Option<LayoutEffect> {
        let window_start_lisp = layout_i64_char_pos_to_lisp_char_pos(window_start.get());
        match self {
            Self::Redisplay => {
                let commit = evaluator.publish_redisplay_window_start(
                    frame_id,
                    window_id,
                    window_start_lisp,
                );
                if commit.runs_window_scroll_functions()
                    && evaluator.window_scroll_functions_may_run(window_id)
                {
                    return Some(LayoutEffect::RunWindowScrollFunctions(
                        WindowScrollEffect::new(WindowScrollHookSite::new(window_id, window_start)),
                    ));
                }
            }
            Self::RedisplayResumedScrollHook | Self::RedisplayMinibufferMeasurement => {
                let _ = evaluator.publish_redisplay_window_start(
                    frame_id,
                    window_id,
                    window_start_lisp,
                );
            }
            Self::InactiveEchoArea | Self::SynchronousQueryEnd => {}
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceRedisplayPublishRequest {
    frame_id: FrameId,
    window_id: WindowId,
    buffer_z: TextPositionAnchor,
    publication: WindowPositionPublication,
}

#[derive(Debug, PartialEq)]
pub(crate) enum BufferSourceRenderAttemptOutcome {
    Skipped,
    /// Lisp evaluated by a buffer-owned display source changed the window's
    /// source projection. The frame transaction must discard this attempt and
    /// recollect the live window/buffer pair.
    LogicalInputsChanged,
    Retry {
        window_start: i64,
    },
    /// Resolve a viewport decision that cannot finish inside the row producer:
    /// either measure more display rows from a transient, unpublished probe or
    /// place the final start relative to point through core display motion.
    ResolveViewport {
        decision: ViewportDecision,
    },
    /// The window start is forced (GNU `w->force_start`): keep it and re-lay
    /// with POINT moved to the last fully-visible position instead of
    /// recomputing the start around point (GNU redisplay_window's
    /// force_start branch moves point when the cursor row is off-window).
    RetryPointIntoWindow {
        /// Layout 0-based charpos point should move to.
        point_charpos: i64,
    },
    /// A bounded fast-path walk failed its post-walk validation
    /// (`ScrollReplay::expected_walk`): the regenerated span did not sync back
    /// up with the reused rows. The caller must re-lay this window with no
    /// replay plan; the checkpoint was already restored.
    ReplayMispredicted,
    Finished {
        redisplay_positions: TextWindowRedisplayPositions,
        window_end_record: neovm_core::window::WindowEndRecord,
        /// Exact canonical inputs after body production and immediately before
        /// GNU's late `display_mode_lines` phase enters Lisp.
        freshness_before_chrome: neovm_core::window::WindowLayoutAttemptFreshness,
        /// The exact window-local default face used to produce this accepted
        /// body. Late TTY padding must consume this artifact rather than
        /// independently falling back to the frame default.
        effective_default_face: EffectiveWindowDefaultFace,
        /// Whether this window took the Phase 1 cursor-only fast path (body rows
        /// reused verbatim) rather than a full body walk.
        cursor_only: bool,
        /// Exact matrix rows installed from an incremental replay.  A localized
        /// edit can reuse rows on both sides of its regenerated span, so a
        /// prefix count cannot faithfully carry this provenance.
        reused_matrix_rows: Option<ReusedMatrixRows>,
        /// Pixel extent this accepted walk reserved for the line-number field
        /// (`LineNumberFieldLayout::extent`), zero when it reserved none.  The
        /// window metadata is published after the walk returns, so the accepted
        /// outcome is the only carrier that has already seen the measurement.
        line_number_field_width: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceRetryPlan {
    window_id: i64,
    window_start: i64,
    point_charpos: i64,
    charpos_end: i64,
    rendered_rows_len: usize,
    retry_bounds: BufferSourceRetryBounds,
    retry: TextWindowVisibilityRetryOutcome,
}

impl<'emit> BufferSourceOutputState<'emit> {
    pub(crate) fn from_parts(
        output: TextWindowOutputTarget<'emit>,
        evaluator: &'emit mut Context,
    ) -> Self {
        Self { output, evaluator }
    }

    pub(crate) fn capture_retry_checkpoint(&mut self) -> TextWindowOutputRetryCheckpoint {
        capture_text_window_retry_checkpoint(self.output.reborrow())
    }

    pub(crate) fn restore_retry_checkpoint(&mut self, checkpoint: TextWindowOutputRetryCheckpoint) {
        restore_text_window_retry_checkpoint(self.output.reborrow(), checkpoint);
    }

    pub(crate) fn evaluator(&mut self) -> &mut Context {
        self.evaluator
    }

    pub(crate) fn output_target(&mut self) -> TextWindowOutputTarget<'_> {
        self.output.reborrow()
    }

    pub(crate) fn into_parts(self) -> (TextWindowOutputTarget<'emit>, &'emit mut Context) {
        (self.output, self.evaluator)
    }

    pub(crate) fn install_cursor_effects(&mut self, params: &WindowParams) -> bool {
        TextWindowCursorEffectsRequest::new(params.window_id, params.cursor_effects.clone())
            .install_and_apply(self.output.reborrow())
    }

    pub(crate) fn begin_text_window_output(
        &mut self,
        begin_request: TextWindowBeginRequest,
    ) -> WindowOutputEmitter {
        begin_request.begin_and_apply(self.output.reborrow(), self.evaluator)
    }

    pub(crate) fn source_render_state<'output>(
        &'output mut self,
        output_emitter: &'output mut WindowOutputEmitter,
        font_metrics: &'output mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        faces: WindowFaces<'output>,
    ) -> TextRowSourceRenderState<'output> {
        TextRowSourceRenderState::from_output_render(
            TextRowOutputRenderState::from_parts(
                self.output.reborrow(),
                output_emitter,
                self.evaluator,
            ),
            font_metrics,
            measurement_mode,
            faces,
        )
    }
}

impl<'a, 'face> BufferSourceRenderAttemptContext<'a, 'face> {
    pub(crate) fn new(
        output: TextWindowOutputTarget<'a>,
        evaluator: &'a mut Context,
        font_metrics: &'a mut Option<FontMetricsService>,
        face_resolver: &'face FaceResolver,
        face_attempt: FrameFaceAttempt,
        window_snapshots: &'a mut Vec<WindowPresentationSnapshot>,
    ) -> Self {
        Self {
            output: BufferSourceOutputState::from_parts(output, evaluator),
            font_metrics,
            face_resolver,
            face_attempt,
            window_snapshots,
        }
    }

    pub(crate) fn from_frame_output_owner(
        frame_output: &'a mut FrameOutputOwner,
        evaluator: &'a mut Context,
        font_metrics: &'a mut Option<FontMetricsService>,
        face_resolver: &'face FaceResolver,
        face_attempt: FrameFaceAttempt,
        window_snapshots: &'a mut Vec<WindowPresentationSnapshot>,
    ) -> Self {
        Self::new(
            frame_output.text_window_output_target(),
            evaluator,
            font_metrics,
            face_resolver,
            face_attempt,
            window_snapshots,
        )
    }

    pub(crate) fn output_mut(&mut self) -> &mut BufferSourceOutputState<'a> {
        &mut self.output
    }

    pub(crate) fn with_face_services<R>(
        &mut self,
        f: impl FnOnce(&FaceResolver, &mut Option<FontMetricsService>) -> R,
    ) -> R {
        f(self.face_resolver, self.font_metrics)
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        BufferSourceOutputState<'a>,
        &'a mut Option<FontMetricsService>,
        &'face FaceResolver,
        FrameFaceAttempt,
        &'a mut Vec<WindowPresentationSnapshot>,
    ) {
        (
            self.output,
            self.font_metrics,
            self.face_resolver,
            self.face_attempt,
            self.window_snapshots,
        )
    }
}

impl BufferSourceRedisplayPublishRequest {
    pub(crate) fn new(
        frame_id: FrameId,
        window_id: WindowId,
        buffer_z: TextPositionAnchor,
        publication: WindowPositionPublication,
    ) -> Self {
        Self {
            frame_id,
            window_id,
            buffer_z,
            publication,
        }
    }

    pub(crate) fn publish_window_end(
        self,
        evaluator: &mut Context,
        positions: TextWindowRedisplayPositions,
    ) {
        let window_end = self.window_end_record(positions);
        match self.publication {
            WindowPositionPublication::Redisplay
            | WindowPositionPublication::RedisplayResumedScrollHook
            | WindowPositionPublication::RedisplayMinibufferMeasurement => {
                evaluator.publish_redisplay_window_end(self.frame_id, self.window_id, window_end);
            }
            // GNU's `Fwindow_end` uses a stack-local display iterator.  The
            // answer belongs to `WindowLayoutQuery`, not retained redisplay
            // state, so a query nested inside a hook cannot validate a
            // discarded attempt.
            WindowPositionPublication::InactiveEchoArea
            | WindowPositionPublication::SynchronousQueryEnd => {}
        }
    }

    pub(crate) const fn frame_id(self) -> FrameId {
        self.frame_id
    }

    pub(crate) fn window_end_record(
        self,
        positions: TextWindowRedisplayPositions,
    ) -> neovm_core::window::WindowEndRecord {
        let window_end = positions.window_end_position();
        neovm_core::window::WindowEndRecord::from_anchors(
            self.buffer_z,
            window_end.anchor(),
            window_end.matrix_row(),
        )
    }
}

impl BufferSourceRetryPlan {
    pub(crate) fn from_post_loop(
        window_id: i64,
        window_start: i64,
        point_charpos: i64,
        charpos_end: i64,
        retry_bounds: BufferSourceRetryBounds,
        post_loop: BufferSourcePostLoopRenderOutcome,
    ) -> Self {
        Self {
            window_id,
            window_start,
            point_charpos,
            charpos_end,
            rendered_rows_len: post_loop.rendered_rows_len,
            retry_bounds,
            retry: post_loop.retry,
        }
    }

    pub(crate) fn log_visibility_adjustments(&self) {
        if self.retry.scroll_down_window_start().is_some() {
            tracing::debug!(
                "layout_window_rust: point={} beyond visible_end={:?} (charpos_end={}), visible_rows={}, new_window_start={:?}",
                layout_i64_char_pos_to_lisp_char_pos(self.point_charpos).as_i64(),
                self.retry.visible_end_lisp(),
                self.charpos_end,
                self.rendered_rows_len,
                self.retry.scroll_down_window_start()
            );
        }
        if self.retry.point_row_window_start().is_some() {
            tracing::debug!(
                "layout_window_rust: point={} row partially visible within {}..{}, new_window_start={:?}",
                self.point_charpos,
                self.retry_bounds.text_area_top(),
                self.retry_bounds.text_area_bottom(),
                self.retry.point_row_window_start()
            );
        }
        if self.retry.point_line_window_start().is_some() {
            tracing::debug!(
                "layout_window_rust: point={} line continues below final visible row, new_window_start={:?}",
                self.point_charpos,
                self.retry.point_line_window_start()
            );
        }
    }

    pub(crate) fn retry_window_start(&self) -> Option<i64> {
        self.retry.retry_window_start()
    }

    pub(crate) fn viewport_resolution(
        &self,
        remaining_visibility_retries: usize,
    ) -> Option<ViewportDecision> {
        match self.retry.viewport_decision() {
            ViewportDecision::NeedMoreMeasurement(measurement)
                if remaining_visibility_retries > 0 =>
            {
                Some(ViewportDecision::NeedMoreMeasurement(measurement))
            }
            ViewportDecision::NeedMoreMeasurement(measurement) => {
                Some(measurement.fallback_placement())
            }
            decision @ ViewportDecision::PlaceRelativeToPoint { .. }
                if remaining_visibility_retries > 0 =>
            {
                Some(decision)
            }
            ViewportDecision::Keep
            | ViewportDecision::Commit { .. }
            | ViewportDecision::PlaceRelativeToPoint { .. } => None,
        }
    }

    /// Target for GNU's force_start point move: the last fully-visible
    /// buffer position of the attempt just laid out (layout 0-based), i.e.
    /// point lands on the final visible row of the kept window start.
    pub(crate) fn forced_start_point_target(&self) -> Option<i64> {
        self.retry
            .visible_end_lisp()
            .map(|pos| pos.as_i64() - 1)
            .filter(|charpos| *charpos >= 0)
    }

    pub(crate) fn should_retry(&self, remaining_visibility_retries: usize) -> Option<i64> {
        self.retry_window_start().filter(|new_window_start| {
            remaining_visibility_retries > 0
                && *new_window_start > self.retry.semantic_window_start()
        })
    }

    pub(crate) fn log_retry(&self, new_window_start: i64, remaining_visibility_retries: usize) {
        tracing::debug!(
            "layout_window_rust: retrying window {} with adjusted window_start {} -> {} (remaining={})",
            self.window_id,
            self.window_start,
            new_window_start,
            remaining_visibility_retries
        );
    }
}
