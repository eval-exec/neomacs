//! GNU Emacs frame.c builtin surface.
//!
//! Frame-object builtins (framep, frame-live-p, selected-frame, frame
//! geometry and parameter accessors, visibility, focus, deletion) that
//! GNU implements in frame.c. The Frame/FrameManager data structures
//! live in crate::window; window.c builtins stay in super::window_cmds.

use super::error::Flow;
use super::error::{EvalResult, LispCondition, signal};
use super::intern::resolve_sym;
use super::value::{Value, ValueKind, VecLikeType};
use super::window_cmds::{
    DeleteFrameMode, FRAME_TEXT_LINES_PARAM, FRAME_TOTAL_COLS_PARAM, FRAME_TOTAL_LINES_PARAM,
    FrameResizeRequest, FrameSizeParam, LIVE_GUI_RESIZE_ACK_TIMEOUT, MIN_FRAME_TEXT_LINES,
    delete_frame_owned, ensure_selected_frame_id_in_state, expect_int,
    flush_pending_live_gui_resize, frame_is_top_level_non_window, frame_name_parameter_value,
    frame_non_text_total_height_pixels, frame_non_text_total_width_pixels_in_state,
    frame_realized_lines, frame_size_param_to_cells, frame_size_param_to_pixels,
    frame_text_height_pixels, frame_text_width_pixels_in_state, frame_total_cols,
    frame_total_lines, make_frame_plain_on_terminal, other_frames_in_state, parse_frame_size_param,
    remember_selected_window_point_in_state, request_live_gui_frame_resize, resize_live_gui_frame,
    resolve_frame_id, resolve_frame_id_in_state, selected_frame_impl, set_frame_text_size,
    stringish_value, sync_selected_window_buffer_in_state,
};
use crate::buffer::{BufferId, BufferManager};
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::window::FrameManager;
use crate::window::{FrameDivider, FrameFullscreen, FrameId, FrameParam, FrameParamKey};

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

/// `(iconify-frame &optional FRAME)` -> nil.
pub(crate) fn builtin_iconify_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = (&mut eval.frames, &mut eval.buffers);
    expect_max_args("iconify-frame", &args, 1)?;
    let fid = resolve_frame_id_in_state(frames, buffers, args.first(), "frame-live-p")?;
    let _frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    set_frame_visibility(eval, fid, false)?;
    Ok(Value::NIL)
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
                LispCondition::WrongTypeArgument,
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
                    sync_selected_window_buffer_in_state(&eval.frames, &mut eval.buffers, fallback);
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

/// `(selected-frame)` -> frame object.
pub(crate) fn builtin_selected_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    selected_frame_impl(&mut eval.frames, &mut eval.buffers, args)
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
                    LispCondition::WrongTypeArgument,
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
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("frame-live-p"), Value::make_frame(raw_id)],
                ));
            }
            fid
        }
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("frame-live-p"), args[0]],
            ));
        }
    };
    if let Some(old_fid) = frames.selected_frame().map(|frame| frame.id) {
        remember_selected_window_point_in_state(frames, buffers, old_fid);
    }
    if !frames.select_frame(fid) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), args[0]],
        ));
    }
    if args.get(1).is_none_or(|v| v.is_nil())
        && let Some(selected_wid) = frames.get(fid).map(|f| f.selected_window)
    {
        let _ = frames.note_window_selected(selected_wid);
    }
    sync_selected_window_buffer_in_state(frames, buffers, fid);
    eval.sync_keyboard_terminal_owner();
    Ok(Value::make_frame(fid.0))
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
    } else if ctx
        .frames
        .get(fid)
        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some())
    {
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
    } else if ctx
        .frames
        .get(fid)
        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some())
    {
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
    } else if ctx
        .frames
        .get(fid)
        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some())
    {
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
    if frame.effective_window_system().is_some() || frame.parent_frame.as_frame_id().is_some() {
        frame.left_pos = x;
        frame.top_pos = y;
        frame.set_parameter(Value::symbol("left"), Value::fixnum(x));
        frame.set_parameter(Value::symbol("top"), Value::fixnum(y));
    }
    Ok(Value::T)
}

