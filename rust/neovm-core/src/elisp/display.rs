//! Frame/display property builtins.
//!
//! Provides stub implementations for display and terminal query functions.
//! Since Neomacs is always a GUI application, most display queries return
//! sensible defaults for a modern graphical display.

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use crate::window::FrameId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

thread_local! {
    static TERMINAL_PARAMS: RefCell<HashMap<HashKey, Value>> = RefCell::new(HashMap::new());
    static TERMINAL_HANDLE: Arc<Mutex<Vec<Value>>> =
        Arc::new(Mutex::new(vec![Value::symbol("--neovm-terminal--")]));
}

const TERMINAL_NAME: &str = "initial_terminal";
const TERMINAL_ID: u64 = 0;
static CURSOR_VISIBLE: AtomicBool = AtomicBool::new(true);

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_max_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    if args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

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

fn expect_range_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_symbol_name(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        Value::Symbol(name) => Ok(name.clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
        )),
    }
}

fn terminal_designator_p(value: &Value) -> bool {
    value.is_nil() || is_terminal_handle(value)
}

fn expect_terminal_designator(value: &Value) -> Result<(), Flow> {
    if terminal_designator_p(value) {
        Ok(())
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("terminal-live-p"), value.clone()],
        ))
    }
}

fn expect_frame_designator(value: &Value) -> Result<(), Flow> {
    if value.is_nil() {
        Ok(())
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), value.clone()],
        ))
    }
}

fn expect_window_designator(value: &Value) -> Result<(), Flow> {
    if value.is_nil() {
        Ok(())
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("windowp"), value.clone()],
        ))
    }
}

fn expect_display_designator(value: &Value) -> Result<(), Flow> {
    if value.is_nil() || terminal_designator_p(value) {
        Ok(())
    } else {
        Err(signal(
            "error",
            vec![Value::string("Invalid argument 1 in 'get-device-terminal'")],
        ))
    }
}

fn live_frame_designator_p(eval: &mut super::eval::Evaluator, value: &Value) -> bool {
    match value {
        Value::Int(id) if *id >= 0 => eval.frames.get(FrameId(*id as u64)).is_some(),
        _ => false,
    }
}

fn expect_display_designator_eval(
    eval: &mut super::eval::Evaluator,
    value: &Value,
) -> Result<(), Flow> {
    if value.is_nil() || terminal_designator_p(value) || live_frame_designator_p(eval, value) {
        Ok(())
    } else {
        Err(signal(
            "error",
            vec![Value::string("Invalid argument 1 in 'get-device-terminal'")],
        ))
    }
}

fn expect_optional_display_designator_eval(
    eval: &mut super::eval::Evaluator,
    name: &str,
    args: &[Value],
) -> Result<(), Flow> {
    expect_max_args(name, args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator_eval(eval, display)?;
    }
    Ok(())
}

fn terminal_handle_value() -> Value {
    TERMINAL_HANDLE.with(|handle| Value::Vector(handle.clone()))
}

fn is_terminal_handle(value: &Value) -> bool {
    match value {
        Value::Vector(v) => TERMINAL_HANDLE.with(|handle| Arc::ptr_eq(v, handle)),
        _ => false,
    }
}

pub(crate) fn terminal_handle_id(value: &Value) -> Option<u64> {
    if is_terminal_handle(value) {
        Some(TERMINAL_ID)
    } else {
        None
    }
}

pub(crate) fn print_terminal_handle(value: &Value) -> Option<String> {
    terminal_handle_id(value).map(|id| format!("#<terminal {id} on {TERMINAL_NAME}>"))
}

// ---------------------------------------------------------------------------
// Helper: build an alist (association list) from key-value pairs
// ---------------------------------------------------------------------------

fn make_alist(pairs: Vec<(Value, Value)>) -> Value {
    let entries: Vec<Value> = pairs.into_iter().map(|(k, v)| Value::cons(k, v)).collect();
    Value::list(entries)
}

// ---------------------------------------------------------------------------
// Display query builtins
// ---------------------------------------------------------------------------

