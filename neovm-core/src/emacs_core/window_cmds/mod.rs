//! Window, frame, and display-related builtins for the Elisp VM.
//!
//! Bridges the `FrameManager` (in `crate::window`) to Elisp by exposing
//! builtins such as `selected-window`, `split-window-internal`,
//! `selected-frame`, etc.
//! Frames are represented as frame handles. Windows are represented as window
//! handles, while legacy integer designators are still accepted in resolver
//! paths for compatibility.

use super::error::{EvalResult, Flow, signal};
use super::intern::{SymId, resolve_sym};
use super::minibuffer::MinibufferManager;
use super::value::{Value, ValueKind, VecLikeType, list_to_vec};
use crate::buffer::{BufferId, BufferManager, EmacsBytePos, LispCharPos1};
use crate::window::{
    CursorTypeSymbol, FrameId, FrameManager, FrameParam, FrameParamKey, Rect, SplitDirection,
    SplitPlacement, Window, WindowBufferDisplayDefaults, WindowFringeDefaults, WindowId,
    WindowMargins, WindowScrollBarDefaults, is_valid_horizontal_scroll_bar_value,
    is_valid_vertical_scroll_bar_value, window_first_child_id, window_next_sibling_id,
    window_parent_id, window_prev_sibling_id,
};
use std::collections::HashSet;
use strum::{EnumString, IntoStaticStr};

fn lisp_char_pos_from_one_based_usize(pos: usize) -> LispCharPos1 {
    LispCharPos1::from_one_based_usize(pos)
}

pub(crate) use super::builtins::symbols::{
    builtin_resize_mini_window_internal, builtin_set_window_new_normal,
    builtin_set_window_new_pixel, builtin_set_window_new_total,
};
pub(crate) use super::builtins::{
    builtin_coordinates_in_window_p, builtin_current_window_configuration,
    builtin_run_window_configuration_change_hook, builtin_run_window_scroll_functions,
    builtin_set_window_configuration, builtin_split_window_internal,
    builtin_window_configuration_equal_p, builtin_window_configuration_frame,
    builtin_window_configuration_p,
};
pub(crate) use super::builtins::{
    builtin_window_lines_pixel_dimensions, builtin_window_new_normal, builtin_window_new_pixel,
    builtin_window_new_total, builtin_window_old_body_pixel_height,
    builtin_window_old_body_pixel_width, builtin_window_old_pixel_height,
    builtin_window_old_pixel_width,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Expect exactly N arguments.
fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Expect at least N arguments.
fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Expect at most N arguments.
fn expect_max_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    if args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Extract an integer from a Value.
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

/// Extract a numeric value from a Value.
fn expect_number(value: &Value) -> Result<f64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(value.xfloat()),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("numberp"), *value],
        )),
    }
}

fn expect_buffer_name_string(value: &Value) -> Result<String, Flow> {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .ok_or_else(|| {
            signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), *value],
            )
        })
}

fn find_buffer_by_name_arg(
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<BufferId>, Flow> {
    let name = expect_buffer_name_string(value)?;
    Ok(buffers.find_buffer_by_name(&name))
}

fn find_or_create_buffer_by_name_arg(
    buffers: &mut BufferManager,
    value: &Value,
) -> Result<BufferId, Flow> {
    let name = expect_buffer_name_string(value)?;
    Ok(buffers
        .find_buffer_by_name(&name)
        .unwrap_or_else(|| buffers.create_buffer(&name)))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString)]
#[strum(serialize_all = "kebab-case")]
enum AllFramesSymbol {
    Visible,
}

impl AllFramesSymbol {
    fn from_lisp_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AllFramesScope {
    BaseFrame,
    AllFrames,
    VisibleFrames,
    VisibleOrIconifiedFrames,
    SpecificFrame(FrameId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum SplitWindowSide {
    Above,
    Below,
    Left,
    Right,
}

impl SplitWindowSide {
    pub(crate) fn from_lisp_value(value: &Value) -> Option<Self> {
        if value.is_nil() {
            return Some(Self::Below);
        }
        if value.is_t() {
            return Some(Self::Right);
        }
        value.as_symbol_name()?.parse().ok()
    }

    pub(crate) fn is_horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        self.into()
    }
}

fn decode_all_frames_scope(
    frames: &FrameManager,
    value: Option<Value>,
) -> Result<AllFramesScope, Flow> {
    let Some(value) = value else {
        return Ok(AllFramesScope::BaseFrame);
    };
    if value.is_nil() {
        return Ok(AllFramesScope::BaseFrame);
    }
    if value == Value::T {
        return Ok(AllFramesScope::AllFrames);
    }
    if AllFramesSymbol::from_lisp_value(value) == Some(AllFramesSymbol::Visible) {
        return Ok(AllFramesScope::VisibleFrames);
    }
    if value.as_fixnum() == Some(0) {
        return Ok(AllFramesScope::VisibleOrIconifiedFrames);
    }
    if let Some(raw_id) = value.as_frame_id() {
        let frame_id = FrameId(raw_id);
        if frames.get(frame_id).is_none() {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), value],
            ));
        }
        return Ok(AllFramesScope::SpecificFrame(frame_id));
    }
    Ok(AllFramesScope::BaseFrame)
}

fn frame_ids_for_all_frames_scope(
    frames: &FrameManager,
    base_fid: FrameId,
    scope: AllFramesScope,
) -> Vec<FrameId> {
    let mut ids = match scope {
        AllFramesScope::BaseFrame => vec![base_fid],
        AllFramesScope::AllFrames => frames.frame_list(),
        AllFramesScope::VisibleFrames | AllFramesScope::VisibleOrIconifiedFrames => frames
            .frame_list()
            .into_iter()
            .filter(|frame_id| frames.get(*frame_id).is_some_and(|frame| frame.visible))
            .collect(),
        AllFramesScope::SpecificFrame(frame_id) => vec![frame_id],
    };
    ids.sort_by_key(|frame_id| frame_id.0);
    if let Some(start_pos) = ids.iter().position(|frame_id| *frame_id == base_fid) {
        ids.rotate_left(start_pos);
    }
    ids
}

#[derive(Clone, Debug)]
enum IntegerOrMarkerArg {
    Int(i64),
    Marker { raw: Value, position: Option<i64> },
}

fn parse_integer_or_marker_arg(value: &Value) -> Result<IntegerOrMarkerArg, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(IntegerOrMarkerArg::Int(n)),
        _ if value.is_marker() => {
            let position = super::marker::marker_logical_fields(value)
                .and_then(|(_, position, _)| position.map(|pos| pos.as_i64()));
            Ok(IntegerOrMarkerArg::Marker {
                raw: *value,
                position,
            })
        }
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

fn expect_number_or_marker_count(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        ValueKind::Float => Ok(value.xfloat().floor() as i64),
        _ if value.is_marker() => match parse_integer_or_marker_arg(value)? {
            IntegerOrMarkerArg::Marker {
                position: Some(pos),
                ..
            } => Ok(pos),
            IntegerOrMarkerArg::Marker { position: None, .. } => Err(signal(
                "error",
                vec![Value::string("Marker does not point anywhere")],
            )),
            IntegerOrMarkerArg::Int(pos) => Ok(pos),
        },
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), *value],
        )),
    }
}

fn clamped_window_position_in_state(
    frames: &FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
    pos: i64,
) -> Option<LispCharPos1> {
    if pos <= 0 {
        return None;
    }
    let requested = pos as usize;
    let Some(Window::Leaf { buffer_id, .. }) =
        frames.get(fid).and_then(|frame| frame.find_window(wid))
    else {
        return Some(LispCharPos1::from_one_based_usize(requested));
    };
    let buffer_end = buffers
        .get(*buffer_id)
        .map(|buf| buf.total_char_len().get().saturating_add(1))
        .unwrap_or(requested);
    Some(LispCharPos1::from_one_based_usize(
        requested.min(buffer_end.max(1)),
    ))
}

/// Extract a fixnum-like integer from a Value.
fn expect_fixnum(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("fixnump"), *value],
        )),
    }
}

/// Extract a number-or-marker argument as f64.
fn expect_number_or_marker(value: &Value) -> Result<f64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(value.xfloat()),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), *value],
        )),
    }
}

/// Parse a window margin argument (`nil` or non-negative integer).
fn expect_margin_width(value: &Value) -> Result<usize, Flow> {
    const MAX_MARGIN: i64 = 2_147_483_647;
    match value.kind() {
        ValueKind::Nil => Ok(0),
        ValueKind::Fixnum(n) => {
            if n < 0 || n > MAX_MARGIN {
                return Err(signal(
                    "args-out-of-range",
                    vec![
                        Value::fixnum(n),
                        Value::fixnum(0),
                        Value::fixnum(MAX_MARGIN),
                    ],
                ));
            }
            Ok(n as usize)
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

fn buffer_margin_width(
    buffers: &BufferManager,
    buffer_id: BufferId,
    name: &str,
) -> Result<usize, Flow> {
    let value = buffers
        .get(buffer_id)
        .and_then(|buffer| buffer.buffer_local_value(name))
        .unwrap_or(Value::NIL);
    expect_margin_width(&value)
}

fn buffer_local_value(buffers: &BufferManager, buffer_id: BufferId, name: &str) -> Value {
    buffers
        .get(buffer_id)
        .and_then(|buffer| buffer.buffer_local_value(name))
        .unwrap_or(Value::NIL)
}

fn buffer_local_optional_dimension(
    buffers: &BufferManager,
    buffer_id: BufferId,
    name: &str,
) -> Result<Option<i32>, Flow> {
    let value = buffer_local_value(buffers, buffer_id, name);
    if value.is_nil() {
        Ok(None)
    } else {
        Ok(Some(i32::try_from(expect_int(&value)?).map_err(|_| {
            signal(
                "args-out-of-range",
                vec![value, Value::fixnum(0), Value::fixnum(i64::from(i32::MAX))],
            )
        })?))
    }
}

fn valid_vertical_scroll_bar_type(value: Value) -> bool {
    is_valid_vertical_scroll_bar_value(value)
}

fn valid_horizontal_scroll_bar_type(value: Value) -> bool {
    is_valid_horizontal_scroll_bar_value(value)
}

fn window_value(wid: WindowId) -> Value {
    Value::make_window(wid.0)
}

fn resolve_window_frame_id_for_pred(
    frames: &FrameManager,
    wid: WindowId,
    pred: &str,
) -> Option<FrameId> {
    match pred {
        "window-valid-p" => frames.find_valid_window_frame_id(wid),
        _ => frames.find_window_frame_id(wid),
    }
}

fn window_id_from_designator(value: &Value) -> Option<WindowId> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Window) => Some(WindowId(value.as_window_id().unwrap())),
        ValueKind::Fixnum(n) if n >= 0 => Some(WindowId(n as u64)),
        _ => None,
    }
}

/// Resolve an optional window designator.
///
/// - nil/omitted => selected window of selected frame
/// - non-nil invalid designator => `(wrong-type-argument PRED VALUE)`
fn resolve_window_id_with_pred(
    eval: &mut super::eval::Context,
    arg: Option<&Value>,
    pred: &str,
) -> Result<(FrameId, WindowId), Flow> {
    resolve_window_id_with_pred_in_state(&mut eval.frames, &mut eval.buffers, arg, pred)
}

fn resolve_window_id_with_pred_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
    pred: &str,
) -> Result<(FrameId, WindowId), Flow> {
    if arg.is_none_or(|v| v.is_nil()) {
        let frame_id = ensure_selected_frame_id_in_state(frames, buffers);
        let frame = frames
            .get(frame_id)
            .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
        return Ok((frame_id, frame.selected_window));
    }
    let val = arg.unwrap(); // None case handled above
    let Some(wid) = window_id_from_designator(val) else {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol(pred), *val],
        ));
    };
    if let Some(frame_id) = resolve_window_frame_id_for_pred(frames, wid, pred) {
        Ok((frame_id, wid))
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol(pred), *val],
        ))
    }
}

fn resolve_window_id(
    eval: &mut super::eval::Context,
    arg: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    resolve_window_id_with_pred(eval, arg, "window-live-p")
}

fn resolve_window_id_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    resolve_window_id_with_pred_in_state(frames, buffers, arg, "window-live-p")
}

fn frame_divider_width(frame: &crate::window::Frame, parameter: FrameParam) -> i64 {
    frame
        .known_parameter(parameter)
        .and_then(|value| value.as_int())
        .unwrap_or(0)
        .max(0)
}

fn window_is_rightmost(frame: &crate::window::Frame, window_id: WindowId) -> bool {
    frame
        .find_window(window_id)
        .is_none_or(|window| window.bounds().x + window.bounds().width >= frame.width as f32 - 1.0)
}

fn window_is_bottommost(frame: &crate::window::Frame, window_id: WindowId) -> bool {
    frame.find_window(window_id).is_none_or(|window| {
        window.bounds().y + window.bounds().height >= frame.height as f32 - 1.0
    })
}

/// Resolve an optional window designator that may be stale (window object).
///
/// - nil/omitted => selected live window
/// - non-nil invalid designator => `(wrong-type-argument PRED VALUE)`
fn resolve_window_object_id_with_pred(
    eval: &mut super::eval::Context,
    arg: Option<&Value>,
    pred: &str,
) -> Result<WindowId, Flow> {
    resolve_window_object_id_with_pred_in_state(&mut eval.frames, &mut eval.buffers, arg, pred)
}

fn resolve_window_object_id_with_pred_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
    pred: &str,
) -> Result<WindowId, Flow> {
    if arg.is_none_or(|v| v.is_nil()) {
        let (_fid, wid) = resolve_window_id_with_pred_in_state(frames, buffers, None, pred)?;
        return Ok(wid);
    }
    let val = arg.unwrap();
    let Some(wid) = window_id_from_designator(val) else {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol(pred), *val],
        ));
    };
    if frames.is_window_object_id(wid) {
        Ok(wid)
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol(pred), *val],
        ))
    }
}

/// Resolve a window designator for mutation-style window ops.
///
/// GNU Emacs uses generic `error` signaling for invalid designators in some
/// split/delete window builtins, rather than `wrong-type-argument`.
fn resolve_window_id_or_error(
    eval: &mut super::eval::Context,
    arg: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    resolve_window_id_or_error_in_state(&mut eval.frames, &mut eval.buffers, arg)
}

fn resolve_window_id_or_error_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    if arg.is_none_or(|v| v.is_nil()) {
        return resolve_window_id_in_state(frames, buffers, arg);
    }
    let value = arg.unwrap();
    let Some(wid) = window_id_from_designator(value) else {
        // GNU window.c: CHECK_VALID_WINDOW signals wrong-type-argument
        // with window-valid-p (or windowp for non-window types).
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("windowp"), *value],
        ));
    };
    if let Some(fid) = frames.find_valid_window_frame_id(wid) {
        Ok((fid, wid))
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("window-valid-p"), *value],
        ))
    }
}

fn format_window_designator_for_error(eval: &super::eval::Context, value: &Value) -> String {
    if let Some(wid) = window_id_from_designator(value) {
        if eval.frames.is_window_object_id(wid) || value.is_window() {
            return format!("#<window {}>", wid.0);
        }
    }
    super::print::print_value(value)
}

fn resolve_window_id_or_window_error(
    eval: &mut super::eval::Context,
    arg: Option<&Value>,
    live_only: bool,
) -> Result<(FrameId, WindowId), Flow> {
    resolve_window_id_or_window_error_in_state(&mut eval.frames, &mut eval.buffers, arg, live_only)
}

fn format_window_designator_for_error_in_state(frames: &FrameManager, value: &Value) -> String {
    if let Some(wid) = window_id_from_designator(value) {
        if frames.is_window_object_id(wid) || value.is_window() {
            return format!("#<window {}>", wid.0);
        }
    }
    super::print::print_value(value)
}

fn resolve_window_id_or_window_error_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
    live_only: bool,
) -> Result<(FrameId, WindowId), Flow> {
    if arg.is_none_or(|v| v.is_nil()) {
        return resolve_window_id_with_pred_in_state(frames, buffers, arg, "window-live-p");
    }
    let val = arg.unwrap();
    let Some(wid) = window_id_from_designator(val) else {
        let window_kind = if live_only { "live" } else { "valid" };
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "{} is not a {} window",
                format_window_designator_for_error_in_state(frames, val),
                window_kind
            ))],
        ));
    };
    let frame_id = if live_only {
        frames.find_window_frame_id(wid)
    } else {
        frames.find_valid_window_frame_id(wid)
    };
    if let Some(fid) = frame_id {
        Ok((fid, wid))
    } else {
        let window_kind = if live_only { "live" } else { "valid" };
        Err(signal(
            "error",
            vec![Value::string(format!(
                "{} is not a {} window",
                format_window_designator_for_error_in_state(frames, val),
                window_kind
            ))],
        ))
    }
}

/// Resolve a frame designator, signaling predicate-shaped type errors.
///
/// When ARG is nil/omitted, GNU Emacs resolves against the selected frame.
/// In batch compatibility mode we bootstrap that frame on demand.
fn resolve_frame_id(
    eval: &mut super::eval::Context,
    arg: Option<&Value>,
    predicate: &str,
) -> Result<FrameId, Flow> {
    resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, arg, predicate)
}

pub(crate) fn resolve_frame_id_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
    predicate: &str,
) -> Result<FrameId, Flow> {
    if arg.is_none_or(|v| v.is_nil()) {
        return Ok(ensure_selected_frame_id_in_state(frames, buffers));
    }
    let val = arg.unwrap();
    match val.kind() {
        ValueKind::Fixnum(n) => {
            let fid = FrameId(n as u64);
            if frames.get(fid).is_some() {
                Ok(fid)
            } else {
                Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol(predicate), Value::fixnum(n)],
                ))
            }
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            let raw_id = val.as_frame_id().unwrap();
            let fid = FrameId(raw_id);
            if frames.get(fid).is_some() {
                Ok(fid)
            } else {
                Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol(predicate), Value::make_frame(raw_id)],
                ))
            }
        }
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol(predicate), *val],
        )),
    }
}

/// Resolve a frame designator that may also be a live window designator.
///
/// `frame-first-window` accepts either a frame or window object in GNU Emacs.
fn resolve_frame_or_window_frame_id(
    eval: &mut super::eval::Context,
    arg: Option<&Value>,
    predicate: &str,
) -> Result<FrameId, Flow> {
    resolve_frame_or_window_frame_id_in_state(&mut eval.frames, &mut eval.buffers, arg, predicate)
}

fn resolve_frame_or_window_frame_id_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
    predicate: &str,
) -> Result<FrameId, Flow> {
    if arg.is_none_or(|v| v.is_nil()) {
        return Ok(ensure_selected_frame_id_in_state(frames, buffers));
    }
    let val = arg.unwrap();
    match val.kind() {
        ValueKind::Veclike(VecLikeType::Frame) => {
            let raw_id = val.as_frame_id().unwrap();
            let fid = FrameId(raw_id);
            if frames.get(fid).is_some() {
                Ok(fid)
            } else {
                Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol(predicate), Value::make_frame(raw_id)],
                ))
            }
        }
        ValueKind::Fixnum(n) => {
            let fid = FrameId(n as u64);
            if frames.get(fid).is_some() {
                return Ok(fid);
            }
            let wid = WindowId(n as u64);
            if let Some(fid) = frames.find_valid_window_frame_id(wid) {
                return Ok(fid);
            }
            Err(signal(
                "wrong-type-argument",
                vec![Value::symbol(predicate), Value::fixnum(n)],
            ))
        }
        ValueKind::Veclike(VecLikeType::Window) => {
            let raw_id = val.as_window_id().unwrap();
            let wid = WindowId(raw_id);
            if let Some(fid) = frames.find_valid_window_frame_id(wid) {
                return Ok(fid);
            }
            Err(signal(
                "wrong-type-argument",
                vec![Value::symbol(predicate), Value::make_window(raw_id)],
            ))
        }
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol(predicate), *val],
        )),
    }
}

/// Helper: get a reference to a leaf window by id.
fn get_leaf(frames: &FrameManager, fid: FrameId, wid: WindowId) -> Result<&Window, Flow> {
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    frame
        .find_window(wid)
        .ok_or_else(|| signal("error", vec![Value::string("Window not found")]))
}

/// Look up any window (leaf or internal) by id, including the root window.
fn get_window(frames: &FrameManager, fid: FrameId, wid: WindowId) -> Result<&Window, Flow> {
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    // find_window checks root_window tree + minibuffer_leaf
    frame
        .find_window(wid)
        .ok_or_else(|| signal("error", vec![Value::string("Window not found")]))
}

/// Ensure a selected frame exists and return its id.
///
/// In batch compatibility mode, GNU Emacs still has an initial frame (`F1`).
/// When the evaluator has no frame yet, synthesize one on demand.
pub(crate) fn ensure_selected_frame_id(eval: &mut super::eval::Context) -> FrameId {
    ensure_selected_frame_id_in_state(&mut eval.frames, &mut eval.buffers)
}

pub(crate) fn ensure_selected_frame_id_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
) -> FrameId {
    ensure_selected_frame_id_in_state_with_policy(frames, buffers, true)
}

pub(crate) fn seed_batch_startup_frame_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
) -> FrameId {
    ensure_selected_frame_id_in_state_with_policy(frames, buffers, false)
}

fn ensure_selected_frame_id_in_state_with_policy(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    warn_on_create: bool,
) -> FrameId {
    if let Some(fid) = frames.selected_frame().map(|f| f.id) {
        // Ensure frame parameters required for defface spec matching.
        // The frame may have been restored from a pdump created before
        // these parameters were seeded.
        if let Some(frame) = frames.get_mut(fid) {
            if frame.parameter("display-type").is_none() {
                // GNU's `frame-set-background-mode` (frame.el) derives display-type:
                // a GUI frame is `color`; a tty frame is `color` only if
                // `tty-display-color-p`, else `mono`. A restored batch frame is a
                // no-color initial terminal, so it must be `mono` (matching GNU).
                let display_type = if frame.effective_window_system().is_some() {
                    "color"
                } else {
                    "mono"
                };
                frame.set_parameter(Value::symbol("display-type"), Value::symbol(display_type));
            }
            if frame.parameter("background-mode").is_none() {
                frame.set_parameter(Value::symbol("background-mode"), Value::symbol("dark"));
            }
        }
        return fid;
    }

    if warn_on_create {
        tracing::warn!(
            "ensure_selected_frame_id_in_state: no selected frame present; synthesizing fallback batch-style frame"
        );
    }

    let buf_id = buffers
        .current_buffer()
        .map(|b| b.id)
        .unwrap_or_else(|| buffers.create_buffer("*scratch*"));
    // GNU batch startup exposes an 80x24 text window plus a 1-line minibuffer.
    // Keep the synthetic startup frame in character-cell units so the GNU
    // `window.el` geometry helpers behave the same way in batch mode.
    //
    // The frame pixel-height must include the minibuffer (24 text + 1 mini = 25)
    // so that `recalculate_minibuffer_bounds()` correctly computes
    // max_root_h = 25 - 1 = 24 instead of clamping the root to 23.
    let fid = frames.create_frame("F1", 80, 25, buf_id);
    let minibuffer_buf_id = buffers
        .find_buffer_by_name(" *Minibuf-0*")
        .unwrap_or_else(|| buffers.create_buffer(" *Minibuf-0*"));
    if let Some(frame) = frames.get_mut(fid) {
        frame.initial = true;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_parameter(Value::symbol("width"), Value::fixnum(80));
        frame.set_parameter(Value::symbol("height"), Value::fixnum(25));
        // Required for defface spec matching (class ...) and (background ...) in
        // `face-spec-set-match-display`. GNU's `frame-set-background-mode`
        // computes a tty frame's display-type as `color` iff `tty-display-color-p`,
        // else `mono`. This synthesized batch frame is the no-color initial
        // terminal (window-system None, set just above), so it is `mono` — matching
        // GNU's batch frame and letting deffaces fall through to their `(t ...)`
        // clauses instead of selecting `(class color)` clauses.
        frame.set_parameter(Value::symbol("display-type"), Value::symbol("mono"));
        frame.set_parameter(Value::symbol("background-mode"), Value::symbol("dark"));
        // The root window covers the 24-line text area (not the minibuffer).
        frame
            .root_window
            .set_bounds(Rect::new(0.0, 0.0, 80.0, 24.0));
        if let Some(Window::Leaf {
            window_start,
            point,
            ..
        }) = frame.find_window_mut(frame.selected_window)
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
        {
            let sel = frame.selected_window;
            if let Some(w) = frame.find_window_mut(sel) {
                crate::window::window_markers::create_window_markers(buffers, w, buf_id);
            }
        }
        if let Some(minibuffer_leaf) = frame.minibuffer_leaf.as_mut() {
            minibuffer_leaf.set_buffer(minibuffer_buf_id);
            minibuffer_leaf.set_bounds(Rect::new(0.0, 24.0, 80.0, 1.0));
            crate::window::window_markers::create_window_markers(
                buffers,
                minibuffer_leaf,
                minibuffer_buf_id,
            );
        }
    }
    fid
}

/// Compute the height of a window in lines.
fn window_height_lines(w: &Window, char_height: f32) -> i64 {
    let h = w.bounds().height;
    if char_height > 0.0 {
        (h / char_height) as i64
    } else {
        0
    }
}

/// Compute the width of a window in columns.
fn window_width_cols(w: &Window, char_width: f32) -> i64 {
    let cw = w.bounds().width;
    if char_width > 0.0 {
        (cw / char_width) as i64
    } else {
        0
    }
}

fn window_height_pixels(w: &Window) -> i64 {
    w.bounds().height.max(0.0) as i64
}

fn window_width_pixels(w: &Window) -> i64 {
    w.bounds().width.max(0.0) as i64
}

fn window_body_horizontal_offsets_pixels(
    frames: &FrameManager,
    fid: FrameId,
    w: &Window,
) -> (i64, i64) {
    let Some(frame) = frames.get(fid) else {
        return (0, 0);
    };
    if frame.effective_window_system().is_none() {
        return (0, 0);
    }
    match w {
        Window::Leaf { margins, .. } => {
            let char_width = frame.char_width.max(1.0);
            let left_margin = (margins.left() as f32 * char_width).round().max(0.0) as i64;
            let right_margin = (margins.right() as f32 * char_width).round().max(0.0) as i64;
            let (left_fringe, right_fringe, _, _) = frames
                .window_fringes(w.id())
                .unwrap_or((0, 0, false, false));
            let left_scroll_bar = frames.window_left_scroll_bar_area_width(w.id());
            let right_scroll_bar = frames.window_right_scroll_bar_area_width(w.id());
            (
                left_scroll_bar
                    .saturating_add(left_fringe)
                    .saturating_add(left_margin),
                right_scroll_bar
                    .saturating_add(right_fringe)
                    .saturating_add(right_margin),
            )
        }
        Window::Internal { .. } => (0, 0),
    }
}

fn window_body_width_pixels(frames: &FrameManager, fid: FrameId, w: &Window) -> i64 {
    let total = window_width_pixels(w);
    let (left, right) = window_body_horizontal_offsets_pixels(frames, fid, w);
    total.saturating_sub(left.saturating_add(right))
}

fn is_minibuffer_window(frames: &FrameManager, fid: FrameId, wid: WindowId) -> bool {
    frames
        .get(fid)
        .is_some_and(|frame| frame.minibuffer_window == Some(wid))
}

fn filtered_window_prev_buffers(
    prev_raw: Value,
    discarded_buffers: &[Value],
) -> Result<Vec<Value>, Flow> {
    let prev_entries = list_to_vec(&prev_raw).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), prev_raw],
        )
    })?;
    Ok(prev_entries
        .into_iter()
        .filter(|entry| {
            let Some(items) = list_to_vec(entry) else {
                return true;
            };
            !items
                .first()
                .is_some_and(|first| discarded_buffers.iter().any(|buffer| *buffer == *first))
        })
        .collect())
}

fn filtered_window_next_buffers(
    next_raw: Value,
    discarded_buffers: &[Value],
) -> Result<Vec<Value>, Flow> {
    let next_entries = list_to_vec(&next_raw).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), next_raw],
        )
    })?;
    Ok(next_entries
        .into_iter()
        .filter(|entry| !discarded_buffers.iter().any(|buffer| *buffer == *entry))
        .collect())
}

fn discard_buffers_from_window_history(
    frames: &mut FrameManager,
    wid: WindowId,
    discarded_buffers: &[Value],
) -> Result<(), Flow> {
    let prev = filtered_window_prev_buffers(frames.window_prev_buffers(wid), discarded_buffers)?;
    frames.set_window_prev_buffers(wid, Value::list(prev));
    let next = filtered_window_next_buffers(frames.window_next_buffers(wid), discarded_buffers)?;
    frames.set_window_next_buffers(wid, Value::list(next));
    Ok(())
}

fn live_frame_buffer_parameter_ids(buffers: &BufferManager, mut value: Value) -> Vec<BufferId> {
    let mut ids = Vec::new();
    while value.is_cons() {
        let car = value.cons_car();
        if let Some(id) = car.as_buffer_id()
            && buffers.get(id).is_some()
        {
            ids.push(id);
        }
        value = value.cons_cdr();
    }
    ids
}

fn should_record_window_history_buffer(
    frames: &FrameManager,
    minibuffers: &MinibufferManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
    buffer_id: BufferId,
) -> bool {
    if is_minibuffer_window(frames, fid, wid) {
        return minibuffers.has_buffer(buffer_id);
    }
    buffers
        .get(buffer_id)
        .is_some_and(|buffer| !buffer.name_starts_with_space())
}

fn window_body_height_lines(frames: &FrameManager, fid: FrameId, wid: WindowId, w: &Window) -> i64 {
    let ch = frames.get(fid).map(|f| f.char_height).unwrap_or(16.0);
    let lines = window_height_lines(w, ch);
    if is_minibuffer_window(frames, fid, wid) {
        lines
    } else {
        lines.saturating_sub(1)
    }
}

fn window_edges_cols_lines(w: &Window, char_width: f32, char_height: f32) -> (i64, i64, i64, i64) {
    let b = w.bounds();
    let left = if char_width > 0.0 {
        (b.x / char_width) as i64
    } else {
        0
    };
    let top = if char_height > 0.0 {
        (b.y / char_height) as i64
    } else {
        0
    };
    let right = if char_width > 0.0 {
        ((b.x + b.width) / char_width) as i64
    } else {
        0
    };
    let bottom = if char_height > 0.0 {
        ((b.y + b.height) / char_height) as i64
    } else {
        0
    };
    (left, top, right, bottom)
}

fn window_edges_pixels(w: &Window) -> (i64, i64, i64, i64) {
    let b = w.bounds();
    (
        b.x.max(0.0) as i64,
        b.y.max(0.0) as i64,
        (b.x + b.width).max(0.0) as i64,
        (b.y + b.height).max(0.0) as i64,
    )
}

fn window_body_edges_cols_lines(
    frames: &FrameManager,
    fid: FrameId,
    wid: WindowId,
    w: &Window,
    char_width: f32,
    char_height: f32,
) -> (i64, i64, i64, i64) {
    let (left, top, right, bottom) = window_body_edges_pixels(frames, fid, wid, w);
    let left = if char_width > 0.0 {
        (left as f32 / char_width).floor() as i64
    } else {
        0
    };
    let top = if char_height > 0.0 {
        (top as f32 / char_height).floor() as i64
    } else {
        0
    };
    let right = if char_width > 0.0 {
        (right as f32 / char_width).ceil() as i64
    } else {
        0
    };
    let bottom = if char_height > 0.0 {
        (bottom as f32 / char_height).ceil() as i64
    } else {
        0
    };
    (left, top, right, bottom)
}

