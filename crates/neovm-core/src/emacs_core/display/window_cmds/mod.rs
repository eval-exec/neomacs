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
use crate::emacs_core::error::LispCondition;
pub(crate) use crate::emacs_core::error::{
    expect_args, expect_fixnum, expect_max_args, expect_min_args,
};
use crate::emacs_core::indent::MotionEngine;
use crate::emacs_core::xdisp::LineWrap;
use crate::window::body::{WindowBodyAxis, WindowBodyCellSize, WindowBodyUnit};
use crate::window::{
    CombinationLimit, CursorTypeSymbol, DeleteResize, FrameDivider, FrameFullscreen, FrameId,
    FrameManager, FrameParam, FrameParamKey, Rect, SplitDirection, SplitPlacement, Window,
    WindowBufferDisplayDefaults, WindowFringeDefaults, WindowId, WindowMargins,
    WindowScrollBarDefaults, is_valid_horizontal_scroll_bar_value,
    is_valid_vertical_scroll_bar_value, window_first_child_id, window_next_sibling_id,
    window_parent_id, window_prev_sibling_id,
};
use neomacs_display_protocol::TransitionDirection;
use std::collections::HashSet;
use strum::{EnumString, IntoStaticStr};

fn navigation_transition_direction(value: Value) -> Result<TransitionDirection, Flow> {
    value
        .as_symbol_name()
        .and_then(|name| name.parse().ok())
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("neomacs--transition-direction-p"), value],
            )
        })
}

fn lisp_char_pos_from_one_based_usize(pos: usize) -> LispCharPos1 {
    LispCharPos1::from_one_based_usize(pos)
}

pub(crate) use super::builtins::symbols::{
    builtin_resize_mini_window_internal, builtin_set_window_new_normal,
    builtin_set_window_new_pixel, builtin_set_window_new_total,
};
pub(crate) use super::builtins::{
    builtin_coordinates_in_window_p, builtin_current_window_configuration,
    builtin_run_window_scroll_functions, builtin_set_window_configuration,
    builtin_split_window_internal, builtin_window_configuration_equal_p,
    builtin_window_configuration_frame, builtin_window_configuration_p,
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

/// Extract an integer from a Value.
pub(crate) fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
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
            LispCondition::WrongTypeArgument,
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *value],
            )
        })
}

/// Decode the optional Lisp unit argument accepted by `window-body-*`.
///
/// This deliberately mirrors GNU `window_body_unit_from_symbol`: `nil` means
/// canonical frame cells, the exact symbol `remap` means buffer-remapped
/// default-face cells, and every other non-nil value means pixels.
fn window_body_unit_from_lisp(value: Option<&Value>) -> WindowBodyUnit {
    match value {
        None => WindowBodyUnit::CanonicalChars,
        Some(value) if value.is_nil() => WindowBodyUnit::CanonicalChars,
        Some(value) if value.is_symbol_named("remap") => WindowBodyUnit::RemappedChars,
        Some(_) => WindowBodyUnit::Pixels,
    }
}

fn find_buffer_by_name_arg(
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<BufferId>, Flow> {
    let name = expect_buffer_name_string(value)?;
    Ok(buffers.find_buffer_by_name(&name))
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
                LispCondition::WrongTypeArgument,
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
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
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

/// Extract a number-or-marker argument as f64.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_number_or_marker(value: &Value) -> Result<f64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(value.xfloat()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
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
            if !(0..=MAX_MARGIN).contains(&n) {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![
                        Value::fixnum(n),
                        Value::fixnum(0),
                        Value::fixnum(MAX_MARGIN),
                    ],
                ));
            }
            Ok(n as usize)
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
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
                LispCondition::ArgsOutOfRange,
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
            LispCondition::WrongTypeArgument,
            vec![Value::symbol(pred), *val],
        ));
    };
    if let Some(frame_id) = resolve_window_frame_id_for_pred(frames, wid, pred) {
        Ok((frame_id, wid))
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol(pred), *val],
        ))
    }
}

fn resolve_window_id_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    resolve_window_id_with_pred_in_state(frames, buffers, arg, "window-live-p")
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
            LispCondition::WrongTypeArgument,
            vec![Value::symbol(pred), *val],
        ));
    };
    if frames.is_window_object_id(wid) {
        Ok(wid)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol(pred), *val],
        ))
    }
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
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("windowp"), *value],
        ));
    };
    if let Some(fid) = frames.find_valid_window_frame_id(wid) {
        Ok((fid, wid))
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-valid-p"), *value],
        ))
    }
}

/// Resolve a frame designator, signaling predicate-shaped type errors.
///
/// When ARG is nil/omitted, GNU Emacs resolves against the selected frame.
/// In batch compatibility mode we bootstrap that frame on demand.
pub(crate) fn resolve_frame_id(
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
                    LispCondition::WrongTypeArgument,
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
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol(predicate), Value::make_frame(raw_id)],
                ))
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol(predicate), *val],
        )),
    }
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
                    LispCondition::WrongTypeArgument,
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
                LispCondition::WrongTypeArgument,
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol(predicate), Value::make_window(raw_id)],
            ))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
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
        // NO `display-type' and NO `background-mode' here.  This is GNU's
        // `make_initial_frame' (src/frame.c:1423), called from
        // `init_window_once' (src/window.c:9148) BEFORE loadup
        // (src/emacs.c:2006), and it sets neither.  Both are DERIVED, by
        // `frame-set-background-mode' (lisp/frame.el:1526), which C reaches
        // only through `init_faces_initial' (src/dispnew.c:7178) ->
        // `tty-set-up-initial-frame-faces' (lisp/faces.el:2409) from
        // `init_display' (src/dispnew.c:7413-7422) -- after the pdump is
        // loaded, never during loadup.  Measured on GNU 31.0.90, `src/temacs
        // --batch -l loadup': `background-mode=nil display-type=nil'.
        // Seeding them here made `show-paren-match's `((background dark)
        // (min-colors 4))' clause (lisp/faces.el:3161) match its first
        // conjunct during `(load "faces")' and call `display-color-cells',
        // which is `lisp/frame.el:2966' and still void ninety-five files
        // later in GNU.  DIVERGENCES.md 157.
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
                crate::window::window_markers::attach_window_position_markers(buffers, w);
            }
        }
        if let Some(minibuffer_leaf) = frame.minibuffer_leaf.as_mut() {
            minibuffer_leaf.set_buffer(minibuffer_buf_id);
            minibuffer_leaf.set_bounds(Rect::new(0.0, 24.0, 80.0, 1.0));
            crate::window::window_markers::attach_window_position_markers(buffers, minibuffer_leaf);
        }
        frame.recalculate_minibuffer_bounds();
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

/// GNU `init_iterator`'s line-wrap decision for one window+buffer pair
/// (src/xdisp.c:3413-3426), as the typed [`LineWrap`] it resolves to.
///
/// Every input is read the way GNU reads it:
///
/// * `truncate-lines` and `word-wrap` are per-buffer slots -- GNU spells them
///   `BVAR (current_buffer, truncate_lines)` / `BVAR (current_buffer,
///   word_wrap)`.
/// * `truncate-partial-width-windows` is a `DEFVAR_LISP`, so `setq-local`
///   localizes the symbol and the C global `Vtruncate_partial_width_windows`
///   always holds the value swapped in for `current_buffer`.  It is therefore
///   a BUFFER-LOCAL-then-global read, not a global one -- GNU's own Lisp
///   predicate says so in as many words:
///
///   ```elisp
///   (defun truncated-partial-width-window-p (&optional window)
///     ...
///     (unless (window-full-width-p window)
///       (let ((t-p-w-w (buffer-local-value 'truncate-partial-width-windows
///                                          (window-buffer window))))
///         (if (integerp t-p-w-w)
///             (< (window-total-width window) t-p-w-w)
///           t-p-w-w))))
///   ```
///   (lisp/window.el:11285-11298).
///
///   `visual-line-mode' depends on exactly that: it does
///   `(setq-local truncate-partial-width-windows nil)` (lisp/simple.el:8716)
///   so a window narrower than the 50-column default still wraps.  Reading the
///   global here instead made every partial-width `visual-line-mode` window
///   report TRUNCATE, which collapses `beginning-of-visual-line` --
///   `(vertical-motion 0)`, lisp/simple.el:8573 -- onto the LOGICAL line start.
/// * A horizontally scrolled window never wraps (`!it->w->hscroll`).
/// * Whether `WORD_WRAP` is on the menu at all is the ENGINE's question, not
///   this window's: `init_iterator` -- and therefore `word-wrap` -- is reached
///   only from the interactive arm of `Fvertical_motion`
///   (src/indent.c:2280-2287).  See [`MotionEngine`].
pub(crate) fn window_line_wrap_for_motion(
    eval: &mut super::eval::Context,
    window: Option<Value>,
    current_buffer_id: BufferId,
    engine: MotionEngine,
) -> LineWrap {
    let _ = ensure_selected_frame_id_in_state(&mut eval.frames, &mut eval.buffers);
    let Ok((fid, wid)) = resolve_window_id_with_pred_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        window.as_ref(),
        "window-live-p",
    ) else {
        return LineWrap::WindowWrap;
    };
    let current_buffer = eval.buffers.get(current_buffer_id);
    let read = |name: &str| {
        crate::emacs_core::indent::dynamic_buffer_or_global_symbol_value(
            &eval.obarray,
            &[],
            current_buffer,
            name,
        )
    };
    // GNU's wrapping method when the window does not truncate.  The buffer
    // asks for word wrapping; the engine decides whether that request can be
    // honoured at all.
    let wrapped = engine.continuation_wrap(read("word-wrap").is_some_and(|value| !value.is_nil()));

    if read("truncate-lines").is_some_and(|value| !value.is_nil()) {
        return LineWrap::Truncate;
    }
    let hscroll_nonzero = eval
        .frames
        .get(fid)
        .and_then(|frame| frame.find_window(wid))
        .is_some_and(|window| matches!(window, Window::Leaf { hscroll, .. } if *hscroll != 0));
    if hscroll_nonzero {
        return LineWrap::Truncate;
    }

    let root_wid = match eval.frames.get(fid) {
        Some(frame) => frame.root_window.id(),
        None => return wrapped,
    };
    let window_cols =
        window_total_width_impl(&mut eval.frames, &mut eval.buffers, vec![window_value(wid)])
            .ok()
            .and_then(|value| value.as_fixnum())
            .unwrap_or(0);
    let root_cols = window_total_width_impl(
        &mut eval.frames,
        &mut eval.buffers,
        vec![window_value(root_wid)],
    )
    .ok()
    .and_then(|value| value.as_fixnum())
    .unwrap_or(window_cols);
    if window_cols >= root_cols {
        return wrapped;
    }

    // Re-read through the buffer: `eval.buffers` was borrowed mutably above.
    let current_buffer = eval.buffers.get(current_buffer_id);
    let partial_width_truncates =
        match crate::emacs_core::indent::dynamic_buffer_or_global_symbol_value(
            &eval.obarray,
            &[],
            current_buffer,
            "truncate-partial-width-windows",
        ) {
            Some(value) if value.is_nil() => false,
            Some(value) if value.is_fixnum() => window_cols < value.as_fixnum().unwrap(),
            Some(_) => true,
            None => false,
        };
    if partial_width_truncates {
        LineWrap::Truncate
    } else {
        wrapped
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
    match w {
        Window::Leaf { margins, .. } => {
            let char_width = frame.char_width.max(1.0);
            let left_margin = (margins.left() as f32 * char_width).round().max(0.0) as i64;
            let right_margin = (margins.right() as f32 * char_width).round().max(0.0) as i64;
            let (left_fringe, right_fringe) = if frame.effective_window_system().is_some() {
                let (left, right, _, _) = frames
                    .window_fringes(w.id())
                    .unwrap_or((0, 0, false, false));
                (left, right)
            } else {
                (0, 0)
            };
            let left_scroll_bar = frames.window_left_scroll_bar_area_width(w.id());
            let right_scroll_bar = frames.window_right_scroll_bar_area_width(w.id());
            // GNU `window_body_width` (`src/window.c`) removes the explicit
            // right divider from every non-rightmost window.  On text
            // terminals without such a divider it instead reserves one
            // canonical column for the vertical separator.
            let right_divider_or_tty_separator = if window_is_rightmost(frame, w.id()) {
                0
            } else {
                let divider = frame.effective_divider_width(FrameDivider::Right);
                if divider > 0 {
                    divider
                } else if frame.effective_window_system().is_none() {
                    char_width.round().max(1.0) as i64
                } else {
                    0
                }
            };
            (
                left_scroll_bar
                    .saturating_add(left_fringe)
                    .saturating_add(left_margin),
                right_scroll_bar
                    .saturating_add(right_fringe)
                    .saturating_add(right_margin)
                    .saturating_add(right_divider_or_tty_separator),
            )
        }
        Window::Internal { .. } => (0, 0),
    }
}