/// (redraw-frame &optional FRAME) -> nil
pub(crate) fn builtin_redraw_frame(args: Vec<Value>) -> EvalResult {
    expect_range_args("redraw-frame", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_designator(frame)?;
    }
    Ok(Value::Nil)
}

/// (redraw-display) -> nil
pub(crate) fn builtin_redraw_display(args: Vec<Value>) -> EvalResult {
    expect_args("redraw-display", &args, 0)?;
    Ok(Value::Nil)
}

/// (open-termscript FILE) -> error
///
/// NeoVM does not support terminal script logging.
pub(crate) fn builtin_open_termscript(args: Vec<Value>) -> EvalResult {
    expect_args("open-termscript", &args, 1)?;
    Err(signal(
        "error",
        vec![Value::string("Current frame is not on a tty device")],
    ))
}

/// (ding &optional ARG) -> nil
pub(crate) fn builtin_ding(args: Vec<Value>) -> EvalResult {
    expect_range_args("ding", &args, 0, 1)?;
    Ok(Value::Nil)
}

/// (send-string-to-terminal STRING &optional TERMINAL) -> nil
pub(crate) fn builtin_send_string_to_terminal(args: Vec<Value>) -> EvalResult {
    expect_range_args("send-string-to-terminal", &args, 1, 2)?;
    match &args[0] {
        Value::Str(_) => {
            if let Some(terminal) = args.get(1) {
                expect_terminal_designator(terminal)?;
            }
            Ok(Value::Nil)
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

/// (internal-show-cursor WINDOW SHOW) -> nil
pub(crate) fn builtin_internal_show_cursor(args: Vec<Value>) -> EvalResult {
    expect_args("internal-show-cursor", &args, 2)?;
    expect_window_designator(&args[0])?;
    CURSOR_VISIBLE.store(!args[1].is_nil(), Ordering::Relaxed);
    Ok(Value::Nil)
}

/// (internal-show-cursor-p &optional WINDOW) -> t/nil
pub(crate) fn builtin_internal_show_cursor_p(args: Vec<Value>) -> EvalResult {
    expect_range_args("internal-show-cursor-p", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_designator(window)?;
    }
    Ok(Value::bool(CURSOR_VISIBLE.load(Ordering::Relaxed)))
}

/// (display-graphic-p &optional DISPLAY) -> nil in batch-style vm context.
pub(crate) fn builtin_display_graphic_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-graphic-p", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Nil)
}

/// (display-color-p &optional DISPLAY) -> nil in batch-style vm context.
pub(crate) fn builtin_display_color_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-color-p", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Nil)
}

/// (display-pixel-width &optional DISPLAY) -> 80 (terminal columns in batch).
pub(crate) fn builtin_display_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-pixel-width", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Int(80))
}

/// (display-pixel-height &optional DISPLAY) -> 25 (terminal rows in batch).
pub(crate) fn builtin_display_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-pixel-height", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Int(25))
}

/// (display-mm-width &optional DISPLAY) -> nil in batch-style vm context.
pub(crate) fn builtin_display_mm_width(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-mm-width", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Nil)
}

/// (display-mm-height &optional DISPLAY) -> nil in batch-style vm context.
pub(crate) fn builtin_display_mm_height(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-mm-height", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Nil)
}

/// (display-screens &optional DISPLAY) -> 1
pub(crate) fn builtin_display_screens(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-screens", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Int(1))
}

/// (display-color-cells &optional DISPLAY) -> 0 in batch-style vm context.
pub(crate) fn builtin_display_color_cells(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-color-cells", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Int(0))
}

/// (display-planes &optional DISPLAY) -> 3 in batch-style vm context.
pub(crate) fn builtin_display_planes(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-planes", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::Int(3))
}

/// (display-visual-class &optional DISPLAY) -> 'static-gray in batch-style vm context.
pub(crate) fn builtin_display_visual_class(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-visual-class", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::symbol("static-gray"))
}

/// (display-backing-store &optional DISPLAY) -> 'not-useful in batch-style vm context.
pub(crate) fn builtin_display_backing_store(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-backing-store", &args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator(display)?;
    }
    Ok(Value::symbol("not-useful"))
}

