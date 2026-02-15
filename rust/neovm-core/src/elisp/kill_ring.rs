//! Kill ring and text editing commands for the Elisp VM.
//!
//! Implements:
//! - Kill ring data structure with push, rotate, yank-pop
//! - Kill/yank commands: kill-region, kill-ring-save, copy-region-as-kill,
//!   kill-line, kill-whole-line, kill-word, backward-kill-word, yank, yank-pop,
//!   current-kill, kill-new, kill-append
//! - Case commands: downcase-region, upcase-region, capitalize-region,
//!   downcase-word, upcase-word, capitalize-word
//! - Transpose commands: transpose-chars, transpose-words, transpose-lines
//! - Indent/newline commands: indent-line-to, indent-to, newline,
//!   newline-and-indent, open-line, delete-horizontal-space, just-one-space,
//!   delete-indentation, tab-to-tab-stop, indent-rigidly

use super::error::{signal, EvalResult, Flow};
use super::syntax::{backward_word, forward_word, scan_sexps, SyntaxClass, SyntaxTable};
use super::value::Value;
use crate::buffer::Buffer;

// ===========================================================================
// Argument helpers (local copies — same pattern as other modules)
// ===========================================================================

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

fn expect_min_max_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    expect_min_args(name, args, min)?;
    expect_max_args(name, args, max)
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

fn expect_indent_column(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        Value::Float(_) => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("fixnump"), value.clone()],
        )),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), other.clone()],
        )),
    }
}

fn expect_string(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

#[inline]
fn is_horizontal_space(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

fn skip_sexp_ignorable_forward(buf: &Buffer, table: &SyntaxTable, mut pos: usize) -> usize {
    let pmax = buf.point_max();
    while pos < pmax {
        let Some(ch) = buf.char_after(pos) else {
            break;
        };
        match table.char_syntax(ch) {
            SyntaxClass::Whitespace | SyntaxClass::Comment | SyntaxClass::EndComment => {
                pos += ch.len_utf8();
            }
            _ => break,
        }
    }
    pos
}

fn collect_sexp_spans(buf: &Buffer, table: &SyntaxTable) -> Vec<(usize, usize)> {
    let pmax = buf.point_max();
    let mut spans = Vec::new();
    let mut pos = skip_sexp_ignorable_forward(buf, table, buf.point_min());
    while pos < pmax {
        let Ok(end) = scan_sexps(buf, table, pos, 1) else {
            let Some(ch) = buf.char_after(pos) else {
                break;
            };
            pos += ch.len_utf8();
            pos = skip_sexp_ignorable_forward(buf, table, pos);
            continue;
        };
        if end <= pos {
            break;
        }
        spans.push((pos, end));
        pos = skip_sexp_ignorable_forward(buf, table, end);
    }
    spans
}

fn move_point_by_sexps(buf: &Buffer, table: &SyntaxTable, mut pos: usize, count: i64) -> usize {
    if count == 0 {
        return pos;
    }
    let step = if count > 0 { 1 } else { -1 };
    for _ in 0..count.unsigned_abs() {
        let Ok(next) = scan_sexps(buf, table, pos, step) else {
            break;
        };
        pos = next;
    }
    pos
}

fn is_text_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r')
}

fn skip_text_whitespace_forward(buf: &Buffer, mut pos: usize) -> usize {
    let pmax = buf.point_max();
    while pos < pmax {
        let Some(ch) = buf.char_after(pos) else {
            break;
        };
        if !is_text_whitespace(ch) {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn is_sentence_terminator(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?')
}

fn collect_sentence_spans(buf: &Buffer) -> Vec<(usize, usize)> {
    let pmin = buf.point_min();
    let pmax = buf.point_max();
    let mut spans = Vec::new();
    let mut pos = skip_text_whitespace_forward(buf, pmin);

    while pos < pmax {
        let start = pos;
        let mut end = pmax;
        let mut scan = pos;

        while scan < pmax {
            let Some(ch) = buf.char_after(scan) else {
                break;
            };
            scan += ch.len_utf8();

            if !is_sentence_terminator(ch) {
                continue;
            }

            if scan >= pmax {
                end = scan;
                break;
            }

            let mut look = scan;
            let mut spaces = 0usize;
            while look < pmax {
                let Some(next) = buf.char_after(look) else {
                    break;
                };
                if next == ' ' || next == '\t' {
                    spaces += 1;
                    look += next.len_utf8();
                } else {
                    break;
                }
            }

            let newline_break = look < pmax
                && buf
                    .char_after(look)
                    .is_some_and(|next| next == '\n' || next == '\r');

            // Emacs defaults to `sentence-end-double-space` semantics.
            if spaces >= 2 || newline_break {
                end = scan;
                break;
            }
        }

        spans.push((start, end));
        if end >= pmax {
            break;
        }
        pos = skip_text_whitespace_forward(buf, end);
    }

    spans
}

fn is_blank_paragraph_line(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|b| matches!(*b, b' ' | b'\t' | b'\r' | 0x0b | 0x0c))
}

fn collect_paragraph_boundaries(buf: &Buffer) -> (Vec<usize>, Vec<usize>) {
    let pmin = buf.point_min();
    let pmax = buf.point_max();
    if pmax <= pmin {
        return (Vec::new(), Vec::new());
    }

    let text = buf.buffer_substring(pmin, pmax);
    let bytes = text.as_bytes();

    #[derive(Copy, Clone)]
    struct LineInfo {
        start: usize,
        blank: bool,
    }

    let mut lines = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let line_start = i;
        while i < bytes.len() && bytes[i] != b'\n' {
            i += 1;
        }
        let line_end = i;
        let blank = is_blank_paragraph_line(&bytes[line_start..line_end]);
        if i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
        }
        lines.push(LineInfo {
            start: pmin + line_start,
            blank,
        });
    }

    let mut forward = Vec::new();
    let mut backward = Vec::new();
    let mut idx = 0usize;
    while idx < lines.len() {
        if !lines[idx].blank {
            idx += 1;
            continue;
        }

        let run_start = idx;
        while idx < lines.len() && lines[idx].blank {
            idx += 1;
        }
        let run_end = idx; // exclusive

        let has_nonblank_before = lines[..run_start].iter().any(|line| !line.blank);
        let has_nonblank_after = lines[run_end..].iter().any(|line| !line.blank);

        if has_nonblank_before {
            forward.push(lines[run_start].start);
        }
        if has_nonblank_after {
            backward.push(lines[run_end - 1].start);
        }
    }

    (forward, backward)
}

fn move_paragraph_boundary(
    mut pos: usize,
    count: i64,
    pmin: usize,
    pmax: usize,
    forward: &[usize],
    backward: &[usize],
) -> usize {
    pos = pos.clamp(pmin, pmax);
    if count == 0 {
        return pos;
    }

    if count > 0 {
        for _ in 0..(count as usize) {
            let idx = forward.partition_point(|boundary| *boundary <= pos);
            pos = if idx < forward.len() {
                forward[idx]
            } else {
                pmax
            };
        }
        return pos;
    }

    for _ in 0..(count.unsigned_abs() as usize) {
        let idx = backward.partition_point(|boundary| *boundary < pos);
        pos = if idx > 0 { backward[idx - 1] } else { pmin };
    }
    pos
}

fn paragraph_aux_position(
    pos: usize,
    step: i64,
    pmin: usize,
    pmax: usize,
    forward: &[usize],
    backward: &[usize],
) -> ((usize, usize), usize) {
    let first = move_paragraph_boundary(pos, step, pmin, pmax, forward, backward);
    let second = move_paragraph_boundary(first, -step, pmin, pmax, forward, backward);
    ((first, second), second)
}

fn transpose_subr_ranges(
    eval: &mut super::eval::Evaluator,
    mut pos1: (usize, usize),
    mut pos2: (usize, usize),
) -> EvalResult {
    let two_thing_error =
        || signal("error", vec![Value::string("Don\u{2019}t have two things to transpose")]);

    if pos1.0 > pos1.1 {
        pos1 = (pos1.1, pos1.0);
    }
    if pos2.0 > pos2.1 {
        pos2 = (pos2.1, pos2.0);
    }
    if pos1.0 > pos2.0 {
        std::mem::swap(&mut pos1, &mut pos2);
    }
    if pos1.1 > pos2.0 {
        return Err(two_thing_error());
    }

    let (a, middle, b) = {
        let buf_r = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf_r.buffer_substring(pos1.0, pos1.1),
            buf_r.buffer_substring(pos1.1, pos2.0),
            buf_r.buffer_substring(pos2.0, pos2.1),
        )
    };

    let buf_m = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf_m.delete_region(pos1.0, pos2.1);
    buf_m.goto_char(pos1.0);
    buf_m.insert(&b);
    buf_m.insert(&middle);
    buf_m.insert(&a);

    Ok(Value::Nil)
}

// ===========================================================================
// KillRing data structure
// ===========================================================================

/// The kill ring — a bounded circular buffer of killed (cut/copied) text.
#[derive(Clone, Debug)]
pub struct KillRing {
    /// The ring entries, most recent at index 0.
    entries: Vec<String>,
    /// Maximum number of entries to keep.
    max_size: usize,
    /// Current yank pointer (index into entries).  Reset to 0 on each new kill.
    yank_pointer: usize,
    /// Tracks whether the last command was a yank (for yank-pop).
    last_was_yank: bool,
    /// Tracks the region replaced by the last yank (start, end byte positions)
    /// so yank-pop can replace it.
    last_yank_region: Option<(usize, usize)>,
}

