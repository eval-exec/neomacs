//! Indentation builtins for the Elisp interpreter.
//!
//! Implements stub versions of Emacs indentation primitives:
//! - `current-indentation`, `indent-to`, `current-column`, `move-to-column`
//! - `indent-region`, `indent-line-to`, `indent-rigidly`, `newline-and-indent`,
//!   `reindent-then-newline-and-indent`, `indent-for-tab-command`,
//!   `indent-according-to-mode`, `tab-to-tab-stop`, `back-to-indentation`,
//!   `delete-indentation`
//!
//! Variables: `tab-width`, `indent-tabs-mode`, `standard-indent`, `tab-stop-list`

use super::error::{signal, EvalResult, Flow};
use super::value::*;

// ---------------------------------------------------------------------------
// Argument helpers (local to this module)
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

fn expect_int(val: &Value) -> Result<i64, Flow> {
    match val {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

fn expect_wholenump(val: &Value) -> Result<usize, Flow> {
    match val {
        Value::Int(n) if *n >= 0 => Ok(*n as usize),
        Value::Char(c) => Ok(*c as usize),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("wholenump"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (current-indentation) -> integer
///
/// Return the indentation of the current line (number of whitespace columns
/// at the beginning of the line).  Stub: always returns 0.
pub(crate) fn builtin_current_indentation(args: Vec<Value>) -> EvalResult {
    expect_args("current-indentation", &args, 0)?;
    Ok(Value::Int(0))
}

/// (indent-to COLUMN &optional MINIMUM) -> COLUMN
///
/// Indent from point with tabs and spaces until COLUMN is reached.
/// Optional second argument MINIMUM says always do at least MINIMUM spaces
/// even if that moves past COLUMN; default is zero.
/// Stub: returns COLUMN unchanged.
pub(crate) fn builtin_indent_to(args: Vec<Value>) -> EvalResult {
    expect_min_args("indent-to", &args, 1)?;
    expect_max_args("indent-to", &args, 2)?;
    let column = expect_int(&args[0])?;
    // Optional MINIMUM argument is accepted but ignored in this stub.
    Ok(Value::Int(column))
}

/// (current-column) -> integer
///
/// Return the horizontal position of point.  Beginning of line is column 0.
/// Stub: always returns 0.
pub(crate) fn builtin_current_column(args: Vec<Value>) -> EvalResult {
    expect_args("current-column", &args, 0)?;
    Ok(Value::Int(0))
}

/// (move-to-column COLUMN &optional FORCE) -> COLUMN
///
/// Move point to column COLUMN in the current line.
/// Stub: returns COLUMN unchanged.
pub(crate) fn builtin_move_to_column(args: Vec<Value>) -> EvalResult {
    expect_min_args("move-to-column", &args, 1)?;
    expect_max_args("move-to-column", &args, 2)?;
    let column = expect_int(&args[0])?;
    // Optional FORCE argument is accepted but ignored in this stub.
    Ok(Value::Int(column))
}

fn tab_width(eval: &super::eval::Evaluator) -> usize {
    match eval.obarray.symbol_value("tab-width") {
        Some(Value::Int(n)) if *n > 0 => *n as usize,
        Some(Value::Char(c)) if (*c as u32) > 0 => *c as usize,
        _ => 8,
    }
}

fn line_bounds(text: &str, begv: usize, zv: usize, point: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let pt = point.clamp(begv, zv);

    let mut bol = pt;
    while bol > begv && bytes[bol - 1] != b'\n' {
        bol -= 1;
    }

    let mut eol = pt;
    while eol < zv && bytes[eol] != b'\n' {
        eol += 1;
    }

    (bol, eol)
}

fn next_column(column: usize, ch: char, tab_width: usize) -> usize {
    if ch == '\t' {
        let tab = tab_width.max(1);
        column + (tab - (column % tab))
    } else {
        column + crate::encoding::char_width(ch)
    }
}

fn column_for_prefix(prefix: &str, tab_width: usize) -> usize {
    let mut column = 0usize;
    for ch in prefix.chars() {
        column = next_column(column, ch, tab_width);
    }
    column
}

fn padding_to_column(mut column: usize, target: usize, tab_width: usize) -> String {
    let mut out = String::new();
    let tab = tab_width.max(1);
    while column < target {
        let next_tab = column + (tab - (column % tab));
        if next_tab <= target && next_tab > column + 1 {
            out.push('\t');
            column = next_tab;
        } else {
            out.push(' ');
            column += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// (current-indentation) -> integer
///
/// Return indentation columns for the current line.
pub(crate) fn builtin_current_indentation_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-indentation", &args, 0)?;
    let Some(buf) = eval.buffers.current_buffer() else {
        return Ok(Value::Int(0));
    };

    let tabw = tab_width(eval);
    let text = buf.text.to_string();
    let (bol, eol) = line_bounds(&text, buf.begv, buf.zv, buf.pt);
    let line = &text[bol..eol];

    let mut column = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' | '\t' => column = next_column(column, ch, tabw),
            _ => break,
        }
    }

    Ok(Value::Int(column as i64))
}

/// (current-column) -> integer
///
/// Return the display column at point on the current line.
pub(crate) fn builtin_current_column_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-column", &args, 0)?;
    let Some(buf) = eval.buffers.current_buffer() else {
        return Ok(Value::Int(0));
    };

    let tabw = tab_width(eval);
    let text = buf.text.to_string();
    let pt = buf.pt.clamp(buf.begv, buf.zv);
    let (bol, _) = line_bounds(&text, buf.begv, buf.zv, pt);
    let prefix = &text[bol..pt];

    Ok(Value::Int(column_for_prefix(prefix, tabw) as i64))
}

/// (move-to-column COLUMN &optional FORCE) -> COLUMN-REACHED
///
/// Move point on the current line according to display columns.
pub(crate) fn builtin_move_to_column_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("move-to-column", &args, 1)?;
    expect_max_args("move-to-column", &args, 2)?;
    let target = expect_wholenump(&args[0])?;
    let force = args.get(1).is_some_and(|v| v.is_truthy());
    let tabw = tab_width(eval);

    let Some(buf) = eval.buffers.current_buffer_mut() else {
        return Ok(Value::Int(0));
    };

    let text = buf.text.to_string();
    let pt = buf.pt.clamp(buf.begv, buf.zv);
    let (bol, eol) = line_bounds(&text, buf.begv, buf.zv, pt);
    let line = &text[bol..eol];

    if target == 0 {
        buf.goto_char(bol);
        return Ok(Value::Int(0));
    }

    let mut column = 0usize;
    let mut dest_byte = bol;
    let mut reached = 0usize;
    let mut found = false;
    let mut tab_split: Option<(usize, usize)> = None;

    for (rel, ch) in line.char_indices() {
        let char_start = bol + rel;
        let char_end = char_start + ch.len_utf8();
        let next = next_column(column, ch, tabw);
        if next >= target {
            if force && ch == '\t' && next > target {
                tab_split = Some((char_start, column));
            } else {
                dest_byte = char_end;
                reached = next;
            }
            found = true;
            break;
        }
        dest_byte = char_end;
        reached = next;
        column = next;
    }

    if !found {
        dest_byte = eol;
        reached = column_for_prefix(line, tabw);
    }

    if let Some((tab_byte, col_before_tab)) = tab_split {
        if buf.read_only {
            return Err(signal(
                "buffer-read-only",
                vec![Value::string(buf.name.clone())],
            ));
        }
        buf.goto_char(tab_byte);
        let pad = padding_to_column(col_before_tab, target, tabw);
        buf.insert(&pad);
        return Ok(Value::Int(target as i64));
    }

    buf.goto_char(dest_byte);

    if force && reached < target {
        if buf.read_only {
            return Err(signal(
                "buffer-read-only",
                vec![Value::string(buf.name.clone())],
            ));
        }
        let pad = padding_to_column(reached, target, tabw);
        buf.insert(&pad);
        reached = target;
    }

    Ok(Value::Int(reached as i64))
}

/// (indent-region START END &optional COLUMN) -> nil
///
/// Indent each nonblank line in the region.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_indent_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("indent-region", &args, 2)?;
    expect_max_args("indent-region", &args, 3)?;
    let start = expect_int(&args[0])?;
    let end = expect_int(&args[1])?;
    let target_column = match args.get(2) {
        Some(Value::Int(n)) => Some((*n).max(0) as usize),
        Some(Value::Char(c)) => Some((*c as i64).max(0) as usize),
        _ => None,
    };

    let Some(buf) = eval.buffers.current_buffer() else {
        return Ok(Value::True);
    };
    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let point_min = buf.text.byte_to_char(buf.point_min()) as i64 + 1;
    let point_max = buf.text.byte_to_char(buf.point_max()) as i64 + 1;
    let a = start.clamp(point_min, point_max);
    let b = end.clamp(point_min, point_max);
    if a >= b {
        return Ok(Value::True);
    }
    let region_start = buf.text.char_to_byte((a - 1).max(0) as usize);
    let region_end = buf.text.char_to_byte((b - 1).max(0) as usize);

    let text = buf.text.to_string();
    let region = &text[region_start..region_end];

    let mut rewritten = String::with_capacity(
        region
            .len()
            .saturating_add(target_column.unwrap_or(0).saturating_mul(4)),
    );
    for segment in region.split_inclusive('\n') {
        let (line, has_newline) = match segment.strip_suffix('\n') {
            Some(prefix) => (prefix, true),
            None => (segment, false),
        };

        if line.chars().all(|ch| ch == ' ' || ch == '\t') {
            rewritten.push_str(line);
        } else {
            let trimmed = line.trim_start_matches([' ', '\t']);
            if let Some(column) = target_column {
                rewritten.push_str(&" ".repeat(column));
            }
            rewritten.push_str(trimmed);
        }

        if has_newline {
            rewritten.push('\n');
        }
    }

    let Some(buf) = eval.buffers.current_buffer_mut() else {
        return Ok(Value::Nil);
    };
    buf.delete_region(region_start, region_end);
    buf.goto_char(region_start);
    buf.insert(&rewritten);

    Ok(Value::True)
}

/// (indent-line-to COLUMN) -> nil
///
/// Indent current line to COLUMN.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_indent_line_to(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("indent-line-to", &args, 1)?;
    let _column = expect_int(&args[0])?;
    Ok(Value::Nil)
}

/// (indent-rigidly START END ARG &optional INTERACTIVE) -> nil
///
/// Indent all lines starting in the region by ARG columns.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_indent_rigidly(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("indent-rigidly", &args, 3)?;
    expect_max_args("indent-rigidly", &args, 4)?;
    let _start = expect_int(&args[0])?;
    let _end = expect_int(&args[1])?;
    let _arg = expect_int(&args[2])?;
    // Optional INTERACTIVE argument is accepted but ignored.
    Ok(Value::Nil)
}