/// Evaluator-aware variant of `display-graphic-p`.
pub(crate) fn builtin_display_graphic_p_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-graphic-p", &args)?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `display-color-p`.
pub(crate) fn builtin_display_color_p_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-color-p", &args)?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `display-pixel-width`.
pub(crate) fn builtin_display_pixel_width_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-pixel-width", &args)?;
    Ok(Value::Int(80))
}

/// Evaluator-aware variant of `display-pixel-height`.
pub(crate) fn builtin_display_pixel_height_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-pixel-height", &args)?;
    Ok(Value::Int(25))
}

/// Evaluator-aware variant of `display-mm-width`.
pub(crate) fn builtin_display_mm_width_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-mm-width", &args)?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `display-mm-height`.
pub(crate) fn builtin_display_mm_height_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-mm-height", &args)?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `display-screens`.
pub(crate) fn builtin_display_screens_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-screens", &args)?;
    Ok(Value::Int(1))
}

/// Evaluator-aware variant of `display-color-cells`.
pub(crate) fn builtin_display_color_cells_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-color-cells", &args)?;
    Ok(Value::Int(0))
}

/// Evaluator-aware variant of `display-planes`.
pub(crate) fn builtin_display_planes_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-planes", &args)?;
    Ok(Value::Int(3))
}

/// Evaluator-aware variant of `display-visual-class`.
pub(crate) fn builtin_display_visual_class_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-visual-class", &args)?;
    Ok(Value::symbol("static-gray"))
}

/// Evaluator-aware variant of `display-backing-store`.
pub(crate) fn builtin_display_backing_store_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-backing-store", &args)?;
    Ok(Value::symbol("not-useful"))
}

// ---------------------------------------------------------------------------
// X display builtins (compatibility stubs)
// ---------------------------------------------------------------------------

/// (x-display-list) -> nil in batch-style vm context.
pub(crate) fn builtin_x_display_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-list", &args, 0)?;
    Ok(Value::Nil)
}

/// (x-open-connection DISPLAY &optional XRM-STRING MUST-SUCCEED) -> nil
/// In batch/no-X context this reports a display-open failure.
pub(crate) fn builtin_x_open_connection(args: Vec<Value>) -> EvalResult {
    expect_range_args("x-open-connection", &args, 1, 3)?;
    match &args[0] {
        Value::Str(display) => Err(signal(
            "error",
            vec![Value::string(format!("Display {display} can't be opened"))],
        )),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

/// (x-close-connection DISPLAY) -> nil
/// In batch/no-X context this signals display/X availability errors.
pub(crate) fn builtin_x_close_connection(args: Vec<Value>) -> EvalResult {
    expect_args("x-close-connection", &args, 1)?;
    match &args[0] {
        Value::Nil => Err(signal(
            "error",
            vec![Value::string("X windows are not in use or not initialized")],
        )),
        Value::Str(display) => Err(signal(
            "error",
            vec![Value::string(format!("Display {display} can't be opened"))],
        )),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), other.clone()],
        )),
    }
}

/// (x-display-pixel-width &optional TERMINAL)
///
/// Batch/no-X semantics: signal X-not-in-use, invalid frame designator, or
/// display-open failure depending on argument shape.
pub(crate) fn builtin_x_display_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-pixel-width", &args, 1)?;
    match args.first() {
        None | Some(Value::Nil) => Err(signal(
            "error",
            vec![Value::string("X windows are not in use or not initialized")],
        )),
        Some(Value::Str(display)) => Err(signal(
            "error",
            vec![Value::string(format!("Display {display} can't be opened"))],
        )),
        Some(other) => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), other.clone()],
        )),
    }
}

/// (x-display-pixel-height &optional TERMINAL)
///
/// Batch/no-X semantics: signal X-not-in-use, invalid frame designator, or
/// display-open failure depending on argument shape.
pub(crate) fn builtin_x_display_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-pixel-height", &args, 1)?;
    match args.first() {
        None | Some(Value::Nil) => Err(signal(
            "error",
            vec![Value::string("X windows are not in use or not initialized")],
        )),
        Some(Value::Str(display)) => Err(signal(
            "error",
            vec![Value::string(format!("Display {display} can't be opened"))],
        )),
        Some(other) => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), other.clone()],
        )),
    }
}