fn window_body_edges_pixels(
    frames: &FrameManager,
    fid: FrameId,
    wid: WindowId,
    w: &Window,
) -> (i64, i64, i64, i64) {
    let (left, top, right, bottom) = window_edges_pixels(w);
    let (body_left_offset, _body_right_offset) =
        window_body_horizontal_offsets_pixels(frames, fid, w);
    let mode_line_height = if is_minibuffer_window(frames, fid, wid) {
        0
    } else {
        frames
            .get(fid)
            .map(|frame| frame.char_height.max(0.0) as i64)
            .unwrap_or(0)
    };
    let body_left = left.saturating_add(body_left_offset);
    let body_right = body_left.saturating_add(window_body_width_pixels(frames, fid, w));
    let body_bottom = bottom.saturating_sub(mode_line_height);
    (body_left, top, body_right.min(right), body_bottom)
}

// ===========================================================================
// Window queries
// ===========================================================================
/// `(selected-window)` -> window object.
pub(crate) fn builtin_selected_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("selected-window", &args, 0)?;
    let fid = ensure_selected_frame_id_in_state(frames, buffers);
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    Ok(window_value(frame.selected_window))
}

/// `(old-selected-window)` -> previous selected window.
pub(crate) fn builtin_old_selected_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("old-selected-window", &args, 0)?;
    let fid = ensure_selected_frame_id(eval);
    let selected_wid = eval
        .frames
        .get(fid)
        .map(|frame| frame.selected_window)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    let old_wid = eval.frames.old_selected_window().unwrap_or(selected_wid);
    Ok(window_value(old_wid))
}
/// `(frame-selected-window &optional FRAME)` -> selected window of FRAME.
pub(crate) fn builtin_frame_selected_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-selected-window", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(window_value(frame.selected_window))
}
/// `(frame-old-selected-window &optional FRAME)` -> the previously
/// selected window of FRAME.
///
/// Mirrors GNU `Fframe_old_selected_window` (`src/frame.c`):
/// returns the value of `frame->old_selected_window`, which is
/// updated by `select-window` / `set-frame-selected-window` /
/// `set-window-configuration` whenever the live `selected_window`
/// changes. Window audit Critical 8 in
/// `drafts/window-system-audit.md`: this builtin used to be a
/// stub returning `nil`, so blink-cursor-mode and other Lisp
/// callers that branch on the previous selection always took the
/// "no previous selection" path.
pub(crate) fn builtin_frame_old_selected_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-old-selected-window", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(frame
        .old_selected_window
        .map(window_value)
        .unwrap_or(Value::NIL))
}

/// `(frame-focus &optional FRAME)` -> frame receiving FRAME's keystrokes, or nil.
pub(crate) fn builtin_frame_focus(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-focus", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(frame.focus_frame_value())
}

/// `(frame-parent &optional FRAME)` -> parent frame or nil.
pub(crate) fn builtin_frame_parent(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-parent", &args, 1)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let Some(parent) = eval.frames.frame_parent_id(fid) else {
        return Ok(Value::NIL);
    };
    Ok(Value::make_frame(parent.0))
}

/// `(frame-ancestor-p ANCESTOR DESCENDANT)` -> t if ANCESTOR parents DESCENDANT.
pub(crate) fn builtin_frame_ancestor_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("frame-ancestor-p", &args, 2)?;
    let ancestor = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let descendant = resolve_frame_id(eval, args.get(1), "frame-live-p")?;
    Ok(Value::bool_val(
        eval.frames.frame_ancestor_p(ancestor, descendant),
    ))
}

fn frame_root_position(frames: &FrameManager, fid: FrameId) -> (i64, i64) {
    let mut x = 0;
    let mut y = 0;
    let mut current = Some(fid);
    let mut seen = HashSet::new();
    while let Some(frame_id) = current {
        if !seen.insert(frame_id) {
            break;
        }
        let Some(frame) = frames.get(frame_id) else {
            break;
        };
        x += frame.left_pos;
        y += frame.top_pos;
        current = frames.frame_parent_id(frame_id);
    }
    (x, y)
}

fn tty_frame_edges_value(frame: &crate::window::Frame) -> Value {
    Value::list(vec![
        Value::fixnum(frame.left_pos),
        Value::fixnum(frame.top_pos),
        Value::fixnum(frame.left_pos + i64::from(frame.width)),
        Value::fixnum(frame.top_pos + i64::from(frame.height)),
    ])
}

/// `(tty-frame-edges &optional FRAME TYPE)` -> native terminal frame edges.
pub(crate) fn builtin_tty_frame_edges(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-frame-edges", &args, 2)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    if frame.initial || frame.effective_window_system().is_some() {
        return Ok(Value::NIL);
    }
    Ok(tty_frame_edges_value(frame))
}

/// `(neomacs-frame-edges &optional FRAME TYPE)` -> GUI frame edges.
///
/// GNU's toolkit-specific `*-frame-edges` functions return a four-number
/// edge list for `outer-edges`, `native-edges` (or nil), and `inner-edges`.
/// Neomacs renders frames into one GPU-composited display surface, so native
/// and outer edges currently coincide; inner edges exclude the frame's
/// internal border just like GNU's `frame_geometry`.
pub(crate) fn builtin_neomacs_frame_edges(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("neomacs-frame-edges", &args, 2)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    if frame.initial || frame.effective_window_system().is_none() {
        return Ok(Value::NIL);
    }

    let (left, top) = eval.frames.frame_origin_in_root(fid).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Frame origin unavailable for frame edges")],
        )
    })?;
    let mut left = left.round() as i64;
    let mut top = top.round() as i64;
    let mut right = left.saturating_add(i64::from(frame.width));
    let mut bottom = top.saturating_add(i64::from(frame.height));

    if args
        .get(1)
        .is_some_and(|value| value.is_symbol_named("inner-edges"))
    {
        let border = frame.internal_border_width().max(0);
        left = left.saturating_add(border);
        top = top.saturating_add(border);
        right = right.saturating_sub(border);
        bottom = bottom.saturating_sub(border);
    }

    Ok(Value::list(vec![
        Value::fixnum(left),
        Value::fixnum(top),
        Value::fixnum(right),
        Value::fixnum(bottom),
    ]))
}

/// `(tty-frame-geometry &optional FRAME)` -> terminal frame geometry alist.
pub(crate) fn builtin_tty_frame_geometry(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-frame-geometry", &args, 1)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    if frame.initial || frame.effective_window_system().is_some() {
        return Ok(Value::NIL);
    }
    Ok(Value::list(vec![
        Value::cons(
            Value::symbol("outer-position"),
            Value::cons(Value::fixnum(frame.left_pos), Value::fixnum(frame.top_pos)),
        ),
        Value::cons(
            Value::symbol("outer-size"),
            Value::cons(
                Value::fixnum(frame.width.into()),
                Value::fixnum(frame.height.into()),
            ),
        ),
        Value::cons(Value::symbol("outer-border-width"), Value::fixnum(0)),
        Value::cons(Value::symbol("native-edges"), tty_frame_edges_value(frame)),
    ]))
}

/// `(tty-frame-list-z-order &optional FRAME)` -> topmost first.
pub(crate) fn builtin_tty_frame_list_z_order(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-frame-list-z-order", &args, 1)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let mut frames = eval.frames.frames_in_reverse_z_order(fid, true);
    frames.reverse();
    Ok(Value::list(
        frames
            .into_iter()
            .map(|frame_id| Value::make_frame(frame_id.0))
            .collect(),
    ))
}

/// `(tty-frame-at X Y)` -> (FRAME CX CY), respecting TTY child-frame z-order.
pub(crate) fn builtin_tty_frame_at(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("tty-frame-at", &args, 2)?;
    let (Some(x), Some(y)) = (args[0].as_fixnum(), args[1].as_fixnum()) else {
        return Ok(Value::NIL);
    };
    let Some(selected) = eval.frames.selected_frame().map(|frame| frame.id) else {
        return Ok(Value::NIL);
    };
    let mut frames = eval.frames.frames_in_reverse_z_order(selected, true);
    frames.reverse();
    for fid in frames {
        let Some(frame) = eval.frames.get(fid) else {
            continue;
        };
        let (fx, fy) = frame_root_position(&eval.frames, fid);
        let width = i64::from(frame.width);
        let height = i64::from(frame.height);
        let is_child = frame.parent_frame.as_frame_id().is_some();

        if is_child && !frame.undecorated {
            if fy - 1 <= y && y <= fy + height && (x == fx - 1 || x == fx + width) {
                return Ok(Value::list(vec![
                    Value::make_frame(fid.0),
                    Value::fixnum(x - fx),
                    Value::fixnum(y - fy),
                ]));
            }
            if fx - 1 <= x && x <= fx + width && (y == fy - 1 || y == fy + height) {
                return Ok(Value::list(vec![
                    Value::make_frame(fid.0),
                    Value::fixnum(x - fx),
                    Value::fixnum(y - fy),
                ]));
            }
        }

        if fx <= x && x < fx + width && fy <= y && y < fy + height {
            return Ok(Value::list(vec![
                Value::make_frame(fid.0),
                Value::fixnum(x - fx),
                Value::fixnum(y - fy),
            ]));
        }
    }
    Ok(Value::NIL)
}

/// `(redirect-frame-focus FRAME FOCUS-FRAME)` -> nil.
pub(crate) fn builtin_redirect_frame_focus(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("redirect-frame-focus", &args, 1)?;
    expect_max_args("redirect-frame-focus", &args, 2)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    let focus_frame = if let Some(value) = args.get(1) {
        if value.is_nil() {
            Value::NIL
        } else {
            let focus_fid = resolve_frame_id_in_state(
                &mut eval.frames,
                &mut eval.buffers,
                Some(value),
                "frame-live-p",
            )?;
            Value::make_frame(focus_fid.0)
        }
    } else {
        Value::NIL
    };
    let frame = eval
        .frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    frame.focus_frame = focus_frame;
    Ok(Value::NIL)
}
/// `(set-frame-selected-window FRAME WINDOW &optional NORECORD)` -> WINDOW.
pub(crate) fn builtin_set_frame_selected_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-frame-selected-window", &args, 2)?;
    expect_max_args("set-frame-selected-window", &args, 3)?;
    let fid = resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "frame-live-p",
    )?;
    let wid = match window_id_from_designator(&args[1]) {
        Some(wid) => {
            if eval.frames.find_window_frame_id(wid).is_none() {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("window-live-p"), args[1]],
                ));
            }
            wid
        }
        None => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("window-live-p"), args[1]],
            ));
        }
    };
    let window_fid = eval
        .frames
        .find_window_frame_id(wid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    if window_fid != fid {
        return Err(signal(
            "error",
            vec![Value::string(
                "In `set-frame-selected-window', WINDOW is not on FRAME",
            )],
        ));
    }
    let selected_fid = ensure_selected_frame_id_in_state(&mut eval.frames, &mut eval.buffers);
    if fid == selected_fid {
        let mut select_args = vec![window_value(wid)];
        if let Some(norecord) = args.get(2) {
            select_args.push(*norecord);
        }
        return builtin_select_window(eval, select_args);
    }

    let frame = eval
        .frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    // GNU `Fset_frame_selected_window` does NOT touch
    // `frame->old_selected_window`. The "old" snapshot is
    // updated only by `window_change_record` (GNU
    // `src/window.c:3954-3990`) at redisplay time. neomacs's
    // analog runs from `frame_window_hook_record_from_live_state`
    // in `builtins/hooks.rs`. Window audit Critical 8.
    frame.selected_window = wid;
    Ok(window_value(wid))
}
/// `(frame-first-window &optional FRAME-OR-WINDOW)` -> first window on frame.
pub(crate) fn builtin_frame_first_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-first-window", &args, 1)?;
    let fid =
        resolve_frame_or_window_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let first = frame
        .window_list()
        .first()
        .copied()
        .unwrap_or(frame.selected_window);
    Ok(window_value(first))
}
/// `(frame-root-window &optional FRAME-OR-WINDOW)` -> root window on frame.
pub(crate) fn builtin_frame_root_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-root-window", &args, 1)?;
    let fid =
        resolve_frame_or_window_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(window_value(frame.root_window.id()))
}
/// `(minibuffer-window &optional FRAME)` -> minibuffer window of FRAME.
pub(crate) fn builtin_minibuffer_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("minibuffer-window", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    match frame.minibuffer_window {
        Some(wid) => Ok(window_value(wid)),
        None => Ok(Value::NIL),
    }
}
/// `(window-minibuffer-p &optional WINDOW)` -> t when WINDOW is minibuffer.
pub(crate) fn builtin_window_minibuffer_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-minibuffer-p", &args, 1)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let is_minibuffer = frames
        .get(fid)
        .is_some_and(|frame| frame.minibuffer_window == Some(wid));
    Ok(Value::bool_val(is_minibuffer))
}

/// `(minibuffer-selected-window)` -> selected window active at minibuffer entry.
pub(crate) fn builtin_minibuffer_selected_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-selected-window", &args, 0)?;
    Ok(eval
        .minibuffer_selected_window
        .map(window_value)
        .unwrap_or(Value::NIL))
}

/// `(active-minibuffer-window)` -> nil in batch.
pub(crate) fn builtin_active_minibuffer_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("active-minibuffer-window", &args, 0)?;
    Ok(active_minibuffer_window_id(eval)
        .map(window_value)
        .unwrap_or(Value::NIL))
}

fn active_minibuffer_window_id(eval: &super::eval::Context) -> Option<WindowId> {
    eval.active_minibuffer_window_id()
}
/// `(window-frame &optional WINDOW)` -> frame of WINDOW.
pub(crate) fn builtin_window_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-frame", &args, 1)?;
    let (fid, _wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    Ok(Value::make_frame(fid.0))
}
/// `(window-buffer &optional WINDOW)` -> buffer object.
pub(crate) fn builtin_window_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-buffer", &args, 1)?;
    let resolve_buffer = |frames: &FrameManager, fid: FrameId, wid: WindowId| -> EvalResult {
        let w = get_leaf(frames, fid, wid)?;
        match w.buffer_id() {
            Some(bid) => Ok(Value::make_buffer(bid)),
            None => Ok(Value::NIL),
        }
    };

    if args.first().is_none_or(|v| v.is_nil()) {
        let (fid, wid) =
            resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "windowp")?;
        return resolve_buffer(frames, fid, wid);
    }
    let val = args.first().unwrap();
    let Some(wid) = window_id_from_designator(val) else {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("windowp"), *val],
        ));
    };
    if let Some(fid) = frames.find_window_frame_id(wid) {
        return resolve_buffer(frames, fid, wid);
    }
    if frames.is_window_object_id(wid) {
        return Ok(Value::NIL);
    }
    Err(signal(
        "wrong-type-argument",
        vec![Value::symbol("windowp"), *val],
    ))
}
/// `(window-display-table &optional WINDOW)` -> display table or nil.
pub(crate) fn builtin_window_display_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-display-table", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(frames.window_display_table(wid))
}
/// `(set-window-display-table WINDOW TABLE)` -> TABLE.
pub(crate) fn builtin_set_window_display_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-display-table", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let table = args[1];
    frames.set_window_display_table(wid, table);
    Ok(table)
}
/// `(window-cursor-type &optional WINDOW)` -> cursor type object.
pub(crate) fn builtin_window_cursor_type(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-cursor-type", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(frames.window_cursor_type(wid))
}
/// `(set-window-cursor-type WINDOW TYPE)` -> TYPE.
///
/// Mirrors GNU `src/window.c:8601-8635 (Fset_window_cursor_type)`,
/// which validates TYPE before storing it on the window. The
/// allowed shapes are:
///
///   nil | t | box | hollow | bar | hbar
///   (box . INTEGERP)  (bar . INTEGERP)  (hbar . INTEGERP)
///
/// Anything else triggers `(error "Invalid cursor type")`. Cursor
/// audit Finding 3 in `drafts/cursor-audit.md`: this builtin used
/// to silently accept any value, which made invalid Lisp typos
/// (e.g. a number, a random symbol, a cons with a non-integer
/// width) look correct until the renderer hit them.
pub(crate) fn builtin_set_window_cursor_type(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-cursor-type", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let cursor_type = args[1];

    if !is_valid_cursor_type(cursor_type) {
        return Err(crate::emacs_core::error::signal(
            "error",
            vec![Value::string("Invalid cursor type")],
        ));
    }

    frames.set_window_cursor_type(wid, cursor_type);
    Ok(cursor_type)
}

pub(crate) fn builtin_window_cursor_info(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-cursor-info", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let Some(frame) = frames.get(fid) else {
        return Ok(Value::NIL);
    };
    let Some(window) = frame.find_window(wid) else {
        return Ok(Value::NIL);
    };
    let Some(display) = window.display() else {
        return Ok(Value::NIL);
    };
    if !display.phys_cursor_on_p || display.cursor_off_p {
        return Ok(Value::NIL);
    }
    let Some(cursor) = display.phys_cursor.as_ref() else {
        return Ok(Value::NIL);
    };
    Ok(Value::vector(vec![
        frames.window_cursor_type(wid),
        Value::fixnum(cursor.x),
        Value::fixnum(cursor.y),
        Value::fixnum(cursor.width),
        Value::fixnum(cursor.height),
        Value::fixnum(cursor.ascent),
    ]))
}

/// Returns true if VALUE is a legal `cursor-type` per GNU
/// `src/window.c:8616-8626`.
fn is_valid_cursor_type(value: Value) -> bool {
    if value.is_nil() || value == Value::T {
        return true;
    }
    if CursorTypeSymbol::from_symbol_value(&value).is_some() {
        return true;
    }
    if matches!(value.kind(), crate::emacs_core::value::ValueKind::Cons) {
        let head_ok = value
            .cons_car()
            .as_symbol_name()
            .and_then(CursorTypeSymbol::from_symbol_name)
            .is_some_and(CursorTypeSymbol::accepts_width_tail);
        let tail = value.cons_cdr();
        let tail_ok = tail.is_integer();
        return head_ok && tail_ok;
    }
    false
}
/// `(window-parameter WINDOW PARAMETER)` -> window parameter or nil.
pub(crate) fn builtin_window_parameter(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("window-parameter", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let wid =
        resolve_window_object_id_with_pred_in_state(frames, buffers, args.first(), "windowp")?;
    Ok(frames.window_parameter(wid, &args[1]).unwrap_or(Value::NIL))
}
/// `(set-window-parameter WINDOW PARAMETER VALUE)` -> VALUE.
pub(crate) fn builtin_set_window_parameter(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-parameter", &args, 3)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let wid =
        resolve_window_object_id_with_pred_in_state(frames, buffers, args.first(), "windowp")?;
    let value = args[2];
    frames.set_window_parameter(wid, args[1], value);
    Ok(value)
}
/// `(window-parameters &optional WINDOW)` -> alist of parameters.
pub(crate) fn builtin_window_parameters(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-parameters", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    Ok(frames.window_parameters_alist(wid))
}
/// `(window-parent &optional WINDOW)` -> parent window or nil.
pub(crate) fn builtin_window_parent(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-parent", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let Some(frame) = frames.get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    Ok(window_parent_id(frame, wid).map_or(Value::NIL, window_value))
}
/// `(window-top-child &optional WINDOW)` -> top child for vertical combinations.
pub(crate) fn builtin_window_top_child(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-top-child", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let Some(frame) = frames.get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    Ok(
        window_first_child_id(frame, wid, SplitDirection::Vertical)
            .map_or(Value::NIL, window_value),
    )
}
/// `(window-left-child &optional WINDOW)` -> left child for horizontal combinations.
pub(crate) fn builtin_window_left_child(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-left-child", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let Some(frame) = frames.get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    Ok(
        window_first_child_id(frame, wid, SplitDirection::Horizontal)
            .map_or(Value::NIL, window_value),
    )
}
/// `(window-next-sibling &optional WINDOW)` -> next sibling or nil.
pub(crate) fn builtin_window_next_sibling(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-next-sibling", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let Some(frame) = frames.get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    Ok(window_next_sibling_id(frame, wid).map_or(Value::NIL, window_value))
}
/// `(window-prev-sibling &optional WINDOW)` -> previous sibling or nil.
pub(crate) fn builtin_window_prev_sibling(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-prev-sibling", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let Some(frame) = frames.get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    Ok(window_prev_sibling_id(frame, wid).map_or(Value::NIL, window_value))
}
/// `(window-normal-size &optional WINDOW HORIZONTAL)` -> proportional size.
///
/// Mirrors GNU `src/window.c:973`:
///
///   return NILP (horizontal) ? w->normal_lines : w->normal_cols;
///
/// The persistent `normal_lines` and `normal_cols` slots are
/// stored on `Window::Leaf` / `Window::Internal` (initialized to
/// 1.0, updated by `window-resize-apply` from `new_normal`). See
/// audit Critical 7 in `drafts/window-system-audit.md`.
pub(crate) fn builtin_window_normal_size(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-normal-size", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let horizontal = args.get(1).is_some_and(|v| v.is_truthy());
    let Some(frame) = frames.get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    let window = frame
        .find_window(wid)
        .ok_or_else(|| signal("error", vec![Value::string("Window not found")]))?;
    Ok(if horizontal {
        window.normal_cols()
    } else {
        window.normal_lines()
    })
}
/// `(window-start &optional WINDOW)` -> integer position.
pub(crate) fn builtin_window_start(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-start", &args, 1)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    match w {
        Window::Leaf { window_start, .. } => Ok(Value::fixnum(window_start.as_i64())),
        _ => Ok(Value::fixnum(0)),
    }
}
/// `(window-group-start &optional WINDOW)` -> integer position.
///
/// Batch GNU Emacs exposes group-start as point-min (`1`) in startup flows.
pub(crate) fn builtin_window_group_start(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-group-start", &args, 1)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if frames
        .get(fid)
        .is_some_and(|frame| frame.minibuffer_window == Some(wid))
    {
        return Ok(Value::fixnum(1));
    }
    let w = get_leaf(frames, fid, wid)?;
    match w {
        Window::Leaf { window_start, .. } => Ok(Value::fixnum(window_start.as_i64())),
        _ => Ok(Value::fixnum(1)),
    }
}
fn estimated_window_end_from_body_lines(
    frames: &FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
    window_start: LispCharPos1,
    bounds: &Rect,
    buffer_id: crate::buffer::BufferId,
) -> usize {
    let Some(frame) = frames.get(fid) else {
        return window_start.to_one_based_usize();
    };
    let body_lines = if is_minibuffer_window(frames, fid, wid) {
        (bounds.height / frame.char_height) as usize
    } else {
        ((bounds.height / frame.char_height) as usize).saturating_sub(1)
    };

    let Some(buf) = buffers.get(buffer_id) else {
        return window_start.to_one_based_usize();
    };
    let buffer_end = buf.total_char_len().get().saturating_add(1);
    let text = buf.full_text_string();
    let start_char = window_start.to_char_pos().get();
    let mut char_pos = start_char;
    let mut lines_seen = 0usize;
    for (i, ch) in text.char_indices().skip(start_char) {
        if lines_seen >= body_lines {
            let _ = i;
            return (char_pos + 1).min(buffer_end);
        }
        char_pos = text[..=i].chars().count();
        if ch == '\n' {
            lines_seen += 1;
        }
    }
    buffer_end
}

/// `(window-end &optional WINDOW UPDATE)` -> integer position.
pub(crate) fn builtin_window_end(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-end", &args, 2)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    match w {
        Window::Leaf {
            window_start,
            window_end_pos,
            window_end_valid,
            bounds,
            buffer_id,
            ..
        } => {
            let update_requested = args.get(1).is_some_and(|arg| !arg.is_nil());
            let stored_end = buffers
                .get(*buffer_id)
                .map(|b| b.point_max_char_pos().get().saturating_add(1))
                .unwrap_or(window_start.to_one_based_usize())
                .saturating_sub(*window_end_pos)
                .max(1);
            if let Some(snapshot_end) = frames
                .get(fid)
                .and_then(|frame| frame.window_display_snapshot(wid))
                .and_then(|snapshot| snapshot.visible_buffer_span().map(|span| span.end()))
            {
                return Ok(Value::fixnum(snapshot_end.as_i64()));
            }
            if !update_requested
                && (*window_end_valid || stored_end > window_start.to_one_based_usize())
            {
                return Ok(Value::fixnum(stored_end as i64));
            }

            Ok(Value::fixnum(estimated_window_end_from_body_lines(
                frames,
                buffers,
                fid,
                wid,
                *window_start,
                bounds,
                *buffer_id,
            ) as i64))
        }
        _ => Ok(Value::fixnum(0)),
    }
}
/// `(window-point &optional WINDOW)` -> integer position.
pub(crate) fn builtin_window_point(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-point", &args, 1)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    match w {
        Window::Leaf {
            buffer_id, point, ..
        } => {
            let selected_live_window = frames.get(fid).is_some_and(|frame| {
                frame.selected_window == wid && frame.selected_window != WindowId(0)
            });
            if selected_live_window {
                if let Some(buffer) = buffers.get(*buffer_id) {
                    return Ok(Value::fixnum(
                        buffer.point_char_pos().get().saturating_add(1) as i64,
                    ));
                }
            }
            Ok(Value::fixnum(point.as_i64()))
        }
        _ => Ok(Value::fixnum(0)),
    }
}
/// `(set-window-start WINDOW POS &optional NOFORCE)` -> POS.
pub(crate) fn builtin_set_window_start(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-window-start", &args, 2)?;
    expect_max_args("set-window-start", &args, 3)?;
    let result = {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        let (fid, wid) =
            resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
        let pos = parse_integer_or_marker_arg(&args[1])?;
        let is_minibuffer = frames
            .get(fid)
            .is_some_and(|frame| frame.minibuffer_window == Some(wid));
        match pos {
            IntegerOrMarkerArg::Int(pos) => {
                if !is_minibuffer {
                    if let Some(clamped) =
                        clamped_window_position_in_state(frames, buffers, fid, wid, pos)
                    {
                        if let Some(window) = frames
                            .get_mut(fid)
                            .and_then(|frame| frame.find_window_mut(wid))
                        {
                            crate::window::window_markers::set_window_start_with_marker(
                                buffers, window, clamped,
                            );
                        }
                    }
                }
                Value::fixnum(pos)
            }
            IntegerOrMarkerArg::Marker { raw, position } => {
                if !is_minibuffer {
                    if let Some(pos) = position {
                        if let Some(clamped) =
                            clamped_window_position_in_state(frames, buffers, fid, wid, pos)
                        {
                            if let Some(window) = frames
                                .get_mut(fid)
                                .and_then(|frame| frame.find_window_mut(wid))
                            {
                                crate::window::window_markers::set_window_start_with_marker(
                                    buffers, window, clamped,
                                );
                            }
                        }
                    }
                }
                raw
            }
        }
    };
    // Run window-scroll-functions hook after setting window start
    let _ = builtin_run_window_scroll_functions(eval, vec![]);
    Ok(result)
}
/// `(set-window-group-start WINDOW POS &optional NOFORCE)` -> POS.
pub(crate) fn builtin_set_window_group_start(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("set-window-group-start", &args, 2)?;
    expect_max_args("set-window-group-start", &args, 3)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let pos = parse_integer_or_marker_arg(&args[1])?;
    let is_minibuffer = frames
        .get(fid)
        .is_some_and(|frame| frame.minibuffer_window == Some(wid));
    match pos {
        IntegerOrMarkerArg::Int(pos) => {
            if !is_minibuffer {
                if let Some(clamped) =
                    clamped_window_position_in_state(frames, buffers, fid, wid, pos)
                {
                    if let Some(window) = frames
                        .get_mut(fid)
                        .and_then(|frame| frame.find_window_mut(wid))
                    {
                        crate::window::window_markers::set_window_start_with_marker(
                            buffers, window, clamped,
                        );
                    }
                }
            }
            Ok(Value::fixnum(pos))
        }
        IntegerOrMarkerArg::Marker { raw, position } => {
            if !is_minibuffer {
                if let Some(pos) = position {
                    if let Some(clamped) =
                        clamped_window_position_in_state(frames, buffers, fid, wid, pos)
                    {
                        if let Some(window) = frames
                            .get_mut(fid)
                            .and_then(|frame| frame.find_window_mut(wid))
                        {
                            crate::window::window_markers::set_window_start_with_marker(
                                buffers, window, clamped,
                            );
                            crate::window::window_markers::set_window_point_with_marker(
                                buffers, window, clamped,
                            );
                        }
                    }
                }
            }
            Ok(raw)
        }
    }
}
/// `(set-window-point WINDOW POS)` -> POS.
pub(crate) fn builtin_set_window_point(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-point", &args, 2)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let pos = parse_integer_or_marker_arg(&args[1])?;
    let is_minibuffer = frames
        .get(fid)
        .is_some_and(|frame| frame.minibuffer_window == Some(wid));
    match pos {
        IntegerOrMarkerArg::Int(pos) => {
            if !is_minibuffer {
                if let Some(clamped) =
                    clamped_window_position_in_state(frames, buffers, fid, wid, pos)
                {
                    let selected_live_window = frames
                        .get(fid)
                        .is_some_and(|frame| frame.selected_window == wid);
                    let mut buffer_to_move = None;
                    if let Some(window) = frames
                        .get_mut(fid)
                        .and_then(|frame| frame.find_window_mut(wid))
                    {
                        let buffer_id = window.buffer_id();
                        crate::window::window_markers::set_window_point_with_marker(
                            buffers, window, clamped,
                        );
                        if selected_live_window {
                            if let Some(buffer_id) = buffer_id
                                && let Some(buffer) = buffers.get(buffer_id)
                            {
                                buffer_to_move =
                                    Some((buffer_id, buffer.lisp_pos_to_emacs_byte_pos(clamped)));
                            }
                        }
                    }
                    if let Some((buffer_id, byte_pos)) = buffer_to_move {
                        let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
                    }
                }
            }
            Ok(Value::fixnum(pos))
        }
        IntegerOrMarkerArg::Marker { raw, position } => {
            if is_minibuffer {
                return Ok(raw);
            }
            let pos = position.ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Marker does not point anywhere")],
                )
            })?;
            if let Some(clamped) = clamped_window_position_in_state(frames, buffers, fid, wid, pos)
            {
                let selected_live_window = frames
                    .get(fid)
                    .is_some_and(|frame| frame.selected_window == wid);
                let mut buffer_to_move = None;
                if let Some(window) = frames
                    .get_mut(fid)
                    .and_then(|frame| frame.find_window_mut(wid))
                {
                    let buffer_id = window.buffer_id();
                    crate::window::window_markers::set_window_point_with_marker(
                        buffers, window, clamped,
                    );
                    if selected_live_window {
                        if let Some(buffer_id) = buffer_id
                            && let Some(buffer) = buffers.get(buffer_id)
                        {
                            buffer_to_move =
                                Some((buffer_id, buffer.lisp_pos_to_emacs_byte_pos(clamped)));
                        }
                    }
                }
                if let Some((buffer_id, byte_pos)) = buffer_to_move {
                    let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
                }
                Ok(Value::fixnum(clamped.as_i64()))
            } else {
                Ok(Value::fixnum(1))
            }
        }
    }
}
/// `(window-use-time &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_use_time(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-use-time", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(Value::fixnum(frames.window_use_time(wid)))
}
/// `(window-bump-use-time &optional WINDOW)` -> integer or nil.
pub(crate) fn builtin_window_bump_use_time(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-bump-use-time", &args, 1)?;
    let selected_fid = ensure_selected_frame_id_in_state(frames, buffers);
    let selected_wid = frames
        .get(selected_fid)
        .map(|frame| frame.selected_window)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    let target_wid = if args.first().is_none_or(|v| v.is_nil()) {
        selected_wid
    } else {
        let val = args.first().unwrap();
        match val.kind() {
            ValueKind::Veclike(VecLikeType::Window) => {
                let raw_id = val.as_window_id().unwrap();
                let wid = WindowId(raw_id);
                if frames.find_window_frame_id(wid).is_none() {
                    return Err(signal(
                        "wrong-type-argument",
                        vec![Value::symbol("window-live-p"), Value::make_window(raw_id)],
                    ));
                }
                wid
            }
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("window-live-p"), *val],
                ));
            }
        }
    };
    Ok(
        match frames.bump_window_use_time(selected_wid, target_wid) {
            Some(use_time) => Value::fixnum(use_time),
            None => Value::NIL,
        },
    )
}
/// `(window-old-point &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_old_point(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-old-point", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    match w {
        Window::Leaf { old_point, .. } => {
            Ok(Value::fixnum((*old_point).max(LispCharPos1::ONE).as_i64()))
        }
        _ => Ok(Value::fixnum(1)),
    }
}
/// `(window-old-buffer &optional WINDOW)` -> nil in batch.
pub(crate) fn builtin_window_old_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-old-buffer", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, _wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(Value::NIL)
}
/// `(window-prev-buffers &optional WINDOW)` -> previous buffer list or nil.
pub(crate) fn builtin_window_prev_buffers(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-prev-buffers", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(frames.window_prev_buffers(wid))
}
/// `(window-next-buffers &optional WINDOW)` -> next buffer list or nil.
pub(crate) fn builtin_window_next_buffers(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-next-buffers", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(frames.window_next_buffers(wid))
}
/// `(set-window-prev-buffers WINDOW PREV-BUFFERS)` -> PREV-BUFFERS.
pub(crate) fn builtin_set_window_prev_buffers(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-prev-buffers", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let value = args[1];
    frames.set_window_prev_buffers(wid, value);
    Ok(value)
}
/// `(set-window-next-buffers WINDOW NEXT-BUFFERS)` -> NEXT-BUFFERS.
pub(crate) fn builtin_set_window_next_buffers(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-next-buffers", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let value = args[1];
    frames.set_window_next_buffers(wid, value);
    Ok(value)
}