impl Default for KillRing {
    fn default() -> Self {
        Self::new()
    }
}

impl KillRing {
    /// Create a new empty kill ring with the default max size of 60.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_size: 60,
            yank_pointer: 0,
            last_was_yank: false,
            last_yank_region: None,
        }
    }

    /// Push a new string onto the kill ring.  Resets the yank pointer.
    pub fn push(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.entries.insert(0, text);
        if self.entries.len() > self.max_size {
            self.entries.truncate(self.max_size);
        }
        self.yank_pointer = 0;
    }

    /// Replace the most recent entry instead of pushing a new one.
    pub fn replace_top(&mut self, text: String) {
        if self.entries.is_empty() {
            self.push(text);
        } else {
            self.entries[0] = text;
        }
        self.yank_pointer = 0;
    }

    /// Append text to the most recent kill entry.
    /// If `before` is true, prepend instead.
    pub fn append(&mut self, text: &str, before: bool) {
        if self.entries.is_empty() {
            self.push(text.to_string());
            return;
        }
        if before {
            let mut new = String::with_capacity(text.len() + self.entries[0].len());
            new.push_str(text);
            new.push_str(&self.entries[0]);
            self.entries[0] = new;
        } else {
            self.entries[0].push_str(text);
        }
    }

    /// Get the current kill (at yank_pointer).  Returns None if empty.
    pub fn current(&self) -> Option<&str> {
        self.entries.get(self.yank_pointer).map(|s| s.as_str())
    }

    /// Rotate the yank pointer by `n` positions (positive = older entries).
    /// Returns the text at the new position, or None if the ring is empty.
    pub fn rotate(&mut self, n: i64) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let len = self.entries.len() as i64;
        let new_ptr = ((self.yank_pointer as i64 + n) % len + len) % len;
        self.yank_pointer = new_ptr as usize;
        self.current()
    }

    /// Return the number of entries in the kill ring.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the kill ring is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Convert the kill ring contents to a Lisp list of strings.
    pub fn to_lisp_list(&self) -> Value {
        let values: Vec<Value> = self
            .entries
            .iter()
            .map(|s| Value::string(s.clone()))
            .collect();
        Value::list(values)
    }
}

// ===========================================================================
// Buffer helper: resolve BEG END from arguments (Emacs convention)
// ===========================================================================

/// Resolve (beg, end) byte positions from Emacs-style 1-based character
/// positions, ensuring beg <= end and both are within the accessible region.
fn resolve_region(buf: &Buffer, beg: i64, end: i64) -> (usize, usize) {
    let point_min = buf.text.byte_to_char(buf.point_min()) as i64 + 1;
    let point_max = buf.text.byte_to_char(buf.point_max()) as i64 + 1;

    let mut a = beg.clamp(point_min, point_max);
    let mut b = end.clamp(point_min, point_max);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }

    // Convert 1-based character positions to 0-based byte positions.
    let a_byte = buf.text.char_to_byte((a - 1).max(0) as usize);
    let b_byte = buf.text.char_to_byte((b - 1).max(0) as usize);
    (a_byte, b_byte)
}

/// Resolve case-region bounds.
/// When REGION-NONCONTIGUOUS-P is non-nil, Emacs ignores BEG/END and uses the
/// active point/mark region instead.
fn resolve_case_region(
    eval: &super::eval::Evaluator,
    beg: i64,
    end: i64,
    arg: Option<&Value>,
) -> Result<(usize, usize), Flow> {
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if arg.is_some_and(|v| !v.is_nil()) {
        let mark = buf.mark().ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("The mark is not set now, so there is no region")],
            )
        })?;
        let pt = buf.point();
        return Ok((pt.min(mark), pt.max(mark)));
    }

    Ok(resolve_region(buf, beg, end))
}

// ===========================================================================
// Kill ring builtins
// ===========================================================================

/// `(kill-new STRING &optional REPLACE)` — add STRING to the kill ring.
/// If REPLACE is non-nil, replace the most recent entry instead.
pub(crate) fn builtin_kill_new(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("kill-new", &args, 1)?;
    let text = expect_string(&args[0])?;
    let replace = args.get(1).map_or(false, |v| v.is_truthy());

    if replace {
        eval.kill_ring.replace_top(text);
    } else {
        eval.kill_ring.push(text);
    }
    Ok(Value::Nil)
}

/// `(kill-append STRING BEFORE-P)` — append STRING to the latest kill.
/// If BEFORE-P is non-nil, prepend instead.
pub(crate) fn builtin_kill_append(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kill-append", &args, 2)?;
    let text = expect_string(&args[0])?;
    let before = args[1].is_truthy();
    eval.kill_ring.append(&text, before);
    Ok(Value::Nil)
}

/// `(current-kill N &optional DO-NOT-MOVE)` — rotate kill ring by N, return text.
/// If DO-NOT-MOVE is non-nil, don't actually rotate, just peek.
pub(crate) fn builtin_current_kill(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("current-kill", &args, 1)?;
    let n = expect_int(&args[0])?;
    let do_not_move = args.get(1).map_or(false, |v| v.is_truthy());

    if eval.kill_ring.is_empty() {
        return Err(signal("error", vec![Value::string("Kill ring is empty")]));
    }

    if do_not_move {
        // Just return current without moving.
        Ok(Value::string(eval.kill_ring.current().unwrap_or("")))
    } else {
        let text = eval.kill_ring.rotate(n).unwrap_or("").to_string();
        Ok(Value::string(text))
    }
}

/// `(kill-region BEG END &optional REGION)` — kill (cut) text between BEG and END.
pub(crate) fn builtin_kill_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("kill-region", &args, 2)?;
    let beg_val = expect_int(&args[0])?;
    let end_val = expect_int(&args[1])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let (beg, end) = resolve_region(buf, beg_val, end_val);
    let text = buf.buffer_substring(beg, end);

    eval.kill_ring.push(text);
    eval.kill_ring.last_was_yank = false;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(beg, end);

    Ok(Value::Nil)
}

/// `(kill-ring-save BEG END &optional REGION)` — save region to kill ring without deleting.
pub(crate) fn builtin_kill_ring_save(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("kill-ring-save", &args, 2)?;
    let beg_val = expect_int(&args[0])?;
    let end_val = expect_int(&args[1])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let (beg, end) = resolve_region(buf, beg_val, end_val);
    let text = buf.buffer_substring(beg, end);

    eval.kill_ring.push(text);
    eval.kill_ring.last_was_yank = false;
    Ok(Value::Nil)
}

/// `(copy-region-as-kill BEG END)` — copy region to kill ring (alias).
pub(crate) fn builtin_copy_region_as_kill(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("copy-region-as-kill", &args, 2)?;
    builtin_kill_ring_save(eval, args)
}

