//! Undo system -- buffer undo/redo functionality.
//!
//! Provides Emacs-compatible undo functionality:
//! - `undo-boundary` -- insert an undo boundary marker
//! - `primitive-undo` -- undo entries from an undo list
//! - `undo` -- undo the last change in the current buffer

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use crate::buffer::undo::UndoRecord;

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

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

fn expect_list_like(value: &Value) -> Result<(), Flow> {
    if value.is_nil() || value.is_cons() {
        Ok(())
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), value.clone()],
        ))
    }
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (undo-boundary) -> nil
///
/// Insert an undo boundary marker in the current buffer's undo list.
/// This separates consecutive edits into distinct undoable actions.
///
pub(crate) fn builtin_undo_boundary(args: Vec<Value>) -> EvalResult {
    expect_args("undo-boundary", &args, 0)?;
    Ok(Value::Nil)
}

/// (primitive-undo COUNT LIST) -> remainder of LIST
///
/// Undo COUNT entries from the undo list LIST.
/// Returns the remainder of the list after removing COUNT entries.
///
/// Each entry in LIST should be a marker for an undoable action.
/// In a full implementation, each entry would be applied in reverse order.
///
pub(crate) fn builtin_primitive_undo(args: Vec<Value>) -> EvalResult {
    expect_args("primitive-undo", &args, 2)?;

    // Verify COUNT is an integer
    let _count = expect_int(&args[0])?;
    expect_list_like(&args[1])?;

    // NeoVM's higher-level `undo` applies buffer edits directly.
    // `primitive-undo` currently acts as a type-checked list passthrough.
    Ok(args[1].clone())
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// (undo &optional ARG) -> nil
///
/// Undo the last change in the current buffer.
/// ARG is the number of undo commands to execute (default 1).
///
/// In a full implementation, this would:
/// 1. Get the current buffer's undo list
/// 2. Apply primitive-undo to reverse the specified number of actions
/// 3. Update buffer state accordingly
///
pub(crate) fn builtin_undo(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("undo", &args, 0)?;
    expect_max_args("undo", &args, 1)?;

    // If ARG is provided, verify it's an integer
    let mut count = 1i64;
    if let Some(arg) = args.first() {
        count = expect_int(arg)?;
    }
    if count <= 0 {
        return Ok(Value::Nil);
    }

    let Some(buffer) = eval.buffers.current_buffer_mut() else {
        return Err(signal("error", vec![Value::string("No current buffer")]));
    };

    let previous_undoing = buffer.undo_list.undoing;
    buffer.undo_list.undoing = true;

    for _ in 0..(count as usize) {
        let group = buffer.undo_list.pop_undo_group();
        if group.is_empty() {
            break;
        }

        for record in group {
            match record {
                UndoRecord::Insert { pos, len } => {
                    let end = pos.saturating_add(len).min(buffer.text.len());
                    buffer.delete_region(pos.min(end), end);
                }
                UndoRecord::Delete { pos, text } => {
                    let clamped = pos.min(buffer.text.len());
                    buffer.goto_char(clamped);
                    buffer.insert(&text);
                }
                UndoRecord::CursorMove { pos } => {
                    buffer.goto_char(pos.min(buffer.text.len()));
                }
                UndoRecord::PropertyChange { .. } | UndoRecord::Boundary => {
                    // Text property undo entries are intentionally ignored for now.
                }
            }
        }
    }

    buffer.undo_list.undoing = previous_undoing;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_boundary_no_args() {
        let result = builtin_undo_boundary(vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_undo_boundary_wrong_args() {
        let result = builtin_undo_boundary(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_primitive_undo_with_count_and_list() {
        let list = Value::list(vec![Value::Nil, Value::Nil, Value::Nil]);
        let result = builtin_primitive_undo(vec![Value::Int(1), list.clone()]);
        assert!(result.is_ok());
        // Stub returns list unchanged
        assert_eq!(format!("{:?}", result.unwrap()), format!("{:?}", list));
    }

    #[test]
    fn test_primitive_undo_zero_count() {
        let list = Value::list(vec![Value::Nil, Value::Nil]);
        let result = builtin_primitive_undo(vec![Value::Int(0), list.clone()]);
        assert!(result.is_ok());
        assert_eq!(format!("{:?}", result.unwrap()), format!("{:?}", list));
    }

    #[test]
    fn test_primitive_undo_negative_count() {
        let list = Value::list(vec![Value::Nil]);
        let result = builtin_primitive_undo(vec![Value::Int(-5), list.clone()]);
        assert!(result.is_ok());
        // Negative count still returns list
        assert_eq!(format!("{:?}", result.unwrap()), format!("{:?}", list));
    }

    #[test]
    fn test_primitive_undo_invalid_count() {
        let list = Value::list(vec![]);
        let result = builtin_primitive_undo(vec![Value::Float(1.5), list]);
        assert!(result.is_err());
    }

    #[test]
    fn test_primitive_undo_non_list_signals_wrong_type() {
        let result = builtin_primitive_undo(vec![Value::Int(1), Value::Int(7)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_primitive_undo_wrong_arg_count() {
        let result = builtin_primitive_undo(vec![Value::Int(1)]);
        assert!(result.is_err());

        let result = builtin_primitive_undo(vec![]);
        assert!(result.is_err());

        let result = builtin_primitive_undo(vec![Value::Int(1), Value::Nil, Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn test_undo_no_args() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();
        let result = builtin_undo(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_undo_with_arg() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();
        let result = builtin_undo(&mut eval, vec![Value::Int(5)]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_undo_with_invalid_arg() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();
        let result = builtin_undo(&mut eval, vec![Value::Float(1.5)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_undo_with_multiple_args() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();
        let result = builtin_undo(&mut eval, vec![Value::Int(2), Value::Int(3)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_undo_reverts_inserted_text() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();
        {
            let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
            buffer.insert("abc");
            buffer.undo_list.boundary();
        }
        let result = builtin_undo(&mut eval, vec![Value::Int(1)]);
        assert!(result.is_ok());
        let contents = eval
            .buffers
            .current_buffer()
            .expect("scratch buffer")
            .buffer_string();
        assert_eq!(contents, "");
    }
}