/// `(set-frame-size-and-position-pixelwise FRAME WIDTH HEIGHT LEFT TOP &optional GRAVITY)` -> nil.
pub(crate) fn builtin_set_frame_size_and_position_pixelwise(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-frame-size-and-position-pixelwise", &args, 5)?;
    expect_max_args("set-frame-size-and-position-pixelwise", &args, 6)?;
    let fid = resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let left = expect_int(&args[3])?;
    let top = expect_int(&args[4])?;
    if let Some(gravity) = args.get(5)
        && gravity.is_truthy()
    {
        let gravity = expect_int(gravity)?;
        if !(0..=10).contains(&gravity) {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![
                    *args.get(5).expect("gravity"),
                    Value::fixnum(0),
                    Value::fixnum(10),
                ],
            ));
        }
    }

    let uses_window_system_pixels = eval
        .frames
        .get(fid)
        .is_some_and(frame_uses_window_system_pixels);
    let is_child_frame = eval
        .frames
        .get(fid)
        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_some());

    if uses_window_system_pixels || is_child_frame {
        let frame = eval
            .frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        frame.left_pos = left;
        frame.top_pos = top;
        frame.set_parameter(Value::symbol("left"), Value::fixnum(left));
        frame.set_parameter(Value::symbol("top"), Value::fixnum(top));
    } else {
        let frame = eval
            .frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        frame.set_parameter(Value::symbol("left"), Value::fixnum(left));
        frame.set_parameter(Value::symbol("top"), Value::fixnum(top));
    }

    builtin_set_frame_size(eval, vec![args[0], args[1], args[2], Value::T])?;
    if eval
        .frames
        .get(fid)
        .is_some_and(frame_is_top_level_non_window)
    {
        let width = check_frame_pixels(&args[1], true, 1.0)?;
        let height = check_frame_pixels(&args[2], true, 1.0)?;
        let frame = eval
            .frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        set_top_level_non_window_pixelwise_totals(frame, width, height);
    }
    Ok(Value::NIL)
}

/// `(make-terminal-frame PARMS)` -> frame.
pub(crate) fn builtin_make_terminal_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("make-terminal-frame", &args, 1)?;
    if !args[0].is_nil() && !args[0].is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[0]],
        ));
    }
    let tty_device = terminal_frame_string_parameter(args[0], "tty")?;
    let tty_type = terminal_frame_string_parameter(args[0], "tty-type")?;

    if let Some(device) = tty_device {
        let terminal_type = tty_type
            .ok_or_else(|| signal("error", vec![Value::string("Invalid terminal type")]))?;
        if let Some(terminal_id) = super::terminal::pure::active_tty_terminal_id_by_name(&device) {
            return make_terminal_frame_on_existing_terminal(eval, args, terminal_id);
        }
        let terminal_id = super::terminal::pure::next_terminal_id();
        let frame = make_frame_plain_on_terminal(
            &mut eval.frames,
            &mut eval.buffers,
            args,
            terminal_id,
            80,
            25,
        )?;
        let frame_id = FrameId(frame.as_frame_id().expect("make-frame returned a frame"));
        if eval
            .frames
            .get(frame_id)
            .is_none_or(|frame| frame.terminal_id != terminal_id)
        {
            eval.frames.delete_frame(frame_id);
            return Err(signal(
                "error",
                vec![Value::string(
                    "A terminal child frame cannot use a different terminal than its parent",
                )],
            ));
        }
        let request = match super::terminal::pure::TtyFrameOpenRequest::new(
            terminal_id,
            frame_id,
            device,
            terminal_type,
        ) {
            Ok(request) => request,
            Err(message) => {
                eval.frames.delete_frame(frame_id);
                return Err(signal("error", vec![Value::string(message)]));
            }
        };
        let opened = match eval.tty_frame_host_factory.as_mut() {
            Some(factory) => factory.open_tty(request.clone()),
            None => Err("TTY frame host unavailable".to_string()),
        };
        let opened = match opened {
            Ok(opened) => opened,
            Err(message) => {
                eval.frames.delete_frame(frame_id);
                return Err(signal("error", vec![Value::string(message)]));
            }
        };
        let size = super::terminal::pure::install_opened_tty(&request, opened);
        let displays_chrome = !eval.noninteractive();
        let (frames, buffers) = (&mut eval.frames, &eval.buffers);
        if let Some(frame) = frames.get_mut(frame_id) {
            apply_terminal_viewport_to_tty_frame(frame, buffers, size, displays_chrome);
        }
        return Ok(frame);
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
    let terminal_id = eval
        .frames
        .selected_frame()
        .map(|frame| frame.terminal_id)
        .unwrap_or(0);
    make_terminal_frame_on_existing_terminal(eval, args, terminal_id)
}

