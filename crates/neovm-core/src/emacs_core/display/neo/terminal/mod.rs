//! Lisp bridge for compositor-owned neo-term terminal instances.
//!
//! The evaluator validates Lisp values and sends typed requests through
//! [`DisplayHost`]. PTY ownership, VT parsing, and rendering remain entirely
//! behind the display-runtime boundary.

mod subrs;

#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

use crate::emacs_core::display_host::{
    DisplayHost, TerminalCreateRequest, TerminalDisplayTarget, TerminalFloatPlacement,
    TerminalGridSize, TerminalId,
};
use crate::emacs_core::error::{EvalResult, Flow, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalOperation {
    Create,
    Write,
    Resize,
    Destroy,
    SetFloat,
    GetText,
}

impl Display for TerminalOperation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Create => "neomacs-terminal-create",
            Self::Write => "neomacs-terminal-write",
            Self::Resize => "neomacs-terminal-resize",
            Self::Destroy => "neomacs-terminal-destroy",
            Self::SetFloat => "neomacs-terminal-set-float",
            Self::GetText => "neomacs-terminal-get-text",
        })
    }
}

fn terminal_error(message: impl Into<String>) -> Flow {
    signal("error", vec![Value::string(message.into())])
}

fn wrong_type(predicate: &str, value: Value) -> Flow {
    signal("wrong-type-argument", vec![Value::symbol(predicate), value])
}

fn positive_u16(
    value: Value,
    operation: TerminalOperation,
    argument: &str,
) -> Result<NonZeroU16, Flow> {
    let integer = value.as_int().ok_or_else(|| wrong_type("fixnump", value))?;
    u16::try_from(integer)
        .ok()
        .and_then(NonZeroU16::new)
        .ok_or_else(|| terminal_error(format!("{operation}: {argument} must be in 1..=65535")))
}

fn terminal_id(value: Value, operation: TerminalOperation) -> Result<TerminalId, Flow> {
    let integer = value.as_int().ok_or_else(|| wrong_type("fixnump", value))?;
    u32::try_from(integer)
        .ok()
        .and_then(TerminalId::new)
        .ok_or_else(|| terminal_error(format!("{operation}: terminal id must be positive")))
}

fn number(value: Value, operation: TerminalOperation, argument: &str) -> Result<f32, Flow> {
    let number = value
        .as_int()
        .map(|value| value as f32)
        .or_else(|| value.as_float().map(|value| value as f32))
        .ok_or_else(|| wrong_type("numberp", value))?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(terminal_error(format!(
            "{operation}: {argument} must be finite"
        )))
    }
}

fn display_host(eval: &Context, operation: TerminalOperation) -> Result<&dyn DisplayHost, Flow> {
    eval.display_host
        .as_deref()
        .ok_or_else(|| terminal_error(format!("{operation}: no GUI display host in this session")))
}

/// `(neomacs-terminal-create COLS ROWS MODE &optional SHELL)`.
fn create(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    const OPERATION: TerminalOperation = TerminalOperation::Create;
    let cols = positive_u16(args[0], OPERATION, "COLS")?;
    let rows = positive_u16(args[1], OPERATION, "ROWS")?;
    let target = match args[2]
        .as_int()
        .ok_or_else(|| wrong_type("fixnump", args[2]))?
    {
        0 => TerminalDisplayTarget::Window {
            buffer: eval.buffers.current_buffer_id().ok_or_else(|| {
                terminal_error(format!(
                    "{OPERATION}: no current buffer for window terminal"
                ))
            })?,
        },
        1 => TerminalDisplayTarget::Inline,
        2 => TerminalDisplayTarget::Floating,
        _ => {
            return Err(terminal_error(format!(
                "{OPERATION}: MODE must be 0, 1, or 2"
            )));
        }
    };
    let shell = match args.get(3).copied().unwrap_or(Value::NIL) {
        value if value.is_nil() => None,
        value => Some(
            value
                .as_lisp_string()
                .ok_or_else(|| wrong_type("stringp", value))?
                .as_utf8_str()
                .ok_or_else(|| terminal_error(format!("{OPERATION}: SHELL must be UTF-8")))?
                .to_owned(),
        ),
    };
    let id = display_host(eval, OPERATION)?
        .create_terminal(TerminalCreateRequest {
            size: TerminalGridSize { cols, rows },
            target,
            shell,
        })
        .map_err(terminal_error)?;
    Ok(Value::fixnum(i64::from(id.get())))
}

/// `(neomacs-terminal-write TERMINAL-ID STRING)`.
fn write(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    const OPERATION: TerminalOperation = TerminalOperation::Write;
    let id = terminal_id(args[0], OPERATION)?;
    let data = args[1]
        .as_lisp_string()
        .ok_or_else(|| wrong_type("stringp", args[1]))?
        .as_bytes()
        .to_vec();
    display_host(eval, OPERATION)?
        .write_terminal(id, data)
        .map_err(terminal_error)?;
    Ok(Value::T)
}

/// `(neomacs-terminal-resize TERMINAL-ID COLS ROWS)`.
fn resize(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    const OPERATION: TerminalOperation = TerminalOperation::Resize;
    let id = terminal_id(args[0], OPERATION)?;
    let size = TerminalGridSize {
        cols: positive_u16(args[1], OPERATION, "COLS")?,
        rows: positive_u16(args[2], OPERATION, "ROWS")?,
    };
    display_host(eval, OPERATION)?
        .resize_terminal(id, size)
        .map_err(terminal_error)?;
    Ok(Value::T)
}

/// `(neomacs-terminal-destroy TERMINAL-ID)`.
fn destroy(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    const OPERATION: TerminalOperation = TerminalOperation::Destroy;
    let id = terminal_id(args[0], OPERATION)?;
    display_host(eval, OPERATION)?
        .destroy_terminal(id)
        .map_err(terminal_error)?;
    Ok(Value::T)
}

/// `(neomacs-terminal-set-float TERMINAL-ID X Y OPACITY)`.
fn set_float(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    const OPERATION: TerminalOperation = TerminalOperation::SetFloat;
    let id = terminal_id(args[0], OPERATION)?;
    let x = number(args[1], OPERATION, "X")?;
    let y = number(args[2], OPERATION, "Y")?;
    let opacity = number(args[3], OPERATION, "OPACITY")?;
    let placement = TerminalFloatPlacement::new(x, y, opacity)
        .ok_or_else(|| terminal_error(format!("{OPERATION}: OPACITY must be in 0.0..=1.0")))?;
    display_host(eval, OPERATION)?
        .set_floating_terminal(id, placement)
        .map_err(terminal_error)?;
    Ok(Value::T)
}

/// `(neomacs-terminal-get-text TERMINAL-ID)`.
fn get_text(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    const OPERATION: TerminalOperation = TerminalOperation::GetText;
    let id = terminal_id(args[0], OPERATION)?;
    Ok(display_host(eval, OPERATION)?
        .terminal_text(id)
        .map_err(terminal_error)?
        .map(Value::string)
        .unwrap_or(Value::NIL))
}