/// `(window-discard-buffer-from-window BUFFER WINDOW &optional ALL)` -> nil.
pub(crate) fn builtin_window_discard_buffer_from_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("window-discard-buffer-from-window", &args, 2)?;
    expect_max_args("window-discard-buffer-from-window", &args, 3)?;
    let buffer_id = match args.first().and_then(|v| v.as_buffer_id()) {
        Some(bid) if buffers.get(bid).is_some() => bid,
        _ => {
            return Err(signal("error", vec![Value::string("Not a live buffer")]));
        }
    };
    let wid = match args.get(1).and_then(window_id_from_designator) {
        Some(wid) if frames.is_live_window_id(wid) => wid,
        _ => return Err(signal("error", vec![Value::string("Not a live window")])),
    };
    discard_buffers_from_window_history(frames, wid, &[Value::make_buffer(buffer_id)])?;
    Ok(Value::NIL)
}

/// `(combine-windows FIRST LAST)` -> nil or a new internal parent window.
///
/// GNU `Fcombine_windows` starts by decoding both arguments with
/// `decode_valid_window`, so nil defaults to the selected window and
/// non-window values signal `window-valid-p`.
pub(crate) fn builtin_combine_windows(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("combine-windows", &args, 2)?;
    let (_first_fid, first_wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let (_last_fid, last_wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.get(1), "window-valid-p")?;

    if first_wid == last_wid {
        return Err(signal(
            "error",
            vec![Value::string("Cannot combine a window with itself")],
        ));
    }

    Ok(Value::NIL)
}

/// `(uncombine-window WINDOW)` -> t if WINDOW was flattened, else nil.
///
/// GNU `Funcombine_window` validates with `decode_valid_window` before testing
/// whether WINDOW is an internal combination of the same direction as its
/// parent.
pub(crate) fn builtin_uncombine_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("uncombine-window", &args, 1)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;

    if frames
        .get(fid)
        .is_some_and(|frame| frame.minibuffer_window == Some(wid))
    {
        return Err(signal(
            "error",
            vec![Value::string("Cannot uncombine a mini window")],
        ));
    }

    Ok(Value::NIL)
}

/// `(window-left-column &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_left_column(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-left-column", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    let cw = frames.get(fid).map(|f| f.char_width).unwrap_or(8.0);
    let left = if cw > 0.0 {
        (w.bounds().x / cw) as i64
    } else {
        0
    };
    Ok(Value::fixnum(left))
}
/// `(window-top-line &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_top_line(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-top-line", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    let ch = frames.get(fid).map(|f| f.char_height).unwrap_or(16.0);
    let top = if ch > 0.0 {
        (w.bounds().y / ch) as i64
    } else {
        0
    };
    Ok(Value::fixnum(top))
}
/// `(window-pixel-left &optional WINDOW)` -> integer.
///
/// In batch-mode GNU Emacs, these "pixel" helpers report character-cell units.
pub(crate) fn builtin_window_pixel_left(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-pixel-left", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    let cw = frames.get(fid).map(|f| f.char_width).unwrap_or(8.0);
    let left = if cw > 0.0 {
        (w.bounds().x / cw) as i64
    } else {
        0
    };
    Ok(Value::fixnum(left))
}
/// `(window-pixel-top &optional WINDOW)` -> integer.
///
/// In batch-mode GNU Emacs, these "pixel" helpers report character-cell units.
pub(crate) fn builtin_window_pixel_top(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-pixel-top", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    let ch = frames.get(fid).map(|f| f.char_height).unwrap_or(16.0);
    let top = if ch > 0.0 {
        (w.bounds().y / ch) as i64
    } else {
        0
    };
    Ok(Value::fixnum(top))
}
/// `(window-hscroll &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_hscroll(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-hscroll", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    match w {
        Window::Leaf { hscroll, .. } => Ok(Value::fixnum(*hscroll as i64)),
        _ => Ok(Value::fixnum(0)),
    }
}
/// `(set-window-hscroll WINDOW NCOLS)` -> integer.
pub(crate) fn builtin_set_window_hscroll(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-hscroll", &args, 2)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let cols = expect_fixnum(&args[1])?.max(0) as usize;
    if let Some(Window::Leaf { hscroll, .. }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        *hscroll = cols;
    }
    Ok(Value::fixnum(cols as i64))
}

fn scroll_prefix_value(value: &Value) -> i64 {
    crate::emacs_core::prefix::prefix_numeric_value(value)
}

fn default_scroll_columns(eval: &super::eval::Context, fid: FrameId, wid: WindowId) -> i64 {
    default_scroll_columns_in_state(&eval.frames, fid, wid)
}

fn default_scroll_columns_in_state(frames: &FrameManager, fid: FrameId, wid: WindowId) -> i64 {
    let char_width = frames.get(fid).map(|f| f.char_width).unwrap_or(8.0);
    let window_cols = get_leaf(frames, fid, wid)
        .ok()
        .map(|leaf| {
            if char_width > 0.0 {
                (leaf.bounds().width / char_width).floor() as i64
            } else {
                80
            }
        })
        .unwrap_or(80);
    (window_cols - 2).max(1)
}
/// `(scroll-left &optional SET-MINIMUM ARG)` -> new horizontal scroll amount.
pub(crate) fn builtin_scroll_left(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("scroll-left", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) = resolve_window_id_in_state(frames, buffers, None)?;
    let base = frames
        .get(fid)
        .and_then(|frame| frame.find_window(wid))
        .and_then(|window| match window {
            Window::Leaf { hscroll, .. } => Some(*hscroll as i64),
            _ => None,
        })
        .unwrap_or(0);
    let delta = if args.first().is_none_or(|v| v.is_nil()) {
        default_scroll_columns_in_state(frames, fid, wid)
    } else {
        scroll_prefix_value(args.first().unwrap())
    };
    let mut next = base as i128 + delta as i128;
    if next < 0 {
        next = 0;
    }
    let next = next.min(i64::MAX as i128) as i64;
    if let Some(Window::Leaf { hscroll, .. }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        *hscroll = next as usize;
    }
    Ok(Value::fixnum(next))
}
/// `(scroll-right &optional SET-MINIMUM ARG)` -> new horizontal scroll amount.
pub(crate) fn builtin_scroll_right(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("scroll-right", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) = resolve_window_id_in_state(frames, buffers, None)?;
    let base = frames
        .get(fid)
        .and_then(|frame| frame.find_window(wid))
        .and_then(|window| match window {
            Window::Leaf { hscroll, .. } => Some(*hscroll as i64),
            _ => None,
        })
        .unwrap_or(0);
    let delta = if args.first().is_none_or(|v| v.is_nil()) {
        default_scroll_columns_in_state(frames, fid, wid)
    } else {
        scroll_prefix_value(args.first().unwrap())
    };
    let mut next = base as i128 - delta as i128;
    if next < 0 {
        next = 0;
    }
    let next = next.min(i64::MAX as i128) as i64;
    if let Some(Window::Leaf { hscroll, .. }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        *hscroll = next as usize;
    }
    Ok(Value::fixnum(next))
}
/// `(window-vscroll &optional WINDOW PIXELWISE)` -> number.
///
/// GNU stores vertical scroll on each window in pixels. Batch-mode windows
/// report zero; GUI windows report either pixels or canonical line units.
pub(crate) fn builtin_window_vscroll(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-vscroll", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let pixelwise = args.get(1).is_some_and(|v| v.is_truthy());
    Ok(frames
        .window_vscroll(wid, pixelwise)
        .unwrap_or(Value::fixnum(0)))
}
/// `(set-window-vscroll WINDOW VSCROLL &optional PIXELWISE PRESERVE)` -> number.
pub(crate) fn builtin_set_window_vscroll(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("set-window-vscroll", &args, 2)?;
    expect_max_args("set-window-vscroll", &args, 4)?;
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let next_vscroll = expect_number(&args[1])?;
    let pixelwise = args.get(2).is_some_and(|v| v.is_truthy());
    let preserve = args.get(3).is_some_and(|v| v.is_truthy());
    Ok(frames
        .set_window_vscroll(wid, next_vscroll, pixelwise, preserve)
        .unwrap_or(Value::fixnum(0)))
}
/// `(set-window-margins WINDOW LEFT-WIDTH &optional RIGHT-WIDTH)` -> changed-p.
pub(crate) fn builtin_set_window_margins(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("set-window-margins", &args, 2)?;
    expect_max_args("set-window-margins", &args, 3)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let left = expect_margin_width(&args[1])?;
    let right = if let Some(arg) = args.get(2) {
        expect_margin_width(arg)?
    } else {
        0
    };

    if let Some(Window::Leaf { margins, .. }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        let next = WindowMargins::new(left, right);
        if *margins != next {
            *margins = next;
            return Ok(Value::T);
        }
    }
    Ok(Value::NIL)
}
/// `(window-margins &optional WINDOW)` -> margins pair or nil.
pub(crate) fn builtin_window_margins(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-margins", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    let margins = match w {
        Window::Leaf { margins, .. } => *margins,
        _ => WindowMargins::ZERO,
    };
    let left = margins.left();
    let right = margins.right();
    let left_v = if left == 0 {
        Value::NIL
    } else {
        Value::fixnum(left as i64)
    };
    let right_v = if right == 0 {
        Value::NIL
    } else {
        Value::fixnum(right as i64)
    };
    Ok(Value::cons(left_v, right_v))
}
/// `(window-fringes &optional WINDOW)` -> fringe tuple.
pub(crate) fn builtin_window_fringes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-fringes", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let (left, right, outside, persistent) =
        frames.window_fringes(wid).unwrap_or((0, 0, false, false));
    Ok(Value::list(vec![
        Value::fixnum(left),
        Value::fixnum(right),
        if outside { Value::T } else { Value::NIL },
        if persistent { Value::T } else { Value::NIL },
    ]))
}
/// `(set-window-fringes WINDOW LEFT &optional RIGHT OUTSIDE-MARGINS PERSISTENT)` -> nil.
pub(crate) fn builtin_set_window_fringes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("set-window-fringes", &args, 2)?;
    expect_max_args("set-window-fringes", &args, 5)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if frames
        .get(fid)
        .is_none_or(|frame| frame.effective_window_system().is_none())
    {
        return Ok(Value::NIL);
    }
    let left = if args[1].is_nil() {
        None
    } else {
        Some(i32::try_from(expect_int(&args[1])?).map_err(|_| {
            signal(
                "args-out-of-range",
                vec![
                    args[1],
                    Value::fixnum(0),
                    Value::fixnum(i64::from(i32::MAX)),
                ],
            )
        })?)
    };
    let right = if let Some(arg) = args.get(2) {
        if arg.is_nil() {
            None
        } else {
            Some(i32::try_from(expect_int(arg)?).map_err(|_| {
                signal(
                    "args-out-of-range",
                    vec![*arg, Value::fixnum(0), Value::fixnum(i64::from(i32::MAX))],
                )
            })?)
        }
    } else {
        left
    };
    Ok(
        if frames.set_window_fringes(
            wid,
            left,
            right,
            args.get(3).is_some_and(|value| value.is_truthy()),
            args.get(4).is_some_and(|value| value.is_truthy()),
        ) {
            Value::T
        } else {
            Value::NIL
        },
    )
}
/// `(window-scroll-bars &optional WINDOW)` -> scroll-bar tuple.
pub(crate) fn builtin_window_scroll_bars(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-scroll-bars", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let (width, columns, vertical_type, height, lines, horizontal_type, persistent) = frames
        .window_scroll_bars(wid)
        .unwrap_or((Value::NIL, 0, Value::T, Value::NIL, 0, Value::T, false));
    Ok(Value::list(vec![
        width,
        Value::fixnum(columns),
        vertical_type,
        height,
        Value::fixnum(lines),
        horizontal_type,
        if persistent { Value::T } else { Value::NIL },
    ]))
}
/// `(set-window-scroll-bars WINDOW &optional WIDTH VERTICAL-TYPE HEIGHT HORIZONTAL-TYPE)` -> nil.
pub(crate) fn builtin_set_window_scroll_bars(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("set-window-scroll-bars", &args, 1)?;
    expect_max_args("set-window-scroll-bars", &args, 6)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if frames
        .get(fid)
        .is_none_or(|frame| frame.effective_window_system().is_none())
    {
        return Ok(Value::NIL);
    }
    let width = if let Some(arg) = args.get(1) {
        if arg.is_nil() {
            None
        } else {
            Some(i32::try_from(expect_int(arg)?).map_err(|_| {
                signal(
                    "args-out-of-range",
                    vec![*arg, Value::fixnum(0), Value::fixnum(i64::from(i32::MAX))],
                )
            })?)
        }
    } else {
        None
    };
    let vertical_type = args.get(2).copied().unwrap_or(Value::T);
    if !valid_vertical_scroll_bar_type(vertical_type) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid type of vertical scroll bar")],
        ));
    }
    let height = if let Some(arg) = args.get(3) {
        if arg.is_nil() {
            None
        } else {
            Some(i32::try_from(expect_int(arg)?).map_err(|_| {
                signal(
                    "args-out-of-range",
                    vec![*arg, Value::fixnum(0), Value::fixnum(i64::from(i32::MAX))],
                )
            })?)
        }
    } else {
        None
    };
    let horizontal_type = args.get(4).copied().unwrap_or(Value::T);
    if !valid_horizontal_scroll_bar_type(horizontal_type) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid type of horizontal scroll bar")],
        ));
    }
    Ok(
        if frames.set_window_scroll_bars(
            wid,
            width,
            vertical_type,
            height,
            horizontal_type,
            args.get(5).is_some_and(|value| value.is_truthy()),
        ) {
            Value::T
        } else {
            Value::NIL
        },
    )
}

/// `(window-scroll-bar-width &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_scroll_bar_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-scroll-bar-width", &args, 1)?;
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(Value::fixnum(frames.window_scroll_bar_area_width(wid)))
}

/// `(window-scroll-bar-height &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_scroll_bar_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-scroll-bar-height", &args, 1)?;
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (_fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(Value::fixnum(frames.window_scroll_bar_area_height(wid)))
}
/// `(window-mode-line-height &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_mode_line_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-mode-line-height", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let height = window_chrome_height_in_state(
        frames,
        fid,
        wid,
        WindowChromeMetric::ModeLine,
        if is_minibuffer_window(frames, fid, wid) {
            0
        } else {
            1
        },
    );
    Ok(Value::fixnum(height))
}
/// `(window-header-line-height &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_header_line_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-header-line-height", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(Value::fixnum(window_chrome_height_in_state(
        frames,
        fid,
        wid,
        WindowChromeMetric::HeaderLine,
        0,
    )))
}
/// `(window-tab-line-height &optional WINDOW)` -> integer.
pub(crate) fn builtin_window_tab_line_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-tab-line-height", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    Ok(Value::fixnum(window_chrome_height_in_state(
        frames,
        fid,
        wid,
        WindowChromeMetric::TabLine,
        0,
    )))
}

#[derive(Clone, Copy)]
enum WindowChromeMetric {
    ModeLine,
    HeaderLine,
    TabLine,
}

fn window_chrome_height_in_state(
    frames: &FrameManager,
    fid: FrameId,
    wid: WindowId,
    metric: WindowChromeMetric,
    fallback: i64,
) -> i64 {
    frames
        .get(fid)
        .and_then(|frame| frame.window_display_snapshot(wid))
        .map(|snapshot| match metric {
            WindowChromeMetric::ModeLine => snapshot.mode_line_height,
            WindowChromeMetric::HeaderLine => snapshot.header_line_height,
            WindowChromeMetric::TabLine => snapshot.tab_line_height,
        })
        .unwrap_or(fallback)
        .max(0)
}
/// `(window-pixel-height &optional WINDOW)` -> integer.
///
/// In batch-mode GNU Emacs, these "pixel" helpers report character-cell units.
pub(crate) fn builtin_window_pixel_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-pixel-height", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    Ok(Value::fixnum(window_height_pixels(w)))
}
/// `(window-pixel-width &optional WINDOW)` -> integer.
///
/// In batch-mode GNU Emacs, these "pixel" helpers report character-cell units.
pub(crate) fn builtin_window_pixel_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-pixel-width", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    Ok(Value::fixnum(window_width_pixels(w)))
}
/// `(window-body-height &optional WINDOW PIXELWISE)` -> integer.
///
/// Returns the body height of WINDOW. When PIXELWISE is non-nil,
/// return pixels; otherwise return character lines.
/// Body excludes mode-line (one row) for non-minibuffer windows.
pub(crate) fn builtin_window_body_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    window_body_height_impl(&mut eval.frames, &mut eval.buffers, args)
}

fn window_body_height_impl(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-body-height", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    let pixelwise = args.get(1).is_some_and(|v| v.is_truthy());
    if pixelwise {
        let total = window_height_pixels(w);
        let body = if is_minibuffer_window(frames, fid, wid) {
            total
        } else {
            let mode_line_height = frames
                .get(fid)
                .map(|frame| frame.char_height.max(0.0) as i64)
                .unwrap_or(0);
            total.saturating_sub(mode_line_height)
        };
        Ok(Value::fixnum(body))
    } else {
        let body_lines = window_body_height_lines(frames, fid, wid, w);
        Ok(Value::fixnum(body_lines))
    }
}
/// `(window-body-width &optional WINDOW PIXELWISE)` -> integer.
///
/// Returns the body width of WINDOW. When PIXELWISE is non-nil,
/// return pixels; otherwise return character columns.
pub(crate) fn builtin_window_body_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-body-width", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    let pixelwise = args.get(1).is_some_and(|v| v.is_truthy());
    if pixelwise {
        Ok(Value::fixnum(window_body_width_pixels(frames, fid, w)))
    } else {
        let cw = frames
            .get(fid)
            .map(|f| f.char_width.max(1.0))
            .unwrap_or(8.0);
        Ok(Value::fixnum(
            (window_body_width_pixels(frames, fid, w) as f32 / cw).floor() as i64,
        ))
    }
}
/// `(window-text-height &optional WINDOW PIXELWISE)` -> integer.
pub(crate) fn builtin_window_text_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-text-height", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    let pixelwise = args.get(1).is_some_and(|v| v.is_truthy());
    if pixelwise {
        let total = window_height_pixels(w);
        let body = if is_minibuffer_window(frames, fid, wid) {
            total
        } else {
            let mode_line_height = frames
                .get(fid)
                .map(|frame| frame.char_height.max(0.0) as i64)
                .unwrap_or(0);
            total.saturating_sub(mode_line_height)
        };
        Ok(Value::fixnum(body))
    } else {
        Ok(Value::fixnum(window_body_height_lines(frames, fid, wid, w)))
    }
}
/// `(window-text-width &optional WINDOW PIXELWISE)` -> integer.
pub(crate) fn builtin_window_text_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-text-width", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    let pixelwise = args.get(1).is_some_and(|v| v.is_truthy());
    if pixelwise {
        Ok(Value::fixnum(window_body_width_pixels(frames, fid, w)))
    } else {
        let cw = frames
            .get(fid)
            .map(|f| f.char_width.max(1.0))
            .unwrap_or(8.0);
        Ok(Value::fixnum(
            (window_body_width_pixels(frames, fid, w) as f32 / cw).floor() as i64,
        ))
    }
}
/// `(window-edges &optional WINDOW BODY ABSOLUTE PIXELWISE)`.
///
/// GNU Emacs returns (LEFT TOP RIGHT BOTTOM) edges of WINDOW.
/// When PIXELWISE is non-nil, return pixel coordinates instead of
/// character-cell units.  When BODY is non-nil, return body edges
/// (excluding mode-line).
pub(crate) fn builtin_window_edges(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-edges", &args, 4)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let body = args.get(1).is_some_and(|v| v.is_truthy());
    let _absolute = args.get(2).is_some_and(|v| v.is_truthy());
    let pixelwise = args.get(3).is_some_and(|v| v.is_truthy());
    let live_only = body;
    let (fid, wid) =
        resolve_window_id_or_window_error_in_state(frames, buffers, args.first(), live_only)?;
    let w = get_window(frames, fid, wid)?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    if pixelwise {
        let (left, top, right, bottom) = if body {
            window_body_edges_pixels(frames, fid, wid, w)
        } else {
            window_edges_pixels(w)
        };
        return Ok(Value::list(vec![
            Value::fixnum(left),
            Value::fixnum(top),
            Value::fixnum(right),
            Value::fixnum(bottom),
        ]));
    }

    let (left, top, right, bottom) = if body {
        window_body_edges_cols_lines(frames, fid, wid, w, frame.char_width, frame.char_height)
    } else {
        window_edges_cols_lines(w, frame.char_width, frame.char_height)
    };
    Ok(Value::list(vec![
        Value::fixnum(left),
        Value::fixnum(top),
        Value::fixnum(right),
        Value::fixnum(bottom),
    ]))
}
/// `(window-pixel-edges &optional WINDOW)` -> (LEFT TOP RIGHT BOTTOM) in pixels.
pub(crate) fn builtin_window_pixel_edges(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-pixel-edges", &args, 1)?;
    let (fid, wid) = resolve_window_id(eval, args.first())?;
    let w = eval.frames.get(fid).and_then(|f| f.find_window(wid));
    let Some(w) = w else {
        return Ok(Value::NIL);
    };
    let (left, top, right, bottom) = window_edges_pixels(w);
    Ok(Value::list(vec![
        Value::fixnum(left),
        Value::fixnum(top),
        Value::fixnum(right),
        Value::fixnum(bottom),
    ]))
}

/// `(window-absolute-pixel-edges &optional WINDOW)` -> (LEFT TOP RIGHT BOTTOM).
/// Same as pixel-edges for NeoVM (frames don't have screen offset tracking).
pub(crate) fn builtin_window_absolute_pixel_edges(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_window_pixel_edges(eval, args)
}

/// `(window-total-height &optional WINDOW ROUND)` -> integer.
///
/// Works for both leaf and internal windows, matching GNU Emacs.
pub(crate) fn builtin_window_total_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    window_total_height_impl(&mut eval.frames, &mut eval.buffers, args)
}

pub(crate) fn window_total_height_impl(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-total-height", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    let ch = frames.get(fid).map(|f| f.char_height).unwrap_or(16.0);
    Ok(Value::fixnum(window_height_lines(w, ch)))
}
/// `(window-total-width &optional WINDOW ROUND)` -> integer.
///
/// Works for both leaf and internal windows, matching GNU Emacs.
pub(crate) fn builtin_window_total_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    window_total_width_impl(&mut eval.frames, &mut eval.buffers, args)
}

pub(crate) fn window_total_width_impl(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-total-width", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    let cw = frames.get(fid).map(|f| f.char_width).unwrap_or(8.0);
    Ok(Value::fixnum(window_width_cols(w, cw)))
}
/// `(window-list &optional FRAME MINIBUF WINDOW)` -> list of window objects.
pub(crate) fn builtin_window_list(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-list", &args, 3)?;
    let selected_fid = ensure_selected_frame_id_in_state(frames, buffers);
    // GNU validates WINDOW before FRAME mismatch checks.
    let requested_start_window = if args.get(2).is_none_or(|v| v.is_nil()) {
        None
    } else {
        let arg = args.get(2).unwrap();
        let Some(wid) = window_id_from_designator(arg) else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("windowp"), *arg],
            ));
        };
        if let Some(window_fid) = frames.find_window_frame_id(wid) {
            Some((wid, window_fid))
        } else if frames.is_window_object_id(wid) {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("window-live-p"), *arg],
            ));
        } else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("windowp"), *arg],
            ));
        }
    };
    let fid = if args.first().is_none_or(|v| v.is_nil()) {
        selected_fid
    } else {
        let val = args.first().unwrap();
        match val.kind() {
            ValueKind::Fixnum(n) => {
                let fid = FrameId(n as u64);
                if frames.get(fid).is_some() {
                    fid
                } else {
                    return Err(signal(
                        "error",
                        vec![Value::string("Window is on a different frame")],
                    ));
                }
            }
            ValueKind::Veclike(VecLikeType::Frame) => {
                let raw_id = val.as_frame_id().unwrap();
                let fid = FrameId(raw_id);
                if frames.get(fid).is_some() {
                    fid
                } else {
                    return Err(signal(
                        "error",
                        vec![Value::string("Window is on a different frame")],
                    ));
                }
            }
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string("Window is on a different frame")],
                ));
            }
        }
    };
    let include_minibuffer = args.get(1).is_some_and(|v| *v == Value::T);
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let start_wid = if let Some((wid, window_fid)) = requested_start_window {
        if window_fid != fid {
            return Err(signal(
                "error",
                vec![Value::string("Window is on a different frame")],
            ));
        }
        wid
    } else {
        frame.selected_window
    };
    let mut window_ids = frame.window_list();
    if let Some(pos) = window_ids.iter().position(|wid| *wid == start_wid) {
        window_ids.rotate_left(pos);
    }
    let mut ids: Vec<Value> = window_ids.into_iter().map(window_value).collect();
    if include_minibuffer {
        if let Some(minibuffer_wid) = frame.minibuffer_window {
            ids.push(window_value(minibuffer_wid));
        }
    }
    Ok(Value::list(ids))
}
/// `(window-list-1 &optional WINDOW MINIBUF ALL-FRAMES)` -> list of live windows.
pub(crate) fn builtin_window_list_1(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let active_minibuffer_window = active_minibuffer_window_id(eval);
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-list-1", &args, 3)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, start_wid) = if args.first().is_none_or(|v| v.is_nil()) {
        resolve_window_id_with_pred_in_state(frames, buffers, None, "window-live-p")?
    } else {
        let val = args.first().unwrap();
        if let Some(raw_id) = val.as_window_id() {
            let wid = WindowId(raw_id);
            if let Some(fid) = frames.find_window_frame_id(wid) {
                (fid, wid)
            } else {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("window-live-p"), args[0]],
                ));
            }
        } else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("window-live-p"), *val],
            ));
        }
    };

    let scope = decode_all_frames_scope(frames, args.get(2).copied())?;
    let mut frame_ids = frame_ids_for_all_frames_scope(frames, fid, scope);
    if frame_ids.is_empty() {
        frame_ids.push(fid);
    }

    #[derive(Clone, Copy)]
    enum MinibufferListMode {
        None,
        Active(WindowId),
        All,
    }

    let minibuffer_list_mode = match args.get(1).copied() {
        Some(value) if value == Value::T => MinibufferListMode::All,
        Some(value) if !value.is_nil() => MinibufferListMode::None,
        _ => active_minibuffer_window
            .map(MinibufferListMode::Active)
            .unwrap_or(MinibufferListMode::None),
    };
    let mut seen_window_ids: HashSet<u64> = HashSet::new();
    let mut windows: Vec<Value> = Vec::new();

    for frame_id in frame_ids {
        let Some(frame) = frames.get(frame_id) else {
            continue;
        };

        // GNU Emacs starts traversal at WINDOW when it appears in the returned list.
        let mut window_ids = frame.window_list();
        if frame_id == fid {
            if let Some(start_index) = window_ids.iter().position(|wid| *wid == start_wid) {
                window_ids.rotate_left(start_index);
            }
        }

        for window_id in window_ids {
            if seen_window_ids.insert(window_id.0) {
                windows.push(window_value(window_id));
            }
        }

        let minibuffer_wid = match minibuffer_list_mode {
            MinibufferListMode::None => None,
            MinibufferListMode::Active(wid) => {
                (frame.minibuffer_window == Some(wid)).then_some(wid)
            }
            MinibufferListMode::All => frame.minibuffer_window,
        };
        if let Some(minibuffer_wid) = minibuffer_wid
            && seen_window_ids.insert(minibuffer_wid.0)
        {
            windows.push(window_value(minibuffer_wid));
        }
    }

    Ok(Value::list(windows))
}