/// (x-display-color-p &optional TERMINAL)
///
/// Batch/no-X semantics: nil for current display, otherwise argument-shape
/// specific errors.
pub(crate) fn builtin_x_display_color_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-color-p", &args, 1)?;
    match args.first() {
        None | Some(Value::Nil) => Ok(Value::Nil),
        Some(Value::Str(display)) => Err(signal(
            "error",
            vec![Value::string(format!("Display {display} does not exist"))],
        )),
        Some(_) => Err(signal(
            "error",
            vec![Value::string("Invalid argument 1 in 'get-device-terminal'")],
        )),
    }
}

// ---------------------------------------------------------------------------
// Terminal builtins
// ---------------------------------------------------------------------------

/// (terminal-name &optional TERMINAL) -> "initial_terminal"
pub(crate) fn builtin_terminal_name(args: Vec<Value>) -> EvalResult {
    expect_max_args("terminal-name", &args, 1)?;
    if let Some(term) = args.first() {
        if !term.is_nil() {
            expect_terminal_designator(term)?;
        }
    }
    Ok(Value::string(TERMINAL_NAME))
}

/// (terminal-list) -> list containing one opaque terminal handle.
pub(crate) fn builtin_terminal_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("terminal-list", &args, 0)?;
    Ok(Value::list(vec![terminal_handle_value()]))
}

/// (frame-terminal &optional FRAME) -> opaque terminal handle.
pub(crate) fn builtin_frame_terminal(args: Vec<Value>) -> EvalResult {
    expect_max_args("frame-terminal", &args, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_designator(frame)?;
    }
    Ok(terminal_handle_value())
}

/// (terminal-live-p TERMINAL) -> t
pub(crate) fn builtin_terminal_live_p(args: Vec<Value>) -> EvalResult {
    expect_range_args("terminal-live-p", &args, 1, 1)?;
    Ok(Value::bool(terminal_designator_p(&args[0])))
}

/// (terminal-parameter TERMINAL PARAMETER) -> nil (stub)
pub(crate) fn builtin_terminal_parameter(args: Vec<Value>) -> EvalResult {
    expect_args("terminal-parameter", &args, 2)?;
    expect_terminal_designator(&args[0])?;
    let key = HashKey::Symbol(expect_symbol_name(&args[1])?);
    TERMINAL_PARAMS.with(|slot| {
        Ok(slot
            .borrow()
            .get(&key)
            .cloned()
            .unwrap_or(Value::Nil))
    })
}

/// (set-terminal-parameter TERMINAL PARAMETER VALUE) -> nil
pub(crate) fn builtin_set_terminal_parameter(args: Vec<Value>) -> EvalResult {
    expect_args("set-terminal-parameter", &args, 3)?;
    expect_terminal_designator(&args[0])?;
    if let Ok(name) = expect_symbol_name(&args[1]) {
        let key = HashKey::Symbol(name);
        TERMINAL_PARAMS.with(|slot| {
            slot.borrow_mut().insert(key, args[2].clone());
        });
    }
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// TTY builtins (we are not a TTY, so these return nil)
// ---------------------------------------------------------------------------

/// (tty-type &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_type(args: Vec<Value>) -> EvalResult {
    expect_max_args("tty-type", &args, 1)?;
    if let Some(terminal) = args.first() {
        expect_terminal_designator(terminal)?;
    }
    Ok(Value::Nil)
}

/// (tty-top-frame &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_top_frame(args: Vec<Value>) -> EvalResult {
    expect_max_args("tty-top-frame", &args, 1)?;
    if let Some(terminal) = args.first() {
        expect_terminal_designator(terminal)?;
    }
    Ok(Value::Nil)
}

/// (controlling-tty-p &optional TERMINAL) -> nil
pub(crate) fn builtin_controlling_tty_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("controlling-tty-p", &args, 1)?;
    if let Some(terminal) = args.first() {
        expect_terminal_designator(terminal)?;
    }
    Ok(Value::Nil)
}

