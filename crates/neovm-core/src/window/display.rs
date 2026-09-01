use super::{
    BufferLayoutInputState, Frame, FrameId, FrameManager, FrameParam, HorizontalScrollBarType,
    VerticalScrollBarType, Window, WindowDisplaySnapshot, WindowDisplaySnapshotFreshness,
    WindowDisplayState, WindowId, WindowLayoutInputState, WindowMargins,
};
use crate::buffer::{BufferId, LispCharPos1};
use crate::emacs_core::value::Value;
use std::cell::Cell;

/// Frontend-owned synchronous layout-query callback state.
///
/// Keeping the active call as an enum makes reentrant entry explicit and
/// prevents a temporarily removed callback from being mistaken for an
/// unavailable display. This is Neomacs dependency-boundary machinery, not a
/// GNU `eval.c` primitive, so it belongs beside the window display adapter.
pub(crate) enum WindowLayoutQueryAdapter {
    Unavailable,
    Ready(
        Box<
            dyn FnMut(
                &mut crate::emacs_core::eval::Context,
                FrameId,
                WindowId,
            ) -> crate::window::WindowLayoutQueryOutcome,
        >,
    ),
    Running,
}

thread_local! {
    /// Mutation generation for retained-layout inputs that contain char tables.
    ///
    /// Char tables are mutable heap objects, so their tagged `Value` identity
    /// does not change when a display table is edited in place. Keeping the
    /// epoch local to the evaluator thread avoids unrelated parallel contexts
    /// invalidating one another while making every in-thread mutation visible.
    static CHAR_TABLE_LAYOUT_MUTATION_EPOCH: Cell<u64> = const { Cell::new(0) };

    /// Unique identities for window-parameter states created on this
    /// evaluator thread.  Window trees are cloned for saved configurations,
    /// so a counter stored on the window itself can fork and later collide.
    static WINDOW_PARAMETERS_LAYOUT_GENERATION: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn char_table_layout_mutation_epoch() -> u64 {
    CHAR_TABLE_LAYOUT_MUTATION_EPOCH.with(Cell::get)
}

pub(crate) fn note_char_table_layout_mutation() {
    CHAR_TABLE_LAYOUT_MUTATION_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
}

pub(super) fn next_window_parameters_generation() -> u64 {
    WINDOW_PARAMETERS_LAYOUT_GENERATION.with(|generation| {
        let next = generation
            .get()
            .checked_add(1)
            .expect("window parameter layout generation exhausted");
        generation.set(next);
        next
    })
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

/// Effective scroll-bar geometry for one live window.
///
/// GNU keeps raw scroll-bar settings on the window, resolves `t` through
/// frame defaults, and then exposes area macros such as
/// `WINDOW_SCROLL_BAR_AREA_WIDTH`.  Keep that resolution in core so layout,
/// Lisp-visible queries, and later input hit-testing cannot drift apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowScrollBarGeometry {
    /// Effective vertical type after frame inheritance (`left`/`right`/nil).
    pub vertical_type: Option<Value>,
    pub left_area_width: i64,
    pub right_area_width: i64,
    pub horizontal_area_height: i64,
}

fn frame_line_height(frame: &Frame) -> i64 {
    frame.char_height.max(1.0).round() as i64
}

fn canon_y_from_pixel_y(frame: &Frame, y: i64) -> Value {
    let unit = frame_line_height(frame).max(1);
    if y % unit != 0 {
        Value::make_float(y as f64 / unit as f64)
    } else {
        Value::fixnum(y / unit)
    }
}

fn frame_default_left_fringe_width(frame: &Frame) -> i32 {
    frame
        .known_parameter(FrameParam::LeftFringe)
        .and_then(|v| v.as_int())
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(8)
}

fn frame_default_right_fringe_width(frame: &Frame) -> i32 {
    frame
        .known_parameter(FrameParam::RightFringe)
        .and_then(|v| v.as_int())
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(8)
}

fn frame_vertical_scroll_bar_side(frame: &Frame) -> Option<Value> {
    let raw = frame
        .known_parameter(FrameParam::VerticalScrollBars)
        .unwrap_or_else(|| {
            if frame.effective_window_system().is_some() {
                Value::symbol("right")
            } else {
                Value::NIL
            }
        });
    match VerticalScrollBarType::from_symbol_value(&raw) {
        Some(side) => Some(side.symbol()),
        _ if raw.is_nil() => None,
        _ if raw.is_truthy() => Some(Value::symbol("right")),
        _ => None,
    }
}

fn frame_has_horizontal_scroll_bar(frame: &Frame) -> bool {
    let raw = frame
        .known_parameter(FrameParam::HorizontalScrollBars)
        .unwrap_or(Value::NIL);
    HorizontalScrollBarType::from_symbol_value(&raw).is_some() || raw.is_truthy()
}

fn frame_config_scroll_bar_width(frame: &Frame) -> i32 {
    frame
        .known_parameter(FrameParam::ScrollBarWidth)
        .and_then(|v| v.as_int())
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| frame.char_width.max(1.0).round() as i32)
}

