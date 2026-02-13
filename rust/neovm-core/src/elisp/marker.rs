//! Marker builtins for the Elisp interpreter.
//!
//! Markers track positions in buffers and adjust when text is inserted or
//! deleted before them.  They are represented as tagged vectors:
//!
//! ```text
//! [":marker"  buffer-name-or-nil  position-or-nil  insertion-type]
//! ```
//!
//! Pure builtins:
//!   `markerp`, `marker-position`, `marker-buffer`,
//!   `marker-insertion-type`, `set-marker-insertion-type`,
//!   `copy-marker`, `make-marker`
//!
//! Eval-dependent builtins:
//!   `set-marker`, `point-marker`, `point-min-marker`,
//!   `point-max-marker`, `mark-marker`

use super::error::{signal, EvalResult, Flow};
use super::value::*;

// ---------------------------------------------------------------------------
// Marker struct (for documentation / internal helpers)
// ---------------------------------------------------------------------------

/// Logical representation of a marker.  Not stored directly in the value
/// system; instead encoded as a tagged `Value::Vector`.
#[allow(dead_code)]
pub(crate) struct Marker {
    pub buffer_name: Option<String>,
    pub position: Option<i64>,
    pub insertion_type: bool, // true = advances when text inserted at marker pos
}

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
// Marker value helpers
// ---------------------------------------------------------------------------

/// The tag keyword used to identify marker vectors.
const MARKER_TAG: &str = ":marker";

/// Check whether `v` is a marker (a 4-element vector whose first element is
/// the keyword `:marker`).
pub(crate) fn is_marker(v: &Value) -> bool {
    match v {
        Value::Vector(vec) => {
            let elems = vec.lock().expect("poisoned");
            elems.len() == 4 && matches!(&elems[0], Value::Keyword(k) if k == MARKER_TAG)
        }
        _ => false,
    }
}

/// Construct a marker `Value` from its logical components.
///
/// - `buffer_name`: `Some(name)` or `None` (stored as `Value::Str` / `Value::Nil`)
/// - `position`: `Some(pos)` or `None` (stored as `Value::Int` / `Value::Nil`)
/// - `insertion_type`: stored as `Value::True` / `Value::Nil`
pub(crate) fn make_marker_value(
    buffer_name: Option<&str>,
    position: Option<i64>,
    insertion_type: bool,
) -> Value {
    Value::vector(vec![
        Value::Keyword(MARKER_TAG.to_string()),
        match buffer_name {
            Some(name) => Value::string(name),
            None => Value::Nil,
        },
        match position {
            Some(pos) => Value::Int(pos),
            None => Value::Nil,
        },
        Value::bool(insertion_type),
    ])
}

/// Assert that a value is a marker and return a wrong-type-argument error if
/// it is not.
fn expect_marker(_name: &str, v: &Value) -> Result<(), Flow> {
    if is_marker(v) {
        Ok(())
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("markerp"), v.clone()],
        ))
    }
}

/// Read the position field from a marker vector (index 2).
fn marker_position_value(v: &Value) -> Value {
    match v {
        Value::Vector(vec) => {
            let elems = vec.lock().expect("poisoned");
            elems[2].clone()
        }
        _ => Value::Nil,
    }
}

/// Return marker position as an integer.
///
/// Signals `error` when marker is unset, matching Emacs behavior in position
/// contexts that require a concrete marker location.
pub(crate) fn marker_position_as_int(v: &Value) -> Result<i64, Flow> {
    expect_marker("marker-position", v)?;
    match marker_position_value(v) {
        Value::Int(n) => Ok(n),
        _ => Err(signal(
            "error",
            vec![Value::string("Marker does not point anywhere")],
        )),
    }
}

/// Read the buffer-name field from a marker vector (index 1).
fn marker_buffer_value(v: &Value) -> Value {
    match v {
        Value::Vector(vec) => {
            let elems = vec.lock().expect("poisoned");
            elems[1].clone()
        }
        _ => Value::Nil,
    }
}