/// Text-area width of a leaf window in pixels (total minus scroll bars,
/// fringes, margins, and the right divider or terminal separator).  Shared
/// with auto-hscroll (`super::hscroll`) so the column geometry it follows
/// matches what `window-body-width` reports and what the layout engine renders.
pub(crate) fn window_body_width_pixels(frames: &FrameManager, fid: FrameId, w: &Window) -> i64 {
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
            LispCondition::WrongTypeArgument,
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
                .is_some_and(|first| discarded_buffers.contains(first))
        })
        .collect())
}

fn filtered_window_next_buffers(
    next_raw: Value,
    discarded_buffers: &[Value],
) -> Result<Vec<Value>, Flow> {
    let next_entries = list_to_vec(&next_raw).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), next_raw],
        )
    })?;
    Ok(next_entries
        .into_iter()
        .filter(|entry| !discarded_buffers.contains(entry))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowBufferHistoryChange {
    pub(crate) outgoing_buffer_id: BufferId,
    pub(crate) incoming_buffer_id: BufferId,
    pub(crate) outgoing_window_start: LispCharPos1,
    pub(crate) outgoing_window_point: LispCharPos1,
}

/// Apply the history part of GNU `record-window-buffer` for a window that is
/// about to change buffers.  Both ordinary `set-window-buffer` and window
/// configuration restoration cross this seam.
pub(crate) fn record_window_buffer_change_history_in_state(
    frames: &mut FrameManager,
    minibuffers: &MinibufferManager,
    buffers: &BufferManager,
    frame_id: FrameId,
    window_id: WindowId,
    change: WindowBufferHistoryChange,
) -> Result<bool, Flow> {
    let outgoing_buffer = Value::make_buffer(change.outgoing_buffer_id);
    let incoming_buffer = Value::make_buffer(change.incoming_buffer_id);
    let history_entry = Value::list(vec![
        outgoing_buffer,
        super::marker::make_marker_value(
            Some(change.outgoing_buffer_id),
            Some(change.outgoing_window_start.max(LispCharPos1::ONE)),
            false,
        ),
        super::marker::make_marker_value(
            Some(change.outgoing_buffer_id),
            Some(change.outgoing_window_point.max(LispCharPos1::ONE)),
            false,
        ),
    ]);

    // GNU removes both the outgoing buffer (before re-adding it at the front)
    // and the incoming buffer (which is no longer a previous buffer).
    let filtered_prev = filtered_window_prev_buffers(
        frames.window_prev_buffers(window_id),
        &[outgoing_buffer, incoming_buffer],
    )?;
    frames.set_window_next_buffers(window_id, Value::NIL);

    let record_outgoing = should_record_window_history_buffer(
        frames,
        minibuffers,
        buffers,
        frame_id,
        window_id,
        change.outgoing_buffer_id,
    );
    if record_outgoing {
        let mut next_prev = Vec::with_capacity(filtered_prev.len() + 1);
        next_prev.push(history_entry);
        next_prev.extend(filtered_prev);
        frames.set_window_prev_buffers(window_id, Value::list(next_prev));
    } else {
        frames.set_window_prev_buffers(window_id, Value::list(filtered_prev));
    }

    Ok(record_outgoing && !is_minibuffer_window(frames, frame_id, window_id))
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

// ===========================================================================
// Window queries
// ===========================================================================
/// Return the selected window as a typed editor-domain identifier.
pub(crate) fn selected_window_id(eval: &mut super::eval::Context) -> Result<WindowId, Flow> {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    let fid = ensure_selected_frame_id_in_state(frames, buffers);
    frames
        .get(fid)
        .map(|frame| frame.selected_window)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))
}

/// `(selected-window)` -> window object.
pub(crate) fn builtin_selected_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("selected-window", &args, 0)?;
    Ok(window_value(selected_window_id(eval)?))
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
    let mut frames = eval
        .frames
        .frames_in_reverse_z_order(fid, crate::window::RenderFrameVisibility::VisibleOnly);
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
    let mut frames = eval
        .frames
        .frames_in_reverse_z_order(selected, crate::window::RenderFrameVisibility::VisibleOnly);
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
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("window-live-p"), args[1]],
                ));
            }
            wid
        }
        None => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
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
            LispCondition::WrongTypeArgument,
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
        LispCondition::WrongTypeArgument,
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
    // A window parameter named after one of the chrome formats OVERRIDES the
    // buffer-local value (`eval_status_line_format_value` consults the window
    // parameter first), so setting one changes this window's chrome with none
    // of the buffer-scoped triggers firing. GNU has no such override and so
    // needs no equivalent; here it is a window-scoped dirty event, the same
    // shape as `set-window-start`.
    if let Some(name) = args[1].as_symbol_name()
        && crate::buffer::buffer::variable_affects_chrome(&name)
    {
        eval.mark_chrome_dirty_window(wid);
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowEndQueryPolicy {
    LastPresented,
    EnsureCurrent,
}

fn current_buffer_z(
    buffers: &BufferManager,
    buffer_id: BufferId,
    fallback: LispCharPos1,
) -> LispCharPos1 {
    buffers
        .get(buffer_id)
        .map(|buffer| {
            LispCharPos1::from_one_based_usize(buffer.point_max_char_pos().get().saturating_add(1))
        })
        .unwrap_or(fallback)
}

/// Resolve GNU `window-end` semantics behind one typed query seam.
///
/// `EnsureCurrent` delegates to the frontend's synchronous layout adapter,
/// which runs the same row producer as redisplay. A missing adapter behaves
/// like GNU's noninteractive/initial-frame case and returns the last recorded
/// value. A busy adapter signals instead of disguising stale state as an
/// updated answer; there is no second approximation algorithm.
fn query_window_end(
    eval: &mut super::eval::Context,
    fid: FrameId,
    wid: WindowId,
    policy: WindowEndQueryPolicy,
) -> EvalResult {
    let noninteractive = eval.noninteractive();
    let frame_initial = eval.frames.get(fid).is_some_and(|frame| frame.initial);
    let (window_start, buffer_id, window_end) = match get_leaf(&eval.frames, fid, wid)? {
        Window::Leaf {
            window_start,
            buffer_id,
            window_end,
            ..
        } => (*window_start, *buffer_id, *window_end),
        Window::Internal { .. } => return Ok(Value::NIL),
    };
    let buffer_z = current_buffer_z(&eval.buffers, buffer_id, window_start);
    let stored_end = window_end.charpos_from_z(buffer_z);
    if policy == WindowEndQueryPolicy::LastPresented || noninteractive || frame_initial {
        return Ok(Value::fixnum(stored_end.as_i64()));
    }
    if window_end.is_current()
        && eval
            .fresh_window_display_snapshot(fid, wid, buffer_id)
            .is_some()
    {
        // GNU Fwindow_end only constructs a stack-local iterator when the
        // window's accepted end is no longer valid. The full retained-layout
        // token is Neomacs's authoritative equivalent of GNU's validity bits.
        return Ok(Value::fixnum(stored_end.as_i64()));
    }

    match eval.query_window_layout(fid, wid) {
        crate::window::WindowLayoutQueryOutcome::Ready(query) => {
            return Ok(Value::fixnum(query.end().as_i64()));
        }
        crate::window::WindowLayoutQueryOutcome::Unavailable => {}
        crate::window::WindowLayoutQueryOutcome::LayoutBusy => {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(
                    "Window layout query reentered an active layout callback",
                )],
            ));
        }
        crate::window::WindowLayoutQueryOutcome::Failed(failure) => {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(failure.message())],
            ));
        }
    }

    Ok(Value::fixnum(stored_end.as_i64()))
}

