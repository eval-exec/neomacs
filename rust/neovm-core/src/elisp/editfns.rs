//! Editing-function builtins — point/mark queries, insertion, deletion,
//! substring extraction, and miscellaneous user/system info.
//!
//! Emacs Lisp uses **1-based character positions** while the internal
//! `Buffer` stores **0-based byte positions**.  Every Lisp↔Buffer boundary
//! must convert:
//!
//! - Lisp char pos  →  byte pos:  `buf.text.char_to_byte(lisp_pos - 1)`
//! - byte pos       →  Lisp char: `buf.text.byte_to_char(byte_pos) + 1`

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

/// Extract an integer (or char-as-integer) from a Value, signalling
/// `wrong-type-argument` on type mismatch.
fn expect_integer(_name: &str, val: &Value) -> Result<i64, Flow> {
    val.as_int().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("integer-or-marker-p"), val.clone()],
        )
    })
}

/// Convert a Lisp 1-based character position to a 0-based byte position,
/// clamping to the accessible region `[begv, zv]`.
fn lisp_pos_to_byte(buf: &crate::buffer::Buffer, lisp_pos: i64) -> usize {
    let char_count = buf.text.byte_to_char(buf.text.len());
    // Clamp the 1-based char pos into [1, char_count+1] (point-max is size+1 in
    // Emacs convention, but that maps to byte pos == text.len()).
    let clamped = (lisp_pos.max(1) as usize).min(char_count + 1);
    let char0 = clamped - 1; // 0-based char index
    let byte = buf.text.char_to_byte(char0);
    byte.clamp(buf.begv, buf.zv)
}

/// Convert a 0-based byte position to a Lisp 1-based character position.
fn byte_to_lisp_pos(buf: &crate::buffer::Buffer, byte: usize) -> i64 {
    (buf.text.byte_to_char(byte) + 1) as i64
}

fn buffer_read_only_active(eval: &super::eval::Evaluator, buf: &crate::buffer::Buffer) -> bool {
    if buf.read_only {
        return true;
    }

    for frame in eval.dynamic.iter().rev() {
        if let Some(value) = frame.get("buffer-read-only") {
            return value.is_truthy();
        }
    }

    if let Some(value) = buf.get_buffer_local("buffer-read-only") {
        return value.is_truthy();
    }

    eval.obarray
        .symbol_value("buffer-read-only")
        .is_some_and(|value| value.is_truthy())
}

fn ensure_current_buffer_writable(eval: &super::eval::Evaluator) -> Result<(), Flow> {
    if let Some(buf) = eval.buffers.current_buffer() {
        if buffer_read_only_active(eval, buf) {
            return Err(signal("buffer-read-only", vec![Value::string(&buf.name)]));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins (need &mut Evaluator for buffer access)
// ---------------------------------------------------------------------------

/// Collect the insertable text from a mixed list of strings and characters.
fn collect_insert_text(_name: &str, args: &[Value]) -> Result<String, Flow> {
    let mut text = String::new();
    for arg in args {
        match arg {
            Value::Str(s) => text.push_str(s),
            Value::Char(c) => text.push(*c),
            Value::Int(n) => {
                // Emacs treats integers as character codes.
                if let Some(ch) = char::from_u32(*n as u32) {
                    text.push(ch);
                } else {
                    return Err(signal(
                        "wrong-type-argument",
                        vec![Value::symbol("characterp"), arg.clone()],
                    ));
                }
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("char-or-string-p"), other.clone()],
                ));
            }
        }
    }
    Ok(text)
}

/// `(insert &rest ARGS)` — insert strings or characters at point.
pub(crate) fn builtin_insert(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    let text = collect_insert_text("insert", &args)?;
    ensure_current_buffer_writable(eval)?;
    if let Some(buf) = eval.buffers.current_buffer_mut() {
        buf.insert(&text);
    }
    Ok(Value::Nil)
}

/// `(insert-before-markers &rest ARGS)` — insert at point (markers advance).
/// For now, identical to `insert`.
pub(crate) fn builtin_insert_before_markers(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    // In a full implementation, all markers at point would advance past the
    // inserted text.  Our `Buffer::insert` already advances markers with
    // InsertionType::After, so this is a reasonable approximation.
    builtin_insert(eval, args)
}

