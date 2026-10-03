//! GNU window.c run_window_change_functions: live preorder local calls,
//! defaults after all locals, buffer-only context, record on unwind.
//! Exclusively borrowed Context owns records; no shared Lisp cache is added.

use super::*;
use crate::buffer::BufferId;
use crate::emacs_core::eval::redisplay_hooks::HookWindowDimensions;
use crate::window::{FrameId, WindowId};

/// Owns one exclusive Context borrow through a window-change pass and its unwind.
/// Saved binding and record state remain local to that Context; independent mutators
/// own separate guards and root stacks.
struct ChangePass<'a> {
    eval: &'a mut super::eval::Context,
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

/// Restores a buffer ID through an exclusive Context borrow. Each mutator owns
/// its saved ID and guard; no Lisp state or mutable cache is shared between them.
struct BufferContext<'a> {
    eval: &'a mut super::eval::Context,
    saved: Option<BufferId>,
}
impl Drop for BufferContext<'_> {
    fn drop(&mut self) {
        if let Some(saved) = self.saved {
            self.eval.restore_current_buffer_if_live(saved);
        }
    }
}

fn window_dimensions(
    eval: &super::eval::Context,
    frame: FrameId,
    window: WindowId,
) -> Option<HookWindowDimensions> {
    let bounds = eval.frames.get(frame)?.find_window(window)?.bounds();
    let (body_width, body_height) = super::super::window_cmds::hook_window_body_dimensions(
        &eval.frames,
        &eval.buffers,
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

fn record_frames(eval: &mut super::eval::Context) {
    let selected = eval.frames.selected_frame().map(|frame| frame.id);
    let mut dimensions = std::collections::HashMap::new();
    for frame_id in eval.gnu_frame_order() {
        for window_id in eval.gnu_window_order(frame_id) {
            if let Some(record) = window_dimensions(eval, frame_id, window_id) {
                dimensions.insert(window_id, record);
            }
        }
        if let Some(frame) = eval.frames.get_mut(frame_id) {
            frame.old_selected_window = Some(frame.selected_window);
            frame.window_hook_record =
                super::frame_window_hook_record_from_live_state(frame, selected == Some(frame_id));
            let stamp = frame.change_stamp.next();
            frame.change_stamp = stamp;
            frame.record_window_change_epoch(stamp);
            frame.window_state_change = false;
        }
    }
    eval.gnu_redisplay_hooks.dimensions = dimensions;
    eval.gnu_redisplay_hooks.frame_window_change.clear();
}

fn finish_record(eval: &mut super::eval::Context, record: bool) {
    if record {
        record_frames(eval);
    }
    eval.gnu_redisplay_hooks.old_selected_frame =
        eval.frames.selected_frame().map(|frame| frame.id);
    eval.gnu_redisplay_hooks.old_selected_window = eval.gnu_selected_window();
}

fn live(eval: &super::eval::Context, frame: FrameId, window: WindowId) -> bool {
    eval.frames
        .get(frame)
        .and_then(|frame| frame.find_window(window))
        .and_then(|window| window.buffer_id())
        .is_some()
}

/// Walk the actual cons chain. Root each currently visited cons across Lisp;
/// reading its cdr after safe_funcall observes setcdr made by the callback.
/// GNU change functions ignore t and do not remove a signaled callback.
fn walk(
    eval: &mut super::eval::Context,
    record: &mut bool,
    mut functions: Value,
    argument: Value,
) -> EvalResult {
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
}

fn local(
    eval: &mut super::eval::Context,
    record: &mut bool,
    buffer: BufferId,
    window: WindowId,
    name: &str,
) -> EvalResult {
    let has_local = eval
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
        .buffers
        .get(buffer)
        .and_then(|buffer| buffer.buffer_local_value(name))
        .ok_or_else(|| signal("void-variable", vec![Value::symbol(name)]))?;
    walk(eval, record, functions, Value::make_window(window.0))
}

/// GNU Fdefault_value reads outside safe per-function callbacks. Preserve its
/// void-variable/alias errors and live forwarded default storage.
fn default_hook_value(
    eval: &mut super::eval::Context,
    symbol: crate::emacs_core::intern::SymId,
) -> EvalResult {
    crate::emacs_core::data::default_value(eval, vec![Value::from_sym_id(symbol)])
}

fn global(
    eval: &mut super::eval::Context,
    record: &mut bool,
    frame: FrameId,
    name: &str,
) -> EvalResult {
    let symbol = crate::emacs_core::intern::intern(name);
    let functions = default_hook_value(eval, symbol)?;
    walk(eval, record, functions, Value::make_frame(frame.0))
}

fn selection_changed(
    eval: &super::eval::Context,
    frame_id: FrameId,
    window: WindowId,
    frame_selected_change: bool,
    frame_selected_window_change: bool,
) -> bool {
    let Some(frame) = eval.frames.get(frame_id) else {
        return false;
    };
    (frame_selected_change
        && (eval.gnu_redisplay_hooks.old_selected_window == Some(window)
            || eval.gnu_selected_window() == Some(window)))
        || (frame_selected_window_change
            && (frame.old_selected_window == Some(window) || frame.selected_window == window))
}

/// Immutable numeric call policy copied into one exclusive Context hook walk.
/// Independent mutators may use their own copies; it retains no Lisp state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NamedHookCall {
    Ordinary,
    Safe,
}

/// GNU run_hook_with_args traverses the live cons chain and samples defaults
/// at each local t marker. This adds no Lisp cache: the current tail is rooted
/// in the exclusively borrowed evaluator's explicit specpdl root slot.
fn named_hook_value(
    eval: &mut super::eval::Context,
    symbol: crate::emacs_core::intern::SymId,
    mut value: Value,
    args: &[Value],
    inherit: bool,
    mode: NamedHookCall,
) -> EvalResult {
    let roots = eval.save_specpdl_roots();
    let tail = eval.push_specpdl_root_slot(value);
    for arg in args {
        eval.push_specpdl_root(*arg);
    }
    let result = (|| {
        if value.is_nil() {
            return Ok(Value::NIL);
        }
        let single = !value.is_cons()
            || if inherit {
                crate::emacs_core::builtins::value_is_function(eval, value)
            } else {
                value.cons_car() == Value::symbol("lambda")
            };
        if single {
            return named_hook_call(eval, symbol, value, args, mode);
        }
        while value.is_cons() {
            eval.set_specpdl_root_slot(&tail, value);
            let function = value.cons_car();
            if function == Value::T {
                if inherit {
                    let defaults = default_hook_value(eval, symbol)?;
                    named_hook_value(eval, symbol, defaults, args, false, mode)?;
                }
            } else {
                named_hook_call(eval, symbol, function, args, mode)?;
            }
            value = value.cons_cdr();
        }
        Ok(Value::NIL)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

fn named_hook_call(
    eval: &mut super::eval::Context,
    symbol: crate::emacs_core::intern::SymId,
    function: Value,
    args: &[Value],
    mode: NamedHookCall,
) -> EvalResult {
    use crate::emacs_core::hook_runtime::HookRuntime;
    match eval.call_hook_callable(function, args) {
        Err(flow) => {
            if mode == NamedHookCall::Safe
                && let Some(signal) = flow.as_signal()
            {
                // The owned carrier keeps the signal's in-flight roots live
                // while error reporting can call arbitrary Lisp and collect.
                eval.report_safe_hook_error(symbol, function, signal)?;
                eval.remove_hook_function_after_error(symbol, function);
                Ok(Value::NIL)
            } else {
                Err(flow)
            }
        }
        Ok(_) => Ok(Value::NIL),
    }
}

fn safe_named_hook(
    eval: &mut super::eval::Context,
    symbol: crate::emacs_core::intern::SymId,
    args: &[Value],
) -> EvalResult {
    let count = eval.specpdl.len();
    let result = (|| {
        eval.try_specbind_or_unwind_to(
            count,
            crate::emacs_core::intern::intern("inhibit-quit"),
            Value::T,
        )?;
        let value = hook_runtime::hook_value_by_id(eval, symbol).unwrap_or(Value::NIL);
        named_hook_value(eval, symbol, value, args, true, NamedHookCall::Safe)
    })();
    eval.unbind_to_with_result(count, result)
}

/// Explicit/eager run-window-scroll-functions has ordinary hook error and
/// reentry semantics. Only the redisplay committed-start seam is safe-called
/// under the private redisplay guard.
pub(super) fn run_eager_scroll(eval: &mut super::eval::Context, window: WindowId) -> EvalResult {
    let frame = eval.frames.find_window_frame_id(window).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), Value::make_window(window.0)],
        )
    })?;
    let argument = Value::make_window(window.0);
    let start = super::super::window_cmds::builtin_window_start(eval, vec![argument])?;
    let saved = eval.buffers.current_buffer_id();
    let mut scope = BufferContext { eval, saved };
    if let Some(buffer) = super::window_buffer_id_in_state(scope.eval, frame, window) {
        scope.eval.set_current_buffer_unrecorded(buffer)?;
    }
    let symbol = hook_runtime::hook_symbol_by_name(scope.eval, "window-scroll-functions");
    let value = hook_runtime::hook_value_by_id(scope.eval, symbol).unwrap_or(Value::NIL);
    named_hook_value(
        scope.eval,
        symbol,
        value,
        &[argument, start],
        true,
        NamedHookCall::Ordinary,
    )
}

