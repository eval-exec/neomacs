use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use crate::emacs_core::hook_runtime;
use crate::gc_trace::GcTrace;

// ===========================================================================
// Hook system
// ===========================================================================

pub(crate) fn builtin_run_hooks(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    hook_runtime::run_named_hooks(eval, &args)
}

pub(crate) fn builtin_run_hook_with_args(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-hook-with-args", &args, 1)?;
    hook_runtime::run_named_hook_with_args(eval, &args)
}

pub(crate) fn builtin_run_hook_with_args_until_success(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-hook-with-args-until-success", &args, 1)?;
    hook_runtime::run_named_hook_with_args_until_success(eval, &args)
}

pub(crate) fn builtin_run_hook_with_args_until_failure(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-hook-with-args-until-failure", &args, 1)?;
    hook_runtime::run_named_hook_with_args_until_failure(eval, &args)
}

pub(crate) fn builtin_run_hook_wrapped(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-hook-wrapped", &args, 2)?;
    hook_runtime::run_named_hook_wrapped(eval, &args)
}

fn expect_optional_live_frame_designator(
    value: &Value,
    eval: &super::eval::Context,
) -> Result<(), Flow> {
    expect_optional_live_frame_designator_in_state(value, &eval.frames)
}

fn expect_optional_live_frame_designator_in_state(
    value: &Value,
    frames: &crate::window::FrameManager,
) -> Result<(), Flow> {
    if value.is_nil() {
        return Ok(());
    }
    if value.is_frame()
        && let Some(fid) = value.as_frame_id()
        && frames.get(crate::window::FrameId(fid)).is_some()
    {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("frame-live-p"), *value],
    ))
}

#[derive(Clone, Copy)]
struct HookCallerContextState {
    selected_frame_id: Option<crate::window::FrameId>,
    selected_window_id: Option<crate::window::WindowId>,
    current_buffer_id: Option<crate::buffer::BufferId>,
}

fn save_hook_caller_context(eval: &super::eval::Context) -> HookCallerContextState {
    let selected_frame_id = eval.frames.selected_frame().map(|frame| frame.id);
    let selected_window_id = selected_frame_id
        .and_then(|frame_id| eval.frames.get(frame_id).map(|frame| frame.selected_window));
    HookCallerContextState {
        selected_frame_id,
        selected_window_id,
        current_buffer_id: eval.buffers.current_buffer_id(),
    }
}

fn window_buffer_id_in_state(
    eval: &super::eval::Context,
    frame_id: crate::window::FrameId,
    window_id: crate::window::WindowId,
) -> Option<crate::buffer::BufferId> {
    eval.frames
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.buffer_id())
}

fn select_frame_window_for_hook_context(
    eval: &mut super::eval::Context,
    frame_id: crate::window::FrameId,
    window_id: crate::window::WindowId,
) {
    let _ = eval.frames.select_frame(frame_id);
    eval.sync_keyboard_terminal_owner();
    if let Some(frame) = eval.frames.get_mut(frame_id) {
        let _ = frame.select_window(window_id);
    }
    if let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, window_id) {
        let _ = eval.switch_current_buffer(buffer_id);
    }
}

fn restore_hook_caller_context(eval: &mut super::eval::Context, saved: HookCallerContextState) {
    if let Some(frame_id) = saved
        .selected_frame_id
        .filter(|frame_id| eval.frames.get(*frame_id).is_some())
    {
        let _ = eval.frames.select_frame(frame_id);
        eval.sync_keyboard_terminal_owner();
        if let Some(window_id) = saved.selected_window_id
            && let Some(frame) = eval.frames.get_mut(frame_id)
        {
            let _ = frame.select_window(window_id);
        }
    }
    if let Some(buffer_id) = saved.current_buffer_id {
        eval.restore_current_buffer_if_live(buffer_id);
    }
}

#[derive(Clone, Copy)]
struct LiveWindowHookState {
    window_id: crate::window::WindowId,
    buffer_id: crate::buffer::BufferId,
    bounds: crate::window::Rect,
}

#[derive(Clone)]
struct FrameWindowHookPlan {
    frame_id: crate::window::FrameId,
    frame_buffer_change: bool,
    frame_size_change: bool,
    frame_selected_window_change: bool,
    frame_state_change: bool,
    /// GNU `run_window_change_functions` runs
    /// `run_window_configuration_change_hook' when a frame's configuration
    /// changed, i.e. `frame_size_change || window_deleted'
    /// (`src/window.c:4308-4312`).
    frame_configuration_change: bool,
    local_buffer_windows: Vec<crate::window::WindowId>,
    local_size_windows: Vec<crate::window::WindowId>,
    local_selection_windows: Vec<crate::window::WindowId>,
    local_state_windows: Vec<crate::window::WindowId>,
}

fn push_unique_window(
    windows: &mut Vec<crate::window::WindowId>,
    window_id: crate::window::WindowId,
) {
    if !windows.contains(&window_id) {
        windows.push(window_id);
    }
}

fn live_windows_for_hook_plan(frame: &crate::window::Frame) -> Vec<LiveWindowHookState> {
    let mut windows = Vec::new();
    for window_id in frame.window_list() {
        let Some(window) = frame.find_window(window_id) else {
            continue;
        };
        let Some(buffer_id) = window.buffer_id() else {
            continue;
        };
        windows.push(LiveWindowHookState {
            window_id,
            buffer_id,
            bounds: *window.bounds(),
        });
    }
    if let Some(minibuffer_window) = frame.minibuffer_window
        && let Some(window) = frame.find_window(minibuffer_window)
        && let Some(buffer_id) = window.buffer_id()
    {
        windows.push(LiveWindowHookState {
            window_id: minibuffer_window,
            buffer_id,
            bounds: *window.bounds(),
        });
    }
    windows
}

fn frame_window_hook_record_from_live_state(
    frame: &crate::window::Frame,
    was_selected_frame: bool,
) -> crate::window::FrameWindowHookRecord {
    let windows = live_windows_for_hook_plan(frame)
        .into_iter()
        .map(|window| {
            (
                window.window_id,
                crate::window::WindowHookSnapshot {
                    buffer_id: window.buffer_id,
                    bounds: window.bounds,
                },
            )
        })
        .collect();
    crate::window::FrameWindowHookRecord {
        windows,
        selected_window: Some(frame.selected_window),
        was_selected_frame,
    }
}

fn run_window_local_hook_values(
    eval: &mut super::eval::Context,
    frame_id: crate::window::FrameId,
    window_ids: &[crate::window::WindowId],
    hook_name: &str,
    hook_sym: crate::emacs_core::intern::SymId,
) -> EvalResult {
    if window_ids.is_empty() {
        return Ok(Value::NIL);
    }

    let saved = save_hook_caller_context(eval);
    let result = (|| -> EvalResult {
        for window_id in window_ids {
            let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, *window_id) else {
                continue;
            };
            let has_local_hook = eval
                .buffers
                .get(buffer_id)
                .and_then(|buffer| buffer.get_buffer_local_binding(hook_name))
                .is_some();
            if !has_local_hook {
                continue;
            }
            select_frame_window_for_hook_context(eval, frame_id, *window_id);
            let Some(local_hook_value) = eval
                .buffers
                .current_buffer()
                .and_then(|buffer| buffer.buffer_local_value(hook_name))
            else {
                continue;
            };
            let _ = hook_runtime::safe_run_hook_value(
                eval,
                hook_sym,
                local_hook_value,
                &[Value::make_window(window_id.0)],
                false,
            )?;
        }
        Ok(Value::NIL)
    })();
    restore_hook_caller_context(eval, saved);
    result
}