fn frame_config_scroll_bar_height(frame: &Frame) -> i32 {
    frame
        .known_parameter(FrameParam::ScrollBarHeight)
        .and_then(|v| v.as_int())
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| frame.char_height.max(1.0).round() as i32)
}

fn effective_vertical_scroll_bar_type(
    frame: &Frame,
    display: &WindowDisplayState,
) -> Option<Value> {
    match VerticalScrollBarType::from_symbol_value(&display.vertical_scroll_bar_type) {
        Some(side) => Some(side.symbol()),
        _ if display.vertical_scroll_bar_type.is_nil() => None,
        _ if display.vertical_scroll_bar_type.is_truthy() => frame_vertical_scroll_bar_side(frame),
        _ => None,
    }
}

fn effective_horizontal_scroll_bar_enabled(
    frame: &Frame,
    display: &WindowDisplayState,
    is_minibuffer: bool,
) -> bool {
    match HorizontalScrollBarType::from_symbol_value(&display.horizontal_scroll_bar_type) {
        Some(_) => true,
        _ if display.horizontal_scroll_bar_type.is_nil() => false,
        _ if is_minibuffer => false,
        _ if display.horizontal_scroll_bar_type.is_truthy() => {
            frame_has_horizontal_scroll_bar(frame)
        }
        _ => false,
    }
}

fn vertical_scroll_bar_area_width(frame: &Frame, display: &WindowDisplayState) -> i64 {
    if effective_vertical_scroll_bar_type(frame, display).is_some() {
        i64::from(if display.scroll_bar_width >= 0 {
            display.scroll_bar_width
        } else {
            frame_config_scroll_bar_width(frame)
        })
    } else {
        0
    }
}

