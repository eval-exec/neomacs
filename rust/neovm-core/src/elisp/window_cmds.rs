//! Window, frame, and display-related builtins for the Elisp VM.
//!
//! Bridges the `FrameManager` (in `crate::window`) to Elisp by exposing
//! builtins such as `selected-window`, `split-window`, `selected-frame`, etc.
//! Windows and frames are represented as integer IDs in Lisp.

use super::error::{signal, EvalResult, Flow};
use super::value::Value;
use crate::buffer::BufferId;
use crate::window::{FrameId, FrameManager, SplitDirection, Window, WindowId};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Expect exactly N arguments.
fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
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
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Extract an integer from a Value.
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

/// Resolve an optional window argument: if nil or absent, use the selected
/// window of the selected frame; otherwise interpret as integer window id.
fn resolve_window_id(
    frames: &FrameManager,
    arg: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    let frame = frames
        .selected_frame()
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    let frame_id = frame.id;

    match arg {
        None | Some(Value::Nil) => Ok((frame_id, frame.selected_window)),
        Some(val) => {
            let id = expect_int(val)? as u64;
            // Verify window exists in the selected frame.
            if frame.find_window(WindowId(id)).is_some() {
                Ok((frame_id, WindowId(id)))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string(format!("No window with id {id}"))],
                ))
            }
        }
    }
}

/// Resolve an optional frame argument: if nil or absent, use the selected frame.
fn resolve_frame_id(frames: &FrameManager, arg: Option<&Value>) -> Result<FrameId, Flow> {
    match arg {
        None | Some(Value::Nil) => frames
            .selected_frame()
            .map(|f| f.id)
            .ok_or_else(|| signal("error", vec![Value::string("No selected frame")])),
        Some(val) => {
            let id = expect_int(val)? as u64;
            if frames.get(FrameId(id)).is_some() {
                Ok(FrameId(id))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string(format!("No frame with id {id}"))],
                ))
            }
        }
    }
}

/// Helper: get a reference to a leaf window by id.
fn get_leaf<'a>(frames: &'a FrameManager, fid: FrameId, wid: WindowId) -> Result<&'a Window, Flow> {
    let frame = frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    frame
        .find_window(wid)
        .ok_or_else(|| signal("error", vec![Value::string("Window not found")]))
}

/// Ensure a selected frame exists and return its id.
///
/// In batch compatibility mode, GNU Emacs still has an initial frame (`F1`).
/// When the evaluator has no frame yet, synthesize one on demand.
pub(crate) fn ensure_selected_frame_id(eval: &mut super::eval::Evaluator) -> FrameId {
    if let Some(fid) = eval.frames.selected_frame().map(|f| f.id) {
        return fid;
    }

    let buf_id = eval
        .buffers
        .current_buffer()
        .map(|b| b.id)
        .unwrap_or_else(|| eval.buffers.create_buffer("*scratch*"));
    eval.frames.create_frame("F1", 80, 25, buf_id)
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

// ===========================================================================
// Window queries
// ===========================================================================

/// `(selected-window)` -> window id (int).
pub(crate) fn builtin_selected_window(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("selected-window", &args, 0)?;
    let frame = eval
        .frames
        .selected_frame()
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    Ok(Value::Int(frame.selected_window.0 as i64))
}

/// `(window-buffer &optional WINDOW)` -> buffer object.
pub(crate) fn builtin_window_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    match w.buffer_id() {
        Some(bid) => Ok(Value::Buffer(bid)),
        None => Ok(Value::Nil),
    }
}

/// `(window-start &optional WINDOW)` -> integer position.
pub(crate) fn builtin_window_start(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    match w {
        Window::Leaf { window_start, .. } => Ok(Value::Int(*window_start as i64)),
        _ => Ok(Value::Int(0)),
    }
}

/// `(window-end &optional WINDOW UPDATE)` -> integer position.
///
/// We approximate window-end as window-start since we don't have real
/// display layout.  The UPDATE argument is ignored.
pub(crate) fn builtin_window_end(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    match w {
        Window::Leaf {
            window_start,
            bounds,
            ..
        } => {
            // Rough estimate: window_start + lines * columns.
            let frame = eval.frames.get(fid).unwrap();
            let lines = (bounds.height / frame.char_height) as usize;
            let cols = (bounds.width / frame.char_width) as usize;
            Ok(Value::Int((*window_start + lines * cols) as i64))
        }
        _ => Ok(Value::Int(0)),
    }
}