fn make_terminal_frame_on_existing_terminal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
    terminal_id: u64,
) -> EvalResult {
    let (width, height) = eval
        .frames
        .selected_frame()
        .filter(|frame| frame.terminal_id == terminal_id)
        .map(|frame| (frame.width, frame.height))
        .or_else(|| {
            eval.frames
                .frame_list()
                .into_iter()
                .filter_map(|frame_id| eval.frames.get(frame_id))
                .find(|frame| {
                    frame.terminal_id == terminal_id && frame.parent_frame.as_frame_id().is_none()
                })
                .map(|frame| (frame.width, frame.height))
        })
        .unwrap_or((80, 25));
    let frame = make_frame_plain_on_terminal(
        &mut eval.frames,
        &mut eval.buffers,
        args,
        terminal_id,
        width,
        height,
    )?;
    let frame_id = FrameId(frame.as_frame_id().expect("make-frame returned a frame"));
    if eval
        .frames
        .get(frame_id)
        .is_none_or(|frame| frame.terminal_id != terminal_id)
    {
        eval.frames.delete_frame(frame_id);
        return Err(signal(
            "error",
            vec![Value::string(
                "A terminal child frame cannot use a different terminal than its parent",
            )],
        ));
    }
    let displays_chrome = !eval.noninteractive();
    let (frames, buffers) = (&mut eval.frames, &eval.buffers);
    if let Some(frame) = frames.get_mut(frame_id) {
        let size = super::terminal::pure::TtyFrameSize::new(width, height)
            .expect("live terminal frames have nonzero dimensions");
        apply_terminal_viewport_to_tty_frame(frame, buffers, size, displays_chrome);
    }
    Ok(frame)
}

/// Apply the physical terminal viewport only to its top-level frame.
///
/// GNU's `change_frame_size` updates the terminal's top frame to the device
/// dimensions.  TTY child frames remain logical subregions with their own
/// explicitly requested geometry; expanding them to the whole device would
/// erase the `left`, `top`, `width`, and `height` contract.
fn apply_terminal_viewport_to_tty_frame(
    frame: &mut crate::window::Frame,
    buffers: &BufferManager,
    size: super::terminal::pure::TtyFrameSize,
    displays_chrome: bool,
) {
    frame.displays_chrome = displays_chrome;
    if frame_is_top_level_non_window(frame) {
        frame.resize_pixelwise_with_buffer_constraints(buffers, size.columns(), size.rows());
    }
}

fn terminal_frame_string_parameter(params: Value, name: &str) -> Result<Option<String>, Flow> {
    let Some(items) = super::value::list_to_vec(&params) else {
        return Ok(None);
    };
    for item in items {
        if !item.is_cons() || item.cons_car() != Value::symbol(name) {
            continue;
        }
        let value = item.cons_cdr();
        let Some(string) = value.as_lisp_string() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), value],
            ));
        };
        return Ok(Some(crate::emacs_core::emacs_char::to_utf8_lossy(
            string.as_bytes(),
        )));
    }
    Ok(None)
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
            LispCondition::WrongTypeArgument,
            vec![
                Value::symbol("frame-live-p"),
                args.first().copied().unwrap_or(Value::NIL),
            ],
        )
    })?;
    frame.window_state_change = state;
    Ok(Value::bool_val(state))
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
    if frame.effective_window_system().is_some() {
        pairs.push(Value::cons(
            Value::symbol("explicit-name"),
            frame.explicit_name_value(),
        ));
    }
    let width = if frame_is_top_level_non_window(frame) {
        Value::fixnum(frame.columns() as i64)
    } else {
        frame
            .parameter("width")
            .unwrap_or(Value::fixnum(frame.columns() as i64))
    };
    let height = if frame_is_top_level_non_window(frame) {
        Value::fixnum(frame_realized_lines(frame))
    } else {
        frame
            .parameter("height")
            .unwrap_or(Value::fixnum(frame.lines() as i64))
    };
    pairs.push(Value::cons(Value::symbol("width"), width));
    pairs.push(Value::cons(Value::symbol("height"), height));
    pairs.push(Value::cons(
        FrameParam::Visibility.symbol(),
        Value::bool_val(frame.visible),
    ));
    if frame.effective_window_system().is_none() {
        pairs.push(Value::cons(FrameParam::Font.symbol(), Value::string("tty")));
    }
    // GNU `Fframe_parameters` (frame.c:4150) stores `modeline' =
    // FRAME_WANTS_MODELINE_P (f).  A normal (non-tooltip, non-minibuffer-only)
    // frame wants a mode line, so this is t.
    pairs.push(Value::cons(Value::symbol("modeline"), Value::T));
    // GNU stores `no-accept-focus' = FRAME_NO_ACCEPT_FOCUS (f) in the tty
    // branch (frame.c:4165).  It defaults to nil.
    if frame.effective_window_system().is_none() {
        pairs.push(Value::cons(
            Value::symbol("no-accept-focus"),
            Value::bool_val(frame.no_accept_focus),
        ));
    }
    // GNU frame.c:4152-4153 — buffer-list and buried-buffer-list are
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
        if k.as_symbol_name()
            .is_some_and(|name| name == "width" || name == "height" || name == "visibility")
        {
            continue;
        }
        // `font-parameter' is a neomacs-internal frame slot used by the font
        // resolution machinery (font.rs); GNU has no such frame parameter and
        // never reports it from `frame-parameters'.  Skip it.
        if k.as_symbol_name() == Some("font-parameter") {
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
    Ok(Value::fixnum(
        frame.effective_divider_width(FrameDivider::Bottom),
    ))
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
    Ok(Value::fixnum(
        frame.effective_divider_width(FrameDivider::Right),
    ))
}