fn run_window_default_hook_value(
    eval: &mut super::eval::Context,
    frame_id: crate::window::FrameId,
    run_hook: bool,
    hook_sym: crate::emacs_core::intern::SymId,
) -> EvalResult {
    if !run_hook {
        return Ok(Value::NIL);
    }
    let global_hook_value = eval
        .obarray
        .default_value_id(hook_sym)
        .copied()
        .unwrap_or(Value::NIL);
    if global_hook_value.is_nil() {
        return Ok(Value::NIL);
    }

    let saved = save_hook_caller_context(eval);
    let result = (|| -> EvalResult {
        let selected_window = eval.frames.get(frame_id).map(|frame| frame.selected_window);
        if let Some(selected_window) = selected_window {
            select_frame_window_for_hook_context(eval, frame_id, selected_window);
        } else {
            let _ = eval.frames.select_frame(frame_id);
            eval.sync_keyboard_terminal_owner();
        }
        let _ = hook_runtime::safe_run_hook_value(
            eval,
            hook_sym,
            global_hook_value,
            &[Value::make_frame(frame_id.0)],
            false,
        )?;
        Ok(Value::NIL)
    })();
    restore_hook_caller_context(eval, saved);
    result
}

pub(crate) fn run_redisplay_window_change_hooks(eval: &mut super::eval::Context) -> EvalResult {
    // Mirrors GNU `run_window_change_functions` (window.c:4116):
    //   specbind (Qinhibit_redisplay, Qt);
    // Any hook function that calls `redisplay` (directly or indirectly)
    // would otherwise re-enter here and infinitely recurse. The specpdl
    // entry is popped when we return, restoring the previous value.
    let specpdl_count = eval.specpdl.len();
    eval.try_specbind_or_unwind_to(
        specpdl_count,
        crate::emacs_core::intern::intern("inhibit-redisplay"),
        Value::T,
    )?;

    let result = run_redisplay_window_change_hooks_inner(eval);

    eval.unbind_to_with_result(specpdl_count, result)
}

fn run_redisplay_window_change_hooks_inner(eval: &mut super::eval::Context) -> EvalResult {
    let frame_ids = eval.frames.frame_list();
    let selected_frame_id = eval.frames.selected_frame().map(|frame| frame.id);
    let mut plans = Vec::new();

    for frame_id in &frame_ids {
        let Some(frame) = eval.frames.get(*frame_id) else {
            continue;
        };
        let previous_record = frame.window_hook_record.clone();
        let current_windows = live_windows_for_hook_plan(frame);
        let selected_window = Some(frame.selected_window);
        let frame_selected_window_change = previous_record.selected_window != selected_window;
        let frame_selected_change =
            previous_record.was_selected_frame != (selected_frame_id == Some(*frame_id));
        let window_deleted = previous_record.windows.keys().any(|window_id| {
            !current_windows
                .iter()
                .any(|window| window.window_id == *window_id)
        });

        let mut local_buffer_windows = Vec::new();
        let mut local_size_windows = Vec::new();
        let mut local_selection_windows = Vec::new();
        let mut local_state_windows = Vec::new();

        for window in &current_windows {
            let previous = previous_record.windows.get(&window.window_id);
            let buffer_changed = previous.is_none()
                || previous.is_some_and(|entry| entry.buffer_id != window.buffer_id);
            let size_changed =
                previous.is_none() || previous.is_some_and(|entry| entry.bounds != window.bounds);
            let selection_changed = frame_selected_window_change
                && (previous_record.selected_window == Some(window.window_id)
                    || selected_window == Some(window.window_id));

            if buffer_changed {
                push_unique_window(&mut local_buffer_windows, window.window_id);
                push_unique_window(&mut local_size_windows, window.window_id);
                push_unique_window(&mut local_state_windows, window.window_id);
            }
            if size_changed {
                push_unique_window(&mut local_size_windows, window.window_id);
                push_unique_window(&mut local_state_windows, window.window_id);
            }
            if selection_changed {
                push_unique_window(&mut local_selection_windows, window.window_id);
                push_unique_window(&mut local_state_windows, window.window_id);
            }
        }

        let frame_buffer_change = !local_buffer_windows.is_empty();
        let frame_size_change = !local_size_windows.is_empty();
        let frame_state_change = frame.window_state_change
            || frame_selected_change
            || frame_selected_window_change
            || frame_buffer_change
            || frame_size_change
            || window_deleted;

        plans.push(FrameWindowHookPlan {
            frame_id: *frame_id,
            frame_buffer_change,
            frame_size_change,
            frame_selected_window_change,
            frame_state_change,
            frame_configuration_change: frame_size_change || window_deleted,
            local_buffer_windows,
            local_size_windows,
            local_selection_windows,
            local_state_windows,
        });
    }

    let window_buffer_change_functions =
        hook_runtime::hook_symbol_by_name(eval, "window-buffer-change-functions");
    let window_size_change_functions =
        hook_runtime::hook_symbol_by_name(eval, "window-size-change-functions");
    let window_selection_change_functions =
        hook_runtime::hook_symbol_by_name(eval, "window-selection-change-functions");
    let window_state_change_functions =
        hook_runtime::hook_symbol_by_name(eval, "window-state-change-functions");
    let window_state_change_hook =
        hook_runtime::hook_symbol_by_name(eval, "window-state-change-hook");

    let mut run_window_state_change_hook = false;
    for plan in &plans {
        if eval.frames.get(plan.frame_id).is_none() {
            continue;
        }
        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_buffer_windows,
            "window-buffer-change-functions",
            window_buffer_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_buffer_change,
            window_buffer_change_functions,
        )?;

        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_size_windows,
            "window-size-change-functions",
            window_size_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_size_change || plan.frame_buffer_change,
            window_size_change_functions,
        )?;

        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_selection_windows,
            "window-selection-change-functions",
            window_selection_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_selected_window_change,
            window_selection_change_functions,
        )?;

        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_state_windows,
            "window-state-change-functions",
            window_state_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_state_change,
            window_state_change_functions,
        )?;
        run_window_state_change_hook |= plan.frame_state_change;

        // GNU `run_window_change_functions` (window.c:4308-4312) runs
        // `window-configuration-change-hook' (via
        // `run_window_configuration_change_hook') when the frame's window
        // configuration changed (a window changed size or was deleted).  This
        // is the redisplay-driven home of the hook -- so in batch (no
        // redisplay) it never fires from bare `split-window'/`delete-window',
        // matching GNU.
        if plan.frame_configuration_change && eval.frames.get(plan.frame_id).is_some() {
            let _ = builtin_run_window_configuration_change_hook(
                eval,
                vec![Value::make_frame(plan.frame_id.0)],
            )?;
        }
    }

    if run_window_state_change_hook {
        let _ = hook_runtime::safe_run_named_hook(eval, window_state_change_hook, &[])?;
    }

    let selected_frame_id = eval.frames.selected_frame().map(|frame| frame.id);
    for frame_id in frame_ids {
        let was_selected_frame = selected_frame_id == Some(frame_id);
        if let Some(frame) = eval.frames.get_mut(frame_id) {
            // GNU `window_change_record` (`src/window.c:3954-3990`)
            // records the current selected_window into
            // `frame->old_selected_window` exactly here, after
            // running the change hooks. neomacs mirrors that
            // step. Window audit Critical 8 in
            // `drafts/window-system-audit.md`.
            frame.old_selected_window = Some(frame.selected_window);
            frame.window_hook_record =
                frame_window_hook_record_from_live_state(frame, was_selected_frame);
            frame.window_state_change = false;
        }
    }

    Ok(Value::NIL)
}

pub(super) fn expect_optional_live_window_designator(
    value: &Value,
    eval: &super::eval::Context,
) -> Result<(), Flow> {
    if value.is_nil() {
        return Ok(());
    }
    if value.is_window()
        && let Some(wid) = value.as_window_id()
        && eval.frames.is_live_window_id(crate::window::WindowId(wid))
    {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("window-live-p"), *value],
    ))
}

const WINDOW_CONFIGURATION_TAG: &str = "window-configuration";
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const SAVE_SELECTED_WINDOW_STATE_TAG: &str = "save-selected-window--state";