/// `(window-end &optional WINDOW UPDATE)` -> integer position.
pub(crate) fn builtin_window_end(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("window-end", &args, 2)?;
    let (fid, wid) = resolve_window_id_with_pred_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "window-live-p",
    )?;
    let policy = if args.get(1).is_some_and(|arg| !arg.is_nil()) {
        WindowEndQueryPolicy::EnsureCurrent
    } else {
        WindowEndQueryPolicy::LastPresented
    };
    query_window_end(eval, fid, wid, policy)
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
            if selected_live_window && let Some(buffer) = buffers.get(*buffer_id) {
                return Ok(Value::fixnum(
                    buffer.point_char_pos().get().saturating_add(1) as i64,
                ));
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
    let chrome_dirty_window;
    let result = {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        let (fid, wid) =
            resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
        let pos = parse_integer_or_marker_arg(&args[1])?;
        // GNU Fset_window_start: w->force_start = !NILP (noforce) ? 0 : 1 —
        // an explicit start is honored by the next redisplay (point moves
        // into the window if needed) unless NOFORCE asks for the soft mode.
        let force_start_p = args.get(2).is_none_or(|noforce| noforce.is_nil());
        // GNU `Fset_window_start` calls `wset_update_mode_line (w)`
        // (window.c:1969): a moved window start changes `%p`/`%l`, and it is
        // window-scoped, not buffer-scoped.
        chrome_dirty_window = Some(wid);
        let is_minibuffer = frames
            .get(fid)
            .is_some_and(|frame| frame.minibuffer_window == Some(wid));
        match pos {
            IntegerOrMarkerArg::Int(pos) => {
                if !is_minibuffer
                    && let Some(clamped) =
                        clamped_window_position_in_state(frames, buffers, fid, wid, pos)
                    && let Some(window) = frames
                        .get_mut(fid)
                        .and_then(|frame| frame.find_window_mut(wid))
                {
                    crate::window::window_markers::set_window_start_with_marker(
                        buffers, window, clamped,
                    );
                    set_window_force_start(window, force_start_p);
                }
                Value::fixnum(pos)
            }
            IntegerOrMarkerArg::Marker { raw, position } => {
                if !is_minibuffer
                    && let Some(pos) = position
                    && let Some(clamped) =
                        clamped_window_position_in_state(frames, buffers, fid, wid, pos)
                    && let Some(window) = frames
                        .get_mut(fid)
                        .and_then(|frame| frame.find_window_mut(wid))
                {
                    crate::window::window_markers::set_window_start_with_marker(
                        buffers, window, clamped,
                    );
                    set_window_force_start(window, force_start_p);
                }
                raw
            }
        }
    };
    if let Some(window) = chrome_dirty_window {
        eval.mark_chrome_dirty_window(window);
    }
    Ok(result)
}
fn set_window_force_start(window: &mut Window, force: bool) {
    if let Window::Leaf { force_start, .. } = window {
        *force_start = force;
    }
    if force {
        window.invalidate_window_end();
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
            if !is_minibuffer
                && let Some(clamped) =
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
                    if selected_live_window
                        && let Some(buffer_id) = buffer_id
                        && let Some(buffer) = buffers.get(buffer_id)
                    {
                        buffer_to_move =
                            Some((buffer_id, buffer.lisp_pos_to_emacs_byte_pos(clamped)));
                    }
                }
                if let Some((buffer_id, byte_pos)) = buffer_to_move {
                    let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
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
                    if selected_live_window
                        && let Some(buffer_id) = buffer_id
                        && let Some(buffer) = buffers.get(buffer_id)
                    {
                        buffer_to_move =
                            Some((buffer_id, buffer.lisp_pos_to_emacs_byte_pos(clamped)));
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
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("window-live-p"), Value::make_window(raw_id)],
                    ));
                }
                wid
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
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
    // GNU `Fwindow_left_column` returns `w->left_col` directly. See
    // `Window::left_col`.
    Ok(Value::fixnum(w.left_col()))
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
    // GNU `Fwindow_top_line` returns `w->top_line` directly (the stored
    // character-line edge maintained by the resize passes, decoupled from pixel
    // geometry -- it includes FRAME_TOP_MARGIN, which has no pixel height in
    // batch). See `Window::top_line`.
    Ok(Value::fixnum(w.top_line()))
}

fn geometry_invariant(message: impl Into<String>) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "GUI geometry invariant violated: {}",
            message.into()
        ))],
    )
}

/// Return regions from the latest completed redisplay, independent of which
/// presentation the renderer currently has active.
///
/// GNU `window-*` primitives query synchronous editor/current-matrix state;
/// they do not wait for a compositor or renderer acknowledgement.
fn redisplay_window_regions(
    frames: &FrameManager,
    fid: FrameId,
    wid: WindowId,
) -> Result<Option<crate::window::geometry::WindowRegions>, Flow> {
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let Some(snapshot) = frame.redisplay_snapshot(wid) else {
        return Ok(None);
    };
    if !snapshot.regions_materialized {
        return Ok(None);
    }
    crate::window::geometry::WindowRegions::from_transport(&snapshot.regions)
        .map(Some)
        .map_err(|error| geometry_invariant(format!("{error:?}")))
}

fn tty_batch_pixel_left(window: &Window, char_width: f32) -> i64 {
    if char_width > 0.0 {
        (window.bounds().x / char_width) as i64
    } else {
        0
    }
}

fn tty_batch_pixel_top(window: &Window, char_height: f32) -> i64 {
    if char_height > 0.0 {
        (window.bounds().y / char_height) as i64
    } else {
        0
    }
}
/// `(window-pixel-left &optional WINDOW)` -> integer.
///
/// Graphical frames report the stored frame-relative pixel coordinate.  In
/// batch-mode GNU Emacs, this helper reports character-cell units instead.
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
    let frame = frames.get(fid);
    let graphical = frame.is_some_and(|frame| frame.effective_window_system().is_some());
    let cw = frame.map(|frame| frame.char_width).unwrap_or(8.0);
    let left = if graphical {
        // GNU `Fwindow_pixel_left` returns `w->pixel_left` directly.
        w.bounds().x as i64
    } else {
        tty_batch_pixel_left(w, cw)
    };
    Ok(Value::fixnum(left))
}
/// `(window-pixel-top &optional WINDOW)` -> integer.
///
/// Graphical frames report the stored frame-relative pixel coordinate.  In
/// batch-mode GNU Emacs, this helper reports character-cell units instead.
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
    let frame = frames.get(fid);
    let graphical = frame.is_some_and(|frame| frame.effective_window_system().is_some());
    let ch = frame.map(|frame| frame.char_height).unwrap_or(16.0);
    let top = if graphical {
        // GNU `Fwindow_pixel_top` returns `w->pixel_top` directly.
        w.bounds().y as i64
    } else {
        tty_batch_pixel_top(w, ch)
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
    if let Some(Window::Leaf {
        hscroll,
        suspend_auto_hscroll,
        ..
    }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        *hscroll = cols;
        // GNU `set_window_hscroll` (src/window.c:1289) suspends auto hscroll
        // so an explicit set-window-hscroll is not immediately overridden by
        // the auto-hscroll redisplay pass; it is un-suspended once window
        // point explicitly moves (hscroll_window_tree STEP 4).
        *suspend_auto_hscroll = true;
    }
    Ok(Value::fixnum(cols as i64))
}

fn scroll_prefix_value(value: &Value) -> i64 {
    crate::emacs_core::prefix::prefix_numeric_value(value)
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
    // GNU `scroll-left` (src/window.c:7113): the optional second argument
    // SET-MINIMUM (non-nil in an interactive call via the `\np` spec) makes
    // the new scroll amount the lower bound for automatic hscrolling.
    let set_minimum = args.get(1).is_some_and(|v| !v.is_nil());
    if let Some(Window::Leaf {
        hscroll,
        min_hscroll,
        suspend_auto_hscroll,
        ..
    }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        *hscroll = next as usize;
        if set_minimum {
            *min_hscroll = *hscroll;
        }
        // GNU suspends auto hscroll after any scroll-left/right so the manual
        // scroll position is honored until window point explicitly moves.
        *suspend_auto_hscroll = true;
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
    // GNU `scroll-right` (src/window.c:7139): mirror of scroll-left.
    let set_minimum = args.get(1).is_some_and(|v| !v.is_nil());
    if let Some(Window::Leaf {
        hscroll,
        min_hscroll,
        suspend_auto_hscroll,
        ..
    }) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        *hscroll = next as usize;
        if set_minimum {
            *min_hscroll = *hscroll;
        }
        *suspend_auto_hscroll = true;
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
    if let Some(geometry) = redisplay_window_regions(frames, fid, wid)? {
        let regions = geometry;
        let left = regions.left_margin_columns();
        let right = regions.right_margin_columns();
        return Ok(Value::cons(
            if left == 0 {
                Value::NIL
            } else {
                Value::fixnum(left)
            },
            if right == 0 {
                Value::NIL
            } else {
                Value::fixnum(right)
            },
        ));
    }
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
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if let Some(geometry) = redisplay_window_regions(frames, fid, wid)? {
        let regions = geometry;
        let left = regions
            .left_fringe()
            .map_or(0, |rect| rect.width().get() as i64);
        let right = regions
            .right_fringe()
            .map_or(0, |rect| rect.width().get() as i64);
        let (_, _, outside, persistent) =
            frames.window_fringes(wid).unwrap_or((0, 0, false, false));
        return Ok(Value::list(vec![
            Value::fixnum(left),
            Value::fixnum(right),
            if outside { Value::T } else { Value::NIL },
            if persistent { Value::T } else { Value::NIL },
        ]));
    }
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
                LispCondition::ArgsOutOfRange,
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
                    LispCondition::ArgsOutOfRange,
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
                    LispCondition::ArgsOutOfRange,
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
                    LispCondition::ArgsOutOfRange,
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
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if let Some(geometry) = redisplay_window_regions(frames, fid, wid)? {
        let regions = geometry;
        let width = regions
            .left_scroll_bar()
            .or(regions.right_scroll_bar())
            .map_or(0, |rect| rect.width().get() as i64);
        return Ok(Value::fixnum(width));
    }
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
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if let Some(geometry) = redisplay_window_regions(frames, fid, wid)? {
        let height = geometry
            .horizontal_scroll_bar()
            .map_or(0, |rect| rect.height().get() as i64);
        return Ok(Value::fixnum(height));
    }
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
    )?;
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
    )?))
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
    )?))
}

#[derive(Clone, Copy)]
// Variant names are the GNU-visible chrome concepts used at call sites.
#[allow(clippy::enum_variant_names)]
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
) -> Result<i64, Flow> {
    if let Some(geometry) = redisplay_window_regions(frames, fid, wid)? {
        let regions = geometry;
        return Ok(match metric {
            WindowChromeMetric::ModeLine => regions.mode_line(),
            WindowChromeMetric::HeaderLine => regions.header_line(),
            WindowChromeMetric::TabLine => regions.tab_line(),
        }
        .map_or(0, |rect| rect.height().get() as i64));
    }
    Ok(frames
        .get(fid)
        .and_then(|frame| frame.redisplay_snapshot(wid))
        .map(|snapshot| match metric {
            WindowChromeMetric::ModeLine => snapshot.mode_line_height,
            WindowChromeMetric::HeaderLine => snapshot.header_line_height,
            WindowChromeMetric::TabLine => snapshot.tab_line_height,
        })
        .unwrap_or(fallback)
        .max(0))
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
    // GNU `Fwindow_pixel_height` returns `w->pixel_height` directly.  This is
    // synchronous window-layout state and exists before the first redisplay;
    // it is not a query against the last frame presented by the renderer.
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
    // GNU `Fwindow_pixel_width` returns `w->pixel_width` directly.  Keep this
    // public Lisp primitive on the logical-layout side of the geometry seam.
    Ok(Value::fixnum(window_width_pixels(w)))
}
/// `(window-body-height &optional WINDOW PIXELWISE)` -> integer.
///
/// Returns the body height of WINDOW.  PIXELWISE follows GNU's three-state
/// contract: nil uses canonical lines, `remap` uses the buffer-remapped
/// default face, and every other non-nil value uses pixels.
/// Body excludes mode-line (one row) for non-minibuffer windows.
pub(crate) fn builtin_window_body_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    expect_max_args("window-body-height", &args, 2)?;
    let unit = window_body_unit_from_lisp(args.get(1));
    let _ = ensure_selected_frame_id_in_state(&mut eval.frames, &mut eval.buffers);
    let (fid, wid) = resolve_window_id_with_pred_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "window-live-p",
    )?;
    let remapped = remapped_window_body_cell_size(eval, fid, unit);
    window_body_height_for_window(&eval.frames, fid, wid, unit, remapped)
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
    let unit = window_body_unit_from_lisp(args.get(1));
    window_body_height_for_window(frames, fid, wid, unit, None)
}