/// Read the insertion-type field from a marker vector (index 3).
fn marker_insertion_type_value(v: &Value) -> Value {
    match v {
        Value::Vector(vec) => {
            let elems = vec.lock().expect("poisoned");
            elems[3].clone()
        }
        _ => Value::Nil,
    }
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (markerp OBJECT) -> t if OBJECT is a marker, nil otherwise
pub(crate) fn builtin_markerp(args: Vec<Value>) -> EvalResult {
    expect_args("markerp", &args, 1)?;
    Ok(Value::bool(is_marker(&args[0])))
}

/// (marker-position MARKER) -> integer position or nil if marker is not set
pub(crate) fn builtin_marker_position(args: Vec<Value>) -> EvalResult {
    expect_args("marker-position", &args, 1)?;
    expect_marker("marker-position", &args[0])?;
    Ok(marker_position_value(&args[0]))
}

/// (marker-buffer MARKER) -> buffer name (string) or nil
pub(crate) fn builtin_marker_buffer(args: Vec<Value>) -> EvalResult {
    expect_args("marker-buffer", &args, 1)?;
    expect_marker("marker-buffer", &args[0])?;
    Ok(marker_buffer_value(&args[0]))
}

/// (marker-insertion-type MARKER) -> t or nil
pub(crate) fn builtin_marker_insertion_type(args: Vec<Value>) -> EvalResult {
    expect_args("marker-insertion-type", &args, 1)?;
    expect_marker("marker-insertion-type", &args[0])?;
    Ok(marker_insertion_type_value(&args[0]))
}

/// (set-marker-insertion-type MARKER TYPE) -> TYPE
pub(crate) fn builtin_set_marker_insertion_type(args: Vec<Value>) -> EvalResult {
    expect_args("set-marker-insertion-type", &args, 2)?;
    expect_marker("set-marker-insertion-type", &args[0])?;
    let new_type = args[1].is_truthy();
    match &args[0] {
        Value::Vector(vec) => {
            let mut elems = vec.lock().expect("poisoned");
            elems[3] = Value::bool(new_type);
        }
        _ => unreachable!(), // guarded by expect_marker
    }
    Ok(args[1].clone())
}

/// (copy-marker MARKER-OR-INTEGER &optional TYPE) -> new marker
///
/// If MARKER-OR-INTEGER is a marker, copy its position (and buffer).
/// If it is an integer, create a marker with that position and no buffer.
/// TYPE, if non-nil, sets the insertion type of the new marker.
pub(crate) fn builtin_copy_marker(args: Vec<Value>) -> EvalResult {
    expect_range_args("copy-marker", &args, 1, 2)?;
    let insertion_type = if args.len() > 1 {
        args[1].is_truthy()
    } else {
        false
    };

    match &args[0] {
        v if is_marker(v) => {
            let buf = marker_buffer_value(v);
            let pos = marker_position_value(v);
            let buffer_name = match &buf {
                Value::Str(s) => Some(s.as_str()),
                _ => None,
            };
            let position = match &pos {
                Value::Int(n) => Some(*n),
                _ => None,
            };
            Ok(make_marker_value(buffer_name, position, insertion_type))
        }
        Value::Int(n) => Ok(make_marker_value(None, Some(*n), insertion_type)),
        Value::Nil => {
            // nil means no position — return an unset marker
            Ok(make_marker_value(None, None, insertion_type))
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integer-or-marker-p"), other.clone()],
        )),
    }
}

/// (make-marker) -> new empty marker (no buffer, no position)
pub(crate) fn builtin_make_marker(args: Vec<Value>) -> EvalResult {
    expect_args("make-marker", &args, 0)?;
    Ok(make_marker_value(None, None, false))
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// (set-marker MARKER POSITION &optional BUFFER) -> MARKER
///
/// Set the position and (optionally) the buffer of MARKER.  If POSITION is
/// nil, the marker is unset (points nowhere).  BUFFER defaults to the current
/// buffer.
pub(crate) fn builtin_set_marker(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_range_args("set-marker", &args, 2, 3)?;
    expect_marker("set-marker", &args[0])?;

    // Resolve buffer name
    let buffer_name: Option<String> = if args.len() > 2 && args[2].is_truthy() {
        match &args[2] {
            Value::Str(s) => Some((**s).clone()),
            Value::Buffer(id) => eval.buffers.get(*id).map(|b| b.name.clone()),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other.clone()],
                ));
            }
        }
    } else {
        // Default to current buffer
        eval.buffers.current_buffer().map(|b| b.name.clone())
    };

    // Resolve position
    let position: Option<i64> = match &args[1] {
        Value::Nil => None,
        Value::Int(n) => Some(*n),
        v if is_marker(v) => match marker_position_value(v) {
            Value::Int(n) => Some(n),
            _ => None,
        },
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integer-or-marker-p"), other.clone()],
            ));
        }
    };

    // Mutate the marker vector in place
    match &args[0] {
        Value::Vector(vec) => {
            let mut elems = vec.lock().expect("poisoned");
            elems[1] = match &buffer_name {
                Some(name) => Value::string(name.as_str()),
                None => Value::Nil,
            };
            elems[2] = match position {
                Some(pos) => Value::Int(pos),
                None => Value::Nil,
            };
        }
        _ => unreachable!(), // guarded by expect_marker
    }

    Ok(args[0].clone())
}