/// Whether restoring a window configuration also selects its saved frame.
///
/// This is the typed form of GNU `set-window-configuration`'s
/// DONT-SET-FRAME argument.  Naming both states prevents the easy-to-miss
/// inversion caused by passing the Lisp flag deeper into the window code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectedFrameRestoration {
    RestoreSaved,
    KeepSelected,
}

/// Whether restoring a window configuration replaces the frame's live
/// minibuffer window with the saved one.
///
/// This is the typed form of GNU `set-window-configuration`'s
/// DONT-SET-MINIWINDOW argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinibufferWindowRestoration {
    RestoreSaved,
    KeepCurrent,
}

/// Exhaustive restore policy shared by the Lisp builtin and native unwind
/// actions.  Callers cannot accidentally swap the two same-shaped Lisp flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowConfigurationRestoreOptions {
    pub(crate) selected_frame: SelectedFrameRestoration,
    pub(crate) minibuffer_window: MinibufferWindowRestoration,
}

/// A validated, GC-traceable window configuration captured from live runtime
/// state.  Native unwind callers cannot manufacture one from an unrelated
/// `Value`; they must cross this constructor boundary.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SavedWindowConfiguration(Value);

impl SavedWindowConfiguration {
    pub(crate) fn capture(eval: &mut super::eval::Context, frame: Value) -> Result<Self, Flow> {
        let configuration = builtin_current_window_configuration(eval, vec![frame])?;
        debug_assert!(configuration.is_window_configuration());
        Ok(Self(configuration))
    }

    pub(crate) fn trace_value(self) -> Value {
        self.0
    }

    pub(crate) fn restore(
        self,
        eval: &mut super::eval::Context,
        options: WindowConfigurationRestoreOptions,
    ) -> EvalResult {
        set_window_configuration_with_options(eval, self.0, options)
    }
}

impl Default for WindowConfigurationRestoreOptions {
    fn default() -> Self {
        Self {
            selected_frame: SelectedFrameRestoration::RestoreSaved,
            minibuffer_window: MinibufferWindowRestoration::RestoreSaved,
        }
    }
}

impl WindowConfigurationRestoreOptions {
    fn from_lisp_args(args: &[Value]) -> Self {
        Self {
            selected_frame: if args.get(1).is_some_and(|value| value.is_truthy()) {
                SelectedFrameRestoration::KeepSelected
            } else {
                SelectedFrameRestoration::RestoreSaved
            },
            minibuffer_window: if args.get(2).is_some_and(|value| value.is_truthy()) {
                MinibufferWindowRestoration::KeepCurrent
            } else {
                MinibufferWindowRestoration::RestoreSaved
            },
        }
    }
}