/// (suspend-tty &optional TTY) -> error in GUI/non-text terminal context.
pub(crate) fn builtin_suspend_tty(args: Vec<Value>) -> EvalResult {
    expect_max_args("suspend-tty", &args, 1)?;
    if let Some(terminal) = args.first() {
        expect_terminal_designator(terminal)?;
    }
    Err(signal(
        "error",
        vec![Value::string("Attempt to suspend a non-text terminal device")],
    ))
}

/// (resume-tty &optional TTY) -> error in GUI/non-text terminal context.
pub(crate) fn builtin_resume_tty(args: Vec<Value>) -> EvalResult {
    expect_max_args("resume-tty", &args, 1)?;
    if let Some(terminal) = args.first() {
        expect_terminal_designator(terminal)?;
    }
    Err(signal(
        "error",
        vec![Value::string("Attempt to resume a non-text terminal device")],
    ))
}

// ---------------------------------------------------------------------------
// Monitor attribute builtins
// ---------------------------------------------------------------------------

/// (display-monitor-attributes-list &optional DISPLAY) -> list with one monitor alist
///
/// Returns a list containing a single alist describing the primary monitor.
/// Keys: geometry, workarea, mm-size, frames.
pub(crate) fn builtin_display_monitor_attributes_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-monitor-attributes-list", &args, 1)?;
    if let Some(display) = args.first() {
        if !display.is_nil() && !terminal_designator_p(display) {
            return Err(signal(
                "error",
                vec![Value::string("Invalid argument 1 in 'get-device-terminal'")],
            ));
        }
    }
    let monitor = make_monitor_alist(Value::Nil);
    Ok(Value::list(vec![monitor]))
}

/// Evaluator-aware variant of `display-monitor-attributes-list`.
///
/// This populates the `frames` slot from the live frame list.
pub(crate) fn builtin_display_monitor_attributes_list_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("display-monitor-attributes-list", &args, 1)?;
    if let Some(display) = args.first() {
        if !display.is_nil() && !terminal_designator_p(display) {
            return Err(signal(
                "error",
                vec![Value::string("Invalid argument 1 in 'get-device-terminal'")],
            ));
        }
    }

    let _ = super::window_cmds::ensure_selected_frame_id(eval);
    let frames = eval
        .frames
        .frame_list()
        .into_iter()
        .map(|fid| Value::Int(fid.0 as i64))
        .collect::<Vec<_>>();
    Ok(Value::list(vec![make_monitor_alist(Value::list(frames))]))
}

/// (frame-monitor-attributes &optional FRAME) -> alist with geometry info
pub(crate) fn builtin_frame_monitor_attributes(args: Vec<Value>) -> EvalResult {
    expect_max_args("frame-monitor-attributes", &args, 1)?;
    if let Some(frame) = args.first() {
        if !frame.is_nil() && !terminal_designator_p(frame) {
            return Err(signal(
                "error",
                vec![Value::string("Invalid argument 1 in 'get-device-terminal'")],
            ));
        }
    }
    Ok(make_monitor_alist(Value::Nil))
}

/// Evaluator-aware variant of `frame-monitor-attributes`.
///
/// This populates the `frames` slot from the live frame list.
pub(crate) fn builtin_frame_monitor_attributes_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-monitor-attributes", &args, 1)?;
    if let Some(frame) = args.first() {
        if !frame.is_nil() && !terminal_designator_p(frame) {
            return Err(signal(
                "error",
                vec![Value::string("Invalid argument 1 in 'get-device-terminal'")],
            ));
        }
    }

    let _ = super::window_cmds::ensure_selected_frame_id(eval);
    let frames = eval
        .frames
        .frame_list()
        .into_iter()
        .map(|fid| Value::Int(fid.0 as i64))
        .collect::<Vec<_>>();
    Ok(make_monitor_alist(Value::list(frames)))
}