/// `(delete-char N &optional KILLFLAG)` — delete N characters forward.
pub(crate) fn builtin_delete_char(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("delete-char", &args, 1)?;
    expect_max_args("delete-char", &args, 2)?;
    let n = expect_integer("delete-char", &args[0])?;
    ensure_current_buffer_writable(eval)?;
    if let Some(buf) = eval.buffers.current_buffer_mut() {
        let pt = buf.pt;
        if n > 0 {
            // Delete N characters forward from point.
            let mut end = pt;
            for _ in 0..n {
                match buf.char_after(end) {
                    Some(ch) => end += ch.len_utf8(),
                    None => {
                        return Err(signal("end-of-buffer", vec![]));
                    }
                }
            }
            buf.delete_region(pt, end);
        } else if n < 0 {
            // Delete |N| characters backward from point.
            let mut start = pt;
            for _ in 0..(-n) {
                match buf.char_before(start) {
                    Some(ch) => start -= ch.len_utf8(),
                    None => {
                        return Err(signal("beginning-of-buffer", vec![]));
                    }
                }
            }
            buf.delete_region(start, pt);
        }
        // n == 0: do nothing.
    }
    Ok(Value::Nil)
}

/// `(buffer-substring START END)` — return text between START and END.
pub(crate) fn builtin_buffer_substring(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("buffer-substring", &args, 2)?;
    let start_pos = expect_integer("buffer-substring", &args[0])?;
    let end_pos = expect_integer("buffer-substring", &args[1])?;
    match eval.buffers.current_buffer() {
        Some(buf) => {
            let start_byte = lisp_pos_to_byte(buf, start_pos);
            let end_byte = lisp_pos_to_byte(buf, end_pos);
            let (lo, hi) = if start_byte <= end_byte {
                (start_byte, end_byte)
            } else {
                (end_byte, start_byte)
            };
            Ok(Value::string(buf.buffer_substring(lo, hi)))
        }
        None => Ok(Value::string("")),
    }
}

/// `(buffer-substring-no-properties START END)` — same as buffer-substring
/// (text properties not yet implemented at the Lisp value level).
pub(crate) fn builtin_buffer_substring_no_properties(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    builtin_buffer_substring(eval, args)
}

/// `(region-beginning)` — return start of region.
/// Stub: returns point-min when no region is active.
pub(crate) fn builtin_region_beginning(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("region-beginning", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => {
            let pt_lisp = byte_to_lisp_pos(buf, buf.pt);
            let mark_lisp = buf
                .mark
                .map(|m| byte_to_lisp_pos(buf, m))
                .unwrap_or_else(|| byte_to_lisp_pos(buf, buf.begv));
            Ok(Value::Int(pt_lisp.min(mark_lisp)))
        }
        None => Ok(Value::Int(1)),
    }
}

/// `(region-end)` — return end of region.
/// Stub: returns point-max when no region is active.
pub(crate) fn builtin_region_end(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("region-end", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => {
            let pt_lisp = byte_to_lisp_pos(buf, buf.pt);
            let mark_lisp = buf
                .mark
                .map(|m| byte_to_lisp_pos(buf, m))
                .unwrap_or_else(|| byte_to_lisp_pos(buf, buf.zv));
            Ok(Value::Int(pt_lisp.max(mark_lisp)))
        }
        None => Ok(Value::Int(1)),
    }
}

/// `(set-mark POS)` — set the mark to POS, return POS.
pub(crate) fn builtin_set_mark(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("set-mark", &args, 1)?;
    let pos = expect_integer("set-mark", &args[0])?;
    let ret = args[0].clone();
    if let Some(buf) = eval.buffers.current_buffer_mut() {
        let byte = lisp_pos_to_byte(buf, pos);
        buf.set_mark(byte);
    }
    Ok(ret)
}

/// `(bobp)` — return t if point is at beginning of accessible region.
pub(crate) fn builtin_bobp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("bobp", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => Ok(Value::bool(buf.pt <= buf.begv)),
        None => Ok(Value::True),
    }
}

/// `(eobp)` — return t if point is at end of accessible region.
pub(crate) fn builtin_eobp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("eobp", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => Ok(Value::bool(buf.pt >= buf.zv)),
        None => Ok(Value::True),
    }
}

/// `(bolp)` — return t if point is at beginning of a line.
/// True at buffer beginning or if the character before point is a newline.
pub(crate) fn builtin_bolp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("bolp", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => {
            if buf.pt <= buf.begv {
                Ok(Value::True)
            } else {
                match buf.char_before(buf.pt) {
                    Some('\n') => Ok(Value::True),
                    _ => Ok(Value::Nil),
                }
            }
        }
        None => Ok(Value::True),
    }
}