/// `(get-buffer-window &optional BUFFER-OR-NAME ALL-FRAMES)` -> window or nil.
///
/// Search the GNU `ALL-FRAMES` scope for a window showing the requested buffer.
pub(crate) fn builtin_get_buffer_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("get-buffer-window", &args, 2)?;
    let fid = ensure_selected_frame_id(eval);
    let target = match args.first().copied() {
        Some(value) if !value.is_nil() => match value.kind() {
            ValueKind::String => match find_buffer_by_name_arg(&eval.buffers, &value)? {
                Some(id) => id,
                None => return Ok(Value::NIL),
            },
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = value.as_buffer_id().unwrap();
                if eval.buffers.get(bid).is_none() {
                    return Ok(Value::NIL);
                }
                bid
            }
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), value],
                ));
            }
        },
        _ => match eval.buffers.current_buffer_id() {
            Some(buffer_id) => buffer_id,
            None => return Ok(Value::NIL),
        },
    };
    let scope = decode_all_frames_scope(&eval.frames, args.get(1).copied())?;
    for frame_id in frame_ids_for_all_frames_scope(&eval.frames, fid, scope) {
        let Some(frame) = eval.frames.get(frame_id) else {
            continue;
        };
        for wid in frame.window_list() {
            let matches = frame
                .find_window(wid)
                .and_then(|w| w.buffer_id())
                .is_some_and(|bid| bid == target);
            if matches {
                return Ok(window_value(wid));
            }
        }
    }

    Ok(Value::NIL)
}
/// `(window-dedicated-p &optional WINDOW)` -> t or nil.
pub(crate) fn builtin_window_dedicated_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-dedicated-p", &args, 1)?;
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    let w = get_leaf(frames, fid, wid)?;
    match w {
        Window::Leaf { dedicated, .. } => Ok(dedicated.clone()),
        _ => Ok(Value::NIL),
    }
}
/// `(set-window-dedicated-p WINDOW FLAG)` -> FLAG.
pub(crate) fn builtin_set_window_dedicated_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-dedicated-p", &args, 2)?;
    let flag = args[1].clone();
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if let Some(w) = frames.get_mut(fid).and_then(|f| f.find_window_mut(wid)) {
        if let Window::Leaf { dedicated, .. } = w {
            *dedicated = flag.clone();
        }
    }
    Ok(flag)
}
/// `(windowp OBJ)` -> t if OBJ is a window object/designator that exists.
///
/// GNU `src/window.c::Fwindowp` is a pure type check on the
/// Lisp value: `WINDOWP(obj)` checks the tag of the boxed Lisp
/// object and returns immediately. neomacs walks the live frame
/// manager because windows are stored as `WindowId(u64)` rather
/// than as a tagged Lisp value, which means a window object that
/// exists in the obarray but not in any frame's window tree
/// returns `nil` here. Window audit Critical 6 in
/// `drafts/window-system-audit.md` tracks adding a
/// `VecLikeType::Window` so this becomes a tag check.
///
/// The semantic difference is observable in tests that hold a
/// `Value` reference to a window, delete it, and then call
/// `windowp` on the dangling reference. GNU returns `t` (it's
/// still a window value, just not live); neomacs returns `nil`.
/// `window-valid-p` and `window-live-p` correctly already test
/// for liveness, so the divergence is restricted to the
/// "exists at all" boundary that `windowp` is supposed to
/// answer.
pub(crate) fn builtin_windowp(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let frames = &eval.frames;
    expect_args("windowp", &args, 1)?;
    let wid = match window_id_from_designator(&args[0]) {
        Some(wid) => wid,
        None => return Ok(Value::NIL),
    };
    Ok(Value::bool_val(frames.is_window_object_id(wid)))
}
/// `(window-valid-p OBJ)` -> t if OBJ is a live window.
pub(crate) fn builtin_window_valid_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let frames = &eval.frames;
    expect_args("window-valid-p", &args, 1)?;
    let wid = match window_id_from_designator(&args[0]) {
        Some(wid) => wid,
        None => return Ok(Value::NIL),
    };
    Ok(Value::bool_val(frames.is_valid_window_id(wid)))
}
/// `(window-live-p OBJ)` -> t if OBJ is a live leaf window.
pub(crate) fn builtin_window_live_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let frames = &eval.frames;
    expect_args("window-live-p", &args, 1)?;
    let wid = match window_id_from_designator(&args[0]) {
        Some(wid) => wid,
        None => return Ok(Value::NIL),
    };
    Ok(Value::bool_val(frames.is_live_window_id(wid)))
}
/// `(window-at X Y &optional FRAME)` -> window object or nil.
pub(crate) fn builtin_window_at(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("window-at", &args, 2)?;
    expect_max_args("window-at", &args, 3)?;
    let x = expect_number(&args[0])?;
    let y = expect_number(&args[1])?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.get(2), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let total_cols = frame_total_cols(frame) as f64;
    let total_lines = frame_total_lines(frame) as f64;
    if x < 0.0 || y < 0.0 || x >= total_cols || y >= total_lines {
        return Ok(Value::NIL);
    }

    let px = (x * frame.char_width as f64) as f32;
    let py = (y * frame.char_height as f64) as f32;
    if let Some(wid) = frame.window_at(px, py) {
        return Ok(window_value(wid));
    }

    if let (Some(minibuffer_wid), Some(minibuffer_leaf)) =
        (frame.minibuffer_window, frame.minibuffer_leaf.as_ref())
    {
        if minibuffer_leaf.bounds().contains(px, py) {
            return Ok(window_value(minibuffer_wid));
        }
    }

    Ok(Value::NIL)
}

// ===========================================================================
// Window manipulation
// ===========================================================================

pub(crate) fn split_window_internal_impl(
    eval: &mut super::eval::Context,
    window: Value,
    size: Value,
    side: Value,
) -> EvalResult {
    split_window_internal_impl_in_state(&mut eval.frames, &mut eval.buffers, window, size, side)
}

pub(crate) fn split_window_internal_impl_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    window: Value,
    size: Value,
    side: Value,
) -> EvalResult {
    split_window_internal_impl_in_state_with_normal(frames, buffers, window, size, side, Value::NIL)
}

/// Variant of [`split_window_internal_impl_in_state`] that also
/// honors the NORMAL-SIZE argument from `split-window-internal`.
///
/// Mirrors GNU `src/window.c::Fsplit_window_internal` (lines
/// 5374-5644). The fourth argument NORMAL-SIZE seeds the new
/// sibling's `normal_lines` (vertical split) or `normal_cols`
/// (horizontal split), overriding the auto-computed fraction
/// from the split bounds. Audit Critical 5 in
/// `drafts/window-system-audit.md`.
pub(crate) fn split_window_internal_impl_in_state_with_normal(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    window: Value,
    size: Value,
    side: Value,
    normal_size: Value,
) -> EvalResult {
    let (fid, wid) = resolve_window_id_or_error_in_state(frames, buffers, Some(&window))?;

    // GNU `Fsplit_window_internal` treats SIDE t as `right`, nil as
    // `below`, and unknown symbols like the vertical/default side.
    let side_kind = SplitWindowSide::from_lisp_value(&side).unwrap_or(SplitWindowSide::Below);
    let direction = if side_kind.is_horizontal() {
        SplitDirection::Horizontal
    } else {
        SplitDirection::Vertical
    };
    let placement = match side_kind {
        SplitWindowSide::Above | SplitWindowSide::Left => SplitPlacement::BeforeTarget,
        SplitWindowSide::Below | SplitWindowSide::Right => SplitPlacement::AfterTarget,
    };

    // Parse SIZE: positive means new window gets SIZE units, negative means
    // old window keeps |SIZE| units, nil/0 means 50/50.
    let size_opt: Option<i64> = match size.kind() {
        ValueKind::Fixnum(n) if n != 0 => Some(n),
        _ => None,
    };

    // Use the same buffer as the window being split.
    let buf_id = {
        let w = get_window(frames, fid, wid)?;
        if let Some(buffer_id) = w.buffer_id() {
            buffer_id
        } else {
            frames
                .get(fid)
                .and_then(|frame| frame.find_window(frame.selected_window))
                .and_then(Window::buffer_id)
                .unwrap_or(BufferId(0))
        }
    };

    let new_wid = frames
        .split_window(fid, wid, direction, buf_id, size_opt, placement)
        .ok_or_else(|| signal("error", vec![Value::string("Cannot split window")]))?;

    // GNU `Fsplit_window_internal` (`src/window.c:5517-5644`)
    // assigns `wset_normal_*` for the new sibling from the
    // NORMAL-SIZE argument when supplied. The corresponding
    // sibling fraction on the OTHER child is `1.0 - normal`,
    // matching what GNU computes for the rebalanced parent.
    if !normal_size.is_nil() {
        let normal_f = match normal_size.kind() {
            ValueKind::Float => normal_size.as_float().unwrap_or(0.5),
            ValueKind::Fixnum(n) => n as f64,
            _ => 0.5,
        };
        let other_f = (1.0 - normal_f).clamp(0.0, 1.0);
        if let Some(frame) = frames.get_mut(fid) {
            if let Some(new_window) = frame.find_window_mut(new_wid) {
                match direction {
                    SplitDirection::Horizontal => {
                        new_window.set_normal_cols(Value::make_float(normal_f));
                    }
                    SplitDirection::Vertical => {
                        new_window.set_normal_lines(Value::make_float(normal_f));
                    }
                }
            }
            if let Some(old_window) = frame.find_window_mut(wid) {
                match direction {
                    SplitDirection::Horizontal => {
                        old_window.set_normal_cols(Value::make_float(other_f));
                    }
                    SplitDirection::Vertical => {
                        old_window.set_normal_lines(Value::make_float(other_f));
                    }
                }
            }
        }
    }

    Ok(window_value(new_wid))
}
/// `(delete-window &optional WINDOW)` -> nil.
pub(crate) fn builtin_delete_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("delete-window", &args, 1)?;
    let (fid, wid) =
        resolve_window_id_or_error_in_state(&mut eval.frames, &mut eval.buffers, args.first())?;
    if !eval.frames.delete_window(fid, wid) {
        return Err(signal(
            "error",
            vec![Value::string("Cannot delete sole window")],
        ));
    }
    let selected_buffer = eval
        .frames
        .get(fid)
        .and_then(|frame| frame.find_window(frame.selected_window))
        .and_then(|w| w.buffer_id());
    if let Some(buffer_id) = selected_buffer {
        eval.buffers.switch_current(buffer_id);
    }
    note_selected_window_buffer_in_state(&mut eval.frames, &mut eval.buffers, fid);
    // Run window-configuration-change-hook after successful deletion.
    let _ = builtin_run_window_configuration_change_hook(eval, vec![]);
    Ok(Value::NIL)
}
/// `(delete-other-windows &optional WINDOW)` -> nil.
///
/// Deletes all windows in the frame except WINDOW (or selected window).
pub(crate) fn builtin_delete_other_windows(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("delete-other-windows", &args, 2)?;
    let (fid, keep_wid) =
        resolve_window_id_or_error_in_state(&mut eval.frames, &mut eval.buffers, args.first())?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    let all_ids: Vec<WindowId> = frame.window_list();
    let to_delete: Vec<WindowId> = all_ids.into_iter().filter(|&w| w != keep_wid).collect();

    for wid in to_delete {
        let _ = eval.frames.delete_window(fid, wid);
    }
    // Select the kept window.
    let selected_buffer = if let Some(f) = eval.frames.get_mut(fid) {
        f.select_window(keep_wid);
        f.find_window(keep_wid).and_then(|w| w.buffer_id())
    } else {
        None
    };
    if let Some(buffer_id) = selected_buffer {
        eval.buffers.switch_current(buffer_id);
    }
    note_selected_window_buffer_in_state(&mut eval.frames, &mut eval.buffers, fid);
    // Run window-configuration-change-hook after successful deletion.
    let _ = builtin_run_window_configuration_change_hook(eval, vec![]);
    Ok(Value::NIL)
}
/// `(delete-window-internal WINDOW)` -> nil.
///
/// GNU Emacs exposes this primitive for low-level window internals. For the
/// compatibility surface we mirror the observable error behavior used by the
/// vm-compat coverage corpus.
pub(crate) fn builtin_delete_window_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("delete-window-internal", &args, 1)?;

    let wid =
        resolve_window_object_id_with_pred_in_state(frames, buffers, args.first(), "windowp")?;
    if !frames.is_valid_window_id(wid) {
        // GNU Emacs treats deleting an already deleted window object as a no-op.
        return Ok(Value::NIL);
    }

    let fid = frames
        .find_valid_window_frame_id(wid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let is_minibuffer = frame.minibuffer_window == Some(wid);
    let is_sole_ordinary_window = frame.window_list().len() <= 1;

    if is_minibuffer || is_sole_ordinary_window {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to delete minibuffer or sole ordinary window",
            )],
        ));
    }

    if frames.delete_window(fid, wid) {
        Ok(Value::NIL)
    } else {
        Err(signal("error", vec![Value::string("Deletion failed")]))
    }
}
/// `(delete-other-windows-internal &optional WINDOW ALL-FRAMES)` -> nil.
///
/// Deletes all ordinary windows in FRAME except WINDOW. ALL-FRAMES is accepted
/// for arity compatibility and currently ignored.
pub(crate) fn builtin_delete_other_windows_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("delete-other-windows-internal", &args, 2)?;
    let (fid, keep_wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    let all_ids: Vec<WindowId> = frame.window_list();
    let to_delete: Vec<WindowId> = all_ids.into_iter().filter(|&w| w != keep_wid).collect();

    for wid in to_delete {
        let _ = frames.delete_window(fid, wid);
    }
    let selected_buffer = if let Some(f) = frames.get_mut(fid) {
        f.select_window(keep_wid);
        f.find_window(keep_wid).and_then(|w| w.buffer_id())
    } else {
        None
    };
    if let Some(buffer_id) = selected_buffer {
        buffers.switch_current(buffer_id);
    }
    Ok(Value::NIL)
}
pub(crate) fn remember_selected_window_point_in_state(
    frames: &mut FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
) {
    let Some(frame) = frames.get(fid) else {
        return;
    };
    let selected_wid = frame.selected_window;
    let Some(buffer_id) = frame
        .find_window(selected_wid)
        .and_then(|window| window.buffer_id())
    else {
        return;
    };
    let Some(point) = buffers
        .get(buffer_id)
        .map(|buffer| buffer.point_char_pos().get().saturating_add(1))
    else {
        return;
    };
    if let Some(Window::Leaf {
        point: window_point,
        ..
    }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(selected_wid))
    {
        *window_point = lisp_char_pos_from_one_based_usize(point);
    }
}

pub(crate) fn sync_selected_window_buffer_in_state(
    frames: &FrameManager,
    buffers: &mut BufferManager,
    fid: FrameId,
) {
    let Some((buffer_id, point)) = frames
        .get(fid)
        .and_then(|frame| frame.find_window(frame.selected_window))
        .and_then(|window| match window {
            Window::Leaf {
                buffer_id, point, ..
            } => Some((*buffer_id, *point)),
            Window::Internal { .. } => None,
        })
    else {
        return;
    };
    // GNU `command_loop_1` only realigns `current_buffer` with
    // `selected_window` via `set_buffer_internal`; it does not call
    // `record_buffer`.  Selection/display primitives record explicitly.
    buffers.switch_current_unrecorded(buffer_id);
    if let Some(buffer) = buffers.get(buffer_id) {
        let byte_pos = buffer.lisp_pos_to_emacs_byte_pos(point);
        let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
    }
}

fn selected_window_buffer_state_in_frame(
    frames: &FrameManager,
    fid: FrameId,
) -> Option<(WindowId, BufferId)> {
    let frame = frames.get(fid)?;
    let selected_wid = frame.selected_window;
    let buffer_id = frame.find_window(selected_wid)?.buffer_id()?;
    Some((selected_wid, buffer_id))
}

fn note_selected_window_buffer_in_state(
    frames: &FrameManager,
    buffers: &mut BufferManager,
    fid: FrameId,
) {
    let Some((selected_wid, buffer_id)) = selected_window_buffer_state_in_frame(frames, fid) else {
        return;
    };
    if let Some(buffer) = buffers.get_mut(buffer_id) {
        buffer.last_selected_window = Some(selected_wid);
    }
}

fn update_buffer_display_metadata_in_state(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
) -> EvalResult {
    let display_time = super::timefns::builtin_current_time(vec![])?;
    let Some(buffer) = buffers.get_mut(buffer_id) else {
        return Ok(Value::NIL);
    };
    if let Some(count) = buffer
        .buffer_local_value("buffer-display-count")
        .and_then(|v| v.as_fixnum())
    {
        buffer.set_buffer_local(
            "buffer-display-count",
            Value::fixnum(count.saturating_add(1)),
        );
    }
    buffer.set_buffer_local("buffer-display-time", display_time);
    Ok(Value::NIL)
}

fn record_buffer_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    fid: FrameId,
) -> EvalResult {
    // Move to front of global buffer order (Vbuffer_alist equivalent).
    buffers.note_buffer_display(buffer_id);
    // Update frame buffer lists (GNU record_buffer, buffer.c:2223-2225).
    if let Some(frame) = frames.get_mut(fid) {
        frame.buffer_list.retain(|bid| *bid != buffer_id);
        frame.buffer_list.insert(0, buffer_id);
        frame.buried_buffer_list.retain(|bid| *bid != buffer_id);
    }
    Ok(Value::NIL)
}

fn window_displays_buffer(frames: &FrameManager, window_id: WindowId, buffer_id: BufferId) -> bool {
    frames
        .find_window_frame_id(window_id)
        .and_then(|frame_id| frames.get(frame_id))
        .and_then(|frame| frame.find_window(window_id))
        .and_then(Window::buffer_id)
        == Some(buffer_id)
}

/// `(select-window WINDOW &optional NORECORD)` -> WINDOW.
pub(crate) fn builtin_select_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("select-window", &args, 1)?;
    expect_max_args("select-window", &args, 2)?;
    let wid = match args.first().and_then(window_id_from_designator) {
        Some(wid) => wid,
        None => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("window-live-p"), args[0]],
            ));
        }
    };
    let (record_selection, run_buffer_list_hook) = {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        let fid = ensure_selected_frame_id_in_state(frames, buffers);
        let record_selection = args.get(1).is_none_or(|v| v.is_nil());
        remember_selected_window_point_in_state(frames, buffers, fid);
        {
            let frame = frames
                .get_mut(fid)
                .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
            if !frame.select_window(wid) {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("window-live-p"), args[0]],
                ));
            }
        }
        if record_selection {
            let _ = frames.note_window_selected(wid);
        }
        sync_selected_window_buffer_in_state(frames, buffers, fid);
        note_selected_window_buffer_in_state(frames, buffers, fid);
        // GNU Fselect_window calls record_buffer when NORECORD is nil.
        // record_buffer updates buffer lists and hooks; display count/time are
        // updated by set_window_buffer, not by selecting an already visible
        // window.
        if record_selection {
            if let Some(buffer_id) = frames
                .get(fid)
                .and_then(|f| f.find_window(wid))
                .and_then(Window::buffer_id)
            {
                record_buffer_in_state(frames, buffers, buffer_id, fid)?;
            }
        }
        let run_buffer_list_hook = record_selection
            && selected_window_buffer_state_in_frame(frames, fid)
                .is_some_and(|(_, buffer_id)| !buffers.buffer_hooks_inhibited(buffer_id));
        (record_selection, run_buffer_list_hook)
    };
    if record_selection && run_buffer_list_hook {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(window_value(wid))
}
/// `(other-window COUNT &optional ALL-FRAMES)` -> nil.
///
/// Select another window in cyclic order.
pub(crate) fn builtin_other_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("other-window", &args, 1)?;
    expect_max_args("other-window", &args, 3)?;
    let count = expect_number_or_marker_count(&args[0])?;
    let run_buffer_list_hook = {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        let _ = ensure_selected_frame_id_in_state(frames, buffers);
        let Some(fid) = frames.selected_frame().map(|f| f.id) else {
            return Ok(Value::NIL);
        };
        let Some(frame) = frames.get(fid) else {
            return Ok(Value::NIL);
        };
        let list = frame.window_list();
        if list.is_empty() {
            return Ok(Value::NIL);
        }
        let cur = frame.selected_window;
        let cur_idx = list.iter().position(|w| *w == cur).unwrap_or(0);
        let len = list.len() as i64;
        let new_idx = ((cur_idx as i64 + count) % len + len) % len;
        let new_wid = list[new_idx as usize];
        remember_selected_window_point_in_state(frames, buffers, fid);
        let switched = if let Some(frame) = frames.get_mut(fid) {
            frame.select_window(new_wid)
        } else {
            false
        };
        if switched {
            let _ = frames.note_window_selected(new_wid);
        };
        sync_selected_window_buffer_in_state(frames, buffers, fid);
        note_selected_window_buffer_in_state(frames, buffers, fid);
        selected_window_buffer_state_in_frame(frames, fid)
            .is_some_and(|(_, buffer_id)| !buffers.buffer_hooks_inhibited(buffer_id))
    };
    if run_buffer_list_hook {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}
/// `(other-window-for-scrolling)` -> window object used for scrolling.
pub(crate) fn builtin_other_window_for_scrolling(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("other-window-for-scrolling", &args, 0)?;
    let fid = ensure_selected_frame_id_in_state(frames, buffers);
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    let windows = frame.window_list();
    if windows.len() <= 1 {
        return Err(signal(
            "error",
            vec![Value::string("There is no other window")],
        ));
    }
    let selected = frame.selected_window;
    let other = windows
        .into_iter()
        .find(|wid| *wid != selected)
        .unwrap_or(selected);
    Ok(window_value(other))
}
/// `(next-window &optional WINDOW MINIBUF ALL-FRAMES)` -> window object.
pub(crate) fn builtin_next_window(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("next-window", &args, 3)?;
    let (fid, wid) = resolve_window_id_in_state(frames, buffers, args.first())?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let list = frame.window_list();
    if list.is_empty() {
        return Ok(Value::NIL);
    }
    let idx = list.iter().position(|w| *w == wid).unwrap_or(0);
    let next = (idx + 1) % list.len();
    Ok(window_value(list[next]))
}
/// `(previous-window &optional WINDOW MINIBUF ALL-FRAMES)` -> window object.
pub(crate) fn builtin_previous_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("previous-window", &args, 3)?;
    let (fid, wid) = resolve_window_id_in_state(frames, buffers, args.first())?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let list = frame.window_list();
    if list.is_empty() {
        return Ok(Value::NIL);
    }
    let idx = list.iter().position(|w| *w == wid).unwrap_or(0);
    let prev = if idx == 0 { list.len() - 1 } else { idx - 1 };
    Ok(window_value(list[prev]))
}
/// `(set-window-buffer WINDOW BUFFER-OR-NAME &optional KEEP-MARGINS)` -> nil.
pub(crate) fn builtin_set_window_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-window-buffer", &args, 2)?;
    expect_max_args("set-window-buffer", &args, 3)?;
    let (fid, wid, buf_id, keep_margins, run_buffer_list_hook) = {
        let (frames, buffers, minibuffers) =
            (&mut eval.frames, &mut eval.buffers, &eval.minibuffers);
        let (fid, wid) = resolve_window_id_in_state(frames, buffers, args.first())?;
        let buf_id = match args[1].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = args[1].as_buffer_id().unwrap();
                if buffers.get(bid).is_none() {
                    return Err(signal(
                        "error",
                        vec![Value::string("Attempt to display deleted buffer")],
                    ));
                }
                bid
            }
            ValueKind::String => match find_buffer_by_name_arg(buffers, &args[1])? {
                Some(id) => id,
                None => {
                    return Err(signal(
                        "wrong-type-argument",
                        vec![Value::symbol("bufferp"), Value::NIL],
                    ));
                }
            },
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), args[1]],
                ));
            }
        };

        let keep_margins = args.get(2).is_some_and(|arg| !arg.is_nil());
        let selected_fid = ensure_selected_frame_id_in_state(frames, buffers);
        let mut old_state = None;
        if let Some(Window::Leaf {
            buffer_id,
            window_start,
            point,
            dedicated,
            ..
        }) = frames.get_mut(fid).and_then(|f| f.find_window_mut(wid))
        {
            old_state = Some((*buffer_id, *window_start, *point, *dedicated));
        }
        let mut run_buffer_list_hook = false;
        if let Some((old_buffer_id, old_window_start, old_point, dedicated)) = old_state {
            if dedicated == Value::T && old_buffer_id != buf_id {
                let old_buffer_name = buffers
                    .get(old_buffer_id)
                    .map(|buffer| buffer.name_runtime_string_owned())
                    .unwrap_or_else(|| "*deleted*".to_string());
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Window is dedicated to ‘{old_buffer_name}’"
                    ))],
                ));
            }
            if let Some(buffer) = buffers.get_mut(old_buffer_id) {
                buffer.last_window_start = old_window_start.max(LispCharPos1::ONE);
            }
            let selected_buffer_id = frames
                .get(selected_fid)
                .and_then(|frame| frame.find_window(frame.selected_window))
                .and_then(Window::buffer_id);
            let old_buffer_last_selected_window = buffers
                .get(old_buffer_id)
                .and_then(|buffer| buffer.last_selected_window);
            let preserve_old_buffer_point = selected_buffer_id == Some(old_buffer_id)
                || old_buffer_last_selected_window.is_some_and(|last_selected_window| {
                    last_selected_window != wid
                        && window_displays_buffer(frames, last_selected_window, old_buffer_id)
                });
            if !preserve_old_buffer_point {
                let old_point_byte_pos = buffers.get(old_buffer_id).map(|buffer| {
                    buffer.lisp_pos_to_emacs_byte_pos(old_point.max(LispCharPos1::ONE))
                });
                if let Some(old_point_byte_pos) = old_point_byte_pos {
                    let _ = buffers.goto_buffer_emacs_byte_pos(old_buffer_id, old_point_byte_pos);
                }
            }
            if old_buffer_id != buf_id
                && let Some(buffer) = buffers.get_mut(old_buffer_id)
                && buffer.last_selected_window == Some(wid)
            {
                buffer.last_selected_window = None;
            }
            if old_buffer_id != buf_id {
                let old_buffer_value = Value::make_buffer(old_buffer_id);
                let old_window_start_pos = old_window_start.max(LispCharPos1::ONE);
                let old_point_pos = old_point.max(LispCharPos1::ONE);
                let history_entry = Value::list(vec![
                    old_buffer_value,
                    super::marker::make_marker_value(
                        Some(old_buffer_id),
                        Some(old_window_start_pos),
                        false,
                    ),
                    super::marker::make_marker_value(
                        Some(old_buffer_id),
                        Some(old_point_pos),
                        false,
                    ),
                ]);
                let filtered_prev = filtered_window_prev_buffers(
                    frames.window_prev_buffers(wid),
                    &[old_buffer_value],
                )?;
                frames.set_window_next_buffers(wid, Value::NIL);
                let record_window_history = should_record_window_history_buffer(
                    frames,
                    minibuffers,
                    buffers,
                    fid,
                    wid,
                    old_buffer_id,
                );
                if record_window_history {
                    let mut next_prev = Vec::with_capacity(filtered_prev.len() + 1);
                    next_prev.push(history_entry);
                    next_prev.extend(filtered_prev);
                    frames.set_window_prev_buffers(wid, Value::list(next_prev));
                } else {
                    frames.set_window_prev_buffers(wid, Value::list(filtered_prev));
                }
                run_buffer_list_hook =
                    record_window_history && !is_minibuffer_window(frames, fid, wid);
            } else {
                discard_buffers_from_window_history(frames, wid, &[Value::make_buffer(buf_id)])?;
            }
        }
        (fid, wid, buf_id, keep_margins, run_buffer_list_hook)
    };
    if run_buffer_list_hook {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        if buffers.get(buf_id).is_none() {
            return Err(signal(
                "error",
                vec![Value::string("Attempt to display deleted buffer")],
            ));
        }
        let next_margins = if keep_margins {
            None
        } else {
            Some(WindowMargins::new(
                buffer_margin_width(buffers, buf_id, "left-margin-width")?,
                buffer_margin_width(buffers, buf_id, "right-margin-width")?,
            ))
        };
        let next_fringes = if keep_margins {
            None
        } else {
            Some(WindowFringeDefaults::new(
                buffer_local_optional_dimension(buffers, buf_id, "left-fringe-width")?,
                buffer_local_optional_dimension(buffers, buf_id, "right-fringe-width")?,
                buffer_local_value(buffers, buf_id, "fringes-outside-margins").is_truthy(),
            ))
        };
        let next_scroll_bars = if keep_margins {
            None
        } else {
            let vertical_type = buffer_local_value(buffers, buf_id, "vertical-scroll-bar");
            if !valid_vertical_scroll_bar_type(vertical_type) {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid type of vertical scroll bar")],
                ));
            }
            let horizontal_type = buffer_local_value(buffers, buf_id, "horizontal-scroll-bar");
            if !valid_horizontal_scroll_bar_type(horizontal_type) {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid type of horizontal scroll bar")],
                ));
            }
            Some(WindowScrollBarDefaults::new(
                buffer_local_optional_dimension(buffers, buf_id, "scroll-bar-width")?,
                vertical_type,
                buffer_local_optional_dimension(buffers, buf_id, "scroll-bar-height")?,
                horizontal_type,
            ))
        };
        let old_state = frames
            .get(fid)
            .and_then(|frame| frame.find_window(wid))
            .and_then(|window| match window {
                Window::Leaf {
                    buffer_id,
                    window_start,
                    point,
                    dedicated,
                    ..
                } => Some((*buffer_id, *window_start, *point, *dedicated)),
                _ => None,
            });
        let selected_window = frames.get(fid).map(|frame| frame.selected_window);
        let same_buffer = old_state.is_some_and(|(old_buffer_id, _, _, _)| old_buffer_id == buf_id);
        let (next_window_start, next_point) = if same_buffer && keep_margins {
            old_state
                .map(|(_, window_start, point, _)| {
                    (
                        window_start.max(LispCharPos1::ONE),
                        point.max(LispCharPos1::ONE),
                    )
                })
                .unwrap_or((LispCharPos1::ONE, LispCharPos1::ONE))
        } else {
            buffers
                .get(buf_id)
                .map(|buf| {
                    (
                        buf.last_window_start.max(LispCharPos1::ONE),
                        lisp_char_pos_from_one_based_usize(
                            buf.point_char_pos().get().saturating_add(1).max(1),
                        ),
                    )
                })
                .unwrap_or((LispCharPos1::ONE, LispCharPos1::ONE))
        };
        frames.apply_set_window_buffer_state(
            wid,
            buf_id,
            next_window_start,
            next_point,
            same_buffer && keep_margins,
            WindowBufferDisplayDefaults {
                margins: next_margins,
                fringes: next_fringes,
                scroll_bars: next_scroll_bars,
            },
        );
        // Mirror GNU: non-T dedication (side, soft, etc.) is cleared
        // when the buffer changes (switch-to-buffer / set-window-buffer).
        if old_state.is_some_and(|(old_buf, _, _, ded)| {
            old_buf != buf_id && ded != Value::NIL && ded != Value::T
        }) {
            if let Some(frame) = frames.get_mut(fid) {
                if let Some(Window::Leaf { dedicated, .. }) = frame.find_window_mut(wid) {
                    *dedicated = Value::NIL;
                }
            }
        }
        update_buffer_display_metadata_in_state(buffers, buf_id)?;
        if let Some(frame) = frames.get_mut(fid) {
            if let Some(window) = frame.find_window_mut(wid) {
                super::super::window::window_markers::create_window_markers(
                    buffers, window, buf_id,
                );
            }
        }
        if selected_window == Some(wid)
            && let Some(buffer) = buffers.get_mut(buf_id)
        {
            buffer.last_selected_window = Some(wid);
        }
    }
    Ok(Value::NIL)
}

/// `(switch-to-buffer BUFFER-OR-NAME &optional NORECORD FORCE-SAME-WINDOW)` -> buffer.
pub(crate) fn builtin_switch_to_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("switch-to-buffer", &args, 1)?;
    expect_max_args("switch-to-buffer", &args, 3)?;
    let record_selection = args.get(1).is_none_or(|v| v.is_nil());
    let (buf_id, run_buffer_list_hook) = {
        let buf_id = match args[0].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = args[0].as_buffer_id().unwrap();
                if eval.buffers.get(bid).is_none() {
                    return Err(signal(
                        "error",
                        vec![Value::string("Attempt to display deleted buffer")],
                    ));
                }
                bid
            }
            ValueKind::String => find_or_create_buffer_by_name_arg(&mut eval.buffers, &args[0])?,
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), args[0]],
                ));
            }
        };

        let fid = ensure_selected_frame_id(eval);
        let sel_wid = eval
            .frames
            .get(fid)
            .map(|f| f.selected_window)
            .ok_or_else(|| signal("error", vec![Value::string("No selected window")]))?;
        if let Some(w) = eval
            .frames
            .get_mut(fid)
            .and_then(|f| f.find_window_mut(sel_wid))
        {
            w.set_buffer(buf_id);
        }
        eval.switch_current_buffer(buf_id)?;
        if let Some(buffer) = eval.buffers.get_mut(buf_id) {
            buffer.last_selected_window = Some(sel_wid);
        }
        update_buffer_display_metadata_in_state(&mut eval.buffers, buf_id)?;
        if record_selection {
            record_buffer_in_state(&mut eval.frames, &mut eval.buffers, buf_id, fid)?;
        }
        (
            buf_id,
            record_selection
                && !is_minibuffer_window(&eval.frames, fid, sel_wid)
                && !eval.buffers.buffer_hooks_inhibited(buf_id),
        )
    };
    if run_buffer_list_hook {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::make_buffer(buf_id))
}

