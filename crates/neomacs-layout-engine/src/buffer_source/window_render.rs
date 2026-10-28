//! Buffer window source rendering requests and actions.
use crate::buffer_source::body_render::BufferSourceWalkSetupRequest;
#[cfg(test)]
pub(crate) use crate::buffer_source::display_property_render::BufferDisplayPropertyTextReplacementOutcome;
use crate::buffer_source::render_attempt::WindowPositionPublication;
pub(crate) use crate::buffer_source::render_attempt::{
    BufferSourceRenderAttemptContext, BufferSourceRenderAttemptOutcome,
};
use crate::buffer_source::render_plan::{BufferSourceDefaultFacePlan, BufferSourceOutputSetup};
use crate::buffer_source::window_geometry::{
    BufferWindowGeometryPlan, BufferWindowGeometryRequest, BufferWindowLocalDisplayPolicy,
};
use crate::buffer_source::window_source::{BufferWindowSourceRequest, ResolvedWindowStart};
use crate::display_row::face_environment::FrameFaces;
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_status_line::{
    WindowChromeRowsPlan, max_mini_window_lines, max_mini_window_lines_for_buffer,
};
use crate::neovm_bridge::{LayoutBufferView, RustBufferAccess};
use crate::types::{FrameParams, WindowParams};
use crate::viewport_resolution::ForwardViewportMeasurement;
use crate::window_layout::WindowLayoutBox;
use neovm_core::buffer::{BufferId, CharLen};
use neovm_core::window::{FrameId, WindowId};

/// Attempt-owned numeric read policy. An artificial horizon retry keeps
/// the exact admitted replay and reads its original window-row budget; it
/// does not repeat outer Lisp preparation or change process/TLS policy.
/// Each exclusive source walk owns its selector; independent mutators share no mutable state.
#[derive(Clone, Copy)]
enum SyncSourceRead {
    AllowHorizon,
    UncappedRetry,
}

/// How a bounded viewport read stops. The two modes differ only in whether
/// the newline-existence query runs first: a synchronous query copies its
/// whole geometry allowance outright, while a live viewport walk asks whether
/// the allowance even holds the rows a viewport can show and copies only
/// through the last preserved newline when it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewportSourceReadMode {
    /// Copy exactly the geometry character allowance.
    CharHorizon,
    /// Stop after the row budget's preserved newlines, never past the
    /// geometry character allowance. Reaching the allowance is a horizon the
    /// retry loop extends, never an end of buffer.
    RowsWithinChars,
}

/// Acquisition allowance owned by one viewport render attempt. Its numeric
/// geometry estimate limits copied source, never ZV or visible geometry. Each
/// mutator grows its own allowance when the canonical walk needs more source.
#[derive(Clone, Copy)]
struct WindowTextReadBudget {
    chars: CharLen,
    mode: ViewportSourceReadMode,
}

impl WindowTextReadBudget {
    fn initial(params: &WindowParams, mode: ViewportSourceReadMode) -> Self {
        let cols = (params.text_bounds.width / params.char_width.max(1.0))
            .ceil()
            .max(1.0) as usize;
        let chars = params
            .source_interpretation_rows()
            .saturating_add(2)
            .saturating_mul(cols)
            .saturating_add(1)
            .max(1);
        Self {
            chars: CharLen::new(chars),
            mode,
        }
    }

    fn grow(self) -> Self {
        Self {
            chars: CharLen::new(self.chars.get().saturating_mul(2)),
            mode: self.mode,
        }
    }