pub(crate) fn builtin_frame_scale_factor(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-scale-factor", &args, 1)?;
    let fid =
        resolve_frame_id_in_state(&mut eval.frames, &mut eval.buffers, args.first(), "framep")?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let scale = if frame.device_scale_factor.is_finite() && frame.device_scale_factor > 0.0 {
        frame.device_scale_factor
    } else {
        1.0
    };
    Ok(Value::make_float(scale))
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
                        if parse_frame_size_param(pair_cdr).is_some()
                            || pair_cdr.as_float().is_some()
                        {
                            requested_width = Some(pair_cdr);
                        }
                        if let Some(size) = parse_frame_size_param(pair_cdr) {
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
                        if parse_frame_size_param(pair_cdr).is_some()
                            || pair_cdr.as_float().is_some()
                        {
                            requested_height = Some(pair_cdr);
                        }
                        if let Some(size) = parse_frame_size_param(pair_cdr) {
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
                        if pair_cdr.as_int().is_some() || pair_cdr.as_float().is_some() {
                            requested_left = Some(pair_cdr);
                        }
                    }
                    "top" => {
                        if pair_cdr.as_int().is_some() || pair_cdr.as_float().is_some() {
                            requested_top = Some(pair_cdr);
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
                                let is_tty = eval
                                    .frames
                                    .get(fid)
                                    .is_some_and(|frame| frame.effective_window_system().is_none());
                                if is_tty {
                                    let _ = eval.frames.set_tty_frame_name_parameter(fid, name);
                                } else if let Some(frame) = eval.frames.get_mut(fid) {
                                    if name.is_nil() {
                                        let current = frame.name_value();
                                        frame.set_generated_name_value(current);
                                    } else {
                                        frame.set_name_value(name);
                                    }
                                }
                            }
                        }
                        FrameParamKey::Known(FrameParam::Title) => {
                            if let Some(title) = frame_title_parameter_value(&pair_cdr)
                                && let Some(frame) = eval.frames.get_mut(fid)
                            {
                                frame.title = title;
                            }
                        }
                        FrameParamKey::Known(FrameParam::IconName) => {
                            if let Some(icon_name) = frame_icon_name_parameter_value(&pair_cdr)
                                && let Some(frame) = eval.frames.get_mut(fid)
                            {
                                frame.icon_name = icon_name;
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
                            if eval.frames.get(fid).is_some_and(|frame| {
                                frame.effective_window_system().is_some()
                                    || frame.parent_frame.as_frame_id().is_some()
                            }) {
                                // Route through set_frame_visibility so the
                                // display runtime is notified — mirrors
                                // GNU's gui_set_visibility → Fmake_frame_invisible.
                                set_frame_visibility(eval, fid, pair_cdr.is_truthy())?;
                            } else if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.set_known_parameter(FrameParam::Visibility, pair_cdr);
                            }
                        }
                        FrameParamKey::Known(FrameParam::Undecorated) => {
                            // Store first, apply second: GNU records a frame
                            // parameter whether or not the backend honours it
                            // (`frame_parms[]`, src/frame.c), so `frame-parameter'
                            // reads back what Lisp set even where nothing can act
                            // on it. Storing WITHOUT applying was the whole bug --
                            // `undecorated' round-tripped through Lisp while the
                            // window manager never heard about it (neomacs#197).
                            let undecorated = pair_cdr.is_truthy();
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.undecorated = undecorated;
                                frame.set_known_parameter(FrameParam::Undecorated, pair_cdr);
                            }
                            let is_top_level_gui = eval.frames.get(fid).is_some_and(|frame| {
                                frame.effective_window_system().is_some()
                                    && frame.parent_frame.as_frame_id().is_none()
                            });
                            if is_top_level_gui && let Some(host) = eval.display_host.as_mut() {
                                host.set_gui_frame_undecorated(fid, undecorated).map_err(
                                    |message| signal("error", vec![Value::string(message)]),
                                )?;
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
                                if frame.effective_window_system().is_none()
                                    && frame.parent_frame.is_nil()
                                {
                                    frame.no_split = false;
                                    frame.set_known_parameter(FrameParam::Unsplittable, Value::NIL);
                                } else {
                                    frame.no_split = pair_cdr.is_truthy();
                                    frame.set_known_parameter(FrameParam::Unsplittable, pair_cdr);
                                }
                            }
                        }
                        FrameParamKey::Known(FrameParam::Fullscreen) => {
                            let fullscreen = FrameFullscreen::from_symbol_value(&pair_cdr);
                            if let Some(frame) = eval.frames.get_mut(fid) {
                                frame.set_known_parameter(FrameParam::Fullscreen, pair_cdr);
                            }
                            if let Some(fullscreen) = fullscreen {
                                let is_top_level_gui = eval.frames.get(fid).is_some_and(|frame| {
                                    frame.effective_window_system().is_some()
                                        && frame.parent_frame.as_frame_id().is_none()
                                });
                                if is_top_level_gui && let Some(host) = eval.display_host.as_mut() {
                                    host.set_gui_frame_fullscreen(fid, fullscreen).map_err(
                                        |message| signal("error", vec![Value::string(message)]),
                                    )?;
                                }
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

    let requested_width = resolve_frame_size_parameter(&eval.frames, fid, requested_width, true);
    let requested_height = resolve_frame_size_parameter(&eval.frames, fid, requested_height, false);

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

    let (left, top) =
        resolve_frame_position_parameters(&eval.frames, fid, requested_left, requested_top);
    if let Some(frame) = eval.frames.get_mut(fid) {
        if let Some(left) = left {
            frame.left_pos = left;
            frame.set_parameter(Value::symbol("left"), Value::fixnum(left));
        }
        if let Some(top) = top {
            frame.top_pos = top;
            frame.set_parameter(Value::symbol("top"), Value::fixnum(top));
        }
    }

    Ok(Value::NIL)
}

fn resolve_frame_size_parameter(
    frames: &FrameManager,
    fid: FrameId,
    value: Option<Value>,
    horizontal: bool,
) -> Option<FrameSizeParam> {
    let value = value?;
    if let Some(size) = parse_frame_size_param(value) {
        return Some(size);
    }
    let fraction = value
        .as_float()
        .filter(|value| (0.0..=1.0).contains(value))?;
    let frame = frames.get(fid)?;
    let parent = frames.get(frame.parent_frame.as_frame_id().map(FrameId)?)?;
    let parent_pixels = if horizontal {
        parent.width
    } else {
        parent.height
    };
    let non_text_pixels = if horizontal {
        frame_non_text_total_width_pixels_in_state(frames, fid)
    } else {
        frame_non_text_total_height_pixels(frame)
    };
    let outer_pixels = (f64::from(parent_pixels) * fraction) as u32;
    Some(FrameSizeParam::TextPixels(
        outer_pixels.saturating_sub(non_text_pixels).max(1),
    ))
}

fn resolve_frame_position_parameters(
    frames: &FrameManager,
    fid: FrameId,
    left: Option<Value>,
    top: Option<Value>,
) -> (Option<i64>, Option<i64>) {
    let Some(frame) = frames.get(fid) else {
        return (None, None);
    };
    let parent = frame
        .parent_frame
        .as_frame_id()
        .map(FrameId)
        .and_then(|parent_id| frames.get(parent_id));
    let resolve = |value: Option<Value>, parent_size: u32, frame_size: u32| {
        value.and_then(|value| {
            value.as_int().or_else(|| {
                let fraction = value
                    .as_float()
                    .filter(|value| (0.0..=1.0).contains(value))?;
                Some((fraction * f64::from(parent_size.saturating_sub(frame_size))) as i64)
            })
        })
    };
    (
        resolve(
            left,
            parent.map_or(frame.width, |parent| parent.width),
            frame.width,
        ),
        resolve(
            top,
            parent.map_or(frame.height, |parent| parent.height),
            frame.height,
        ),
    )
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("frame-live-p"), *val],
            ));
        }
    };
    let frame = frames.get(fid).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
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