/// `(kill-line &optional ARG)` — kill to end of line (or ARG lines forward).
pub(crate) fn builtin_kill_line(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pt = buf.point();
    let pmax = buf.point_max();

    let arg = if args.is_empty() || args[0].is_nil() {
        None
    } else {
        Some(expect_int(&args[0])?)
    };

    // Compute the end of kill region.
    let kill_end = match arg {
        None => {
            // Kill to end of line.  If at end of line (next char is \n), kill the newline too.
            let text = buf.buffer_substring(pt, pmax);
            if let Some(nl_offset) = text.find('\n') {
                if nl_offset == 0 {
                    // At a newline — kill the newline character.
                    pt + 1
                } else {
                    // Kill up to (but not including) the newline.
                    pt + nl_offset
                }
            } else {
                // No newline found — kill to end of buffer.
                pmax
            }
        }
        Some(n) if n > 0 => {
            // Kill N lines forward (including their newlines).
            let text = buf.buffer_substring(pt, pmax);
            let mut pos = 0;
            let mut lines_killed = 0;
            for (i, ch) in text.char_indices() {
                if ch == '\n' {
                    lines_killed += 1;
                    if lines_killed >= n {
                        pos = i + 1; // past the newline
                        break;
                    }
                }
                pos = i + ch.len_utf8();
            }
            pt + pos
        }
        Some(n) if n == 0 => {
            // Kill to beginning of line.
            let text = buf.buffer_substring(buf.point_min(), pt);
            if let Some(nl_pos) = text.rfind('\n') {
                buf.point_min() + nl_pos + 1
            } else {
                buf.point_min()
            }
        }
        Some(n) => {
            // n < 0: Kill backward N lines.
            let text = buf.buffer_substring(buf.point_min(), pt);
            let mut pos = text.len();
            let count = (-n) as usize;
            let mut lines_killed = 0;
            for (i, ch) in text.char_indices().rev() {
                if ch == '\n' {
                    lines_killed += 1;
                    if lines_killed >= count {
                        pos = i + 1;
                        break;
                    }
                }
                pos = i;
            }
            buf.point_min() + pos
        }
    };

    // Determine actual beg and end.
    let (kill_beg, kill_end) = if arg.map_or(false, |n| n <= 0) {
        (kill_end, pt)
    } else {
        (pt, kill_end)
    };

    if kill_beg == kill_end {
        // Nothing to kill — but don't error, just do nothing.
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let killed_text = buf.buffer_substring(kill_beg, kill_end);

    eval.kill_ring.push(killed_text);
    eval.kill_ring.last_was_yank = false;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(kill_beg, kill_end);

    Ok(Value::Nil)
}

/// `(kill-whole-line &optional ARG)` — kill the entire current line.
pub(crate) fn builtin_kill_whole_line(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let n = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        expect_int(&args[0])?
    };

    let pt = buf.point();
    let pmin = buf.point_min();
    let pmax = buf.point_max();

    // Find start of current line.
    let text_before = buf.buffer_substring(pmin, pt);
    let line_start = if let Some(nl_pos) = text_before.rfind('\n') {
        pmin + nl_pos + 1
    } else {
        pmin
    };

    // Find end of current line (including newline).
    let text_after = buf.buffer_substring(pt, pmax);
    let line_end_offset = if let Some(nl_pos) = text_after.find('\n') {
        nl_pos + 1
    } else {
        text_after.len()
    };
    let mut kill_end = pt + line_end_offset;

    // If n > 1, extend forward by (n-1) more lines.
    if n > 1 {
        let remaining = buf.buffer_substring(kill_end, pmax);
        let mut extra_lines = 0i64;
        for (i, ch) in remaining.char_indices() {
            if ch == '\n' {
                extra_lines += 1;
                if extra_lines >= n - 1 {
                    kill_end = kill_end + i + 1;
                    break;
                }
            }
            if extra_lines < n - 1 {
                kill_end = kill_end + i + ch.len_utf8();
            }
        }
        if extra_lines < n - 1 {
            kill_end = pmax;
        }
    } else if n < 0 {
        // Kill backward: kill current line and abs(n)-1 lines above.
        let full_text_before = buf.buffer_substring(pmin, line_start);
        let mut kill_start = line_start;
        let count = (-n) as usize;
        let mut lines_killed = 0;
        for (i, ch) in full_text_before.char_indices().rev() {
            if ch == '\n' {
                lines_killed += 1;
                if lines_killed >= count {
                    kill_start = pmin + i + 1;
                    break;
                }
            }
            kill_start = pmin + i;
        }
        if lines_killed < count {
            kill_start = pmin;
        }
        let killed_text = buf.buffer_substring(kill_start, kill_end);
        eval.kill_ring.push(killed_text);
        eval.kill_ring.last_was_yank = false;

        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.delete_region(kill_start, kill_end);
        return Ok(Value::Nil);
    }

    kill_end = kill_end.min(pmax);

    let killed_text = buf.buffer_substring(line_start, kill_end);
    eval.kill_ring.push(killed_text);
    eval.kill_ring.last_was_yank = false;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(line_start, kill_end);

    Ok(Value::Nil)
}

/// `(kill-word ARG)` — kill characters forward until encountering the end of a word.
pub(crate) fn builtin_kill_word(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("kill-word", &args, 1)?;
    let n = expect_int(&args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let table = buf.syntax_table.clone();
    let pt = buf.point();
    let target = forward_word(buf, &table, n);

    let (beg, end) = if target >= pt {
        (pt, target)
    } else {
        (target, pt)
    };
    let killed_text = buf.buffer_substring(beg, end);

    eval.kill_ring.push(killed_text);
    eval.kill_ring.last_was_yank = false;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(beg, end);

    Ok(Value::Nil)
}

/// `(backward-kill-word ARG)` — kill characters backward until encountering the start of a word.
pub(crate) fn builtin_backward_kill_word(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("backward-kill-word", &args, 1)?;
    let n = expect_int(&args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let table = buf.syntax_table.clone();
    let pt = buf.point();
    let target = backward_word(buf, &table, n);

    let (beg, end) = if target <= pt {
        (target, pt)
    } else {
        (pt, target)
    };
    let killed_text = buf.buffer_substring(beg, end);

    eval.kill_ring.push(killed_text);
    eval.kill_ring.last_was_yank = false;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(beg, end);

    Ok(Value::Nil)
}

/// `(yank &optional ARG)` — reinsert the last stretch of killed text.
pub(crate) fn builtin_yank(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    // If ARG is given, rotate kill ring first.
    if !args.is_empty() && args[0].is_truthy() {
        let n = expect_int(&args[0])?;
        if n != 0 {
            eval.kill_ring.rotate(n - 1);
        }
    }

    let text = eval
        .kill_ring
        .current()
        .ok_or_else(|| signal("error", vec![Value::string("Kill ring is empty")]))?
        .to_string();

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

    let start = buf.point();
    buf.insert(&text);
    let end = buf.point();

    // Set mark at the beginning of the yanked text.
    buf.set_mark(start);

    eval.kill_ring.last_was_yank = true;
    eval.kill_ring.last_yank_region = Some((start, end));

    Ok(Value::Nil)
}

/// `(yank-pop &optional ARG)` — replace yanked text with an older kill.
pub(crate) fn builtin_yank_pop(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if !eval.kill_ring.last_was_yank {
        return Err(signal(
            "error",
            vec![Value::string("Previous command was not a yank")],
        ));
    }

    let n = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        expect_int(&args[0])?
    };

    let (old_start, old_end) = eval
        .kill_ring
        .last_yank_region
        .ok_or_else(|| signal("error", vec![Value::string("No previous yank")]))?;

    // Rotate to get new text.
    let new_text = eval
        .kill_ring
        .rotate(n)
        .ok_or_else(|| signal("error", vec![Value::string("Kill ring is empty")]))?
        .to_string();

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

    // Delete the previously yanked text.
    buf.delete_region(old_start, old_end);
    // Point is now at old_start.
    buf.goto_char(old_start);
    buf.insert(&new_text);
    let new_end = buf.point();

    buf.set_mark(old_start);

    eval.kill_ring.last_was_yank = true;
    eval.kill_ring.last_yank_region = Some((old_start, new_end));

    Ok(Value::Nil)
}

// ===========================================================================
// Case commands
// ===========================================================================

/// `(downcase-region BEG END &optional REGION-NONCONTIGUOUS-P)` — convert the
/// region to lower case.
pub(crate) fn builtin_downcase_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("downcase-region", &args, 2, 3)?;
    let beg_val = expect_int(&args[0])?;
    let end_val = expect_int(&args[1])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let (beg, end) = resolve_case_region(eval, beg_val, end_val, args.get(2))?;
    let text = buf.buffer_substring(beg, end);
    let lower = text.to_lowercase();

    if text == lower {
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let saved_pt = buf.point();
    buf.delete_region(beg, end);
    buf.goto_char(beg);
    buf.insert(&lower);
    buf.goto_char(saved_pt.min(buf.point_max()));

    Ok(Value::Nil)
}

/// `(upcase-region BEG END &optional REGION-NONCONTIGUOUS-P)` — convert the
/// region to upper case.
pub(crate) fn builtin_upcase_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("upcase-region", &args, 2, 3)?;
    let beg_val = expect_int(&args[0])?;
    let end_val = expect_int(&args[1])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let (beg, end) = resolve_case_region(eval, beg_val, end_val, args.get(2))?;
    let text = buf.buffer_substring(beg, end);
    let upper = text.to_uppercase();

    if text == upper {
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let saved_pt = buf.point();
    buf.delete_region(beg, end);
    buf.goto_char(beg);
    buf.insert(&upper);
    buf.goto_char(saved_pt.min(buf.point_max()));

    Ok(Value::Nil)
}

/// `(capitalize-region BEG END &optional REGION-NONCONTIGUOUS-P)` — capitalize
/// each word in the region.
pub(crate) fn builtin_capitalize_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("capitalize-region", &args, 2, 3)?;
    let beg_val = expect_int(&args[0])?;
    let end_val = expect_int(&args[1])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let (beg, end) = resolve_case_region(eval, beg_val, end_val, args.get(2))?;
    let text = buf.buffer_substring(beg, end);
    let result = capitalize_words_preserving_boundaries(&text);

    if text == result {
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let saved_pt = buf.point();
    buf.delete_region(beg, end);
    buf.goto_char(beg);
    buf.insert(&result);
    buf.goto_char(saved_pt.min(buf.point_max()));

    Ok(Value::Nil)
}

fn capitalize_words_preserving_boundaries(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_word = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if !in_word {
                for c in ch.to_uppercase() {
                    result.push(c);
                }
                in_word = true;
            } else {
                for c in ch.to_lowercase() {
                    result.push(c);
                }
            }
        } else {
            result.push(ch);
            in_word = false;
        }
    }
    result
}

/// `(upcase-initials-region BEG END)` — uppercase first char of each word in region.
pub(crate) fn builtin_upcase_initials_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("upcase-initials-region", &args, 2)?;
    let beg_val = expect_int(&args[0])?;
    let end_val = expect_int(&args[1])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let (beg, end) = resolve_region(buf, beg_val, end_val);
    let text = buf.buffer_substring(beg, end);

    let mut result = String::with_capacity(text.len());
    let mut in_word = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if !in_word {
                for c in ch.to_uppercase() {
                    result.push(c);
                }
                in_word = true;
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
            in_word = false;
        }
    }

    if text == result {
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let saved_pt = buf.point();
    buf.delete_region(beg, end);
    buf.goto_char(beg);
    buf.insert(&result);
    buf.goto_char(saved_pt.min(buf.point_max()));

    Ok(Value::Nil)
}