fn canonical_window_body_cell_size(frames: &FrameManager, fid: FrameId) -> WindowBodyCellSize {
    frames
        .get(fid)
        .map(|frame| WindowBodyCellSize::new(frame.char_width, frame.char_height))
        .unwrap_or_else(|| WindowBodyCellSize::new(8.0, 16.0))
}

fn remapped_window_body_cell_size(
    eval: &mut super::eval::Context,
    fid: FrameId,
    unit: WindowBodyUnit,
) -> Option<WindowBodyCellSize> {
    if unit != WindowBodyUnit::RemappedChars {
        return None;
    }
    let font = super::font::resolve_current_buffer_remapped_default_face_font(eval, fid)?;
    Some(WindowBodyCellSize::new(
        font.font.char_width(),
        font.font.line_height(),
    ))
}

fn window_body_height_for_window(
    frames: &FrameManager,
    fid: FrameId,
    wid: WindowId,
    unit: WindowBodyUnit,
    remapped: Option<WindowBodyCellSize>,
) -> EvalResult {
    let window = get_leaf(frames, fid, wid)?;
    let pixels = match redisplay_window_regions(frames, fid, wid)? {
        Some(geometry) => geometry.text_body().height().get() as i64,
        None => {
            let total = window_height_pixels(window);
            if is_minibuffer_window(frames, fid, wid) {
                total
            } else {
                let mode_line_height = frames
                    .get(fid)
                    .map(|frame| frame.char_height.max(0.0) as i64)
                    .unwrap_or(0);
                total.saturating_sub(mode_line_height)
            }
        }
    };
    Ok(Value::fixnum(unit.measure(
        WindowBodyAxis::Height,
        pixels,
        canonical_window_body_cell_size(frames, fid),
        remapped,
    )))
}
/// `(window-body-width &optional WINDOW PIXELWISE)` -> integer.
///
/// Returns the body width of WINDOW.  PIXELWISE follows GNU's three-state
/// contract: nil uses canonical columns, `remap` uses the buffer-remapped
/// default face, and every other non-nil value uses pixels.
pub(crate) fn builtin_window_body_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_pending_resize_events();
    expect_max_args("window-body-width", &args, 2)?;
    let unit = window_body_unit_from_lisp(args.get(1));
    let _ = ensure_selected_frame_id_in_state(&mut eval.frames, &mut eval.buffers);
    let (fid, wid) = resolve_window_id_with_pred_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "window-live-p",
    )?;
    let remapped = remapped_window_body_cell_size(eval, fid, unit);
    let window = get_leaf(&eval.frames, fid, wid)?;
    let pixels = match redisplay_window_regions(&eval.frames, fid, wid)? {
        Some(geometry) => geometry.text_body().width().get() as i64,
        None => window_body_width_pixels(&eval.frames, fid, window),
    };
    Ok(Value::fixnum(unit.measure(
        WindowBodyAxis::Width,
        pixels,
        canonical_window_body_cell_size(&eval.frames, fid),
        remapped,
    )))
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
        let body = match redisplay_window_regions(frames, fid, wid)? {
            Some(geometry) => geometry.text_body().height().get() as i64,
            None => {
                let total = window_height_pixels(w);
                if is_minibuffer_window(frames, fid, wid) {
                    total
                } else {
                    let mode_line_height = frames
                        .get(fid)
                        .map(|frame| frame.char_height.max(0.0) as i64)
                        .unwrap_or(0);
                    total.saturating_sub(mode_line_height)
                }
            }
        };
        Ok(Value::fixnum(body))
    } else {
        let char_height = frames
            .get(fid)
            .map(|frame| frame.char_height.max(1.0))
            .unwrap_or(16.0);
        let height = match redisplay_window_regions(frames, fid, wid)? {
            Some(geometry) => (geometry.text_body().height().get() / char_height).floor() as i64,
            None => window_body_height_lines(frames, fid, wid, w),
        };
        Ok(Value::fixnum(height))
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
        let width = match redisplay_window_regions(frames, fid, wid)? {
            Some(geometry) => geometry.text_body().width().get() as i64,
            None => window_body_width_pixels(frames, fid, w),
        };
        Ok(Value::fixnum(width))
    } else {
        let cw = frames
            .get(fid)
            .map(|f| f.char_width.max(1.0))
            .unwrap_or(8.0);
        let width = match redisplay_window_regions(frames, fid, wid)? {
            Some(geometry) => geometry.text_body().width().get() as i64,
            None => window_body_width_pixels(frames, fid, w),
        };
        Ok(Value::fixnum((width as f32 / cw).floor() as i64))
    }
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("windowp"), *arg],
            ));
        };
        if let Some(window_fid) = frames.find_window_frame_id(wid) {
            Some((wid, window_fid))
        } else if frames.is_window_object_id(wid) {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("window-live-p"), *arg],
            ));
        } else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
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
    if include_minibuffer && let Some(minibuffer_wid) = frame.minibuffer_window {
        ids.push(window_value(minibuffer_wid));
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
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("window-live-p"), args[0]],
                ));
            }
        } else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
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
        if frame_id == fid
            && let Some(start_index) = window_ids.iter().position(|wid| *wid == start_wid)
        {
            window_ids.rotate_left(start_index);
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
                    LispCondition::WrongTypeArgument,
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
        // GNU's `window_loop` starts each search at the selected window of
        // its base frame.  Apart from matching its traversal order, this is
        // the public selection policy of `get-buffer-window`: when the same
        // buffer is displayed more than once, the selected window wins.
        let mut window_ids = frame.window_list();
        if let Some(selected_index) = window_ids
            .iter()
            .position(|window_id| *window_id == frame.selected_window)
        {
            window_ids.rotate_left(selected_index);
        }
        for wid in window_ids {
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
        Window::Leaf { dedicated, .. } => Ok(*dedicated),
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
    let flag = args[1];
    let (fid, wid) =
        resolve_window_id_with_pred_in_state(frames, buffers, args.first(), "window-live-p")?;
    if let Some(w) = frames.get_mut(fid).and_then(|f| f.find_window_mut(wid))
        && let Window::Leaf { dedicated, .. } = w
    {
        *dedicated = flag;
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
        && minibuffer_leaf.bounds().contains(px, py)
    {
        return Ok(window_value(minibuffer_wid));
    }

    Ok(Value::NIL)
}

// ===========================================================================
// Window manipulation
// ===========================================================================

pub(crate) fn split_window_internal_impl_in_state(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    window: Value,
    size: Value,
    side: Value,
    combination_limit: CombinationLimit,
) -> EvalResult {
    split_window_internal_impl_in_state_with_normal(
        frames,
        buffers,
        window,
        size,
        side,
        Value::NIL,
        combination_limit,
    )
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
    combination_limit: CombinationLimit,
) -> EvalResult {
    let (fid, wid) = resolve_window_id_or_error_in_state(frames, buffers, Some(&window))?;

    // GNU's `window_point` reads the selected window's live buffer point.  Keep
    // the leaf cache in sync before cloning the window tree so a same-buffer
    // split inherits that effective point, not a stale marker value.
    remember_selected_window_point_in_state(frames, buffers, fid);

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
        .split_window_with_combination_limit(
            fid,
            wid,
            direction,
            buf_id,
            size_opt,
            placement,
            combination_limit,
        )
        .ok_or_else(|| signal("error", vec![Value::string("Cannot split window")]))?;

    // GNU `Fsplit_window_internal` finishes by staging the new window's own
    // size and normal size and then committing the whole parent combination
    // (`src/window.c:5636-5672`):
    //
    //     wset_new_pixel (n, pixel_size);
    //     wset_new_normal (n, normal_size);
    //     ...
    //     window_resize_apply (p, horflag);
    //
    // Under `window-combination-resize' the new window's space comes from EVERY
    // sibling, not just the split target, and `window.el' has already staged
    // each sibling's share -- so the primitive must apply that plan instead of
    // computing a layout of its own.
    frames.apply_staged_split_sizes(fid, new_wid, size_opt, normal_size, direction);

    // GNU allocates independent start/point/old-point markers for the new
    // live leaf.  `FrameManager::split_window` intentionally clears marker IDs
    // copied from the old leaf; attach fresh markers before returning the Lisp
    // window object so subsequent buffer edits adjust both windows.
    if let Some(frame) = frames.get_mut(fid)
        && let Some(new_window) = frame.find_window_mut(new_wid)
    {
        crate::window::window_markers::attach_window_position_markers(buffers, new_window);
    }

    Ok(window_value(new_wid))
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

    // GNU `Fdelete_window_internal` commits the sizes `lisp/window.el`'s
    // `delete-window` staged in `new_pixel` (`window_resize_apply`), rather
    // than laying the surviving windows out afresh.  The Lisp layer is what
    // decides which sibling absorbs the deleted window's space.
    if frames.delete_window_with_resize(fid, wid, DeleteResize::ApplyStaged) {
        Ok(Value::NIL)
    } else {
        Err(signal("error", vec![Value::string("Deletion failed")]))
    }
}
/// `(delete-other-windows-internal &optional WINDOW ROOT)` -> nil.
///
/// Replace ROOT with its descendant WINDOW, defaulting ROOT to WINDOW's frame
/// root.
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
    let root_wid = if args.get(1).is_none_or(|root| root.is_nil()) {
        frame.root_window.id()
    } else {
        let root_wid = resolve_window_object_id_with_pred_in_state(
            frames,
            buffers,
            args.get(1),
            "window-valid-p",
        )?;
        if frames.find_valid_window_frame_id(root_wid) != Some(fid) {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Specified root is not an ancestor of specified window",
                )],
            ));
        }
        root_wid
    };
    if !frames.keep_only_window_in_subtree(fid, keep_wid, root_wid) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Specified root is not an ancestor of specified window",
            )],
        ));
    }
    let selected_buffer = if let Some(frame) = frames.get_mut(fid) {
        if frame
            .find_window(keep_wid)
            .is_some_and(crate::window::Window::is_leaf)
        {
            frame.select_window(keep_wid);
        }
        frame
            .find_window(frame.selected_window)
            .and_then(|window| window.buffer_id())
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
    buffers: &mut BufferManager,
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
    if let Some(window) = frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(selected_wid))
    {
        crate::window::window_markers::set_window_point_with_marker(
            buffers,
            window,
            lisp_char_pos_from_one_based_usize(point),
        );
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("window-live-p"), args[0]],
            ));
        }
    };
    // GNU `Fselect_window' does `CHECK_LIVE_WINDOW(window)': an internal
    // (non-leaf) window such as `(window-parent W)' is a valid window but not a
    // *live* one, so selecting it signals `wrong-type-argument window-live-p'.
    if !eval.frames.is_live_window_id(wid) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), args[0]],
        ));
    }
    // GNU `select_window` (window.c) marks BOTH the old and the new window
    // for redisplay "since the selected-window has a different mode-line";
    // `wset_redisplay` on a non-selected window raises
    // `windows_or_buffers_changed`, which `redisplay_internal` promotes into
    // `update_mode_lines` (xdisp.c:17545-17550). The observable reason is the
    // mode-line active/inactive face, chosen from the REAL selected window
    // (`CURRENT_MODE_LINE_ACTIVE_FACE_ID_3`, dispextern.h:1541-1549), so both
    // windows' chrome changes. GNU's promotion is frame-wide; so is ours.
    eval.mark_chrome_dirty_all();
    let (record_selection, run_buffer_list_hook, frame_changed) = {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        let selected_fid = ensure_selected_frame_id_in_state(frames, buffers);
        // GNU `select_window' derives WINDOW_FRAME and selects that frame when
        // it differs.  Posframe/transient relies on this for child-frame roots.
        let fid = frames.find_window_frame_id(wid).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("window-live-p"), args[0]],
            )
        })?;
        let record_selection = args.get(1).is_none_or(|v| v.is_nil());
        remember_selected_window_point_in_state(frames, buffers, selected_fid);
        {
            let frame = frames
                .get_mut(fid)
                .ok_or_else(|| signal("error", vec![Value::string("No window frame")]))?;
            if !frame.select_window(wid) {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("window-live-p"), args[0]],
                ));
            }
        }
        let frame_changed = fid != selected_fid;
        if frame_changed && !frames.select_frame(fid) {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("window-live-p"), args[0]],
            ));
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
        if record_selection
            && let Some(buffer_id) = frames
                .get(fid)
                .and_then(|f| f.find_window(wid))
                .and_then(Window::buffer_id)
        {
            record_buffer_in_state(frames, buffers, buffer_id, fid)?;
        }
        let run_buffer_list_hook = record_selection
            && selected_window_buffer_state_in_frame(frames, fid)
                .is_some_and(|(_, buffer_id)| !buffers.buffer_hooks_inhibited(buffer_id));
        (record_selection, run_buffer_list_hook, frame_changed)
    };
    if frame_changed {
        eval.sync_keyboard_terminal_owner();
    }
    if record_selection && run_buffer_list_hook {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(window_value(wid))
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
    // GNU's `set-window-buffer` internals call `wset_update_mode_line (w)`
    // (window.c:4383): the window now shows a different buffer, so every
    // buffer-derived construct in its chrome (`%b`, `%m`, `%*`, and a
    // buffer-local mode-line-format) is stale.
    if let Some(window) = args.first().and_then(window_id_from_designator) {
        eval.mark_chrome_dirty_window(window);
    } else {
        // No designator resolves to a live window id yet (nil means the
        // selected window, resolved below). GNU's site is unconditional, so
        // fall back to the frame-wide flag rather than dropping the event.
        eval.mark_chrome_dirty_all();
    }
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
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("bufferp"), Value::NIL],
                    ));
                }
            },
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
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
                run_buffer_list_hook = record_window_buffer_change_history_in_state(
                    frames,
                    minibuffers,
                    buffers,
                    fid,
                    wid,
                    WindowBufferHistoryChange {
                        outgoing_buffer_id: old_buffer_id,
                        incoming_buffer_id: buf_id,
                        outgoing_window_start: old_window_start,
                        outgoing_window_point: old_point,
                    },
                )?;
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
        }) && let Some(frame) = frames.get_mut(fid)
            && let Some(Window::Leaf { dedicated, .. }) = frame.find_window_mut(wid)
        {
            *dedicated = Value::NIL;
        }
        update_buffer_display_metadata_in_state(buffers, buf_id)?;
        if let Some(frame) = frames.get_mut(fid)
            && let Some(window) = frame.find_window_mut(wid)
        {
            super::super::window::window_markers::attach_window_position_markers(buffers, window);
        }
        if selected_window == Some(wid)
            && let Some(buffer) = buffers.get_mut(buf_id)
        {
            buffer.last_selected_window = Some(wid);
        }
    }
    builtin_run_window_scroll_functions(eval, vec![window_value(wid)])?;
    Ok(Value::NIL)
}

