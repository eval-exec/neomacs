//! Buffer navigation, line operations, and mark/region management builtins.
//!
//! All functions here take `(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult`
//! and are dispatched from `builtins.rs` via `dispatch_builtin`.

use super::error::{signal, EvalResult, Flow};
use super::value::Value;

// ---------------------------------------------------------------------------
// Argument helpers (duplicated from builtins.rs — they are not `pub`)
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

/// Get a no-current-buffer signal flow.
fn no_buffer() -> Flow {
    signal("error", vec![Value::string("No current buffer")])
}

fn dynamic_or_global_symbol_value(eval: &super::eval::Evaluator, name: &str) -> Option<Value> {
    for frame in eval.dynamic.iter().rev() {
        if let Some(v) = frame.get(name) {
            return Some(v.clone());
        }
    }
    eval.obarray.symbol_value(name).cloned()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert a 1-based Emacs char position to a 0-based byte position in the
/// current buffer.  Clamps to valid range.
fn char_pos_to_byte(buf: &crate::buffer::Buffer, pos: i64) -> usize {
    let char_pos = if pos > 0 { pos as usize - 1 } else { 0 };
    buf.text.char_to_byte(char_pos.min(buf.text.char_count()))
}

/// Convert a 0-based byte position to a 1-based Emacs char position.
fn byte_to_char_pos(buf: &crate::buffer::Buffer, byte_pos: usize) -> i64 {
    buf.text.byte_to_char(byte_pos) as i64 + 1
}

/// Return the full buffer text as a String.
fn buffer_text(buf: &crate::buffer::Buffer) -> String {
    buf.text.to_string()
}

/// Find the byte position of the beginning of the line containing `byte_pos`.
fn line_beginning_byte(text: &str, byte_pos: usize) -> usize {
    // Search backwards for '\n'.
    let pos = byte_pos.min(text.len());
    match text[..pos].rfind('\n') {
        Some(nl) => nl + 1,
        None => 0,
    }
}

/// Find the byte position of the end of the line containing `byte_pos`
/// (position of the '\n', or text length if no trailing newline).
fn line_end_byte(text: &str, byte_pos: usize) -> usize {
    let pos = byte_pos.min(text.len());
    match text[pos..].find('\n') {
        Some(offset) => pos + offset,
        None => text.len(),
    }
}

/// Count newlines in the byte range [start, end).
fn count_newlines(text: &str, start: usize, end: usize) -> usize {
    let s = start.min(text.len());
    let e = end.min(text.len());
    text[s..e].chars().filter(|&c| c == '\n').count()
}

/// Move from `byte_pos` by `n` lines.  Positive = forward, negative = backward.
/// Returns the byte position at the beginning of the destination line and the
/// number of lines actually moved (may be fewer than requested at buffer edges).
fn move_by_lines(text: &str, byte_pos: usize, n: i64) -> (usize, i64) {
    if n == 0 {
        return (line_beginning_byte(text, byte_pos), 0);
    }
    let mut pos = byte_pos.min(text.len());
    let mut moved: i64 = 0;
    if n > 0 {
        for _ in 0..n {
            match text[pos..].find('\n') {
                Some(offset) => {
                    pos = pos + offset + 1;
                    moved += 1;
                }
                None => break,
            }
        }
    } else {
        // Move to start of current line first, if not already there.
        let bol = line_beginning_byte(text, pos);
        if bol < pos {
            pos = bol;
            // This counts as moving backward one line for the purpose of
            // backward navigation — but Emacs counts it differently.
            // Actually, in Emacs `forward-line` with negative N first goes to
            // BOL, and that counts as one of the N lines moved.
            moved -= 1;
        }
        let remaining = n - moved; // remaining is still negative
        for _ in 0..(-remaining) {
            if pos == 0 {
                break;
            }
            // Move before the newline at pos-1
            pos -= 1;
            pos = line_beginning_byte(text, pos);
            moved -= 1;
        }
    }
    (pos, moved)
}

// ===========================================================================
// Position predicates
// ===========================================================================

/// (bobp) -- at beginning of buffer?
pub(crate) fn builtin_bobp(eval: &mut super::eval::Evaluator, _args: Vec<Value>) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    Ok(Value::bool(buf.pt == buf.begv))
}

/// (eobp) -- at end of buffer?
pub(crate) fn builtin_eobp(eval: &mut super::eval::Evaluator, _args: Vec<Value>) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    Ok(Value::bool(buf.pt == buf.zv))
}

