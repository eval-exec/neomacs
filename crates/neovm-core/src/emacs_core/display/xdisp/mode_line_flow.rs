//! GNU's `dsafe_eval` policy at the mode-line Lisp boundary.
//!
//! | Knob | Default | Effect |
//! | --- | --- | --- |
//! | `NEOVM_MODE_LINE_FLOW` | off | Catch/log signals, propagate nonlocal exits after restoring display scopes. Graduate after active GNU parity and instruction gates. |

use super::*;
use crate::emacs_core::error::{FlowKind, FlowResultExt};
use crate::emacs_core::eval::{ConditionFrame, ResumeTarget};

/// Immutable process configuration; shared by mutators, contains no Lisp state.
#[inline]
pub(super) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NEOVM_MODE_LINE_FLOW").is_ok_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "on" | "true" | "yes"
            )
        })
    })
}

pub(super) fn eval_form(eval: &mut super::super::eval::Context, form: &Value) -> EvalResult {
    if !enabled() {
        return Ok(eval.eval_value(form).unwrap_or(Value::NIL));
    }
    // GNU xdisp.c dsafe__call binds these before installing its Qt condition
    // handler, even for Fformat_mode_line. Binding failures stay outside it.
    let count = eval.specpdl.len();
    eval.try_specbind_or_unwind_to(count, intern("inhibit-redisplay"), Value::T)?;
    eval.try_specbind_or_unwind_to(count, intern("inhibit-quit"), Value::T)?;
    let condition_stack_base = eval.condition_stack_len();
    eval.push_condition_frame(ConditionFrame::ConditionCase {
        conditions: Value::T,
        resume: ResumeTarget::InterpreterConditionCase {
            handler_index: 0,
            condition_stack_base,
        },
    });
    // dsafe_eval calls the current `eval` function with lexical argument t.
    // The internal handler blocks outer handler-bind callbacks and ordinary
    // debugger entry; debug-on-signal retains GNU's explicit override.
    let result = eval.funcall_general(Value::symbol("eval"), vec![*form, Value::T]);
    eval.truncate_condition_stack(condition_stack_base);
    let result = if result.is_err() {
        handle_error(eval, form, result)
    } else {
        result
    };
    // GNU removes the internal handler before logging, then unbinds the safe
    // call's bindings. Signals from that outer unwind must still propagate.
    eval.unbind_to_with_result(count, result)
}

#[cold]
#[inline(never)]
fn handle_error(
    eval: &mut super::super::eval::Context,
    form: &Value,
    result: EvalResult,
) -> EvalResult {
    match result.kinded() {
        Err(FlowKind::Signal(data)) => {
            let message = signal_message(form, &data);
            eval.add_to_log(&message);
            tracing::debug!("{message}");
            Ok(Value::NIL)
        }
        other => other.map_err(Flow::from_kind),
    }
}

#[cold]
#[inline(never)]
fn signal_message(form: &Value, data: &crate::emacs_core::error::SignalData) -> String {
    use crate::emacs_core::print::print_value;
    let payload = data
        .data
        .iter()
        .map(print_value)
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "Error during redisplay: (eval {} t) signaled ({} {})",
        print_value(form),
        print_value(&Value::from_sym_id(data.symbol)),
        payload
    )
}

pub(super) fn split_eval_result(form: &Value, result: EvalResult) -> EvalResult {
    if !enabled() || result.is_ok() {
        return result;
    }
    match result.kinded() {
        Err(FlowKind::Signal(data)) => {
            // The compatibility seam has no mutable Context for *Messages*.
            tracing::debug!("{}", signal_message(form, &data));
            Ok(Value::NIL)
        }
        other => other.map_err(Flow::from_kind),
    }
}

/// Fformat_mode_line's window/point unwind data. Owned by one Context's
/// mutator for this call. Its marker is pinned to that Context's specpdl;
/// neither the selection token nor its Lisp state is shared between mutators.
pub(super) struct FormatSelection {
    original_window: Option<WindowId>,
    target_selection: (Option<FrameId>, Option<(FrameId, WindowId)>),
    target_point: Option<(BufferId, EmacsBytePos)>,
    point_marker: Option<(u64, Value)>,
    root_scope: crate::emacs_core::eval::SpecpdlRootScopeState,
    point_root: crate::emacs_core::eval::SpecpdlRootSlot,
}