/// Record direction for the next accepted content replacement in WINDOW.
/// Lisp commands own semantic navigation; redisplay consumes this durable,
/// typed intent only after publishing the matching presentation.
pub(crate) fn builtin_neomacs_record_window_navigation_intent(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("neomacs--record-window-navigation-intent", &args, 1)?;
    expect_max_args("neomacs--record-window-navigation-intent", &args, 2)?;
    let direction = navigation_transition_direction(args[0])?;
    let (_, window_id) = resolve_window_id_with_pred(eval, args.get(1), "window-live-p")?;
    eval.frames
        .record_window_navigation_intent(window_id, direction);
    Ok(Value::NIL)
}

/// Record direction for the next accepted frame-content replacement.
pub(crate) fn builtin_neomacs_record_frame_navigation_intent(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("neomacs--record-frame-navigation-intent", &args, 1)?;
    expect_max_args("neomacs--record-frame-navigation-intent", &args, 2)?;
    let direction = navigation_transition_direction(args[0])?;
    let frame_id = resolve_frame_id(eval, args.get(1), "frame-live-p")?;
    eval.frames
        .record_frame_navigation_intent(frame_id, direction);
    Ok(Value::NIL)
}

const MIN_FRAME_COLS: i64 = 10;
pub(crate) const MIN_FRAME_TEXT_LINES: i64 = 5;
pub(crate) const FRAME_TEXT_LINES_PARAM: &str = "neovm--frame-text-lines";
pub(crate) const FRAME_TOTAL_COLS_PARAM: &str = "neovm--frame-total-cols";
pub(crate) const FRAME_TOTAL_LINES_PARAM: &str = "neovm--frame-total-lines";
pub(crate) const LIVE_GUI_RESIZE_ACK_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(1);

#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameSizeParam {
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
pub(crate) enum FrameResizeRequest {
    TextPixels { width: u32, height: u32 },
    Cells { cols: i64, total_lines: i64 },
}

impl FrameResizeRequest {
    pub(crate) fn text_pixels(
        self,
        frames: &FrameManager,
        fid: FrameId,
    ) -> Result<(u32, u32), Flow> {
        match self {
            Self::TextPixels { width, height } => Ok((width.max(1), height.max(1))),
            Self::Cells { cols, total_lines } => {
                live_gui_resize_pixels_from_logical_size(frames, fid, cols, total_lines)
            }
        }
    }