/// `(display-buffer BUFFER-OR-NAME &optional ACTION FRAME)` -> window object or nil.
///
/// Simplified but functional implementation that respects basic display
/// actions.  The strategy is:
///
/// 1. If the buffer is already displayed in a window on the frame, reuse that
///    window.
/// 2. If ACTION contains `display-buffer-same-window`, use the selected window.
/// 3. If ACTION contains `display-buffer-pop-up-window`, split the selected
///    window and display there.
/// 4. Default (no matching action): try to find another (non-selected) window,
///    and if only one window exists, split it.
pub(crate) fn builtin_display_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("display-buffer", &args, 1)?;
    expect_max_args("display-buffer", &args, 3)?;
    let buf_id = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let bid = args[0].as_buffer_id().unwrap();
            if eval.buffers.get(bid).is_none() {
                return Err(signal("error", vec![Value::string("Invalid buffer")]));
            }
            bid
        }
        ValueKind::String => match find_buffer_by_name_arg(&eval.buffers, &args[0])? {
            Some(id) => id,
            None => return Err(signal("error", vec![Value::string("Invalid buffer")])),
        },
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };

    let fid = ensure_selected_frame_id(eval);
    let sel_wid = eval
        .frames
        .get(fid)
        .map(|f| f.selected_window)
        .ok_or_else(|| signal("error", vec![Value::string("No selected window")]))?;

    // Collect the ACTION argument to check for specific display functions.
    let action = args.get(1).copied().unwrap_or(Value::NIL);
    let display_in_window =
        |eval: &mut super::eval::Context, wid: WindowId, buf_id: BufferId| -> Result<(), Flow> {
            builtin_set_window_buffer(eval, vec![window_value(wid), Value::make_buffer(buf_id)])?;
            Ok(())
        };

    // Helper: check whether a particular display-function symbol appears in
    // the ACTION value.  ACTION can be:
    //   - nil                         -> no specific action
    //   - (FUNCTION . ALIST)          -> single function
    //   - ((FUNCTION ...) . ALIST)    -> list of functions
    let action_contains = |name: &str| -> bool {
        if action.is_nil() {
            return false;
        }
        // ACTION is a cons cell; the car is a function or a list of functions.
        let car = match action.kind() {
            ValueKind::Cons => {
                let snap_car = action.cons_car();
                let snap_cdr = action.cons_cdr();
                snap_car
            }
            _ => return false,
        };
        // car could be a symbol directly ...
        if let Some(sym_name) = car.as_symbol_name() {
            if sym_name == name {
                return true;
            }
        }
        // ... or a list of symbols.
        let mut cursor = car;
        while cursor.is_cons() {
            let snap_car = cursor.cons_car();
            let snap_cdr = cursor.cons_cdr();
            if let Some(sym_name) = snap_car.as_symbol_name() {
                if sym_name == name {
                    return true;
                }
            }
            cursor = snap_cdr;
        }
        false
    };

    // --- Strategy 1: reuse an existing window showing the buffer. -----------
    // (covers `display-buffer-reuse-window` action and the default check)
    {
        let frame = eval
            .frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        for wid in frame.window_list() {
            if let Some(w) = frame.find_window(wid) {
                if w.buffer_id() == Some(buf_id) {
                    return Ok(window_value(wid));
                }
            }
        }
    }

    // --- Strategy 2: `display-buffer-same-window` ---------------------------
    if action_contains("display-buffer-same-window") {
        display_in_window(eval, sel_wid, buf_id)?;
        return Ok(window_value(sel_wid));
    }

    // --- Strategy 3: `display-buffer-pop-up-window` -------------------------
    if action_contains("display-buffer-pop-up-window") {
        let new_wid = eval
            .frames
            .split_window(
                fid,
                sel_wid,
                SplitDirection::Vertical,
                buf_id,
                None,
                SplitPlacement::AfterTarget,
            )
            .ok_or_else(|| signal("error", vec![Value::string("Cannot split window")]))?;
        display_in_window(eval, new_wid, buf_id)?;
        eval.frames.set_window_parameter(
            new_wid,
            Value::symbol("quit-restore"),
            Value::list(vec![
                Value::symbol("window"),
                Value::symbol("window"),
                window_value(sel_wid),
                Value::make_buffer(buf_id),
            ]),
        );
        return Ok(window_value(new_wid));
    }

    // --- Strategy 4 (default): use another window, or split if needed. ------
    {
        let window_list = eval
            .frames
            .get(fid)
            .map(|f| f.window_list())
            .unwrap_or_default();

        // Prefer a different window from the selected one.
        if let Some(&other_wid) = window_list.iter().find(|&&wid| wid != sel_wid) {
            display_in_window(eval, other_wid, buf_id)?;
            return Ok(window_value(other_wid));
        }

        // Only one window -- split it.
        let new_wid = eval
            .frames
            .split_window(
                fid,
                sel_wid,
                SplitDirection::Vertical,
                buf_id,
                None,
                SplitPlacement::AfterTarget,
            )
            .ok_or_else(|| signal("error", vec![Value::string("Cannot split window")]))?;
        display_in_window(eval, new_wid, buf_id)?;
        eval.frames.set_window_parameter(
            new_wid,
            Value::symbol("quit-restore"),
            Value::list(vec![
                Value::symbol("window"),
                Value::symbol("window"),
                window_value(sel_wid),
                Value::make_buffer(buf_id),
            ]),
        );
        Ok(window_value(new_wid))
    }
}

/// `(pop-to-buffer BUFFER-OR-NAME &optional ACTION NORECORD)` -> buffer.
///
/// Batch compatibility follows Emacs' noninteractive behavior: switch current
/// buffer and return the buffer object.
pub(crate) fn builtin_pop_to_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("pop-to-buffer", &args, 1)?;
    expect_max_args("pop-to-buffer", &args, 3)?;
    let record_selection = args.get(2).is_none_or(|v| v.is_nil());
    let buf_id = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let bid = args[0].as_buffer_id().unwrap();
            if eval.buffers.get(bid).is_none() {
                return Err(signal("error", vec![Value::string("Invalid buffer")]));
            }
            bid
        }
        ValueKind::String => find_or_create_buffer_by_name_arg(&mut eval.buffers, &args[0])?,
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };

    let fid = ensure_selected_frame_id(eval);
    let sel_wid = eval
        .frames
        .get(fid)
        .map(|f| f.selected_window)
        .ok_or_else(|| signal("error", vec![Value::string("No selected window")]))?;
    if let Some(w) = eval
        .frames
        .get_mut(fid)
        .and_then(|f| f.find_window_mut(sel_wid))
    {
        w.set_buffer(buf_id);
    }
    eval.switch_current_buffer(buf_id)?;
    if let Some(buffer) = eval.buffers.get_mut(buf_id) {
        buffer.last_selected_window = Some(sel_wid);
    }
    update_buffer_display_metadata_in_state(&mut eval.buffers, buf_id)?;
    if record_selection {
        record_buffer_in_state(&mut eval.frames, &mut eval.buffers, buf_id, fid)?;
        if !eval.buffers.buffer_hooks_inhibited(buf_id) {
            super::builtins::run_buffer_list_update_hook(eval)?;
        }
    }
    Ok(Value::make_buffer(buf_id))
}

const MIN_FRAME_COLS: i64 = 10;
const MIN_FRAME_TEXT_LINES: i64 = 5;
const FRAME_TEXT_LINES_PARAM: &str = "neovm--frame-text-lines";
const LIVE_GUI_RESIZE_ACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone, Copy, Debug)]
enum FrameSizeParam {
    Cells(i64),
    TextPixels(u32),
}

impl FrameSizeParam {
    fn is_zero(self) -> bool {
        match self {
            Self::Cells(n) => n == 0,
            Self::TextPixels(px) => px == 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameResizeRequest {
    TextPixels { width: u32, height: u32 },
    Cells { cols: i64, total_lines: i64 },
}

impl FrameResizeRequest {
    fn text_pixels(self, frames: &FrameManager, fid: FrameId) -> Result<(u32, u32), Flow> {
        match self {
            Self::TextPixels { width, height } => Ok((width.max(1), height.max(1))),
            Self::Cells { cols, total_lines } => {
                live_gui_resize_pixels_from_logical_size(frames, fid, cols, total_lines)
            }
        }
    }

    fn logical_size(self, frames: &FrameManager, fid: FrameId) -> Result<(i64, i64), Flow> {
        let frame = frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        Ok(match self {
            Self::Cells { cols, total_lines } => (cols.max(1), total_lines.max(1)),
            Self::TextPixels { width, height } => {
                let char_width = frame.char_width.max(1.0);
                let char_height = frame.char_height.max(1.0);
                (
                    ((width as f32) / char_width).floor().max(1.0) as i64,
                    ((height as f32) / char_height).floor().max(1.0) as i64,
                )
            }
        })
    }
}

fn sync_live_gui_resize_for_geometry_queries(
    eval: &mut super::eval::Context,
    fid: FrameId,
) -> Result<(), Flow> {
    eval.sync_pending_resize_events();
    if flush_pending_live_gui_resize(eval, fid)? {
        eval.wait_for_pending_resize_events(LIVE_GUI_RESIZE_ACK_TIMEOUT);
    }
    Ok(())
}

fn frame_total_cols(frame: &crate::window::Frame) -> i64 {
    frame
        .parameter("width")
        .and_then(|v| v.as_int())
        .unwrap_or(frame.columns() as i64)
}

fn frame_text_cols(frame: &crate::window::Frame) -> i64 {
    frame_total_cols(frame)
}

fn frame_uses_window_system_pixels(frame: &crate::window::Frame) -> bool {
    frame.effective_window_system().is_some()
}

fn frame_non_text_height_pixels(frame: &crate::window::Frame) -> u32 {
    // GNU frame text size includes the minibuffer window.  Only true frame
    // chrome lives outside the text area for sizing math here.
    frame
        .menu_bar_height
        .saturating_add(frame.tool_bar_height)
        .saturating_add(frame.tab_bar_height)
}

fn frame_non_text_width_pixels_in_state(frames: &FrameManager, fid: FrameId) -> u32 {
    frames
        .get(fid)
        .map(|frame| frame.horizontal_non_text_width().max(0) as u32)
        .unwrap_or(0)
}

fn frame_internal_border_total_pixels(frame: &crate::window::Frame) -> u32 {
    u32::try_from(frame.internal_border_width().max(0))
        .unwrap_or(u32::MAX / 2)
        .saturating_mul(2)
}

fn frame_non_text_total_width_pixels_in_state(frames: &FrameManager, fid: FrameId) -> u32 {
    let border = frames
        .get(fid)
        .map(frame_internal_border_total_pixels)
        .unwrap_or(0);
    frame_non_text_width_pixels_in_state(frames, fid).saturating_add(border)
}

fn frame_non_text_total_height_pixels(frame: &crate::window::Frame) -> u32 {
    frame_non_text_height_pixels(frame).saturating_add(frame_internal_border_total_pixels(frame))
}

fn frame_text_width_pixels_in_state(frames: &FrameManager, fid: FrameId) -> u32 {
    let Some(frame) = frames.get(fid) else {
        return 0;
    };
    frame
        .width
        .saturating_sub(frame_non_text_total_width_pixels_in_state(frames, fid))
        .max(1)
}

fn frame_text_height_pixels(frame: &crate::window::Frame) -> u32 {
    frame
        .height
        .saturating_sub(frame_non_text_total_height_pixels(frame))
        .max(1)
}

fn check_frame_pixels(value: &Value, pixelwise: bool, item_size: f32) -> Result<u32, Flow> {
    let size = expect_int(value)?;
    if size <= 0 {
        return Err(signal(
            "args-out-of-range",
            vec![*value, Value::fixnum(1), Value::fixnum(i64::from(i32::MAX))],
        ));
    }
    let unit = if pixelwise {
        1
    } else {
        item_size.max(1.0).round() as i64
    };
    let pixels = size.checked_mul(unit).ok_or_else(|| {
        signal(
            "args-out-of-range",
            vec![*value, Value::fixnum(1), Value::fixnum(i64::from(i32::MAX))],
        )
    })?;
    if pixels <= 0 || pixels > u32::MAX as i64 {
        return Err(signal(
            "args-out-of-range",
            vec![*value, Value::fixnum(1), Value::fixnum(i64::from(i32::MAX))],
        ));
    }
    Ok(pixels as u32)
}

fn parse_frame_size_param(value: Value) -> Option<FrameSizeParam> {
    if let Some(n) = value.as_int().filter(|n| *n >= 0) {
        return Some(FrameSizeParam::Cells(n));
    }
    if value.is_cons()
        && value
            .cons_car()
            .as_symbol_name()
            .is_some_and(|name| name == "text-pixels")
    {
        return value
            .cons_cdr()
            .as_int()
            .filter(|n| *n >= 0 && *n <= i64::from(u32::MAX))
            .map(|n| FrameSizeParam::TextPixels(n as u32));
    }
    None
}

fn frame_size_param_to_cells(param: FrameSizeParam, item_size: f32) -> i64 {
    match param {
        FrameSizeParam::Cells(n) => n,
        FrameSizeParam::TextPixels(px) => {
            ((px as f32) / item_size.max(1.0)).floor().max(1.0) as i64
        }
    }
}

fn frame_size_param_to_pixels(param: FrameSizeParam, item_size: f32) -> u32 {
    match param {
        FrameSizeParam::Cells(n) => {
            let unit = item_size.max(1.0).round() as i64;
            n.saturating_mul(unit).max(1).min(u32::MAX as i64) as u32
        }
        FrameSizeParam::TextPixels(px) => px.max(1),
    }
}

fn frame_resize_request_from_params(
    width: Option<FrameSizeParam>,
    height: Option<FrameSizeParam>,
    current_cols: i64,
    current_total_lines: i64,
    current_text_width_px: u32,
    current_text_height_px: u32,
    char_width: f32,
    char_height: f32,
) -> FrameResizeRequest {
    let has_text_pixels = width.is_some_and(|size| matches!(size, FrameSizeParam::TextPixels(_)))
        || height.is_some_and(|size| matches!(size, FrameSizeParam::TextPixels(_)));
    if has_text_pixels {
        FrameResizeRequest::TextPixels {
            width: width
                .map(|size| frame_size_param_to_pixels(size, char_width))
                .unwrap_or(current_text_width_px),
            height: height
                .map(|size| frame_size_param_to_pixels(size, char_height))
                .unwrap_or(current_text_height_px),
        }
    } else {
        FrameResizeRequest::Cells {
            cols: width
                .map(|size| frame_size_param_to_cells(size, char_width))
                .unwrap_or(current_cols)
                .max(1),
            total_lines: height
                .map(|size| frame_size_param_to_cells(size, char_height))
                .unwrap_or(current_total_lines)
                .max(1),
        }
    }
}

fn frame_total_lines(frame: &crate::window::Frame) -> i64 {
    frame
        .parameter("height")
        .and_then(|v| v.as_int())
        .unwrap_or(frame.lines() as i64)
}

fn frame_text_lines(frame: &crate::window::Frame) -> i64 {
    frame
        .parameter(FRAME_TEXT_LINES_PARAM)
        .and_then(|v| v.as_int())
        .unwrap_or_else(|| frame_total_lines(frame))
}

fn clamp_frame_dimension(value: i64, minimum: i64) -> i64 {
    value.max(minimum).min(u32::MAX as i64)
}

fn set_frame_text_size(frame: &mut crate::window::Frame, cols: i64, text_lines: i64) {
    let is_child_frame = frame.parent_frame.as_frame_id().is_some();
    let min_cols = if is_child_frame { 1 } else { MIN_FRAME_COLS };
    let min_text_lines = if is_child_frame {
        1
    } else {
        MIN_FRAME_TEXT_LINES
    };
    let cols = clamp_frame_dimension(cols, min_cols);
    let text_lines = clamp_frame_dimension(text_lines, min_text_lines);
    let minibuffer_lines = i64::from(frame.minibuffer_leaf.is_some());
    let total_lines = text_lines
        .saturating_add(minibuffer_lines)
        .min(u32::MAX as i64);

    frame.set_parameter(Value::symbol("width"), Value::fixnum(cols));
    frame.set_parameter(Value::symbol("height"), Value::fixnum(total_lines));
    frame.set_parameter(
        Value::symbol(FRAME_TEXT_LINES_PARAM),
        Value::fixnum(text_lines),
    );
    if frame.parent_frame.as_frame_id().is_some() {
        let char_width = frame.char_width.max(1.0).round() as u32;
        let char_height = frame.char_height.max(1.0).round() as u32;
        frame.width = (cols as u32).saturating_mul(char_width).max(1);
        frame.height = (total_lines as u32).saturating_mul(char_height).max(1);
        frame.sync_window_area_bounds();
    }
}

fn live_gui_resize_pixels_from_logical_size(
    frames: &FrameManager,
    fid: FrameId,
    desired_cols: i64,
    desired_total_lines: i64,
) -> Result<(u32, u32), Flow> {
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let char_width = frame.char_width.max(1.0).round();
    let char_height = frame.char_height.max(1.0).round();
    let non_text_height = frame_non_text_height_pixels(frame);
    let total_height_px = ((desired_total_lines.max(1) as f32) * char_height)
        .round()
        .max(1.0) as u32;
    let text_width_px = ((desired_cols.max(1) as f32) * char_width).round().max(1.0) as u32;
    let text_height_px = total_height_px
        .saturating_sub(non_text_height)
        .max(char_height.round().max(1.0) as u32);
    Ok((text_width_px, text_height_px))
}

fn resize_live_gui_frame(
    frames: &mut FrameManager,
    buffers: &BufferManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    fid: FrameId,
    text_width_px: u32,
    text_height_px: u32,
    pretend: bool,
) -> Result<(), Flow> {
    let (total_width_px, total_height_px, title, cols, text_lines) = {
        let frame = frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        let char_width = frame.char_width.max(1.0).round();
        let char_height = frame.char_height.max(1.0).round();
        let cols = ((text_width_px as f32) / char_width).floor().max(1.0) as i64;
        let text_lines = ((text_height_px as f32) / char_height).floor().max(1.0) as i64;
        let non_text_width = frame_non_text_total_width_pixels_in_state(frames, fid);
        let non_text_height = frame_non_text_total_height_pixels(frame);
        let title = frame.host_title_lisp_string();
        (
            text_width_px.saturating_add(non_text_width).max(1),
            text_height_px.saturating_add(non_text_height).max(1),
            title,
            cols,
            text_lines,
        )
    };

    {
        let frame = frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        frame.clear_pending_gui_resize();
        tracing::debug!(
            "resize_live_gui_frame: fid={:?} pretend={} total={}x{} cols={} text_lines={}",
            fid,
            pretend,
            total_width_px,
            total_height_px,
            cols,
            text_lines
        );
        if pretend {
            set_frame_text_size(frame, cols, text_lines);
        } else {
            frame.resize_pixelwise_with_buffer_constraints(
                buffers,
                total_width_px,
                total_height_px,
            );
            frame.set_parameter(
                Value::symbol(FRAME_TEXT_LINES_PARAM),
                Value::fixnum(text_lines),
            );
        }
    }

    let is_child_frame = frames
        .get(fid)
        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some());
    if !pretend
        && !is_child_frame
        && let Some(host) = display_host.as_mut()
    {
        let geometry_hints = frames
            .get(fid)
            .map(|frame| frame.gui_geometry_hints())
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        tracing::debug!(
            "resize_live_gui_frame: notifying host fid={:?} size={}x{} title={:?}",
            fid,
            total_width_px,
            total_height_px,
            title
        );
        host.resize_gui_frame(super::eval::GuiFrameHostRequest {
            frame_id: fid,
            width: total_width_px,
            height: total_height_px,
            title,
            geometry_hints,
        })
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    }

    Ok(())
}

fn request_live_gui_frame_resize(
    frames: &mut FrameManager,
    buffers: &BufferManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    fid: FrameId,
    text_width_px: u32,
    text_height_px: u32,
    pretend: bool,
) -> Result<(), Flow> {
    let (total_width_px, total_height_px, title, cols, text_lines) = {
        let frame = frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        let char_width = frame.char_width.max(1.0).round();
        let char_height = frame.char_height.max(1.0).round();
        let cols = ((text_width_px as f32) / char_width).floor().max(1.0) as i64;
        let text_lines = ((text_height_px as f32) / char_height).floor().max(1.0) as i64;
        let non_text_width = frame_non_text_total_width_pixels_in_state(frames, fid);
        let non_text_height = frame_non_text_total_height_pixels(frame);
        let title = frame.host_title_lisp_string();
        (
            text_width_px.saturating_add(non_text_width).max(1),
            text_height_px.saturating_add(non_text_height).max(1),
            title,
            cols,
            text_lines,
        )
    };

    tracing::debug!(
        "request_live_gui_frame_resize: fid={:?} pretend={} total={}x{} cols={} text_lines={} host={}",
        fid,
        pretend,
        total_width_px,
        total_height_px,
        cols,
        text_lines,
        display_host.is_some()
    );

    if pretend {
        let frame = frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        frame.clear_pending_gui_resize();
        set_frame_text_size(frame, cols, text_lines);
        return Ok(());
    }

    if let Some(frame) = frames.get_mut(fid) {
        frame.clear_pending_gui_resize();
    }

    let is_child_frame = frames
        .get(fid)
        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some());
    if !is_child_frame && let Some(host) = display_host.as_mut() {
        let geometry_hints = frames
            .get(fid)
            .map(|frame| frame.gui_geometry_hints())
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        host.resize_gui_frame(super::eval::GuiFrameHostRequest {
            frame_id: fid,
            width: total_width_px,
            height: total_height_px,
            title,
            geometry_hints,
        })
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
        return Ok(());
    }

    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    frame.resize_pixelwise_with_buffer_constraints(buffers, total_width_px, total_height_px);
    frame.set_parameter(
        Value::symbol(FRAME_TEXT_LINES_PARAM),
        Value::fixnum(text_lines),
    );
    Ok(())
}

fn request_live_gui_frame_resize_and_keep_pending(
    frames: &mut FrameManager,
    buffers: &BufferManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    fid: FrameId,
    request: FrameResizeRequest,
) -> Result<(), Flow> {
    let (text_width_px, text_height_px) = request.text_pixels(frames, fid)?;
    let (total_width_px, total_height_px, title, geometry_hints) = {
        let frame = frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        let non_text_width = frame_non_text_total_width_pixels_in_state(frames, fid);
        let non_text_height = frame_non_text_total_height_pixels(frame);
        (
            text_width_px.saturating_add(non_text_width).max(1),
            text_height_px.saturating_add(non_text_height).max(1),
            frame.host_title_lisp_string(),
            frame.gui_geometry_hints(),
        )
    };

    let is_child_frame = frames
        .get(fid)
        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some());
    let Some(host) = display_host.as_mut().filter(|_| !is_child_frame) else {
        return request_live_gui_frame_resize(
            frames,
            buffers,
            display_host,
            fid,
            text_width_px,
            text_height_px,
            false,
        );
    };

    host.resize_gui_frame(super::eval::GuiFrameHostRequest {
        frame_id: fid,
        width: total_width_px,
        height: total_height_px,
        title,
        geometry_hints,
    })
    .map_err(|message| signal("error", vec![Value::string(message)]))?;

    let (desired_cols, desired_total_lines) = request.logical_size(frames, fid)?;
    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    frame.queue_pending_gui_resize(desired_cols, desired_total_lines, true);
    Ok(())
}

fn flush_pending_live_gui_resize(
    eval: &mut super::eval::Context,
    fid: FrameId,
) -> Result<bool, Flow> {
    let pending = eval
        .frames
        .get_mut(fid)
        .and_then(|frame| frame.take_pending_gui_resize());
    let Some(pending) = pending else {
        return Ok(false);
    };

    let (text_width_px, text_height_px) = live_gui_resize_pixels_from_logical_size(
        &eval.frames,
        fid,
        pending.width_cols,
        pending.total_lines,
    )?;

    tracing::debug!(
        "flush_pending_live_gui_resize: fid={:?} cols={} total_lines={} text={}x{}",
        fid,
        pending.width_cols,
        pending.total_lines,
        text_width_px,
        text_height_px
    );

    if pending.host_request_sent {
        Ok(true)
    } else {
        let waits_for_host_ack = eval.display_host.is_some()
            && eval
                .frames
                .get(fid)
                .is_some_and(|frame| frame.parent_frame.as_frame_id().is_none());
        request_live_gui_frame_resize(
            &mut eval.frames,
            &eval.buffers,
            &mut eval.display_host,
            fid,
            text_width_px,
            text_height_px,
            false,
        )?;
        Ok(waits_for_host_ack)
    }
}

// ===========================================================================
// Scroll / frame visibility command shims
// ===========================================================================

fn recenter_missing_display_error() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "‘recenter’ing a window that does not display current-buffer.",
        )],
    )
}

fn scroll_up_batch_error() -> Flow {
    signal("end-of-buffer", vec![])
}

fn scroll_down_batch_error() -> Flow {
    signal("beginning-of-buffer", vec![])
}

/// `(scroll-up-command &optional ARG)` — delegates to scroll-up.
pub(crate) fn builtin_scroll_up_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("scroll-up-command", &args, 1)?;
    builtin_scroll_up(eval, args)
}

/// `(scroll-down-command &optional ARG)` — delegates to scroll-down.
pub(crate) fn builtin_scroll_down_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("scroll-down-command", &args, 1)?;
    builtin_scroll_down(eval, args)
}

/// Compute scroll distance: if ARG is nil, use window height minus
/// next-screen-context-lines; otherwise use ARG as line count.
fn scroll_lines(eval: &mut super::eval::Context, arg: Option<&Value>, direction: i64) -> i64 {
    scroll_lines_in_state(
        &eval.obarray,
        &mut eval.frames,
        &mut eval.buffers,
        arg,
        direction,
    )
}

fn scroll_lines_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
    direction: i64,
) -> i64 {
    if let Some(v) = arg {
        if !v.is_nil() {
            // Explicit line count.
            let n = match v.kind() {
                ValueKind::Fixnum(n) => n,
                _ => 1,
            };
            return n * direction;
        }
    }
    // nil or absent: full window minus context lines.
    let wh = window_body_height_impl(frames, buffers, vec![])
        .ok()
        .and_then(|v| v.as_fixnum())
        .unwrap_or(24);
    let ctx = obarray
        .symbol_value("next-screen-context-lines")
        .and_then(|v| v.as_fixnum())
        .unwrap_or(2);
    (wh - ctx).max(1) * direction
}
/// `(scroll-up &optional ARG)` — scroll text upward (forward in buffer).
///
/// Mirror GNU Emacs Fscroll_up (window.c): move point forward by ARG lines
/// (or a windowful if nil).  Signals end-of-buffer if already at end.
pub(crate) fn builtin_scroll_up(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("scroll-up", &args, 1)?;
    let arg = args.first().cloned();
    let lines = scroll_lines_in_state(
        &eval.obarray,
        &mut eval.frames,
        &mut eval.buffers,
        arg.as_ref(),
        1,
    );
    let result = scroll_by_lines_in_state(&mut eval.frames, &mut eval.buffers, lines);
    // Run window-scroll-functions hook after scroll completes
    let _ = builtin_run_window_scroll_functions(eval, vec![]);
    result
}
/// `(scroll-down &optional ARG)` — scroll text downward (backward in buffer).
///
/// Mirror GNU Emacs Fscroll_down (window.c): move point backward by ARG lines
/// (or a windowful if nil).  Signals beginning-of-buffer if already at start.
pub(crate) fn builtin_scroll_down(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("scroll-down", &args, 1)?;
    let arg = args.first().cloned();
    let lines = scroll_lines_in_state(
        &eval.obarray,
        &mut eval.frames,
        &mut eval.buffers,
        arg.as_ref(),
        -1,
    );
    let result = scroll_by_lines_in_state(&mut eval.frames, &mut eval.buffers, lines);
    // Run window-scroll-functions hook after scroll completes
    let _ = builtin_run_window_scroll_functions(eval, vec![]);
    result
}

/// Move point by `lines` newlines (positive=forward, negative=backward).
/// Signals end-of-buffer or beginning-of-buffer on boundary.
fn scroll_by_lines(eval: &mut super::eval::Context, lines: i64) -> EvalResult {
    scroll_by_lines_in_state(&mut eval.frames, &mut eval.buffers, lines)
}

fn scroll_by_lines_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    lines: i64,
) -> EvalResult {
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) = resolve_window_id_in_state(frames, buffers, None)?;
    let (buffer_id, window_point) = match get_leaf(frames, fid, wid)? {
        Window::Leaf {
            buffer_id, point, ..
        } => (*buffer_id, *point),
        _ => return Ok(Value::NIL),
    };
    let Some(buf) = buffers.get(buffer_id) else {
        return Ok(Value::NIL);
    };
    let text = buf.full_text_string();
    let accessible = buf.accessible_emacs_byte_region();
    let pt = accessible
        .clamp(buf.lisp_pos_to_emacs_byte_pos(window_point))
        .get();
    let bytes = text.as_bytes();
    let begv = accessible.start().get();
    let zv = accessible.end().get();

    let mut pos = pt;

    if lines > 0 {
        if pt >= zv {
            return Err(scroll_up_batch_error());
        }
        for _ in 0..lines {
            while pos < zv && bytes[pos] != b'\n' {
                pos += 1;
            }
            if pos < zv {
                pos += 1; // past newline
            }
        }
    } else {
        if pt <= begv {
            return Err(scroll_down_batch_error());
        }
        let target = (-lines) as usize;
        // First go to beginning of current line.
        while pos > begv && bytes[pos - 1] != b'\n' {
            pos -= 1;
        }
        for _ in 0..target {
            if pos <= begv {
                break;
            }
            pos -= 1; // before newline
            while pos > begv && bytes[pos - 1] != b'\n' {
                pos -= 1;
            }
        }
    }

    let point_lisp = buf.emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(pos));
    let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, EmacsBytePos::new(pos));
    if let Some(window) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        crate::window::window_markers::set_window_point_with_marker(buffers, window, point_lisp);
        crate::window::window_markers::set_window_start_with_marker(buffers, window, point_lisp);
    }
    Ok(Value::NIL)
}