/// (bolp) -- at beginning of line?
pub(crate) fn builtin_bolp(eval: &mut super::eval::Evaluator, _args: Vec<Value>) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    if buf.pt == buf.begv {
        return Ok(Value::True);
    }
    let text = buffer_text(buf);
    let at_bol = buf.pt > 0 && buf.pt <= text.len() && text.as_bytes()[buf.pt - 1] == b'\n';
    Ok(Value::bool(buf.pt == 0 || at_bol))
}

/// (eolp) -- at end of line?
pub(crate) fn builtin_eolp(eval: &mut super::eval::Evaluator, _args: Vec<Value>) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    if buf.pt == buf.zv {
        return Ok(Value::True);
    }
    match buf.char_after(buf.pt) {
        Some('\n') => Ok(Value::True),
        _ => Ok(Value::Nil),
    }
}

// ===========================================================================
// Line operations
// ===========================================================================

/// (line-beginning-position &optional N)
pub(crate) fn builtin_line_beginning_position(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let text = buffer_text(buf);
    let mut pos = buf.pt;
    if n != 1 {
        let delta = n - 1;
        let (new_pos, _) = move_by_lines(&text, pos, delta);
        pos = new_pos;
    }
    let bol = line_beginning_byte(&text, pos);
    Ok(Value::Int(byte_to_char_pos(buf, bol)))
}

/// (line-end-position &optional N)
pub(crate) fn builtin_line_end_position(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let text = buffer_text(buf);
    let mut pos = buf.pt;
    if n != 1 {
        let delta = n - 1;
        let (new_pos, _) = move_by_lines(&text, pos, delta);
        pos = new_pos;
    }
    let eol = line_end_byte(&text, pos);
    Ok(Value::Int(byte_to_char_pos(buf, eol)))
}

/// (line-number-at-pos &optional POS ABSOLUTE)
pub(crate) fn builtin_line_number_at_pos(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let byte_pos = if args.is_empty() || args[0].is_nil() {
        buf.pt
    } else {
        char_pos_to_byte(buf, expect_int(&args[0])?)
    };
    let _absolute = args.get(1).is_some_and(|v| v.is_truthy());
    // Count newlines from start of buffer to byte_pos.
    let text = buffer_text(buf);
    let start = if _absolute { 0 } else { buf.begv };
    let line_num = count_newlines(&text, start, byte_pos) + 1;
    Ok(Value::Int(line_num as i64))
}

/// (count-lines BEG END)
pub(crate) fn builtin_count_lines(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("count-lines", &args, 2)?;
    let beg = expect_int(&args[0])?;
    let end = expect_int(&args[1])?;
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let byte_beg = char_pos_to_byte(buf, beg);
    let byte_end = char_pos_to_byte(buf, end);
    let (s, e) = if byte_beg <= byte_end {
        (byte_beg, byte_end)
    } else {
        (byte_end, byte_beg)
    };
    let text = buffer_text(buf);
    let n = count_newlines(&text, s, e);
    Ok(Value::Int(n as i64))
}

/// (forward-line &optional N) -> integer
pub(crate) fn builtin_forward_line(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let text = buffer_text(buf);
    let (new_pos, moved) = move_by_lines(&text, buf.pt, n);
    buf.goto_char(new_pos);
    Ok(Value::Int(n - moved))
}

/// (next-line &optional N)
pub(crate) fn builtin_next_line(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let remainder = match builtin_forward_line(eval, vec![Value::Int(n)])? {
        Value::Int(v) => v,
        _ => 0,
    };
    if remainder > 0 {
        return Err(signal("end-of-buffer", vec![]));
    }
    if remainder < 0 {
        return Err(signal("beginning-of-buffer", vec![]));
    }
    Ok(Value::Nil)
}

/// (previous-line &optional N)
pub(crate) fn builtin_previous_line(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let before_line = {
        let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
        let text = buffer_text(buf);
        count_newlines(&text, 0, line_beginning_byte(&text, buf.pt)) as i64
    };

    let remainder = match builtin_forward_line(eval, vec![Value::Int(-n)])? {
        Value::Int(v) => v,
        _ => 0,
    };

    let after_line = {
        let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
        let text = buffer_text(buf);
        count_newlines(&text, 0, line_beginning_byte(&text, buf.pt)) as i64
    };

    if n > 0 {
        let moved_up = before_line.saturating_sub(after_line);
        if moved_up < n {
            return Err(signal("beginning-of-buffer", vec![]));
        }
    } else if n < 0 {
        let moved_down = after_line.saturating_sub(before_line);
        let wanted = -n;
        if moved_down < wanted {
            return Err(signal("end-of-buffer", vec![]));
        }
    }

    if remainder > 0 {
        return Err(signal("end-of-buffer", vec![]));
    }
    if remainder < 0 {
        return Err(signal("beginning-of-buffer", vec![]));
    }
    Ok(Value::Nil)
}

