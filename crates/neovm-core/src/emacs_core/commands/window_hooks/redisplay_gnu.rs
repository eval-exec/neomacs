//! GNU window.c run_window_change_functions: live preorder local calls,
//! defaults after all locals, buffer-only context, record on unwind.
//! Exclusively borrowed Context owns records; no shared Lisp cache is added.

use super::*;
use crate::buffer::BufferId;
use crate::emacs_core::eval::redisplay_hooks::{
    BufferContext, HookWindowDimensions, default_hook_value, safe_named_hook,
    window_buffer_id_in_state,
};
use crate::emacs_core::intern::WellKnown;
use crate::window::DisplayFrameExt;
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_value::display_vocab::FrameId;

// Fixed GNU hook spellings run on every redisplay; see `hooks.rs`.
static WINDOW_STATE_CHANGE_HOOK: WellKnown = WellKnown::WindowStateChangeHook;
static INHIBIT_REDISPLAY: WellKnown = WellKnown::InhibitRedisplay;

/// Owns one exclusive Context borrow through a window-change pass and its unwind.
/// Saved binding and record state remain local to that Context; independent mutators
/// own separate guards and root stacks.
struct ChangePass<'a> {
    eval: &'a mut crate::emacs_core::eval::Context,
    record: bool,
    binding_count: usize,
    armed: bool,
}

impl Drop for ChangePass<'_> {
    fn drop(&mut self) {
        // Pure geometry/ID recording does not invoke Lisp, so it is safe also
        // during Rust unwinding. Lisp nonlocal exits reach this same boundary.
        if self.armed {
            finish_record(self.eval, self.record);
            // Panic recovery only. Normal Lisp exits use unbind_to_with_result.
            self.eval.unbind_to(self.binding_count);
        }
    }
}

fn window_dimensions(
    eval: &crate::emacs_core::eval::Context,
    frame: FrameId,
    window: WindowId,
) -> Option<HookWindowDimensions> {
    let bounds = eval
        .frame_manager()
        .get(frame)?
        .find_window(window)?
        .bounds();
    let (body_width, body_height) = super::super::window_cmds::hook_window_body_dimensions(
        &eval.frame_manager(),
        &eval.edit.buffers,
        frame,
        window,
    )
    .ok()?;
    Some(HookWindowDimensions {
        total_width: bounds.width,
        total_height: bounds.height,
        body_width,
        body_height,
    })
}

fn record_frames(eval: &mut crate::emacs_core::eval::Context) {
    let selected = eval.frame_manager().selected_frame().map(|frame| frame.id);
    let mut dimensions = std::collections::HashMap::new();
    for frame_id in eval.gnu_frame_order() {
        for window_id in eval.gnu_window_order(frame_id) {
            if let Some(record) = window_dimensions(eval, frame_id, window_id) {
                dimensions.insert(window_id, record);
            }
        }
        if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
            frame.old_selected_window = Some(frame.selected_window);
            frame.window_hook_record =
                super::frame_window_hook_record_from_live_state(frame, selected == Some(frame_id));
            let stamp = frame.change_stamp.next();
            frame.change_stamp = stamp;
            frame.record_window_change_epoch(stamp);
            frame.window_state_change = false;
        }
    }
    eval.gnu_redisplay_hooks_mut().dimensions = dimensions;
    eval.gnu_redisplay_hooks_mut().frame_window_change.clear();
}

fn finish_record(eval: &mut crate::emacs_core::eval::Context, record: bool) {
    if record {
        record_frames(eval);
    }
    eval.record_redisplay_selection();
}

fn live(eval: &crate::emacs_core::eval::Context, frame: FrameId, window: WindowId) -> bool {
    eval.frame_manager()
        .get(frame)
        .and_then(|frame| frame.find_window(window))
        .and_then(|window| window.buffer_id())
        .is_some()
}