/// `(recenter-top-bottom &optional ARG)` — delegates to recenter.
pub(crate) fn builtin_recenter_top_bottom(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("recenter-top-bottom", &args, 1)?;
    builtin_recenter(eval, args)
}
/// `(recenter &optional ARG REDISPLAY)` — center point in window.
///
/// Mirror GNU Emacs Frecenter (window.c): adjust window-start so that
/// point appears at the center of the window, or at line ARG from the
/// top (or bottom if ARG is negative).
pub(crate) fn builtin_recenter(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("recenter", &args, 2)?;
    {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);

        let wh = window_body_height_impl(frames, buffers, vec![])
            .ok()
            .and_then(|v| v.as_fixnum())
            .unwrap_or(24);

        // Determine target line from top of window where point should appear.
        let target_line = match args.first().and_then(|v| v.as_fixnum()) {
            Some(n) => {
                if n >= 0 {
                    n
                } else {
                    // Negative: count from bottom.
                    (wh + n).max(0)
                }
            }
            None if args.first().is_some_and(|v| !v.is_nil()) => wh / 2, // non-integer truthy = center
            _ => wh / 2,                                                 // nil or absent = center
        };

        // Compute new window-start by moving backward target_line lines from point.
        let _ = ensure_selected_frame_id_in_state(frames, buffers);
        let (fid, wid) = resolve_window_id_in_state(frames, buffers, None)?;
        let (buffer_id, window_point) = match get_leaf(frames, fid, wid)? {
            Window::Leaf {
                buffer_id, point, ..
            } => {
                if buffers.current_buffer_id() != Some(*buffer_id) {
                    return Err(signal(
                        "error",
                        vec![Value::string(
                            "`recenter'ing a window that does not display current-buffer",
                        )],
                    ));
                }
                let point = buffers
                    .get(*buffer_id)
                    .map(|buf| {
                        lisp_char_pos_from_one_based_usize(
                            buf.point_char_pos().get().saturating_add(1),
                        )
                    })
                    .unwrap_or(*point);
                (*buffer_id, point)
            }
            _ => return Ok(Value::NIL),
        };
        let Some(buf) = buffers.get(buffer_id) else {
            return Ok(Value::NIL);
        };
        let text = buf.full_text_string();
        let accessible = buf.accessible_emacs_byte_region();
        let pt = accessible
            .clamp(buf.lisp_pos_to_emacs_byte_pos(window_point))
            .get();
        let bytes = text.as_bytes();
        let begv = accessible.start().get();

        // Go to beginning of current line.
        let mut pos = pt;
        while pos > begv && bytes[pos - 1] != b'\n' {
            pos -= 1;
        }
        // Move backward target_line lines.
        for _ in 0..target_line {
            if pos <= begv {
                break;
            }
            pos -= 1;
            while pos > begv && bytes[pos - 1] != b'\n' {
                pos -= 1;
            }
        }

        // Set window-start.
        let pos_lisp = buf
            .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(pos))
            .as_i64();
        if let Some(clamped) = clamped_window_position_in_state(frames, buffers, fid, wid, pos_lisp)
        {
            if let Some(window) = frames
                .get_mut(fid)
                .and_then(|frame| frame.find_window_mut(wid))
            {
                crate::window::window_markers::set_window_start_with_marker(
                    buffers, window, clamped,
                );
                if let Window::Leaf {
                    window_end_valid,
                    vscroll,
                    preserve_vscroll_p,
                    ..
                } = window
                {
                    *window_end_valid = false;
                    *vscroll = 0;
                    *preserve_vscroll_p = false;
                }
            }
        }
    } // end borrow scope

    // Run window-scroll-functions hook after recenter
    let _ = builtin_run_window_scroll_functions(eval, vec![]);
    eval.invalidate_redisplay();
    Ok(Value::NIL)
}
/// `(iconify-frame &optional FRAME)` -> nil.
pub(crate) fn builtin_iconify_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("iconify-frame", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    set_frame_visibility(eval, fid, false)?;
    Ok(Value::NIL)
}

/// Single source of truth for frame visibility changes.
///
/// Mirrors GNU Emacs's design where `gui_set_visibility` delegates to
/// `Fmake_frame_invisible` / `Fmake_frame_visible`, ensuring the host
/// notification side effect always fires regardless of the entry point.
///
/// All callers — `iconify-frame`, `make-frame-invisible`,
/// `make-frame-visible`, and `modify-frame-parameters` with `visibility`
/// — must route through this function to guarantee the display runtime
/// receives `RemoveChildFrame` when a GUI child frame becomes invisible.
fn set_frame_visibility(
    eval: &mut super::eval::Context,
    fid: FrameId,
    visible: bool,
) -> Result<(), Flow> {
    let was_visible = eval.frames.get(fid).is_some_and(|f| f.visible);
    if visible {
        if let Some(frame) = eval.frames.get_mut(fid) {
            frame.visible = true;
            if frame.parent_frame.as_frame_id().is_some() {
                eval.frames.raise_or_lower_child_frame(fid, true);
            }
        }
    } else if was_visible {
        if let Some(frame) = eval.frames.get_mut(fid) {
            frame.visible = false;
        }
        // Notify display runtime: GUI child frames need RemoveChildFrame.
        let is_gui_child_frame = eval.frames.get(fid).is_some_and(|frame| {
            frame.effective_window_system().is_some() && frame.parent_frame.as_frame_id().is_some()
        });
        if is_gui_child_frame && let Some(host) = eval.display_host.as_mut() {
            host.remove_gui_child_frame(fid)
                .map_err(|message| signal("error", vec![Value::string(message)]))?;
        }
    }
    Ok(())
}

fn mru_rooted_frame_in_state(frames: &FrameManager, hidden: FrameId) -> FrameId {
    let root = frames.root_frame_id(hidden).unwrap_or(hidden);
    let mut best: Option<(i64, FrameId)> = None;

    for candidate in frames.frames_in_reverse_z_order(root, true) {
        if candidate == hidden {
            continue;
        }
        let Some(frame) = frames.get(candidate) else {
            continue;
        };
        let use_time = frames.window_use_time(frame.selected_window);
        if best.is_none_or(|(best_time, _)| use_time > best_time) {
            best = Some((use_time, candidate));
        }
    }

    best.map(|(_, frame_id)| frame_id).unwrap_or(root)
}

/// `(make-frame-invisible &optional FRAME FORCE)` -> nil.
///
/// Mirrors GNU Emacs `Fmake_frame_invisible` (`src/frame.c`): a TTY
/// top-level frame is not made invisible, but a TTY child frame is hidden and
/// selection moves back to the most-recently-used frame with the same root.
pub(crate) fn builtin_make_frame_invisible(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("make-frame-invisible", &args, 2)?;
    let fid = {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?
    };
    let force = args.get(1).copied().unwrap_or(Value::NIL).is_truthy();
    if !force && !other_frames_in_state(eval, fid, false) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to make invisible the sole visible or iconified frame",
            )],
        ));
    }

    let (is_tty_child, is_window_frame) = eval
        .frames
        .get(fid)
        .map(|frame| {
            (
                frame.effective_window_system().is_none()
                    && frame.parent_frame.as_frame_id().is_some(),
                frame.effective_window_system().is_some(),
            )
        })
        .ok_or_else(|| {
            signal(
                "wrong-type-argument",
                vec![
                    Value::symbol("frame-live-p"),
                    args.first().copied().unwrap_or(Value::NIL),
                ],
            )
        })?;

    if is_tty_child || is_window_frame {
        set_frame_visibility(eval, fid, false)?;
        if is_tty_child
            && eval
                .frames
                .selected_frame()
                .is_some_and(|frame| frame.id == fid)
        {
            let fallback = mru_rooted_frame_in_state(&eval.frames, fid);
            if fallback != fid {
                if let Some(old_fid) = eval.frames.selected_frame().map(|frame| frame.id) {
                    remember_selected_window_point_in_state(
                        &mut eval.frames,
                        &mut eval.buffers,
                        old_fid,
                    );
                }
                if eval.frames.select_frame(fallback) {
                    if let Some(selected_wid) =
                        eval.frames.get(fallback).map(|frame| frame.selected_window)
                    {
                        let _ = eval.frames.note_window_selected(selected_wid);
                    }
                    sync_selected_window_buffer_in_state(
                        &mut eval.frames,
                        &mut eval.buffers,
                        fallback,
                    );
                    eval.sync_keyboard_terminal_owner();
                }
            }
        }
    }

    eval.invalidate_redisplay();
    Ok(Value::NIL)
}

/// `(make-frame-visible &optional FRAME)` -> frame.
pub(crate) fn builtin_make_frame_visible(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("make-frame-visible", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    // Ensure the frame exists.
    if eval.frames.get(fid).is_none() {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    }
    set_frame_visibility(eval, fid, true)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::make_frame(frame.id.0))
}

// ===========================================================================
// Frame operations
// ===========================================================================
/// `(selected-frame)` -> frame object.
pub(crate) fn builtin_selected_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    selected_frame_impl(&mut eval.frames, &mut eval.buffers, args)
}

pub(crate) fn selected_frame_impl(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("selected-frame", &args, 0)?;
    let fid = ensure_selected_frame_id_in_state(frames, buffers);
    Ok(Value::make_frame(fid.0))
}
/// `(select-frame FRAME &optional NORECORD)` -> frame.
pub(crate) fn builtin_select_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("select-frame", &args, 1)?;
    expect_max_args("select-frame", &args, 2)?;
    let fid = match args[0].kind() {
        ValueKind::Fixnum(n) => {
            let fid = FrameId(n as u64);
            if frames.get(fid).is_none() {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("frame-live-p"), Value::fixnum(n)],
                ));
            }
            fid
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            let raw_id = args[0].as_frame_id().unwrap();
            let fid = FrameId(raw_id);
            if frames.get(fid).is_none() {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("frame-live-p"), Value::make_frame(raw_id)],
                ));
            }
            fid
        }
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), args[0]],
            ));
        }
    };
    if let Some(old_fid) = frames.selected_frame().map(|frame| frame.id) {
        remember_selected_window_point_in_state(frames, buffers, old_fid);
    }
    if !frames.select_frame(fid) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), args[0]],
        ));
    }
    if args.get(1).is_none_or(|v| v.is_nil()) {
        if let Some(selected_wid) = frames.get(fid).map(|f| f.selected_window) {
            let _ = frames.note_window_selected(selected_wid);
        }
    }
    sync_selected_window_buffer_in_state(frames, buffers, fid);
    eval.sync_keyboard_terminal_owner();
    Ok(Value::make_frame(fid.0))
}
/// `(select-frame-set-input-focus FRAME &optional NORECORD)` -> nil.
pub(crate) fn builtin_select_frame_set_input_focus(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_min_args("select-frame-set-input-focus", &args, 1)?;
    expect_max_args("select-frame-set-input-focus", &args, 2)?;
    let fid = match args[0].kind() {
        ValueKind::Fixnum(n) => {
            let fid = FrameId(n as u64);
            if frames.get(fid).is_none() {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("frame-live-p"), Value::fixnum(n)],
                ));
            }
            fid
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            let raw_id = args[0].as_frame_id().unwrap();
            let fid = FrameId(raw_id);
            if frames.get(fid).is_none() {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("frame-live-p"), Value::make_frame(raw_id)],
                ));
            }
            fid
        }
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), args[0]],
            ));
        }
    };
    if let Some(old_fid) = frames.selected_frame().map(|frame| frame.id) {
        remember_selected_window_point_in_state(frames, buffers, old_fid);
    }
    if !frames.select_frame(fid) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), args[0]],
        ));
    }
    if args.get(1).is_none_or(|v| v.is_nil()) {
        if let Some(selected_wid) = frames.get(fid).map(|f| f.selected_window) {
            let _ = frames.note_window_selected(selected_wid);
        }
    }
    sync_selected_window_buffer_in_state(frames, buffers, fid);
    eval.sync_keyboard_terminal_owner();
    Ok(Value::NIL)
}
/// `(frame-list)` -> list of frame objects.
pub(crate) fn builtin_frame_list(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("frame-list", &args, 0)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let ids: Vec<Value> = frames
        .frame_list()
        .into_iter()
        .map(|fid| Value::make_frame(fid.0))
        .collect();
    Ok(Value::list(ids))
}
/// `(visible-frame-list)` -> list of visible frame objects.
pub(crate) fn builtin_visible_frame_list(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("visible-frame-list", &args, 0)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let mut frame_ids = frames.frame_list();
    frame_ids.sort_by_key(|fid| fid.0);
    let visible = frame_ids
        .into_iter()
        .filter(|fid| frames.get(*fid).is_some_and(|frame| frame.visible))
        .map(|fid| Value::make_frame(fid.0))
        .collect::<Vec<_>>();
    Ok(Value::list(visible))
}
/// `(frame-char-height &optional FRAME)` -> integer.
///
/// GNU Emacs returns the default character height in pixels for FRAME.
pub(crate) fn builtin_frame_char_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-char-height", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "framep")?;
    let ch = frames.get(fid).map(|f| f.char_height as i64).unwrap_or(16);
    Ok(Value::fixnum(ch))
}
/// `(frame-char-width &optional FRAME)` -> integer.
///
/// GNU Emacs returns the default character width in pixels for FRAME.
pub(crate) fn builtin_frame_char_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-char-width", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "framep")?;
    let cw = frames.get(fid).map(|f| f.char_width as i64).unwrap_or(8);
    Ok(Value::fixnum(cw))
}
/// `(frame-native-height &optional FRAME)` -> integer.
pub(crate) fn builtin_frame_native_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-native-height", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(if frame_uses_window_system_pixels(frame) {
        frame.height as i64
    } else {
        frame_total_lines(frame)
    }))
}
/// `(frame-native-width &optional FRAME)` -> integer.
pub(crate) fn builtin_frame_native_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-native-width", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let uses_window_system_pixels = frame_uses_window_system_pixels(frame);
    if std::env::var("NEOMACS_TRACE_FRAME_GEOMETRY")
        .ok()
        .is_some_and(|value| value == "1")
    {
        tracing::debug!(
            "frame-native-width: fid={:?} selected={:?} size={}x{} uses_pixels={} effective_ws={:?} param_ws={:?}",
            fid,
            eval.frames.selected_frame().map(|selected| selected.id),
            frame.width,
            frame.height,
            uses_window_system_pixels,
            frame.effective_window_system(),
            frame.parameter("window-system")
        );
    }
    Ok(Value::fixnum(if uses_window_system_pixels {
        frame.width as i64
    } else {
        frame_total_cols(frame)
    }))
}
/// `(frame-text-cols &optional FRAME)` -> integer.
pub(crate) fn builtin_frame_text_cols(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-text-cols", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(frame_total_cols(frame)))
}
/// `(frame-text-lines &optional FRAME)` -> integer.
pub(crate) fn builtin_frame_text_lines(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-text-lines", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(if frame_uses_window_system_pixels(frame) {
        let char_height = frame.char_height.max(1.0);
        ((frame_text_height_pixels(frame) as f32) / char_height)
            .floor()
            .max(1.0) as i64
    } else {
        frame_text_lines(frame)
    }))
}
/// `(frame-text-width &optional FRAME)` -> integer.
///
/// GNU Emacs returns the text area width in pixels.
pub(crate) fn builtin_frame_text_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-text-width", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(if frame_uses_window_system_pixels(frame) {
        frame_text_width_pixels_in_state(&eval.frames, fid) as i64
    } else {
        frame_text_cols(frame)
    }))
}
/// `(frame-text-height &optional FRAME)` -> integer.
///
/// GNU Emacs returns the text area height in pixels.
pub(crate) fn builtin_frame_text_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-text-height", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(if frame_uses_window_system_pixels(frame) {
        frame_text_height_pixels(frame) as i64
    } else {
        frame_text_lines(frame)
    }))
}
/// `(frame-total-cols &optional FRAME)` -> integer.
pub(crate) fn builtin_frame_total_cols(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-total-cols", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(frame_total_cols(frame)))
}
/// `(frame-total-lines &optional FRAME)` -> integer.
pub(crate) fn builtin_frame_total_lines(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-total-lines", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    sync_live_gui_resize_for_geometry_queries(eval, fid)?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(frame_total_lines(frame)))
}
/// `(frame-position &optional FRAME)` -> (X . Y).
pub(crate) fn builtin_frame_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-position", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::cons(
        Value::fixnum(frame.left_pos),
        Value::fixnum(frame.top_pos),
    ))
}
/// `(set-frame-height FRAME HEIGHT &optional PRETEND PIXELWISE)` -> nil.
pub(crate) fn builtin_set_frame_height(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-frame-height", &args, 2)?;
    expect_max_args("set-frame-height", &args, 4)?;
    let fid = resolve_frame_id_in_state(
        &mut ctx.frames,
        &mut ctx.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let pretend = args.get(2).is_some_and(|v| v.is_truthy());
    let pixelwise = args.get(3).is_some_and(|v| v.is_truthy());
    let (current_text_width_px, char_height, uses_window_system_pixels) = {
        let frame = &mut ctx
            .frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        (
            frame_text_width_pixels_in_state(&ctx.frames, fid),
            frame.char_height,
            frame_uses_window_system_pixels(frame),
        )
    };
    let text_height_px = check_frame_pixels(&args[1], pixelwise, char_height)?;
    if uses_window_system_pixels {
        if ctx.display_host.is_some() && !pretend {
            let desired_cols = {
                let frame = ctx
                    .frames
                    .get(fid)
                    .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
                frame_total_cols(frame)
            };
            let desired_total_lines = ((text_height_px as f32) / char_height.max(1.0))
                .floor()
                .max(1.0) as i64;
            request_live_gui_frame_resize_and_keep_pending(
                &mut ctx.frames,
                &ctx.buffers,
                &mut ctx.display_host,
                fid,
                if pixelwise {
                    FrameResizeRequest::TextPixels {
                        width: current_text_width_px,
                        height: text_height_px,
                    }
                } else {
                    FrameResizeRequest::Cells {
                        cols: desired_cols,
                        total_lines: desired_total_lines,
                    }
                },
            )?;
        } else {
            request_live_gui_frame_resize(
                &mut ctx.frames,
                &ctx.buffers,
                &mut ctx.display_host,
                fid,
                current_text_width_px,
                text_height_px,
                pretend,
            )?;
        }
    } else {
        let cols = {
            let frame = &mut ctx
                .frames
                .get(fid)
                .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
            frame_total_cols(frame)
        };
        let text_lines = ((text_height_px as f32) / char_height.max(1.0))
            .floor()
            .max(1.0) as i64;
        let frame = &mut ctx
            .frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        set_frame_text_size(frame, cols, text_lines);
    }
    Ok(Value::NIL)
}
/// `(set-frame-width FRAME WIDTH &optional PRETEND PIXELWISE)` -> nil.
pub(crate) fn builtin_set_frame_width(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-frame-width", &args, 2)?;
    expect_max_args("set-frame-width", &args, 4)?;
    let fid = resolve_frame_id_in_state(
        &mut ctx.frames,
        &mut ctx.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let pretend = args.get(2).is_some_and(|v| v.is_truthy());
    let pixelwise = args.get(3).is_some_and(|v| v.is_truthy());
    let (current_text_height_px, char_width, uses_window_system_pixels) = {
        let frame = &mut ctx
            .frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        (
            frame_text_height_pixels(frame),
            frame.char_width,
            frame_uses_window_system_pixels(frame),
        )
    };
    let text_width_px = check_frame_pixels(&args[1], pixelwise, char_width)?;
    if uses_window_system_pixels {
        if ctx.display_host.is_some() && !pretend {
            let desired_cols = ((text_width_px as f32) / char_width.max(1.0))
                .floor()
                .max(1.0) as i64;
            let desired_total_lines = {
                let frame = ctx
                    .frames
                    .get(fid)
                    .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
                frame_total_lines(frame)
            };
            request_live_gui_frame_resize_and_keep_pending(
                &mut ctx.frames,
                &ctx.buffers,
                &mut ctx.display_host,
                fid,
                if pixelwise {
                    FrameResizeRequest::TextPixels {
                        width: text_width_px,
                        height: current_text_height_px,
                    }
                } else {
                    FrameResizeRequest::Cells {
                        cols: desired_cols,
                        total_lines: desired_total_lines,
                    }
                },
            )?;
        } else {
            request_live_gui_frame_resize(
                &mut ctx.frames,
                &ctx.buffers,
                &mut ctx.display_host,
                fid,
                text_width_px,
                current_text_height_px,
                pretend,
            )?;
        }
    } else {
        let text_lines = {
            let frame = &mut ctx
                .frames
                .get(fid)
                .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
            frame_text_lines(frame)
        };
        let cols = ((text_width_px as f32) / char_width.max(1.0))
            .floor()
            .max(1.0) as i64;
        let frame = &mut ctx
            .frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        set_frame_text_size(frame, cols, text_lines);
    }
    Ok(Value::NIL)
}
/// `(set-frame-size FRAME WIDTH HEIGHT &optional PIXELWISE)` -> nil.
pub(crate) fn builtin_set_frame_size(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-frame-size", &args, 3)?;
    expect_max_args("set-frame-size", &args, 4)?;
    let fid = resolve_frame_id_in_state(
        &mut ctx.frames,
        &mut ctx.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let pixelwise = args.get(3).is_some_and(|v| v.is_truthy());
    let (char_width, char_height, uses_window_system_pixels) = {
        let frame = &mut ctx
            .frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        (
            frame.char_width,
            frame.char_height,
            frame_uses_window_system_pixels(frame),
        )
    };
    let text_width_px = check_frame_pixels(&args[1], pixelwise, char_width)?;
    let text_height_px = check_frame_pixels(&args[2], pixelwise, char_height)?;
    tracing::debug!(
        "set-frame-size: fid={:?} pixelwise={} gui={} requested_text={}x{} char={}x{}",
        fid,
        pixelwise,
        uses_window_system_pixels,
        text_width_px,
        text_height_px,
        char_width,
        char_height
    );
    if uses_window_system_pixels {
        let desired_cols = ((text_width_px as f32) / char_width.max(1.0))
            .floor()
            .max(1.0) as i64;
        let desired_total_lines = ((text_height_px as f32) / char_height.max(1.0))
            .floor()
            .max(1.0) as i64;
        if ctx.display_host.is_some() {
            request_live_gui_frame_resize_and_keep_pending(
                &mut ctx.frames,
                &ctx.buffers,
                &mut ctx.display_host,
                fid,
                if pixelwise {
                    FrameResizeRequest::TextPixels {
                        width: text_width_px,
                        height: text_height_px,
                    }
                } else {
                    FrameResizeRequest::Cells {
                        cols: desired_cols,
                        total_lines: desired_total_lines,
                    }
                },
            )?;
        } else {
            request_live_gui_frame_resize(
                &mut ctx.frames,
                &ctx.buffers,
                &mut ctx.display_host,
                fid,
                text_width_px,
                text_height_px,
                false,
            )?;
        }
    } else {
        let cols = ((text_width_px as f32) / char_width.max(1.0))
            .floor()
            .max(1.0) as i64;
        let text_lines = ((text_height_px as f32) / char_height.max(1.0))
            .floor()
            .max(1.0) as i64;
        let frame = &mut ctx
            .frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        tracing::debug!(
            "set-frame-size: non-gui fallback fid={:?} cols={} text_lines={}",
            fid,
            cols,
            text_lines
        );
        set_frame_text_size(frame, cols, text_lines);
    }
    Ok(Value::NIL)
}
/// `(set-frame-position FRAME X Y)` -> t.
pub(crate) fn builtin_set_frame_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-frame-position", &args, 3)?;
    let fid = resolve_frame_id_in_state(frames, buffers, Some(&args[0]), "frame-live-p")?;
    let x = expect_int(&args[1])?;
    let y = expect_int(&args[2])?;
    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    frame.left_pos = x;
    frame.top_pos = y;
    frame.set_parameter(Value::symbol("left"), Value::fixnum(x));
    frame.set_parameter(Value::symbol("top"), Value::fixnum(y));
    Ok(Value::T)
}

/// `(make-frame &optional PARAMETERS)` -> frame id.
///
/// GNU Emacs routes GUI frame creation through `x-create-frame` and keeps
/// terminal-only creation on the plain frame path. NeoVM mirrors that split:
/// if the current runtime has an active GUI display host (or the caller
/// explicitly requests a GUI window-system), delegate to the GUI boundary;
/// otherwise create a plain frame directly.
pub(crate) fn builtin_make_frame(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    // GNU creates a TTY frame via make-terminal-frame -> init_tty when there is
    // no window system, which signals "Unknown terminal type" if the terminal
    // has no type (e.g. --batch). Mirror that: with no GUI display host and no
    // usable terminal, refuse rather than fabricate a frame.
    if eval.display_host.is_none() && !super::terminal::pure::selected_terminal_is_usable_tty(eval)
    {
        return Err(signal(
            "error",
            vec![Value::string("Unknown terminal type")],
        ));
    }
    let backend = resolve_make_frame_backend_request(args.first(), eval.display_host.is_some());
    tracing::debug!(
        "builtin_make_frame: backend={backend:?} display_host_available={} args={:?}",
        eval.display_host.is_some(),
        args
    );
    if backend == MakeFrameBackend::Gui {
        eval.sync_pending_resize_events();
    }
    let result = make_frame_with_state(
        &mut eval.frames,
        &mut eval.buffers,
        &mut eval.display_host,
        args,
    );
    eval.sync_keyboard_terminal_owner();
    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MakeFrameBackend {
    Plain,
    Gui,
}

fn resolve_make_frame_backend_request(
    params: Option<&Value>,
    display_host_available: bool,
) -> MakeFrameBackend {
    let Some(params) = params else {
        return if display_host_available {
            MakeFrameBackend::Gui
        } else {
            MakeFrameBackend::Plain
        };
    };
    let Some(items) = super::value::list_to_vec(params) else {
        return if display_host_available {
            MakeFrameBackend::Gui
        } else {
            MakeFrameBackend::Plain
        };
    };

    let mut requested_window_system = None;
    let mut requested_display = false;
    let mut requested_terminal = false;

    for item in items {
        if !item.is_cons() {
            continue;
        };
        let pair_car = item.cons_car();
        let pair_cdr = item.cons_cdr();
        let Some(key) = pair_car.as_symbol_name() else {
            continue;
        };
        match key {
            "window-system" => requested_window_system = Some(!pair_cdr.is_nil()),
            "display" => requested_display = !pair_cdr.is_nil(),
            "terminal" => requested_terminal = !pair_cdr.is_nil(),
            _ => {}
        }
    }

    if requested_terminal || matches!(requested_window_system, Some(false)) {
        return MakeFrameBackend::Plain;
    }

    if requested_display || matches!(requested_window_system, Some(true)) {
        return MakeFrameBackend::Gui;
    }

    if display_host_available {
        MakeFrameBackend::Gui
    } else {
        MakeFrameBackend::Plain
    }
}

pub(crate) fn make_frame_with_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    args: Vec<Value>,
) -> EvalResult {
    let backend = resolve_make_frame_backend_request(args.first(), display_host.is_some());
    tracing::debug!(
        "make_frame_with_state: backend={backend:?} display_host_available={} args={:?}",
        display_host.is_some(),
        args
    );
    if backend == MakeFrameBackend::Gui {
        if display_host.is_none() {
            return Err(signal(
                "error",
                vec![Value::string("GUI frame creation requires a display host")],
            ));
        }
        let gui_args = vec![args.first().copied().unwrap_or(Value::NIL)];
        return x_create_frame_impl(frames, buffers, display_host, gui_args);
    }
    make_frame_plain(frames, buffers, args)
}

fn resolve_child_shared_minibuffer(
    frames: &FrameManager,
    parent_id: FrameId,
    minibuffer_param: Option<Value>,
) -> Result<Option<WindowId>, Flow> {
    let Some(minibuffer_param) = minibuffer_param else {
        return Ok(None);
    };

    if minibuffer_param.is_nil() || matches!(minibuffer_param.as_symbol_name(), Some("none")) {
        return Ok(frames
            .root_frame_id(parent_id)
            .and_then(|root_id| frames.get(root_id))
            .and_then(|root| root.minibuffer_window));
    }

    let Some(raw_window_id) = minibuffer_param.as_window_id() else {
        return Ok(None);
    };
    let window_id = WindowId(raw_window_id);
    let valid_minibuffer = frames
        .find_valid_window_frame_id(window_id)
        .and_then(|frame_id| {
            let owner = frames.get(frame_id)?;
            (owner.minibuffer_window == Some(window_id)
                && frames.root_frame_id(frame_id) == frames.root_frame_id(parent_id))
            .then_some(())
        })
        .is_some();
    if valid_minibuffer {
        Ok(Some(window_id))
    } else {
        Err(signal(
            "error",
            vec![Value::string(
                "The `minibuffer' parameter does not specify a valid minibuffer window",
            )],
        ))
    }
}

/// `(make-terminal-frame PARMS)` -> frame.
pub(crate) fn builtin_make_terminal_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("make-terminal-frame", &args, 1)?;
    if !args[0].is_nil() && !args[0].is_cons() {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), args[0]],
        ));
    }
    // GNU `Fmake_terminal_frame` -> `init_tty` signals "Unknown terminal type"
    // when the terminal has no type (--batch). Mirror that.
    if eval.display_host.is_none() && !super::terminal::pure::selected_terminal_is_usable_tty(eval)
    {
        return Err(signal(
            "error",
            vec![Value::string("Unknown terminal type")],
        ));
    }
    make_frame_plain(&mut eval.frames, &mut eval.buffers, args)
}