/// Build a single monitor alist with reasonable default values.
fn make_monitor_alist(frames: Value) -> Value {
    // geometry: (x y width height)
    let geometry = Value::list(vec![
        Value::Int(0),
        Value::Int(0),
        Value::Int(80),
        Value::Int(25),
    ]);

    // workarea: (x y width height)
    let workarea = Value::list(vec![
        Value::Int(0),
        Value::Int(0),
        Value::Int(80),
        Value::Int(25),
    ]);

    // mm-size: (width-mm height-mm)
    let mm_size = Value::list(vec![Value::Nil, Value::Nil]);

    make_alist(vec![
        (Value::symbol("geometry"), geometry),
        (Value::symbol("workarea"), workarea),
        (Value::symbol("mm-size"), mm_size),
        (Value::symbol("frames"), frames),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_terminal_parameters() {
        TERMINAL_PARAMS.with(|slot| slot.borrow_mut().clear());
    }

    fn reset_cursor_visible() {
        CURSOR_VISIBLE.store(true, Ordering::Relaxed);
    }

    #[test]
    fn terminal_parameter_defaults_to_nil() {
        clear_terminal_parameters();
        let result = builtin_terminal_parameter(vec![Value::Nil, Value::symbol("neovm-param")]);
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn terminal_parameter_round_trips() {
        clear_terminal_parameters();
        let set_result = builtin_set_terminal_parameter(vec![
            Value::Nil,
            Value::symbol("neovm-param"),
            Value::Int(42),
        ])
        .unwrap();
        assert!(set_result.is_nil());

        let get_result =
            builtin_terminal_parameter(vec![Value::Nil, Value::symbol("neovm-param")]).unwrap();
        assert_eq!(get_result, Value::Int(42));
    }

    #[test]
    fn terminal_parameter_distinct_keys_do_not_alias() {
        clear_terminal_parameters();
        builtin_set_terminal_parameter(vec![Value::Nil, Value::symbol("k1"), Value::Int(1)])
            .unwrap();
        builtin_set_terminal_parameter(vec![Value::Nil, Value::symbol("k2"), Value::Int(2)])
            .unwrap();

        let first = builtin_terminal_parameter(vec![Value::Nil, Value::symbol("k1")]).unwrap();
        let second = builtin_terminal_parameter(vec![Value::Nil, Value::symbol("k2")]).unwrap();
        assert_eq!(first, Value::Int(1));
        assert_eq!(second, Value::Int(2));
    }

    #[test]
    fn terminal_parameter_rejects_non_symbol_key() {
        clear_terminal_parameters();
        let result = builtin_terminal_parameter(vec![Value::Nil, Value::string("k")]);
        assert!(result.is_err());
    }

    #[test]
    fn set_terminal_parameter_ignores_non_symbol_key() {
        clear_terminal_parameters();
        let set_result =
            builtin_set_terminal_parameter(vec![Value::Nil, Value::string("k"), Value::Int(9)])
                .unwrap();
        assert!(set_result.is_nil());

        let get_result = builtin_terminal_parameter(vec![Value::Nil, Value::symbol("k")]).unwrap();
        assert!(get_result.is_nil());
    }

    #[test]
    fn terminal_parameter_rejects_non_terminal_designator() {
        clear_terminal_parameters();
        let result = builtin_terminal_parameter(vec![Value::Int(1), Value::symbol("k")]);
        assert!(result.is_err());
    }

    #[test]
    fn set_terminal_parameter_rejects_non_terminal_designator() {
        clear_terminal_parameters();
        let result =
            builtin_set_terminal_parameter(vec![Value::Int(1), Value::symbol("k"), Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn terminal_live_p_reflects_designator_shape() {
        let live_nil = builtin_terminal_live_p(vec![Value::Nil]).unwrap();
        let live_handle = builtin_terminal_live_p(vec![terminal_handle_value()]).unwrap();
        let live_string = builtin_terminal_live_p(vec![Value::string("initial_terminal")]).unwrap();
        let live_int = builtin_terminal_live_p(vec![Value::Int(1)]).unwrap();
        assert_eq!(live_nil, Value::True);
        assert_eq!(live_handle, Value::True);
        assert!(live_string.is_nil());
        assert!(live_int.is_nil());
    }

    #[test]
    fn terminal_name_rejects_invalid_designator() {
        let result = builtin_terminal_name(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn frame_terminal_rejects_non_frame_designator() {
        let result = builtin_frame_terminal(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn frame_terminal_returns_live_terminal_handle() {
        let handle = builtin_frame_terminal(vec![Value::Nil]).unwrap();
        let live = builtin_terminal_live_p(vec![handle]).unwrap();
        assert_eq!(live, Value::True);
    }

    #[test]
    fn redraw_frame_rejects_non_frame_designator() {
        let result = builtin_redraw_frame(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn open_termscript_uses_batch_tty_error_payload() {
        let result = builtin_open_termscript(vec![Value::Nil]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(sig.data, vec![Value::string("Current frame is not on a tty device")]);
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    #[test]
    fn send_string_to_terminal_rejects_invalid_terminal_designator() {
        let result = builtin_send_string_to_terminal(vec![Value::string(""), Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn send_string_to_terminal_accepts_live_terminal_handle() {
        let handle = terminal_handle_value();
        let result = builtin_send_string_to_terminal(vec![Value::string(""), handle]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_show_cursor_tracks_visibility_state() {
        reset_cursor_visible();
        let default_visible = builtin_internal_show_cursor_p(vec![]).unwrap();
        assert_eq!(default_visible, Value::True);

        builtin_internal_show_cursor(vec![Value::Nil, Value::Nil]).unwrap();
        let hidden = builtin_internal_show_cursor_p(vec![]).unwrap();
        assert!(hidden.is_nil());

        builtin_internal_show_cursor(vec![Value::Nil, Value::True]).unwrap();
        let visible = builtin_internal_show_cursor_p(vec![]).unwrap();
        assert_eq!(visible, Value::True);
    }

    #[test]
    fn internal_show_cursor_rejects_non_window_designator() {
        let result = builtin_internal_show_cursor(vec![Value::Int(1), Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn tty_queries_reject_invalid_terminal_designator() {
        let tty_type = builtin_tty_type(vec![Value::Int(1)]);
        let tty_top_frame = builtin_tty_top_frame(vec![Value::Int(1)]);
        let controlling = builtin_controlling_tty_p(vec![Value::Int(1)]);
        assert!(tty_type.is_err());
        assert!(tty_top_frame.is_err());
        assert!(controlling.is_err());
    }

    #[test]
    fn suspend_tty_signals_non_text_terminal_error() {
        for args in [vec![], vec![Value::Nil], vec![terminal_handle_value()]] {
            let result = builtin_suspend_tty(args);
            match result {
                Err(Flow::Signal(sig)) => {
                    assert_eq!(sig.symbol, "error");
                    assert_eq!(
                        sig.data,
                        vec![Value::string("Attempt to suspend a non-text terminal device")]
                    );
                }
                other => panic!("expected error signal, got {other:?}"),
            }
        }
    }

    #[test]
    fn resume_tty_signals_non_text_terminal_error() {
        for args in [vec![], vec![Value::Nil], vec![terminal_handle_value()]] {
            let result = builtin_resume_tty(args);
            match result {
                Err(Flow::Signal(sig)) => {
                    assert_eq!(sig.symbol, "error");
                    assert_eq!(
                        sig.data,
                        vec![Value::string("Attempt to resume a non-text terminal device")]
                    );
                }
                other => panic!("expected error signal, got {other:?}"),
            }
        }
    }

    #[test]
    fn x_open_connection_requires_string_display_arg() {
        let bad = builtin_x_open_connection(vec![Value::Nil]);
        assert!(bad.is_err());
    }

    #[test]
    fn x_close_connection_argument_shape_errors() {
        let x_nil = builtin_x_close_connection(vec![Value::Nil]);
        let x_int = builtin_x_close_connection(vec![Value::Int(1)]);
        let x_str = builtin_x_close_connection(vec![Value::string("")]);
        assert!(x_nil.is_err());
        assert!(x_int.is_err());
        assert!(x_str.is_err());
    }

    #[test]
    fn x_display_pixel_size_errors_match_batch_shapes() {
        let width_none = builtin_x_display_pixel_width(vec![]);
        let width_int = builtin_x_display_pixel_width(vec![Value::Int(1)]);
        let width_str = builtin_x_display_pixel_width(vec![Value::string("")]);
        let height_none = builtin_x_display_pixel_height(vec![]);
        let height_int = builtin_x_display_pixel_height(vec![Value::Int(1)]);
        let height_str = builtin_x_display_pixel_height(vec![Value::string("")]);
        assert!(width_none.is_err());
        assert!(width_int.is_err());
        assert!(width_str.is_err());
        assert!(height_none.is_err());
        assert!(height_int.is_err());
        assert!(height_str.is_err());
    }

    #[test]
    fn x_display_color_p_batch_and_arg_errors() {
        let none = builtin_x_display_color_p(vec![]).unwrap();
        let nil = builtin_x_display_color_p(vec![Value::Nil]).unwrap();
        let int_err = builtin_x_display_color_p(vec![Value::Int(1)]);
        let str_err = builtin_x_display_color_p(vec![Value::string("")]);
        assert!(none.is_nil());
        assert!(nil.is_nil());
        assert!(int_err.is_err());
        assert!(str_err.is_err());
    }

    #[test]
    fn eval_monitor_attributes_include_bootstrapped_frame() {
        let mut eval = crate::elisp::Evaluator::new();
        let list = builtin_display_monitor_attributes_list_eval(&mut eval, vec![]).unwrap();
        let monitors = list_to_vec(&list).expect("monitor list");
        let attrs = list_to_vec(monitors.first().expect("first monitor")).expect("monitor attrs");

        let mut frames_value = Value::Nil;
        for attr in attrs {
            if let Value::Cons(cell) = attr {
                let pair = cell.lock().expect("poisoned");
                if matches!(&pair.car, Value::Symbol(name) if name == "frames") {
                    frames_value = pair.cdr.clone();
                    break;
                }
            }
        }

        let frames = list_to_vec(&frames_value).expect("frames list");
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn eval_display_queries_accept_live_frame_designator() {
        let mut eval = crate::elisp::Evaluator::new();
        let frame_id = crate::elisp::window_cmds::ensure_selected_frame_id(&mut eval).0 as i64;

        assert!(
            builtin_display_graphic_p_eval(&mut eval, vec![Value::Int(frame_id)])
                .unwrap()
                .is_nil()
        );
        assert!(
            builtin_display_color_p_eval(&mut eval, vec![Value::Int(frame_id)])
                .unwrap()
                .is_nil()
        );
        assert_eq!(
            builtin_display_pixel_width_eval(&mut eval, vec![Value::Int(frame_id)]).unwrap(),
            Value::Int(80)
        );
        assert_eq!(
            builtin_display_pixel_height_eval(&mut eval, vec![Value::Int(frame_id)]).unwrap(),
            Value::Int(25)
        );
        assert!(
            builtin_display_mm_width_eval(&mut eval, vec![Value::Int(frame_id)])
                .unwrap()
                .is_nil()
        );
        assert!(
            builtin_display_mm_height_eval(&mut eval, vec![Value::Int(frame_id)])
                .unwrap()
                .is_nil()
        );
        assert_eq!(
            builtin_display_screens_eval(&mut eval, vec![Value::Int(frame_id)]).unwrap(),
            Value::Int(1)
        );
        assert_eq!(
            builtin_display_color_cells_eval(&mut eval, vec![Value::Int(frame_id)]).unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            builtin_display_planes_eval(&mut eval, vec![Value::Int(frame_id)]).unwrap(),
            Value::Int(3)
        );
        assert_eq!(
            builtin_display_visual_class_eval(&mut eval, vec![Value::Int(frame_id)]).unwrap(),
            Value::symbol("static-gray")
        );
        assert_eq!(
            builtin_display_backing_store_eval(&mut eval, vec![Value::Int(frame_id)]).unwrap(),
            Value::symbol("not-useful")
        );
    }

    #[test]
    fn eval_display_queries_reject_invalid_frame_designator() {
        let mut eval = crate::elisp::Evaluator::new();
        let _ = crate::elisp::window_cmds::ensure_selected_frame_id(&mut eval);
        let result = builtin_display_pixel_width_eval(&mut eval, vec![Value::Int(999_999)]);
        assert!(result.is_err());
    }
}