pub(super) fn run(eval: &mut super::eval::Context) -> EvalResult {
    let count = eval.specpdl.len();
    let mut pass = ChangePass {
        eval,
        record: false,
        binding_count: count,
        armed: true,
    };
    let result = (|| {
        pass.eval.try_specbind_or_unwind_to(
            count,
            crate::emacs_core::intern::intern("inhibit-redisplay"),
            Value::T,
        )?;
        run_inner(&mut pass)
    })();
    finish_record(pass.eval, pass.record);
    pass.armed = false;
    pass.eval.unbind_to_with_result(count, result)
}

fn run_inner(pass: &mut ChangePass<'_>) -> EvalResult {
    let selected_frame_change = pass.eval.frames.selected_frame().map(|frame| frame.id)
        != pass.eval.gnu_redisplay_hooks.old_selected_frame;
    let mut run_state_hook = false;

    for frame_id in pass.eval.gnu_frame_order() {
        let Some(frame) = pass.eval.frames.get(frame_id) else {
            continue;
        };
        // All published Neo frames are complete, size-capable frames. Tooltip
        // frames retain GNU's explicit exclusion. Frame creation/size mutation
        // publishers are responsible for FRAME_WINDOW_CHANGE.
        if frame
            .parameter("tooltip")
            .is_some_and(|value| value.is_truthy())
        {
            continue;
        }
        let frame_window_change = pass
            .eval
            .gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame_id)
            || pass.eval.gnu_window_order(frame_id).iter().any(|window| {
                !pass
                    .eval
                    .gnu_redisplay_hooks
                    .dimensions
                    .contains_key(window)
            });
        let frame_selected_change = selected_frame_change
            && (pass.eval.gnu_redisplay_hooks.old_selected_frame == Some(frame_id)
                || pass
                    .eval
                    .frames
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
            let Some(frame) = pass.eval.frames.get(frame_id) else {
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
                            .gnu_redisplay_hooks
                            .dimensions
                            .get(&window_id)
                            .copied());
            frame_buffer_change |= buffer_changed;
            frame_size_change |= size_changed;
            let saved = pass.eval.buffers.current_buffer_id();
            // A separate &mut scope restores current-buffer on throw/panic,
            // while deliberately preserving changed window/frame selection.
            let mut scope = BufferContext {
                eval: &mut *pass.eval,
                saved,
            };
            let result = (|| -> EvalResult {
                if buffer_changed {
                    if let Some(old_buffer) =
                        old_buffer.filter(|old| scope.eval.buffers.get(*old).is_some())
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
        if (frame_buffer_change || deleted) && pass.eval.frames.get(frame_id).is_some() {
            global(
                pass.eval,
                &mut pass.record,
                frame_id,
                "window-buffer-change-functions",
            )?;
        }
        if frame_size_change && pass.eval.frames.get(frame_id).is_some() {
            global(
                pass.eval,
                &mut pass.record,
                frame_id,
                "window-size-change-functions",
            )?;
        }
        if (frame_selected_change || frame_selected_window_change)
            && pass.eval.frames.get(frame_id).is_some()
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
            && pass.eval.frames.get(frame_id).is_some()
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
        if (frame_size_change || deleted) && pass.eval.frames.get(frame_id).is_some() {
            run_configuration(pass.eval, frame_id)?;
        }
    }
    if run_state_hook {
        let symbol = hook_runtime::hook_symbol_by_name(pass.eval, "window-state-change-hook");
        safe_named_hook(pass.eval, symbol, &[])?;
    }
    Ok(Value::NIL)
}

/// safe_run_hooks_2 at the committed-start seam, without a Lisp-visible
/// inhibit-redisplay binding. The enclosing private transaction owns reentry.
pub(super) fn run_committed_scroll(
    eval: &mut super::eval::Context,
    window: WindowId,
) -> EvalResult {
    let mut guard = eval.gnu_guard_committed_scroll();
    let eval = &mut *guard.eval;
    let frame = eval.frames.find_window_frame_id(window).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), Value::make_window(window.0)],
        )
    })?;
    let window_arg = Value::make_window(window.0);
    let start = super::super::window_cmds::builtin_window_start(eval, vec![window_arg])?;
    let saved = eval.buffers.current_buffer_id();
    let mut scope = BufferContext { eval, saved };
    if let Some(buffer) = super::window_buffer_id_in_state(scope.eval, frame, window) {
        scope.eval.set_current_buffer_unrecorded(buffer)?;
    }
    let symbol = hook_runtime::hook_symbol_by_name(scope.eval, "window-scroll-functions");
    safe_named_hook(scope.eval, symbol, &[window_arg, start])
}