pub(crate) fn live_frame_buffer_parameter_ids(
    buffers: &BufferManager,
    mut value: Value,
) -> Vec<BufferId> {
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

pub(crate) fn sync_live_gui_resize_for_geometry_queries(
    eval: &mut super::eval::Context,
    fid: FrameId,
) -> Result<(), Flow> {
    eval.sync_pending_resize_events();
    if flush_pending_live_gui_resize(eval, fid)? {
        eval.wait_for_pending_resize_events(LIVE_GUI_RESIZE_ACK_TIMEOUT);
    }
    Ok(())
}

pub(crate) fn frame_text_cols(frame: &crate::window::Frame) -> i64 {
    frame_total_cols(frame)
}

pub(crate) fn frame_uses_window_system_pixels(frame: &crate::window::Frame) -> bool {
    frame.effective_window_system().is_some()
}

pub(crate) fn check_frame_pixels(
    value: &Value,
    pixelwise: bool,
    item_size: f32,
) -> Result<u32, Flow> {
    let size = expect_int(value)?;
    if size <= 0 {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
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
            LispCondition::ArgsOutOfRange,
            vec![*value, Value::fixnum(1), Value::fixnum(i64::from(i32::MAX))],
        )
    })?;
    if pixels <= 0 || pixels > u32::MAX as i64 {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![*value, Value::fixnum(1), Value::fixnum(i64::from(i32::MAX))],
        ));
    }
    Ok(pixels as u32)
}