/// `(window-point &optional WINDOW)` -> integer position.
pub(crate) fn builtin_window_point(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    match w {
        Window::Leaf { point, .. } => Ok(Value::Int(*point as i64)),
        _ => Ok(Value::Int(0)),
    }
}

/// `(set-window-start WINDOW POS &optional NOFORCE)` -> POS.
pub(crate) fn builtin_set_window_start(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-window-start", &args, 2)?;
    let wid_val = expect_int(&args[0])? as u64;
    let pos = expect_int(&args[1])? as usize;

    let frame = eval
        .frames
        .selected_frame_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    let fid = frame.id;
    if let Some(w) = eval
        .frames
        .get_mut(fid)
        .and_then(|f| f.find_window_mut(WindowId(wid_val)))
    {
        if let Window::Leaf { window_start, .. } = w {
            *window_start = pos;
        }
    }
    Ok(Value::Int(pos as i64))
}

/// `(set-window-point WINDOW POS)` -> POS.
pub(crate) fn builtin_set_window_point(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-window-point", &args, 2)?;
    let wid_val = expect_int(&args[0])? as u64;
    let pos = expect_int(&args[1])? as usize;

    let fid = eval
        .frames
        .selected_frame()
        .map(|f| f.id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    if let Some(w) = eval
        .frames
        .get_mut(fid)
        .and_then(|f| f.find_window_mut(WindowId(wid_val)))
    {
        if let Window::Leaf { point, .. } = w {
            *point = pos;
        }
    }
    Ok(Value::Int(pos as i64))
}

/// `(window-height &optional WINDOW)` -> integer (lines).
pub(crate) fn builtin_window_height(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    let ch = eval.frames.get(fid).map(|f| f.char_height).unwrap_or(16.0);
    Ok(Value::Int(window_height_lines(w, ch)))
}

/// `(window-width &optional WINDOW)` -> integer (columns).
pub(crate) fn builtin_window_width(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    let cw = eval.frames.get(fid).map(|f| f.char_width).unwrap_or(8.0);
    Ok(Value::Int(window_width_cols(w, cw)))
}

/// `(window-body-height &optional WINDOW PIXELWISE)` -> integer.
pub(crate) fn builtin_window_body_height(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    let pixelwise = args.get(1).map_or(false, |v| v.is_truthy());
    if pixelwise {
        Ok(Value::Int(w.bounds().height as i64))
    } else {
        let ch = eval.frames.get(fid).map(|f| f.char_height).unwrap_or(16.0);
        Ok(Value::Int(window_height_lines(w, ch)))
    }
}

/// `(window-body-width &optional WINDOW PIXELWISE)` -> integer.
pub(crate) fn builtin_window_body_width(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    let pixelwise = args.get(1).map_or(false, |v| v.is_truthy());
    if pixelwise {
        Ok(Value::Int(w.bounds().width as i64))
    } else {
        let cw = eval.frames.get(fid).map(|f| f.char_width).unwrap_or(8.0);
        Ok(Value::Int(window_width_cols(w, cw)))
    }
}

/// `(window-list &optional FRAME MINIBUF ALL-FRAMES)` -> list of window ids.
pub(crate) fn builtin_window_list(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let fid = resolve_frame_id(&eval.frames, args.first())?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let ids: Vec<Value> = frame
        .window_list()
        .into_iter()
        .map(|wid| Value::Int(wid.0 as i64))
        .collect();
    Ok(Value::list(ids))
}

/// `(window-dedicated-p &optional WINDOW)` -> t or nil.
pub(crate) fn builtin_window_dedicated_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let w = get_leaf(&eval.frames, fid, wid)?;
    match w {
        Window::Leaf { dedicated, .. } => Ok(Value::bool(*dedicated)),
        _ => Ok(Value::Nil),
    }
}

/// `(set-window-dedicated-p WINDOW FLAG)` -> FLAG.
pub(crate) fn builtin_set_window_dedicated_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-window-dedicated-p", &args, 2)?;
    let wid_val = expect_int(&args[0])? as u64;
    let flag = args[1].is_truthy();

    let fid = eval
        .frames
        .selected_frame()
        .map(|f| f.id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    if let Some(w) = eval
        .frames
        .get_mut(fid)
        .and_then(|f| f.find_window_mut(WindowId(wid_val)))
    {
        if let Window::Leaf { dedicated, .. } = w {
            *dedicated = flag;
        }
    }
    Ok(Value::bool(flag))
}

