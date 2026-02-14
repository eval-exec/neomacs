//! Frame/display property builtins.
//!
//! Provides stub implementations for display and terminal query functions.
//! Since Neomacs is always a GUI application, most display queries return
//! sensible defaults for a modern graphical display.

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static TERMINAL_PARAMS: RefCell<HashMap<HashKey, Value>> = RefCell::new(HashMap::new());
}

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
    matches!(value, Value::Nil | Value::Str(_))
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
    if let Some(term) = args.first() {
        if !term.is_nil() {
            expect_terminal_designator(term)?;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_terminal_parameters() {
        TERMINAL_PARAMS.with(|slot| slot.borrow_mut().clear());
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
        let live_string = builtin_terminal_live_p(vec![Value::string("neomacs")]).unwrap();
        let live_int = builtin_terminal_live_p(vec![Value::Int(1)]).unwrap();
        assert_eq!(live_nil, Value::True);
        assert_eq!(live_string, Value::True);
        assert!(live_int.is_nil());
    }

    #[test]
    fn terminal_name_rejects_invalid_designator() {
        let result = builtin_terminal_name(vec![Value::Int(1)]);
        assert!(result.is_err());
    }
}