    pub(crate) fn logical_size(
        self,
        frames: &FrameManager,
        fid: FrameId,
    ) -> Result<(i64, i64), Flow> {
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

pub(crate) fn frame_total_cols(frame: &crate::window::Frame) -> i64 {
    frame
        .parameter(FRAME_TOTAL_COLS_PARAM)
        .and_then(|v| v.as_int())
        .or_else(|| frame.parameter("width").and_then(|v| v.as_int()))
        .unwrap_or(frame.columns() as i64)
}

pub(crate) fn frame_is_top_level_non_window(frame: &crate::window::Frame) -> bool {
    frame.effective_window_system().is_none() && frame.parent_frame.as_frame_id().is_none()
}

/// GNU `Fframe_parameters` reports the `height` parameter of a top-level
/// terminal frame from live geometry as FRAME_LINES (frame.c): the whole
/// terminal minus the realized menu-bar / tab-bar rows (the minibuffer stays
/// INCLUDED). `frame.lines()` is FRAME_TOTAL_LINES (the whole terminal), so
/// subtract the top margin here. Only realized (displayed) chrome reduces the
/// count; a non-displayed frame (--batch) keeps FRAME_LINES == FRAME_TOTAL_LINES,
/// matching the batch geometry the oracle pins (frame-total-lines == frame-height).
pub(crate) fn frame_realized_lines(frame: &crate::window::Frame) -> i64 {
    let total = frame.lines() as i64;
    let top_margin = if frame.displays_chrome {
        frame.frame_top_margin()
    } else {
        0
    };
    (total - top_margin).max(1)
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

pub(crate) fn frame_non_text_total_width_pixels_in_state(
    frames: &FrameManager,
    fid: FrameId,
) -> u32 {
    let border = frames
        .get(fid)
        .map(frame_internal_border_total_pixels)
        .unwrap_or(0);
    frame_non_text_width_pixels_in_state(frames, fid).saturating_add(border)
}

pub(crate) fn frame_non_text_total_height_pixels(frame: &crate::window::Frame) -> u32 {
    frame_non_text_height_pixels(frame).saturating_add(frame_internal_border_total_pixels(frame))
}

pub(crate) fn frame_text_width_pixels_in_state(frames: &FrameManager, fid: FrameId) -> u32 {
    let Some(frame) = frames.get(fid) else {
        return 0;
    };
    frame
        .width
        .saturating_sub(frame_non_text_total_width_pixels_in_state(frames, fid))
        .max(1)
}

pub(crate) fn frame_text_height_pixels(frame: &crate::window::Frame) -> u32 {
    frame
        .height
        .saturating_sub(frame_non_text_total_height_pixels(frame))
        .max(1)
}

pub(crate) fn parse_frame_size_param(value: Value) -> Option<FrameSizeParam> {
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

pub(crate) fn frame_size_param_to_cells(param: FrameSizeParam, item_size: f32) -> i64 {
    match param {
        FrameSizeParam::Cells(n) => n,
        FrameSizeParam::TextPixels(px) => {
            ((px as f32) / item_size.max(1.0)).floor().max(1.0) as i64
        }
    }
}

pub(crate) fn frame_size_param_to_pixels(param: FrameSizeParam, item_size: f32) -> u32 {
    match param {
        FrameSizeParam::Cells(n) => {
            let unit = item_size.max(1.0).round() as i64;
            n.saturating_mul(unit).max(1).min(u32::MAX as i64) as u32
        }
        FrameSizeParam::TextPixels(px) => px.max(1),
    }
}

pub(crate) fn frame_total_lines(frame: &crate::window::Frame) -> i64 {
    frame
        .parameter(FRAME_TOTAL_LINES_PARAM)
        .and_then(|v| v.as_int())
        .or_else(|| frame.parameter("height").and_then(|v| v.as_int()))
        .unwrap_or(frame.lines() as i64)
}

fn clamp_frame_dimension(value: i64, minimum: i64) -> i64 {
    value.max(minimum).min(u32::MAX as i64)
}

pub(crate) fn set_frame_text_size(frame: &mut crate::window::Frame, cols: i64, text_lines: i64) {
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

pub(crate) fn resize_live_gui_frame(
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
            fullscreen: None,
        })
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    }

    Ok(())
}

pub(crate) fn request_live_gui_frame_resize(
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
            fullscreen: None,
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

pub(crate) fn flush_pending_live_gui_resize(
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

fn scroll_up_batch_error() -> Flow {
    signal(LispCondition::EndOfBuffer, vec![])
}

fn scroll_down_batch_error() -> Flow {
    signal(LispCondition::BeginningOfBuffer, vec![])
}

impl super::eval::Context {
    /// Smooth scroll (Phase 1, T3a): set `window_id`'s start (marker-backed) to
    /// `new_start` and its vertical pixel-scroll residual to `new_vscroll_px`
    /// (>= 0 — pixels of the top row hidden above the top edge; stored internally
    /// as the negated GNU `w->vscroll`). Marker-backed so the pre-redisplay marker
    /// sync does not undo it. Returns false if the window is gone.
    pub fn apply_pixel_scroll(
        &mut self,
        window_id: WindowId,
        new_start: crate::buffer::LispCharPos1,
        new_vscroll_px: i32,
    ) -> bool {
        let Some(frame_id) = self.frames.find_window_frame_id(window_id) else {
            return false;
        };
        let Some(frame) = self.frames.get_mut(frame_id) else {
            return false;
        };
        let Some(window) = frame.find_window_mut(window_id) else {
            return false;
        };
        crate::window::window_markers::set_window_start_with_marker(
            &mut self.buffers,
            window,
            new_start,
        );
        // vscroll is the GNU `w->vscroll`: stored zero-or-negative, so a residual of
        // N pixels hidden above the top edge is `-N`.
        if let crate::window::Window::Leaf { vscroll, .. } = window {
            *vscroll = -new_vscroll_px.max(0);
        }
        true
    }
}

#[cfg(test)]
mod pixel_scroll_apply_tests {
    #[test]
    fn apply_pixel_scroll_sets_vscroll_and_start() {
        let mut eval = crate::emacs_core::eval::Context::new();
        let buf_id = eval
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        let frame_id = eval
            .frame_manager_mut()
            .create_frame("pscroll", 400, 300, buf_id);
        let wid = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;

        assert!(eval.apply_pixel_scroll(wid, crate::buffer::LispCharPos1::ONE, 7));

        let frame = eval.frame_manager().get(frame_id).expect("frame");
        match frame.find_window(wid).expect("window") {
            crate::window::Window::Leaf {
                window_start,
                vscroll,
                ..
            } => {
                assert_eq!(window_start.as_i64(), 1, "window-start set to one-based 1");
                assert_eq!(*vscroll, -7, "vscroll stored as negated residual");
            }
            _ => panic!("expected leaf window"),
        }
    }
}

fn scroll_lines_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    arg: Option<&Value>,
    direction: i64,
) -> i64 {
    if let Some(v) = arg
        && !v.is_nil()
    {
        if crate::emacs_core::value::eq_value(v, &Value::symbol("-")) {
            let wh = window_body_height_impl(frames, buffers, vec![])
                .ok()
                .and_then(|v| v.as_fixnum())
                .unwrap_or(24);
            let ctx = obarray
                .symbol_value("next-screen-context-lines")
                .and_then(|v| v.as_fixnum())
                .unwrap_or(2);
            return -((wh - ctx).max(1) * direction);
        }
        // Explicit line count.
        let n = match v.kind() {
            ValueKind::Fixnum(n) => n,
            _ => 1,
        };
        return n * direction;
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
    let result = scroll_by_screen_lines(eval, lines);
    eval.invalidate_redisplay();
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
    let result = scroll_by_screen_lines(eval, lines);
    eval.invalidate_redisplay();
    result
}

/// Point's location relative to the screen-line viewport used by scrolling.
///
/// This is deliberately independent of [`crate::window::WindowEndState`]: a
/// stale window-end record says only that redisplay has not refreshed a cache;
/// it says nothing about whether point is visible from the current start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
enum PointViewportLocation {
    Before,
    Visible,
    After,
}

/// The current viewport expressed in buffer positions.
///
/// `exclusive_end` is the first screen-line start below the viewport.  When it
/// reaches `buffer_end`, the end-of-buffer insertion position remains visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
struct ScreenLineViewport {
    start: EmacsBytePos,
    exclusive_end: EmacsBytePos,
    buffer_end: EmacsBytePos,
}

impl ScreenLineViewport {
    fn locate(self, point: EmacsBytePos) -> PointViewportLocation {
        if point < self.start {
            PointViewportLocation::Before
        } else if point < self.exclusive_end
            || (self.exclusive_end == self.buffer_end && point <= self.buffer_end)
        {
            PointViewportLocation::Visible
        } else {
            PointViewportLocation::After
        }
    }
}

/// Why a scroll operation chose its starting position.
///
/// Keeping the reason in the type makes the GNU-compatible recovery path an
/// explicit consequence of point visibility, rather than a boolean cache
/// heuristic hidden in the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
enum ScrollOrigin {
    WindowStart(EmacsBytePos),
    RecoveredAroundPoint(EmacsBytePos),
}

impl ScrollOrigin {
    const fn position(self) -> EmacsBytePos {
        match self {
            Self::WindowStart(position) | Self::RecoveredAroundPoint(position) => position,
        }
    }
}

fn screen_line_viewport(
    eval: &mut super::eval::Context,
    buffer_id: BufferId,
    window_id: WindowId,
    start: EmacsBytePos,
    body_height: i64,
    buffer_end: EmacsBytePos,
) -> Result<ScreenLineViewport, Flow> {
    let exclusive_end = crate::emacs_core::indent::screen_line_motion_target(
        eval,
        buffer_id,
        start,
        Some(Value::make_window(window_id.0)),
        body_height,
    )?
    .0;
    Ok(ScreenLineViewport {
        start,
        exclusive_end,
        buffer_end,
    })
}

fn scroll_origin(
    eval: &mut super::eval::Context,
    buffer_id: BufferId,
    window_id: WindowId,
    window_start: EmacsBytePos,
    point: EmacsBytePos,
    body_height: i64,
) -> Result<ScrollOrigin, Flow> {
    // GNU window_scroll_line_based / window_scroll_pixel_based gate the
    // recovery on `Fpos_visible_in_window_p (PT, window)` — a DISPLAY-STATE
    // predicate, not a geometric one: a never-displayed window (every
    // --batch window, a fresh split before redisplay) answers nil, so GNU
    // scrolls from `vertical-motion -(ht/2)` around point even when point
    // lies geometrically inside [start, start + height).  Asking our own
    // `pos-visible-in-window-p` keeps the two in lock-step (it answers nil
    // when noninteractive, matrix-exact or geometric from the CURRENT start
    // otherwise — so queued interactive scrolls still stay monotonic).
    let point_lisp = match eval.buffers.get(buffer_id) {
        Some(buf) => buf.emacs_byte_pos_to_lisp_char_pos(point),
        None => return Ok(ScrollOrigin::WindowStart(window_start)),
    };
    let visible = crate::emacs_core::xdisp::builtin_pos_visible_in_window_p_ctx(
        eval,
        vec![
            Value::fixnum(point_lisp.as_i64()),
            Value::make_window(window_id.0),
        ],
    )?
    .is_truthy();
    if visible {
        Ok(ScrollOrigin::WindowStart(window_start))
    } else {
        let recovered = crate::emacs_core::indent::screen_line_motion_target(
            eval,
            buffer_id,
            point,
            Some(Value::make_window(window_id.0)),
            -(body_height / 2),
        )?
        .0;
        Ok(ScrollOrigin::RecoveredAroundPoint(recovered))
    }
}

fn scroll_by_screen_lines(eval: &mut super::eval::Context, lines: i64) -> EvalResult {
    let _ = ensure_selected_frame_id_in_state(&mut eval.frames, &mut eval.buffers);
    let (fid, wid) = resolve_window_id_in_state(&mut eval.frames, &mut eval.buffers, None)?;
    let body_height = window_body_height_impl(&mut eval.frames, &mut eval.buffers, vec![])
        .ok()
        .and_then(|v| v.as_fixnum())
        .unwrap_or(24)
        .max(1);
    let (buffer_id, window_point, window_start) = match get_leaf(&eval.frames, fid, wid)? {
        Window::Leaf {
            buffer_id,
            point,
            window_start,
            ..
        } => (*buffer_id, *point, *window_start),
        _ => return Ok(Value::NIL),
    };
    let Some(buf) = eval.buffers.get(buffer_id) else {
        return Ok(Value::NIL);
    };
    let accessible = buf.accessible_emacs_byte_region();
    let selected_live_window = eval
        .frames
        .get(fid)
        .is_some_and(|frame| frame.selected_window == wid);
    let effective_point = if selected_live_window {
        lisp_char_pos_from_one_based_usize(buf.point_char_pos().get().saturating_add(1))
    } else {
        window_point
    };
    let pt = accessible
        .clamp(buf.lisp_pos_to_emacs_byte_pos(effective_point))
        .get();
    let begv = accessible.start().get();
    let zv = accessible.end().get();
    let window_start = accessible.clamp(buf.lisp_pos_to_emacs_byte_pos(window_start));
    let start = scroll_origin(
        eval,
        buffer_id,
        wid,
        window_start,
        EmacsBytePos::new(pt),
        body_height,
    )?
    .position()
    .get();

    let pos;
    let mut next_point = pt;
    if lines > 0 {
        pos = crate::emacs_core::indent::screen_line_motion_target(
            eval,
            buffer_id,
            EmacsBytePos::new(start),
            Some(Value::make_window(wid.0)),
            lines,
        )?
        .0
        .get();
        if pos >= zv {
            return Err(scroll_up_batch_error());
        }
        if pos > pt {
            next_point = pos;
        }
    } else if lines < 0 {
        if start <= begv {
            return Err(scroll_down_batch_error());
        }
        pos = crate::emacs_core::indent::screen_line_motion_target(
            eval,
            buffer_id,
            EmacsBytePos::new(start),
            Some(Value::make_window(wid.0)),
            lines,
        )?
        .0
        .get();
        // GNU window_scroll_line_based: after scrolling backward, a point that
        // fell BELOW the new window is pulled up to the start of the last
        // fully-visible line; a point still visible stays put. When the window
        // now reaches end-of-buffer, `bottom` clamps to ZV and everything up to
        // ZV (including point-max) is visible, so point must NOT be pulled.
        let viewport = screen_line_viewport(
            eval,
            buffer_id,
            wid,
            EmacsBytePos::new(pos),
            body_height,
            EmacsBytePos::new(zv),
        )?;
        match viewport.locate(EmacsBytePos::new(pt)) {
            PointViewportLocation::After => {
                next_point = crate::emacs_core::indent::screen_line_motion_target(
                    eval,
                    buffer_id,
                    viewport.exclusive_end,
                    Some(Value::make_window(wid.0)),
                    -1,
                )?
                .0
                .get();
            }
            PointViewportLocation::Before | PointViewportLocation::Visible => {}
        }
    } else {
        pos = start;
    }

    let Some(buf) = eval.buffers.get(buffer_id) else {
        return Ok(Value::NIL);
    };
    let start_lisp = buf.emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(pos));
    let point_lisp = buf.emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(next_point));
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, EmacsBytePos::new(next_point));
    if let Some(window) = eval
        .frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        crate::window::window_markers::set_window_point_with_marker(
            &mut eval.buffers,
            window,
            point_lisp,
        );
        crate::window::window_markers::set_window_start_with_marker(
            &mut eval.buffers,
            window,
            start_lisp,
        );
        window.invalidate_window_end();
        if let Window::Leaf {
            vscroll,
            preserve_vscroll_p,
            force_start,
            ..
        } = window
        {
            *vscroll = 0;
            *preserve_vscroll_p = false;
            // GNU window_scroll sets w->force_start: the next redisplay must
            // display from this start and move point into the window if it
            // ended up outside, never recompute the start around point.
            *force_start = true;
        }
    }
    Ok(Value::NIL)
}