/// Walk the actual cons chain. Root each currently visited cons across Lisp;
/// reading its cdr after safe_funcall observes setcdr made by the callback.
/// GNU change functions ignore t and do not remove a signaled callback.
fn walk(
    eval: &mut crate::emacs_core::eval::Context,
    record: &mut bool,
    mut functions: Value,
    argument: Value,
) -> EvalResult {
    // `xdisp.c:17562` skips the change-function walk when `Vrun_hooks` is
    // nil. The loop still reads the live cdr after each callback.
    crate::emacs_core::hook_runtime::with_live_hook_walk(
        eval,
        || Ok(Value::NIL),
        |eval| {
            let roots = eval.save_specpdl_roots();
            let tail = eval.push_specpdl_root_slot(functions);
            eval.push_specpdl_root(argument);
            let result = (|| {
                while functions.is_cons() {
                    eval.set_specpdl_root_slot(&tail, functions);
                    let function = functions.cons_car();
                    if function != Value::T {
                        *record = true;
                        eval.safe_funcall(function, vec![argument])?;
                    }
                    functions = functions.cons_cdr();
                }
                Ok(Value::NIL)
            })();
            eval.restore_specpdl_roots(roots);
            result
        },
    )
}

fn local(
    eval: &mut crate::emacs_core::eval::Context,
    record: &mut bool,
    buffer: BufferId,
    window: WindowId,
    name: &str,
) -> EvalResult {
    let has_local = eval
        .edit
        .buffers
        .get(buffer)
        .and_then(|buffer| buffer.get_buffer_local_binding(name))
        .is_some();
    if !has_local {
        return Ok(Value::NIL);
    }
    // Captured buffer, buffer-only context, and no buffer-list recency change.
    eval.set_current_buffer_unrecorded(buffer)?;
    let functions = eval
        .edit
        .buffers
        .get(buffer)
        .and_then(|buffer| buffer.buffer_local_value(name))
        .ok_or_else(|| signal("void-variable", vec![Value::symbol(name)]))?;
    walk(eval, record, functions, Value::make_window(window.0))
}

fn global(
    eval: &mut crate::emacs_core::eval::Context,
    record: &mut bool,
    frame: FrameId,
    name: &str,
) -> EvalResult {
    let symbol = crate::emacs_core::intern::intern(name);
    let functions = default_hook_value(eval, symbol)?;
    walk(eval, record, functions, Value::make_frame(frame.0))
}

fn selection_changed(
    eval: &crate::emacs_core::eval::Context,
    frame_id: FrameId,
    window: WindowId,
    frame_selected_change: bool,
    frame_selected_window_change: bool,
) -> bool {
    let Some(frame) = eval.frame_manager().get(frame_id) else {
        return false;
    };
    (frame_selected_change
        && (eval.gnu_redisplay_hooks().old_selected_window == Some(window)
            || eval.gnu_selected_window() == Some(window)))
        || (frame_selected_window_change
            && (frame.old_selected_window == Some(window) || frame.selected_window == window))
}

pub(super) fn run(eval: &mut crate::emacs_core::eval::Context) -> EvalResult {
    let count = eval.specpdl().len();
    let mut pass = ChangePass {
        eval,
        record: false,
        binding_count: count,
        armed: true,
    };
    let result = (|| {
        pass.eval
            .try_specbind_or_unwind_to(count, INHIBIT_REDISPLAY.id(), Value::T)?;
        run_inner(&mut pass)
    })();
    finish_record(pass.eval, pass.record);
    pass.armed = false;
    pass.eval.unbind_to_with_result(count, result)
}