    fn apply(self, request: BufferWindowSourceRequest) -> BufferWindowSourceRequest {
        match self.mode {
            ViewportSourceReadMode::CharHorizon => request.with_window_chars(self.chars),
            ViewportSourceReadMode::RowsWithinChars => request.with_window_rows_within(self.chars),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Force the initial viewport source allowance (tests only). Numeric
    /// witness for the bounded read and its re-acquisition; no Lisp state is
    /// retained and the shipping build has no such override.
    static VIEWPORT_READ_ALLOWANCE_OVERRIDE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
    /// Viewport source re-acquisitions on this thread (tests only).
    static VIEWPORT_READ_RETRIES: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

/// Force the initial viewport source allowance on this thread (tests only).
#[cfg(test)]
pub(crate) fn set_viewport_read_allowance_for_test(chars: Option<usize>) {
    VIEWPORT_READ_ALLOWANCE_OVERRIDE.with(|cell| cell.set(chars));
}

#[cfg(test)]
pub(crate) fn viewport_read_retries_for_test() -> u64 {
    VIEWPORT_READ_RETRIES.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_viewport_read_retries_for_test() {
    VIEWPORT_READ_RETRIES.with(|cell| cell.set(0));
}

pub(crate) struct BufferWindowRenderRequest<'a, B>
where
    B: LayoutBufferView,
{
    frame_id: FrameId,
    window_id: WindowId,
    params: &'a WindowParams,
    frame_params: &'a FrameParams,
    layout_box: &'a WindowLayoutBox,
    buffer_id: BufferId,
    buffer: &'a B,
    buffer_name: &'a str,
    reserve_right_border_col: bool,
    position_publication: WindowPositionPublication,
    /// Evaluate and measure chrome on this attempt. Set for a Position query's
    /// first pass; its measured-metric retry clears it.
    remeasure_chrome: bool,
    resolved_window_start: ResolvedWindowStart,
    forward_viewport_measurement: Option<ForwardViewportMeasurement>,
}

impl<'a, B> BufferWindowRenderRequest<'a, B>
where
    B: LayoutBufferView,
{
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        frame_id: FrameId,
        window_id: WindowId,
        params: &'a WindowParams,
        frame_params: &'a FrameParams,
        layout_box: &'a WindowLayoutBox,
        buffer_id: BufferId,
        buffer: &'a B,
        buffer_name: &'a str,
        reserve_right_border_col: bool,
        resolved_window_start: ResolvedWindowStart,
    ) -> Self {
        Self {
            frame_id,
            window_id,
            params,
            frame_params,
            layout_box,
            buffer_id,
            buffer,
            buffer_name,
            reserve_right_border_col,
            position_publication: WindowPositionPublication::Redisplay,
            remeasure_chrome: false,
            resolved_window_start,
            forward_viewport_measurement: None,
        }
    }

    pub(crate) fn with_position_publication(
        mut self,
        publication: WindowPositionPublication,
    ) -> Self {
        self.position_publication = publication;
        self
    }

    pub(crate) fn with_remeasure_chrome(mut self, remeasure: bool) -> Self {
        self.remeasure_chrome = remeasure;
        self
    }

    pub(crate) fn with_forward_viewport_measurement(
        mut self,
        measurement: Option<ForwardViewportMeasurement>,
    ) -> Self {
        self.forward_viewport_measurement = measurement;
        self
    }

    /// A capped source that fails to synchronize retries here, after the
    /// leaf's fontification and display-when preparation. Re-entering the
    /// outer leaf would repeat fontification callbacks that leave fontified
    /// nil. No tail/chrome/end snapshot was published by the failed body.
    ///
    /// A bounded viewport read that runs out of source re-acquires here the
    /// same way, with its geometry allowance grown; the discarded attempt's
    /// unpublished window grid is dropped before the retaken body replaces it.
    pub(crate) fn render_into<FS: crate::neovm_bridge::FaceServices>(
        self,
        mut context: BufferSourceRenderAttemptContext<'_, '_, FS>,
        text_buf: &mut Vec<u8>,
        remaining_visibility_retries: usize,
        mut cursor_only: Option<crate::incremental_layout::CursorOnlyReplay>,
        scroll: Option<crate::incremental_layout::ScrollReplay>,
    ) -> BufferSourceRenderAttemptOutcome {
        if let Some(mode) = self.viewport_source_read_mode(scroll.is_none()) {
            // A live visibility/point retry re-enters the producer without
            // finishing the discarded attempt's window; its open grid was
            // never published, and the retaken body replaces it. Drop it so
            // the source-attempt checkpoint begins between completed windows.
            if let Some(owner) = context
                .output_mut()
                .output_target()
                .builder()
                .unpublished_window_grid_owner()
            {
                tracing::debug!(
                    "layout_window_rust: dropping unpublished grid window={} for window={}",
                    owner,
                    self.params.window_id
                );
                context
                    .output_mut()
                    .output_target()
                    .builder()
                    .discard_unpublished_window_grid(self.params.window_id as u64);
            }
            let mut budget = WindowTextReadBudget::initial(self.params, mode);
            #[cfg(test)]
            if let Some(chars) = VIEWPORT_READ_ALLOWANCE_OVERRIDE.with(std::cell::Cell::get) {
                budget.chars = CharLen::new(chars.max(1));
            }
            loop {
                let checkpoint = context
                    .output_mut()
                    .output_target()
                    .builder()
                    .capture_source_attempt_checkpoint(
                        neomacs_display_protocol::types::DisplayWindowId::new(
                            self.params.window_id,
                        ),
                    );
                let request = Self {
                    frame_id: self.frame_id,
                    window_id: self.window_id,
                    params: self.params,
                    frame_params: self.frame_params,
                    layout_box: self.layout_box,
                    buffer_id: self.buffer_id,
                    buffer: self.buffer,
                    buffer_name: self.buffer_name,
                    reserve_right_border_col: self.reserve_right_border_col,
                    position_publication: self.position_publication,
                    remeasure_chrome: self.remeasure_chrome,
                    resolved_window_start: self.resolved_window_start,
                    forward_viewport_measurement: self.forward_viewport_measurement.clone(),
                };
                let outcome = request.render_once(
                    context.reborrow(),
                    text_buf,
                    remaining_visibility_retries,
                    // A retained cursor-only body is consumed by its first
                    // attempt exactly once: it walks no source, so it can
                    // never exhaust the horizon, and a re-acquired body must
                    // be walked afresh.
                    cursor_only.take(),
                    None,
                    SyncSourceRead::AllowHorizon,
                    Some(budget),
                );
                if !matches!(
                    outcome,
                    BufferSourceRenderAttemptOutcome::QuerySourceHorizonExhausted
                ) {
                    return outcome;
                }
                context
                    .output_mut()
                    .output_target()
                    .builder()
                    .restore_source_attempt_checkpoint(checkpoint);
                #[cfg(test)]
                VIEWPORT_READ_RETRIES.with(|cell| cell.set(cell.get() + 1));
                budget = budget.grow();
            }
        }
        let uses_sync_budget = scroll
            .as_ref()
            .is_some_and(|replay| replay.edit && replay.sync.is_some())
            && crate::buffer_source::window_source::sync_source_budget_enabled();
        if !uses_sync_budget {
            return self.render_once(
                context,
                text_buf,
                remaining_visibility_retries,
                cursor_only,
                scroll,
                SyncSourceRead::AllowHorizon,
                None,
            );
        }
        let full_request = Self {
            frame_id: self.frame_id,
            window_id: self.window_id,
            params: self.params,
            frame_params: self.frame_params,
            layout_box: self.layout_box,
            buffer_id: self.buffer_id,
            buffer: self.buffer,
            buffer_name: self.buffer_name,
            reserve_right_border_col: self.reserve_right_border_col,
            position_publication: self.position_publication,
            remeasure_chrome: self.remeasure_chrome,
            resolved_window_start: self.resolved_window_start,
            forward_viewport_measurement: self.forward_viewport_measurement.clone(),
        };
        let checkpoint = context
            .output_mut()
            .output_target()
            .builder()
            .capture_source_attempt_checkpoint(
                neomacs_display_protocol::types::DisplayWindowId::new(self.params.window_id),
            );
        let outcome = self.render_once(
            context.reborrow(),
            text_buf,
            remaining_visibility_retries,
            cursor_only,
            scroll,
            SyncSourceRead::AllowHorizon,
            None,
        );
        if let BufferSourceRenderAttemptOutcome::SyncSourceHorizonExhausted { replay } = outcome {
            #[cfg(test)]
            crate::buffer_source::window_source::record_sync_source_budget_retry_for_test();
            context
                .output_mut()
                .output_target()
                .builder()
                .restore_source_attempt_checkpoint(checkpoint);
            // RetainedWindowKey does not own the buffer-local remapping
            // alist. A raw remap can leave admitted prefix glyphs resolved
            // against an older default/named face even though the namespace
            // generation is current. This rare retry must freshly resolve
            // every body face when remapping is present; restoring numeric
            // face IDs cannot establish those glyphs' semantic freshness.
            // Ordinary joined-line retries retain their exact edit replay.
            let replay = replay.filter(|_| {
                full_request
                    .buffer
                    .layout_buffer_local_value(crate::neovm_bridge::LayoutVar::FaceRemappingAlist)
                    .is_none_or(|alist| alist.is_nil())
            });
            full_request.render_once(
                context,
                text_buf,
                remaining_visibility_retries,
                None,
                replay.map(|replay| *replay),
                SyncSourceRead::UncappedRetry,
                None,
            )
        } else {
            outcome
        }
    }

    /// The bounded source mode for this attempt, when its acquisition may be
    /// limited by the viewport geometry.
    ///
    /// A synchronous query copies its geometry allowance outright; a plain
    /// live viewport walk (no edit replay to synchronize) asks the same
    /// allowance for the rows a viewport can show, so a newline-less tail is
    /// never walked and copied to ZV; a scroll replay keeps its own sync-stop
    /// horizon, and an `AccessibleEnd` (mini-window) measurement must observe
    /// the real ZV.
    fn viewport_source_read_mode(
        &self,
        plain_viewport_walk: bool,
    ) -> Option<ViewportSourceReadMode> {
        if self.params.source_extent != crate::types::WindowSourceExtent::Viewport
            // Selective indentation classification needs a complete indent.
            // Its current scanner has no incomplete-acquisition witness.
            || self.params.selective_display != 0
        {
            return None;
        }
        if self.position_publication.is_synchronous_query() {
            Some(ViewportSourceReadMode::CharHorizon)
        } else if plain_viewport_walk {
            Some(ViewportSourceReadMode::RowsWithinChars)
        } else {
            None
        }
    }

    fn render_once<FS: crate::neovm_bridge::FaceServices>(
        self,
        context: BufferSourceRenderAttemptContext<'_, '_, FS>,
        text_buf: &mut Vec<u8>,
        remaining_visibility_retries: usize,
        cursor_only: Option<crate::incremental_layout::CursorOnlyReplay>,
        scroll: Option<crate::incremental_layout::ScrollReplay>,
        sync_source_read: SyncSourceRead,
        window_text_budget: Option<WindowTextReadBudget>,
    ) -> BufferSourceRenderAttemptOutcome {
        let Self {
            frame_id,
            window_id,
            params,
            frame_params,
            layout_box,
            buffer_id,
            buffer,
            buffer_name,
            reserve_right_border_col,
            position_publication,
            remeasure_chrome,
            resolved_window_start,
            forward_viewport_measurement,
        } = self;
        let mut state = context;
        let buf_access = RustBufferAccess::new(buffer);
        state.output_mut().install_cursor_effects(params);

        let char_w = params.char_width;
        let char_h = params.char_height;
        let window_metrics = DisplayRowFallbackMetrics::from_window_defaults(params);
        let local_display_policy = BufferWindowLocalDisplayPolicy::from_window(buffer, params);

        let mut default_face = state.with_face_services(|face_resolver, font_metrics| {
            BufferSourceDefaultFacePlan::new(
                face_resolver.resolver(),
                buffer,
                font_metrics,
                DisplayRowMeasurementMode::from_frame_window_system(frame_params.window_system),
                window_metrics,
            )
        });
        state.realize_face_for_display(default_face.face_mut());
        let default_resolved = default_face.face();

        // GNU derives `lnum_pixel_width` from glyphs produced with the
        // window-resolved line-number face, not from FRAME_COLUMN_WIDTH.  Plan
        // the widest line-number role Neomacs can emit so every row keeps one
        // stable buffer-text origin even when face remapping changes font size.
        let line_number_cell_width = if params.display_line_numbers.enabled() {
            state.with_face_services(|face_resolver, font_metrics| {
                let faces = FrameFaces::new(face_resolver).for_window(buffer);
                crate::display_row::walk_state::LineNumberTextPrefixFace::ALL
                    .into_iter()
                    .map(|role| faces.resolve_named_face(role.face_name()))
                    .map(|face| {
                        font_metrics.as_mut().map_or(char_w, |service| {
                            service
                                .font_metrics(
                                    &face.font_family,
                                    face.font_weight,
                                    face.italic,
                                    face.font_size,
                                )
                                .char_width
                        })
                    })
                    .fold(char_w.max(1.0), f32::max)
            })
        } else {
            char_w
        };

        tracing::debug!(
            "layout font metrics: family={:?} weight={} italic={} size={} char_w={:.2} char_h={:.2} ascent={:.2} (window char_w={:.2} char_h={:.2})",
            default_resolved.font_family,
            default_resolved.font_weight,
            default_resolved.italic,
            default_resolved.font_size,
            default_face.char_width(),
            default_face.row_height(),
            default_face.ascent(),
            char_w,
            char_h,
        );

        let chrome_plan = state.with_face_services(|face_resolver, _font_metrics| {
            WindowChromeRowsPlan::new(params, buffer, face_resolver.resolver())
        });
        let max_mini_window_rows = {
            let frame_rows = frame_params.height / char_h.max(1.0);
            if params.is_minibuffer() {
                max_mini_window_lines_for_buffer(state.output_mut().evaluator(), buffer, frame_rows)
            } else {
                max_mini_window_lines(state.output_mut().evaluator(), frame_rows)
            }
            .ceil()
            .max(1.0) as usize
        };
        let geometry_request = BufferWindowGeometryRequest::new(params, layout_box, char_w, char_h);
        let geometry_request = if params.is_minibuffer()
            && matches!(
                self.position_publication,
                WindowPositionPublication::Redisplay
                    | WindowPositionPublication::RedisplayResumedScrollHook
                    | WindowPositionPublication::RedisplayMiniEobFallback
            )
            && state
                .output_mut()
                .evaluator()
                .gnu_redisplay_hooks_policy_enabled()
            && state
                .output_mut()
                .evaluator()
                .gnu_redisplay_transaction_active()
        {
            // GNU resize_mini_window already measured and committed allocation.
            // try_window now produces only its physical rows; extending this
            // walk to the sizing ceiling invokes virtual strings off screen.
            geometry_request
        } else {
            geometry_request.with_max_mini_window_rows(max_mini_window_rows)
        };
        let BufferWindowGeometryPlan {
            mut geometry,
            line_number_field,
        } = geometry_request.into_window_plan(
            &local_display_policy,
            &buf_access,
            line_number_cell_width,
        );

        // Phase 2 pure-scroll: lay ONLY the newly-exposed rows. Start the body
        // walk at the exposed region (`text_y` + first row index); the unchanged
        // `visibility_bottom_y` caps it at the window bottom — exactly the
        // newly-exposed row count. Chrome geometry is left intact (the mode-line
        // re-walks at the full-window bottom). The source reads from
        // `walk_start` so its text slice + byte indices align with the
        // walk, while the published positions later use the real window_start.
        let source_request = if let Some(scroll) = &scroll {
            geometry.text_y = params.bounds.y + scroll.exposed_text_y;
            geometry.display_text_row_base = scroll.exposed_row_base;
            // Phase 3 below-reuse: BOUND the walk to the edited line only — the
            // rows below it are reused (charpos-shifted) and supplied as reused
            // rows. Without this the walk would run to the window bottom and
            // overwrite the reused-below rows (no CPU saving + wrong positions).
            if scroll.bound_walk {
                geometry.max_rows = scroll.exposed_row_count;
            }
            // Exact partial reads do not run the viewport scrolling heuristic,
            // so there is no need to counterfeit point.  The request keeps the
            // real semantic point for line numbers and other row decoration.
            let request = BufferWindowSourceRequest::for_partial_walk(
                params,
                scroll.walk_start,
                geometry.max_rows,
            );
            if scroll.edit
                && matches!(sync_source_read, SyncSourceRead::AllowHorizon)
                && let Some(plan) = &scroll.sync
                && crate::buffer_source::window_source::sync_source_budget_enabled()
            {
                request.with_sync_stop(crate::types::LayoutCharPos0::new(
                    plan.conservative_source_position().get(),
                ))
            } else {
                request
            }
        } else {
            BufferWindowSourceRequest::from_window_params(params, geometry.max_rows)
        };

        let source_request = match window_text_budget {
            Some(budget) => budget.apply(source_request),
            None => source_request,
        };

        let text_source = if scroll.is_some() {
            source_request.read_exact_into(&buf_access, text_buf)
        } else {
            source_request.read_resolved_into(resolved_window_start, &buf_access, text_buf)
        };
        let bytes_read = text_source.bytes_read();
        let text = if bytes_read > 0 {
            &text_buf[..bytes_read]
        } else {
            &[]
        };
        tracing::debug!(
            "  layout_window_rust id={}: text_y={:.1} text_h={:.1} max_rows={} bytes_read={}",
            params.window_id,
            geometry.text_y,
            geometry.text_height,
            geometry.max_rows,
            bytes_read
        );

        if geometry.text_width <= 0.0 {
            return BufferSourceRenderAttemptOutcome::Skipped;
        }

        // A non-minibuffer window laid out at a degenerate (<= 1 row) height is a
        // transient/probe pass (child-frame/posframe or frame resize in flight).
        // Disable the visibility scroll retries for it: the retry would scroll
        // window_start to point — which then PERSISTS and corrupts the real (tall)
        // window. Pairs with the forward-scroll guard in
        // `BufferWindowSourceRequest::should_forward_scroll_without_layout`.
        let remaining_visibility_retries = if scroll.as_ref().is_some_and(|replay| !replay.edit)
            || (geometry.max_rows <= 1 && !params.is_minibuffer())
        {
            // A genuine scroll consumes an authoritative window_start.
            // An edit can still discover that point needs a new viewport;
            // preserve its retry budget for the measured-row decision.
            0
        } else {
            remaining_visibility_retries
        };

        let reserve_right_special_col =
            !frame_params.window_system && params.right_fringe_width == 0.0;
        let mut walk_setup = BufferSourceWalkSetupRequest::from_window_geometry(
            text_source,
            params,
            &geometry,
            &local_display_policy,
            &default_face,
            reserve_right_border_col,
            reserve_right_special_col,
        )
        .into_setup();
        let text_append_surface = walk_setup.text_append_surface.clone();
        let output_setup = BufferSourceOutputSetup::from_window_geometry(
            frame_id,
            window_id,
            params,
            &geometry,
            layout_box,
            geometry.max_rows,
            &walk_setup,
        )
        .with_position_publication(position_publication)
        .with_remeasure_chrome(remeasure_chrome);

        if geometry.text_height <= 0.0 {
            return output_setup.measure_chrome_for_empty_body(
                state,
                chrome_plan.render_request(
                    params,
                    *layout_box,
                    geometry.mode_line_display_row,
                    reserve_right_border_col,
                    window_metrics,
                    buffer_name,
                ),
                params,
            );
        }

        output_setup.render_body_attempt(
            &mut walk_setup,
            state,
            chrome_plan.render_request(
                params,
                *layout_box,
                geometry.mode_line_display_row,
                reserve_right_border_col,
                window_metrics,
                buffer_name,
            ),
            remaining_visibility_retries,
            forward_viewport_measurement,
            local_display_policy,
            line_number_field,
            &geometry,
            layout_box,
            buffer,
            buffer_id,
            text_source,
            params,
            &default_face,
            window_metrics,
            params.window_id as u64,
            &text_append_surface,
            reserve_right_special_col,
            reserve_right_border_col,
            text,
            &buf_access,
            cursor_only,
            scroll,
        )
    }
}