/// (beginning-of-line &optional N)
pub(crate) fn builtin_beginning_of_line(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let text = buffer_text(buf);
    let mut pos = buf.pt;
    if n != 1 {
        let delta = n - 1;
        let (new_pos, _) = move_by_lines(&text, pos, delta);
        pos = new_pos;
    }
    let bol = line_beginning_byte(&text, pos);
    buf.goto_char(bol);
    Ok(Value::Nil)
}

/// (end-of-line &optional N)
pub(crate) fn builtin_end_of_line(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let text = buffer_text(buf);
    let mut pos = buf.pt;
    if n != 1 {
        let delta = n - 1;
        let (new_pos, _) = move_by_lines(&text, pos, delta);
        pos = new_pos;
    }
    let eol = line_end_byte(&text, pos);
    buf.goto_char(eol);
    Ok(Value::Nil)
}

/// (goto-line LINE)
pub(crate) fn builtin_goto_line(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("goto-line", &args, 1)?;
    let line = expect_int(&args[0])?;
    if line < 1 {
        return Err(signal(
            "args-out-of-range",
            vec![Value::string("Line number must be >= 1"), Value::Int(line)],
        ));
    }
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let text = buffer_text(buf);
    // Go to line 1 (beginning of buffer), then move forward (line-1) lines.
    let (new_pos, _) = move_by_lines(&text, 0, line - 1);
    buf.goto_char(new_pos);
    Ok(Value::Nil)
}

// ===========================================================================
// Character movement
// ===========================================================================

/// (forward-char &optional N)
pub(crate) fn builtin_forward_char(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let cur_char = buf.point_char();
    let total_chars = buf.text.char_count();
    let new_char = if n >= 0 {
        let nc = cur_char.saturating_add(n as usize);
        nc.min(total_chars)
    } else {
        let abs_n = (-n) as usize;
        cur_char.saturating_sub(abs_n)
    };
    let new_byte = buf.text.char_to_byte(new_char);
    buf.goto_char(new_byte);
    // Signal if we couldn't move the full distance
    let desired = cur_char as i64 + n;
    if desired < 0 || desired > total_chars as i64 {
        return Err(signal(
            "beginning-of-buffer",
            if n < 0 { vec![] } else { vec![] },
        ));
    }
    Ok(Value::Nil)
}

/// (backward-char &optional N)
pub(crate) fn builtin_backward_char(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_int(&args[0])?
    };
    // backward-char N == forward-char (- N)
    builtin_forward_char(eval, vec![Value::Int(-n)])
}

/// Parse a skip-chars character set string into a set of chars.
/// Supports ranges like "a-z" and negation with "^" prefix.
fn parse_skip_chars_set(s: &str) -> (bool, Vec<char>) {
    let mut chars: Vec<char> = Vec::new();
    let mut negate = false;
    let mut iter = s.chars().peekable();

    if iter.peek() == Some(&'^') {
        negate = true;
        iter.next();
    }

    let mut prev: Option<char> = None;
    while let Some(c) = iter.next() {
        if c == '-' {
            if let (Some(start), Some(end)) = (prev, iter.peek().copied()) {
                iter.next();
                for ch in start..=end {
                    if !chars.contains(&ch) {
                        chars.push(ch);
                    }
                }
                prev = Some(end);
                continue;
            }
        }
        if !chars.contains(&c) {
            chars.push(c);
        }
        prev = Some(c);
    }
    (negate, chars)
}

/// (skip-chars-forward STRING &optional LIM)
pub(crate) fn builtin_skip_chars_forward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("skip-chars-forward", &args, 1)?;
    let set_str = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ))
        }
    };
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let lim_byte = if args.len() > 1 && !args[1].is_nil() {
        char_pos_to_byte(buf, expect_int(&args[1])?)
    } else {
        buf.zv
    };

    let (negate, char_set) = parse_skip_chars_set(&set_str);
    let text = buffer_text(buf);
    let start_pos = buf.pt;
    let mut pos = buf.pt;
    let limit = lim_byte.min(text.len());

    while pos < limit {
        if let Some(ch) = buf.text.char_at(pos) {
            let in_set = char_set.contains(&ch);
            if negate {
                if in_set {
                    break;
                }
            } else if !in_set {
                break;
            }
            pos += ch.len_utf8();
        } else {
            break;
        }
    }
    buf.goto_char(pos);
    let moved_chars = buf.text.byte_to_char(pos) as i64 - buf.text.byte_to_char(start_pos) as i64;
    Ok(Value::Int(moved_chars))
}

