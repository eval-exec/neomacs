//! Frame/display property builtins.
//!
//! Provides stub implementations for display and terminal query functions.
//! Since Neomacs is always a GUI application, most display queries return
//! sensible defaults for a modern graphical display.

use super::error::{signal, EvalResult, Flow};
use super::value::*;

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

// ---------------------------------------------------------------------------
// Helper: build an alist (association list) from key-value pairs
// ---------------------------------------------------------------------------

fn make_alist(pairs: Vec<(Value, Value)>) -> Value {
    let entries: Vec<Value> = pairs
        .into_iter()
        .map(|(k, v)| Value::cons(k, v))
        .collect();
    Value::list(entries)
}

// ---------------------------------------------------------------------------
// Display query builtins
// ---------------------------------------------------------------------------

/// (display-graphic-p &optional DISPLAY) -> t
/// We are always a GUI application.
pub(crate) fn builtin_display_graphic_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-graphic-p", &args, 1)?;
    Ok(Value::True)
}

/// (display-color-p &optional DISPLAY) -> t
pub(crate) fn builtin_display_color_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-color-p", &args, 1)?;
    Ok(Value::True)
}

/// (display-pixel-width &optional DISPLAY) -> 1920
pub(crate) fn builtin_display_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-pixel-width", &args, 1)?;
    Ok(Value::Int(1920))
}

/// (display-pixel-height &optional DISPLAY) -> 1080
pub(crate) fn builtin_display_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-pixel-height", &args, 1)?;
    Ok(Value::Int(1080))
}

/// (display-mm-width &optional DISPLAY) -> 530
pub(crate) fn builtin_display_mm_width(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-mm-width", &args, 1)?;
    Ok(Value::Int(530))
}

/// (display-mm-height &optional DISPLAY) -> 300
pub(crate) fn builtin_display_mm_height(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-mm-height", &args, 1)?;
    Ok(Value::Int(300))
}

/// (display-screens &optional DISPLAY) -> 1
pub(crate) fn builtin_display_screens(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-screens", &args, 1)?;
    Ok(Value::Int(1))
}

/// (display-color-cells &optional DISPLAY) -> 16777216 (24-bit color)
pub(crate) fn builtin_display_color_cells(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-color-cells", &args, 1)?;
    Ok(Value::Int(16777216))
}

/// (display-planes &optional DISPLAY) -> 24
pub(crate) fn builtin_display_planes(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-planes", &args, 1)?;
    Ok(Value::Int(24))
}

/// (display-visual-class &optional DISPLAY) -> 'true-color
pub(crate) fn builtin_display_visual_class(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-visual-class", &args, 1)?;
    Ok(Value::symbol("true-color"))
}

/// (display-backing-store &optional DISPLAY) -> 'always
pub(crate) fn builtin_display_backing_store(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-backing-store", &args, 1)?;
    Ok(Value::symbol("always"))
}

// ---------------------------------------------------------------------------
// X display builtins (compatibility stubs)
// ---------------------------------------------------------------------------

/// (x-display-list) -> ("") — list with one empty string representing our display
pub(crate) fn builtin_x_display_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-list", &args, 0)?;
    Ok(Value::list(vec![Value::string("")]))
}

/// (x-open-connection DISPLAY &optional XRM-STRING MUST-SUCCEED) -> nil
/// Stub: we don't actually open X connections.
pub(crate) fn builtin_x_open_connection(args: Vec<Value>) -> EvalResult {
    expect_range_args("x-open-connection", &args, 1, 3)?;
    Ok(Value::Nil)
}

/// (x-close-connection DISPLAY) -> nil
/// Stub: we don't actually close X connections.
pub(crate) fn builtin_x_close_connection(args: Vec<Value>) -> EvalResult {
    expect_args("x-close-connection", &args, 1)?;
    Ok(Value::Nil)
}

/// (x-display-pixel-width &optional TERMINAL) -> 1920
pub(crate) fn builtin_x_display_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-pixel-width", &args, 1)?;
    Ok(Value::Int(1920))
}

/// (x-display-pixel-height &optional TERMINAL) -> 1080
pub(crate) fn builtin_x_display_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-pixel-height", &args, 1)?;
    Ok(Value::Int(1080))
}

/// (x-display-color-p &optional TERMINAL) -> t
pub(crate) fn builtin_x_display_color_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-color-p", &args, 1)?;
    Ok(Value::True)
}

// ---------------------------------------------------------------------------
// Terminal builtins
// ---------------------------------------------------------------------------

/// (terminal-name &optional TERMINAL) -> "neomacs"
pub(crate) fn builtin_terminal_name(args: Vec<Value>) -> EvalResult {
    expect_max_args("terminal-name", &args, 1)?;
    Ok(Value::string("neomacs"))
}