impl FormatSelection {
    pub(super) fn enter(
        eval: &mut super::super::eval::Context,
        window: Option<&Value>,
    ) -> Result<Self, Flow> {
        let original_window = eval
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window);
        let target = window
            .and_then(|value| value.as_window_id())
            .map(WindowId)
            .or(original_window);
        let original_frame = eval.frames.selected_frame().map(|frame| frame.id);
        let target_frame = target
            .and_then(|wid| eval.frames.find_window_frame_id(wid))
            .and_then(|fid| eval.frames.get(fid));
        let target_selection = (
            original_frame,
            target_frame.map(|frame| (frame.id, frame.selected_window)),
        );
        let target_point = target_frame
            .and_then(|frame| frame.selected_window())
            .and_then(Window::buffer_id)
            .and_then(|bid| {
                eval.buffers
                    .get(bid)
                    .map(|buffer| (bid, buffer.point_emacs_byte_pos()))
            });
        if let Some(wid) = target.filter(|wid| Some(*wid) != original_window) {
            super::super::window_cmds::select_window(
                eval,
                wid,
                Value::T,
                crate::window::FrameFocusTracking::FollowSelection,
            )?;
        }
        let root_scope = eval.save_specpdl_roots();
        let point_root = eval.push_specpdl_root_slot(Value::NIL);
        Ok(Self {
            original_window,
            target_selection,
            target_point,
            point_marker: None,
            root_scope,
            point_root,
        })
    }

    /// Pure elements cannot move point. Allocate GNU's saved-point marker only
    /// before the first Lisp evaluation, keeping non-evaluating walks cheap.
    pub(super) fn before_eval(&mut self, eval: &mut super::super::eval::Context) {
        if self.point_marker.is_some() {
            return;
        }
        if let Some((bid, point)) = self.target_point
            && let Some(buffer) = eval.buffers.get(bid)
        {
            let position = buffer.emacs_byte_pos_to_lisp_char_pos(point);
            let marker = crate::emacs_core::marker::make_registered_buffer_marker(
                &mut eval.buffers,
                bid,
                position,
                false,
            );
            if let Some(id) = marker.as_marker_data().and_then(|data| data.marker_id) {
                eval.set_specpdl_root_slot(&self.point_root, marker);
                self.point_marker = Some((id, marker));
            }
        }
    }

    pub(super) fn restore(self, eval: &mut super::super::eval::Context) {
        let frame_before = eval.frames.selected_frame().map(|frame| frame.id);
        eval.frames
            .restore_selected_window_for_mode_line(self.target_selection);
        if let Some(wid) = self
            .original_window
            .filter(|wid| eval.frames.is_live_window_id(*wid))
        {
            let selected = eval
                .frames
                .selected_frame()
                .map(|frame| frame.selected_window);
            let window_buffer = eval
                .frames
                .find_window_frame_id(wid)
                .and_then(|fid| eval.frames.get(fid))
                .and_then(|frame| frame.find_window(wid))
                .and_then(Window::buffer_id);
            if selected != Some(wid) || eval.buffers.current_buffer_id() != window_buffer {
                let _ = super::super::window_cmds::select_window(
                    eval,
                    wid,
                    Value::T,
                    crate::window::FrameFocusTracking::FollowSelection,
                );
            }
        }
        if frame_before != eval.frames.selected_frame().map(|frame| frame.id) {
            eval.sync_keyboard_terminal_owner();
        }
        if let Some((bid, saved_point)) = self.target_point {
            let point = self
                .point_marker
                .as_ref()
                .and_then(|(id, _)| eval.buffers.marker_emacs_byte_pos(bid, *id))
                .unwrap_or(saved_point);
            if let Some(buffer) = eval.buffers.get_mut(bid) {
                buffer.goto_emacs_byte_pos(point);
            }
        }
        if let Some((_, marker)) = self.point_marker {
            crate::emacs_core::marker::unchain_marker(&mut eval.buffers, &marker);
        }
        eval.restore_specpdl_roots(self.root_scope);
    }
}