struct WindowConfigurationSnapshot {
    frame_id: crate::window::FrameId,
    root_window: crate::window::Window,
    selected_window: crate::window::WindowId,
    current_buffer: Option<crate::buffer::BufferId>,
    minibuffer_window: Option<crate::window::WindowId>,
    minibuffer_leaf: Option<crate::window::Window>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisconnectedWindowPoint {
    PreserveSelectedBuffer,
    PreserveLastSelectedWindow(crate::window::WindowId),
    CommitWindowPoint(LispCharPos1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OutgoingWindowBuffer {
    window_id: crate::window::WindowId,
    buffer_id: crate::buffer::BufferId,
    window_start: LispCharPos1,
    window_point: LispCharPos1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReusedWindowHistoryTransition {
    PreserveLiveHistory,
    RecordOutgoingBuffer {
        outgoing: OutgoingWindowBuffer,
        incoming_buffer_id: crate::buffer::BufferId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SavedWindowBufferRestoration {
    RestoreSavedBuffer,
    KeepReusedWindowBuffer {
        buffer_id: crate::buffer::BufferId,
        point: LispCharPos1,
    },
    FindSubstituteBuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CurrentWindowBuffer {
    buffer_id: crate::buffer::BufferId,
    point: LispCharPos1,
}

fn apply_saved_window_buffer_restoration(
    window: &mut crate::window::Window,
    current_buffers: &HashMap<crate::window::WindowId, CurrentWindowBuffer>,
    buffers: &mut crate::buffer::BufferManager,
) {
    match window {
        crate::window::Window::Leaf {
            id,
            buffer_id,
            window_start,
            position_markers,
            point,
            old_point,
            ..
        } => {
            let restoration = if buffers.get(*buffer_id).is_some() {
                SavedWindowBufferRestoration::RestoreSavedBuffer
            } else if let Some(current) = current_buffers.get(id) {
                SavedWindowBufferRestoration::KeepReusedWindowBuffer {
                    buffer_id: current.buffer_id,
                    point: current.point,
                }
            } else {
                SavedWindowBufferRestoration::FindSubstituteBuffer
            };

            match restoration {
                SavedWindowBufferRestoration::RestoreSavedBuffer => {}
                SavedWindowBufferRestoration::KeepReusedWindowBuffer {
                    buffer_id: current_buffer_id,
                    point: current_point,
                } => {
                    *buffer_id = current_buffer_id;
                    *window_start = LispCharPos1::ONE;
                    *point = current_point;
                    *old_point = current_point;
                    *position_markers = crate::window::WindowPositionMarkerState::Detached;
                    crate::window::window_markers::attach_window_position_markers(buffers, window);
                }
                SavedWindowBufferRestoration::FindSubstituteBuffer => {}
            }
        }
        crate::window::Window::Internal { children, .. } => {
            for child in children {
                apply_saved_window_buffer_restoration(child, current_buffers, buffers);
            }
        }
    }
}

fn prepare_saved_window_buffer_restoration(
    eval: &mut super::eval::Context,
    snapshot: &mut WindowConfigurationSnapshot,
) {
    let globally_selected_window = eval
        .frames
        .selected_frame()
        .map(|frame| frame.selected_window);
    let current_buffers = eval
        .frames
        .get(snapshot.frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| {
                    let crate::window::Window::Leaf {
                        buffer_id, point, ..
                    } = frame.find_window(window_id)?
                    else {
                        return None;
                    };
                    let point = if globally_selected_window == Some(window_id) {
                        eval.buffers
                            .get(*buffer_id)
                            .map(|buffer| {
                                LispCharPos1::from_one_based_usize(
                                    buffer.point_char_pos().get().saturating_add(1),
                                )
                            })
                            .unwrap_or(*point)
                    } else {
                        *point
                    };
                    Some((
                        window_id,
                        CurrentWindowBuffer {
                            buffer_id: *buffer_id,
                            point,
                        },
                    ))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    apply_saved_window_buffer_restoration(
        &mut snapshot.root_window,
        &current_buffers,
        &mut eval.buffers,
    );
}

fn collect_leaf_buffer_ids(
    window: &crate::window::Window,
    buffers: &mut HashMap<crate::window::WindowId, crate::buffer::BufferId>,
) {
    match window {
        crate::window::Window::Leaf { id, buffer_id, .. } => {
            buffers.insert(*id, *buffer_id);
        }
        crate::window::Window::Internal { children, .. } => {
            for child in children {
                collect_leaf_buffer_ids(child, buffers);
            }
        }
    }
}

fn merge_live_window_histories(
    window: &mut crate::window::Window,
    live_histories: &HashMap<crate::window::WindowId, crate::window::WindowHistoryState>,
) {
    match window {
        crate::window::Window::Leaf { id, history, .. } => {
            if let Some(live_history) = live_histories.get(id) {
                *history = live_history.clone();
            }
        }
        crate::window::Window::Internal { children, .. } => {
            for child in children {
                merge_live_window_histories(child, live_histories);
            }
        }
    }
}

fn prepare_reused_window_histories(
    eval: &mut super::eval::Context,
    snapshot: &mut WindowConfigurationSnapshot,
) -> Result<(), Flow> {
    let mut saved_buffers = HashMap::new();
    collect_leaf_buffer_ids(&snapshot.root_window, &mut saved_buffers);

    let globally_selected_window = eval
        .frames
        .selected_frame()
        .map(|frame| frame.selected_window);
    let transitions = eval
        .frames
        .get(snapshot.frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| {
                    let saved_buffer_id = saved_buffers.get(&window_id).copied()?;
                    let crate::window::Window::Leaf {
                        buffer_id,
                        window_start,
                        point,
                        ..
                    } = frame.find_window(window_id)?
                    else {
                        return None;
                    };
                    let window_point = if globally_selected_window == Some(window_id) {
                        eval.buffers
                            .get(*buffer_id)
                            .map(|buffer| {
                                LispCharPos1::from_one_based_usize(
                                    buffer.point_char_pos().get().saturating_add(1),
                                )
                            })
                            .unwrap_or(*point)
                    } else {
                        *point
                    };
                    let outgoing = OutgoingWindowBuffer {
                        window_id,
                        buffer_id: *buffer_id,
                        window_start: *window_start,
                        window_point,
                    };
                    if outgoing.buffer_id != saved_buffer_id
                        && eval.buffers.get(saved_buffer_id).is_some()
                    {
                        Some(ReusedWindowHistoryTransition::RecordOutgoingBuffer {
                            outgoing,
                            incoming_buffer_id: saved_buffer_id,
                        })
                    } else {
                        Some(ReusedWindowHistoryTransition::PreserveLiveHistory)
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for transition in transitions {
        match transition {
            ReusedWindowHistoryTransition::PreserveLiveHistory => {}
            ReusedWindowHistoryTransition::RecordOutgoingBuffer {
                outgoing,
                incoming_buffer_id,
            } => {
                let run_buffer_list_hook = {
                    let (frames, buffers, minibuffers) =
                        (&mut eval.frames, &eval.buffers, &eval.minibuffers);
                    super::window_cmds::record_window_buffer_change_history_in_state(
                        frames,
                        minibuffers,
                        buffers,
                        snapshot.frame_id,
                        outgoing.window_id,
                        super::window_cmds::WindowBufferHistoryChange {
                            outgoing_buffer_id: outgoing.buffer_id,
                            incoming_buffer_id,
                            outgoing_window_start: outgoing.window_start,
                            outgoing_window_point: outgoing.window_point,
                        },
                    )?
                };
                if run_buffer_list_hook {
                    super::super::buffer::run_buffer_list_update_hook(eval)?;
                }
            }
        }
    }

    let live_histories = eval
        .frames
        .get(snapshot.frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| {
                    frame
                        .find_window(window_id)
                        .and_then(crate::window::Window::history)
                        .cloned()
                        .map(|history| (window_id, history))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    merge_live_window_histories(&mut snapshot.root_window, &live_histories);
    Ok(())
}

fn live_window_displays_buffer(
    frames: &crate::window::FrameManager,
    window_id: crate::window::WindowId,
    buffer_id: crate::buffer::BufferId,
) -> bool {
    frames
        .lookup_window(window_id)
        .and_then(crate::window::Window::buffer_id)
        == Some(buffer_id)
}

fn disconnected_window_point(
    eval: &super::eval::Context,
    outgoing: OutgoingWindowBuffer,
) -> DisconnectedWindowPoint {
    let selected_buffer_id = eval
        .frames
        .selected_frame()
        .and_then(|frame| frame.find_window(frame.selected_window))
        .and_then(crate::window::Window::buffer_id);
    if selected_buffer_id == Some(outgoing.buffer_id) {
        return DisconnectedWindowPoint::PreserveSelectedBuffer;
    }

    let last_selected_window = eval
        .buffers
        .get(outgoing.buffer_id)
        .and_then(|buffer| buffer.last_selected_window);
    if let Some(window_id) = last_selected_window
        && window_id != outgoing.window_id
        && live_window_displays_buffer(&eval.frames, window_id, outgoing.buffer_id)
    {
        return DisconnectedWindowPoint::PreserveLastSelectedWindow(window_id);
    }

    DisconnectedWindowPoint::CommitWindowPoint(outgoing.window_point)
}

/// Disconnect every current root leaf from its buffer before restoring a
/// saved window tree.  This is the neomacs counterpart of GNU
/// `delete_all_child_windows` calling `unshow_buffer` for each outgoing leaf.
fn unshow_frame_root_buffers(eval: &mut super::eval::Context, frame_id: crate::window::FrameId) {
    // GNU first swaps the globally selected window's live buffer point into
    // its window marker.  The frame being restored need not be selected.
    if let Some(selected_frame_id) = eval.frames.selected_frame().map(|frame| frame.id) {
        super::window_cmds::remember_selected_window_point_in_state(
            &mut eval.frames,
            &mut eval.buffers,
            selected_frame_id,
        );
    }

    let outgoing_windows = eval
        .frames
        .get(frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| match frame.find_window(window_id) {
                    Some(crate::window::Window::Leaf {
                        buffer_id,
                        window_start,
                        point,
                        ..
                    }) => Some(OutgoingWindowBuffer {
                        window_id,
                        buffer_id: *buffer_id,
                        window_start: *window_start,
                        window_point: *point,
                    }),
                    Some(crate::window::Window::Internal { .. }) | None => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for outgoing in outgoing_windows {
        let point = disconnected_window_point(eval, outgoing);
        if let Some(buffer) = eval.buffers.get_mut(outgoing.buffer_id) {
            buffer.last_window_start = outgoing.window_start.max(LispCharPos1::ONE);
        }

        match point {
            DisconnectedWindowPoint::PreserveSelectedBuffer => {}
            DisconnectedWindowPoint::PreserveLastSelectedWindow(window_id) => {
                debug_assert!(live_window_displays_buffer(
                    &eval.frames,
                    window_id,
                    outgoing.buffer_id
                ));
            }
            DisconnectedWindowPoint::CommitWindowPoint(window_point) => {
                let byte_pos = eval.buffers.get(outgoing.buffer_id).map(|buffer| {
                    buffer.lisp_pos_to_emacs_byte_pos(window_point.max(LispCharPos1::ONE))
                });
                if let Some(byte_pos) = byte_pos {
                    let _ = eval
                        .buffers
                        .goto_buffer_emacs_byte_pos(outgoing.buffer_id, byte_pos);
                }
            }
        }

        if let Some(buffer) = eval.buffers.get_mut(outgoing.buffer_id)
            && buffer.last_selected_window == Some(outgoing.window_id)
        {
            buffer.last_selected_window = None;
        }
    }
}

impl WindowConfigurationSnapshot {
    fn clone_for_restore(&self, buffers: &mut crate::buffer::BufferManager) -> Self {
        Self {
            frame_id: self.frame_id,
            root_window:
                crate::window::window_markers::clone_window_tree_with_independent_position_markers(
                    buffers,
                    &self.root_window,
                ),
            selected_window: self.selected_window,
            current_buffer: self.current_buffer,
            minibuffer_window: self.minibuffer_window,
            minibuffer_leaf: self.minibuffer_leaf.as_ref().map(|window| {
                crate::window::window_markers::clone_window_tree_with_independent_position_markers(
                    buffers, window,
                )
            }),
        }
    }

    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.root_window.trace_roots(roots);
        if let Some(minibuffer) = &self.minibuffer_leaf {
            minibuffer.trace_roots(roots);
        }
    }
}

fn window_configuration_snapshot_roots(snapshot: &WindowConfigurationSnapshot) -> Value {
    let mut roots = Vec::new();
    snapshot.trace_roots(&mut roots);
    Value::vector(roots)
}

fn normalize_selected_window_point_in_snapshot(
    snapshot: &mut WindowConfigurationSnapshot,
    buffers: &mut crate::buffer::BufferManager,
) {
    let selected_buffer_id = snapshot
        .root_window
        .find(snapshot.selected_window)
        .or_else(|| {
            snapshot
                .minibuffer_leaf
                .as_ref()
                .filter(|window| window.id() == snapshot.selected_window)
        })
        .and_then(|window| window.buffer_id());
    let Some(buffer_id) = selected_buffer_id else {
        return;
    };
    let Some(point) = buffers
        .get(buffer_id)
        .map(|buffer| buffer.point_char_pos().get().saturating_add(1))
    else {
        return;
    };

    if let Some(window) = snapshot.root_window.find_mut(snapshot.selected_window) {
        crate::window::window_markers::set_window_point_with_marker(
            buffers,
            window,
            LispCharPos1::from_one_based_usize(point),
        );
        return;
    }

    if let Some(window) = snapshot
        .minibuffer_leaf
        .as_mut()
        .filter(|window| window.id() == snapshot.selected_window)
    {
        crate::window::window_markers::set_window_point_with_marker(
            buffers,
            window,
            LispCharPos1::from_one_based_usize(point),
        );
    }
}

fn persistent_window_parameter_keys(value: Value) -> Vec<Value> {
    let mut keys = Vec::new();
    let mut cursor = value;
    let mut remaining = 1024;
    while cursor.is_cons() && remaining > 0 {
        let entry = cursor.cons_car();
        if entry.is_cons() && entry.cons_cdr().is_truthy() {
            keys.push(entry.cons_car());
        }
        cursor = cursor.cons_cdr();
        remaining -= 1;
    }
    keys
}

fn window_parameter_is_persistent(key: &Value, persistent_keys: &[Value]) -> bool {
    persistent_keys
        .iter()
        .any(|persistent| crate::emacs_core::value::eq_value(key, persistent))
}

fn save_persistent_window_parameters(
    window: &mut crate::window::Window,
    persistent_keys: &[Value],
) {
    let current = window.parameters().clone();
    let saved = persistent_keys
        .iter()
        .map(|key| {
            let value = current
                .iter()
                .find(|(existing_key, _)| crate::emacs_core::value::eq_value(existing_key, key))
                .map(|(_, value)| *value)
                .unwrap_or(Value::NIL);
            (*key, value)
        })
        .collect();
    *window.parameters_mut() = saved;
    if let crate::window::Window::Internal { children, .. } = window {
        for child in children {
            save_persistent_window_parameters(child, persistent_keys);
        }
    }
}

fn save_snapshot_persistent_window_parameters(
    snapshot: &mut WindowConfigurationSnapshot,
    persistent_parameters: Value,
) -> Vec<Value> {
    let persistent_keys = persistent_window_parameter_keys(persistent_parameters);
    save_persistent_window_parameters(&mut snapshot.root_window, &persistent_keys);
    if let Some(minibuffer) = &mut snapshot.minibuffer_leaf {
        save_persistent_window_parameters(minibuffer, &persistent_keys);
    }
    persistent_keys
}

fn collect_window_parameters(
    window: &crate::window::Window,
    out: &mut HashMap<crate::window::WindowId, Vec<(Value, Value)>>,
) {
    out.insert(window.id(), window.parameters().clone());
    if let crate::window::Window::Internal { children, .. } = window {
        for child in children {
            collect_window_parameters(child, out);
        }
    }
}

fn collect_frame_window_parameters(
    frame: &crate::window::Frame,
) -> HashMap<crate::window::WindowId, Vec<(Value, Value)>> {
    let mut parameters = HashMap::new();
    collect_window_parameters(&frame.root_window, &mut parameters);
    if let Some(minibuffer) = &frame.minibuffer_leaf {
        collect_window_parameters(minibuffer, &mut parameters);
    }
    parameters
}

fn merge_restored_window_parameters(
    window: &mut crate::window::Window,
    live_parameters: &HashMap<crate::window::WindowId, Vec<(Value, Value)>>,
) {
    let saved_parameters = window.parameters().clone();
    let saved_keys = saved_parameters
        .iter()
        .map(|(key, _)| *key)
        .collect::<Vec<_>>();
    let live = live_parameters.get(&window.id());
    let mut merged = live
        .into_iter()
        .flat_map(|params| params.iter().copied())
        .filter(|(key, _)| !window_parameter_is_persistent(key, &saved_keys))
        .collect::<Vec<_>>();

    for (key, saved_value) in saved_parameters {
        if saved_value.is_nil() {
            if live.is_some_and(|params| {
                params.iter().any(|(live_key, live_value)| {
                    crate::emacs_core::value::eq_value(live_key, &key) && live_value.is_truthy()
                })
            }) {
                merged.insert(0, (key, Value::NIL));
            }
        } else {
            merged.insert(0, (key, saved_value));
        }
    }

    *window.parameters_mut() = merged;
    if let crate::window::Window::Internal { children, .. } = window {
        for child in children {
            merge_restored_window_parameters(child, live_parameters);
        }
    }
}

fn merge_snapshot_window_parameters(
    snapshot: &mut WindowConfigurationSnapshot,
    live_parameters: &HashMap<crate::window::WindowId, Vec<(Value, Value)>>,
) {
    merge_restored_window_parameters(&mut snapshot.root_window, live_parameters);
    if let Some(minibuffer) = &mut snapshot.minibuffer_leaf {
        merge_restored_window_parameters(minibuffer, live_parameters);
    }
}

thread_local! {
    static WINDOW_CONFIGURATION_SNAPSHOTS: RefCell<HashMap<i64, WindowConfigurationSnapshot>> =
        RefCell::new(HashMap::new());
}

pub(super) fn reset_hooks_thread_locals() {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| slot.borrow_mut().clear());
}

fn window_configuration_parts_from_value(value: &Value) -> Option<(Value, i64)> {
    // A window-configuration is its own pseudovector type, read via the
    // type-gated accessor (never `as_vector_data`, so it stays opaque to
    // `vectorp`). Slot 0 holds the tag symbol; slots 1/2 the frame and serial.
    let items = value.as_window_configuration_data()?;
    if items.len() != 4 {
        return None;
    }
    match (items[1].kind(), items[2].kind()) {
        (ValueKind::Veclike(VecLikeType::Frame), ValueKind::Fixnum(serial)) => {
            Some((items[1], serial))
        }
        _ => None,
    }
}

fn window_configuration_frame_from_value(value: &Value) -> Option<Value> {
    window_configuration_parts_from_value(value).map(|(frame, _)| frame)
}

fn next_window_configuration_serial() -> i64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_WINDOW_CONFIGURATION_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_WINDOW_CONFIGURATION_ID.fetch_add(1, Ordering::Relaxed) as i64
}

fn make_window_configuration_value(frame: Value, serial: i64, roots: Value) -> Value {
    // GNU stores saved windows inside the opaque window-configuration
    // pseudovector (`src/window.c:Fcurrent_window_configuration`), so the
    // saved Lisp values are traced as part of the object.  Neomacs keeps the
    // Rust window tree in a serial side table; this hidden slot makes the
    // object's GC ownership match GNU's saved-data ownership.
    Value::make_window_configuration(vec![
        Value::symbol(WINDOW_CONFIGURATION_TAG),
        frame,
        Value::fixnum(serial),
        roots,
    ])
}

pub(crate) fn builtin_window_configuration_p(args: Vec<Value>) -> EvalResult {
    expect_args("window-configuration-p", &args, 1)?;
    // GNU: t exactly for the window-configuration pseudovector -- a pure tag
    // check, independent of the saved contents.
    Ok(Value::bool_val(args[0].is_window_configuration()))
}

pub(crate) fn builtin_window_configuration_frame(args: Vec<Value>) -> EvalResult {
    expect_args("window-configuration-frame", &args, 1)?;
    window_configuration_frame_from_value(&args[0]).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), args[0]],
        )
    })
}

/// GNU `compare_window_configurations` (`src/window.c:9012`) compares the
/// *layout* of two configurations: selected frame, current buffer, window
/// count, and -- per window, in order -- the current-window correspondence,
/// buffer, and geometry/structure.  It deliberately ignores point and scroll.
///
/// neomacs stamps every `current-window-configuration` value with a unique
/// serial, so a raw `equal' (the previous implementation) always returned nil
/// for two distinct-but-identical configurations.  Compare the stored
/// snapshots structurally instead.
fn window_tree_layout_equal(a: &crate::window::Window, b: &crate::window::Window) -> bool {
    use crate::window::Window;
    match (a, b) {
        (
            Window::Leaf {
                buffer_id: ba,
                bounds: bounds_a,
                top_line: tla,
                left_col: lca,
                ..
            },
            Window::Leaf {
                buffer_id: bb,
                bounds: bounds_b,
                top_line: tlb,
                left_col: lcb,
                ..
            },
        ) => ba == bb && bounds_a == bounds_b && tla == tlb && lca == lcb,
        (
            Window::Internal {
                direction: da,
                children: ca,
                bounds: bounds_a,
                top_line: tla,
                left_col: lca,
                ..
            },
            Window::Internal {
                direction: db,
                children: cb,
                bounds: bounds_b,
                top_line: tlb,
                left_col: lcb,
                ..
            },
        ) => {
            da == db
                && bounds_a == bounds_b
                && tla == tlb
                && lca == lcb
                && ca.len() == cb.len()
                && ca
                    .iter()
                    .zip(cb.iter())
                    .all(|(x, y)| window_tree_layout_equal(x, y))
        }
        _ => false,
    }
}

fn window_snapshots_layout_equal(
    a: &WindowConfigurationSnapshot,
    b: &WindowConfigurationSnapshot,
) -> bool {
    a.frame_id == b.frame_id
        && a.current_buffer == b.current_buffer
        // The "current"/selected window must correspond between configurations.
        && a.selected_window == b.selected_window
        && a.minibuffer_window == b.minibuffer_window
        && window_tree_layout_equal(&a.root_window, &b.root_window)
        && match (&a.minibuffer_leaf, &b.minibuffer_leaf) {
            (Some(x), Some(y)) => window_tree_layout_equal(x, y),
            (None, None) => true,
            _ => false,
        }
}

pub(crate) fn builtin_window_configuration_equal_p(args: Vec<Value>) -> EvalResult {
    expect_args("window-configuration-equal-p", &args, 2)?;
    let Some((_, serial_a)) = window_configuration_parts_from_value(&args[0]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), args[0]],
        ));
    };
    let Some((_, serial_b)) = window_configuration_parts_from_value(&args[1]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), args[1]],
        ));
    };