#[allow(clippy::too_many_arguments)] // keeps text/pixel geometry inputs explicit at the resize boundary
pub(crate) fn frame_resize_request_from_params(
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

pub(crate) fn frame_text_lines(frame: &crate::window::Frame) -> i64 {
    frame
        .parameter(FRAME_TEXT_LINES_PARAM)
        .and_then(|v| v.as_int())
        .unwrap_or_else(|| frame_total_lines(frame))
}

pub(crate) fn set_top_level_non_window_pixelwise_totals(
    frame: &mut crate::window::Frame,
    text_width: u32,
    text_height: u32,
) {
    let text_width = i64::from(text_width).max(1);
    let text_height = i64::from(text_height).max(1);
    // GNU frame.c line accounting: FRAME_TOTAL_LINES = FRAME_LINES +
    // FRAME_MENU_BAR_LINES + FRAME_TAB_BAR_LINES, where FRAME_LINES is the text
    // area including the minibuffer, excluding the menu/tab bar rows.
    // `text_height` here is the text-area line count (menu/tab already excluded).
    //
    // This helper only owns the NATIVE geometry parameters (total cols/lines,
    // text lines). It must NOT write the logical `height`/`width` frame
    // parameters: those are FRAME_LINES/FRAME_COLS and are set by the preceding
    // `builtin_set_frame_size` call in the pixelwise builtin. Writing `height`
    // here clobbered the logical char height for top-level TTY frames (regressed
    // set_frame_size_and_position_pixelwise_updates_top_level_tty_native_totals:
    // height became text_height+minibuffer instead of the logical FRAME_LINES).
    // The terminal-resize FRAME_LINES fix lives in `Frame::resize_pixelwise`
    // (window/mod.rs), a separate path, and is unaffected.
    let minibuffer_lines = i64::from(frame.minibuffer_leaf.is_some());
    let frame_lines = text_height
        .saturating_add(minibuffer_lines)
        .min(u32::MAX as i64);
    // Only realized (displayed) chrome adds rows over FRAME_LINES; a
    // non-displayed frame (--batch) keeps FRAME_TOTAL_LINES == FRAME_LINES,
    // matching GNU's batch geometry that the oracle pins.
    let top_margin = if frame.displays_chrome {
        frame.frame_top_margin()
    } else {
        0
    };
    let total_lines = frame_lines.saturating_add(top_margin).min(u32::MAX as i64);

    frame.set_parameter(
        Value::symbol(FRAME_TOTAL_COLS_PARAM),
        Value::fixnum(text_width),
    );
    frame.set_parameter(
        Value::symbol(FRAME_TOTAL_LINES_PARAM),
        Value::fixnum(total_lines),
    );
    frame.set_parameter(
        Value::symbol(FRAME_TEXT_LINES_PARAM),
        Value::fixnum(text_height),
    );
}

pub(crate) fn request_live_gui_frame_resize_and_keep_pending(
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
        fullscreen: None,
    })
    .map_err(|message| signal("error", vec![Value::string(message)]))?;

    let (desired_cols, desired_total_lines) = request.logical_size(frames, fid)?;
    let frame = frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    frame.queue_pending_gui_resize(desired_cols, desired_total_lines, true);
    Ok(())
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
pub(crate) fn set_frame_visibility(
    eval: &mut super::eval::Context,
    fid: FrameId,
    visible: bool,
) -> Result<(), Flow> {
    let was_visible = eval.frames.get(fid).is_some_and(|f| f.visible);
    let is_gui_child_frame = eval.frames.get(fid).is_some_and(|frame| {
        frame.effective_window_system().is_some() && frame.parent_frame.as_frame_id().is_some()
    });
    if is_gui_child_frame {
        tracing::info!(
            frame_id = fid.0,
            was_visible,
            visible,
            "child_frame_lifecycle: core_visibility_change"
        );
    }
    if visible {
        if let Some(frame) = eval.frames.get_mut(fid) {
            frame.visible = true;
            if frame.parent_frame.as_frame_id().is_some() {
                eval.frames.raise_or_lower_child_frame(fid, true);
            }
        }
        if !was_visible
            && is_gui_child_frame
            && let Some(host) = eval.display_host.as_mut()
        {
            host.show_gui_child_frame(fid)
                .map_err(|message| signal("error", vec![Value::string(message)]))?;
        }
    } else if was_visible {
        if let Some(frame) = eval.frames.get_mut(fid) {
            frame.visible = false;
        }
        // Notify display runtime: GUI child frames need RemoveChildFrame.
        if is_gui_child_frame && let Some(host) = eval.display_host.as_mut() {
            host.remove_gui_child_frame(fid)
                .map_err(|message| signal("error", vec![Value::string(message)]))?;
        }
    }
    Ok(())
}