/// (point-marker) -> marker at current point
pub(crate) fn builtin_point_marker(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let pos = buf.point_char() as i64 + 1; // 1-based
    let name = buf.name.clone();
    Ok(make_marker_value(Some(&name), Some(pos), false))
}

/// (point-min-marker) -> marker at point-min
pub(crate) fn builtin_point_min_marker(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let pos = buf.text.byte_to_char(buf.point_min()) as i64 + 1; // 1-based
    let name = buf.name.clone();
    Ok(make_marker_value(Some(&name), Some(pos), false))
}

/// (point-max-marker) -> marker at point-max
pub(crate) fn builtin_point_max_marker(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let pos = buf.text.byte_to_char(buf.point_max()) as i64 + 1; // 1-based
    let name = buf.name.clone();
    Ok(make_marker_value(Some(&name), Some(pos), false))
}

/// (mark-marker) -> marker at mark, or error if no mark set
pub(crate) fn builtin_mark_marker(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let name = buf.name.clone();
    match buf.mark() {
        Some(byte_pos) => {
            let pos = buf.text.byte_to_char(byte_pos) as i64 + 1; // 1-based
            Ok(make_marker_value(Some(&name), Some(pos), false))
        }
        None => {
            // Return a marker with no position (mark not set)
            Ok(make_marker_value(Some(&name), None, false))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_marker_creates_tagged_vector() {
        let m = make_marker_value(Some("*scratch*"), Some(42), false);
        assert!(is_marker(&m));
    }

    #[test]
    fn make_marker_empty() {
        let m = make_marker_value(None, None, false);
        assert!(is_marker(&m));
        assert!(marker_position_value(&m).is_nil());
        assert!(marker_buffer_value(&m).is_nil());
    }

    #[test]
    fn is_marker_rejects_non_markers() {
        assert!(!is_marker(&Value::Nil));
        assert!(!is_marker(&Value::Int(42)));
        assert!(!is_marker(&Value::vector(vec![Value::Int(1)])));
        // Wrong tag
        assert!(!is_marker(&Value::vector(vec![
            Value::Keyword(":not-marker".to_string()),
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ])));
    }

    #[test]
    fn builtin_markerp_works() {
        let m = make_marker_value(None, None, false);
        assert!(builtin_markerp(vec![m]).unwrap().is_truthy());
        assert!(builtin_markerp(vec![Value::Int(5)]).unwrap().is_nil());
    }

    #[test]
    fn builtin_marker_position_returns_position() {
        let m = make_marker_value(Some("buf"), Some(10), false);
        let pos = builtin_marker_position(vec![m]).unwrap();
        assert!(matches!(pos, Value::Int(10)));
    }

    #[test]
    fn builtin_marker_position_returns_nil_when_unset() {
        let m = make_marker_value(None, None, false);
        let pos = builtin_marker_position(vec![m]).unwrap();
        assert!(pos.is_nil());
    }

    #[test]
    fn builtin_marker_buffer_returns_name() {
        let m = make_marker_value(Some("*scratch*"), Some(1), false);
        let buf = builtin_marker_buffer(vec![m]).unwrap();
        assert_eq!(buf.as_str(), Some("*scratch*"));
    }

    #[test]
    fn builtin_marker_insertion_type_roundtrip() {
        let m = make_marker_value(None, None, false);
        assert!(builtin_marker_insertion_type(vec![m.clone()])
            .unwrap()
            .is_nil());

        builtin_set_marker_insertion_type(vec![m.clone(), Value::True]).unwrap();
        assert!(builtin_marker_insertion_type(vec![m]).unwrap().is_truthy());
    }

    #[test]
    fn builtin_copy_marker_from_marker() {
        let m = make_marker_value(Some("buf"), Some(5), true);
        let copy = builtin_copy_marker(vec![m]).unwrap();
        assert!(is_marker(&copy));
        assert!(matches!(marker_position_value(&copy), Value::Int(5)));
    }

    #[test]
    fn builtin_copy_marker_from_integer() {
        let copy = builtin_copy_marker(vec![Value::Int(99)]).unwrap();
        assert!(is_marker(&copy));
        assert!(matches!(marker_position_value(&copy), Value::Int(99)));
        assert!(marker_buffer_value(&copy).is_nil());
    }

    #[test]
    fn builtin_make_marker_returns_empty() {
        let m = builtin_make_marker(vec![]).unwrap();
        assert!(is_marker(&m));
        assert!(marker_position_value(&m).is_nil());
        assert!(marker_buffer_value(&m).is_nil());
        assert!(marker_insertion_type_value(&m).is_nil());
    }

    #[test]
    fn wrong_type_signals_error() {
        let result = builtin_marker_position(vec![Value::Int(5)]);
        assert!(result.is_err());
    }
}