    // Identical object (or identical serial) is trivially equal.
    if serial_a == serial_b {
        return Ok(Value::T);
    }

    let result = WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
        let store = slot.borrow();
        match (store.get(&serial_a), store.get(&serial_b)) {
            (Some(a), Some(b)) => Some(window_snapshots_layout_equal(a, b)),
            _ => None,
        }
    });

    match result {
        Some(equal) => Ok(Value::bool_val(equal)),
        // Fall back to a structural value comparison if a snapshot was evicted
        // from the bounded side table (very old configurations).
        None => Ok(Value::bool_val(equal_value(&args[0], &args[1], 0))),
    }
}

pub(crate) fn builtin_current_window_configuration(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("current-window-configuration", &args, 1)?;

    let frame = if let Some(frame) = args.first() {
        expect_optional_live_frame_designator_in_state(frame, &eval.frames)?;
        if frame.is_nil() {
            super::window_cmds::selected_frame_impl(&mut eval.frames, &mut eval.buffers, vec![])?
        } else {
            *frame
        }
    } else {
        super::window_cmds::selected_frame_impl(&mut eval.frames, &mut eval.buffers, vec![])?
    };

    if !frame.is_frame() {
        return Ok(make_window_configuration_value(
            frame,
            next_window_configuration_serial(),
            Value::vector(Vec::new()),
        ));
    };
    let Some(fid) = frame.as_frame_id() else {
        return Ok(make_window_configuration_value(
            frame,
            next_window_configuration_serial(),
            Value::vector(Vec::new()),
        ));
    };
    let frame_id = crate::window::FrameId(fid);
    if let Some(frame_state) = eval.frames.get(frame_id) {
        let mut snapshot = WindowConfigurationSnapshot {
            frame_id,
            root_window:
                crate::window::window_markers::clone_window_tree_with_independent_position_markers(
                    &mut eval.buffers,
                    &frame_state.root_window,
                ),
            selected_window: frame_state.selected_window,
            current_buffer: eval.buffers.current_buffer_id(),
            minibuffer_window: frame_state.minibuffer_window,
            minibuffer_leaf: frame_state.minibuffer_leaf.as_ref().map(|window| {
                crate::window::window_markers::clone_window_tree_with_independent_position_markers(
                    &mut eval.buffers,
                    window,
                )
            }),
        };
        normalize_selected_window_point_in_snapshot(&mut snapshot, &mut eval.buffers);
        save_snapshot_persistent_window_parameters(
            &mut snapshot,
            eval.obarray
                .symbol_value("window-persistent-parameters")
                .copied()
                .unwrap_or(Value::NIL),
        );
        let roots = window_configuration_snapshot_roots(&snapshot);
        let serial = next_window_configuration_serial();
        WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
            let mut store = slot.borrow_mut();
            store.insert(serial, snapshot);
            if store.len() > 4096
                && let Some(oldest) = store.keys().min().copied()
            {
                store.remove(&oldest);
            }
        });
        return Ok(make_window_configuration_value(frame, serial, roots));
    }

    Ok(make_window_configuration_value(
        frame,
        next_window_configuration_serial(),
        Value::vector(Vec::new()),
    ))
}