fn left_scroll_bar_area_width(frame: &Frame, display: &WindowDisplayState) -> i64 {
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

fn right_scroll_bar_area_width(frame: &Frame, display: &WindowDisplayState) -> i64 {
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

fn horizontal_scroll_bar_area_height(
    frame: &Frame,
    display: &WindowDisplayState,
    is_minibuffer: bool,
) -> i64 {
    if effective_horizontal_scroll_bar_enabled(frame, display, is_minibuffer) {
        i64::from(if display.scroll_bar_height >= 0 {
            display.scroll_bar_height
        } else {
            frame_config_scroll_bar_height(frame)
        })
    } else {
        0
    }
}

fn vertical_scroll_bar_cols(frame: &Frame, display: &WindowDisplayState) -> i64 {
    let width = vertical_scroll_bar_area_width(frame, display);
    if width == 0 {
        0
    } else {
        (width + frame.char_width.max(1.0).round() as i64 - 1)
            / frame.char_width.max(1.0).round() as i64
    }
}

fn horizontal_scroll_bar_lines(
    frame: &Frame,
    display: &WindowDisplayState,
    is_minibuffer: bool,
) -> i64 {
    let height = horizontal_scroll_bar_area_height(frame, display, is_minibuffer);
    if height == 0 {
        0
    } else {
        (height + frame.char_height.max(1.0).round() as i64 - 1)
            / frame.char_height.max(1.0).round() as i64
    }
}

pub fn resolve_window_scroll_bar_geometry(
    frame: &Frame,
    display: &WindowDisplayState,
    is_minibuffer: bool,
) -> WindowScrollBarGeometry {
    let vertical_type = effective_vertical_scroll_bar_type(frame, display);
    let vertical_area_width = if vertical_type.is_some() {
        vertical_scroll_bar_area_width(frame, display)
    } else {
        0
    };
    let horizontal_area_height = horizontal_scroll_bar_area_height(frame, display, is_minibuffer);

    WindowScrollBarGeometry {
        vertical_type,
        left_area_width: if matches!(
            vertical_type.and_then(|value| VerticalScrollBarType::from_symbol_value(&value)),
            Some(VerticalScrollBarType::Left)
        ) {
            vertical_area_width
        } else {
            0
        },
        right_area_width: if matches!(
            vertical_type.and_then(|value| VerticalScrollBarType::from_symbol_value(&value)),
            Some(VerticalScrollBarType::Right)
        ) {
            vertical_area_width
        } else {
            0
        },
        horizontal_area_height,
    }
}

impl Frame {
    /// Canonical projection of every live-window input that can affect rows.
    ///
    /// Keeping effective margins, fringes, and scroll-bar partitions here
    /// makes redisplay skipping and retained-row freshness share one source of
    /// truth, including frame-default values inherited by a window.
    pub(crate) fn window_layout_inputs(
        &self,
        window_id: WindowId,
    ) -> Option<WindowLayoutInputState> {
        let window = self.find_window(window_id)?;
        let state = window.redisplay_state()?;
        let Window::Leaf {
            margins, display, ..
        } = window
        else {
            return None;
        };
        let is_window_system = self.effective_window_system().is_some();
        let left_fringe_width = if is_window_system {
            if display.left_fringe_width >= 0 {
                display.left_fringe_width
            } else {
                frame_default_left_fringe_width(self)
            }
        } else {
            0
        };
        let right_fringe_width = if is_window_system {
            if display.right_fringe_width >= 0 {
                display.right_fringe_width
            } else {
                frame_default_right_fringe_width(self)
            }
        } else {
            0
        };
        let is_minibuffer = self.minibuffer_window == Some(window_id);
        let scroll_bars = resolve_window_scroll_bar_geometry(self, display, is_minibuffer);

        Some(WindowLayoutInputState {
            id: state.id,
            buffer_id: state.buffer_id,
            bounds: state.bounds,
            window_start: state.window_start,
            point: state.point,
            hscroll: state.hscroll,
            vscroll: state.vscroll,
            preserve_vscroll_p: state.preserve_vscroll_p,
            margins: *margins,
            display_table_identity: display.display_table.bits(),
            char_table_mutation_epoch: char_table_layout_mutation_epoch(),
            window_parameters_generation: window.parameters_generation(),
            left_fringe_width,
            right_fringe_width,
            fringes_outside_margins: display.fringes_outside_margins,
            left_scroll_bar_width: scroll_bars.left_area_width,
            right_scroll_bar_width: scroll_bars.right_area_width,
            horizontal_scroll_bar_height: scroll_bars.horizontal_area_height,
        })
    }
}

impl crate::emacs_core::eval::Context {
    /// Capture both the live window and semantic source-buffer identity used by
    /// one speculative leaf layout.
    pub fn window_layout_attempt_freshness(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        source_buffer_id: BufferId,
    ) -> Option<crate::window::WindowLayoutAttemptFreshness> {
        use strum::VariantArray;

        let frame = self.frames.get(frame_id)?;
        let live_buffer_id = frame.find_window(window_id)?.buffer_id()?;
        let source_buffer = self.buffers.get(source_buffer_id)?;
        let mut source_layout_variables = crate::window::WindowLayoutVariableState {
            values: [None; <crate::window::WindowLayoutVariable as strum::EnumCount>::COUNT],
        };
        for variable in crate::window::WindowLayoutVariable::VARIANTS {
            source_layout_variables.values[*variable as usize] = self
                .obarray
                .value_in_buffer_id(Some(source_buffer), variable.sym_id())
                .map(crate::window::WindowLayoutValueIdentity::of);
        }
        Some(crate::window::WindowLayoutAttemptFreshness {
            context_instance_id: self.context_instance_id(),
            window_topology_generation: self.frames.window_topology_generation(),
            frame: frame.layout_inputs(),
            window: frame.window_layout_inputs(window_id)?,
            live_buffer: self.buffer_layout_inputs(live_buffer_id)?,
            source_buffer: self.buffer_layout_inputs(source_buffer_id)?,
            source_layout_variables,
            selection: crate::window::WindowLayoutSelectionState {
                selected_frame: self.frames.selected_frame().map(|frame| frame.id),
                frame_selected_window: frame.selected_window,
                minibuffer_selected_window: self.minibuffer_selected_window,
                active_minibuffer_window: self.active_minibuffer_window,
            },
            face_change_count: self.face_change_count,
            media_generation: self.media_generation(),
            function_epoch: self.obarray.function_epoch(),
        })
    }

    /// Install the frontend half of the synchronous layout-query boundary.
    pub fn install_window_layout_query<F>(&mut self, query: F)
    where
        F: FnMut(&mut Self, FrameId, WindowId) -> crate::window::WindowLayoutQueryOutcome + 'static,
    {
        self.window_layout_query_adapter = WindowLayoutQueryAdapter::Ready(Box::new(query));
    }

    /// Refresh one window through the frontend's real layout engine and return
    /// the window-end record and display geometry produced by that one
    /// stack-local row walk.
    ///
    /// Like GNU `Fwindow_end`, this is an observation, not redisplay
    /// publication: retained window positions remain owned by an accepted
    /// presentation.
    pub(crate) fn query_window_layout(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
    ) -> crate::window::WindowLayoutQueryOutcome {
        self.sync_pending_resize_events();
        if let Some(buffer_id) = self
            .frames
            .get(frame_id)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(Window::buffer_id)
        {
            crate::window::window_markers::sync_all_frames_for_buffer(
                &mut self.frames,
                &self.buffers,
                buffer_id,
            );
        }
        crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
            &mut self.frames,
            &mut self.buffers,
            frame_id,
        );
        let adapter = std::mem::replace(
            &mut self.window_layout_query_adapter,
            WindowLayoutQueryAdapter::Running,
        );
        let mut query = match adapter {
            WindowLayoutQueryAdapter::Unavailable => {
                self.window_layout_query_adapter = WindowLayoutQueryAdapter::Unavailable;
                return crate::window::WindowLayoutQueryOutcome::Unavailable;
            }
            WindowLayoutQueryAdapter::Running => {
                return crate::window::WindowLayoutQueryOutcome::LayoutBusy;
            }
            WindowLayoutQueryAdapter::Ready(query) => query,
        };
        let saved_restrictions = self.buffers.reset_outermost_restrictions();
        let record = query(self, frame_id, window_id);
        self.buffers
            .restore_outermost_restrictions(saved_restrictions);
        self.window_layout_query_adapter = WindowLayoutQueryAdapter::Ready(query);
        record
    }

    #[must_use = "a committed window start owes window-scroll-functions a run"]
    pub fn publish_redisplay_window_start(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        window_start_lisp: LispCharPos1,
    ) -> crate::window::WindowStartCommit {
        let frames = &mut self.frames;
        let buffers = &mut self.buffers;
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

        if let Some(window) = frame.root_window.find_mut(window_id) {
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
    pub fn publish_redisplay_window_end(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        window_end: crate::window::WindowEndRecord,
    ) {
        let Some(window) = self
            .frames
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
        else {
            return;
        };
        window.set_window_end_record(window_end);
    }

    /// Start one speculative body/chrome publication scope.
    pub fn begin_redisplay_window_end_attempt(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<crate::window::WindowEndAttempt> {
        let window = self.frames.get(frame_id)?.find_window(window_id)?;
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
    pub fn reject_redisplay_window_end_attempt(
        &mut self,
        attempt: crate::window::WindowEndAttempt,
    ) {
        let Some(window) = self
            .frames
            .get_mut(attempt.frame_id)
            .and_then(|frame| frame.find_window_mut(attempt.window_id))
        else {
            return;
        };
        if window.buffer_id() == Some(attempt.buffer_id) {
            window.restore_window_end_state(attempt.previous);
        }
    }

    /// Canonical buffer-layout projection shared with redisplay skipping.
    pub(crate) fn buffer_layout_inputs(
        &self,
        buffer_id: BufferId,
    ) -> Option<BufferLayoutInputState> {
        let buffer = self.buffers.get(buffer_id)?;
        Some(BufferLayoutInputState {
            id: buffer.id,
            modified_tick: buffer.modified_tick(),
            chars_modified_tick: buffer.chars_modified_tick(),
            props_modified_tick: buffer.props_modified_tick(),
            overlay_modified_tick: buffer.overlay_modified_tick(),
            accessible_chars: buffer.accessible_char_region().range(),
            accessible_bytes: buffer.accessible_emacs_byte_region().range(),
            total_chars: buffer.total_char_len(),
            total_emacs_bytes: buffer.total_emacs_byte_len(),
        })
    }

    /// Capture the complete layout identity for one live window.
    ///
    /// This is the single freshness boundary for consumers of retained rows.
    /// Its frame/window/buffer projections are also used by the redisplay skip
    /// signature, so both cache policies evolve together.
    pub fn window_display_snapshot_freshness(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<WindowDisplaySnapshotFreshness> {
        let frame = self.frames.get(frame_id)?;
        let window = frame.window_layout_inputs(window_id)?;
        if window.buffer_id != buffer_id {
            return None;
        }
        Some(WindowDisplaySnapshotFreshness {
            context_instance_id: self.context_instance_id(),
            window_topology_generation: self.frames.window_topology_generation(),
            frame: frame.layout_inputs(),
            window,
            buffer: self.buffer_layout_inputs(buffer_id)?,
            face_change_count: self.face_change_count,
            display_var_change_count: self.display_var_change_count,
            redisplay_generation: self.redisplay_generation(),
            media_generation: self.media_generation(),
            function_epoch: self.obarray().function_epoch(),
        })
    }

    /// Return retained rows only while every layout input still matches.
    ///
    /// Older snapshots without a full token retain the legacy `buffer_modiff`
    /// gate. Fixtures with neither value explicitly opt into trusted synthetic
    /// geometry.
    pub fn fresh_window_display_snapshot(
        &self,
        frame_id: FrameId,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> Option<&WindowDisplaySnapshot> {
        let current = self.window_display_snapshot_freshness(frame_id, window_id, buffer_id)?;
        let snapshot = self.frames.get(frame_id)?.redisplay_snapshot(window_id)?;
        if let Some(recorded) = snapshot.layout_freshness {
            return (recorded == current).then_some(snapshot);
        }
        if let Some(recorded_modiff) = snapshot.buffer_modiff
            && recorded_modiff != current.buffer.modified_tick
        {
            return None;
        }
        Some(snapshot)
    }
}

impl FrameManager {
    fn window_display_state(
        &self,
        window_id: WindowId,
    ) -> Option<(&Frame, &WindowDisplayState, bool)> {
        self.frames.values().find_map(|frame| {
            frame.find_window(window_id).and_then(|window| {
                window
                    .display()
                    .map(|display| (frame, display, frame.minibuffer_window == Some(window_id)))
            })
        })
    }

    fn window_display_location(&self, window_id: WindowId) -> Option<(super::FrameId, bool)> {
        let frame_id = self.find_window_frame_id(window_id)?;
        let frame = self.get(frame_id)?;
        Some((frame_id, frame.minibuffer_window == Some(window_id)))
    }

    /// Return WINDOW-ID's Lisp-visible vertical scroll amount.
    pub fn window_vscroll(&self, window_id: WindowId, pixelwise: bool) -> Option<Value> {
        let frame_id = self.find_window_frame_id(window_id)?;
        let frame = self.get(frame_id)?;
        let Window::Leaf { vscroll, .. } = frame.find_window(window_id)? else {
            return None;
        };
        if frame.effective_window_system().is_none() {
            return Some(Value::fixnum(0));
        }
        let pixels = -i64::from(*vscroll);
        Some(if pixelwise {
            Value::fixnum(pixels)
        } else {
            canon_y_from_pixel_y(frame, pixels)
        })
    }

    /// Set WINDOW-ID's vertical scroll amount and return the Lisp-visible value.
    pub fn set_window_vscroll(
        &mut self,
        window_id: WindowId,
        vscroll: f64,
        pixelwise: bool,
        preserve_vscroll_p: bool,
    ) -> Option<Value> {
        let frame_id = self.find_window_frame_id(window_id)?;
        let line_height = self
            .get(frame_id)
            .map(frame_line_height)
            .unwrap_or(1)
            .max(1);
        if self
            .get(frame_id)
            .is_none_or(|frame| frame.effective_window_system().is_none())
        {
            return Some(Value::fixnum(0));
        }

        let next_pixels = if pixelwise {
            vscroll.trunc() as i64
        } else {
            (vscroll * line_height as f64).trunc() as i64
        };
        let next_raw = std::cmp::min(-next_pixels, 0).clamp(i64::from(i32::MIN), 0) as i32;

        let frame = self.get_mut(frame_id)?;
        if let Some(Window::Leaf {
            vscroll,
            preserve_vscroll_p: preserve,
            ..
        }) = frame.find_window_mut(window_id)
        {
            let changed = *vscroll != next_raw;
            *vscroll = next_raw;
            *preserve = preserve_vscroll_p;
            if changed {
                frame.remove_redisplay_snapshot(window_id);
            }
        }

        let frame = self.get(frame_id)?;
        if frame.effective_window_system().is_none() {
            return Some(Value::fixnum(0));
        }
        let Window::Leaf { vscroll, .. } = frame.find_window(window_id)? else {
            return None;
        };
        let pixels = -i64::from(*vscroll);
        Some(if pixelwise {
            Value::fixnum(pixels)
        } else {
            canon_y_from_pixel_y(frame, pixels)
        })
    }

    /// Return window display table object for WINDOW-ID, or nil when unset.
    pub fn window_display_table(&self, window_id: WindowId) -> Value {
        self.window_display_state(window_id)
            .map(|(_, display, _)| display.display_table)
            .unwrap_or(Value::NIL)
    }

    /// Set window display table object for WINDOW-ID.
    pub fn set_window_display_table(&mut self, window_id: WindowId, table: Value) {
        if let Some((frame_id, _)) = self.window_display_location(window_id)
            && let Some(display) = self
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(window_id))
                .and_then(Window::display_mut)
        {
            display.display_table = table;
        }
    }

    /// Return window cursor-type object for WINDOW-ID.
    ///
    /// GNU Emacs defaults to `t` when no explicit per-window cursor-type is set.
    pub fn window_cursor_type(&self, window_id: WindowId) -> Value {
        self.window_display_state(window_id)
            .map(|(_, display, _)| display.cursor_type)
            .unwrap_or(Value::T)
    }

    /// Set window cursor-type object for WINDOW-ID.
    pub fn set_window_cursor_type(&mut self, window_id: WindowId, cursor_type: Value) {
        if let Some((frame_id, _)) = self.window_display_location(window_id)
            && let Some(display) = self
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(window_id))
                .and_then(Window::display_mut)
        {
            display.cursor_type = cursor_type;
        }
    }

    /// Return whether WINDOW-ID's cursor is logically visible.
    pub fn window_cursor_visible(&self, window_id: WindowId) -> bool {
        self.window_display_state(window_id)
            .map(|(_, display, _)| !display.cursor_off_p)
            .unwrap_or(true)
    }

    /// Set whether WINDOW-ID's cursor is logically visible.
    ///
    /// This mirrors GNU's `w->cursor_off_p`: redisplay may continue to own a
    /// physical cursor geometry while Lisp has requested that it stay hidden.
    pub fn set_window_cursor_visible(&mut self, window_id: WindowId, visible: bool) {
        if let Some((frame_id, _)) = self.window_display_location(window_id)
            && let Some(display) = self
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(window_id))
                .and_then(Window::display_mut)
        {
            display.cursor_off_p = !visible;
        }
    }

    /// Return the effective fringe widths and raw flags for WINDOW-ID.
    pub fn window_fringes(&self, window_id: WindowId) -> Option<(i64, i64, bool, bool)> {
        let (frame, display, _) = self.window_display_state(window_id)?;
        if frame.effective_window_system().is_none() {
            return Some((0, 0, false, false));
        }
        Some((
            i64::from(if display.left_fringe_width >= 0 {
                display.left_fringe_width
            } else {
                frame_default_left_fringe_width(frame)
            }),
            i64::from(if display.right_fringe_width >= 0 {
                display.right_fringe_width
            } else {
                frame_default_right_fringe_width(frame)
            }),
            display.fringes_outside_margins,
            display.fringes_persistent,
        ))
    }

    /// Set raw window fringe settings. Returns true when the visible fringe state changed.
    pub fn set_window_fringes(
        &mut self,
        window_id: WindowId,
        left_width: Option<i32>,
        right_width: Option<i32>,
        outside_margins: bool,
        persistent: bool,
    ) -> bool {
        let previous = self.window_fringes(window_id);
        let Some((frame_id, _)) = self.window_display_location(window_id) else {
            return false;
        };
        if self
            .get(frame_id)
            .is_none_or(|frame| frame.effective_window_system().is_none())
        {
            return false;
        }
        if let Some(display) = self
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .and_then(Window::display_mut)
        {
            display.left_fringe_width = left_width.unwrap_or(-1);
            display.right_fringe_width = right_width.unwrap_or(-1);
            display.fringes_outside_margins = outside_margins;
            display.fringes_persistent = persistent;
        }
        self.window_fringes(window_id) != previous
    }

    /// Return raw window scroll-bar settings plus effective area metrics.
    pub fn window_scroll_bars(
        &self,
        window_id: WindowId,
    ) -> Option<(Value, i64, Value, Value, i64, Value, bool)> {
        let (frame, display, is_minibuffer) = self.window_display_state(window_id)?;
        Some((
            if display.scroll_bar_width >= 0 {
                Value::fixnum(i64::from(display.scroll_bar_width))
            } else {
                Value::NIL
            },
            vertical_scroll_bar_cols(frame, display),
            display.vertical_scroll_bar_type,
            if display.scroll_bar_height >= 0 {
                Value::fixnum(i64::from(display.scroll_bar_height))
            } else {
                Value::NIL
            },
            horizontal_scroll_bar_lines(frame, display, is_minibuffer),
            display.horizontal_scroll_bar_type,
            display.scroll_bars_persistent,
        ))
    }

    /// Return the effective vertical scroll-bar area width in pixels.
    pub fn window_scroll_bar_area_width(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, _)| vertical_scroll_bar_area_width(frame, display))
            .unwrap_or(0)
    }

    /// Return the effective left vertical scroll-bar area width in pixels.
    pub fn window_left_scroll_bar_area_width(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, _)| left_scroll_bar_area_width(frame, display))
            .unwrap_or(0)
    }

    /// Return the effective right vertical scroll-bar area width in pixels.
    pub fn window_right_scroll_bar_area_width(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, _)| right_scroll_bar_area_width(frame, display))
            .unwrap_or(0)
    }

    /// Return the effective horizontal scroll-bar area height in pixels.
    pub fn window_scroll_bar_area_height(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, is_minibuffer)| {
                horizontal_scroll_bar_area_height(frame, display, is_minibuffer)
            })
            .unwrap_or(0)
    }

    /// Set raw window scroll-bar settings. Returns true when the visible scroll-bar state changed.
    pub fn set_window_scroll_bars(
        &mut self,
        window_id: WindowId,
        width: Option<i32>,
        vertical_type: Value,
        height: Option<i32>,
        horizontal_type: Value,
        persistent: bool,
    ) -> bool {
        let previous = self.window_scroll_bars(window_id);
        let Some((frame_id, is_minibuffer)) = self.window_display_location(window_id) else {
            return false;
        };
        if self
            .get(frame_id)
            .is_none_or(|frame| frame.effective_window_system().is_none())
        {
            return false;
        }
        let mut next_vertical_type = vertical_type;
        if width == Some(0) {
            next_vertical_type = Value::NIL;
        }
        let mut next_horizontal_type = horizontal_type;
        if height == Some(0)
            || (is_minibuffer
                && HorizontalScrollBarType::from_symbol_value(&next_horizontal_type).is_none())
        {
            next_horizontal_type = Value::NIL;
        }
        if let Some(display) = self
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .and_then(Window::display_mut)
        {
            display.scroll_bar_width = width.unwrap_or(-1);
            display.vertical_scroll_bar_type = next_vertical_type;
            display.scroll_bar_height = height.unwrap_or(-1);
            display.horizontal_scroll_bar_type = next_horizontal_type;
            display.scroll_bars_persistent = persistent;
        }
        self.window_scroll_bars(window_id) != previous
    }

    /// Apply the window-owned state changes that GNU performs during
    /// `set_window_buffer`.
    pub fn apply_set_window_buffer_state(
        &mut self,
        window_id: WindowId,
        buffer_id: BufferId,
        window_start: LispCharPos1,
        point: LispCharPos1,
        preserve_display_state: bool,
        defaults: WindowBufferDisplayDefaults,
    ) {
        let Some(frame_id) = self.find_window_frame_id(window_id) else {
            return;
        };
        let (fringes_persistent, scroll_bars_persistent) = self
            .get(frame_id)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(Window::display)
            .map(|display| (display.fringes_persistent, display.scroll_bars_persistent))
            .unwrap_or((false, false));

        if let Some(Window::Leaf {
            buffer_id: leaf_buffer_id,
            window_start: leaf_window_start,
            position_markers,
            point: leaf_point,
            old_point,
            hscroll,
            min_hscroll,
            suspend_auto_hscroll,
            vscroll,
            preserve_vscroll_p,
            margins,
            ..
        }) = self
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
        {
            *leaf_buffer_id = buffer_id;
            *leaf_window_start = window_start.max(LispCharPos1::ONE);
            *position_markers = super::WindowPositionMarkerState::Detached;
            *leaf_point = point.max(LispCharPos1::ONE);
            if !preserve_display_state {
                *old_point = point.max(LispCharPos1::ONE);
                *hscroll = 0;
                // GNU resets the auto-hscroll lower bound and clears the
                // suspend flag on buffer switch (src/window.c:4368).
                *min_hscroll = 0;
                *suspend_auto_hscroll = false;
                *vscroll = 0;
                *preserve_vscroll_p = false;
            }
            if let Some(next_margins) = defaults.margins {
                *margins = next_margins;
            }
        }

        if let Some(next_fringes) = defaults.fringes
            && !fringes_persistent
        {
            let _ = self.set_window_fringes(
                window_id,
                next_fringes.left_width,
                next_fringes.right_width,
                next_fringes.outside_margins,
                false,
            );
        }
        if let Some(next_scroll_bars) = defaults.scroll_bars
            && !scroll_bars_persistent
        {
            let _ = self.set_window_scroll_bars(
                window_id,
                next_scroll_bars.vertical_width,
                next_scroll_bars.vertical_type,
                next_scroll_bars.horizontal_height,
                next_scroll_bars.horizontal_type,
                false,
            );
        }
    }
}