fn make_frame_plain(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("make-frame", &args, 1)?;
    let mut width: u32 = 800;
    let mut height: u32 = 600;
    let mut requested_width = None;
    let mut requested_height = None;
    let mut name = Value::string("F");
    let mut all_params: Vec<(Value, Value)> = Vec::new();
    let mut parent_frame = Value::NIL;
    let mut left = 0_i64;
    let mut top = 0_i64;
    let mut visibility = None;
    let mut minibuffer_param = None;
    let mut undecorated = false;
    let mut no_accept_focus = false;
    let mut no_split = false;

    // Parse optional alist parameters.
    if let Some(params) = args.first() {
        if let Some(items) = super::value::list_to_vec(params) {
            for item in &items {
                if item.is_cons() {
                    let pair_car = item.cons_car();
                    let pair_cdr = item.cons_cdr();
                    if let Some(key) = pair_car.as_symbol_id() {
                        all_params.push((pair_car, pair_cdr));
                        match resolve_sym(key) {
                            "width" => {
                                if let Some(size) = parse_frame_size_param(pair_cdr) {
                                    requested_width = Some(size);
                                    width = frame_size_param_to_cells(size, 1.0).max(1) as u32;
                                }
                            }
                            "height" => {
                                if let Some(size) = parse_frame_size_param(pair_cdr) {
                                    requested_height = Some(size);
                                    height = frame_size_param_to_cells(size, 1.0).max(1) as u32;
                                }
                            }
                            "name" => {
                                if let Some(value) = frame_name_parameter_value(&pair_cdr) {
                                    name = value;
                                }
                            }
                            "parent-frame" => {
                                if pair_cdr
                                    .as_frame_id()
                                    .map(|id| frames.get(FrameId(id)).is_some())
                                    .unwrap_or(false)
                                {
                                    parent_frame = pair_cdr;
                                }
                            }
                            "left" => {
                                if let Some(n) = pair_cdr.as_int() {
                                    left = n;
                                }
                            }
                            "top" => {
                                if let Some(n) = pair_cdr.as_int() {
                                    top = n;
                                }
                            }
                            "visibility" => visibility = Some(pair_cdr.is_truthy()),
                            "minibuffer" => minibuffer_param = Some(pair_cdr),
                            "undecorated" => undecorated = pair_cdr.is_truthy(),
                            "no-accept-focus" => no_accept_focus = pair_cdr.is_truthy(),
                            "unsplittable" => no_split = pair_cdr.is_truthy(),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let parent_id = parent_frame.as_frame_id().map(FrameId);
    if let Some(parent_id) = parent_id {
        let metrics = frames.get(parent_id).map(|parent| {
            (
                parent.terminal_id,
                parent.char_width.max(1.0),
                parent.char_height.max(1.0),
                parent.font_pixel_size.max(1.0),
            )
        });
        if let Some((terminal_id, char_width, char_height, font_pixel_size)) = metrics {
            if let Some(size) = requested_width {
                width = frame_size_param_to_cells(size, char_width).max(1) as u32;
            }
            if let Some(size) = requested_height {
                height = frame_size_param_to_cells(size, char_height).max(1) as u32;
            }
            width = width.max(1);
            height = height.max(1);
            let buf_id = buffers
                .current_buffer()
                .map(|b| b.id)
                .unwrap_or(BufferId(0));
            let fid =
                frames.create_frame_value_on_terminal(name, terminal_id, width, height, buf_id);
            let shared_minibuffer =
                resolve_child_shared_minibuffer(frames, parent_id, minibuffer_param)?;
            let z_order = 1 + frames.max_child_z_order(parent_id);
            let frame = frames
                .get_mut(fid)
                .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
            frame.parent_frame = parent_frame;
            frame.z_order = z_order;
            frame.left_pos = left;
            frame.top_pos = top;
            frame.width = width;
            frame.height = height;
            frame.char_width = char_width;
            frame.char_height = char_height;
            frame.font_pixel_size = font_pixel_size;
            frame.visible = visibility.unwrap_or(frame.visible);
            frame.undecorated = undecorated;
            frame.no_accept_focus = no_accept_focus;
            frame.no_split = no_split;
            if let Some(shared_minibuffer) = shared_minibuffer {
                frame.minibuffer_leaf = None;
                frame.minibuffer_window = Some(shared_minibuffer);
            }
            for (key, value) in all_params {
                if let Some(param_key) = FrameParamKey::from_symbol_value(key) {
                    frame.set_parameter_key(param_key, value);
                } else {
                    frame.set_parameter(key, value);
                }
            }
            frame.set_parameter(Value::symbol("width"), Value::fixnum(i64::from(width)));
            frame.set_parameter(Value::symbol("height"), Value::fixnum(i64::from(height)));
            if let Some(shared_minibuffer) = shared_minibuffer {
                frame.set_parameter(
                    Value::symbol("minibuffer"),
                    Value::make_window(shared_minibuffer.0),
                );
            }
            frame.set_known_parameter(FrameParam::ParentFrame, parent_frame);
            frame.set_parameter(Value::symbol("left"), Value::fixnum(left));
            frame.set_parameter(Value::symbol("top"), Value::fixnum(top));
            frame.sync_tab_bar_height_from_parameters();
            frame.sync_menu_bar_height_from_parameters();
            frame.sync_tool_bar_height_from_parameters();
            frame.sync_window_area_bounds();
            tracing::debug!(
                "make_frame_plain: created tty child frame {:?} parent={:?} pos={}x{} size={}x{}",
                fid,
                parent_id,
                left,
                top,
                width,
                height
            );
            return Ok(Value::make_frame(fid.0));
        }
    }

    // Use the current buffer (or BufferId(0) as fallback) for the initial window.
    if let Some(size) = requested_width {
        width = frame_size_param_to_cells(size, 1.0).max(1) as u32;
    }
    if let Some(size) = requested_height {
        height = frame_size_param_to_cells(size, 1.0).max(1) as u32;
    }
    let buf_id = buffers
        .current_buffer()
        .map(|b| b.id)
        .unwrap_or(BufferId(0));
    let fid = frames.create_frame_value(name, width, height, buf_id);
    if let Some(frame) = frames.get_mut(fid) {
        for (key, value) in all_params {
            frame.set_parameter(key, value);
        }
        frame.set_parameter(Value::symbol("width"), Value::fixnum(i64::from(width)));
        frame.set_parameter(Value::symbol("height"), Value::fixnum(i64::from(height)));
        frame.visible = visibility.unwrap_or(frame.visible);
        frame.undecorated = undecorated;
        frame.no_accept_focus = no_accept_focus;
        frame.no_split = no_split;
        frame.sync_tab_bar_height_from_parameters();
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
    }
    tracing::debug!(
        "make_frame_plain: created plain frame {:?} size={}x{} name={}",
        fid,
        width,
        height,
        name.as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .unwrap_or_default()
    );
    Ok(Value::make_frame(fid.0))
}

#[derive(Default)]
struct ParsedGuiFrameParams {
    name: Option<Value>,
    title: Option<Value>,
    width: Option<FrameSizeParam>,
    height: Option<FrameSizeParam>,
    visibility: Option<bool>,
    parent_frame: Option<FrameId>,
    left: Option<i64>,
    top: Option<i64>,
    minibuffer: Option<Value>,
    internal_border_width: Option<i64>,
    child_frame_border_width: Option<i64>,
    undecorated: bool,
    no_accept_focus: bool,
    unsplittable: bool,
    all: std::collections::HashMap<SymId, Value>,
}

#[derive(Clone, Copy)]
struct GuiFrameMetrics {
    width_px: u32,
    height_px: u32,
    char_width: f32,
    char_height: f32,
    font_pixel_size: f32,
    minibuffer_height: f32,
}

fn stringish_value(value: &Value) -> Option<Value> {
    match value.kind() {
        ValueKind::String => Some(*value),
        ValueKind::Symbol(id) => Some(Value::string(resolve_sym(id))),
        _ => None,
    }
}

fn frame_name_parameter_value(value: &Value) -> Option<Value> {
    if value.is_nil() {
        Some(Value::NIL)
    } else {
        stringish_value(value)
    }
}

fn frame_title_parameter_value(value: &Value) -> Option<Value> {
    if value.is_nil() {
        Some(Value::NIL)
    } else {
        stringish_value(value)
    }
}

fn frame_icon_name_parameter_value(value: &Value) -> Option<Value> {
    if value.is_nil() {
        Some(Value::NIL)
    } else {
        stringish_value(value)
    }
}

fn parse_gui_frame_params(value: Option<&Value>) -> ParsedGuiFrameParams {
    let mut parsed = ParsedGuiFrameParams::default();
    let Some(value) = value else {
        return parsed;
    };
    let Some(items) = list_to_vec(value) else {
        return parsed;
    };
    for item in items {
        if !item.is_cons() {
            continue;
        };
        let pair_car = item.cons_car();
        let pair_cdr = item.cons_cdr();
        let Some(key) = pair_car.as_symbol_id() else {
            continue;
        };
        parsed.all.insert(key, pair_cdr);
        match resolve_sym(key) {
            "name" => parsed.name = stringish_value(&pair_cdr),
            "title" => parsed.title = stringish_value(&pair_cdr),
            "width" => {
                parsed.width = parse_frame_size_param(pair_cdr).filter(|size| !size.is_zero());
            }
            "height" => {
                parsed.height = parse_frame_size_param(pair_cdr).filter(|size| !size.is_zero());
            }
            "visibility" => parsed.visibility = Some(pair_cdr.is_truthy()),
            "parent-frame" => {
                if let Some(id) = pair_cdr.as_frame_id() {
                    parsed.parent_frame = Some(FrameId(id));
                }
            }
            "left" => parsed.left = pair_cdr.as_int(),
            "top" => parsed.top = pair_cdr.as_int(),
            "minibuffer" => parsed.minibuffer = Some(pair_cdr),
            "internal-border-width" => parsed.internal_border_width = pair_cdr.as_int(),
            "child-frame-border-width" => parsed.child_frame_border_width = pair_cdr.as_int(),
            "undecorated" => parsed.undecorated = pair_cdr.is_truthy(),
            "no-accept-focus" => parsed.no_accept_focus = pair_cdr.is_truthy(),
            "unsplittable" => parsed.unsplittable = pair_cdr.is_truthy(),
            _ => {}
        }
    }
    parsed
}

fn parsed_effective_internal_border_width(
    parsed: &ParsedGuiFrameParams,
    is_child_frame: bool,
) -> u32 {
    if is_child_frame && let Some(width) = parsed.child_frame_border_width {
        return width.max(0) as u32;
    }
    parsed
        .internal_border_width
        .map(|width| width.max(0) as u32)
        .unwrap_or(0)
}

fn current_gui_frame_metrics(eval: &super::eval::Context) -> GuiFrameMetrics {
    current_gui_frame_metrics_in_state(&eval.frames)
}

fn current_gui_frame_metrics_in_state(frames: &FrameManager) -> GuiFrameMetrics {
    if let Some(frame) = frames.selected_frame() {
        // A frame's minibuffer defaults to a single text line (GNU
        // `make-frame` / `Fframe_char_height`); the layout-engine
        // `resize_mini_window` pass grows it on demand for multi-line
        // messages. Falling back to two lines here seeded every GUI frame
        // with a permanently two-line echo area, since grow-only never
        // shrinks an over-allocated mini-window back down.
        let minibuffer_height = frame
            .minibuffer_leaf
            .as_ref()
            .map(|leaf| leaf.bounds().height.max(frame.char_height).max(1.0))
            .unwrap_or_else(|| frame.char_height.max(1.0));
        return GuiFrameMetrics {
            width_px: frame.width.max(1),
            height_px: frame.height.max(minibuffer_height.ceil() as u32 + 1),
            char_width: frame.char_width.max(1.0),
            char_height: frame.char_height.max(1.0),
            font_pixel_size: frame.font_pixel_size.max(1.0),
            minibuffer_height,
        };
    }
    GuiFrameMetrics {
        width_px: 960,
        height_px: 640,
        char_width: 8.0,
        char_height: 16.0,
        font_pixel_size: 16.0,
        minibuffer_height: 32.0,
    }
}

fn current_primary_window_size(
    display_host: &Option<Box<dyn super::eval::DisplayHost>>,
) -> Option<super::eval::GuiFrameHostSize> {
    display_host
        .as_ref()
        .and_then(|host| host.current_primary_window_size())
        .filter(|size| size.width > 0 && size.height > 0)
}

/// `(x-create-frame PARMS)` -> frame.
///
/// GNU Emacs owns `make-frame` in Lisp and delegates the host-window boundary
/// to the C primitive `x-create-frame`.  NeoVM mirrors that split here:
/// this builtin realizes a fresh Lisp frame object and lets the frontend
/// binary decide whether to adopt the existing primary window or create a
/// new top-level OS window for it.
pub(crate) fn builtin_x_create_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    tracing::debug!(
        "builtin_x_create_frame: syncing pending resize events before frame realization"
    );
    // GNU's initial GUI frame creation observes the actual host surface
    // geometry that exists at make-frame time. Our bootstrap window can
    // already have queued resize events before Lisp reaches x-create-frame,
    // so apply them first instead of reusing stale bootstrap dimensions.
    eval.sync_pending_resize_events();
    let result = x_create_frame_impl(
        &mut eval.frames,
        &mut eval.buffers,
        &mut eval.display_host,
        args,
    );
    eval.sync_keyboard_terminal_owner();
    result
}

pub(crate) fn x_create_frame_impl(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("x-create-frame", &args, 1)?;

    let parsed = parse_gui_frame_params(args.first());
    tracing::debug!(
        "x_create_frame_impl: display_host_available={} params={:?}",
        display_host.is_some(),
        args.first()
    );
    let parent_id = parsed
        .parent_frame
        .filter(|parent_id| frames.get(*parent_id).is_some());
    let parent_frame_value = parent_id
        .map(|parent_id| Value::make_frame(parent_id.0))
        .unwrap_or(Value::NIL);
    let metrics = parent_id
        .and_then(|parent_id| frames.get(parent_id))
        .map(|parent| GuiFrameMetrics {
            width_px: parent.width.max(1),
            height_px: parent.height.max(1),
            char_width: parent.char_width.max(1.0),
            char_height: parent.char_height.max(1.0),
            font_pixel_size: parent.font_pixel_size.max(1.0),
            minibuffer_height: parent
                .minibuffer_leaf
                .as_ref()
                .map(|leaf| leaf.bounds().height.max(parent.char_height).max(1.0))
                // A frame's minibuffer defaults to one text line (GNU
                // `make-frame`); see current_gui_frame_metrics_in_state.
                .unwrap_or_else(|| parent.char_height.max(1.0)),
        })
        .unwrap_or_else(|| current_gui_frame_metrics_in_state(frames));
    let host_size = current_primary_window_size(&*display_host);
    let opening_frame_adoption = display_host
        .as_ref()
        .is_some_and(|host| host.opening_gui_frame_pending());
    let is_child_frame = parent_id.is_some();
    let internal_border_width = parsed_effective_internal_border_width(&parsed, is_child_frame);
    let width_px = parsed
        .width
        .map(|size| {
            frame_size_param_to_pixels(size, metrics.char_width)
                .saturating_add(2 * internal_border_width)
        })
        .unwrap_or_else(|| {
            if is_child_frame {
                metrics.width_px
            } else {
                host_size.map(|size| size.width).unwrap_or(metrics.width_px)
            }
        });
    let text_height_px = parsed.height.map(|size| {
        frame_size_param_to_pixels(size, metrics.char_height)
            .saturating_add(2 * internal_border_width)
    });
    let height_px = text_height_px.map(|text| text).unwrap_or_else(|| {
        if is_child_frame {
            metrics.height_px
        } else {
            host_size
                .map(|size| size.height)
                .unwrap_or(metrics.height_px)
        }
    });
    tracing::debug!(
        "x-create-frame: parsed width={:?} height={:?} host_size={:?} metrics={}x{} char={}x{} mini_h={} -> size={}x{}",
        parsed.width,
        parsed.height,
        host_size,
        metrics.width_px,
        metrics.height_px,
        metrics.char_width,
        metrics.char_height,
        metrics.minibuffer_height,
        width_px,
        height_px
    );
    let explicit_title = parsed.title;
    let host_title = explicit_title
        .and_then(|title| title.as_lisp_string().cloned())
        .or_else(|| parsed.name.and_then(|name| name.as_lisp_string().cloned()))
        .unwrap_or_else(|| crate::heap_types::LispString::from_utf8("Neomacs"));
    let name = parsed
        .name
        .unwrap_or_else(|| Value::heap_string(host_title.clone()));
    let current_buffer_id = buffers
        .current_buffer()
        .map(|buffer| buffer.id)
        .unwrap_or_else(|| buffers.create_buffer("*scratch*"));
    let shared_minibuffer = parent_id
        .map(|parent_id| resolve_child_shared_minibuffer(frames, parent_id, parsed.minibuffer))
        .transpose()?
        .flatten();
    let z_order = parent_id.map(|parent_id| 1 + frames.max_child_z_order(parent_id));
    let minibuffer_buffer_id = buffers.find_buffer_by_name(" *Minibuf-0*");
    let fid = frames.create_frame_value(name, width_px, height_px, current_buffer_id);
    {
        let frame = frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        frame.set_name_value(name);
        if let Some(title) = explicit_title {
            frame.set_title_value(title);
        } else {
            frame.clear_title();
        }
        frame.width = width_px;
        frame.height = height_px;
        frame.visible = parsed.visibility.unwrap_or(frame.visible);
        frame.parent_frame = parent_frame_value;
        if let Some(z_order) = z_order {
            frame.z_order = z_order;
        }
        frame.left_pos = parsed.left.unwrap_or(0);
        frame.top_pos = parsed.top.unwrap_or(0);
        frame.undecorated = parsed.undecorated;
        frame.no_accept_focus = parsed.no_accept_focus;
        frame.no_split = parsed.unsplittable;
        frame.char_width = metrics.char_width;
        frame.char_height = metrics.char_height;
        frame.font_pixel_size = metrics.font_pixel_size;
        frame.set_window_system(Some(Value::symbol(
            crate::emacs_core::display::gui_window_system_symbol(),
        )));
        frame.set_parameter(Value::symbol("display-type"), Value::symbol("color"));
        frame.set_parameter(Value::symbol("background-mode"), Value::symbol("dark"));
        frame.install_gnu_gui_default_parameters();
        for (key, value) in parsed.all {
            frame.set_parameter_key(FrameParamKey::from_symbol_id(key), value);
        }
        frame.set_known_parameter(FrameParam::ParentFrame, parent_frame_value);
        frame.set_parameter(Value::symbol("left"), Value::fixnum(frame.left_pos));
        frame.set_parameter(Value::symbol("top"), Value::fixnum(frame.top_pos));
        if let Some(shared_minibuffer) = shared_minibuffer {
            frame.minibuffer_leaf = None;
            frame.minibuffer_window = Some(shared_minibuffer);
            frame.set_parameter(
                Value::symbol("minibuffer"),
                Value::make_window(shared_minibuffer.0),
            );
        }
        if let Window::Leaf { buffer_id, .. } = &mut frame.root_window {
            *buffer_id = current_buffer_id;
        }
        if let Some(minibuffer_leaf) = frame.minibuffer_leaf.as_mut() {
            if let Some(minibuffer_buffer_id) = minibuffer_buffer_id {
                minibuffer_leaf.set_buffer(minibuffer_buffer_id);
            }
            minibuffer_leaf.set_bounds(Rect::new(
                0.0,
                0.0,
                width_px as f32,
                metrics.minibuffer_height.min(height_px as f32),
            ));
        }
        // x-create-frame builds a GUI frame that is shown, so its menu/tab/tool
        // bars occupy window rows (GNU realizes FRAME_TOP_MARGIN only on shown
        // frames). Mark it displaying chrome before the area reflow so the bar
        // rows are reserved above the root window.
        frame.displays_chrome = true;
        frame.sync_tab_bar_height_from_parameters();
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
        frame.sync_window_area_bounds();
    }
    if !is_child_frame && let Some(host) = display_host.as_mut() {
        let geometry_hints = frames
            .get(fid)
            .map(|frame| frame.gui_geometry_hints())
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        host.realize_gui_frame(super::eval::GuiFrameHostRequest {
            frame_id: fid,
            width: width_px,
            height: height_px,
            title: host_title,
            geometry_hints,
        })
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    }
    if !is_child_frame && opening_frame_adoption {
        frames.select_frame(fid);
        if let Some(selected_wid) = frames.get(fid).map(|frame| frame.selected_window) {
            let _ = frames.note_window_selected(selected_wid);
        }
        buffers.switch_current(current_buffer_id);
    }
    Ok(Value::make_frame(fid.0))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeleteFrameMode {
    Public { force_non_nil: bool },
    Noelisp,
}

impl DeleteFrameMode {
    fn runs_hooks_immediately(self) -> bool {
        matches!(self, Self::Public { .. })
    }

    fn force_non_nil(self) -> bool {
        match self {
            Self::Public { force_non_nil } => force_non_nil,
            Self::Noelisp => true,
        }
    }

    fn bypasses_only_frame_check(self) -> bool {
        matches!(self, Self::Noelisp)
    }

    fn allows_terminal_cascade(self) -> bool {
        matches!(self, Self::Public { .. })
    }
}

fn other_frames_in_state(
    eval: &super::eval::Context,
    deleting: crate::window::FrameId,
    include_invisible: bool,
) -> bool {
    eval.frames
        .frame_list()
        .into_iter()
        .filter(|frame_id| *frame_id != deleting)
        .filter(|frame_id| eval.frames.frame_parent_id(*frame_id).is_none())
        .any(|frame_id| {
            eval.frames
                .get(frame_id)
                .is_some_and(|frame| include_invisible || frame.visible)
        })
}

fn direct_child_frame_ids(
    eval: &super::eval::Context,
    parent_id: crate::window::FrameId,
) -> Vec<FrameId> {
    eval.frames
        .frame_list()
        .into_iter()
        .filter(|frame_id| eval.frames.frame_parent_id(*frame_id) == Some(parent_id))
        .collect()
}

pub(crate) fn delete_frame_owned(
    eval: &mut super::eval::Context,
    fid: crate::window::FrameId,
    mode: DeleteFrameMode,
) -> EvalResult {
    if eval.frames.get(fid).is_none() {
        return Ok(Value::NIL);
    }
    let force_non_nil = mode.force_non_nil();
    if !mode.bypasses_only_frame_check() && !other_frames_in_state(eval, fid, force_non_nil) {
        return Err(signal(
            "error",
            vec![Value::string(if force_non_nil {
                "Attempt to delete the only frame"
            } else {
                "Attempt to delete the sole visible or iconified frame"
            })],
        ));
    }
    for child_id in direct_child_frame_ids(eval, fid) {
        if eval.frames.get(child_id).is_some() {
            let _ = delete_frame_owned(
                eval,
                child_id,
                DeleteFrameMode::Public {
                    force_non_nil: false,
                },
            )?;
        }
    }
    if eval.frames.get(fid).is_none() {
        return Ok(Value::NIL);
    }
    let terminal_id = eval
        .frames
        .get(fid)
        .map(|frame| frame.terminal_id)
        .unwrap_or(crate::emacs_core::terminal::pure::TERMINAL_ID);
    let was_gui_child_frame = eval.frames.get(fid).is_some_and(|frame| {
        frame.effective_window_system().is_some() && frame.parent_frame.as_frame_id().is_some()
    });
    let was_top_level_gui_frame = eval.frames.get(fid).is_some_and(|frame| {
        frame.effective_window_system().is_some() && frame.parent_frame.as_frame_id().is_none()
    });
    let frame_value = Value::make_frame(fid.0);
    if mode.runs_hooks_immediately() {
        let delete_hook =
            crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "delete-frame-functions");
        let _ = crate::emacs_core::hook_runtime::safe_run_named_hook(
            eval,
            delete_hook,
            &[frame_value],
        )?;
    } else {
        eval.queue_pending_safe_hook("delete-frame-functions", &[frame_value]);
    }
    if eval.frames.get(fid).is_none() {
        return Ok(Value::NIL);
    }
    if !eval.frames.delete_frame(fid) {
        return Err(signal("error", vec![Value::string("Cannot delete frame")]));
    }
    if let Some(host) = eval.display_host.as_mut() {
        if was_gui_child_frame {
            host.remove_gui_child_frame(fid)
                .map_err(|message| signal("error", vec![Value::string(message)]))?;
        } else if was_top_level_gui_frame {
            host.destroy_gui_frame(fid)
                .map_err(|message| signal("error", vec![Value::string(message)]))?;
        }
    }
    let terminal_is_empty = eval.frames.frame_list().into_iter().all(|frame_id| {
        eval.frames
            .get(frame_id)
            .is_none_or(|frame| frame.terminal_id != terminal_id)
    });
    if mode.allows_terminal_cascade() && terminal_is_empty && !eval.frames.frame_list().is_empty() {
        if let Some(terminal) =
            crate::emacs_core::terminal::pure::terminal_handle_value_for_id(terminal_id)
        {
            let _ = crate::emacs_core::terminal::pure::delete_terminal_owned(
                eval,
                crate::emacs_core::terminal::pure::terminal_handle_id(&terminal)
                    .expect("live terminal handle id"),
                crate::emacs_core::terminal::pure::DeleteTerminalMode::Public {
                    force_non_nil: true,
                },
            )?;
        }
    }
    eval.sync_keyboard_terminal_owner();
    if mode.runs_hooks_immediately() {
        let after_delete_hook = crate::emacs_core::hook_runtime::hook_symbol_by_name(
            eval,
            "after-delete-frame-functions",
        );
        let _ = crate::emacs_core::hook_runtime::safe_run_named_hook(
            eval,
            after_delete_hook,
            &[frame_value],
        )?;
    } else {
        eval.queue_pending_safe_hook("after-delete-frame-functions", &[frame_value]);
    }
    Ok(Value::NIL)
}

/// `(delete-frame &optional FRAME FORCE)` -> nil.
pub(crate) fn builtin_delete_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("delete-frame", &args, 2)?;
    if let Some(frame) = args.first()
        && matches!(frame.kind(), ValueKind::Veclike(VecLikeType::Frame))
        && let Some(raw_id) = frame.as_frame_id()
        && eval.frames.get(FrameId(raw_id)).is_none()
    {
        return Ok(Value::NIL);
    }
    let fid = {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        resolve_frame_id_in_state(frames, buffers, args.first(), "framep")?
    };
    let force_non_nil = args.get(1).copied().unwrap_or(Value::NIL).is_truthy();
    delete_frame_owned(eval, fid, DeleteFrameMode::Public { force_non_nil })
}

pub(crate) fn builtin_frame_window_state_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-window-state-change", &args, 1)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    Ok(Value::bool_val(
        eval.frames
            .get(fid)
            .is_some_and(|frame| frame.window_state_change),
    ))
}

pub(crate) fn builtin_set_frame_window_state_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("set-frame-window-state-change", &args, 2)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let state = args.get(1).copied().unwrap_or(Value::NIL).is_truthy();
    let frame = eval.frames.get_mut(fid).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![
                Value::symbol("frame-live-p"),
                args.first().copied().unwrap_or(Value::NIL),
            ],
        )
    })?;
    frame.window_state_change = state;
    Ok(Value::bool_val(state))
}

/// `(frame-parameter FRAME PARAMETER)` -> value or nil.
fn frame_parameter_value(frame: &crate::window::Frame, param_key: FrameParamKey) -> Value {
    match param_key {
        FrameParamKey::Known(FrameParam::Name) => return frame.name_value(),
        FrameParamKey::Known(FrameParam::Title) => return frame.title_value(),
        FrameParamKey::Known(FrameParam::IconName) => return frame.icon_name_value(),
        FrameParamKey::Known(FrameParam::Visibility) => {
            return if frame.visible { Value::T } else { Value::NIL };
        }
        FrameParamKey::Known(FrameParam::Font) if frame.effective_window_system().is_none() => {
            return Value::string("tty");
        }
        FrameParamKey::Known(FrameParam::Font) => {
            return frame
                .known_parameter(FrameParam::Font)
                .map(super::font::public_frame_font_parameter_value)
                .unwrap_or(Value::NIL);
        }
        _ => {}
    }

    match param_key.name() {
        "explicit-name" => frame.explicit_name_value(),
        // In Emacs, frame parameter width/height are text columns/lines.
        // For the bootstrap batch frame, explicit parameter overrides preserve
        // the 80x25 report shape.
        "width" => frame
            .parameter("width")
            .unwrap_or(Value::fixnum(frame.columns() as i64)),
        "height" => frame
            .parameter("height")
            .unwrap_or(Value::fixnum(frame.lines() as i64)),
        // GNU frame.c:4117 — buffer-list frame parameter stored
        // directly from f->buffer_list (most-recently-shown first).
        "buffer-list" => {
            let vals: Vec<Value> = frame
                .buffer_list
                .iter()
                .map(|id| Value::make_buffer(*id))
                .collect();
            Value::list(vals)
        }
        // GNU frame.c:4118 — buried-buffer-list frame parameter
        // stored from f->buried_buffer_list (most-recently-buried first).
        "buried-buffer-list" => {
            let vals: Vec<Value> = frame
                .buried_buffer_list
                .iter()
                .map(|id| Value::make_buffer(*id))
                .collect();
            Value::list(vals)
        }
        _ => frame.parameter_key(param_key).unwrap_or(Value::NIL),
    }
}

pub(crate) fn builtin_frame_parameter(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("frame-parameter", &args, 2)?;
    expect_max_args("frame-parameter", &args, 2)?;
    let fid = resolve_frame_id(eval, Some(&args[0]), "framep")?;
    let Some(param_key) = FrameParamKey::from_symbol_value(args[1]) else {
        return Ok(Value::NIL);
    };
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    Ok(frame_parameter_value(frame, param_key))
}
/// `(frame-parameters &optional FRAME)` -> alist.
pub(crate) fn builtin_frame_parameters(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("frame-parameters", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "framep")?;
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let mut pairs: Vec<Value> = Vec::new();
    // Built-in parameters.
    pairs.push(Value::cons(FrameParam::Name.symbol(), frame.name_value()));
    pairs.push(Value::cons(
        FrameParam::IconName.symbol(),
        frame.icon_name_value(),
    ));
    pairs.push(Value::cons(FrameParam::Title.symbol(), frame.title_value()));
    pairs.push(Value::cons(
        Value::symbol("explicit-name"),
        frame.explicit_name_value(),
    ));
    let width = frame
        .parameter("width")
        .unwrap_or(Value::fixnum(frame.columns() as i64));
    let height = frame
        .parameter("height")
        .unwrap_or(Value::fixnum(frame.lines() as i64));
    pairs.push(Value::cons(Value::symbol("width"), width));
    pairs.push(Value::cons(Value::symbol("height"), height));
    pairs.push(Value::cons(
        FrameParam::Visibility.symbol(),
        Value::bool_val(frame.visible),
    ));
    if frame.effective_window_system().is_none() {
        pairs.push(Value::cons(FrameParam::Font.symbol(), Value::string("tty")));
    }
    // GNU frame.c:4117-4118 — buffer-list and buried-buffer-list are
    // stored as frame parameters.
    {
        let blist: Vec<Value> = frame
            .buffer_list
            .iter()
            .map(|id| Value::make_buffer(*id))
            .collect();
        pairs.push(Value::cons(
            Value::symbol("buffer-list"),
            Value::list(blist),
        ));
    }
    {
        let buried: Vec<Value> = frame
            .buried_buffer_list
            .iter()
            .map(|id| Value::make_buffer(*id))
            .collect();
        pairs.push(Value::cons(
            Value::symbol("buried-buffer-list"),
            Value::list(buried),
        ));
    }
    // User parameters.
    for (k, v) in &frame.parameters {
        if frame.effective_window_system().is_none()
            && k.as_symbol_id()
                .and_then(FrameParam::from_symbol_id)
                .is_some_and(|param| param == FrameParam::Font)
        {
            continue;
        }
        let value = k
            .as_symbol_id()
            .and_then(FrameParam::from_symbol_id)
            .filter(|param| *param == FrameParam::Font)
            .map_or(*v, |_| super::font::public_frame_font_parameter_value(*v));
        pairs.push(Value::cons(*k, value));
    }
    Ok(Value::list(pairs))
}

pub(crate) fn builtin_frame_bottom_divider_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-bottom-divider-width", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(frame_divider_width(
        frame,
        FrameParam::BottomDividerWidth,
    )))
}

pub(crate) fn builtin_frame_child_frame_border_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-child-frame-border-width", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(frame.frame_child_frame_border_width()))
}

pub(crate) fn builtin_frame_internal_border_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-internal-border-width", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(frame.internal_border_width()))
}

pub(crate) fn builtin_frame_right_divider_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-right-divider-width", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::fixnum(frame_divider_width(
        frame,
        FrameParam::RightDividerWidth,
    )))
}

pub(crate) fn builtin_window_bottom_divider_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-bottom-divider-width", &args, 1)?;
    let (fid, wid) = resolve_window_id_with_pred(eval, args.first(), "window-live-p")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let width = if window_is_bottommost(frame, wid) {
        0
    } else {
        frame_divider_width(frame, FrameParam::BottomDividerWidth)
    };
    Ok(Value::fixnum(width))
}

pub(crate) fn builtin_window_right_divider_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-right-divider-width", &args, 1)?;
    let (fid, wid) = resolve_window_id_with_pred(eval, args.first(), "window-live-p")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let width = if window_is_rightmost(frame, wid) {
        0
    } else {
        frame_divider_width(frame, FrameParam::RightDividerWidth)
    };
    Ok(Value::fixnum(width))
}