pub(crate) fn builtin_set_window_configuration(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-window-configuration", &args, 1, 3)?;
    let options = WindowConfigurationRestoreOptions::from_lisp_args(&args);
    set_window_configuration_with_options(eval, args[0], options)
}

/// The one window whose point a restore must NOT take from the snapshot.
///
/// `current-window-configuration` deliberately does not record point in the
/// buffer that was current when it ran ("does not save the value of point in
/// the current buffer", `Fcurrent_window_configuration` docstring,
/// `src/window.c`).  `Fset_window_configuration` honours that by computing
/// `old_point` from the LIVE session before touching anything
/// (`src/window.c:7692-7733`) and writing it back over the window that was
/// selected when the configuration was saved, once the saved tree is installed
/// (`src/window.c:7978-7984`):
///
/// ```c
///   /* Arrange *not* to restore point in the buffer that was
///      current when the window configuration was saved.  */
///   if (EQ (XWINDOW (data->current_window)->contents, new_current_buffer))
///     set_marker_restricted (XWINDOW (data->current_window)->pointm,
///                            make_fixnum (old_point),
///                            XWINDOW (data->current_window)->contents);
/// ```
///
/// Restoring that window's saved point instead rewinds an in-progress session:
/// Helm selects its own window and reads the minibuffer, so every Helm exit
/// restored the selection to wherever it stood when the prompt opened, and the
/// exit action then resolved the wrong source (DIVERGENCES.md 114).
///
/// `Some` carries the whole decision -- window, the buffer that window must
/// still show for the rule to apply, and the live point -- so no call site can
/// apply half of it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LiveCurrentBufferPoint {
    window: crate::window::WindowId,
    buffer: crate::buffer::BufferId,
    point: LispCharPos1,
}