pub(crate) fn mru_rooted_frame_in_state(frames: &FrameManager, hidden: FrameId) -> FrameId {
    let root = frames.root_frame_id(hidden).unwrap_or(hidden);
    let mut best: Option<(i64, FrameId)> = None;

    for candidate in
        frames.frames_in_reverse_z_order(root, crate::window::RenderFrameVisibility::VisibleOnly)
    {
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

pub(crate) fn frame_title_parameter_value(value: &Value) -> Option<Value> {
    if value.is_nil() {
        Some(Value::NIL)
    } else {
        stringish_value(value)
    }
}

pub(crate) fn frame_icon_name_parameter_value(value: &Value) -> Option<Value> {
    if value.is_nil() {
        Some(Value::NIL)
    } else {
        stringish_value(value)
    }
}

/// `(frame-parameter FRAME PARAMETER)` -> value or nil.
pub(crate) fn frame_parameter_value(
    frame: &crate::window::Frame,
    param_key: FrameParamKey,
) -> Value {
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
        FrameParamKey::Known(FrameParam::LineSpacing) => {
            return frame
                .known_parameter(FrameParam::LineSpacing)
                .unwrap_or(Value::fixnum(0));
        }
        FrameParamKey::Known(FrameParam::Unsplittable)
            if frame.effective_window_system().is_none() && frame.parent_frame.is_nil() =>
        {
            return Value::NIL;
        }
        _ => {}
    }

    match param_key.name() {
        "explicit-name" if frame.effective_window_system().is_some() => frame.explicit_name_value(),
        "explicit-name" => Value::NIL,
        // GNU `Fframe_parameters` reports width/height from live frame geometry
        // for top-level non-window frames, even after their parameter alist has
        // stored requested size values.
        "width" if frame_is_top_level_non_window(frame) => Value::fixnum(frame.columns() as i64),
        "height" if frame_is_top_level_non_window(frame) => {
            Value::fixnum(frame_realized_lines(frame))
        }
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
