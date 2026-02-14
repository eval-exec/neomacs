//! Display engine builtins for the Elisp interpreter.
//!
//! Implements display-related functions from Emacs `xdisp.c`:
//! - `format-mode-line` — format a mode line string
//! - `invisible-p` — check if a position or property is invisible
//! - `line-pixel-height` — get line height in pixels
//! - `window-text-pixel-size` — calculate text pixel dimensions
//! - `pos-visible-in-window-p` — check if position is visible
//! - `move-point-visually` — move point in visual order
//! - `lookup-image-map` — lookup image map coordinates
//! - `current-bidi-paragraph-direction` — get bidi paragraph direction
//! - `move-to-window-line` — move to a specific window line
//! - `tool-bar-height` — get tool bar height
//! - `tab-bar-height` — get tab bar height
//! - `line-number-display-width` — get line number display width
//! - `long-line-optimizations-p` — check if long-line optimizations are enabled

use super::error::{signal, EvalResult, Flow};
use super::value::*;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

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

fn expect_args_range(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
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
// Pure builtins
// ---------------------------------------------------------------------------

/// (format-mode-line &optional FORMAT FACE WINDOW BUFFER) -> string
///
/// Formats a mode line string. Stub implementation returns empty string.
pub(crate) fn builtin_format_mode_line(args: Vec<Value>) -> EvalResult {
    expect_args_range("format-mode-line", &args, 0, 4)?;
    // Stub: return empty string
    Ok(Value::string(""))
}

/// (invisible-p POS-OR-PROP) -> boolean
///
/// Check if a position or property is invisible. Stub implementation
/// always returns nil (nothing is invisible).
pub(crate) fn builtin_invisible_p(args: Vec<Value>) -> EvalResult {
    expect_args("invisible-p", &args, 1)?;
    // Stub: always return nil
    Ok(Value::Nil)
}

/// (line-pixel-height) -> integer
///
/// Get the height of the current line in pixels. Stub implementation
/// returns 16 (typical line height).
pub(crate) fn builtin_line_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_args("line-pixel-height", &args, 0)?;
    // Stub: return 16 pixels
    Ok(Value::Int(16))
}

/// (window-text-pixel-size &optional WINDOW FROM TO X-LIMIT Y-LIMIT MODE) -> (WIDTH . HEIGHT)
///
/// Calculate the pixel size of window text. Stub implementation
/// returns a fixed size.
pub(crate) fn builtin_window_text_pixel_size(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-text-pixel-size", &args, 0, 6)?;
    // Stub: return (100 . 16)
    Ok(Value::cons(Value::Int(100), Value::Int(16)))
}

/// (pos-visible-in-window-p &optional POS WINDOW PARTIALLY) -> boolean
///
/// Check if a position is visible in a window. Stub implementation
/// always returns t.
pub(crate) fn builtin_pos_visible_in_window_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("pos-visible-in-window-p", &args, 0, 3)?;
    // Stub: always return t
    Ok(Value::True)
}

/// (move-point-visually DIRECTION) -> boolean
///
/// Move point in visual order (left/right). Stub implementation
/// always returns nil.
pub(crate) fn builtin_move_point_visually(args: Vec<Value>) -> EvalResult {
    expect_args("move-point-visually", &args, 1)?;
    // Stub: return nil
    Ok(Value::Nil)
}

/// (lookup-image-map MAP X Y) -> symbol or nil
///
/// Lookup an image map at coordinates. Stub implementation
/// always returns nil.
pub(crate) fn builtin_lookup_image_map(args: Vec<Value>) -> EvalResult {
    expect_args("lookup-image-map", &args, 3)?;
    // Stub: return nil
    Ok(Value::Nil)
}

/// (current-bidi-paragraph-direction &optional BUFFER) -> symbol
///
/// Get the bidi paragraph direction. Returns the symbol 'left-to-right.
pub(crate) fn builtin_current_bidi_paragraph_direction(args: Vec<Value>) -> EvalResult {
    expect_args_range("current-bidi-paragraph-direction", &args, 0, 1)?;
    // Return 'left-to-right
    Ok(Value::symbol("left-to-right"))
}

/// (move-to-window-line ARG) -> integer or nil
///
/// Move to a specific line in the window. Stub implementation
/// always returns nil.
pub(crate) fn builtin_move_to_window_line(args: Vec<Value>) -> EvalResult {
    expect_args("move-to-window-line", &args, 1)?;
    // Stub: return nil
    Ok(Value::Nil)
}

/// (tool-bar-height &optional FRAME PIXELWISE) -> integer
///
/// Get the height of the tool bar. Returns 0 (no tool bar).
pub(crate) fn builtin_tool_bar_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("tool-bar-height", &args, 0, 2)?;
    // Return 0 (no tool bar)
    Ok(Value::Int(0))
}

/// (tab-bar-height &optional FRAME PIXELWISE) -> integer
///
/// Get the height of the tab bar. Returns 0 (no tab bar).
pub(crate) fn builtin_tab_bar_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("tab-bar-height", &args, 0, 2)?;
    // Return 0 (no tab bar)
    Ok(Value::Int(0))
}