/// (skip-chars-backward STRING &optional LIM)
pub(crate) fn builtin_skip_chars_backward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("skip-chars-backward", &args, 1)?;
    let set_str = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ))
        }
    };
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let lim_byte = if args.len() > 1 && !args[1].is_nil() {
        char_pos_to_byte(buf, expect_int(&args[1])?)
    } else {
        buf.begv
    };

    let (negate, char_set) = parse_skip_chars_set(&set_str);
    let _text = buffer_text(buf);
    let start_pos = buf.pt;
    let mut pos = buf.pt;
    let limit = lim_byte;

    while pos > limit {
        // Find the character before `pos`.
        if let Some(ch) = buf.char_before(pos) {
            let in_set = char_set.contains(&ch);
            if negate {
                if in_set {
                    break;
                }
            } else if !in_set {
                break;
            }
            pos -= ch.len_utf8();
        } else {
            break;
        }
    }
    buf.goto_char(pos);
    let moved_chars = buf.text.byte_to_char(pos) as i64 - buf.text.byte_to_char(start_pos) as i64;
    Ok(Value::Int(moved_chars))
}

// ===========================================================================
// Mark and region
// ===========================================================================

/// (push-mark &optional LOCATION NOMSG ACTIVATE)
pub(crate) fn builtin_push_mark(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let byte_pos = if args.is_empty() || args[0].is_nil() {
        buf.pt
    } else {
        char_pos_to_byte(buf, expect_int(&args[0])?)
    };
    // Store current mark on the mark ring (buffer-local property "mark-ring")
    if let Some(old_mark) = buf.mark {
        let old_char = buf.text.byte_to_char(old_mark) as i64 + 1;
        let ring = buf
            .properties
            .entry("mark-ring".to_string())
            .or_insert(Value::Nil);
        // Prepend old mark to the ring list
        *ring = Value::cons(Value::Int(old_char), ring.clone());
    }
    buf.set_mark(byte_pos);
    let _nomsg = args.get(1).is_some_and(|v| v.is_truthy());
    let activate = args.get(2).is_some_and(|v| v.is_truthy());
    if activate {
        buf.properties
            .insert("mark-active".to_string(), Value::True);
    }
    Ok(Value::Nil)
}

/// (pop-mark)
pub(crate) fn builtin_pop_mark(eval: &mut super::eval::Evaluator, _args: Vec<Value>) -> EvalResult {
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let ring = buf
        .properties
        .get("mark-ring")
        .cloned()
        .unwrap_or(Value::Nil);
    match &ring {
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            let pos_val = pair.car.clone();
            let rest = pair.cdr.clone();
            drop(pair);
            buf.properties.insert("mark-ring".to_string(), rest);
            if let Some(pos) = pos_val.as_int() {
                let byte_pos = char_pos_to_byte(buf, pos);
                buf.set_mark(byte_pos);
            }
            Ok(Value::Nil)
        }
        _ => {
            // Empty mark ring — just clear the mark
            buf.mark = None;
            Ok(Value::Nil)
        }
    }
}

/// (set-mark POS) — set mark at pos and activate
/// Note: this overrides the existing builtin_set_mark in builtins.rs with
/// additional mark-active behavior.  We keep it here but the dispatch will
/// point to the one in builtins.rs unless re-routed.
pub(crate) fn builtin_set_mark_nav(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-mark", &args, 1)?;
    let pos = expect_int(&args[0])? as usize;
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let char_pos = if pos > 0 { pos - 1 } else { 0 };
    let byte_pos = buf.text.char_to_byte(char_pos.min(buf.text.char_count()));
    buf.set_mark(byte_pos);
    buf.properties
        .insert("mark-active".to_string(), Value::True);
    Ok(args[0].clone())
}

/// (mark &optional FORCE) -> integer or signal
pub(crate) fn builtin_mark_nav(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    let _force = args.first().is_some_and(|v| v.is_truthy());
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    match buf.mark() {
        Some(byte_pos) => Ok(Value::Int(byte_to_char_pos(buf, byte_pos))),
        None => Ok(Value::Nil),
    }
}

/// (region-beginning) -> integer
pub(crate) fn builtin_region_beginning(
    eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let mark = buf.mark().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string(
                "The mark is not set now, so there is no region",
            )],
        )
    })?;
    let pt = buf.pt;
    let start = pt.min(mark);
    Ok(Value::Int(byte_to_char_pos(buf, start)))
}