/// `(recenter &optional ARG REDISPLAY)` — center point in window.
///
/// Mirror GNU Emacs Frecenter (window.c): adjust window-start so that
/// point appears at the center of the window, or at line ARG from the
/// top (or bottom if ARG is negative).
pub(crate) fn builtin_recenter(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("recenter", &args, 2)?;
    let Some((fid, wid, buffer_id, pt, target_line)) = recenter_backward_origin(eval, &args)?
    else {
        return Ok(Value::NIL);
    };

    // Move back `target_line` SCREEN lines, through the same display-motion
    // seam `vertical-motion` and window scrolling use. GNU's positive-ARG
    // branch runs the display iterator for exactly this --  `start_display`,
    // `move_it_by_lines (&it, 0)` onto the head of point's screen line, then
    // `move_it_by_lines (&it, -nlines)` (src/window.c:7395-7407) -- so
    // invisible text, continuation rows and display properties all count the
    // way redisplay counts them. Walking buffer newlines here instead made a
    // hidden line consume one of the ARG lines and left window-start one line
    // short of GNU's.
    let (pos, _moved) = crate::emacs_core::indent::screen_line_motion_target(
        eval,
        buffer_id,
        pt,
        None,
        -target_line,
    )?;

    {
        let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
        let Some(buf) = buffers.get(buffer_id) else {
            return Ok(Value::NIL);
        };
        let pos_lisp = buf.emacs_byte_pos_to_lisp_char_pos(pos).as_i64();
        if let Some(clamped) = clamped_window_position_in_state(frames, buffers, fid, wid, pos_lisp)
            && let Some(window) = frames
                .get_mut(fid)
                .and_then(|frame| frame.find_window_mut(wid))
        {
            crate::window::window_markers::set_window_start_with_marker(buffers, window, clamped);
            window.invalidate_window_end();
            if let Window::Leaf {
                vscroll,
                preserve_vscroll_p,
                ..
            } = window
            {
                *vscroll = 0;
                *preserve_vscroll_p = false;
            }
        }
    }

    eval.invalidate_redisplay();
    Ok(Value::NIL)
}

/// Resolve everything `recenter` needs before it moves: the window to restart,
/// the buffer and point it restarts around, and how many screen lines above
/// point the new window-start sits.
///
/// `Ok(None)` means there is nothing to recenter (the selected window is not a
/// leaf), which GNU also answers with nil.
#[allow(clippy::type_complexity)]
fn recenter_backward_origin(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> Result<Option<(FrameId, WindowId, BufferId, EmacsBytePos, i64)>, Flow> {
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
                let quoting_style =
                    crate::emacs_core::coding::effective_text_quoting_style(&eval.obarray);
                let message = crate::emacs_core::coding::requote_c_error_message(
                    "`recenter'ing a window that does not display current-buffer",
                    quoting_style,
                );
                return Err(signal("error", vec![Value::string(message)]));
            }
            let point = buffers
                .get(*buffer_id)
                .map(|buf| {
                    lisp_char_pos_from_one_based_usize(buf.point_char_pos().get().saturating_add(1))
                })
                .unwrap_or(*point);
            (*buffer_id, point)
        }
        _ => return Ok(None),
    };
    let Some(buf) = buffers.get(buffer_id) else {
        return Ok(None);
    };
    let accessible = buf.accessible_emacs_byte_region();
    let pt = accessible.clamp(buf.lisp_pos_to_emacs_byte_pos(window_point));

    Ok(Some((fid, wid, buffer_id, pt, target_line)))
}

// ===========================================================================
// Frame operations
// ===========================================================================

pub(crate) fn selected_frame_impl(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("selected-frame", &args, 0)?;
    let fid = ensure_selected_frame_id_in_state(frames, buffers);
    Ok(Value::make_frame(fid.0))
}
#[derive(Clone, Copy)]
enum ChildMinibuffer {
    Own,
    Shared(WindowId),
    Only,
}

fn resolve_child_shared_minibuffer(
    frames: &FrameManager,
    parent_id: FrameId,
    minibuffer_param: Option<Value>,
) -> Result<ChildMinibuffer, Flow> {
    let Some(minibuffer_param) = minibuffer_param else {
        return Ok(ChildMinibuffer::Own);
    };

    if minibuffer_param.is_nil() || matches!(minibuffer_param.as_symbol_name(), Some("none")) {
        return Ok(frames
            .root_frame_id(parent_id)
            .and_then(|root_id| frames.get(root_id))
            .and_then(|root| root.minibuffer_window)
            .map_or(ChildMinibuffer::Own, ChildMinibuffer::Shared));
    }
    if matches!(minibuffer_param.as_symbol_name(), Some("only")) {
        return Ok(ChildMinibuffer::Only);
    }

    let Some(raw_window_id) = minibuffer_param.as_window_id() else {
        return Ok(ChildMinibuffer::Own);
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
        Ok(ChildMinibuffer::Shared(window_id))
    } else {
        Err(signal(
            "error",
            vec![Value::string(
                "The `minibuffer' parameter does not specify a valid minibuffer window",
            )],
        ))
    }
}

#[cfg(test)]
pub(crate) fn make_frame_plain(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    make_frame_plain_on_terminal(frames, buffers, args, 0, 800, 600)
}