/// `(eolp)` — return t if point is at end of a line.
/// True at buffer end or if the character after point is a newline.
pub(crate) fn builtin_eolp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("eolp", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => {
            if buf.pt >= buf.zv {
                Ok(Value::True)
            } else {
                match buf.char_after(buf.pt) {
                    Some('\n') => Ok(Value::True),
                    _ => Ok(Value::Nil),
                }
            }
        }
        None => Ok(Value::True),
    }
}

/// `(following-char)` — return character after point (0 if at end).
pub(crate) fn builtin_following_char(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("following-char", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => match buf.char_after(buf.pt) {
            Some(ch) => Ok(Value::Int(ch as i64)),
            None => Ok(Value::Int(0)),
        },
        None => Ok(Value::Int(0)),
    }
}

/// `(preceding-char)` — return character before point (0 if at beginning).
pub(crate) fn builtin_preceding_char(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("preceding-char", &args, 0)?;
    match eval.buffers.current_buffer() {
        Some(buf) => match buf.char_before(buf.pt) {
            Some(ch) => Ok(Value::Int(ch as i64)),
            None => Ok(Value::Int(0)),
        },
        None => Ok(Value::Int(0)),
    }
}

// ---------------------------------------------------------------------------
// Pure builtins (no evaluator needed)
// ---------------------------------------------------------------------------

/// `(user-login-name &optional UID)` — return login name.
pub(crate) fn builtin_user_login_name(args: Vec<Value>) -> EvalResult {
    expect_max_args("user-login-name", &args, 1)?;
    let name = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    Ok(Value::string(name))
}

/// `(user-real-login-name)` — return real login name (same as user-login-name).
pub(crate) fn builtin_user_real_login_name(args: Vec<Value>) -> EvalResult {
    expect_args("user-real-login-name", &args, 0)?;
    builtin_user_login_name(vec![])
}

/// `(user-uid)` — return effective user ID.
/// Uses the `id -u` command on Unix; falls back to 1000.
pub(crate) fn builtin_user_uid(args: Vec<Value>) -> EvalResult {
    expect_args("user-uid", &args, 0)?;
    Ok(Value::Int(get_uid()))
}

/// `(user-real-uid)` — return real user ID.
pub(crate) fn builtin_user_real_uid(args: Vec<Value>) -> EvalResult {
    expect_args("user-real-uid", &args, 0)?;
    Ok(Value::Int(get_uid()))
}

/// `(user-full-name &optional UID)` — return the user's full name.
/// Falls back to the USER environment variable.
pub(crate) fn builtin_user_full_name(args: Vec<Value>) -> EvalResult {
    expect_max_args("user-full-name", &args, 1)?;
    let name = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "Unknown".to_string());
    Ok(Value::string(name))
}

/// `(system-name)` — return the host name of the machine.
pub(crate) fn builtin_system_name(args: Vec<Value>) -> EvalResult {
    expect_args("system-name", &args, 0)?;
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "localhost".to_string());
    Ok(Value::string(hostname))
}

/// `(emacs-pid)` — return the process ID of this Emacs process.
pub(crate) fn builtin_emacs_pid(args: Vec<Value>) -> EvalResult {
    expect_args("emacs-pid", &args, 0)?;
    Ok(Value::Int(std::process::id() as i64))
}

/// `(group-gid)` — return the effective group ID.
pub(crate) fn builtin_group_gid(args: Vec<Value>) -> EvalResult {
    expect_args("group-gid", &args, 0)?;
    Ok(Value::Int(get_gid()))
}

/// `(group-real-gid)` — return the real group ID.
pub(crate) fn builtin_group_real_gid(args: Vec<Value>) -> EvalResult {
    expect_args("group-real-gid", &args, 0)?;
    Ok(Value::Int(get_gid()))
}

// ---------------------------------------------------------------------------
// OS helpers (avoid libc dependency)
// ---------------------------------------------------------------------------

/// Retrieve the effective UID via `id -u`, falling back to 1000.
fn get_uid() -> i64 {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(1000)
}

/// Retrieve the effective GID via `id -g`, falling back to 1000.
fn get_gid() -> i64 {
    std::process::Command::new("id")
        .arg("-g")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(1000)
}
