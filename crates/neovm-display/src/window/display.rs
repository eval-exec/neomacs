use super::display_table_input::LayoutDisplayTableInputExt;
use super::invisibility_input::LayoutInvisibilityInputExt;
use super::{
    BufferLayoutInputState, Frame, FrameId, FrameParam, HorizontalScrollBarType,
    VerticalScrollBarType, Window, WindowDisplaySnapshot, WindowDisplaySnapshotFreshness,
    WindowDisplayState, WindowId, WindowMargins,
};
use crate::emacs_core::neo::scroll::ScrollContextExt;
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use crate::window::LayoutPrefixInputExt;
use neovm_edit::buffer::{BufferId, LispCharPos1};
use neovm_eval::emacs_core::eval::data::window_layout_query::{
    LayoutQueryInstallError, WindowLayoutQueryAdapter,
};
use neovm_value::intern::WellKnown;
use neovm_value::value::Value;

/// One complete marker-backed viewport transition chosen by a scroll engine.
/// Row-based terminal and pixel-based graphical scrolling share its commit;
/// neither may leave point, start and the partial-row offset out of sync.
pub(crate) struct WindowScrollUpdate {
    pub frame: FrameId,
    pub window: WindowId,
    pub buffer: BufferId,
    pub start: LispCharPos1,
    pub point: LispCharPos1,
    pub hidden_top_pixels: i32,
}