/// (line-number-display-width &optional ON-DISPLAY) -> integer
///
/// Get the width of the line number display. Returns 0 (no line numbers).
pub(crate) fn builtin_line_number_display_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("line-number-display-width", &args, 0, 1)?;
    // Return 0 (no line numbers)
    Ok(Value::Int(0))
}

/// (long-line-optimizations-p) -> boolean
///
/// Check if long-line optimizations are enabled. Returns nil.
pub(crate) fn builtin_long_line_optimizations_p(args: Vec<Value>) -> EvalResult {
    expect_args("long-line-optimizations-p", &args, 0)?;
    // Return nil (optimizations not enabled)
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_mode_line() {
        let result = builtin_format_mode_line(vec![]).unwrap();
        assert_eq!(result, Value::string(""));

        let result =
            builtin_format_mode_line(vec![Value::string("test"), Value::symbol("default")])
                .unwrap();
        assert_eq!(result, Value::string(""));
    }

    #[test]
    fn test_invisible_p() {
        let result = builtin_invisible_p(vec![Value::Int(1)]).unwrap();
        assert!(result.is_nil());

        let result = builtin_invisible_p(vec![Value::symbol("invisible")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_line_pixel_height() {
        let result = builtin_line_pixel_height(vec![]).unwrap();
        assert_eq!(result, Value::Int(16));
    }

    #[test]
    fn test_window_text_pixel_size() {
        let result = builtin_window_text_pixel_size(vec![]).unwrap();
        if let Value::Cons(cell) = result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car, Value::Int(100));
            assert_eq!(pair.cdr, Value::Int(16));
        } else {
            panic!("expected cons");
        }
    }

    #[test]
    fn test_pos_visible_in_window_p() {
        let result = builtin_pos_visible_in_window_p(vec![]).unwrap();
        assert!(result.is_truthy());

        let result =
            builtin_pos_visible_in_window_p(vec![Value::Int(100), Value::symbol("window")])
                .unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn test_move_point_visually() {
        let result = builtin_move_point_visually(vec![Value::Int(1)]).unwrap();
        assert!(result.is_nil());

        let result = builtin_move_point_visually(vec![Value::Int(-1)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_lookup_image_map() {
        let result =
            builtin_lookup_image_map(vec![Value::symbol("map"), Value::Int(10), Value::Int(20)])
                .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_current_bidi_paragraph_direction() {
        let result = builtin_current_bidi_paragraph_direction(vec![]).unwrap();
        assert_eq!(result, Value::symbol("left-to-right"));

        let result =
            builtin_current_bidi_paragraph_direction(vec![Value::symbol("buffer")]).unwrap();
        assert_eq!(result, Value::symbol("left-to-right"));
    }

    #[test]
    fn test_move_to_window_line() {
        let result = builtin_move_to_window_line(vec![Value::Int(0)]).unwrap();
        assert!(result.is_nil());

        let result = builtin_move_to_window_line(vec![Value::Int(5)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_tool_bar_height() {
        let result = builtin_tool_bar_height(vec![]).unwrap();
        assert_eq!(result, Value::Int(0));

        let result = builtin_tool_bar_height(vec![Value::symbol("frame")]).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_tab_bar_height() {
        let result = builtin_tab_bar_height(vec![]).unwrap();
        assert_eq!(result, Value::Int(0));

        let result = builtin_tab_bar_height(vec![Value::symbol("frame")]).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_line_number_display_width() {
        let result = builtin_line_number_display_width(vec![]).unwrap();
        assert_eq!(result, Value::Int(0));

        let result = builtin_line_number_display_width(vec![Value::True]).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_long_line_optimizations_p() {
        let result = builtin_long_line_optimizations_p(vec![]).unwrap();
        assert!(result.is_nil());
    }

    // Test wrong arity errors
    #[test]
    fn test_wrong_arity() {
        assert!(builtin_line_pixel_height(vec![Value::Int(1)]).is_err());
        assert!(builtin_invisible_p(vec![]).is_err());
        assert!(builtin_move_point_visually(vec![]).is_err());
        assert!(builtin_lookup_image_map(vec![Value::Int(1), Value::Int(2)]).is_err());
        assert!(builtin_move_to_window_line(vec![]).is_err());
    }

    // Test optional args
    #[test]
    fn test_optional_args() {
        // format-mode-line allows 0-4 args
        assert!(builtin_format_mode_line(vec![]).is_ok());
        assert!(builtin_format_mode_line(vec![Value::string("fmt")]).is_ok());
        assert!(builtin_format_mode_line(vec![
            Value::string("fmt"),
            Value::symbol("face"),
            Value::symbol("window"),
            Value::symbol("buffer"),
        ])
        .is_ok());

        // window-text-pixel-size allows 0-6 args
        assert!(builtin_window_text_pixel_size(vec![]).is_ok());
        assert!(builtin_window_text_pixel_size(vec![
            Value::symbol("window"),
            Value::Int(1),
            Value::Int(100),
            Value::Int(500),
            Value::Int(300),
            Value::symbol("mode"),
        ])
        .is_ok());
    }
}