fn run_inner(pass: &mut ChangePass<'_>) -> EvalResult {
    let selected_frame_change = pass
        .eval
        .frame_manager()
        .selected_frame()
        .map(|frame| frame.id)
        != pass.eval.gnu_redisplay_hooks().old_selected_frame;
    let mut run_state_hook = false;

    for frame_id in pass.eval.gnu_frame_order() {
        let Some(frame) = pass.eval.frame_manager().get(frame_id) else {
            continue;
        };
        // All published Neo frames are complete, size-capable frames. Tooltip
        // frames retain GNU's explicit exclusion. Frame creation/size mutation
        // publishers are responsible for FRAME_WINDOW_CHANGE.
        if frame
            .fixed_parameter(neovm_value::display_vocab::FrameQueryParam::Tooltip)
            .is_some_and(|value| value.is_truthy())
        {
            continue;
        }
        let frame_window_change = pass
            .eval
            .gnu_redisplay_hooks()
            .frame_window_change
            .contains(&frame_id)
            || pass.eval.gnu_window_order(frame_id).iter().any(|window| {
                !pass
                    .eval
                    .gnu_redisplay_hooks()
                    .dimensions
                    .contains_key(window)
            });
        let frame_selected_change = selected_frame_change
            && (pass.eval.gnu_redisplay_hooks().old_selected_frame == Some(frame_id)
                || pass
                    .eval
                    .frame_manager()
                    .selected_frame()
                    .is_some_and(|frame| frame.id == frame_id));
        let frame_selected_window_change = frame.old_selected_window != Some(frame.selected_window);
        let forced_state = frame.window_state_change;
        if !(frame_window_change
            || frame_selected_change
            || frame_selected_window_change
            || forced_state)
        {
            continue;
        }
        let old_window_count = frame.window_hook_record.windows.len();
        let windows = pass.eval.gnu_window_order(frame_id);
        let number_of_windows = windows.len();
        let mut frame_buffer_change = false;
        let mut frame_size_change = false;

        for window_id in windows {
            let Some(frame) = pass.eval.frame_manager().get(frame_id) else {
                continue;
            };
            let Some(window) = frame.find_window(window_id) else {
                continue;
            };
            let Some(buffer_id) = window.buffer_id() else {
                continue;
            };
            let old_buffer = window.old_buffer();
            let buffer_changed = frame_window_change
                && (old_buffer != Some(buffer_id)
                    || window.change_stamp() != Some(frame.change_stamp));
            let size_changed = frame_window_change
                && (buffer_changed
                    || window_dimensions(pass.eval, frame_id, window_id)
                        != pass
                            .eval
                            .gnu_redisplay_hooks()
                            .dimensions
                            .get(&window_id)
                            .copied());
            frame_buffer_change |= buffer_changed;
            frame_size_change |= size_changed;
            let saved = pass.eval.edit.buffers.current_buffer_id();
            // A separate &mut scope restores current-buffer on throw/panic,
            // while deliberately preserving changed window/frame selection.
            let mut scope = BufferContext {
                eval: &mut *pass.eval,
                saved,
            };
            let result = (|| -> EvalResult {
                if buffer_changed {
                    if let Some(old_buffer) =
                        old_buffer.filter(|old| scope.eval.edit.buffers.get(*old).is_some())
                    {
                        local(
                            scope.eval,
                            &mut pass.record,
                            old_buffer,
                            window_id,
                            "window-buffer-change-functions",
                        )?;
                    }
                    local(
                        scope.eval,
                        &mut pass.record,
                        buffer_id,
                        window_id,
                        "window-buffer-change-functions",
                    )?;
                }
                if size_changed && live(scope.eval, frame_id, window_id) {
                    local(
                        scope.eval,
                        &mut pass.record,
                        buffer_id,
                        window_id,
                        "window-size-change-functions",
                    )?;
                }
                if selection_changed(
                    scope.eval,
                    frame_id,
                    window_id,
                    frame_selected_change,
                    frame_selected_window_change,
                ) && live(scope.eval, frame_id, window_id)
                {
                    local(
                        scope.eval,
                        &mut pass.record,
                        buffer_id,
                        window_id,
                        "window-selection-change-functions",
                    )?;
                }
                if (buffer_changed
                    || size_changed
                    || selection_changed(
                        scope.eval,
                        frame_id,
                        window_id,
                        frame_selected_change,
                        frame_selected_window_change,
                    ))
                    && live(scope.eval, frame_id, window_id)
                {
                    local(
                        scope.eval,
                        &mut pass.record,
                        buffer_id,
                        window_id,
                        "window-state-change-functions",
                    )?;
                }
                Ok(Value::NIL)
            })();
            drop(scope);
            result?;
        }
        let deleted = number_of_windows < old_window_count;
        if (frame_buffer_change || deleted) && pass.eval.frame_manager().get(frame_id).is_some() {
            global(
                pass.eval,
                &mut pass.record,
                frame_id,
                "window-buffer-change-functions",
            )?;
        }
        if frame_size_change && pass.eval.frame_manager().get(frame_id).is_some() {
            global(
                pass.eval,
                &mut pass.record,
                frame_id,
                "window-size-change-functions",
            )?;
        }
        if (frame_selected_change || frame_selected_window_change)
            && pass.eval.frame_manager().get(frame_id).is_some()
        {
            global(
                pass.eval,
                &mut pass.record,
                frame_id,
                "window-selection-change-functions",
            )?;
        }
        if (frame_selected_change
            || frame_selected_window_change
            || frame_buffer_change
            || deleted
            || frame_size_change
            || forced_state)
            && pass.eval.frame_manager().get(frame_id).is_some()
        {
            global(
                pass.eval,
                &mut pass.record,
                frame_id,
                "window-state-change-functions",
            )?;
            run_state_hook = true;
            pass.record = true;
        }
        if (frame_size_change || deleted) && pass.eval.frame_manager().get(frame_id).is_some() {
            run_configuration(pass.eval, frame_id)?;
        }
    }
    if run_state_hook {
        let symbol = hook_runtime::hook_symbol_by_id(pass.eval, WINDOW_STATE_CHANGE_HOOK.id());
        safe_named_hook(pass.eval, symbol, &[])?;
    }
    Ok(Value::NIL)
}