/// (terminal-list) -> ("neomacs")
pub(crate) fn builtin_terminal_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("terminal-list", &args, 0)?;
    Ok(Value::list(vec![Value::string("neomacs")]))
}

/// (frame-terminal &optional FRAME) -> "neomacs"
pub(crate) fn builtin_frame_terminal(args: Vec<Value>) -> EvalResult {
    expect_max_args("frame-terminal", &args, 1)?;
    Ok(Value::string("neomacs"))
}

/// (selected-terminal) -> "neomacs"
pub(crate) fn builtin_selected_terminal(args: Vec<Value>) -> EvalResult {
    expect_max_args("selected-terminal", &args, 0)?;
    Ok(Value::string("neomacs"))
}

/// (terminal-live-p TERMINAL) -> t
pub(crate) fn builtin_terminal_live_p(args: Vec<Value>) -> EvalResult {
    expect_range_args("terminal-live-p", &args, 1, 1)?;
    Ok(Value::True)
}

/// (terminal-parameter TERMINAL PARAMETER) -> nil (stub)
pub(crate) fn builtin_terminal_parameter(args: Vec<Value>) -> EvalResult {
    expect_args("terminal-parameter", &args, 2)?;
    Ok(Value::Nil)
}

/// (set-terminal-parameter TERMINAL PARAMETER VALUE) -> VALUE (stub)
pub(crate) fn builtin_set_terminal_parameter(args: Vec<Value>) -> EvalResult {
    expect_args("set-terminal-parameter", &args, 3)?;
    Ok(args[2].clone())
}

// ---------------------------------------------------------------------------
// TTY builtins (we are not a TTY, so these return nil)
// ---------------------------------------------------------------------------

/// (tty-type &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_type(args: Vec<Value>) -> EvalResult {
    expect_max_args("tty-type", &args, 1)?;
    Ok(Value::Nil)
}

/// (tty-top-frame &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_top_frame(args: Vec<Value>) -> EvalResult {
    expect_max_args("tty-top-frame", &args, 1)?;
    Ok(Value::Nil)
}

/// (controlling-tty-p &optional TERMINAL) -> nil
pub(crate) fn builtin_controlling_tty_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("controlling-tty-p", &args, 1)?;
    Ok(Value::Nil)
}

/// (suspend-tty &optional TTY) -> nil
pub(crate) fn builtin_suspend_tty(args: Vec<Value>) -> EvalResult {
    expect_max_args("suspend-tty", &args, 1)?;
    Ok(Value::Nil)
}

/// (resume-tty &optional TTY) -> nil
pub(crate) fn builtin_resume_tty(args: Vec<Value>) -> EvalResult {
    expect_max_args("resume-tty", &args, 1)?;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Monitor attribute builtins
// ---------------------------------------------------------------------------

/// (display-monitor-attributes-list &optional DISPLAY) -> list with one monitor alist
///
/// Returns a list containing a single alist describing the primary monitor.
/// Keys: geometry, workarea, mm-size, frames, name, source.
pub(crate) fn builtin_display_monitor_attributes_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("display-monitor-attributes-list", &args, 1)?;
    let monitor = make_monitor_alist();
    Ok(Value::list(vec![monitor]))
}

/// (frame-monitor-attributes &optional FRAME) -> alist with geometry info
pub(crate) fn builtin_frame_monitor_attributes(args: Vec<Value>) -> EvalResult {
    expect_max_args("frame-monitor-attributes", &args, 1)?;
    Ok(make_monitor_alist())
}

/// Build a single monitor alist with reasonable default values.
fn make_monitor_alist() -> Value {
    // geometry: (x y width height)
    let geometry = Value::list(vec![
        Value::Int(0),
        Value::Int(0),
        Value::Int(1920),
        Value::Int(1080),
    ]);

    // workarea: (x y width height)
    let workarea = Value::list(vec![
        Value::Int(0),
        Value::Int(0),
        Value::Int(1920),
        Value::Int(1080),
    ]);

    // mm-size: (width-mm . height-mm)
    let mm_size = Value::cons(Value::Int(530), Value::Int(300));

    // frames: nil (no frame objects in our stub)
    let frames = Value::Nil;

    make_alist(vec![
        (Value::symbol("geometry"), geometry),
        (Value::symbol("workarea"), workarea),
        (Value::symbol("mm-size"), mm_size),
        (Value::symbol("frames"), frames),
        (Value::symbol("name"), Value::string("default")),
        (Value::symbol("source"), Value::string("neomacs")),
    ])
}