/// (newline-and-indent) -> nil
///
/// Insert a newline, then indent according to major mode.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_newline_and_indent(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("newline-and-indent", &args, 0)?;
    Ok(Value::Nil)
}

/// (reindent-then-newline-and-indent) -> nil
///
/// Reindent current line, insert newline, then indent the new line.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_reindent_then_newline_and_indent(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("reindent-then-newline-and-indent", &args, 0)?;
    Ok(Value::Nil)
}

/// (indent-for-tab-command &optional ARG) -> nil
///
/// Indent the current line or region, or insert a tab, as appropriate.
/// Current behavior: insert a tab character at point.
pub(crate) fn builtin_indent_for_tab_command(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("indent-for-tab-command", &args, 1)?;
    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }
    buf.insert("\t");
    Ok(Value::Nil)
}

/// (indent-according-to-mode) -> nil
///
/// Indent line in proper way for current major mode.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_indent_according_to_mode(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("indent-according-to-mode", &args, 0)?;
    Ok(Value::Nil)
}

/// (tab-to-tab-stop) -> nil
///
/// Insert spaces or tabs to next defined tab-stop column.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_tab_to_tab_stop(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("tab-to-tab-stop", &args, 0)?;
    Ok(Value::Nil)
}

/// (back-to-indentation) -> nil
///
/// Move point to first non-space/tab on current line.
pub(crate) fn builtin_back_to_indentation(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("back-to-indentation", &args, 0)?;
    let Some(buf) = eval.buffers.current_buffer_mut() else {
        return Ok(Value::Nil);
    };

    let text = buf.text.to_string();
    let pt = buf.pt.clamp(buf.begv, buf.zv);
    let (bol, eol) = line_bounds(&text, buf.begv, buf.zv, pt);
    let line = &text[bol..eol];

    let mut dest = eol;
    for (rel, ch) in line.char_indices() {
        if ch != ' ' && ch != '\t' {
            dest = bol + rel;
            break;
        }
    }

    buf.goto_char(dest);
    Ok(Value::Nil)
}