/// Exclusively borrows the owning Context while restoring configuration-hook state.
/// Saved IDs and the root-stack boundary belong to that Context; independent
/// mutators never share this guard or its mutable restore state.
struct ConfigurationContext<'a> {
    eval: &'a mut crate::emacs_core::eval::Context,
    frame: Option<FrameId>,
    buffer: Option<BufferId>,
    roots: Option<crate::emacs_core::eval::SpecpdlRootScopeState>,
    armed: bool,
}
impl ConfigurationContext<'_> {
    fn restore(&mut self) -> EvalResult {
        let result = if let Some(frame) = self
            .frame
            .take()
            .filter(|frame| self.eval.frame_manager().get(*frame).is_some())
        {
            crate::emacs_core::frame::builtin_select_frame(
                self.eval,
                vec![Value::make_frame(frame.0), Value::T],
            )
        } else {
            Ok(Value::NIL)
        };
        if let Some(buffer) = self.buffer.take() {
            self.eval.restore_current_buffer_if_live(buffer);
        }
        if let Some(roots) = self.roots.take() {
            self.eval.restore_specpdl_roots(roots);
        }
        result
    }
}
impl Drop for ConfigurationContext<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.restore();
        }
    }
}

/// Restores a selected-window ID through one exclusive Context borrow. Each
/// mutator owns its guard and saved ID without shared mutable selection state.
struct SelectedWindowContext<'a> {
    eval: &'a mut crate::emacs_core::eval::Context,
    window: Option<WindowId>,
}
impl SelectedWindowContext<'_> {
    fn restore(&mut self) -> EvalResult {
        if let Some(window) = self.window.take().filter(|window| {
            self.eval
                .frame_manager()
                .find_window_frame_id(*window)
                .is_some()
        }) {
            crate::emacs_core::window_cmds::builtin_select_window(
                self.eval,
                vec![Value::make_window(window.0), Value::T],
            )
        } else {
            Ok(Value::NIL)
        }
    }
}
impl Drop for SelectedWindowContext<'_> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn run_funs(eval: &mut crate::emacs_core::eval::Context, mut functions: Value) -> EvalResult {
    // `window.c:3797` returns before the configuration-hook walk when
    // `Vrun_hooks` is nil. The loop still reads the live cdr after each
    // callback.
    crate::emacs_core::hook_runtime::with_live_hook_walk(
        eval,
        || Ok(Value::NIL),
        |eval| {
            let roots = eval.save_specpdl_roots();
            let tail = eval.push_specpdl_root_slot(functions);
            let result = (|| {
                while functions.is_cons() {
                    eval.set_specpdl_root_slot(&tail, functions);
                    let function = functions.cons_car();
                    if function != Value::T {
                        eval.funcall_general(function, vec![])?;
                    }
                    functions = functions.cons_cdr();
                }
                Ok(Value::NIL)
            })();
            eval.restore_specpdl_roots(roots);
            result
        },
    )
}