/// `(downcase-word ARG)` — convert next ARG words to lower case, moving point past them.
pub(crate) fn builtin_downcase_word(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("downcase-word", &args, 1)?;
    let n = expect_int(&args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let table = buf.syntax_table.clone();
    let pt = buf.point();
    let target = forward_word(buf, &table, n);

    let (beg, end) = if target >= pt {
        (pt, target)
    } else {
        (target, pt)
    };
    let text = buf.buffer_substring(beg, end);
    let lower = text.to_lowercase();

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(beg, end);
    buf.goto_char(beg);
    buf.insert(&lower);
    // Point ends after the replacement.

    Ok(Value::Nil)
}

/// `(upcase-word ARG)` — convert next ARG words to upper case, moving point past them.
pub(crate) fn builtin_upcase_word(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("upcase-word", &args, 1)?;
    let n = expect_int(&args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let table = buf.syntax_table.clone();
    let pt = buf.point();
    let target = forward_word(buf, &table, n);

    let (beg, end) = if target >= pt {
        (pt, target)
    } else {
        (target, pt)
    };
    let text = buf.buffer_substring(beg, end);
    let upper = text.to_uppercase();

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(beg, end);
    buf.goto_char(beg);
    buf.insert(&upper);

    Ok(Value::Nil)
}

/// `(capitalize-word ARG)` — capitalize the next ARG words, moving point past them.
pub(crate) fn builtin_capitalize_word(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("capitalize-word", &args, 1)?;
    let n = expect_int(&args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let table = buf.syntax_table.clone();
    let pt = buf.point();
    let target = forward_word(buf, &table, n);

    let (beg, end) = if target >= pt {
        (pt, target)
    } else {
        (target, pt)
    };
    let text = buf.buffer_substring(beg, end);
    let result = capitalize_words_preserving_boundaries(&text);

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(beg, end);
    buf.goto_char(beg);
    buf.insert(&result);

    Ok(Value::Nil)
}

// ===========================================================================
// Transpose commands
// ===========================================================================

/// `(transpose-chars ARG)` — interchange characters around point.
pub(crate) fn builtin_transpose_chars(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("transpose-chars", &args, 1)?;
    let _n = expect_int(&args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pt = buf.point();
    let pmin = buf.point_min();
    let pmax = buf.point_max();

    // Need at least 2 characters.
    if pmax - pmin < 2 {
        return Err(signal(
            "error",
            vec![Value::string("Buffer too small to transpose")],
        ));
    }

    // If at end of buffer, transpose the two characters before point.
    // Otherwise, transpose the character before and after point.
    let (pos_a, pos_b) = if pt >= pmax {
        // At end: swap the two chars before point.
        let ch_b = buf
            .char_before(pt)
            .ok_or_else(|| signal("error", vec![Value::string("Beginning of buffer")]))?;
        let before_b = pt - ch_b.len_utf8();
        let ch_a = buf
            .char_before(before_b)
            .ok_or_else(|| signal("error", vec![Value::string("Beginning of buffer")]))?;
        let before_a = before_b - ch_a.len_utf8();
        (before_a, before_b)
    } else if pt <= pmin {
        return Err(signal("error", vec![Value::string("Beginning of buffer")]));
    } else {
        let ch_before = buf
            .char_before(pt)
            .ok_or_else(|| signal("error", vec![Value::string("Beginning of buffer")]))?;
        (pt - ch_before.len_utf8(), pt)
    };

    let text_a = buf.buffer_substring(pos_a, pos_b);
    let text_b_end = {
        let ch = buf.char_after(pos_b);
        match ch {
            Some(c) => pos_b + c.len_utf8(),
            None => return Err(signal("error", vec![Value::string("End of buffer")])),
        }
    };
    let text_b = buf.buffer_substring(pos_b, text_b_end);

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Replace the range [pos_a, text_b_end) with text_b + text_a.
    buf.delete_region(pos_a, text_b_end);
    buf.goto_char(pos_a);
    buf.insert(&text_b);
    buf.insert(&text_a);

    Ok(Value::Nil)
}

/// `(transpose-words ARG)` — interchange words around point.
pub(crate) fn builtin_transpose_words(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("transpose-words", &args, 1)?;
    let n = expect_int(&args[0])?;
    if n == 0 {
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let steps = n.unsigned_abs() as usize;
    let backward = n < 0;
    let two_word_error =
        || signal("error", vec![Value::string("Don\u{2019}t have two things to transpose")]);

    for _ in 0..steps {
        let (w1s, w1e, w2s, w2e) = {
            let buf = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

            let table = buf.syntax_table.clone();
            let pt = buf.point();
            let pmin = buf.point_min();
            let pmax = buf.point_max();
            let is_word_char = |ch: char| matches!(table.char_syntax(ch), SyntaxClass::Word);

            let retreat_to_word_start = |start_pos: usize| -> Option<usize> {
                let mut pos = start_pos;
                while pos > pmin {
                    let ch = buf.char_before(pos)?;
                    if !is_word_char(ch) {
                        break;
                    }
                    pos -= ch.len_utf8();
                }
                if pos < pmax {
                    let ch = buf.char_after(pos)?;
                    if is_word_char(ch) {
                        return Some(pos);
                    }
                }
                None
            };

            let word_start_at_or_after = |start_pos: usize| -> Option<usize> {
                let mut pos = start_pos;
                while pos < pmax {
                    let ch = buf.char_after(pos)?;
                    if is_word_char(ch) {
                        return Some(pos);
                    }
                    pos += ch.len_utf8();
                }
                None
            };

            let word_start_before = |start_pos: usize| -> Option<usize> {
                let mut pos = start_pos;
                while pos > pmin {
                    let ch = buf.char_before(pos)?;
                    pos -= ch.len_utf8();
                    if is_word_char(ch) {
                        return retreat_to_word_start(pos);
                    }
                }
                None
            };

            let word_end_from_start = |start_pos: usize| -> usize {
                let mut pos = start_pos;
                while pos < pmax {
                    let Some(ch) = buf.char_after(pos) else {
                        break;
                    };
                    if !is_word_char(ch) {
                        break;
                    }
                    pos += ch.len_utf8();
                }
                pos
            };

            if backward {
                let pivot = if pt > pmin && buf.char_before(pt).is_some_and(|ch| is_word_char(ch)) {
                    retreat_to_word_start(pt)
                } else if pt < pmax && buf.char_after(pt).is_some_and(|ch| is_word_char(ch)) {
                    retreat_to_word_start(pt)
                } else {
                    word_start_before(pt)
                }
                .ok_or_else(&two_word_error)?;
                let prev = word_start_before(pivot).ok_or_else(&two_word_error)?;
                let prev_end = word_end_from_start(prev);
                let pivot_end = word_end_from_start(pivot);
                (prev, prev_end, pivot, pivot_end)
            } else {
                let pivot = if pt > pmin && buf.char_before(pt).is_some_and(|ch| is_word_char(ch)) {
                    retreat_to_word_start(pt)
                } else if pt < pmax && buf.char_after(pt).is_some_and(|ch| is_word_char(ch)) {
                    retreat_to_word_start(pt)
                } else {
                    word_start_at_or_after(pt)
                }
                .ok_or_else(&two_word_error)?;

                let pivot_end = word_end_from_start(pivot);
                if let Some(prev) = word_start_before(pivot) {
                    let prev_end = word_end_from_start(prev);
                    (prev, prev_end, pivot, pivot_end)
                } else {
                    let next = word_start_at_or_after(pivot_end).ok_or_else(&two_word_error)?;
                    let next_end = word_end_from_start(next);
                    (pivot, pivot_end, next, next_end)
                }
            }
        };

        let (w1_text, between, w2_text) = {
            let buf_r = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            (
                buf_r.buffer_substring(w1s, w1e),
                buf_r.buffer_substring(w1e, w2s),
                buf_r.buffer_substring(w2s, w2e),
            )
        };

        let buf_m = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

        // Replace the whole range [w1s, w2e) with w2 + between + w1.
        buf_m.delete_region(w1s, w2e);
        buf_m.goto_char(w1s);
        buf_m.insert(&w2_text);
        buf_m.insert(&between);
        buf_m.insert(&w1_text);

        // Keep point at end of the first transposed word.
        buf_m.goto_char(w1s + w2_text.len() + between.len());
    }

    Ok(Value::Nil)
}

/// `(transpose-sexps ARG)` — interchange sexps around point.
pub(crate) fn builtin_transpose_sexps(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("transpose-sexps", &args, 1)?;
    let n = expect_int(&args[0])?;
    if n == 0 {
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let two_sexp_error =
        || signal("error", vec![Value::string("Don\u{2019}t have two things to transpose")]);

    let (point, table, spans, scan_error_pos) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let table = buf.syntax_table.clone();
        let spans = collect_sexp_spans(buf, &table);
        let point = buf.point();
        let scan_error_pos = spans
            .iter()
            .find_map(|(start, end)| (point > *start && point < *end).then_some(*start));
        (point, table, spans, scan_error_pos)
    };

    if let Some(start) = scan_error_pos {
        return Err(signal(
            "scan-error",
            vec![
                Value::string("Containing expression ends prematurely"),
                Value::Int((start + 1) as i64),
                Value::Int((start + 1) as i64),
            ],
        ));
    }

    let moving_idx = spans.iter().rposition(|(_, end)| *end <= point);

    // Emacs behavior: when point is effectively at BOB for positive transpose,
    // only move point over sexps and do not transpose text.
    if n > 0 && moving_idx.is_none() {
        let next_point = {
            let buf = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            move_point_by_sexps(buf, &table, point, n)
        };
        let buf_m = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf_m.goto_char(next_point);
        return Ok(Value::Nil);
    }

    let Some(i) = moving_idx else {
        return Err(two_sexp_error());
    };

    if n > 0 {
        let max_forward = spans.len().saturating_sub(1).saturating_sub(i);
        if max_forward == 0 {
            return Err(two_sexp_error());
        }
        let shift = usize::min(n as usize, max_forward);
        let dest_idx = i + shift;
        let (start_i, end_i) = spans[i];
        let (start_next, _) = spans[i + 1];
        let (_, end_dest) = spans[dest_idx];

        let (moving, block, separator) = {
            let buf_r = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            (
                buf_r.buffer_substring(start_i, end_i),
                buf_r.buffer_substring(start_next, end_dest),
                buf_r.buffer_substring(end_i, start_next),
            )
        };

        let new_point = end_dest;
        let buf_m = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf_m.delete_region(start_i, end_dest);
        buf_m.goto_char(start_i);
        buf_m.insert(&block);
        buf_m.insert(&separator);
        buf_m.insert(&moving);
        buf_m.goto_char(new_point);
        return Ok(Value::Nil);
    }

    let max_backward = i;
    if max_backward == 0 {
        return Err(two_sexp_error());
    }
    let shift = usize::min(n.unsigned_abs() as usize, max_backward);
    let dest_idx = i - shift;
    let (start_dest, _) = spans[dest_idx];
    let (_, end_prev) = spans[i - 1];
    let (start_i, end_i) = spans[i];

    let (moving, separator, block) = {
        let buf_r = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf_r.buffer_substring(start_i, end_i),
            buf_r.buffer_substring(end_prev, start_i),
            buf_r.buffer_substring(start_dest, end_prev),
        )
    };

    let new_point = start_dest + moving.len();
    let buf_m = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf_m.delete_region(start_dest, end_i);
    buf_m.goto_char(start_dest);
    buf_m.insert(&moving);
    buf_m.insert(&separator);
    buf_m.insert(&block);
    buf_m.goto_char(new_point);

    Ok(Value::Nil)
}

/// `(transpose-sentences ARG)` — interchange sentences around point.
pub(crate) fn builtin_transpose_sentences(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("transpose-sentences", &args, 1)?;
    let n = expect_int(&args[0])?;
    if n == 0 {
        return Ok(Value::Nil);
    }

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let steps = n.unsigned_abs() as usize;
    let backward = n < 0;

    for _ in 0..steps {
        let (s1, e1, s2, e2) = {
            let buf = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            let spans = collect_sentence_spans(buf);
            if spans.is_empty() {
                return Err(signal("end-of-buffer", vec![]));
            }
            let pt = buf.point();
            let pivot_idx = spans
                .iter()
                .enumerate()
                .find_map(|(idx, (start, end))| {
                    if (pt >= *start && pt <= *end) || pt < *start {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| signal("end-of-buffer", vec![]))?;

            let (first_idx, second_idx) = if backward {
                if pivot_idx == 0 {
                    return Err(signal("beginning-of-buffer", vec![]));
                }
                (pivot_idx - 1, pivot_idx)
            } else if pivot_idx > 0 {
                (pivot_idx - 1, pivot_idx)
            } else if spans.len() > 1 {
                (0, 1)
            } else {
                return Err(signal("end-of-buffer", vec![]));
            };

            let (first_start, first_end) = spans[first_idx];
            let (second_start, second_end) = spans[second_idx];
            (first_start, first_end, second_start, second_end)
        };

        let (sentence_a, between, sentence_b) = {
            let buf_r = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            (
                buf_r.buffer_substring(s1, e1),
                buf_r.buffer_substring(e1, s2),
                buf_r.buffer_substring(s2, e2),
            )
        };

        let buf_m = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf_m.delete_region(s1, e2);
        buf_m.goto_char(s1);
        buf_m.insert(&sentence_b);
        buf_m.insert(&between);
        buf_m.insert(&sentence_a);
        buf_m.goto_char(s1 + sentence_b.len() + between.len() + sentence_a.len());
    }

    Ok(Value::Nil)
}

/// `(transpose-paragraphs ARG)` — interchange paragraphs around point.
pub(crate) fn builtin_transpose_paragraphs(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("transpose-paragraphs", &args, 1)?;
    let n = expect_int(&args[0])?;
    if n == 0 {
        return Ok(Value::Nil);
    }

    let (pmin, pmax, point, forward_boundaries, backward_boundaries) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        if buf.read_only {
            return Err(signal(
                "buffer-read-only",
                vec![Value::string(buf.name.clone())],
            ));
        }
        let (forward, backward) = collect_paragraph_boundaries(buf);
        (
            buf.point_min(),
            buf.point_max(),
            buf.point(),
            forward,
            backward,
        )
    };

    let mut cursor = point;
    let (pos1, after_pos1) = paragraph_aux_position(
        cursor,
        -1,
        pmin,
        pmax,
        &forward_boundaries,
        &backward_boundaries,
    );
    cursor = after_pos1;

    let target_point = if n > 0 {
        let (pos2, _) = paragraph_aux_position(
            cursor,
            n,
            pmin,
            pmax,
            &forward_boundaries,
            &backward_boundaries,
        );
        transpose_subr_ranges(eval, pos1, pos2)?;
        pos2.0
    } else {
        cursor = pos1.0;
        let (pos2, _) = paragraph_aux_position(
            cursor,
            n,
            pmin,
            pmax,
            &forward_boundaries,
            &backward_boundaries,
        );
        let pos1_len = pos1.1 as isize - pos1.0 as isize;
        transpose_subr_ranges(eval, pos1, pos2)?;
        (pos2.0 as isize + pos1_len).max(pmin as isize) as usize
    };

    let buf_m = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf_m.goto_char(target_point);
    Ok(Value::Nil)
}

/// `(transpose-lines ARG)` — interchange lines around point.
pub(crate) fn builtin_transpose_lines(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("transpose-lines", &args, 1)?;
    let _n = expect_int(&args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pt = buf.point();
    let pmin = buf.point_min();
    let pmax = buf.point_max();

    // Find current line boundaries.
    let text_before = buf.buffer_substring(pmin, pt);
    let cur_line_start = if let Some(nl_pos) = text_before.rfind('\n') {
        pmin + nl_pos + 1
    } else {
        pmin
    };

    let text_from_pt = buf.buffer_substring(pt, pmax);
    let cur_line_end = if let Some(nl_pos) = text_from_pt.find('\n') {
        pt + nl_pos + 1 // Include the newline.
    } else {
        pmax
    };

    // Emacs special-cases BOB: transpose current line with following line,
    // and with an empty line when there is no following line.
    if cur_line_start == pmin {
        let next_line_start = cur_line_end;
        let next_line_end = if next_line_start < pmax {
            let text_from_next = buf.buffer_substring(next_line_start, pmax);
            if let Some(nl_pos) = text_from_next.find('\n') {
                next_line_start + nl_pos + 1
            } else {
                pmax
            }
        } else {
            cur_line_end
        };

        let cur_line_text = buf.buffer_substring(cur_line_start, cur_line_end);
        let next_line_text = if next_line_start < pmax {
            buf.buffer_substring(next_line_start, next_line_end)
        } else {
            String::new()
        };

        let cur_no_nl = cur_line_text.strip_suffix('\n').unwrap_or(&cur_line_text);
        let next_no_nl = next_line_text.strip_suffix('\n').unwrap_or(&next_line_text);
        let replacement = format!("{next_no_nl}\n{cur_no_nl}\n");

        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.delete_region(cur_line_start, next_line_end);
        buf.goto_char(cur_line_start);
        buf.insert(&replacement);
        return Ok(Value::Nil);
    }

    let text_before_cur = buf.buffer_substring(pmin, cur_line_start);
    let prev_line_start = if let Some(nl_pos) =
        text_before_cur[..text_before_cur.len().saturating_sub(1)].rfind('\n')
    {
        pmin + nl_pos + 1
    } else {
        pmin
    };

    let prev_line_text = buf.buffer_substring(prev_line_start, cur_line_start);
    let cur_line_text = buf.buffer_substring(cur_line_start, cur_line_end);

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Replace [prev_line_start, cur_line_end) with cur_line + prev_line.
    buf.delete_region(prev_line_start, cur_line_end);
    buf.goto_char(prev_line_start);
    buf.insert(&cur_line_text);
    buf.insert(&prev_line_text);

    Ok(Value::Nil)
}

// ===========================================================================
// Indent / newline commands
// ===========================================================================

/// `(indent-line-to COLUMN)` — indent current line to COLUMN, deleting existing indentation.
pub(crate) fn builtin_indent_line_to(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("indent-line-to", &args, 1)?;
    let column = expect_indent_column(&args[0])?.max(0) as usize;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pt = buf.point();
    let pmin = buf.point_min();

    // Find line start.
    let text_before = buf.buffer_substring(pmin, pt);
    let line_start = if let Some(nl_pos) = text_before.rfind('\n') {
        pmin + nl_pos + 1
    } else {
        pmin
    };

    // Find end of existing indentation (spaces/tabs from line_start).
    let pmax = buf.point_max();
    let line_text = buf.buffer_substring(line_start, pmax);
    let indent_end_offset = line_text
        .chars()
        .take_while(|&c| c == ' ' || c == '\t')
        .map(|c| c.len_utf8())
        .sum::<usize>();
    let indent_end = line_start + indent_end_offset;

    let new_indent: String = " ".repeat(column);

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(line_start, indent_end);
    buf.goto_char(line_start);
    buf.insert(&new_indent);

    Ok(Value::Nil)
}

/// `(indent-to COLUMN &optional MINIMUM)` — indent from point to COLUMN.
/// Insert at least MINIMUM spaces (default 0).
pub(crate) fn builtin_indent_to(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("indent-to", &args, 1)?;
    let column = expect_int(&args[0])?.max(0) as usize;
    let minimum = if args.len() > 1 && args[1].is_truthy() {
        expect_int(&args[1])?.max(0) as usize
    } else {
        0
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pt = buf.point();
    let pmin = buf.point_min();

    // Compute current column (simple: count chars from line start).
    let text_before = buf.buffer_substring(pmin, pt);
    let cur_col = if let Some(nl_pos) = text_before.rfind('\n') {
        text_before.len() - nl_pos - 1
    } else {
        text_before.len()
    };

    let spaces_needed = if column > cur_col {
        (column - cur_col).max(minimum)
    } else {
        minimum
    };

    if spaces_needed > 0 {
        let spaces: String = " ".repeat(spaces_needed);
        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.insert(&spaces);
    }

    Ok(Value::Int(column as i64))
}

/// `(newline &optional ARG INTERACTIVE)` — insert one or more newlines.
pub(crate) fn builtin_newline(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1usize
    } else {
        expect_int(&args[0])?.max(0) as usize
    };

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

    let newlines: String = "\n".repeat(n);
    buf.insert(&newlines);

    Ok(Value::Nil)
}

/// `(newline-and-indent &optional ARG)` — insert a newline then indent.
pub(crate) fn builtin_newline_and_indent(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    // Insert newline(s).
    builtin_newline(eval, args)?;

    // Simple indentation: copy the indentation of the previous line.
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let pt = buf.point();
    let pmin = buf.point_min();

    // Find previous line.
    let text_before = buf.buffer_substring(pmin, pt);
    // pt is right after the newline we just inserted. Find the line before that.
    let prev_nl = if text_before.len() > 1 {
        text_before[..text_before.len() - 1].rfind('\n')
    } else {
        None
    };

    let prev_line_start = match prev_nl {
        Some(pos) => pos + 1,
        None => 0,
    };
    let prev_line = &text_before[prev_line_start..text_before.len().saturating_sub(1)];

    // Extract leading whitespace.
    let indent: String = prev_line
        .chars()
        .take_while(|&c| c == ' ' || c == '\t')
        .collect();

    if !indent.is_empty() {
        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.insert(&indent);
    }

    Ok(Value::Nil)
}

/// `(open-line N)` — insert N newlines after point, leaving point before them.
pub(crate) fn builtin_open_line(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_max_args("open-line", &args, 1)?;
    let n = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        expect_int(&args[0])?
    };

    if n < 0 {
        return Err(signal(
            "error",
            vec![Value::string("open-line expects a non-negative count")],
        ));
    }

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

    let pt = buf.point();
    if n > 0 {
        let newlines = "\n".repeat(n as usize);
        buf.insert(&newlines);
        buf.goto_char(pt);
    }

    Ok(Value::Nil)
}

/// `(delete-horizontal-space &optional BACKWARD-ONLY)` —
/// delete spaces/tabs around point (or only before point when BACKWARD-ONLY).
pub(crate) fn builtin_delete_horizontal_space(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("delete-horizontal-space", &args, 1)?;
    let backward_only = args.first().is_some_and(Value::is_truthy);

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pmin = buf.point_min();
    let pmax = buf.point_max();
    let pt = buf.point();

    let mut left = pt;
    while left > pmin {
        let Some(ch) = buf.char_before(left) else {
            break;
        };
        if !is_horizontal_space(ch) {
            break;
        }
        left -= ch.len_utf8();
    }

    let mut right = pt;
    if !backward_only {
        while right < pmax {
            let Some(ch) = buf.char_after(right) else {
                break;
            };
            if !is_horizontal_space(ch) {
                break;
            }
            right += ch.len_utf8();
        }
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(left, right);
    Ok(Value::Nil)
}

/// `(just-one-space &optional N)` —
/// replace surrounding spaces/tabs with exactly |N| spaces (default 1).
pub(crate) fn builtin_just_one_space(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("just-one-space", &args, 1)?;
    let count = if args.is_empty() || args[0].is_nil() {
        1usize
    } else {
        let raw = expect_int(&args[0])?;
        if raw == i64::MIN {
            i64::MAX as usize
        } else {
            raw.unsigned_abs() as usize
        }
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pmin = buf.point_min();
    let pmax = buf.point_max();
    let pt = buf.point();

    let mut left = pt;
    while left > pmin {
        let Some(ch) = buf.char_before(left) else {
            break;
        };
        if !is_horizontal_space(ch) {
            break;
        }
        left -= ch.len_utf8();
    }

    let mut right = pt;
    while right < pmax {
        let Some(ch) = buf.char_after(right) else {
            break;
        };
        if !is_horizontal_space(ch) {
            break;
        }
        right += ch.len_utf8();
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(left, right);
    if count > 0 {
        buf.insert(&" ".repeat(count));
    }
    Ok(Value::Nil)
}

/// `(delete-indentation &optional ARG BEG END)` — join this line to the previous one.
/// With ARG, join this line to the following one.
pub(crate) fn builtin_delete_indentation(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let join_following = !args.is_empty() && args[0].is_truthy();

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let pt = buf.point();
    let pmin = buf.point_min();
    let pmax = buf.point_max();

    if join_following {
        // Move to the beginning of the next line, then join.
        let text_after = buf.buffer_substring(pt, pmax);
        let nl_pos = text_after.find('\n');
        match nl_pos {
            None => return Ok(Value::Nil), // No next line.
            Some(offset) => {
                let nl_byte = pt + offset;
                // Find end of indentation on next line.
                let next_line_start = nl_byte + 1;
                let rest = buf.buffer_substring(next_line_start, pmax);
                let indent_len: usize = rest
                    .chars()
                    .take_while(|&c| c == ' ' || c == '\t')
                    .map(|c| c.len_utf8())
                    .sum();

                let buf = eval
                    .buffers
                    .current_buffer_mut()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                buf.delete_region(nl_byte, next_line_start + indent_len);
                // Insert a single space (unless at beginning of line).
                if nl_byte > pmin {
                    buf.goto_char(nl_byte);
                    buf.insert(" ");
                }
            }
        }
    } else {
        // Join current line with previous: delete newline and indentation at start of current line.
        let text_before = buf.buffer_substring(pmin, pt);
        let cur_line_start = if let Some(nl_pos) = text_before.rfind('\n') {
            pmin + nl_pos + 1
        } else {
            return Ok(Value::Nil); // Already on first line.
        };

        // The newline is at cur_line_start - 1.
        let nl_byte = cur_line_start - 1;

        // Find end of indentation on current line.
        let line_text = buf.buffer_substring(cur_line_start, pmax);
        let indent_len: usize = line_text
            .chars()
            .take_while(|&c| c == ' ' || c == '\t')
            .map(|c| c.len_utf8())
            .sum();

        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.delete_region(nl_byte, cur_line_start + indent_len);
        // Insert a single space.
        buf.goto_char(nl_byte);
        buf.insert(" ");
    }

    Ok(Value::Nil)
}

/// `(tab-to-tab-stop)` — insert a tab character (batch-compatible subset).
pub(crate) fn builtin_tab_to_tab_stop(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("tab-to-tab-stop", &args, 0)?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.insert("\t");

    Ok(Value::Nil)
}

/// `(indent-rigidly START END ARG)` — indent all lines in region by ARG columns.
/// Positive ARG adds spaces; negative ARG removes leading spaces.
pub(crate) fn builtin_indent_rigidly(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("indent-rigidly", &args, 3)?;
    let start_val = expect_int(&args[0])?;
    let end_val = expect_int(&args[1])?;
    let indent_arg = expect_int(&args[2])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if buf.read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buf.name.clone())],
        ));
    }

    let (start, end) = resolve_region(buf, start_val, end_val);

    // Extract the region text.
    let region_text = buf.buffer_substring(start, end);

    // Process each line.
    let mut result = String::with_capacity(region_text.len() + 64);
    for (i, line) in region_text.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        if line.is_empty() {
            continue;
        }
        if indent_arg > 0 {
            let prefix: String = " ".repeat(indent_arg as usize);
            result.push_str(&prefix);
            result.push_str(line);
        } else if indent_arg < 0 {
            // Remove up to abs(indent_arg) leading spaces.
            let remove = (-indent_arg) as usize;
            let leading_spaces = line.chars().take_while(|&c| c == ' ').count();
            let actual_remove = remove.min(leading_spaces);
            result.push_str(&line[actual_remove..]);
        } else {
            result.push_str(line);
        }
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(start, end);
    buf.goto_char(start);
    buf.insert(&result);

    Ok(Value::Nil)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use crate::elisp::{format_eval_result, parse_forms, Evaluator};

    fn eval_one(src: &str) -> String {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        let result = ev.eval_expr(&forms[0]);
        format_eval_result(&result)
    }

    fn eval_all(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        ev.eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    // -- KillRing data structure tests --

    #[test]
    fn kill_ring_push_and_current() {
        let mut kr = super::KillRing::new();
        assert!(kr.is_empty());
        kr.push("hello".to_string());
        assert_eq!(kr.current(), Some("hello"));
        assert_eq!(kr.len(), 1);
    }

    #[test]
    fn kill_ring_push_multiple() {
        let mut kr = super::KillRing::new();
        kr.push("first".to_string());
        kr.push("second".to_string());
        assert_eq!(kr.current(), Some("second"));
        assert_eq!(kr.len(), 2);
    }

    #[test]
    fn kill_ring_rotate() {
        let mut kr = super::KillRing::new();
        kr.push("a".to_string());
        kr.push("b".to_string());
        kr.push("c".to_string());
        assert_eq!(kr.current(), Some("c"));
        kr.rotate(1);
        assert_eq!(kr.current(), Some("b"));
        kr.rotate(1);
        assert_eq!(kr.current(), Some("a"));
        kr.rotate(1);
        assert_eq!(kr.current(), Some("c")); // wraps around
    }

    #[test]
    fn kill_ring_rotate_negative() {
        let mut kr = super::KillRing::new();
        kr.push("a".to_string());
        kr.push("b".to_string());
        kr.push("c".to_string());
        kr.rotate(-1);
        assert_eq!(kr.current(), Some("a"));
    }

    #[test]
    fn kill_ring_replace_top() {
        let mut kr = super::KillRing::new();
        kr.push("original".to_string());
        kr.replace_top("replaced".to_string());
        assert_eq!(kr.current(), Some("replaced"));
        assert_eq!(kr.len(), 1);
    }

    #[test]
    fn kill_ring_append() {
        let mut kr = super::KillRing::new();
        kr.push("hello".to_string());
        kr.append(" world", false);
        assert_eq!(kr.current(), Some("hello world"));
    }

    #[test]
    fn kill_ring_prepend() {
        let mut kr = super::KillRing::new();
        kr.push("world".to_string());
        kr.append("hello ", true);
        assert_eq!(kr.current(), Some("hello world"));
    }

    #[test]
    fn kill_ring_max_size() {
        let mut kr = super::KillRing::new();
        for i in 0..100 {
            kr.push(format!("item-{}", i));
        }
        assert_eq!(kr.len(), 60); // Default max is 60.
    }

    #[test]
    fn kill_ring_push_empty_ignored() {
        let mut kr = super::KillRing::new();
        kr.push("".to_string());
        assert!(kr.is_empty());
    }

    #[test]
    fn kill_ring_to_lisp_list() {
        let mut kr = super::KillRing::new();
        kr.push("a".to_string());
        kr.push("b".to_string());
        let list = kr.to_lisp_list();
        assert!(list.is_list());
    }

    // -- kill-new builtin tests --

    #[test]
    fn kill_new_basic() {
        let results = eval_all(r#"(kill-new "hello") (current-kill 0)"#);
        assert_eq!(results[0], "OK nil");
        assert_eq!(results[1], r#"OK "hello""#);
    }

    #[test]
    fn kill_new_replace() {
        let results = eval_all(r#"(kill-new "first") (kill-new "second" t) (current-kill 0)"#);
        assert_eq!(results[2], r#"OK "second""#);
    }

    #[test]
    fn kill_append_basic() {
        let results = eval_all(r#"(kill-new "hello") (kill-append " world" nil) (current-kill 0)"#);
        assert_eq!(results[2], r#"OK "hello world""#);
    }

    #[test]
    fn kill_append_before() {
        let results = eval_all(r#"(kill-new "world") (kill-append "hello " t) (current-kill 0)"#);
        assert_eq!(results[2], r#"OK "hello world""#);
    }

    #[test]
    fn current_kill_rotate() {
        let results = eval_all(
            r#"(kill-new "a") (kill-new "b") (kill-new "c")
               (current-kill 0)
               (current-kill 1)
               (current-kill 1)"#,
        );
        assert_eq!(results[3], r#"OK "c""#);
        assert_eq!(results[4], r#"OK "b""#);
        assert_eq!(results[5], r#"OK "a""#);
    }

    #[test]
    fn current_kill_empty_ring_errors() {
        let result = eval_one("(current-kill 0)");
        assert!(result.starts_with("ERR"));
    }

    // -- kill-region tests --

    #[test]
    fn kill_region_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (kill-region 1 6)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK " world""#);
    }

    #[test]
    fn kill_region_adds_to_kill_ring() {
        let results = eval_all(
            r#"(insert "hello world")
               (kill-region 1 6)
               (current-kill 0)"#,
        );
        assert_eq!(results[2], r#"OK "hello""#);
    }

    // -- kill-ring-save tests --

    #[test]
    fn kill_ring_save_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (kill-ring-save 1 6)
               (buffer-string)
               (current-kill 0)"#,
        );
        // Buffer content should be unchanged.
        assert_eq!(results[2], r#"OK "hello world""#);
        // Kill ring should have the text.
        assert_eq!(results[3], r#"OK "hello""#);
    }

    // -- kill-line tests --

    #[test]
    fn kill_line_to_end() {
        let results = eval_all(
            r#"(insert "hello\nworld")
               (goto-char 0)
               (kill-line)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "\nworld""#);
    }

    #[test]
    fn kill_line_at_newline() {
        let results = eval_all(
            r#"(insert "hello\nworld")
               (goto-char 6)
               (kill-line)
               (buffer-string)"#,
        );
        // When at the newline, kill-line should kill the newline.
        assert_eq!(results[3], r#"OK "helloworld""#);
    }

    #[test]
    fn kill_line_with_count() {
        let results = eval_all(
            r#"(insert "line1\nline2\nline3")
               (goto-char 0)
               (kill-line 2)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "line3""#);
    }

    // -- kill-whole-line tests --

    #[test]
    fn kill_whole_line_basic() {
        let results = eval_all(
            r#"(insert "line1\nline2\nline3")
               (goto-char 8)
               (kill-whole-line)
               (buffer-string)"#,
        );
        // Point is at "line2", should kill "line2\n".
        assert_eq!(results[3], r#"OK "line1\nline3""#);
    }

    // -- kill-word tests --

    #[test]
    fn kill_word_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (goto-char 0)
               (kill-word 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK " world""#);
    }

    #[test]
    fn kill_word_adds_to_ring() {
        let results = eval_all(
            r#"(insert "hello world")
               (goto-char 0)
               (kill-word 1)
               (current-kill 0)"#,
        );
        assert_eq!(results[3], r#"OK "hello""#);
    }

    // -- backward-kill-word tests --

    #[test]
    fn backward_kill_word_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (backward-kill-word 1)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "hello ""#);
    }

    // -- yank tests --

    #[test]
    fn yank_basic() {
        let results = eval_all(
            r#"(kill-new "inserted")
               (yank)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "inserted""#);
    }

    #[test]
    fn yank_with_arg() {
        let results = eval_all(
            r#"(kill-new "first")
               (kill-new "second")
               (yank 2)
               (buffer-string)"#,
        );
        // yank with arg 2 should insert the second-most-recent kill (i.e. "first").
        assert_eq!(results[3], r#"OK "first""#);
    }

    #[test]
    fn yank_empty_ring_errors() {
        let result = eval_one("(yank)");
        assert!(result.starts_with("ERR"));
    }

    // -- yank-pop tests --

    #[test]
    fn yank_pop_basic() {
        let results = eval_all(
            r#"(kill-new "first")
               (kill-new "second")
               (yank)
               (yank-pop)
               (buffer-string)"#,
        );
        // After yank-pop, "second" should be replaced by "first".
        assert_eq!(results[4], r#"OK "first""#);
    }

    #[test]
    fn yank_pop_without_yank_errors() {
        let results = eval_all(r#"(kill-new "hello") (yank-pop)"#);
        assert!(results[1].starts_with("ERR"));
    }

    // -- downcase-region tests --

    #[test]
    fn downcase_region_basic() {
        let results = eval_all(
            r#"(insert "HELLO WORLD")
               (downcase-region 1 12)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "hello world""#);
    }

    // -- upcase-region tests --

    #[test]
    fn upcase_region_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (upcase-region 1 12)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "HELLO WORLD""#);
    }

    // -- capitalize-region tests --

    #[test]
    fn capitalize_region_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (capitalize-region 1 12)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "Hello World""#);
    }

    #[test]
    fn upcase_initials_region_basic() {
        let results = eval_all(
            r#"(insert "hELLo wORLD")
               (upcase-initials-region 1 12)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "HELLo WORLD""#);
    }

    #[test]
    fn upcase_region_noncontiguous_requires_mark() {
        let result = eval_one(
            r#"(with-temp-buffer
                 (insert "abc")
                 (upcase-region 1 3 t))"#,
        );
        assert!(result.starts_with("ERR (error"));
        assert!(result.contains("The mark is not set now, so there is no region"));
    }

    #[test]
    fn upcase_region_noncontiguous_accepts_live_mark() {
        let results = eval_all(
            r#"(insert "abc")
               (set-mark 2)
               (upcase-region 1 3 t)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "aBC""#);
    }

    // -- downcase-word tests --

    #[test]
    fn downcase_word_basic() {
        let results = eval_all(
            r#"(insert "HELLO WORLD")
               (goto-char 0)
               (downcase-word 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "hello WORLD""#);
    }

    // -- upcase-word tests --

    #[test]
    fn upcase_word_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (goto-char 0)
               (upcase-word 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "HELLO world""#);
    }

    // -- capitalize-word tests --

    #[test]
    fn capitalize_word_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (goto-char 0)
               (capitalize-word 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "Hello world""#);
    }

    #[test]
    fn capitalize_word_mixed_case() {
        let results = eval_all(
            r#"(insert "hELLO world")
               (goto-char 0)
               (capitalize-word 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "Hello world""#);
    }

    // -- transpose-chars tests --

    #[test]
    fn transpose_chars_basic() {
        let results = eval_all(
            r#"(insert "abc")
               (goto-char 2)
               (transpose-chars 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "bac""#);
    }

    #[test]
    fn transpose_chars_at_end() {
        let results = eval_all(
            r#"(insert "abc")
               (transpose-chars 1)
               (buffer-string)"#,
        );
        // Point is at end (3), should swap 'b' and 'c'.
        assert_eq!(results[2], r#"OK "acb""#);
    }

    // -- transpose-lines tests --

    #[test]
    fn transpose_lines_basic() {
        let results = eval_all(
            r#"(insert "line1\nline2\nline3")
               (goto-char 8)
               (transpose-lines 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "line2\nline1\nline3""#);
    }

    #[test]
    fn transpose_lines_at_buffer_start() {
        let results = eval_all(
            r#"(insert "line1\nline2")
               (goto-char 1)
               (transpose-lines 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "line2\nline1\n""#);
    }

    // -- transpose-words tests --

    #[test]
    fn transpose_words_basic() {
        let results = eval_all(
            r#"(insert "aa bb")
               (goto-char 0)
               (transpose-words 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "bb aa""#);
    }

    #[test]
    fn transpose_words_not_enough_words_errors() {
        let result = eval_one(
            r#"(with-temp-buffer
                 (insert "aa")
                 (goto-char 0)
                 (transpose-words 1))"#,
        );
        assert!(result.starts_with("ERR"));
    }

    // -- transpose-sexps tests --

    #[test]
    fn transpose_sexps_basic() {
        let results = eval_all(
            r#"(insert "(aa) (bb)")
               (goto-char 5)
               (transpose-sexps 1)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "(bb) (aa)""#);
    }

    #[test]
    fn transpose_sexps_at_bob_advances_without_swapping() {
        let results = eval_all(
            r#"(insert "(aa) (bb)")
               (goto-char 1)
               (transpose-sexps 1)
               (list (buffer-string) (point))"#,
        );
        assert_eq!(results[3], r#"OK ("(aa) (bb)" 5)"#);
    }

    // -- transpose-sentences tests --

    #[test]
    fn transpose_sentences_basic() {
        let results = eval_all(
            r#"(insert "One.  Two.")
               (goto-char 1)
               (transpose-sentences 1)
               (list (buffer-string) (point))"#,
        );
        assert_eq!(results[3], r#"OK ("Two.  One." 11)"#);
    }

    #[test]
    fn transpose_sentences_with_single_space_signals_end_of_buffer() {
        let result = eval_one(
            r#"(with-temp-buffer
                 (insert "One. Two.")
                 (goto-char 1)
                 (transpose-sentences 1))"#,
        );
        assert!(result.starts_with("ERR (end-of-buffer"));
    }

    // -- transpose-paragraphs tests --

    #[test]
    fn transpose_paragraphs_basic() {
        let result = eval_one(
            r#"(with-temp-buffer
                 (insert "A\n\nB")
                 (goto-char 1)
                 (transpose-paragraphs 1)
                 (list (buffer-string) (point)))"#,
        );
        assert_eq!(result, "OK (\"\\nBA\\n\" 5)");
    }

    #[test]
    fn transpose_paragraphs_backward_from_eob() {
        let result = eval_one(
            r#"(with-temp-buffer
                 (insert "A\n\nB\n\nC")
                 (goto-char (point-max))
                 (transpose-paragraphs -1)
                 (list (buffer-string) (point)))"#,
        );
        assert_eq!(result, "OK (\"A\\n\\nC\\nB\\n\" 5)");
    }

    // -- indent-line-to tests --

    #[test]
    fn indent_line_to_basic() {
        let results = eval_all(
            r#"(insert "hello")
               (indent-line-to 4)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "    hello""#);
    }

    #[test]
    fn indent_line_to_replaces_existing() {
        let results = eval_all(
            r#"(insert "  hello")
               (indent-line-to 4)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "    hello""#);
    }

    // -- indent-to tests --

    #[test]
    fn indent_to_basic() {
        let results = eval_all(
            r#"(insert "hi")
               (indent-to 8)
               (buffer-string)"#,
        );
        // "hi" is at col 0-1, we want to indent to col 8, so 6 spaces after "hi".
        assert_eq!(results[2], r#"OK "hi      ""#);
    }

    // -- newline tests --

    #[test]
    fn newline_basic() {
        let results = eval_all(
            r#"(insert "ab")
               (goto-char 2)
               (newline)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "a\nb""#);
    }

    #[test]
    fn newline_multiple() {
        let results = eval_all(r#"(newline 3) (buffer-string)"#);
        assert_eq!(results[1], r#"OK "\n\n\n""#);
    }

    // -- newline-and-indent tests --

    #[test]
    fn newline_and_indent_basic() {
        let results = eval_all(
            r#"(insert "    hello")
               (newline-and-indent)
               (buffer-string)"#,
        );
        // Should add newline + 4 spaces of indentation (copying prev line).
        assert_eq!(results[2], r#"OK "    hello\n    ""#);
    }

    // -- open-line tests --

    #[test]
    fn open_line_keeps_point_before_inserted_newlines() {
        let results = eval_all(
            r#"(insert "ab")
               (goto-char 2)
               (open-line 2)
               (list (buffer-string) (point))"#,
        );
        assert_eq!(results[3], r#"OK ("a\n\nb" 2)"#);
    }

    // -- delete-horizontal-space tests --

    #[test]
    fn delete_horizontal_space_deletes_both_sides() {
        let results = eval_all(
            r#"(insert "a \t  b")
               (goto-char 4)
               (delete-horizontal-space)
               (list (buffer-string) (point))"#,
        );
        assert_eq!(results[3], r#"OK ("ab" 2)"#);
    }

    #[test]
    fn delete_horizontal_space_backward_only() {
        let results = eval_all(
            r#"(insert "a \t  b")
               (goto-char 4)
               (delete-horizontal-space t)
               (list (buffer-string) (point))"#,
        );
        assert_eq!(results[3], r#"OK ("a  b" 2)"#);
    }

    // -- just-one-space tests --

    #[test]
    fn just_one_space_default() {
        let results = eval_all(
            r#"(insert "a \t  b")
               (goto-char 4)
               (just-one-space)
               (list (buffer-string) (point))"#,
        );
        assert_eq!(results[3], r#"OK ("a b" 3)"#);
    }

    #[test]
    fn just_one_space_zero() {
        let results = eval_all(
            r#"(insert "a \t  b")
               (goto-char 4)
               (just-one-space 0)
               (list (buffer-string) (point))"#,
        );
        assert_eq!(results[3], r#"OK ("ab" 2)"#);
    }

    // -- delete-indentation tests --

    #[test]
    fn delete_indentation_basic() {
        let results = eval_all(
            r#"(insert "hello\n    world")
               (goto-char 14)
               (delete-indentation)
               (buffer-string)"#,
        );
        assert_eq!(results[3], r#"OK "hello world""#);
    }

    // -- tab-to-tab-stop tests --

    #[test]
    fn tab_to_tab_stop_basic() {
        let results = eval_all(
            r#"(insert "hi")
               (tab-to-tab-stop)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "hi\t""#);
    }

    // -- indent-rigidly tests --

    #[test]
    fn indent_rigidly_forward() {
        let results = eval_all(
            r#"(insert "a\nb\nc")
               (indent-rigidly 1 6 2)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "  a\n  b\n  c""#);
    }

    #[test]
    fn indent_rigidly_backward() {
        let results = eval_all(
            r#"(insert "  a\n  b\n  c")
               (indent-rigidly 1 12 -2)
               (buffer-string)"#,
        );
        assert_eq!(results[2], r#"OK "a\nb\nc""#);
    }

    // -- copy-region-as-kill tests --

    #[test]
    fn copy_region_as_kill_basic() {
        let results = eval_all(
            r#"(insert "hello world")
               (copy-region-as-kill 1 6)
               (buffer-string)
               (current-kill 0)"#,
        );
        assert_eq!(results[2], r#"OK "hello world""#);
        assert_eq!(results[3], r#"OK "hello""#);
    }

    // -- wrong args tests --

    #[test]
    fn kill_new_wrong_type() {
        let result = eval_one("(kill-new 42)");
        assert!(result.starts_with("ERR"));
    }

    #[test]
    fn kill_word_wrong_args() {
        let result = eval_one("(kill-word)");
        assert!(result.starts_with("ERR"));
    }

    #[test]
    fn downcase_region_wrong_args() {
        let result = eval_one("(downcase-region 0)");
        assert!(result.starts_with("ERR"));
    }

    #[test]
    fn transpose_chars_wrong_args() {
        let result = eval_one("(transpose-chars)");
        assert!(result.starts_with("ERR"));
    }
}