impl LiveCurrentBufferPoint {
    /// GNU's `old_point` computation, read from the live session.
    fn read(
        eval: &super::eval::Context,
        snapshot: &WindowConfigurationSnapshot,
    ) -> Option<LiveCurrentBufferPoint> {
        // `new_current_buffer`: the buffer current at save time, unless it has
        // since been killed (GNU drops it then, and with it the whole rule).
        let saved_current_buffer = snapshot.current_buffer?;
        eval.buffers.get(saved_current_buffer)?;

        let frame = eval.frames.get(snapshot.frame_id)?;
        // A dead saved-selected window has nil contents in GNU, so it can never
        // match `new_current_buffer`; `None` here takes the same branches.
        let saved_selected = frame
            .find_window(snapshot.selected_window)
            .or_else(|| {
                frame
                    .minibuffer_leaf
                    .as_ref()
                    .filter(|window| window.id() == snapshot.selected_window)
            })
            .and_then(|window| match window {
                crate::window::Window::Leaf {
                    buffer_id, point, ..
                } => Some((*buffer_id, *point)),
                crate::window::Window::Internal { .. } => None,
            });
        let saved_selected_shows_it =
            saved_selected.is_some_and(|(buffer_id, _)| buffer_id == saved_current_buffer);

        let live_selected_window = eval
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window);
        let live_selected_is_saved_selected =
            live_selected_window == Some(snapshot.selected_window);
        let live_selected_buffer = eval.frames.selected_frame().and_then(|frame| {
            frame
                .find_window(frame.selected_window)
                .or_else(|| frame.minibuffer_leaf.as_ref())
                .and_then(|window| window.buffer_id())
        });

        let buffer_point = |buffer_id: crate::buffer::BufferId| {
            eval.buffers.get(buffer_id).map(|buffer| {
                LispCharPos1::from_one_based_usize(buffer.point_char_pos().get().saturating_add(1))
            })
        };

        let point = if eval.buffers.current_buffer_id() == Some(saved_current_buffer) {
            if saved_selected_shows_it
                && live_selected_buffer == Some(saved_current_buffer)
                && !live_selected_is_saved_selected
            {
                saved_selected.map(|(_, point)| point)?
            } else {
                buffer_point(saved_current_buffer)?
            }
        } else if saved_selected_shows_it && !live_selected_is_saved_selected {
            saved_selected.map(|(_, point)| point)?
        } else {
            buffer_point(saved_current_buffer)?
        };

        Some(LiveCurrentBufferPoint {
            window: snapshot.selected_window,
            buffer: saved_current_buffer,
            point,
        })
    }

    /// Write the live point back, but only while the restored window really
    /// does show the buffer that was current at save time -- GNU's
    /// `EQ (XWINDOW (data->current_window)->contents, new_current_buffer)`
    /// guard, evaluated against the freshly restored tree.
    fn apply(self, eval: &mut super::eval::Context, frame_id: crate::window::FrameId) {
        let buffers = &mut eval.buffers;
        let Some(frame) = eval.frames.get_mut(frame_id) else {
            return;
        };
        let window = match frame.find_window_mut(self.window) {
            Some(window) => window,
            None => match frame
                .minibuffer_leaf
                .as_mut()
                .filter(|window| window.id() == self.window)
            {
                Some(window) => window,
                None => return,
            },
        };
        if window.buffer_id() != Some(self.buffer) {
            return;
        }
        crate::window::window_markers::set_window_point_with_marker(buffers, window, self.point);
    }
}

trait OptionalLiveCurrentBufferPoint {
    fn apply(self, eval: &mut super::eval::Context, frame_id: crate::window::FrameId);
}

impl OptionalLiveCurrentBufferPoint for Option<LiveCurrentBufferPoint> {
    fn apply(self, eval: &mut super::eval::Context, frame_id: crate::window::FrameId) {
        if let Some(live) = self {
            live.apply(eval, frame_id);
        }
    }
}