/// Configuration retains its separate selection/unwind contract (window.c3790):
/// save current buffer only if it differs at entry; restore selected frame only
/// when selecting the requested frame at entry; restore each local window;
/// leave deliberate global callback selections intact when no outer restore
/// was registered. Snapshot and root the default list before local callbacks.
pub(super) fn run_configuration(
    eval: &mut crate::emacs_core::eval::Context,
    frame: FrameId,
) -> EvalResult {
    let symbol = crate::emacs_core::intern::intern("window-configuration-change-hook");
    let functions = default_hook_value(eval, symbol)?;
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(functions);
    let selected_frame = eval.frame_manager().selected_frame().map(|frame| frame.id);
    let selected_buffer = eval
        .frame_manager()
        .selected_frame()
        .and_then(|frame| frame.selected_window())
        .and_then(|window| window.buffer_id());
    let saved_buffer = (eval.edit.buffers.current_buffer_id() != selected_buffer)
        .then(|| eval.edit.buffers.current_buffer_id())
        .flatten();
    let saved_frame = (selected_frame != Some(frame))
        .then_some(selected_frame)
        .flatten();
    let mut scope = ConfigurationContext {
        eval,
        frame: saved_frame,
        buffer: saved_buffer,
        roots: Some(roots),
        armed: true,
    };
    let result = (|| -> EvalResult {
        if let Some(buffer) = selected_buffer {
            scope.eval.set_current_buffer_unrecorded(buffer)?;
        }
        if saved_frame.is_some() {
            crate::emacs_core::frame::builtin_select_frame(
                scope.eval,
                vec![Value::make_frame(frame.0), Value::T],
            )?;
        }
        // Fwindow_list starts at the frame's selected window, then cycles the
        // ordinary canonical leaves; minibuffer is explicitly excluded.
        let Some(state) = scope.eval.frame_manager().get(frame) else {
            return Ok(Value::NIL);
        };
        let mut windows = state.window_list();
        if let Some(index) = windows
            .iter()
            .position(|window| *window == state.selected_window)
        {
            windows.rotate_left(index);
        }
        for window in windows {
            let Some(buffer) = window_buffer_id_in_state(scope.eval, frame, window) else {
                continue;
            };
            if !scope
                .eval
                .edit
                .buffers
                .get(buffer)
                .and_then(|buffer| {
                    buffer.get_buffer_local_binding("window-configuration-change-hook")
                })
                .is_some()
            {
                continue;
            }
            let old_window = scope.eval.gnu_selected_window();
            let mut window_scope = SelectedWindowContext {
                eval: &mut *scope.eval,
                window: old_window,
            };
            let result = (|| {
                crate::emacs_core::window_cmds::builtin_select_window(
                    window_scope.eval,
                    vec![Value::make_window(window.0), Value::T],
                )?;
                let local = window_scope
                    .eval
                    .edit
                    .buffers
                    .get(buffer)
                    .and_then(|buffer| {
                        buffer.buffer_local_value("window-configuration-change-hook")
                    })
                    .ok_or_else(|| signal("void-variable", vec![Value::from_sym_id(symbol)]))?;
                run_funs(window_scope.eval, local)
            })();
            let restored = window_scope.restore();
            restored?;
            result?;
        }
        run_funs(scope.eval, functions)
    })();
    let restored = scope.restore();
    scope.armed = false;
    restored?;
    result
}

#[cfg(test)]
#[path = "redisplay_gnu/tests/default_read_test.rs"]
mod default_read_tests;

#[cfg(test)]
#[path = "redisplay_gnu/tests/live_cdr_test.rs"]
mod live_cdr_tests;