/// (region-end) -> integer
pub(crate) fn builtin_region_end(
    eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let mark = buf.mark().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string(
                "The mark is not set now, so there is no region",
            )],
        )
    })?;
    let pt = buf.pt;
    let end = pt.max(mark);
    Ok(Value::Int(byte_to_char_pos(buf, end)))
}

/// (use-region-p) -> t or nil
pub(crate) fn builtin_use_region_p(
    eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let mark_active = match dynamic_or_global_symbol_value(eval, "mark-active") {
        Some(v) => v.is_truthy(),
        None => buf
            .properties
            .get("mark-active")
            .is_some_and(|v| v.is_truthy()),
    };
    let transient_mark_mode = dynamic_or_global_symbol_value(eval, "transient-mark-mode")
        .is_some_and(|v| v.is_truthy());
    let non_empty_region = buf.mark().is_some_and(|m| m != buf.point());
    Ok(Value::bool(
        mark_active && transient_mark_mode && non_empty_region,
    ))
}

/// (deactivate-mark &optional FORCE)
pub(crate) fn builtin_deactivate_mark(
    eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    buf.properties.insert("mark-active".to_string(), Value::Nil);
    Ok(Value::Nil)
}

/// (exchange-point-and-mark &optional ARG)
pub(crate) fn builtin_exchange_point_and_mark(
    eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer_mut().ok_or_else(no_buffer)?;
    let mark = buf.mark().ok_or_else(|| {
        signal(
            "mark-not-set",
            vec![Value::string("No mark set in this buffer")],
        )
    })?;
    let old_pt = buf.pt;
    buf.pt = mark;
    buf.mark = Some(old_pt);
    // Activate the mark
    buf.properties
        .insert("mark-active".to_string(), Value::True);
    Ok(Value::Nil)
}