pub(crate) fn set_window_configuration_with_options(
    eval: &mut super::eval::Context,
    configuration: Value,
    options: WindowConfigurationRestoreOptions,
) -> EvalResult {
    let Some((_frame, serial)) = window_configuration_parts_from_value(&configuration) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), configuration],
        ));
    };

    let snapshot = WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
        slot.borrow()
            .get(&serial)
            .map(|snapshot| snapshot.clone_for_restore(&mut eval.buffers))
    });

    if let Some(snapshot) = snapshot {
        let selected_frame_before_restore = eval.frames.selected_frame().map(|frame| frame.id);
        let live_parameters = eval
            .frames
            .get(snapshot.frame_id)
            .map(collect_frame_window_parameters)
            .unwrap_or_default();
        let mut snapshot = snapshot;
        // Read the live point BEFORE anything is restored: GNU computes
        // `old_point` at the very top of `Fset_window_configuration`, from the
        // session as it stands (`src/window.c:7692-7733`).
        let live_current_buffer_point = LiveCurrentBufferPoint::read(eval, &snapshot);
        merge_snapshot_window_parameters(&mut snapshot, &live_parameters);
        prepare_saved_window_buffer_restoration(eval, &mut snapshot);
        prepare_reused_window_histories(eval, &mut snapshot)?;
        unshow_frame_root_buffers(eval, snapshot.frame_id);
        if let Some(frame) = eval.frames.get_mut(snapshot.frame_id) {
            frame.root_window = snapshot.root_window;
            // GNU `Fset_window_configuration` does NOT touch
            // `frame->old_selected_window` directly — that field
            // is updated by `window_change_record` from the next
            // `run_window_change_functions` cycle. neomacs's
            // analog is `frame_window_hook_record_from_live_state`.
            frame.selected_window = snapshot.selected_window;
            if options.minibuffer_window == MinibufferWindowRestoration::RestoreSaved {
                frame.minibuffer_window = snapshot.minibuffer_window;
                frame.minibuffer_leaf = snapshot.minibuffer_leaf;
            }
        }
        eval.frames.mark_window_topology_changed();
        // GNU `Fset_window_configuration` (window.c) substitutes a live buffer via
        // `other_buffer_safely` for any restored window whose saved buffer was
        // killed, instead of leaving a dead buffer in the window (which would
        // signal "Selecting deleted buffer" on the next redisplay).
        let dead_leaf_windows: Vec<crate::window::WindowId> = eval
            .frames
            .get(snapshot.frame_id)
            .map(|frame| {
                frame
                    .window_list()
                    .into_iter()
                    .filter_map(|wid| match frame.find_window(wid) {
                        Some(crate::window::Window::Leaf { buffer_id, .. }) => {
                            Some((wid, *buffer_id))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, buffer_id)| eval.buffers.get(*buffer_id).is_none())
            .map(|(wid, _)| wid)
            .collect();
        if !dead_leaf_windows.is_empty() {
            let avoid = eval
                .buffers
                .current_buffer_id()
                .map(Value::make_buffer)
                .unwrap_or(Value::NIL);
            let substitute = super::super::buffer::other_buffer_impl_in_state(
                &mut eval.frames,
                &mut eval.buffers,
                vec![avoid, Value::NIL, Value::NIL],
            )
            .ok()
            .and_then(|value| value.as_buffer_id());
            if let Some(substitute) = substitute {
                let substitute_point = eval
                    .buffers
                    .get(substitute)
                    .map(|buffer| {
                        LispCharPos1::from_one_based_usize(
                            buffer.point_char_pos().get().saturating_add(1),
                        )
                    })
                    .unwrap_or(LispCharPos1::ONE);
                let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
                if let Some(frame) = frames.get_mut(snapshot.frame_id) {
                    for wid in dead_leaf_windows {
                        if let Some(window) = frame.find_window_mut(wid) {
                            if let crate::window::Window::Leaf {
                                buffer_id,
                                window_start,
                                position_markers,
                                point,
                                old_point,
                                ..
                            } = window
                            {
                                *buffer_id = substitute;
                                *window_start = LispCharPos1::ONE;
                                *point = substitute_point;
                                *old_point = substitute_point;
                                *position_markers =
                                    crate::window::WindowPositionMarkerState::Detached;
                            }
                            crate::window::window_markers::attach_window_position_markers(
                                buffers, window,
                            );
                        }
                    }
                }
            }
        }
        // GNU: "Arrange *not* to restore point in the buffer that was current
        // when the window configuration was saved" (`src/window.c:7978-7984`),
        // which it does after installing the saved tree and before selecting
        // the saved window -- so the buffer point derived from that window
        // below sees the live position too.
        live_current_buffer_point.apply(eval, snapshot.frame_id);
        let selected_window_state = eval.frames.get(snapshot.frame_id).and_then(|frame| {
            frame
                .find_window(frame.selected_window)
                .and_then(|window| match window {
                    crate::window::Window::Leaf {
                        buffer_id, point, ..
                    } => Some((*buffer_id, *point)),
                    crate::window::Window::Internal { .. } => None,
                })
        });
        if let Some((buffer_id, point)) = selected_window_state
            && let Some(buffer) = eval.buffers.get(buffer_id)
        {
            let byte_pos = buffer.lisp_pos_to_emacs_byte_pos(point);
            let _ = eval.buffers.goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
        }
        if let Some(buffer_id) = snapshot.current_buffer {
            if eval.buffers.get(buffer_id).is_some() {
                eval.set_current_buffer_unrecorded(buffer_id)?;
            } else if let Some((buffer_id, _)) = selected_window_state {
                eval.set_current_buffer_unrecorded(buffer_id)?;
            }
        } else if let Some((buffer_id, _)) = selected_window_state {
            eval.set_current_buffer_unrecorded(buffer_id)?;
        }

        // GNU `Fset_window_configuration` restores the saved tree first, then
        // calls `adjust_frame_size` before returning.  The adjustment matters
        // for the initial batch frame: its saved root may still cover all 24
        // pre-menu-bar rows, while the live frame has a one-line top margin and
        // therefore a 23-line root at row 1.
        if let Some(frame) = eval.frames.get_mut(snapshot.frame_id) {
            frame.reconcile_restored_window_configuration_geometry();
        }

        let frame_to_select = match options.selected_frame {
            SelectedFrameRestoration::RestoreSaved => Some(snapshot.frame_id),
            SelectedFrameRestoration::KeepSelected => selected_frame_before_restore,
        };
        if let Some(frame_id) = frame_to_select {
            let _ = eval.frames.select_frame(frame_id);
        }
    }

    eval.redisplay();
    // Run window-configuration-change-hook after restoring configuration.
    let _ = builtin_run_window_configuration_change_hook(eval, vec![]);
    Ok(Value::T)
}

pub(crate) fn builtin_run_window_configuration_change_hook(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("run-window-configuration-change-hook", &args, 1)?;
    // NOTE: GNU's `run_window_configuration_change_hook` (window.c) does NOT
    // gate on `window--sides-inhibit-check`. That variable is let-bound to t
    // only to suppress `window--check` (the side-window consistency validator,
    // window.el), never the hook. It routinely sits at t in real configs (e.g.
    // Doom), so gating the hook on it wrongly silenced
    // `window-configuration-change-hook' entirely -- winum never renumbered a
    // freshly-opened popup/compile window until an unrelated command forced it
    // (#191). The redisplay driver (run_window_change_functions) is the sole
    // caller and only fires post-op, so there is no re-entrancy to guard.
    if let Some(frame) = args.first() {
        expect_optional_live_frame_designator(frame, eval)?;
    }
    let frame = match args.first().copied().unwrap_or(Value::NIL).kind() {
        ValueKind::Nil => {
            super::window_cmds::selected_frame_impl(&mut eval.frames, &mut eval.buffers, vec![])?
        }
        _value => args.first().copied().unwrap_or(Value::NIL),
    };
    let Some(fid) = frame.as_frame_id() else {
        return Ok(Value::NIL);
    };
    let frame_id = crate::window::FrameId(fid);
    let Some(frame_state) = eval.frames.get(frame_id) else {
        return Ok(Value::NIL);
    };

    let hook_sym = hook_runtime::hook_symbol_by_name(eval, "window-configuration-change-hook");
    let global_hook_value = eval
        .obarray
        .default_value_id(hook_sym)
        .copied()
        .unwrap_or(Value::NIL);
    let selected_window = frame_state.selected_window;
    let window_ids = frame_state.window_list();
    let hook_name = crate::emacs_core::intern::resolve_sym(hook_sym);
    let saved = save_hook_caller_context(eval);

    // The snapshot of the GLOBAL hook list is held in a Rust local across
    // the per-window buffer-local hook runs (arbitrary Lisp); a local hook
    // that setq-defaults or globally remove-hooks the variable unlinks the
    // snapshot from its obarray root, and a GC frees the conses before the
    // final global run walks them. One root for the snapshot preserves the
    // GNU behavior of running the pre-local-hooks snapshot.
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(global_hook_value);

    let result = (|| -> EvalResult {
        select_frame_window_for_hook_context(eval, frame_id, selected_window);
        for window_id in &window_ids {
            let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, *window_id) else {
                continue;
            };
            let has_local_hook = eval
                .buffers
                .get(buffer_id)
                .and_then(|buffer| buffer.get_buffer_local_binding(hook_name))
                .is_some();
            if !has_local_hook {
                continue;
            }
            select_frame_window_for_hook_context(eval, frame_id, *window_id);
            let Some(local_hook_value) = eval
                .buffers
                .current_buffer()
                .and_then(|buffer| buffer.buffer_local_value(hook_name))
            else {
                continue;
            };
            let _ = hook_runtime::run_hook_value(eval, hook_sym, local_hook_value, &[], false)?;
            select_frame_window_for_hook_context(eval, frame_id, selected_window);
        }
        let _ = hook_runtime::run_hook_value(eval, hook_sym, global_hook_value, &[], false)?;
        Ok(Value::NIL)
    })();

    eval.restore_specpdl_roots(root_scope);
    restore_hook_caller_context(eval, saved);
    result
}

pub(crate) fn builtin_run_window_scroll_functions(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("run-window-scroll-functions", &args, 1)?;
    if let Some(window) = args.first() {
        expect_optional_live_window_designator(window, eval)?;
    }

    let window_arg = match args.first().copied().unwrap_or(Value::NIL).kind() {
        ValueKind::Nil => super::window_cmds::builtin_selected_window(eval, vec![])?,
        _value => args.first().copied().unwrap_or(Value::NIL),
    };
    let Some(wid) = window_arg.as_window_id() else {
        return Ok(Value::NIL);
    };
    let window_id = crate::window::WindowId(wid);
    let frame_id = eval.frames.find_window_frame_id(window_id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), window_arg],
        )
    })?;
    let window_start = super::window_cmds::builtin_window_start(eval, vec![window_arg])?;
    let hook_sym = hook_runtime::hook_symbol_by_name(eval, "window-scroll-functions");
    let saved_buffer_id = eval.buffers.current_buffer_id();
    if let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, window_id) {
        // GNU `set_window_buffer` temporarily enters the displayed buffer
        // with `Fset_buffer` before running `window-scroll-functions`
        // (window.c), which does not call `record_buffer` or change
        // `buffer-list` recency.
        let _ = eval.set_current_buffer_unrecorded(buffer_id);
    }
    let hook_value = hook_runtime::hook_value_by_id(eval, hook_sym).unwrap_or(Value::NIL);
    let result = hook_runtime::run_hook_value(
        eval,
        hook_sym,
        hook_value,
        &[window_arg, window_start],
        true,
    );
    if let Some(buffer_id) = saved_buffer_id {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    result
}

pub(crate) fn builtin_featurep(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("featurep", &args, 1)?;
    expect_max_args("featurep", &args, 2)?;
    let feature = eval.unwrap_symbol(args[0]);
    let sym_id = feature.as_symbol_id().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;
    crate::emacs_core::eval::refresh_features_from_variable_in_state(
        &eval.obarray,
        &mut eval.features,
    );
    if !eval.features.contains(&sym_id) {
        return Ok(Value::NIL);
    }

    let Some(subfeature) = args.get(1) else {
        return Ok(Value::T);
    };
    if subfeature.is_nil() {
        return Ok(Value::T);
    }

    let subfeatures = eval
        .obarray
        .get_property_id(sym_id, crate::emacs_core::intern::intern("subfeatures"))
        .unwrap_or(Value::NIL);
    let items = list_to_vec(&subfeatures).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), subfeatures],
        )
    })?;
    Ok(Value::bool_val(items.iter().any(|item| item == subfeature)))
}