/// `(windowp OBJ)` -> t if OBJ is a window id (integer) that exists.
pub(crate) fn builtin_windowp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("windowp", &args, 1)?;
    let id = match args[0].as_int() {
        Some(n) => n as u64,
        None => return Ok(Value::Nil),
    };
    let found = eval
        .frames
        .selected_frame()
        .and_then(|f| f.find_window(WindowId(id)))
        .is_some();
    Ok(Value::bool(found))
}

/// `(window-live-p OBJ)` -> t if OBJ is a live leaf window id.
pub(crate) fn builtin_window_live_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("window-live-p", &args, 1)?;
    let id = match args[0].as_int() {
        Some(n) => n as u64,
        None => return Ok(Value::Nil),
    };
    let live = eval
        .frames
        .selected_frame()
        .and_then(|f| f.find_window(WindowId(id)))
        .map_or(false, |w| w.is_leaf());
    Ok(Value::bool(live))
}

// ===========================================================================
// Window manipulation
// ===========================================================================

/// `(split-window &optional WINDOW SIZE SIDE)` -> new window id.
///
/// SIDE: nil or `below` = vertical split, `right` = horizontal split.
pub(crate) fn builtin_split_window(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;

    // Determine split direction from SIDE argument.
    let direction = match args.get(2) {
        Some(Value::Symbol(s)) if s == "right" || s == "left" => SplitDirection::Horizontal,
        _ => SplitDirection::Vertical,
    };

    // Use the same buffer as the window being split.
    let buf_id = {
        let w = get_leaf(&eval.frames, fid, wid)?;
        w.buffer_id().unwrap_or(BufferId(0))
    };

    let new_wid = eval
        .frames
        .split_window(fid, wid, direction, buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Cannot split window")]))?;
    Ok(Value::Int(new_wid.0 as i64))
}

/// `(delete-window &optional WINDOW)` -> nil.
pub(crate) fn builtin_delete_window(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    if !eval.frames.delete_window(fid, wid) {
        return Err(signal(
            "error",
            vec![Value::string("Cannot delete sole window")],
        ));
    }
    Ok(Value::Nil)
}

/// `(delete-other-windows &optional WINDOW)` -> nil.
///
/// Deletes all windows in the frame except WINDOW (or selected window).
pub(crate) fn builtin_delete_other_windows(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, keep_wid) = resolve_window_id(&eval.frames, args.first())?;
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
    if let Some(f) = eval.frames.get_mut(fid) {
        f.select_window(keep_wid);
    }
    Ok(Value::Nil)
}

/// `(select-window WINDOW &optional NORECORD)` -> WINDOW.
pub(crate) fn builtin_select_window(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("select-window", &args, 1)?;
    let wid_val = expect_int(&args[0])? as u64;
    let fid = eval
        .frames
        .selected_frame()
        .map(|f| f.id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    let ok = eval
        .frames
        .get_mut(fid)
        .map(|f| f.select_window(WindowId(wid_val)))
        .unwrap_or(false);
    if !ok {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Window {} not found in selected frame",
                wid_val
            ))],
        ));
    }
    Ok(Value::Int(wid_val as i64))
}

/// `(other-window COUNT &optional ALL-FRAMES)` -> nil.
///
/// Select another window in cyclic order.
pub(crate) fn builtin_other_window(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let Some(fid) = eval.frames.selected_frame().map(|f| f.id) else {
        return Ok(Value::Nil);
    };
    let Some(frame) = eval.frames.get(fid) else {
        return Ok(Value::Nil);
    };
    let list = frame.window_list();
    if list.is_empty() {
        return Ok(Value::Nil);
    }
    let cur = frame.selected_window;
    let cur_idx = list.iter().position(|w| *w == cur).unwrap_or(0);
    let len = list.len() as i64;
    let new_idx = ((cur_idx as i64 + count) % len + len) % len;
    let new_wid = list[new_idx as usize];
    if let Some(f) = eval.frames.get_mut(fid) {
        f.select_window(new_wid);
    }
    Ok(Value::Nil)
}