/// (transient-mark-mode &optional ARG) — toggle/query mode flag.
pub(crate) fn builtin_transient_mark_mode(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("transient-mark-mode"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let arg = args.first().unwrap_or(&Value::Nil);
    let numeric = match arg {
        Value::Nil => 1,
        Value::Int(n) => *n,
        Value::Float(f) => *f as i64,
        Value::Char(c) => *c as i64,
        _ => 1,
    };

    let val = if numeric > 0 { Value::True } else { Value::Nil };
    eval.obarray
        .set_symbol_value("transient-mark-mode", val.clone());
    Ok(val)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::super::eval::Evaluator;
    use super::super::value::Value;

    /// Helper: create an evaluator, insert text, and position point.
    fn eval_with_text(text: &str) -> Evaluator {
        let mut ev = Evaluator::new();
        {
            let buf = ev.buffers.current_buffer_mut().unwrap();
            buf.insert(text);
            // Point is now at the end. Reset to beginning.
            buf.goto_char(0);
        }
        ev
    }

    /// Evaluate an Elisp string and return the result Value.
    fn eval_str(ev: &mut Evaluator, src: &str) -> Value {
        let forms = super::super::parser::parse_forms(src).unwrap();
        let results = ev.eval_forms(&forms);
        results.into_iter().last().unwrap().unwrap()
    }

    /// Evaluate and expect an integer result.
    fn eval_int(ev: &mut Evaluator, src: &str) -> i64 {
        match eval_str(ev, src) {
            Value::Int(n) => n,
            other => panic!("expected Int, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Position predicates
    // -----------------------------------------------------------------------

    #[test]
    fn test_bobp_at_beginning() {
        let mut ev = eval_with_text("hello");
        let val = eval_str(&mut ev, "(bobp)");
        assert!(val.is_truthy());
    }

    #[test]
    fn test_bobp_not_at_beginning() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(forward-char 2)");
        let val = eval_str(&mut ev, "(bobp)");
        assert!(val.is_nil());
    }

    #[test]
    fn test_eobp_at_end() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(goto-char 6)"); // past last char (1-based)
        let val = eval_str(&mut ev, "(eobp)");
        assert!(val.is_truthy());
    }

    #[test]
    fn test_eobp_not_at_end() {
        let mut ev = eval_with_text("hello");
        let val = eval_str(&mut ev, "(eobp)");
        assert!(val.is_nil());
    }

    #[test]
    fn test_bolp_at_beginning_of_buffer() {
        let mut ev = eval_with_text("hello");
        let val = eval_str(&mut ev, "(bolp)");
        assert!(val.is_truthy());
    }

    #[test]
    fn test_bolp_after_newline() {
        let mut ev = eval_with_text("abc\ndef");
        eval_str(&mut ev, "(goto-char 5)"); // right after newline
        let val = eval_str(&mut ev, "(bolp)");
        assert!(val.is_truthy());
    }

    #[test]
    fn test_bolp_not_at_bol() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(forward-char 2)");
        let val = eval_str(&mut ev, "(bolp)");
        assert!(val.is_nil());
    }

    #[test]
    fn test_eolp_at_newline() {
        let mut ev = eval_with_text("abc\ndef");
        eval_str(&mut ev, "(goto-char 4)"); // at newline
        let val = eval_str(&mut ev, "(eolp)");
        assert!(val.is_truthy());
    }

    #[test]
    fn test_eolp_at_eob() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(goto-char 6)");
        let val = eval_str(&mut ev, "(eolp)");
        assert!(val.is_truthy());
    }

    #[test]
    fn test_eolp_not_at_eol() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(goto-char 2)");
        let val = eval_str(&mut ev, "(eolp)");
        assert!(val.is_nil());
    }

    // -----------------------------------------------------------------------
    // Line operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_line_beginning_position() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        eval_str(&mut ev, "(goto-char 6)"); // middle of "def"
        let pos = eval_int(&mut ev, "(line-beginning-position)");
        assert_eq!(pos, 5); // start of "def" line
    }

    #[test]
    fn test_line_end_position() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        eval_str(&mut ev, "(goto-char 6)"); // middle of "def"
        let pos = eval_int(&mut ev, "(line-end-position)");
        assert_eq!(pos, 8); // end of "def" (position of newline)
    }

    #[test]
    fn test_line_beginning_position_with_offset() {
        let mut ev = eval_with_text("aaa\nbbb\nccc");
        eval_str(&mut ev, "(goto-char 1)"); // beginning of first line
        let pos = eval_int(&mut ev, "(line-beginning-position 2)");
        assert_eq!(pos, 5); // beginning of second line
    }

    #[test]
    fn test_line_end_position_with_offset() {
        let mut ev = eval_with_text("aaa\nbbb\nccc");
        eval_str(&mut ev, "(goto-char 1)");
        let pos = eval_int(&mut ev, "(line-end-position 2)");
        assert_eq!(pos, 8); // end of second line (position of newline)
    }

    #[test]
    fn test_line_number_at_pos() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        let n = eval_int(&mut ev, "(line-number-at-pos 6)");
        assert_eq!(n, 2); // "def" is line 2
    }

    #[test]
    fn test_line_number_at_pos_default() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        // Point is at 1 (first char)
        let n = eval_int(&mut ev, "(line-number-at-pos)");
        assert_eq!(n, 1);
    }

    #[test]
    fn test_count_lines() {
        let mut ev = eval_with_text("abc\ndef\nghi\n");
        let n = eval_int(&mut ev, "(count-lines 1 13)");
        assert_eq!(n, 3);
    }

    #[test]
    fn test_count_lines_same_line() {
        let mut ev = eval_with_text("abcdef");
        let n = eval_int(&mut ev, "(count-lines 1 4)");
        assert_eq!(n, 0);
    }

    #[test]
    fn test_forward_line() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        let remainder = eval_int(&mut ev, "(forward-line 1)");
        assert_eq!(remainder, 0);
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 5); // beginning of "def" line
    }

    #[test]
    fn test_forward_line_past_end() {
        let mut ev = eval_with_text("abc\ndef");
        let remainder = eval_int(&mut ev, "(forward-line 5)");
        assert!(remainder > 0);
    }

    #[test]
    fn test_next_line_moves_to_next_line() {
        let mut ev = eval_with_text("abc\ndef");
        eval_str(&mut ev, "(next-line)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 5);
    }

    #[test]
    fn test_next_line_signals_end_of_buffer() {
        let mut ev = eval_with_text("abc");
        let val = eval_str(&mut ev, "(condition-case err (next-line) (error (car err)))");
        assert_eq!(val.as_symbol_name(), Some("end-of-buffer"));
    }

    #[test]
    fn test_previous_line_moves_to_previous_line() {
        let mut ev = eval_with_text("abc\ndef");
        eval_str(&mut ev, "(goto-char 5)");
        eval_str(&mut ev, "(previous-line)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 1);
    }

    #[test]
    fn test_previous_line_signals_beginning_of_buffer() {
        let mut ev = eval_with_text("abc");
        let val = eval_str(
            &mut ev,
            "(condition-case err (previous-line) (error (car err)))",
        );
        assert_eq!(val.as_symbol_name(), Some("beginning-of-buffer"));
    }

    #[test]
    fn test_previous_line_signals_beginning_of_buffer_from_middle_of_line() {
        let mut ev = eval_with_text("abc");
        eval_str(&mut ev, "(goto-char 2)");
        let val = eval_str(
            &mut ev,
            "(condition-case err (previous-line) (error (car err)))",
        );
        assert_eq!(val.as_symbol_name(), Some("beginning-of-buffer"));
    }

    #[test]
    fn test_beginning_of_line() {
        let mut ev = eval_with_text("abc\ndef");
        eval_str(&mut ev, "(goto-char 6)");
        eval_str(&mut ev, "(beginning-of-line)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 5);
    }

    #[test]
    fn test_end_of_line() {
        let mut ev = eval_with_text("abc\ndef");
        eval_str(&mut ev, "(goto-char 1)");
        eval_str(&mut ev, "(end-of-line)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 4); // position of '\n'
    }

    #[test]
    fn test_goto_line() {
        let mut ev = eval_with_text("aaa\nbbb\nccc");
        eval_str(&mut ev, "(goto-line 3)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 9); // beginning of third line
    }

    // -----------------------------------------------------------------------
    // Character movement
    // -----------------------------------------------------------------------

    #[test]
    fn test_forward_char() {
        let mut ev = eval_with_text("abcdef");
        eval_str(&mut ev, "(forward-char 3)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 4); // 1-based
    }

    #[test]
    fn test_backward_char() {
        let mut ev = eval_with_text("abcdef");
        eval_str(&mut ev, "(goto-char 5)");
        eval_str(&mut ev, "(backward-char 2)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 3);
    }

    #[test]
    fn test_forward_char_default() {
        let mut ev = eval_with_text("abcdef");
        eval_str(&mut ev, "(forward-char)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 2);
    }

    #[test]
    fn test_skip_chars_forward() {
        let mut ev = eval_with_text("aaabbbccc");
        let moved = eval_int(&mut ev, "(skip-chars-forward \"a\")");
        assert_eq!(moved, 3);
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 4);
    }

    #[test]
    fn test_skip_chars_forward_range() {
        let mut ev = eval_with_text("abcdef123");
        let moved = eval_int(&mut ev, "(skip-chars-forward \"a-f\")");
        assert_eq!(moved, 6);
    }

    #[test]
    fn test_skip_chars_backward() {
        let mut ev = eval_with_text("aaabbbccc");
        eval_str(&mut ev, "(goto-char 10)"); // end
        let moved = eval_int(&mut ev, "(skip-chars-backward \"c\")");
        assert_eq!(moved, -3);
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 7);
    }

    #[test]
    fn test_skip_chars_forward_negate() {
        let mut ev = eval_with_text("aaabbbccc");
        let moved = eval_int(&mut ev, "(skip-chars-forward \"^b\")");
        assert_eq!(moved, 3);
    }

    // -----------------------------------------------------------------------
    // Mark and region
    // -----------------------------------------------------------------------

    #[test]
    fn test_push_mark_and_mark() {
        let mut ev = eval_with_text("hello world");
        eval_str(&mut ev, "(push-mark 3)");
        let m = eval_int(&mut ev, "(mark t)");
        assert_eq!(m, 3);
    }

    #[test]
    fn test_push_mark_default_pos() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(goto-char 3)");
        eval_str(&mut ev, "(push-mark)");
        let m = eval_int(&mut ev, "(mark t)");
        assert_eq!(m, 3);
    }

    #[test]
    fn test_pop_mark() {
        let mut ev = eval_with_text("hello world");
        eval_str(&mut ev, "(push-mark 3)");
        eval_str(&mut ev, "(push-mark 5)");
        // Mark is now at 5, ring has [3]
        let m = eval_int(&mut ev, "(mark t)");
        assert_eq!(m, 5);
        eval_str(&mut ev, "(pop-mark)");
        let m2 = eval_int(&mut ev, "(mark t)");
        assert_eq!(m2, 3);
    }

    #[test]
    fn test_region_beginning_and_end() {
        let mut ev = eval_with_text("hello world");
        eval_str(&mut ev, "(goto-char 8)");
        eval_str(&mut ev, "(push-mark 3 nil t)");
        let beg = eval_int(&mut ev, "(region-beginning)");
        let end = eval_int(&mut ev, "(region-end)");
        assert_eq!(beg, 3);
        assert_eq!(end, 8);
    }

    #[test]
    fn test_use_region_p() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(push-mark 3 nil t)"); // activate
        let active = eval_str(&mut ev, "(use-region-p)");
        assert!(active.is_truthy());
    }

    #[test]
    fn test_use_region_p_inactive() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(push-mark 3)"); // not activated
        let active = eval_str(&mut ev, "(use-region-p)");
        assert!(active.is_nil());
    }

    #[test]
    fn test_deactivate_mark() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(push-mark 3 nil t)");
        eval_str(&mut ev, "(deactivate-mark)");
        let active = eval_str(&mut ev, "(use-region-p)");
        assert!(active.is_nil());
    }

    #[test]
    fn test_exchange_point_and_mark() {
        let mut ev = eval_with_text("hello world");
        eval_str(&mut ev, "(goto-char 3)");
        eval_str(&mut ev, "(push-mark 8 nil t)");
        eval_str(&mut ev, "(exchange-point-and-mark)");
        let pt = eval_int(&mut ev, "(point)");
        let mk = eval_int(&mut ev, "(mark t)");
        assert_eq!(pt, 8);
        assert_eq!(mk, 3);
    }

    #[test]
    fn test_transient_mark_mode() {
        let mut ev = eval_with_text("hello");
        let enabled = eval_str(&mut ev, "(transient-mark-mode)");
        assert!(enabled.is_truthy());

        let disabled = eval_str(&mut ev, "(transient-mark-mode -1)");
        assert!(disabled.is_nil());

        let reenabled_nil = eval_str(&mut ev, "(transient-mark-mode nil)");
        assert!(reenabled_nil.is_truthy());

        let zero = eval_str(&mut ev, "(transient-mark-mode 0)");
        assert!(zero.is_nil());

        let positive_float = eval_str(&mut ev, "(transient-mark-mode 1.5)");
        assert!(positive_float.is_truthy());

        let small_float = eval_str(&mut ev, "(transient-mark-mode 0.5)");
        assert!(small_float.is_nil());
    }

    #[test]
    fn test_transient_mark_mode_over_arity() {
        let mut ev = eval_with_text("hello");
        let result = eval_str(
            &mut ev,
            "(condition-case err (transient-mark-mode nil nil) (error (car err)))",
        );
        assert_eq!(result, Value::symbol("wrong-number-of-arguments"));
    }

    #[test]
    fn test_mark_marker() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(push-mark 4)");
        let pos = eval_int(&mut ev, "(marker-position (mark-marker))");
        assert_eq!(pos, 4);
    }

    #[test]
    fn test_set_mark_activates() {
        let mut ev = eval_with_text("hello");
        eval_str(&mut ev, "(set-mark 3)");
        let active = eval_str(&mut ev, "(use-region-p)");
        assert!(active.is_truthy());
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_buffer_predicates() {
        let mut ev = Evaluator::new();
        let val = eval_str(&mut ev, "(bobp)");
        assert!(val.is_truthy());
        let val = eval_str(&mut ev, "(eobp)");
        assert!(val.is_truthy());
        let val = eval_str(&mut ev, "(bolp)");
        assert!(val.is_truthy());
        let val = eval_str(&mut ev, "(eolp)");
        assert!(val.is_truthy());
    }

    #[test]
    fn test_forward_line_negative() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        eval_str(&mut ev, "(goto-char 9)"); // on "ghi" line
        eval_str(&mut ev, "(forward-line -1)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 5); // beginning of "def"
    }

    #[test]
    fn test_line_number_at_pos_last_line() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        let n = eval_int(&mut ev, "(line-number-at-pos 10)");
        assert_eq!(n, 3);
    }

    #[test]
    fn test_skip_chars_forward_with_limit() {
        let mut ev = eval_with_text("aaaaaaa");
        let moved = eval_int(&mut ev, "(skip-chars-forward \"a\" 4)");
        assert_eq!(moved, 3); // limited to position 4 (1-based = 3 chars from pos 1)
    }

    #[test]
    fn test_goto_line_first() {
        let mut ev = eval_with_text("abc\ndef\nghi");
        eval_str(&mut ev, "(goto-char 7)"); // somewhere in the middle
        eval_str(&mut ev, "(goto-line 1)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 1);
    }

    #[test]
    fn test_forward_char_negative() {
        let mut ev = eval_with_text("abcdef");
        eval_str(&mut ev, "(goto-char 4)");
        eval_str(&mut ev, "(forward-char -2)");
        let pos = eval_int(&mut ev, "(point)");
        assert_eq!(pos, 2);
    }
}