/// Exclusively borrows the owning Context while restoring configuration-hook state.
/// Saved IDs and the root-stack boundary belong to that Context; independent
/// mutators never share this guard or its mutable restore state.
struct ConfigurationContext<'a> {
    eval: &'a mut super::eval::Context,
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
            .filter(|frame| self.eval.frames.get(*frame).is_some())
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
    eval: &'a mut super::eval::Context,
    window: Option<WindowId>,
}
impl SelectedWindowContext<'_> {
    fn restore(&mut self) -> EvalResult {
        if let Some(window) = self
            .window
            .take()
            .filter(|window| self.eval.frames.find_window_frame_id(*window).is_some())
        {
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

fn run_funs(eval: &mut super::eval::Context, mut functions: Value) -> EvalResult {
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
}

/// Configuration retains its separate selection/unwind contract (window.c3790):
/// save current buffer only if it differs at entry; restore selected frame only
/// when selecting the requested frame at entry; restore each local window;
/// leave deliberate global callback selections intact when no outer restore
/// was registered. Snapshot and root the default list before local callbacks.
pub(super) fn run_configuration(eval: &mut super::eval::Context, frame: FrameId) -> EvalResult {
    let symbol = crate::emacs_core::intern::intern("window-configuration-change-hook");
    let functions = default_hook_value(eval, symbol)?;
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(functions);
    let selected_frame = eval.frames.selected_frame().map(|frame| frame.id);
    let selected_buffer = eval
        .frames
        .selected_frame()
        .and_then(|frame| frame.selected_window())
        .and_then(|window| window.buffer_id());
    let saved_buffer = (eval.buffers.current_buffer_id() != selected_buffer)
        .then(|| eval.buffers.current_buffer_id())
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
        let Some(state) = scope.eval.frames.get(frame) else {
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
            let Some(buffer) = super::window_buffer_id_in_state(scope.eval, frame, window) else {
                continue;
            };
            if !scope
                .eval
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
#[path = "redisplay_gnu/tests/default_read.rs"]
mod default_read_tests;