impl WindowScrollUpdate {
    /// No Lisp callbacks occur between validating the target and updating all
    /// markers and redisplay flags. Callers validate their measurement inputs
    /// before entering this commit boundary.
    pub(crate) fn commit(
        self,
        eval: &mut neovm_eval::emacs_core::eval::Context,
    ) -> Result<(), neovm_value::error::Flow> {
        use neovm_value::error::{LispCondition, signal};
        let invalid = || {
            signal(
                LispCondition::Error,
                vec![Value::string("Scroll window changed during measurement")],
            )
        };
        let (frames, buffers) = eval.frame_manager_and_buffers_mut();
        let window = frames
            .get_mut(self.frame)
            .and_then(|frame| frame.find_window_mut(self.window))
            .ok_or_else(invalid)?;
        if window.buffer_id() != Some(self.buffer) {
            return Err(invalid());
        }
        let buffer = buffers.get(self.buffer).ok_or_else(invalid)?;
        let region = buffer.accessible_char_region();
        if self.start < region.start_lisp()
            || self.start > region.end_lisp()
            || self.point < region.start_lisp()
            || self.point > region.end_lisp()
        {
            return Err(invalid());
        }
        let position = buffer.lisp_pos_to_emacs_byte_pos(self.point);
        let adjust_old_point =
            matches!(window, Window::Leaf { point, old_point, .. } if point == old_point);
        buffers.goto_buffer_emacs_byte_pos(self.buffer, position);
        super::window_markers::set_window_point_with_marker(buffers, window, self.point);
        super::window_markers::set_window_start_with_marker(buffers, window, self.start);
        if adjust_old_point {
            super::window_markers::set_window_old_point_with_marker(buffers, window, self.point);
        }
        window.invalidate_window_end();
        if let Window::Leaf {
            vscroll,
            preserve_vscroll_p,
            force_start,
            ..
        } = window
        {
            *vscroll = -self.hidden_top_pixels.max(0);
            *preserve_vscroll_p = false;
            *force_start = true;
        }
        eval.publish_committed_scroll_preview(self.frame, self.window);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowBufferDisplayDefaults {
    pub margins: Option<WindowMargins>,
    pub fringes: Option<WindowFringeDefaults>,
    pub scroll_bars: Option<WindowScrollBarDefaults>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowFringeDefaults {
    pub left_width: Option<i32>,
    pub right_width: Option<i32>,
    pub outside_margins: bool,
}

impl WindowFringeDefaults {
    pub fn new(left_width: Option<i32>, right_width: Option<i32>, outside_margins: bool) -> Self {
        Self {
            left_width,
            right_width,
            outside_margins,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowScrollBarDefaults {
    pub vertical_width: Option<i32>,
    pub vertical_type: Value,
    pub horizontal_height: Option<i32>,
    pub horizontal_type: Value,
}

impl WindowScrollBarDefaults {
    pub fn new(
        vertical_width: Option<i32>,
        vertical_type: Value,
        horizontal_height: Option<i32>,
        horizontal_type: Value,
    ) -> Self {
        Self {
            vertical_width,
            vertical_type,
            horizontal_height,
            horizontal_type,
        }
    }
}

pub(crate) fn frame_line_height(frame: &Frame) -> i64 {
    frame.char_height.max(1.0).round() as i64
}

pub(crate) fn canon_y_from_pixel_y(frame: &Frame, y: i64) -> Value {
    let unit = frame_line_height(frame).max(1);
    if y % unit != 0 {
        Value::make_float(y as f64 / unit as f64)
    } else {
        Value::fixnum(y / unit)
    }
}

pub(crate) fn left_scroll_bar_area_width(frame: &Frame, display: &WindowDisplayState) -> i64 {
    if matches!(
        effective_vertical_scroll_bar_type(frame, display)
            .and_then(|value| VerticalScrollBarType::from_symbol_value(&value)),
        Some(VerticalScrollBarType::Left)
    ) {
        vertical_scroll_bar_area_width(frame, display)
    } else {
        0
    }
}

pub(crate) fn right_scroll_bar_area_width(frame: &Frame, display: &WindowDisplayState) -> i64 {
    if matches!(
        effective_vertical_scroll_bar_type(frame, display)
            .and_then(|value| VerticalScrollBarType::from_symbol_value(&value)),
        Some(VerticalScrollBarType::Right)
    ) {
        vertical_scroll_bar_area_width(frame, display)
    } else {
        0
    }
}

pub(crate) fn vertical_scroll_bar_cols(frame: &Frame, display: &WindowDisplayState) -> i64 {
    let width = vertical_scroll_bar_area_width(frame, display);
    if width == 0 {
        0
    } else {
        (width + frame.char_width.max(1.0).round() as i64 - 1)
            / frame.char_width.max(1.0).round() as i64
    }
}

/// Stage-4 W5/E13 upper extension trait over the eval-owned type.
#[doc(hidden)]
pub trait WindowDisplayContextExt {
    fn set_window_point_for_redisplay(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        point_lisp: LispCharPos1,
    );

    fn create_window_markers_for_root(&mut self, frame_id: FrameId, buffer_id: BufferId);

    fn create_window_markers_for_minibuffer(&mut self, frame_id: FrameId, buffer_id: BufferId);

    fn sync_window_positions(&mut self, buffer_id: BufferId);

    fn window_layout_attempt_freshness(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        source_buffer_id: BufferId,
    ) -> Option<crate::window::WindowLayoutAttemptFreshness>;

    fn window_layout_attempt_freshness_with_capture(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        source_buffer_id: BufferId,
        capture: impl FnMut(&Self, BufferId) -> Option<super::WindowBufferLayoutInputState>,
    ) -> Option<crate::window::WindowLayoutAttemptFreshness>;

    fn window_layout_attempt_freshness_after_span<'a>(
        &self,
        before: &'a crate::window::WindowLayoutAttemptFreshness,
        admission: super::WindowLayoutLispAdmission,
    ) -> Option<std::borrow::Cow<'a, crate::window::WindowLayoutAttemptFreshness>>;

    fn install_window_layout_query<F>(&mut self, query: F) -> Result<(), LayoutQueryInstallError>
    where
        F: FnMut(
                &mut Self,
                FrameId,
                WindowId,
                crate::window::WindowLayoutQueryScope,
            ) -> crate::window::WindowLayoutQueryOutcome
            + 'static;

    /// Whether a layout query is running, so a nested one would be refused
    /// as `LayoutBusy`.
    fn window_layout_query_running(&self) -> bool;

    fn query_window_layout(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
    ) -> crate::window::WindowLayoutQueryOutcome;

    fn query_window_layout_scope(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        scope: crate::window::WindowLayoutQueryScope,
    ) -> crate::window::WindowLayoutQueryOutcome;

    fn publish_redisplay_window_start(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        window_start_lisp: LispCharPos1,
    ) -> crate::window::WindowStartCommit;

    fn publish_redisplay_window_end(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        window_end: crate::window::WindowEndRecord,
    );

    fn begin_redisplay_window_end_attempt(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<crate::window::WindowEndAttempt>;

    fn reject_redisplay_window_end_attempt(&mut self, attempt: crate::window::WindowEndAttempt);

    fn buffer_layout_inputs(
        &self,
        buffer_id: BufferId,
    ) -> Option<super::WindowBufferLayoutInputState>;

    fn redisplay_buffer_layout_inputs(&self, buffer_id: BufferId)
    -> Option<BufferLayoutInputState>;

    fn capture_buffer_layout_inputs<Fontification>(
        &self,
        buffer_id: BufferId,
        capture_fontification: impl FnOnce(&neovm_edit::buffer::Buffer) -> Fontification,
    ) -> Option<BufferLayoutInputState<Fontification>>;

    fn layout_word_wrap_policy(&self, buffer_id: BufferId) -> Option<super::WordWrapPolicy>;

    fn layout_display_table_input(
        &self,
        buffer_id: BufferId,
    ) -> Option<super::LayoutDisplayTableInput>;

    fn layout_invisibility_input(
        &self,
        buffer_id: BufferId,
    ) -> Option<super::LayoutInvisibilityInput>;

    fn layout_prefix_inputs(&self, buffer_id: BufferId) -> Option<super::LayoutPrefixInputs>;

    fn window_display_snapshot_freshness(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<WindowDisplaySnapshotFreshness>;

    fn fresh_window_display_snapshot(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<&WindowDisplaySnapshot>;
}

impl WindowDisplayContextExt for neovm_eval::emacs_core::eval::Context {
    /// Move a window's point marker during redisplay (GNU force_start branch
    /// moving point into the window). The buffer point for the selected
    /// window is the caller's responsibility.
    fn set_window_point_for_redisplay(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        point_lisp: LispCharPos1,
    ) {
        let (frames, buffers) = self.frame_manager_and_buffers_mut();
        if let Some(window) = frames
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
        {
            super::window_markers::set_window_point_with_marker(buffers, window, point_lisp);
        }
    }

    fn create_window_markers_for_root(&mut self, frame_id: FrameId, buffer_id: BufferId) {
        let (frames, buffers) = self.frame_manager_and_buffers_mut();
        let root = frames.get_mut(frame_id).unwrap().root_window_mut();
        debug_assert_eq!(root.buffer_id(), Some(buffer_id));
        super::window_markers::attach_window_position_markers(buffers, root);
    }

    fn create_window_markers_for_minibuffer(&mut self, frame_id: FrameId, buffer_id: BufferId) {
        let (frames, buffers) = self.frame_manager_and_buffers_mut();
        let mini = frames.get_mut(frame_id).unwrap().minibuffer_leaf.as_mut();
        if let Some(mini) = mini {
            debug_assert_eq!(mini.buffer_id(), Some(buffer_id));
            super::window_markers::attach_window_position_markers(buffers, mini);
        }
    }

    fn sync_window_positions(&mut self, buffer_id: BufferId) {
        for frame in self.frame_manager_mut().frames_mut() {
            super::window_markers::sync_window_positions_from_markers(frame, buffer_id);
        }
    }

    /// Capture both the live window and semantic source-buffer identity used by
    /// one speculative leaf layout.
    fn window_layout_attempt_freshness(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        source_buffer_id: BufferId,
    ) -> Option<crate::window::WindowLayoutAttemptFreshness> {
        self.window_layout_attempt_freshness_with_capture(
            frame_id,
            window_id,
            source_buffer_id,
            Self::buffer_layout_inputs,
        )
    }

    #[inline]
    fn window_layout_attempt_freshness_with_capture(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        source_buffer_id: BufferId,
        mut capture: impl FnMut(&Self, BufferId) -> Option<super::WindowBufferLayoutInputState>,
    ) -> Option<crate::window::WindowLayoutAttemptFreshness> {
        use strum::VariantArray;

        let frame = self.frame_manager().get(frame_id)?;
        let live_buffer_id = frame.find_window(window_id)?.buffer_id()?;
        let source_buffer = self.edit.buffers.get(source_buffer_id)?;
        let mut source_layout_variables = crate::window::WindowLayoutVariableState {
            values: [None; <crate::window::WindowLayoutVariable as strum::EnumCount>::COUNT],
        };
        for variable in crate::window::WindowLayoutVariable::VARIANTS {
            source_layout_variables.values[*variable as usize] = self
                .variable_value_in_buffer(Some(source_buffer), variable.sym_id())
                .map(crate::window::WindowLayoutValueIdentity::of);
        }
        // Keep the original native-read order. The canonical projections own
        // immutable snapshots and invoke no Lisp callbacks; cloning shares
        // native payloads, never mutable Lisp source data.
        let fontset_generation = crate::emacs_core::fontset::fontset_generation();
        let context_instance_id = self.context_instance_id();
        let window_topology_generation = self.frame_manager().window_topology_generation();
        let frame_inputs = frame.layout_inputs();
        let window_inputs = frame.window_layout_inputs(window_id)?;
        let namespace_before = (live_buffer_id == source_buffer_id)
            .then(neovm_value::intern::symbol_registry_epoch_value);
        let live_buffer = capture(self, live_buffer_id)?;
        let source_buffer = if let Some(namespace_before) = namespace_before {
            let owned_source = live_buffer.clone();
            let namespace_after = neovm_value::intern::symbol_registry_epoch_value();
            // Another World may unintern the fontifier's process-global name
            // without borrowing this Context. Reuse requires the same observed
            // namespace epoch across the actual read and clone. This relies on
            // native intern retaining its positive entry until an observed
            // epoch change (no eviction): an unpublished removal under the
            // registry write lock also leaves the old second lookup a cache
            // hit. A miss waits for that writer and observes its publication.
            // Owner/buffer reads remain private and noncalling in this scope.
            if namespace_before == namespace_after {
                owned_source
            } else {
                capture(self, source_buffer_id)?
            }
        } else {
            capture(self, source_buffer_id)?
        };
        Some(crate::window::WindowLayoutAttemptFreshness {
            fontset_generation,
            context_instance_id,
            window_topology_generation,
            frame: frame_inputs,
            window: window_inputs,
            live_buffer,
            source_buffer,
            source_layout_variables,
            selection: crate::window::WindowLayoutSelectionState {
                selected_frame: self.frame_manager().selected_frame().map(|frame| frame.id),
                frame_selected_window: frame.selected_window,
                minibuffer_selected_window: (*self.minibuffer_selected_window()),
                active_minibuffer_window: (*self.active_minibuffer_window()),
            },
            face_change_count: self.face_change_count(),
            media_generation: self.media_generation(),
            body_redisplay: self.body_redisplay_revision(window_id, live_buffer_id),
            function_epoch: self.obarray().function_epoch(),
        })
    }

    /// Pass an owned token through native layout preparation/publication.
    ///
    /// Native admission is restricted to audited layout preparation/publication
    /// that changes no logical input and restores temporary buffer/point
    /// selection. Merely avoiding Lisp does not admit arbitrary native edits.
    /// Possible Lisp always recaptures. This is an attempt-local borrow, not
    /// retained state or a key-based cache. Each evaluator exclusively owns its
    /// inputs; independently published fontsets/dispatch and lazy native
    /// character-table initialization still require the checks below.
    fn window_layout_attempt_freshness_after_span<'a>(
        &self,
        before: &'a crate::window::WindowLayoutAttemptFreshness,
        admission: super::WindowLayoutLispAdmission,
    ) -> Option<std::borrow::Cow<'a, crate::window::WindowLayoutAttemptFreshness>> {
        use std::borrow::Cow;
        // Enumerate the complete projection: adding an input requires auditing
        // whether native preparation can change it without a Lisp boundary.
        let crate::window::WindowLayoutAttemptFreshness {
            fontset_generation,
            context_instance_id,
            window_topology_generation: _,
            frame,
            window,
            live_buffer: _,
            source_buffer,
            source_layout_variables: _,
            selection: _,
            face_change_count: _,
            media_generation: _,
            body_redisplay: _,
            function_epoch,
        } = before;
        if admission == super::WindowLayoutLispAdmission::NativeOnly
            && *context_instance_id == self.context_instance_id()
            && *fontset_generation == crate::emacs_core::fontset::fontset_generation()
            && window.char_table_mutation_epoch == current_char_table_layout_revision()
            && *function_epoch == self.obarray().function_epoch()
        {
            Some(Cow::Borrowed(before))
        } else {
            self.window_layout_attempt_freshness(frame.id, window.id, source_buffer.id)
                .map(Cow::Owned)
        }
    }

    /// Install the frontend half of the synchronous layout-query boundary.
    fn install_window_layout_query<F>(&mut self, query: F) -> Result<(), LayoutQueryInstallError>
    where
        F: FnMut(
                &mut Self,
                FrameId,
                WindowId,
                crate::window::WindowLayoutQueryScope,
            ) -> crate::window::WindowLayoutQueryOutcome
            + 'static,
    {
        self.install_window_layout_query_handler(query)
    }

    /// Whether a layout query is running, so a nested one would be refused
    /// as `LayoutBusy`.
    fn window_layout_query_running(&self) -> bool {
        matches!(
            self.window_layout_query_adapter(),
            WindowLayoutQueryAdapter::Running
        )
    }

    /// Refresh one window through the frontend's real layout engine and return
    /// the window-end record and display geometry produced by that one
    /// stack-local row walk.
    ///
    /// Like GNU `Fwindow_end`, this is an observation, not redisplay
    /// publication: retained window positions remain owned by an accepted
    /// presentation.
    fn query_window_layout(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
    ) -> crate::window::WindowLayoutQueryOutcome {
        self.query_window_layout_scope(
            frame_id,
            window_id,
            crate::window::WindowLayoutQueryScope::Viewport,
        )
    }

    fn query_window_layout_scope(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        scope: crate::window::WindowLayoutQueryScope,
    ) -> crate::window::WindowLayoutQueryOutcome {
        self.sync_pending_resize_events();
        if let Some(buffer_id) = self
            .frame_manager()
            .get(frame_id)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(Window::buffer_id)
        {
            crate::window::window_markers::sync_all_frames_for_buffer(
                self.frame_manager_mut(),
                buffer_id,
            );
        }
        // A queried frame need not be globally selected. Its locally selected
        // window still owns an independent point marker in that case.
        if self
            .frame_manager()
            .selected_frame()
            .is_some_and(|frame| frame.id == frame_id)
        {
            let (frames, buffers) = self.frame_manager_and_buffers_mut();
            crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
                frames, buffers, frame_id,
            );
        }
        let mut take = self.take_window_layout_query();
        let mut query = match take.take_previous() {
            WindowLayoutQueryAdapter::Unavailable => {
                take.put_previous(WindowLayoutQueryAdapter::Unavailable);
                self.restore_window_layout_query(take);
                return crate::window::WindowLayoutQueryOutcome::Unavailable;
            }
            WindowLayoutQueryAdapter::Running => {
                // A position query can measure chrome whose :eval asks for
                // another position. The outer adapter owns the walk; let the
                // nested consumer fall back to accepted geometry instead of
                // turning an ordinary chrome expression into a layout error.
                take.put_previous(WindowLayoutQueryAdapter::Running);
                self.restore_window_layout_query(take);
                if matches!(
                    scope,
                    crate::window::WindowLayoutQueryScope::Position { .. }
                ) && crate::emacs_core::xdisp::mode_line_display_active(self)
                {
                    return crate::window::WindowLayoutQueryOutcome::Unavailable;
                }
                return crate::window::WindowLayoutQueryOutcome::LayoutBusy;
            }
            WindowLayoutQueryAdapter::Ready(query) => query,
        };
        // Redisplay observes the outermost restriction; motion observes the
        // caller's accessible region. Do not widen a measured motion query.
        let saved_restrictions = matches!(
            scope,
            crate::window::WindowLayoutQueryScope::Viewport
                | crate::window::WindowLayoutQueryScope::Position { .. }
        )
        .then(|| self.edit.buffers.reset_outermost_restrictions());
        let record = query(self, frame_id, window_id, scope);
        if let Some(saved_restrictions) = saved_restrictions {
            self.edit
                .buffers
                .restore_outermost_restrictions(saved_restrictions);
        }
        take.put_previous(WindowLayoutQueryAdapter::Ready(query));
        self.restore_window_layout_query(take);
        record
    }

    #[must_use = "a committed window start owes window-scroll-functions a run"]
    fn publish_redisplay_window_start(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        window_start_lisp: LispCharPos1,
    ) -> crate::window::WindowStartCommit {
        let (frames, buffers) = self.frame_manager_and_buffers_mut();
        let Some(frame) = frames.get_mut(frame_id) else {
            return crate::window::WindowStartCommit::Inherited;
        };

        let mut commit = crate::window::WindowStartCommit::Inherited;
        let mut update_window = |window: &mut Window| {
            // GNU decides between "the start was inherited" and "redisplay
            // committed a start" before it overwrites `w->start`: the
            // `force_start` branch (src/xdisp.c:20724) runs the hook even when
            // the forced start equals the old one, while `try_scrolling`
            // (src/xdisp.c:19645) and the recenter fallback
            // (src/xdisp.c:21227) are only reached because the start moved.
            let forced = matches!(
                window,
                Window::Leaf {
                    force_start: true,
                    ..
                }
            );
            let moved = window.window_start() != Some(window_start_lisp);
            commit = crate::window::WindowStartCommit::of(forced, moved);
            crate::window::window_markers::set_window_start_with_marker(
                buffers,
                window,
                window_start_lisp,
            );
            // GNU clears `w->force_start` once redisplay has consumed it
            // (redisplay_window force_start branch) — one-shot semantics.
            if let Window::Leaf { force_start, .. } = window {
                *force_start = false;
            }
        };

        if let Some(window) = frame.tree_mut().find_mut(window_id) {
            update_window(window);
        } else if let Some(ref mut mini) = frame.minibuffer_leaf
            && mini.id() == window_id
        {
            update_window(mini);
        }
        commit
    }

    /// Publish the final end record after the post-hook row walk completes.
    ///
    /// GNU invalidates `window_end_valid` before
    /// `run_window_scroll_functions` and marks the new end valid only after
    /// `try_window` succeeds. Keeping this separate from start publication
    /// prevents Lisp callbacks from observing a discarded attempt's end.
    fn publish_redisplay_window_end(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        window_end: crate::window::WindowEndRecord,
    ) {
        let Some(window) = self
            .frame_manager_mut()
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
        else {
            return;
        };
        window.set_window_end_record(window_end);
    }

    /// Start one speculative body/chrome publication scope.
    fn begin_redisplay_window_end_attempt(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<crate::window::WindowEndAttempt> {
        let window = self.frame_manager().get(frame_id)?.find_window(window_id)?;
        if window.buffer_id()? != buffer_id {
            return None;
        }
        Some(crate::window::WindowEndAttempt {
            frame_id,
            window_id,
            buffer_id,
            previous: window.window_end_state()?,
        })
    }

    /// Reject a speculative end while preserving any explicit buffer switch
    /// performed by Lisp during chrome evaluation.
    fn reject_redisplay_window_end_attempt(&mut self, attempt: crate::window::WindowEndAttempt) {
        let Some(window) = self
            .frame_manager_mut()
            .get_mut(attempt.frame_id)
            .and_then(|frame| frame.find_window_mut(attempt.window_id))
        else {
            return;
        };
        if window.buffer_id() == Some(attempt.buffer_id) {
            window.restore_window_end_state(attempt.previous);
        }
    }

    /// Complete buffer projection for retained rows and speculative attempts.
    /// Installing a hook can change canonical motion without changing text
    /// ticks or the generic redisplay generation. Keep this effective identity
    /// read and the attempt's namespace guard at their existing boundaries.
    fn buffer_layout_inputs(
        &self,
        buffer_id: BufferId,
    ) -> Option<super::WindowBufferLayoutInputState> {
        // GNU reads the DEFSYM `Qfontification_functions`; this projection
        // runs for every layout attempt and previously re-interned the
        // spelling at each call.
        static FONTIFICATION_FUNCTIONS: WellKnown = WellKnown::FontificationFunctions;
        self.capture_buffer_layout_inputs(buffer_id, |buffer| {
            super::FontificationHookIdentity(
                self.variable_value_in_buffer_inline(Some(buffer), FONTIFICATION_FUNCTIONS.id())
                    .map(super::WindowLayoutValueIdentity::of),
            )
        })
    }

    /// Redisplay scheduling projection. GNU does not make replacing the
    /// fontification hook itself a repaint request. Fontification mutations
    /// still participate through the shared text/property/overlay revisions.
    /// Unit erases only that callback identity; full freshness consumers cannot
    /// accept this type in place of their required fontification payload.
    fn redisplay_buffer_layout_inputs(
        &self,
        buffer_id: BufferId,
    ) -> Option<BufferLayoutInputState> {
        self.capture_buffer_layout_inputs(buffer_id, |_| ())
    }

    #[inline]
    fn capture_buffer_layout_inputs<Fontification>(
        &self,
        buffer_id: BufferId,
        capture_fontification: impl FnOnce(&neovm_edit::buffer::Buffer) -> Fontification,
    ) -> Option<BufferLayoutInputState<Fontification>> {
        let buffer = self.edit.buffers.get(buffer_id)?;
        Some(BufferLayoutInputState {
            id: buffer.id(),
            modified_tick: buffer.modified_tick(),
            chars_modified_tick: buffer.chars_modified_tick(),
            props_modified_tick: buffer.props_modified_tick(),
            overlay_modified_tick: buffer.overlay_modified_tick(),
            accessible_chars: buffer.accessible_char_region().range(),
            accessible_bytes: buffer.accessible_emacs_byte_region().range(),
            total_chars: buffer.total_char_len(),
            total_emacs_bytes: buffer.total_emacs_byte_len(),
            fontification_functions: capture_fontification(buffer),
            prefixes: self.layout_prefix_inputs(buffer_id)?,
            invisibility: self.layout_invisibility_input(buffer_id)?,
            display_table: self.layout_display_table_input(buffer_id)?,
            word_wrap_policy: self.layout_word_wrap_policy(buffer_id)?,
        })
    }

    fn layout_word_wrap_policy(&self, buffer_id: BufferId) -> Option<super::WordWrapPolicy> {
        let buffer = self.edit.buffers.get(buffer_id)?;
        let enabled = |variable: super::WindowLayoutVariable| {
            self.variable_value_in_buffer(Some(buffer), variable.sym_id())
                .is_some_and(|value| value.is_truthy())
        };
        let mode = if enabled(super::WindowLayoutVariable::WordWrap)
            && enabled(super::WindowLayoutVariable::WordWrapByCategory)
        {
            super::WordWrapMode::Category
        } else {
            super::WordWrapMode::Whitespace
        };
        Some(super::word_wrap_policy_for_buffer(buffer, mode))
    }

    /// Capture glyph-vector inputs for the effective buffer/standard table.
    fn layout_display_table_input(
        &self,
        buffer_id: BufferId,
    ) -> Option<super::LayoutDisplayTableInput> {
        let buffer = self.edit.buffers.get(buffer_id)?;
        let value = |variable: super::WindowLayoutVariable| {
            self.variable_value_in_buffer(Some(buffer), variable.sym_id())
        };
        let table = value(super::WindowLayoutVariable::BufferDisplayTable)
            .filter(|value| !value.is_nil())
            .or_else(|| value(super::WindowLayoutVariable::StandardDisplayTable))
            .unwrap_or(Value::NIL);
        Some(super::LayoutDisplayTableInput::capture(table))
    }

    /// Capture ordered invisibility membership and ellipsis selection by value.
    fn layout_invisibility_input(
        &self,
        buffer_id: BufferId,
    ) -> Option<super::LayoutInvisibilityInput> {
        let buffer = self.edit.buffers.get(buffer_id)?;
        Some(super::LayoutInvisibilityInput::capture(
            self.variable_value_in_buffer(
                Some(buffer),
                super::WindowLayoutVariable::BufferInvisibilitySpec.sym_id(),
            )
            .unwrap_or(Value::NIL),
        ))
    }

    /// Capture effective buffer-local prefix values, never references to mutable
    /// Lisp strings. Every geometry consumer uses this same projection.
    fn layout_prefix_inputs(&self, buffer_id: BufferId) -> Option<super::LayoutPrefixInputs> {
        let buffer = self.edit.buffers.get(buffer_id)?;
        let capture = |variable: super::WindowLayoutVariable| {
            super::LayoutPrefixInput::capture(
                self.variable_value_in_buffer(Some(buffer), variable.sym_id()),
            )
        };
        Some(super::LayoutPrefixInputs {
            line: capture(super::WindowLayoutVariable::LinePrefix),
            wrap: capture(super::WindowLayoutVariable::WrapPrefix),
        })
    }

    /// Capture the complete layout identity for one live window.
    ///
    /// This is the single freshness boundary for consumers of retained rows.
    /// Its frame/window/buffer projections share every geometry input with
    /// redisplay scheduling. This full token additionally owns the effective
    /// fontification callback identity, which is not a repaint request.
    fn window_display_snapshot_freshness(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<WindowDisplaySnapshotFreshness> {
        let frame = self.frame_manager().get(frame_id)?;
        let window = frame.window_layout_inputs(window_id)?;
        if window.buffer_id != buffer_id {
            return None;
        }
        Some(WindowDisplaySnapshotFreshness {
            fontset_generation: crate::emacs_core::fontset::fontset_generation(),
            context_instance_id: self.context_instance_id(),
            window_topology_generation: self.frame_manager().window_topology_generation(),
            frame: frame.layout_inputs(),
            window,
            buffer: self.buffer_layout_inputs(buffer_id)?,
            face_change_count: self.face_change_count(),
            display_var_change_count: self.display_var_change_count(),
            redisplay_generation: self.redisplay_generation(),
            body_redisplay: self.body_redisplay_revision(window_id, buffer_id),
            media_generation: self.media_generation(),
            function_epoch: self.obarray().function_epoch(),
            symbol_property_revision: neovm_value::symbol::SymbolPropertyRevision::current(),
            overlays_face_only: self
                .edit
                .buffers
                .get(buffer_id)
                .is_some_and(|buffer| buffer.overlays_are_face_only()),
        })
    }

    /// Return retained rows only while every layout input still matches.
    ///
    /// Older snapshots without a full token retain the legacy `buffer_modiff`
    /// gate. Fixtures with neither value explicitly opt into trusted synthetic
    /// geometry.
    fn fresh_window_display_snapshot(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<&WindowDisplaySnapshot> {
        let current = self.window_display_snapshot_freshness(frame_id, window_id, buffer_id)?;
        let Some(snapshot) = self
            .frame_manager()
            .get(frame_id)?
            .redisplay_snapshot(window_id)
        else {
            tracing::trace!(target: "neovm_core::motion_geometry", ?frame_id, ?window_id,
                "retained rows unavailable");
            return None;
        };
        if let Some(recorded) = &snapshot.layout_freshness {
            tracing::trace!(target: "neovm_core::motion_geometry", ?frame_id, ?window_id,
                ?recorded, ?current,
                freshness_matches = recorded == &current,
                fontset_matches = recorded.fontset_generation == current.fontset_generation,
                context_matches = recorded.context_instance_id == current.context_instance_id,
                topology_matches = recorded.window_topology_generation == current.window_topology_generation,
                point_matches = recorded.window.point == current.window.point,
                window_matches = recorded.window == current.window,
                buffer_matches = recorded.buffer == current.buffer,
                frame_matches = recorded.frame == current.frame,
                presentation_matches = recorded.redisplay_generation == current.redisplay_generation,
                body_matches = recorded.body_redisplay == current.body_redisplay,
                faces_match = recorded.face_change_count == current.face_change_count,
                display_vars_match = recorded.display_var_change_count == current.display_var_change_count,
                media_match = recorded.media_generation == current.media_generation,
                functions_match = recorded.function_epoch == current.function_epoch,
                symbol_properties_match = recorded.symbol_property_revision == current.symbol_property_revision,
                "retained row freshness");
            return (recorded == &current).then_some(snapshot);
        }
        if let Some(recorded_modiff) = snapshot.buffer_modiff
            && recorded_modiff != current.buffer.modified_tick
        {
            return None;
        }
        Some(snapshot)
    }
}

// The default bodies above are the implementation; this binds them to the
// upper-layer type that owns the state.

#[cfg(test)]
#[path = "tests/attempt_freshness_reuse_test.rs"]
mod attempt_freshness_reuse_tests;

#[cfg(test)]
#[path = "tests/buffer_layout_projection_test.rs"]
mod buffer_layout_projection_tests;

pub(crate) use neovm_eval::emacs_core::eval::data::window_state::{
    current_char_table_layout_revision, effective_horizontal_scroll_bar_enabled,
    effective_vertical_scroll_bar_type, frame_config_scroll_bar_height,
    frame_config_scroll_bar_width, frame_has_horizontal_scroll_bar, frame_vertical_scroll_bar_side,
    horizontal_scroll_bar_area_height, horizontal_scroll_bar_lines, vertical_scroll_bar_area_width,
};