/// `(next-window &optional WINDOW MINIBUF ALL-FRAMES)` -> window id.
pub(crate) fn builtin_next_window(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let list = frame.window_list();
    if list.is_empty() {
        return Ok(Value::Nil);
    }
    let idx = list.iter().position(|w| *w == wid).unwrap_or(0);
    let next = (idx + 1) % list.len();
    Ok(Value::Int(list[next].0 as i64))
}

/// `(previous-window &optional WINDOW MINIBUF ALL-FRAMES)` -> window id.
pub(crate) fn builtin_previous_window(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let (fid, wid) = resolve_window_id(&eval.frames, args.first())?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let list = frame.window_list();
    if list.is_empty() {
        return Ok(Value::Nil);
    }
    let idx = list.iter().position(|w| *w == wid).unwrap_or(0);
    let prev = if idx == 0 { list.len() - 1 } else { idx - 1 };
    Ok(Value::Int(list[prev].0 as i64))
}

/// `(set-window-buffer WINDOW BUFFER-OR-NAME &optional KEEP-MARGINS)` -> nil.
pub(crate) fn builtin_set_window_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-window-buffer", &args, 2)?;
    let wid_val = expect_int(&args[0])? as u64;
    let buf_id = resolve_buffer_id(eval, &args[1])?;

    let fid = eval
        .frames
        .selected_frame()
        .map(|f| f.id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    if let Some(w) = eval
        .frames
        .get_mut(fid)
        .and_then(|f| f.find_window_mut(WindowId(wid_val)))
    {
        w.set_buffer(buf_id);
    }
    Ok(Value::Nil)
}

/// `(switch-to-buffer BUFFER-OR-NAME &optional NORECORD FORCE-SAME-WINDOW)` -> buffer.
pub(crate) fn builtin_switch_to_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("switch-to-buffer", &args, 1)?;
    let buf_id = resolve_buffer_id(eval, &args[0])?;

    // Set the selected window's buffer.
    let fid = eval
        .frames
        .selected_frame()
        .map(|f| f.id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
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
    // Also switch the buffer manager's current buffer.
    eval.buffers.set_current(buf_id);
    Ok(Value::Buffer(buf_id))
}

/// `(display-buffer BUFFER-OR-NAME &optional ACTION FRAME)` -> window id or nil.
///
/// Simplified: displays the buffer in the selected window.
pub(crate) fn builtin_display_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("display-buffer", &args, 1)?;
    let buf_id = resolve_buffer_id(eval, &args[0])?;

    let fid = eval
        .frames
        .selected_frame()
        .map(|f| f.id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
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
    Ok(Value::Int(sel_wid.0 as i64))
}

/// `(pop-to-buffer BUFFER-OR-NAME &optional ACTION NORECORD)` -> window id.
///
/// Simplified: same as display-buffer + select that window.
pub(crate) fn builtin_pop_to_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("pop-to-buffer", &args, 1)?;
    let buf_id = resolve_buffer_id(eval, &args[0])?;

    let fid = eval
        .frames
        .selected_frame()
        .map(|f| f.id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
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
    eval.buffers.set_current(buf_id);
    Ok(Value::Int(sel_wid.0 as i64))
}

// ===========================================================================
// Frame operations
// ===========================================================================

/// `(selected-frame)` -> frame id (int).
pub(crate) fn builtin_selected_frame(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("selected-frame", &args, 0)?;
    let fid = ensure_selected_frame_id(eval);
    Ok(Value::Int(fid.0 as i64))
}

/// `(frame-list)` -> list of frame ids.
pub(crate) fn builtin_frame_list(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("frame-list", &args, 0)?;
    let _ = ensure_selected_frame_id(eval);
    let ids: Vec<Value> = eval
        .frames
        .frame_list()
        .into_iter()
        .map(|fid| Value::Int(fid.0 as i64))
        .collect();
    Ok(Value::list(ids))
}