/// (delete-indentation &optional ARG REGION) -> nil
///
/// Join this line to previous and fix up whitespace at join.
/// Stub: does nothing, returns nil.
pub(crate) fn builtin_delete_indentation(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("delete-indentation", &args, 2)?;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Variable initialisation
// ---------------------------------------------------------------------------

/// Pre-populate the obarray with standard indentation variables.
///
/// Must be called during evaluator initialisation (after the obarray is created
/// but before any user code runs).
pub fn init_indent_vars(obarray: &mut super::symbol::Obarray) {
    // tab-width: default 8 (buffer-local in real Emacs, global default here)
    let sym = obarray.get_or_intern("tab-width");
    sym.value = Some(Value::Int(8));
    sym.special = true;

    // indent-tabs-mode: default t
    let sym = obarray.get_or_intern("indent-tabs-mode");
    sym.value = Some(Value::True);
    sym.special = true;

    // standard-indent: default 4
    let sym = obarray.get_or_intern("standard-indent");
    sym.value = Some(Value::Int(4));
    sym.special = true;

    // tab-stop-list: default nil
    let sym = obarray.get_or_intern("tab-stop-list");
    sym.value = Some(Value::Nil);
    sym.special = true;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_indentation_returns_zero() {
        let result = builtin_current_indentation(vec![]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    #[test]
    fn indent_to_returns_column() {
        let result = builtin_indent_to(vec![Value::Int(42)]).unwrap();
        assert_eq!(result.as_int(), Some(42));
    }

    #[test]
    fn indent_to_with_minimum() {
        let result = builtin_indent_to(vec![Value::Int(10), Value::Int(4)]).unwrap();
        assert_eq!(result.as_int(), Some(10));
    }

    #[test]
    fn current_column_returns_zero() {
        let result = builtin_current_column(vec![]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    #[test]
    fn move_to_column_returns_column() {
        let result = builtin_move_to_column(vec![Value::Int(15)]).unwrap();
        assert_eq!(result.as_int(), Some(15));
    }

    #[test]
    fn move_to_column_with_force() {
        let result = builtin_move_to_column(vec![Value::Int(8), Value::True]).unwrap();
        assert_eq!(result.as_int(), Some(8));
    }

    #[test]
    fn eval_column_and_indentation_subset() {
        let mut ev = super::super::eval::Evaluator::new();
        let forms = super::super::parser::parse_forms(
            r#"
            (with-temp-buffer
              (insert "abc")
              (goto-char (+ (point-min) 2))
              (current-column))
            (with-temp-buffer
              (insert "  abc")
              (goto-char (point-max))
              (current-indentation))
            (with-temp-buffer
              (insert "a\tb")
              (goto-char (point-min))
              (move-to-column 5)
              (list (point) (current-column)))
            "#,
        )
        .expect("parse forms");

        let col = ev.eval(&forms[0]).expect("eval current-column");
        assert_eq!(col, Value::Int(2));

        let indent = ev.eval(&forms[1]).expect("eval current-indentation");
        assert_eq!(indent, Value::Int(2));

        let move_result = ev.eval(&forms[2]).expect("eval move-to-column");
        let items = list_to_vec(&move_result).expect("list result");
        assert_eq!(items, vec![Value::Int(3), Value::Int(8)]);
    }

    #[test]
    fn eval_move_to_column_wholenump_validation() {
        let mut ev = super::super::eval::Evaluator::new();
        let err = builtin_move_to_column_eval(&mut ev, vec![Value::string("x")]).unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("wholenump"), Value::string("x")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn eval_move_to_column_force_subset() {
        let mut ev = super::super::eval::Evaluator::new();
        let forms = super::super::parser::parse_forms(
            r#"
            (with-temp-buffer
              (insert "abc")
              (goto-char (point-min))
              (list (move-to-column 10 t) (point) (string-to-list (buffer-string))))
            (with-temp-buffer
              (insert "a\tb")
              (goto-char (point-min))
              (list (move-to-column 5 t) (point) (string-to-list (buffer-string))))
            "#,
        )
        .expect("parse forms");

        let first = ev.eval(&forms[0]).expect("eval first force case");
        let first_items = list_to_vec(&first).expect("first list");
        assert_eq!(first_items[0], Value::Int(10));
        assert_eq!(first_items[1], Value::Int(7));
        assert_eq!(
            list_to_vec(&first_items[2]).expect("first buffer bytes"),
            vec![
                Value::Int(97),
                Value::Int(98),
                Value::Int(99),
                Value::Int(9),
                Value::Int(32),
                Value::Int(32),
            ]
        );

        let second = ev.eval(&forms[1]).expect("eval second force case");
        let second_items = list_to_vec(&second).expect("second list");
        assert_eq!(second_items[0], Value::Int(5));
        assert_eq!(second_items[1], Value::Int(6));
        assert_eq!(
            list_to_vec(&second_items[2]).expect("second buffer bytes"),
            vec![
                Value::Int(97),
                Value::Int(32),
                Value::Int(32),
                Value::Int(32),
                Value::Int(32),
                Value::Int(9),
                Value::Int(98),
            ]
        );
    }

    #[test]
    fn eval_back_to_indentation_subset() {
        let mut ev = super::super::eval::Evaluator::new();
        let forms = super::super::parser::parse_forms(
            r#"
            (with-temp-buffer
              (insert "  abc")
              (goto-char (point-max))
              (back-to-indentation)
              (point))
            (with-temp-buffer
              (insert "   ")
              (goto-char (point-max))
              (back-to-indentation)
              (point))
            (with-temp-buffer
              (insert (string 9 97 98 99))
              (goto-char (point-max))
              (back-to-indentation)
              (point))
            (with-temp-buffer
              (insert (string 10 32 32 97 98 99))
              (goto-char (point-max))
              (back-to-indentation)
              (point))
            "#,
        )
        .expect("parse forms");

        let first = ev.eval(&forms[0]).expect("eval nonblank line");
        assert_eq!(first, Value::Int(3));

        let second = ev.eval(&forms[1]).expect("eval whitespace-only line");
        assert_eq!(second, Value::Int(4));

        let third = ev.eval(&forms[2]).expect("eval tab-indent line");
        assert_eq!(third, Value::Int(2));

        let fourth = ev.eval(&forms[3]).expect("eval indented second line");
        assert_eq!(fourth, Value::Int(4));
    }

    #[test]
    fn eval_indent_region_column_subset() {
        let mut ev = super::super::eval::Evaluator::new();
        let forms = super::super::parser::parse_forms(
            r#"
            (with-temp-buffer
              (insert (string 97 10 32 32 98 10 10 9 99))
              (indent-region (point-min) (point-max) 2)
              (string-to-list (buffer-string)))
            (with-temp-buffer
              (insert (string 97 10 32 32 98))
              (indent-region (point-min) (point-max))
              (string-to-list (buffer-string)))
            (with-temp-buffer
              (insert (string 97 10 98))
              (indent-region (point-max) (point-min) 1)
              (string-to-list (buffer-string)))
            (with-temp-buffer
              (insert "a")
              (indent-region (point-min) (point-max) "x"))
            "#,
        )
        .expect("parse forms");

        let first = ev.eval(&forms[0]).expect("eval indent-region column");
        assert_eq!(
            list_to_vec(&first).expect("first byte list"),
            vec![
                Value::Int(32),
                Value::Int(32),
                Value::Int(97),
                Value::Int(10),
                Value::Int(32),
                Value::Int(32),
                Value::Int(98),
                Value::Int(10),
                Value::Int(10),
                Value::Int(32),
                Value::Int(32),
                Value::Int(99),
            ]
        );

        let second = ev.eval(&forms[1]).expect("eval indent-region nil column");
        assert_eq!(
            list_to_vec(&second).expect("second byte list"),
            vec![Value::Int(97), Value::Int(10), Value::Int(98),]
        );

        let third = ev.eval(&forms[2]).expect("eval indent-region swapped bounds");
        assert_eq!(
            list_to_vec(&third).expect("third byte list"),
            vec![Value::Int(97), Value::Int(10), Value::Int(98),]
        );

        let fourth = ev.eval(&forms[3]).expect("eval indent-region non-numeric column");
        assert_eq!(fourth, Value::True);
    }

    #[test]
    fn wrong_arg_count_errors() {
        // current-indentation takes no args
        assert!(builtin_current_indentation(vec![Value::Int(1)]).is_err());
        // indent-to requires at least 1 arg
        assert!(builtin_indent_to(vec![]).is_err());
        // indent-to accepts at most 2 args
        assert!(builtin_indent_to(vec![Value::Int(1), Value::Int(2), Value::Int(3)]).is_err());
        // current-column takes no args
        assert!(builtin_current_column(vec![Value::Int(1)]).is_err());
    }

    #[test]
    fn indent_to_rejects_non_integer() {
        assert!(builtin_indent_to(vec![Value::string("foo")]).is_err());
    }

    #[test]
    fn init_indent_vars_sets_defaults() {
        let mut obarray = super::super::symbol::Obarray::new();
        init_indent_vars(&mut obarray);

        assert_eq!(obarray.symbol_value("tab-width").unwrap().as_int(), Some(8));
        assert!(obarray
            .symbol_value("indent-tabs-mode")
            .unwrap()
            .is_truthy());
        assert_eq!(
            obarray.symbol_value("standard-indent").unwrap().as_int(),
            Some(4)
        );
        assert!(obarray.symbol_value("tab-stop-list").unwrap().is_nil());

        // All should be special (dynamically bound)
        assert!(obarray.is_special("tab-width"));
        assert!(obarray.is_special("indent-tabs-mode"));
        assert!(obarray.is_special("standard-indent"));
        assert!(obarray.is_special("tab-stop-list"));
    }

    #[test]
    fn indent_for_tab_command_inserts_tab() {
        let mut ev = super::super::eval::Evaluator::new();
        let forms = super::super::parser::parse_forms(
            r#"(with-temp-buffer
                 (insert "x")
                 (goto-char 1)
                 (indent-for-tab-command)
                 (buffer-string))"#,
        )
        .expect("parse forms");
        let value = ev.eval(&forms[0]).expect("eval");
        assert_eq!(value.as_str(), Some("\tx"));
    }
}