pub(crate) fn make_frame_plain_on_terminal(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
    terminal_id: u64,
    default_width: u32,
    default_height: u32,
) -> EvalResult {
    expect_max_args("make-frame", &args, 1)?;
    let mut width = default_width;
    let mut height = default_height;
    let mut requested_width = None;
    let mut requested_height = None;
    let mut requested_name = None;
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
    if let Some(params) = args.first()
        && let Some(items) = super::value::list_to_vec(params)
    {
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
                                requested_name = (!value.is_nil()).then_some(value);
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

    // GNU `Fmake_terminal_frame` consumes `frame_next_F_name` for every new
    // terminal frame before applying an optional explicit `name` parameter.
    // Keep that presentation sequence independent of FrameId allocation.
    let generated_name = frames.next_generated_tty_frame_name();
    let explicit_name = requested_name.is_some();
    let name = requested_name.unwrap_or(generated_name);

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
            let child_minibuffer =
                resolve_child_shared_minibuffer(frames, parent_id, minibuffer_param)?;
            let minibuffer_buffer_id = if matches!(child_minibuffer, ChildMinibuffer::Only) {
                Some(
                    buffers
                        .find_buffer_by_name(" *Minibuf-0*")
                        .unwrap_or_else(|| buffers.create_buffer(" *Minibuf-0*")),
                )
            } else {
                None
            };
            let z_order = 1 + frames.max_child_z_order(parent_id);
            let frame = frames
                .get_mut(fid)
                .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
            if explicit_name {
                frame.set_name_value(name);
            } else {
                frame.set_generated_name_value(name);
            }
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
            match child_minibuffer {
                ChildMinibuffer::Shared(shared_minibuffer) => {
                    frame.minibuffer_leaf = None;
                    frame.minibuffer_window = Some(shared_minibuffer);
                }
                ChildMinibuffer::Only => {
                    frame.minibuffer_leaf = None;
                    frame.minibuffer_window = Some(frame.root_window.id());
                    frame.no_split = true;
                    if let Some(minibuffer_buffer_id) = minibuffer_buffer_id
                        && let Window::Leaf { buffer_id, .. } = &mut frame.root_window
                    {
                        *buffer_id = minibuffer_buffer_id;
                    }
                }
                ChildMinibuffer::Own => {}
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
            if let ChildMinibuffer::Shared(shared_minibuffer) = child_minibuffer {
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
            crate::window::window_markers::attach_frame_window_position_markers(buffers, frame);
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
    let fid = frames.create_frame_value_on_terminal(name, terminal_id, width, height, buf_id);
    if let Some(frame) = frames.get_mut(fid) {
        // This constructor is exclusively for termcap frames.  Their geometry
        // is already expressed in character cells, and GNU initializes both
        // `column_width' and `line_height' to one in `make_frame'.  Keeping the
        // invariant here prevents a TTY created by a GUI daemon from inheriting
        // the GUI Frame defaults (8x16) and laying out only a fraction of its
        // actual columns.
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        if let Some(minibuffer) = frame.minibuffer_leaf.as_mut() {
            let bounds = *minibuffer.bounds();
            minibuffer.set_bounds(crate::window::Rect::new(
                bounds.x,
                bounds.y,
                bounds.width,
                1.0,
            ));
        }
        if explicit_name {
            frame.set_name_value(name);
        } else {
            frame.set_generated_name_value(name);
        }
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
        frame.sync_window_area_bounds();
        crate::window::window_markers::attach_frame_window_position_markers(buffers, frame);
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
    fullscreen: Option<FrameFullscreen>,
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
    device_scale_factor: f64,
}

pub(crate) fn stringish_value(value: &Value) -> Option<Value> {
    match value.kind() {
        ValueKind::String => Some(*value),
        ValueKind::Symbol(id) => Some(Value::string(resolve_sym(id))),
        _ => None,
    }
}

pub(crate) fn frame_name_parameter_value(value: &Value) -> Option<Value> {
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
            "fullscreen" => parsed.fullscreen = FrameFullscreen::from_symbol_value(&pair_cdr),
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
            device_scale_factor: frame.device_scale_factor,
        };
    }
    GuiFrameMetrics {
        width_px: 960,
        height_px: 640,
        char_width: 8.0,
        char_height: 16.0,
        font_pixel_size: 16.0,
        minibuffer_height: 32.0,
        device_scale_factor: 1.0,
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
    let explicit_font_value = Value::symbol("font")
        .as_symbol_id()
        .and_then(|font_key| parsed.all.get(&font_key).copied());
    let explicit_font = explicit_font_value.is_some();
    let parent_id = parsed
        .parent_frame
        .filter(|parent_id| frames.get(*parent_id).is_some());
    let parent_frame_value = parent_id
        .map(|parent_id| Value::make_frame(parent_id.0))
        .unwrap_or(Value::NIL);
    let inherited_font_state = if explicit_font {
        None
    } else {
        parent_id
            .and_then(|parent_id| frames.get(parent_id))
            .and_then(|parent| {
                let public_font = parent.known_parameter(FrameParam::Font)?;
                let font_parameter = parent.parameter("font-parameter")?;
                Some((public_font, font_parameter))
            })
    };
    let inherited_display_identity = parent_id
        .and_then(|parent_id| frames.get(parent_id))
        .or_else(|| frames.selected_frame())
        .map(|frame| frame.display_identity().clone())
        .unwrap_or_default();
    let metrics = parent_id
        .and_then(|parent_id| frames.get(parent_id))
        .map(|parent| GuiFrameMetrics {
            width_px: parent.width.max(1),
            height_px: parent.height.max(1),
            char_width: parent.char_width.max(1.0),
            char_height: parent.char_height.max(1.0),
            font_pixel_size: parent.font_pixel_size.max(1.0),
            device_scale_factor: parent.device_scale_factor,
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
    let height_px = text_height_px.unwrap_or_else(|| {
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
    let child_minibuffer = parent_id
        .map(|parent_id| resolve_child_shared_minibuffer(frames, parent_id, parsed.minibuffer))
        .transpose()?
        .unwrap_or(ChildMinibuffer::Own);
    let z_order = parent_id.map(|parent_id| 1 + frames.max_child_z_order(parent_id));
    let minibuffer_buffer_id = if matches!(child_minibuffer, ChildMinibuffer::Only) {
        Some(
            buffers
                .find_buffer_by_name(" *Minibuf-0*")
                .unwrap_or_else(|| buffers.create_buffer(" *Minibuf-0*")),
        )
    } else {
        buffers.find_buffer_by_name(" *Minibuf-0*")
    };
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
        frame.device_scale_factor = metrics.device_scale_factor;
        frame.set_window_system(Some(Value::symbol(
            crate::emacs_core::display::gui_window_system_symbol(),
        )));
        frame.set_display_identity(inherited_display_identity);
        // NO `display-type' and NO `background-mode' here either.  GNU's
        // `x-create-frame' (src/xfns.c:4916) does not set them; its Lisp
        // caller `x-create-frame-with-faces' (lisp/faces.el:2242-2243) does,
        // by running `(frame-set-background-mode frame t)' and then
        // `(face-set-after-frame-default frame parameters)' -- the same pair
        // `tty-set-up-initial-frame-faces' runs for a terminal frame.
        // `frame.el's `make-frame' funcalls `frame-creation-function', which
        // reaches that Lisp, so a frame built through Lisp still gets both.
        // DIVERGENCES.md 157.
        frame.install_gnu_gui_default_parameters();
        if let Some((public_font, font_parameter)) = inherited_font_state {
            frame.set_known_parameter(FrameParam::Font, public_font);
            frame.set_parameter(Value::symbol("font-parameter"), font_parameter);
        }
        for (key, value) in parsed.all {
            frame.set_parameter_key(FrameParamKey::from_symbol_id(key), value);
        }
        frame.set_known_parameter(FrameParam::ParentFrame, parent_frame_value);
        frame.set_parameter(Value::symbol("left"), Value::fixnum(frame.left_pos));
        frame.set_parameter(Value::symbol("top"), Value::fixnum(frame.top_pos));
        match child_minibuffer {
            ChildMinibuffer::Shared(shared_minibuffer) => {
                frame.minibuffer_leaf = None;
                frame.minibuffer_window = Some(shared_minibuffer);
                frame.set_parameter(
                    Value::symbol("minibuffer"),
                    Value::make_window(shared_minibuffer.0),
                );
            }
            ChildMinibuffer::Only => {
                frame.minibuffer_leaf = None;
                frame.minibuffer_window = Some(frame.root_window.id());
                frame.no_split = true;
            }
            ChildMinibuffer::Own => {}
        }
        let root_buffer_id = if matches!(child_minibuffer, ChildMinibuffer::Only) {
            minibuffer_buffer_id.unwrap_or(current_buffer_id)
        } else {
            current_buffer_id
        };
        if let Window::Leaf { buffer_id, .. } = &mut frame.root_window {
            *buffer_id = root_buffer_id;
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
        crate::window::window_markers::attach_frame_window_position_markers(buffers, frame);
    }
    if let Some(font_value) = explicit_font_value {
        super::font::sync_live_frame_font_parameter_in_state(frames, display_host, fid, font_value);
        if let Some(frame) = frames.get_mut(fid) {
            frame.sync_window_area_bounds();
        }
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
            fullscreen: parsed.fullscreen,
        })
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    }
    if is_child_frame {
        tracing::info!(
            frame_id = fid.0,
            parent_frame_id = parent_id.map(|parent| parent.0).unwrap_or(0),
            visible = frames.get(fid).is_some_and(|frame| frame.visible),
            width_px,
            height_px,
            left = parsed.left.unwrap_or(0),
            top = parsed.top.unwrap_or(0),
            "child_frame_lifecycle: core_created"
        );
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

pub(crate) fn other_frames_in_state(
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
            tracing::info!(
                frame_id = fid.0,
                "child_frame_lifecycle: core_delete_notify_remove"
            );
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
    if mode.allows_terminal_cascade()
        && terminal_is_empty
        && !eval.frames.frame_list().is_empty()
        && let Some(terminal) =
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
        frame.effective_divider_width(FrameDivider::Bottom)
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
        frame.effective_divider_width(FrameDivider::Right)
    };
    Ok(Value::fixnum(width))
}

// `frame-initial-p` lives in `emacs_core::terminal::pure`, where GNU keeps it
// (src/terminal.c): its argument is a frame OR a terminal, and only the
// terminal module can answer the terminal half.

// ===========================================================================
// Bootstrap variables
// ===========================================================================

pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    use crate::emacs_core::value::Value;

    // window.c:9541 DEFVAR_LISP,
    // `window_dead_windows_table = CALLN (Fmake_hash_table, QCweakness, Qvalue)'.
    // The weakness is the point: `record_killed_window' puts every dead window
    // in here so `window-restore-killed-buffer-windows' can find it again, and
    // a strong table would keep every window ever killed alive.  An `eql' table
    // with `:weakness value' is what GNU builds; nil would not be a weaker
    // version of it, it would signal on the first `puthash'.
    obarray.define_special_variable(
        "window-dead-windows-table",
        Value::hash_table_with_options(
            crate::emacs_core::value::HashTableTest::Eql,
            0,
            Some(crate::emacs_core::value::HashTableWeakness::Value),
            1.5,
            0.8125,
        ),
    );
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
    // GNU window.c declares all of these through DEFVAR_LISP.  Register their
    // value and dynamic-binding semantics atomically so lexical package code
    // cannot observe a bound-but-non-special hook variable.
    for name in [
        "delete-frame-functions",
        "after-delete-frame-functions",
        "window-buffer-change-functions",
        "window-size-change-functions",
        "window-selection-change-functions",
        "window-state-change-functions",
        "window-state-change-hook",
    ] {
        obarray.define_special_variable(name, Value::NIL);
    }
    obarray.set_symbol_value("window-sides-vertical", Value::NIL);
    obarray.set_symbol_value("window-sides-slots", Value::NIL);
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
    // window.c:9270 DEFVAR_LISP, init nil.
    obarray.define_special_variable("window-point-insertion-type", Value::NIL);
    // window.c:9247 DEFVAR_INT, init 2.
    obarray.define_int_variable("next-screen-context-lines", 2);
    obarray.set_symbol_value("scroll-error-top-bottom", Value::NIL);
    obarray.set_symbol_value(
        "temp-buffer-max-height",
        Value::make_float(1.0 / 3.0), // (/ (frame-height) 3) approximation
    );
    obarray.set_symbol_value("temp-buffer-max-width", Value::NIL);
    // `even-window-sizes' is NOT a C variable in GNU -- it is defined purely by
    // `(defcustom even-window-sizes t)' in window.el. A Rust bootstrap value
    // here would win (defcustom never overwrites an already-bound variable) and
    // shadow the .el default, so we deliberately do not seed it: neomacs's
    // window.el provides the value, matching GNU.
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

    // GNU `Fwindow_resize_apply_total` (window.c:5016) roots the character-line
    // geometry at the frame's top margin: `r->left_col = 0; r->top_line =
    // FRAME_TOP_MARGIN(f)`. In batch the margin is a line count with no pixel
    // height, so the root's top_line sits below the menu/tab-bar rows while its
    // pixel top stays 0. The recursive pass then flows the char edges to
    // children.
    let top_margin = frame.frame_top_margin();
    frame.root_window.set_left_col(0);
    frame.root_window.set_top_line(top_margin);
    crate::window::window_resize_apply_total(&mut frame.root_window, horflag, cw, ch);

    // Handle minibuffer window — its `new_total` lives on the
    // minibuffer leaf itself now.
    if !horflag
        && frame.minibuffer_window.is_some()
        && let Some(mb) = frame.minibuffer_leaf.as_mut()
        && let Some(new_total) = mb.new_total()
    {
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

    // Ensure root + minibuffer fit in frame after total resize.
    frame.recalculate_minibuffer_bounds();

    Ok(Value::T)
}

// ===========================================================================
// balance-windows
// ===========================================================================

// ===========================================================================
// enlarge-window / shrink-window
// ===========================================================================

// ===========================================================================
// window-tree
// ===========================================================================

// ===========================================================================
// fit-window-to-buffer
// ===========================================================================

/// (force-window-update &optional OBJECT) -> t/nil
///
/// GNU `Fforce_window_update` (`src/window.c:4488`):
///
/// - nil OBJECT: mark everything for redisplay, return t.
/// - a live WINDOW: mark that window, return t.
/// - a buffer/string: return t iff that buffer is shown in some window.
///
/// neomacs has no incremental redisplay state to mark, but the *return value*
/// is observable (oracle test cx409): a live window must yield t, not nil.
/// The previous stub returned nil for *every* non-nil OBJECT, which is wrong
/// for the common `(force-window-update (selected-window))` call.
pub(crate) fn builtin_force_window_update(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("force-window-update", &args, 1)?;
    let Some(object) = args.first().filter(|v| !v.is_nil()) else {
        // nil OBJECT: force all windows.
        return Ok(Value::T);
    };

    // A live window forces just that window and returns t.
    if let Some(id) = object.as_window_id()
        && eval.frames.is_live_window_id(WindowId(id))
    {
        return Ok(Value::T);
    }

    // A buffer (or buffer name) shown in at least one window also returns t in
    // GNU; otherwise (dead window, unshown buffer, anything else) the value is
    // nil -- the safe default neomacs already produced for those cases.
    Ok(Value::NIL)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests;