/// `(make-frame &optional PARAMETERS)` -> frame id.
///
/// Creates a new frame.  PARAMETERS is an alist; we currently
/// only honour `width`, `height`, and `name`.
pub(crate) fn builtin_make_frame(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let mut width: u32 = 800;
    let mut height: u32 = 600;
    let mut name = String::from("F");

    // Parse optional alist parameters.
    if let Some(params) = args.first() {
        if let Some(items) = super::value::list_to_vec(params) {
            for item in &items {
                if let Value::Cons(cell) = item {
                    let pair = cell.lock().expect("poisoned");
                    if let Value::Symbol(key) = &pair.car {
                        match key.as_str() {
                            "width" => {
                                if let Some(n) = pair.cdr.as_int() {
                                    width = n as u32;
                                }
                            }
                            "height" => {
                                if let Some(n) = pair.cdr.as_int() {
                                    height = n as u32;
                                }
                            }
                            "name" => {
                                if let Some(s) = pair.cdr.as_str() {
                                    name = s.to_string();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // Use the current buffer (or BufferId(0) as fallback) for the initial window.
    let buf_id = eval
        .buffers
        .current_buffer()
        .map(|b| b.id)
        .unwrap_or(BufferId(0));
    let fid = eval.frames.create_frame(&name, width, height, buf_id);
    Ok(Value::Int(fid.0 as i64))
}

/// `(delete-frame &optional FRAME FORCE)` -> nil.
pub(crate) fn builtin_delete_frame(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let fid = resolve_frame_id(&eval.frames, args.first())?;
    if !eval.frames.delete_frame(fid) {
        return Err(signal("error", vec![Value::string("Cannot delete frame")]));
    }
    Ok(Value::Nil)
}

/// `(frame-parameter FRAME PARAMETER)` -> value or nil.
pub(crate) fn builtin_frame_parameter(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("frame-parameter", &args, 2)?;
    let fid = resolve_frame_id(&eval.frames, Some(&args[0]))?;
    let param_name = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Ok(Value::Nil),
    };
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    // Check built-in properties first.
    match param_name.as_str() {
        "name" => return Ok(Value::string(frame.name.clone())),
        "title" => return Ok(Value::string(frame.title.clone())),
        "width" => return Ok(Value::Int(frame.width as i64)),
        "height" => return Ok(Value::Int(frame.height as i64)),
        "visibility" => {
            return Ok(if frame.visible {
                Value::True
            } else {
                Value::Nil
            })
        }
        _ => {}
    }
    // User-set parameters.
    Ok(frame
        .parameters
        .get(&param_name)
        .cloned()
        .unwrap_or(Value::Nil))
}

/// `(frame-parameters &optional FRAME)` -> alist.
pub(crate) fn builtin_frame_parameters(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let fid = resolve_frame_id(&eval.frames, args.first())?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    let mut pairs: Vec<Value> = Vec::new();
    // Built-in parameters.
    pairs.push(Value::cons(
        Value::symbol("name"),
        Value::string(frame.name.clone()),
    ));
    pairs.push(Value::cons(
        Value::symbol("title"),
        Value::string(frame.title.clone()),
    ));
    pairs.push(Value::cons(
        Value::symbol("width"),
        Value::Int(frame.width as i64),
    ));
    pairs.push(Value::cons(
        Value::symbol("height"),
        Value::Int(frame.height as i64),
    ));
    pairs.push(Value::cons(
        Value::symbol("visibility"),
        Value::bool(frame.visible),
    ));
    // User parameters.
    for (k, v) in &frame.parameters {
        pairs.push(Value::cons(Value::symbol(k.clone()), v.clone()));
    }
    Ok(Value::list(pairs))
}

/// `(modify-frame-parameters FRAME ALIST)` -> nil.
pub(crate) fn builtin_modify_frame_parameters(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("modify-frame-parameters", &args, 2)?;
    let fid = resolve_frame_id(&eval.frames, Some(&args[0]))?;
    let items = super::value::list_to_vec(&args[1]).unwrap_or_default();

    let frame = eval
        .frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;

    for item in items {
        if let Value::Cons(cell) = &item {
            let pair = cell.lock().expect("poisoned");
            if let Value::Symbol(key) = &pair.car {
                match key.as_str() {
                    "name" => {
                        if let Some(s) = pair.cdr.as_str() {
                            frame.name = s.to_string();
                        }
                    }
                    "title" => {
                        if let Some(s) = pair.cdr.as_str() {
                            frame.title = s.to_string();
                        }
                    }
                    "width" => {
                        if let Some(n) = pair.cdr.as_int() {
                            frame.width = n as u32;
                        }
                    }
                    "height" => {
                        if let Some(n) = pair.cdr.as_int() {
                            frame.height = n as u32;
                        }
                    }
                    "visibility" => {
                        frame.visible = pair.cdr.is_truthy();
                    }
                    _ => {
                        frame.parameters.insert(key.clone(), pair.cdr.clone());
                    }
                }
            }
        }
    }
    Ok(Value::Nil)
}

/// `(frame-visible-p &optional FRAME)` -> t or nil.
pub(crate) fn builtin_frame_visible_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let fid = resolve_frame_id(&eval.frames, args.first())?;
    let frame = eval
        .frames
        .get(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    Ok(Value::bool(frame.visible))
}

/// `(framep OBJ)` -> t if OBJ is a frame id that exists.
pub(crate) fn builtin_framep(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("framep", &args, 1)?;
    let id = match args[0].as_int() {
        Some(n) => n as u64,
        None => return Ok(Value::Nil),
    };
    Ok(Value::bool(eval.frames.get(FrameId(id)).is_some()))
}

/// `(frame-live-p OBJ)` -> t if OBJ is a live frame id.
pub(crate) fn builtin_frame_live_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("frame-live-p", &args, 1)?;
    let id = match args[0].as_int() {
        Some(n) => n as u64,
        None => return Ok(Value::Nil),
    };
    Ok(Value::bool(eval.frames.get(FrameId(id)).is_some()))
}

// ===========================================================================
// Internal helper — resolve buffer from int id, Buffer value, or string name
// ===========================================================================

fn resolve_buffer_id(eval: &super::eval::Evaluator, val: &Value) -> Result<BufferId, Flow> {
    match val {
        Value::Buffer(id) => Ok(*id),
        Value::Str(name) => eval.buffers.find_buffer_by_name(name).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string(format!("No buffer named {name}"))],
            )
        }),
        Value::Int(n) => Ok(BufferId(*n as u64)),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("bufferp"), val.clone()],
        )),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use crate::elisp::{format_eval_result, parse_forms, Evaluator};

    /// Evaluate all forms with a fresh evaluator that has a frame+window set up.
    fn eval_with_frame(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        // Create a buffer for the initial window.
        let buf = ev.buffers.create_buffer("*scratch*");
        // Create a frame so window/frame builtins have something to work with.
        ev.frames.create_frame("F1", 800, 600, buf);
        ev.eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    fn eval_one_with_frame(src: &str) -> String {
        eval_with_frame(src).into_iter().next().unwrap()
    }

    // -- Window queries --

    #[test]
    fn selected_window_returns_int() {
        let r = eval_one_with_frame("(selected-window)");
        assert!(r.starts_with("OK "), "expected OK, got: {r}");
        let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
        assert!(val > 0);
    }

    #[test]
    fn windowp_true() {
        let r = eval_with_frame("(windowp (selected-window))");
        assert_eq!(r[0], "OK t");
    }

    #[test]
    fn windowp_false() {
        let r = eval_one_with_frame("(windowp 999999)");
        assert_eq!(r, "OK nil");
    }

    #[test]
    fn window_live_p_true() {
        let r = eval_with_frame("(window-live-p (selected-window))");
        assert_eq!(r[0], "OK t");
    }

    #[test]
    fn window_live_p_false_for_non_window() {
        let r = eval_one_with_frame("(window-live-p 999999)");
        assert_eq!(r, "OK nil");
    }

    #[test]
    fn window_buffer_returns_buffer() {
        let r = eval_one_with_frame("(bufferp (window-buffer))");
        assert_eq!(r, "OK t");
    }

    #[test]
    fn window_start_default() {
        let r = eval_one_with_frame("(window-start)");
        assert_eq!(r, "OK 0");
    }

    #[test]
    fn set_window_start_and_read() {
        let results = eval_with_frame(
            "(set-window-start (selected-window) 42)
             (window-start)",
        );
        assert_eq!(results[0], "OK 42");
        assert_eq!(results[1], "OK 42");
    }

    #[test]
    fn window_point_default() {
        let r = eval_one_with_frame("(window-point)");
        assert_eq!(r, "OK 0");
    }

    #[test]
    fn set_window_point_and_read() {
        let results = eval_with_frame(
            "(set-window-point (selected-window) 10)
             (window-point)",
        );
        assert_eq!(results[0], "OK 10");
        assert_eq!(results[1], "OK 10");
    }

    #[test]
    fn window_height_positive() {
        let r = eval_one_with_frame("(window-height)");
        assert!(r.starts_with("OK "));
        let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
        assert!(val > 0, "window-height should be positive, got {val}");
    }

    #[test]
    fn window_width_positive() {
        let r = eval_one_with_frame("(window-width)");
        assert!(r.starts_with("OK "));
        let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
        assert!(val > 0, "window-width should be positive, got {val}");
    }

    #[test]
    fn window_body_height_pixelwise() {
        let r = eval_one_with_frame("(window-body-height nil t)");
        assert!(r.starts_with("OK "));
        let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
        // 600 pixels
        assert_eq!(val, 600);
    }

    #[test]
    fn window_body_width_pixelwise() {
        let r = eval_one_with_frame("(window-body-width nil t)");
        assert!(r.starts_with("OK "));
        let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
        assert_eq!(val, 800);
    }

    #[test]
    fn window_list_returns_list() {
        let r = eval_one_with_frame("(listp (window-list))");
        assert_eq!(r, "OK t");
    }

    #[test]
    fn window_list_has_one_entry() {
        let r = eval_one_with_frame("(length (window-list))");
        assert_eq!(r, "OK 1");
    }

    #[test]
    fn window_dedicated_p_default() {
        let r = eval_one_with_frame("(window-dedicated-p)");
        assert_eq!(r, "OK nil");
    }

    #[test]
    fn set_window_dedicated_p() {
        let results = eval_with_frame(
            "(set-window-dedicated-p (selected-window) t)
             (window-dedicated-p)",
        );
        assert_eq!(results[0], "OK t");
        assert_eq!(results[1], "OK t");
    }

    // -- Window manipulation --

    #[test]
    fn split_window_creates_new() {
        let results = eval_with_frame(
            "(split-window)
             (length (window-list))",
        );
        assert!(results[0].starts_with("OK "));
        assert_eq!(results[1], "OK 2");
    }

    #[test]
    fn delete_window_after_split() {
        let results = eval_with_frame(
            "(let ((new-win (split-window)))
               (delete-window new-win)
               (length (window-list)))",
        );
        assert_eq!(results[0], "OK 1");
    }

    #[test]
    fn delete_sole_window_errors() {
        let r = eval_one_with_frame("(delete-window)");
        assert!(r.contains("ERR"), "deleting sole window should error: {r}");
    }

    #[test]
    fn delete_other_windows_keeps_one() {
        let results = eval_with_frame(
            "(split-window)
             (split-window)
             (delete-other-windows)
             (length (window-list))",
        );
        assert_eq!(results[3], "OK 1");
    }

    #[test]
    fn select_window_works() {
        let results = eval_with_frame(
            "(let ((new-win (split-window)))
               (select-window new-win)
               (= (selected-window) new-win))",
        );
        assert_eq!(results[0], "OK t");
    }

    #[test]
    fn other_window_cycles() {
        let results = eval_with_frame(
            "(let ((w1 (selected-window)))
               (split-window)
               (other-window 1)
               (/= (selected-window) w1))",
        );
        assert_eq!(results[0], "OK t");
    }

    #[test]
    fn other_window_defaults_count_to_one() {
        let results = eval_with_frame(
            "(let ((w1 (selected-window)))
               (split-window)
               (other-window)
               (/= (selected-window) w1))",
        );
        assert_eq!(results[0], "OK t");
    }

    #[test]
    fn other_window_without_selected_frame_returns_nil() {
        let forms = parse_forms("(other-window)").expect("parse");
        let mut ev = Evaluator::new();
        let results = ev.eval_forms(&forms);
        assert_eq!(format_eval_result(&results[0]), "OK nil");
    }

    #[test]
    fn selected_frame_bootstraps_initial_frame() {
        let forms = parse_forms("(list (framep (selected-frame)) (length (frame-list)))").expect("parse");
        let mut ev = Evaluator::new();
        let results = ev.eval_forms(&forms);
        assert_eq!(format_eval_result(&results[0]), "OK (t 1)");
    }

    #[test]
    fn next_window_cycles() {
        let results = eval_with_frame(
            "(let ((w1 (selected-window)))
               (split-window)
               (let ((w2 (next-window)))
                 (/= w1 w2)))",
        );
        assert_eq!(results[0], "OK t");
    }

    #[test]
    fn previous_window_wraps() {
        let results = eval_with_frame(
            "(split-window)
             (let ((w (previous-window)))
               (windowp w))",
        );
        assert_eq!(results[1], "OK t");
    }

    // -- Frame operations --

    #[test]
    fn selected_frame_returns_int() {
        let r = eval_one_with_frame("(selected-frame)");
        assert!(r.starts_with("OK "));
        let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
        assert!(val > 0);
    }

    #[test]
    fn frame_list_has_one() {
        let r = eval_one_with_frame("(length (frame-list))");
        assert_eq!(r, "OK 1");
    }

    #[test]
    fn make_frame_creates_new() {
        let results = eval_with_frame(
            "(make-frame)
             (length (frame-list))",
        );
        assert!(results[0].starts_with("OK "));
        assert_eq!(results[1], "OK 2");
    }

    #[test]
    fn delete_frame_works() {
        let results = eval_with_frame(
            "(let ((f2 (make-frame)))
               (delete-frame f2)
               (length (frame-list)))",
        );
        assert_eq!(results[0], "OK 1");
    }

    #[test]
    fn framep_true() {
        let r = eval_one_with_frame("(framep (selected-frame))");
        assert_eq!(r, "OK t");
    }

    #[test]
    fn framep_false() {
        let r = eval_one_with_frame("(framep 999999)");
        assert_eq!(r, "OK nil");
    }

    #[test]
    fn frame_live_p_true() {
        let r = eval_one_with_frame("(frame-live-p (selected-frame))");
        assert_eq!(r, "OK t");
    }

    #[test]
    fn frame_live_p_false() {
        let r = eval_one_with_frame("(frame-live-p 999999)");
        assert_eq!(r, "OK nil");
    }

    #[test]
    fn frame_visible_p_default() {
        let r = eval_one_with_frame("(frame-visible-p)");
        assert_eq!(r, "OK t");
    }

    #[test]
    fn frame_parameter_name() {
        let r = eval_one_with_frame("(frame-parameter (selected-frame) 'name)");
        assert_eq!(r, r#"OK "F1""#);
    }

    #[test]
    fn frame_parameter_width() {
        let r = eval_one_with_frame("(frame-parameter (selected-frame) 'width)");
        assert_eq!(r, "OK 800");
    }

    #[test]
    fn frame_parameters_returns_alist() {
        let r = eval_one_with_frame("(listp (frame-parameters))");
        assert_eq!(r, "OK t");
    }

    #[test]
    fn modify_frame_parameters_name() {
        let results = eval_with_frame(
            "(modify-frame-parameters (selected-frame) '((name . \"NewName\")))
             (frame-parameter (selected-frame) 'name)",
        );
        assert_eq!(results[0], "OK nil");
        assert_eq!(results[1], r#"OK "NewName""#);
    }

    #[test]
    fn switch_to_buffer_changes_window() {
        let results = eval_with_frame(
            "(get-buffer-create \"other-buf\")
             (switch-to-buffer \"other-buf\")
             (bufferp (window-buffer))",
        );
        assert_eq!(results[2], "OK t");
    }

    #[test]
    fn set_window_buffer_works() {
        let results = eval_with_frame(
            "(get-buffer-create \"buf2\")
             (set-window-buffer (selected-window) \"buf2\")
             (bufferp (window-buffer))",
        );
        assert_eq!(results[1], "OK nil"); // set-window-buffer returns nil
        assert_eq!(results[2], "OK t");
    }

    #[test]
    fn window_end_greater_than_start() {
        let r = eval_one_with_frame("(> (window-end) (window-start))");
        assert_eq!(r, "OK t");
    }

    #[test]
    fn display_buffer_returns_window() {
        let results = eval_with_frame(
            "(get-buffer-create \"disp-buf\")
             (windowp (display-buffer \"disp-buf\"))",
        );
        assert_eq!(results[1], "OK t");
    }

    #[test]
    fn pop_to_buffer_returns_window() {
        let results = eval_with_frame(
            "(get-buffer-create \"pop-buf\")
             (windowp (pop-to-buffer \"pop-buf\"))",
        );
        assert_eq!(results[1], "OK t");
    }
}