/// `(modify-frame-parameters FRAME ALIST)` -> nil.
pub(crate) fn builtin_modify_frame_parameters(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("modify-frame-parameters", &args, 2)?;
    expect_max_args("modify-frame-parameters", &args, 2)?;
    let fid = resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let items = super::value::list_to_vec(&args[1]).unwrap_or_default();

    if eval.frames.get(fid).is_none() {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    }

    let mut requested_width = None;
    let mut requested_height = None;
    let mut requested_left = None;
    let mut requested_top = None;

    for item in items.into_iter().rev() {
        if item.is_cons() {
            let pair_car = item.cons_car();
            let pair_cdr = item.cons_cdr();
            if let Some(key) = pair_car.as_symbol_id() {
                let key_name = resolve_sym(key);
                match key_name {
                    "width" => {
                        if let Some(size) = parse_frame_size_param(pair_cdr) {
                            requested_width = Some(size);
                            let cols = eval
                                .frames
                                .get(fid)
                                .map(|frame| frame_size_param_to_cells(size, frame.char_width))
                                .unwrap_or(1)
                                .max(1);
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.set_parameter(Value::symbol("width"), Value::fixnum(cols));
                            }
                        }
                    }
                    "height" => {
                        if let Some(size) = parse_frame_size_param(pair_cdr) {
                            requested_height = Some(size);
                            let lines = eval
                                .frames
                                .get(fid)
                                .map(|frame| frame_size_param_to_cells(size, frame.char_height))
                                .unwrap_or(1)
                                .max(1);
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.set_parameter(Value::symbol("height"), Value::fixnum(lines));
                            }
                        }
                    }
                    "left" => {
                        if let Some(n) = pair_cdr.as_int() {
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.left_pos = n;
                                frame.set_parameter(Value::symbol("left"), Value::fixnum(n));
                            }
                            requested_left = Some(n);
                        }
                    }
                    "top" => {
                        if let Some(n) = pair_cdr.as_int() {
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.top_pos = n;
                                frame.set_parameter(Value::symbol("top"), Value::fixnum(n));
                            }
                            requested_top = Some(n);
                        }
                    }
                    "buffer-list" => {
                        let ids = live_frame_buffer_parameter_ids(&eval.buffers, pair_cdr);
                        if let Some(frame) = eval.frames.get_mut(fid) {
                            // GNU `store_frame_param` keeps these in
                            // `struct frame`, not in the generic parameter
                            // alist, and silently drops non-live entries.
                            frame.buffer_list = ids;
                            frame.remove_parameter(Value::symbol("buffer-list"));
                        }
                    }
                    "buried-buffer-list" => {
                        let ids = live_frame_buffer_parameter_ids(&eval.buffers, pair_cdr);
                        if let Some(frame) = eval.frames.get_mut(fid) {
                            frame.buried_buffer_list = ids;
                            frame.remove_parameter(Value::symbol("buried-buffer-list"));
                        }
                    }
                    _ => match FrameParamKey::from_symbol_id(key) {
                        FrameParamKey::Known(FrameParam::Name) => {
                            if let Some(name) = frame_name_parameter_value(&pair_cdr) {
                                if let Some(frame) = eval.frames.get_mut(fid) {
                                    frame.set_name_parameter_value(name);
                                }
                            }
                        }
                        FrameParamKey::Known(FrameParam::Title) => {
                            if let Some(title) = frame_title_parameter_value(&pair_cdr) {
                                if let Some(frame) = eval.frames.get_mut(fid) {
                                    frame.title = title;
                                }
                            }
                        }
                        FrameParamKey::Known(FrameParam::IconName) => {
                            if let Some(icon_name) = frame_icon_name_parameter_value(&pair_cdr) {
                                if let Some(frame) = eval.frames.get_mut(fid) {
                                    frame.icon_name = icon_name;
                                }
                            }
                        }
                        FrameParamKey::Known(FrameParam::ParentFrame) => {
                            let parent = if pair_cdr
                                .as_frame_id()
                                .map(|id| eval.frames.get(FrameId(id)).is_some())
                                .unwrap_or(false)
                            {
                                pair_cdr
                            } else {
                                Value::NIL
                            };
                            let parent_id = parent.as_frame_id().map(FrameId);
                            let z_order = parent_id.map(|id| 1 + eval.frames.max_child_z_order(id));
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.parent_frame = parent;
                                if let Some(z_order) = z_order {
                                    frame.z_order = z_order;
                                }
                                frame.set_known_parameter(FrameParam::ParentFrame, parent);
                            }
                        }
                        FrameParamKey::Known(FrameParam::Visibility) => {
                            // Route through set_frame_visibility so the
                            // display runtime is notified — mirrors
                            // GNU's gui_set_visibility → Fmake_frame_invisible.
                            set_frame_visibility(eval, fid, pair_cdr.is_truthy())?;
                        }
                        FrameParamKey::Known(FrameParam::Undecorated) => {
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.undecorated = pair_cdr.is_truthy();
                                frame.set_known_parameter(FrameParam::Undecorated, pair_cdr);
                            }
                        }
                        FrameParamKey::Known(FrameParam::NoAcceptFocus) => {
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.no_accept_focus = pair_cdr.is_truthy();
                                frame.set_known_parameter(FrameParam::NoAcceptFocus, pair_cdr);
                            }
                        }
                        FrameParamKey::Known(FrameParam::Unsplittable) => {
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.no_split = pair_cdr.is_truthy();
                                frame.set_known_parameter(FrameParam::Unsplittable, pair_cdr);
                            }
                        }
                        FrameParamKey::Known(
                            param @ (FrameParam::ForegroundColor | FrameParam::BackgroundColor),
                        ) => {
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.set_known_parameter(param, pair_cdr);
                            }
                            super::font::update_face_from_frame_parameter(
                                eval, fid, param, pair_cdr,
                            )?;
                        }
                        key => {
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.set_parameter_key(key, pair_cdr);
                            }
                        }
                    },
                }
            }
        }
    }
    if let Some(frame) = eval.frames.get_mut(fid) {
        frame.sync_tab_bar_height_from_parameters();
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
    }

    if requested_width.is_some()
        || requested_height.is_some()
        || requested_left.is_some()
        || requested_top.is_some()
    {
        let uses_window_system_pixels = eval
            .frames
            .get(fid)
            .is_some_and(frame_uses_window_system_pixels);
        if uses_window_system_pixels {
            let (
                current_cols,
                current_total_lines,
                current_text_width_px,
                current_text_height_px,
                char_width,
                char_height,
                should_defer_resize,
            ) = {
                let frame = eval
                    .frames
                    .get(fid)
                    .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
                (
                    frame_total_cols(frame),
                    frame_total_lines(frame),
                    frame_text_width_pixels_in_state(&eval.frames, fid),
                    frame_text_height_pixels(frame),
                    frame.char_width,
                    frame.char_height,
                    frame.should_defer_gui_parameter_resize(),
                )
            };
            let resize_request = frame_resize_request_from_params(
                requested_width,
                requested_height,
                current_cols,
                current_total_lines,
                current_text_width_px,
                current_text_height_px,
                char_width,
                char_height,
            );
            if should_defer_resize {
                let (desired_cols, desired_total_lines) =
                    resize_request.logical_size(&eval.frames, fid)?;
                let frame = eval
                    .frames
                    .get_mut(fid)
                    .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
                frame.queue_pending_gui_resize(desired_cols, desired_total_lines, false);
            } else {
                let (text_width_px, text_height_px) =
                    resize_request.text_pixels(&eval.frames, fid)?;
                resize_live_gui_frame(
                    &mut eval.frames,
                    &eval.buffers,
                    &mut eval.display_host,
                    fid,
                    text_width_px,
                    text_height_px,
                    false,
                )?;
            }
        } else if eval
            .frames
            .get(fid)
            .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some())
        {
            let (cols, total_lines) = {
                let frame = eval
                    .frames
                    .get(fid)
                    .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
                (
                    requested_width
                        .map(|size| frame_size_param_to_cells(size, frame.char_width))
                        .unwrap_or_else(|| frame_total_cols(frame)),
                    requested_height
                        .map(|size| frame_size_param_to_cells(size, frame.char_height))
                        .unwrap_or_else(|| frame_total_lines(frame)),
                )
            };
            let frame = eval
                .frames
                .get_mut(fid)
                .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
            let text_lines = total_lines
                .saturating_sub(i64::from(frame.minibuffer_leaf.is_some()))
                .max(if frame.parent_frame.as_frame_id().is_some() {
                    1
                } else {
                    MIN_FRAME_TEXT_LINES
                });
            set_frame_text_size(frame, cols, text_lines);
        }
    }

    Ok(Value::NIL)
}
/// `(frame-visible-p FRAME)` -> t or nil.
pub(crate) fn builtin_frame_visible_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let frames = &eval.frames;
    expect_args("frame-visible-p", &args, 1)?;
    let val = args.first().unwrap(); // expect_args enforced
    let fid = match val.kind() {
        ValueKind::Fixnum(n) => FrameId(n as u64),
        ValueKind::Veclike(VecLikeType::Frame) => FrameId(val.as_frame_id().unwrap()),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), *val],
            ));
        }
    };
    let frame = frames.get(fid).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), args[0]],
        )
    })?;
    Ok(Value::bool_val(frame.visible))
}

/// `(framep OBJ)` -> t if OBJ is a frame object or frame id that exists.
pub(crate) fn builtin_framep(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("framep", &args, 1)?;
    let id = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Frame) => args[0].as_frame_id().unwrap(),
        ValueKind::Fixnum(n) => n as u64,
        _ => return Ok(Value::NIL),
    };
    let Some(frame) = eval.frames.get(FrameId(id)) else {
        return Ok(Value::NIL);
    };
    // GNU `Fframep` (frame.c) returns the frame's intrinsic output method
    // (output_pgtk -> `pgtk`, output_x_window -> `x`, output_termcap /
    // output_initial -> `t`), NOT a frame parameter. Mirror that by reading the
    // frame's effective window system -- the same source redisplay uses -- so a
    // graphic frame is never misreported as a termcap `t` frame merely because
    // its `window-system` *parameter* is unset while the field is set. Reading
    // the parameter alone made `display-graphic-p` (elisp, via
    // `framep-on-display`) return nil for the GUI frame, sending it down the
    // TTY branch of `face-spec-reset-face` (`:family "default" :height 1`).
    Ok(frame.effective_window_system().unwrap_or(Value::T))
}
/// `(frame-live-p OBJ)` -> t if OBJ is a live frame object or frame id.
pub(crate) fn builtin_frame_live_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let frames = &eval.frames;
    expect_args("frame-live-p", &args, 1)?;
    let id = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Frame) => args[0].as_frame_id().unwrap(),
        ValueKind::Fixnum(n) => n as u64,
        _ => return Ok(Value::NIL),
    };
    Ok(Value::bool_val(frames.get(FrameId(id)).is_some()))
}

/// `(frame-initial-p &optional FRAME)` -> t if FRAME is the bootstrap initial
/// frame. Mirrors GNU `Fframe_initial_p` (src/terminal.c): FRAME defaults to the
/// selected frame, and the result is t when the frame is live and
/// `FRAME_INITIAL_P` -- the placeholder frame used during daemon mode, batch
/// mode, and the early stages of startup before a real terminal or
/// window-system frame is installed. `resolve_frame_id_in_state` only yields the
/// id of a live frame, so the remaining `frame.initial` check completes GNU's
/// `FRAME_LIVE_P (f) && FRAME_INITIAL_P (f)` test.
///
/// GNU also accepts a terminal object: `t->type == output_initial` is t. In
/// Neomacs that is the bootstrap initial terminal (`TERMINAL_ID`), so when the
/// argument is a terminal handle rather than a frame we resolve the terminal id
/// and compare it instead of signaling `wrong-type-argument`.
pub(crate) fn builtin_frame_initial_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    match resolve_frame_id(eval, args.first(), "frame-initial-p") {
        Ok(fid) => Ok(Value::bool_val(
            eval.frames.get(fid).is_some_and(|f| f.initial),
        )),
        Err(flow) => {
            // FRAMEP failed. GNU falls through to `decode_terminal (frame)` and
            // reports `t->type == output_initial`, i.e. the bootstrap initial
            // terminal. Terminal handles are only produced for real terminals,
            // so `terminal_handle_id` cannot alias a frame object.
            if let Some(arg) = args.first() {
                if let Some(id) = crate::emacs_core::terminal::pure::terminal_handle_id(arg) {
                    return Ok(Value::bool_val(
                        id == crate::emacs_core::terminal::pure::TERMINAL_ID,
                    ));
                }
            }
            Err(flow)
        }
    }
}

// ===========================================================================
// Bootstrap variables
// ===========================================================================

pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    use crate::emacs_core::value::Value;

    // window.c:9483 — DEFVAR_LISP
    obarray.set_symbol_value(
        "window-persistent-parameters",
        Value::list(vec![Value::cons(Value::symbol("clone-of"), Value::T)]),
    );
    obarray.set_symbol_value("recenter-redisplay", Value::symbol("tty"));
    obarray.set_symbol_value("window-restore-killed-buffer-windows", Value::NIL);
    obarray.set_symbol_value("window-combination-resize", Value::NIL);
    obarray.set_symbol_value("window-combination-limit", Value::symbol("window-size"));
    for name in [
        "window-persistent-parameters",
        "recenter-redisplay",
        "window-restore-killed-buffer-windows",
        "window-combination-resize",
        "window-combination-limit",
    ] {
        obarray.make_special(name);
    }
    obarray.set_symbol_value("delete-frame-functions", Value::NIL);
    obarray.set_symbol_value("after-delete-frame-functions", Value::NIL);
    obarray.set_symbol_value("window-buffer-change-functions", Value::NIL);
    obarray.set_symbol_value("window-size-change-functions", Value::NIL);
    obarray.set_symbol_value("window-selection-change-functions", Value::NIL);
    obarray.set_symbol_value("window-state-change-functions", Value::NIL);
    obarray.set_symbol_value("window-state-change-hook", Value::NIL);
    obarray.set_symbol_value("window-sides-vertical", Value::NIL);
    obarray.set_symbol_value("window-sides-slots", Value::NIL);
    obarray.set_symbol_value("window-resize-pixelwise", Value::NIL);
    obarray.set_symbol_value("fit-window-to-buffer-horizontally", Value::NIL);
    obarray.set_symbol_value("fit-frame-to-buffer", Value::NIL);
    obarray.set_symbol_value(
        "fit-frame-to-buffer-margins",
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
    );
    obarray.set_symbol_value("fit-frame-to-buffer-sizes", Value::NIL);
    obarray.set_symbol_value("window-min-height", Value::fixnum(4));
    obarray.set_symbol_value("window-min-width", Value::fixnum(10));
    obarray.set_symbol_value("window-safe-min-height", Value::fixnum(1));
    obarray.set_symbol_value("window-safe-min-width", Value::fixnum(2));
    obarray.set_symbol_value("scroll-preserve-screen-position", Value::NIL);
    obarray.set_symbol_value("window-point-insertion-type", Value::NIL);
    obarray.set_symbol_value("next-screen-context-lines", Value::fixnum(2));
    obarray.set_symbol_value("fast-but-imprecise-scrolling", Value::NIL);
    obarray.set_symbol_value("scroll-error-top-bottom", Value::NIL);
    obarray.set_symbol_value(
        "temp-buffer-max-height",
        Value::make_float(1.0 / 3.0), // (/ (frame-height) 3) approximation
    );
    obarray.set_symbol_value("temp-buffer-max-width", Value::NIL);
    obarray.set_symbol_value("even-window-sizes", Value::symbol("width-only"));
    obarray.set_symbol_value("auto-window-vscroll", Value::T);
}
/// `(window-combination-limit WINDOW)` -> nil or t.
///
/// Mirrors GNU Emacs: returns the combination limit of an internal window.
/// Signals an error if WINDOW is a leaf window.
pub(crate) fn builtin_window_combination_limit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("window-combination-limit", &args, 1)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let w = get_window(frames, fid, wid)?;
    match w.combination_limit() {
        Some(true) => Ok(Value::T),
        Some(false) => Ok(Value::NIL),
        None => Err(signal(
            "error",
            vec![Value::string(
                "Combination limit is meaningful for internal windows only",
            )],
        )),
    }
}
/// `(set-window-combination-limit WINDOW LIMIT)` -> LIMIT.
///
/// Set the combination limit of an internal window.
/// Signals an error if WINDOW is a leaf window.
pub(crate) fn builtin_set_window_combination_limit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_args("set-window-combination-limit", &args, 2)?;
    let _ = ensure_selected_frame_id_in_state(frames, buffers);
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-valid-p")?;
    let limit = args[1].is_truthy();
    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let w = frame
        .find_window_mut(wid)
        .ok_or_else(|| signal("error", vec![Value::string("Window not found")]))?;
    if w.is_leaf() {
        return Err(signal(
            "error",
            vec![Value::string(
                "Combination limit is meaningful for internal windows only",
            )],
        ));
    }
    w.set_combination_limit(limit);
    Ok(args[1])
}
/// `(window-resize-apply &optional FRAME HORIZONTAL)` -> t or nil.
///
/// Apply requested pixel size values for the window-tree of FRAME.
/// Mirrors GNU Emacs `Fwindow_resize_apply` in window.c.
pub(crate) fn builtin_window_resize_apply(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-resize-apply", &args, 2)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let horflag = args.get(1).is_some_and(|v| v.is_truthy());

    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    let cw = frame.char_width;
    let ch = frame.char_height;

    // Validate: root's new_pixel must match the frame dimension.
    if !crate::window::window_resize_check(&frame.root_window, horflag) {
        return Ok(Value::NIL);
    }

    // Check root's new_pixel matches frame size.
    let root_new = frame.root_window.new_pixel().unwrap_or_else(|| {
        let b = frame.root_window.bounds();
        if horflag {
            b.width as i64
        } else {
            b.height as i64
        }
    });
    let frame_dim = if horflag {
        frame.root_window.bounds().width as i64
    } else {
        frame.root_window.bounds().height as i64
    };
    if root_new != frame_dim {
        return Ok(Value::NIL);
    }

    // Apply. The recursive walk reads new_pixel directly from each
    // node now (audit Structural 1).
    crate::window::window_resize_apply(&mut frame.root_window, horflag, cw, ch);

    // Recalculate minibuffer position after tree resize.
    frame.recalculate_minibuffer_bounds();

    Ok(Value::T)
}
/// `(window-resize-apply-total &optional FRAME HORIZONTAL)` -> t.
///
/// Apply requested total (character-cell) size values for the window-tree of FRAME.
/// Mirrors GNU Emacs `Fwindow_resize_apply_total` in window.c.
pub(crate) fn builtin_window_resize_apply_total(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("window-resize-apply-total", &args, 2)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let horflag = args.get(1).is_some_and(|v| v.is_truthy());

    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    let cw = frame.char_width;
    let ch = frame.char_height;

    crate::window::window_resize_apply_total(&mut frame.root_window, horflag, cw, ch);

    // Handle minibuffer window — its `new_total` lives on the
    // minibuffer leaf itself now.
    if !horflag {
        if frame.minibuffer_window.is_some() {
            if let Some(mb) = frame.minibuffer_leaf.as_mut() {
                if let Some(new_total) = mb.new_total() {
                    let root_bounds = *frame.root_window.bounds();
                    let mb_top = root_bounds.y + root_bounds.height;
                    let mb_bounds = *mb.bounds();
                    let new_h = new_total.max(0) as f32 * ch;
                    mb.set_bounds(crate::window::Rect::new(
                        mb_bounds.x,
                        mb_top,
                        mb_bounds.width,
                        new_h,
                    ));
                    mb.set_new_total(None);
                }
            }
        }
    }

    // Ensure root + minibuffer fit in frame after total resize.
    frame.recalculate_minibuffer_bounds();

    Ok(Value::T)
}

// ===========================================================================
// balance-windows
// ===========================================================================

/// `(balance-windows &optional WINDOW-OR-FRAME)` -> nil.
///
/// Redistribute space equally among sibling windows.  When
/// WINDOW-OR-FRAME is a frame (or nil for the selected frame), balance
/// the entire window tree of that frame.  When it is a window, balance
/// the subtree rooted at its parent.
pub(crate) fn builtin_balance_windows(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("balance-windows", &args, 1)?;
    let fid = ensure_selected_frame_id(eval);
    let frame = eval
        .frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("No frame")]))?;

    fn balance_subtree(window: &mut Window) {
        if let Window::Internal {
            direction,
            children,
            bounds,
            ..
        } = window
        {
            let parent_bounds = *bounds;
            let n = children.len() as f32;
            if n < 1.0 {
                return;
            }
            match direction {
                SplitDirection::Horizontal => {
                    let w = parent_bounds.width / n;
                    for (i, child) in children.iter_mut().enumerate() {
                        child.set_bounds(Rect::new(
                            parent_bounds.x + i as f32 * w,
                            parent_bounds.y,
                            w,
                            parent_bounds.height,
                        ));
                    }
                }
                SplitDirection::Vertical => {
                    let h = parent_bounds.height / n;
                    for (i, child) in children.iter_mut().enumerate() {
                        child.set_bounds(Rect::new(
                            parent_bounds.x,
                            parent_bounds.y + i as f32 * h,
                            parent_bounds.width,
                            h,
                        ));
                    }
                }
            }
            for child in children.iter_mut() {
                balance_subtree(child);
            }
        }
    }

    balance_subtree(&mut frame.root_window);
    frame.recalculate_minibuffer_bounds();
    Ok(Value::NIL)
}

// ===========================================================================
// enlarge-window / shrink-window
// ===========================================================================

/// Minimum window dimension in pixels when resizing.
const MIN_WINDOW_PIXEL_SIZE: f32 = 1.0;

/// `(enlarge-window DELTA &optional HORIZONTAL)` -> nil.
///
/// Make the selected window DELTA lines taller.  If HORIZONTAL is
/// non-nil, make it DELTA columns wider instead.  Interactively, if no
/// argument is given, make the window one line taller.
pub(crate) fn builtin_enlarge_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("enlarge-window", &args, 1)?;
    expect_max_args("enlarge-window", &args, 2)?;
    let delta = expect_int(&args[0])?;
    let horizontal = args.get(1).is_some_and(|v| !v.is_nil());
    resize_selected_window(eval, delta, horizontal, "enlarge-window")
}

/// `(shrink-window DELTA &optional HORIZONTAL)` -> nil.
///
/// Make the selected window DELTA lines shorter.  If HORIZONTAL is
/// non-nil, make it DELTA columns narrower instead.
pub(crate) fn builtin_shrink_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("shrink-window", &args, 1)?;
    expect_max_args("shrink-window", &args, 2)?;
    let delta = expect_int(&args[0])?;
    let horizontal = args.get(1).is_some_and(|v| !v.is_nil());
    // shrink = enlarge by negative delta
    resize_selected_window(eval, -delta, horizontal, "shrink-window")
}

/// Shared implementation for enlarge-window and shrink-window.
///
/// Finds the selected window's parent internal node, then grows/shrinks
/// the selected window by `delta` character units along the appropriate
/// axis.  The space is taken from (or given to) sibling windows equally.
fn resize_selected_window(
    eval: &mut super::eval::Context,
    delta: i64,
    horizontal: bool,
    name: &str,
) -> EvalResult {
    if delta == 0 {
        return Ok(Value::NIL);
    }

    let fid = ensure_selected_frame_id(eval);
    let frame = eval
        .frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("No frame")]))?;

    let sel_wid = frame.selected_window;
    let char_size = if horizontal {
        frame.char_width.max(1.0)
    } else {
        frame.char_height.max(1.0)
    };
    let delta_px = delta as f32 * char_size;

    // Walk the tree to find the parent of the selected window and resize.
    fn resize_in_tree(
        node: &mut Window,
        target: WindowId,
        delta_px: f32,
        horizontal: bool,
    ) -> bool {
        let Window::Internal {
            direction,
            children,
            ..
        } = node
        else {
            return false;
        };

        // Check if target is a direct child.
        if let Some(target_idx) = children.iter().position(|c| c.id() == target) {
            // Direction must match: horizontal split for horizontal resize,
            // vertical split for vertical resize.
            let dir_matches = (*direction == SplitDirection::Horizontal) == horizontal;
            if !dir_matches || children.len() < 2 {
                // Can't resize — try parent.  But since we don't have a parent
                // pointer in this recursive approach, we just return false.
                return false;
            }

            let sibling_count = (children.len() - 1) as f32;
            let shrink_each = delta_px / sibling_count;

            // Check if siblings can absorb the shrink.
            for (i, child) in children.iter().enumerate() {
                if i == target_idx {
                    continue;
                }
                let dim = if horizontal {
                    child.bounds().width
                } else {
                    child.bounds().height
                };
                if dim - shrink_each < MIN_WINDOW_PIXEL_SIZE {
                    // Not enough room — clamp what we can.
                    // (We proceed anyway with clamping below.)
                }
                let _ = dim; // suppress unused warning
            }

            // Apply: grow target, shrink siblings.
            // First, compute the new sizes.
            let sizes: Vec<f32> = children
                .iter()
                .enumerate()
                .map(|(i, child)| {
                    let dim = if horizontal {
                        child.bounds().width
                    } else {
                        child.bounds().height
                    };
                    if i == target_idx {
                        (dim + delta_px).max(MIN_WINDOW_PIXEL_SIZE)
                    } else {
                        (dim - shrink_each).max(MIN_WINDOW_PIXEL_SIZE)
                    }
                })
                .collect();

            // Re-layout children with new sizes.
            let parent_bounds = *node.bounds();
            if let Window::Internal { children, .. } = node {
                let mut edge = if horizontal {
                    parent_bounds.x
                } else {
                    parent_bounds.y
                };
                for (i, child) in children.iter_mut().enumerate() {
                    let b = *child.bounds();
                    if horizontal {
                        child.set_bounds(Rect::new(edge, b.y, sizes[i], b.height));
                        edge += sizes[i];
                    } else {
                        child.set_bounds(Rect::new(b.x, edge, b.width, sizes[i]));
                        edge += sizes[i];
                    }
                }
            }

            return true;
        }

        // Recurse into children.
        for child in children.iter_mut() {
            if resize_in_tree(child, target, delta_px, horizontal) {
                return true;
            }
        }
        false
    }

    if !resize_in_tree(&mut frame.root_window, sel_wid, delta_px, horizontal) {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "{name}: cannot resize the only window"
            ))],
        ));
    }

    frame.recalculate_minibuffer_bounds();
    Ok(Value::NIL)
}

/// Resize TARGET within FRAME by DELTA_PX while preserving a contiguous split.
///
/// This is a narrower helper than GNU's full window-resize machinery, but it
/// keeps sibling bounds consistent for common two-pane temporary/help window
/// flows. Returns false when TARGET cannot be resized in the requested axis.
fn resize_window_by_delta_px(
    frame: &mut crate::window::Frame,
    target: WindowId,
    delta_px: f32,
    horizontal: bool,
) -> bool {
    if delta_px.abs() < 0.5 {
        return true;
    }

    fn resize_in_tree(
        node: &mut Window,
        target: WindowId,
        delta_px: f32,
        horizontal: bool,
    ) -> bool {
        let parent_bounds = *node.bounds();
        let Window::Internal {
            direction,
            children,
            ..
        } = node
        else {
            return false;
        };

        if let Some(target_idx) = children.iter().position(|child| child.id() == target) {
            let dir_matches = (*direction == SplitDirection::Horizontal) == horizontal;
            if !dir_matches || children.len() < 2 {
                return false;
            }

            let parent_size = if horizontal {
                parent_bounds.width
            } else {
                parent_bounds.height
            };
            let current_target_size = if horizontal {
                children[target_idx].bounds().width
            } else {
                children[target_idx].bounds().height
            };
            let min_target_size = MIN_WINDOW_PIXEL_SIZE;
            let min_other_total = MIN_WINDOW_PIXEL_SIZE * (children.len() - 1) as f32;
            let desired_target_size = (current_target_size + delta_px).clamp(
                min_target_size,
                (parent_size - min_other_total).max(min_target_size),
            );
            let remaining = (parent_size - desired_target_size).max(min_other_total);
            let sibling_size = remaining / (children.len() - 1) as f32;

            let mut edge = if horizontal {
                parent_bounds.x
            } else {
                parent_bounds.y
            };
            for (idx, child) in children.iter_mut().enumerate() {
                let size = if idx == target_idx {
                    desired_target_size
                } else {
                    sibling_size
                };
                let bounds = *child.bounds();
                if horizontal {
                    child.set_bounds(Rect::new(edge, bounds.y, size, bounds.height));
                    edge += size;
                } else {
                    child.set_bounds(Rect::new(bounds.x, edge, bounds.width, size));
                    edge += size;
                }
            }
            return true;
        }

        for child in children.iter_mut() {
            if resize_in_tree(child, target, delta_px, horizontal) {
                return true;
            }
        }
        false
    }

    let resized = resize_in_tree(&mut frame.root_window, target, delta_px, horizontal);
    if resized {
        frame.root_window.invalidate_display_state();
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            mini.invalidate_display_state();
        }
    }
    resized
}

// ===========================================================================
// window-tree
// ===========================================================================

/// `(window-tree &optional FRAME)` -> nested list describing the window tree.
///
/// For a leaf window, returns the window object.
/// For an internal node, returns:
///   `(HORIZONTAL-P TOP LEFT RIGHT BOTTOM CHILD1 CHILD2 ...)`
/// where HORIZONTAL-P is t for horizontal combination, nil for vertical.
pub(crate) fn builtin_window_tree(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("window-tree", &args, 1)?;
    let fid = resolve_frame_id(eval, args.first(), "frame-live-p")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("No frame")]))?;

    fn window_tree_to_value(window: &Window) -> Value {
        match window {
            Window::Leaf { id, .. } => Value::make_window(id.0),
            Window::Internal {
                direction,
                children,
                bounds,
                ..
            } => {
                let horizontal_p = if *direction == SplitDirection::Horizontal {
                    Value::T
                } else {
                    Value::NIL
                };
                // Edges: top left right bottom (in pixel coordinates)
                let top = Value::fixnum(bounds.y as i64);
                let left = Value::fixnum(bounds.x as i64);
                let right = Value::fixnum(bounds.right() as i64);
                let bottom = Value::fixnum(bounds.bottom() as i64);

                let mut elts = vec![horizontal_p, top, left, right, bottom];
                for child in children {
                    elts.push(window_tree_to_value(child));
                }
                Value::list(elts)
            }
        }
    }

    let tree = window_tree_to_value(&frame.root_window);
    // GNU returns (TREE . MINI-WINDOW)
    let mini = frame
        .minibuffer_window
        .map(|wid| Value::make_window(wid.0))
        .unwrap_or(Value::NIL);
    Ok(Value::cons(tree, mini))
}

// ===========================================================================
// fit-window-to-buffer
// ===========================================================================

/// `(fit-window-to-buffer &optional WINDOW MAX-HEIGHT MIN-HEIGHT MAX-WIDTH MIN-WIDTH PRESERVE-SIZE)` -> nil.
///
/// Adjust WINDOW height to fit its buffer contents, clamped to the
/// optional MAX-HEIGHT and MIN-HEIGHT limits (in lines).  In batch /
/// non-GUI mode this is mostly a no-op — we still validate arguments
/// so callers see the correct error behaviour.
pub(crate) fn builtin_fit_window_to_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("fit-window-to-buffer", &args, 6)?;

    // Resolve WINDOW — nil means selected window.
    let (fid, wid) = resolve_window_id_or_error(eval, args.first())?;

    let (buf_id, bounds) = {
        let w = get_leaf(&eval.frames, fid, wid)?;
        match w {
            Window::Leaf {
                buffer_id, bounds, ..
            } => (*buffer_id, bounds.clone()),
            _ => return Ok(Value::NIL),
        }
    };

    let buf = match eval.buffers.get(buf_id) {
        Some(b) => b,
        None => return Ok(Value::NIL),
    };

    // Count lines in the buffer.
    let text = buf.full_text_string();
    let buf_lines = text.chars().filter(|&c| c == '\n').count() + 1;

    // Parse optional height limits.
    let parse_opt_int = |idx: usize| -> Option<usize> {
        args.get(idx)
            .and_then(|v| v.as_fixnum())
            .filter(|&n| n > 0)
            .map(|n| n as usize)
    };
    let max_height = parse_opt_int(1);
    let min_height = parse_opt_int(2);

    // Clamp desired height.
    let mut desired = buf_lines;
    if let Some(max_h) = max_height {
        desired = desired.min(max_h);
    }
    if let Some(min_h) = min_height {
        desired = desired.max(min_h);
    }

    // In batch / TUI mode, attempt to resize the window via the frame manager.
    let ch = eval
        .frames
        .get(fid)
        .map(|f| f.char_height.max(1.0))
        .unwrap_or(16.0);
    let new_pixel_height = (desired as f32 * ch) + ch; // +1 row for mode line
    let current_height = bounds.height;
    if (new_pixel_height - current_height).abs() > 0.5 {
        if let Some(frame) = eval.frames.get_mut(fid) {
            let _ = resize_window_by_delta_px(frame, wid, new_pixel_height - current_height, false);
        }
    }

    Ok(Value::NIL)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "../window_cmds_test.rs"]
mod tests;
