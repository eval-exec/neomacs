//! Miscellaneous commonly-needed builtins.
//!
//! Contains:
//! - Special forms: prog2, save-current-buffer
//! - Pure builtins: copy-alist, rassoc, rassq, assoc-default, make-list, safe-length,
//!   subst-char-in-string, string/char encoding stubs, locale-info
//! - Eval-dependent builtins: backtrace-* helpers, recursion-depth

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use strum::{EnumString, IntoStaticStr};

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const MAX_EMACS_CHAR: i64 = 0x3FFFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum LocaleInfoItem {
    Codeset,
    Days,
    Months,
    Paper,
}

impl LocaleInfoItem {
    fn from_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn symbol_name(self) -> &'static str {
        self.into()
    }
}

// ---------------------------------------------------------------------------
// Argument helpers (local to this module)
// ---------------------------------------------------------------------------

fn expect_wholenump(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *val],
        )),
    }
}

/// GNU `CHECK_FIXNUM` (`src/lisp.h`): a fixnum of ANY sign, and `fixnump` --
/// not `wholenump` -- is the predicate the signal names.
///
/// `Fbacktrace_debug` checks LEVEL twice and the two checks disagree:
/// `CHECK_FIXNUM (level)` first (`src/eval.c:4022`), then `CHECK_FIXNAT
/// (nframes)` inside `get_backtrace_frame` (`src/eval.c:3987`).  So GNU
/// answers `(wrong-type-argument fixnump "x")` for a string and
/// `(wrong-type-argument wholenump -1)` for a negative fixnum, and a single
/// `wholenump` check cannot produce both.
fn expect_fixnum(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), *val],
        )),
    }
}

fn expect_character_code(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(c) if (0..=0x3F_FFFF).contains(&c) => Ok(c),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *val],
        )),
    }
}

/// Convert unibyte LispString bytes to multibyte Emacs encoding.
fn convert_unibyte_to_multibyte_bytes(src: &[u8]) -> Vec<u8> {
    crate::emacs_core::emacs_char::str_to_multibyte(src)
}

/// Reinterpret unibyte bytes as an Emacs multibyte sequence.
///
/// Valid multibyte sequences are preserved as-is; lone high bytes become
/// raw-byte characters.
fn reinterpret_unibyte_as_multibyte_bytes(src: &[u8]) -> Vec<u8> {
    crate::emacs_core::emacs_char::str_as_multibyte(src)
}

// ===========================================================================
// Special forms
// ===========================================================================

// ===========================================================================
// Pure builtins (no eval needed)
// ===========================================================================

/// `(copy-alist ALIST)` -- shallow copy an association list.
/// Each top-level cons is copied; the car/cdr of each entry are shared.
pub(crate) fn builtin_copy_alist(args: Vec<Value>) -> EvalResult {
    expect_args("copy-alist", &args, 1)?;
    let alist = &args[0];
    if !alist.is_nil() && !alist.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *alist],
        ));
    }

    let copy = super::builtins::builtin_copy_sequence(vec![*alist])?;
    let mut cursor = copy;
    while cursor.is_cons() {
        let element = cursor.cons_car();
        if element.is_cons() {
            cursor.set_car(Value::cons(element.cons_car(), element.cons_cdr()));
        }
        cursor = cursor.cons_cdr();
    }
    Ok(copy)
}

/// `(rassoc KEY ALIST)` -- find the first entry in ALIST whose cdr equals KEY
/// (using `equal`).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_rassoc(args: Vec<Value>) -> EvalResult {
    builtin_rassoc_with_symbols(args, false)
}

pub(crate) fn builtin_rassoc_with_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_rassoc_with_symbols(args, eval.symbols_with_pos_enabled)
}

fn builtin_rassoc_with_symbols(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args("rassoc", &args, 2)?;
    let key = &args[0];
    let alist = &args[1];
    let mut cursor = *alist;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return Ok(Value::NIL),
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if pair_car.is_cons() {
                    let inner_pair_cdr = pair_car.cons_cdr();
                    if equal_value_swp(&inner_pair_cdr, key, 0, symbols_with_pos_enabled) {
                        return Ok(pair_car);
                    }
                }
                cursor = pair_cdr;
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), *alist],
                ));
            }
        }
    }
}

/// `(rassq KEY ALIST)` -- like rassoc but uses `eq` for comparison.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_rassq(args: Vec<Value>) -> EvalResult {
    builtin_rassq_with_symbols(args, false)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn builtin_rassq_with_symbols(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args("rassq", &args, 2)?;
    builtin_rassq_values(args[0], args[1], symbols_with_pos_enabled)
}

pub(crate) fn builtin_rassq_2(
    eval: &mut super::eval::Context,
    key: Value,
    alist: Value,
) -> EvalResult {
    builtin_rassq_values(key, alist, eval.symbols_with_pos_enabled)
}

fn builtin_rassq_values(key: Value, alist: Value, symbols_with_pos_enabled: bool) -> EvalResult {
    if symbols_with_pos_enabled {
        return builtin_rassq_values_swp(key, alist);
    }

    let key_bits = key.bits();
    let mut tail = alist;
    let mut tortoise = alist;
    let mut power = 1usize;
    let mut distance = 0usize;

    while tail.is_cons() {
        let pair_car = tail.cons_car();
        if pair_car.is_cons() && pair_car.cons_cdr().bits() == key_bits {
            return Ok(pair_car);
        }

        tail = tail.cons_cdr();
        if tail.is_cons() {
            distance = distance.saturating_add(1);
            if tail.bits() == tortoise.bits() {
                return Err(signal(LispCondition::CircularList, vec![tail]));
            }
            if distance == power {
                tortoise = tail;
                power = power.saturating_mul(2).max(1);
                distance = 0;
            }
        }
    }

    if tail.is_nil() {
        Ok(Value::NIL)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), alist],
        ))
    }
}

fn builtin_rassq_values_swp(key: Value, alist: Value) -> EvalResult {
    let mut tail = alist;
    let mut tortoise = alist;
    let mut power = 1usize;
    let mut distance = 0usize;

    while tail.is_cons() {
        let pair_car = tail.cons_car();
        if pair_car.is_cons() {
            let pair_cdr = pair_car.cons_cdr();
            if eq_value_swp(&pair_cdr, &key, true) {
                return Ok(pair_car);
            }
        }

        tail = tail.cons_cdr();
        if tail.is_cons() {
            distance = distance.saturating_add(1);
            if tail.bits() == tortoise.bits() {
                return Err(signal(LispCondition::CircularList, vec![tail]));
            }
            if distance == power {
                tortoise = tail;
                power = power.saturating_mul(2).max(1);
                distance = 0;
            }
        }
    }

    if tail.is_nil() {
        Ok(Value::NIL)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), alist],
        ))
    }
}

/// `(make-list LENGTH INIT)` -- create a list of LENGTH elements, each INIT.
pub(crate) fn builtin_make_list(args: Vec<Value>) -> EvalResult {
    expect_args("make-list", &args, 2)?;
    let length = expect_wholenump(&args[0])?;
    let init = &args[1];
    let items: Vec<Value> = (0..length as usize).map(|_| *init).collect();
    Ok(Value::list(items))
}

/// `(string-repeat STRING COUNT)` -- repeat STRING COUNT times.
#[cfg(test)]
pub(crate) fn builtin_string_repeat(args: Vec<Value>) -> EvalResult {
    expect_args("string-repeat", &args, 2)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let count = expect_wholenump(&args[1])? as usize;
    // Issue #131: repeat the real Emacs bytes so raw-unibyte content round-trips,
    // instead of the PUA-sentinel storage string.
    let mut bytes = Vec::with_capacity(ls.as_bytes().len() * count);
    for _ in 0..count {
        bytes.extend_from_slice(ls.as_bytes());
    }
    let repeated = if ls.is_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes(bytes)
    } else {
        crate::heap_types::LispString::from_unibyte(bytes)
    };
    Ok(Value::heap_string(repeated))
}

/// `(safe-length LIST)` -- return the length of LIST without signaling.
pub(crate) fn builtin_safe_length(args: Vec<Value>) -> EvalResult {
    expect_args("safe-length", &args, 1)?;
    let mut tail = args[0];
    let mut length: i64 = 0;
    let mut tortoise = tail;
    let mut max: i64 = 2;
    let mut n: i64 = 0;
    let mut q: u16 = 2;

    while tail.is_cons() {
        length += 1;
        tail = tail.cons_cdr();

        q = q.wrapping_sub(1);
        let check_cycle = if q != 0 {
            true
        } else {
            n -= 1;
            if n > 0 {
                true
            } else {
                max <<= 1;
                n = max >> u16::BITS;
                q = max as u16;
                tortoise = tail;
                false
            }
        };

        if check_cycle && eq_value(&tail, &tortoise) {
            break;
        }
    }

    Ok(Value::fixnum(length))
}

/// `(subst-char-in-string FROMCHAR TOCHAR STRING &optional INPLACE)` --
/// replace all occurrences of FROMCHAR with TOCHAR in STRING.
/// INPLACE is ignored (we always return a new string).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_subst_char_in_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("subst-char-in-string", &args, 3)?;
    expect_max_args("subst-char-in-string", &args, 4)?;
    let from_code = expect_character_code(&args[0])? as u32;
    let to_code = expect_character_code(&args[1])? as u32;
    let src_ls = args[2].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[2]],
        )
    })?;

    use crate::emacs_core::emacs_char;
    let src_bytes = src_ls.as_bytes();

    // Unibyte path: each byte is one character. If either FROM or TO
    // doesn't fit in a single byte the substitution can't apply, so
    // return the original string unchanged (mirroring GNU which only
    // allows unibyte chars in unibyte contexts).
    if !src_ls.is_multibyte() {
        if from_code > 0xFF || to_code > 0xFF {
            return Ok(args[2]);
        }
        let from_byte = from_code as u8;
        if !src_bytes.contains(&from_byte) {
            return Ok(args[2]);
        }
        let to_byte = to_code as u8;
        let replaced: Vec<u8> = src_bytes
            .iter()
            .map(|&b| if b == from_byte { to_byte } else { b })
            .collect();
        return Ok(Value::heap_string(
            crate::heap_types::LispString::from_unibyte(replaced),
        ));
    }

    // Multibyte path: walk FROM via emacs_char::string_char, emitting
    // TO's Emacs encoding whenever the decoded char matches FROM.
    // Unlike the old same-length-only helper, this handles FROM/TO pairs
    // that encode to different byte counts (matching GNU fns.c:3196).
    let mut to_buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
    let to_len = emacs_char::char_string(to_code, &mut to_buf);
    let to_bytes = &to_buf[..to_len];

    let mut out = Vec::with_capacity(src_bytes.len());
    let mut changed = false;
    let mut pos = 0;
    while pos < src_bytes.len() {
        let (code, len) = emacs_char::string_char(&src_bytes[pos..]);
        let clen = len.max(1);
        if code == from_code {
            out.extend_from_slice(to_bytes);
            changed = true;
        } else {
            out.extend_from_slice(&src_bytes[pos..pos + clen]);
        }
        pos += clen;
    }
    if !changed {
        return Ok(args[2]);
    }
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_emacs_bytes(out),
    ))
}

/// `(string-to-multibyte STRING)` -- convert unibyte storage bytes to multibyte chars.
pub(crate) fn builtin_string_to_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-multibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if ls.is_multibyte() {
        return Ok(args[0]);
    }
    let out = convert_unibyte_to_multibyte_bytes(ls.as_bytes());
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_emacs_bytes(out),
    ))
}

/// `(string-to-unibyte STRING)` -- convert to unibyte storage.
pub(crate) fn builtin_string_to_unibyte(args: Vec<Value>) -> EvalResult {
    use crate::emacs_core::emacs_char;
    expect_args("string-to-unibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if !ls.is_multibyte() {
        return Ok(args[0]);
    }
    let src = ls.as_bytes();
    let mut bytes = Vec::with_capacity(ls.schars());
    let mut pos = 0;
    let mut idx = 0usize;
    while pos < src.len() {
        let (cp, len) = emacs_char::string_char(&src[pos..]);
        pos += len;
        if cp <= 0x7F {
            bytes.push(cp as u8);
        } else if emacs_char::char_byte8_p(cp) {
            bytes.push(emacs_char::char_to_byte8(cp));
        } else {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Cannot convert character at index {idx} to unibyte"
                ))],
            ));
        }
        idx += 1;
    }
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(bytes),
    ))
}

/// `(string-as-unibyte STRING)` -- reinterpret as unibyte byte sequence.
pub(crate) fn builtin_string_as_unibyte(args: Vec<Value>) -> EvalResult {
    use crate::emacs_core::emacs_char;
    expect_args("string-as-unibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if !ls.is_multibyte() {
        return Ok(args[0]);
    }
    // Reinterpret: raw-byte chars become their byte value, other chars keep
    // their UTF-8 encoding as raw bytes.
    let src = ls.as_bytes();
    let mut bytes = Vec::with_capacity(src.len());
    let mut pos = 0;
    while pos < src.len() {
        let (cp, len) = emacs_char::string_char(&src[pos..]);
        if emacs_char::char_byte8_p(cp) {
            bytes.push(emacs_char::char_to_byte8(cp));
        } else {
            // Keep the raw encoding bytes
            bytes.extend_from_slice(&src[pos..pos + len]);
        }
        pos += len;
    }
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(bytes),
    ))
}

/// `(string-as-multibyte STRING)` -- reinterpret unibyte storage as multibyte.
pub(crate) fn builtin_string_as_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-as-multibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if ls.is_multibyte() {
        return Ok(args[0]);
    }
    let out = reinterpret_unibyte_as_multibyte_bytes(ls.as_bytes());
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_emacs_bytes(out),
    ))
}

/// `(unibyte-char-to-multibyte CHAR)` -- map 0..255 to multibyte/raw-byte char code.
pub(crate) fn builtin_unibyte_char_to_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("unibyte-char-to-multibyte", &args, 1)?;
    let code = expect_character_code(&args[0])?;
    if code > 0xFF {
        return Err(signal(
            "error",
            vec![Value::string(format!("Not a unibyte character: {code}"))],
        ));
    }
    if code < 0x80 {
        Ok(Value::fixnum(code))
    } else {
        Ok(Value::fixnum(code + 0x3FFF00))
    }
}

/// `(multibyte-char-to-unibyte CHAR)` -- map multibyte/raw-byte char code to byte.
///
/// Mirrors GNU `Fmultibyte_char_to_unibyte` in `character.c`: characters
/// below 256 are passed through (latin1/unibyte ambiguity), eight-bit
/// raw-byte chars decode to their byte via `CHAR_TO_BYTE_SAFE`, and
/// anything else returns -1.
pub(crate) fn builtin_multibyte_char_to_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("multibyte-char-to-unibyte", &args, 1)?;
    let code = expect_character_code(&args[0])?;
    if code < 256 {
        // Can't distinguish a byte read from a unibyte buffer from a
        // latin1 char, so let it slide.
        return Ok(Value::fixnum(code));
    }
    Ok(Value::fixnum(
        match crate::emacs_core::emacs_char::char_to_byte_safe(code as u32) {
            Some(b) => b as i64,
            None => -1,
        },
    ))
}

/// `(locale-info ITEM)` -- minimal locale info.
/// Returns a small oracle-aligned subset in batch mode.
pub(crate) fn builtin_locale_info(args: Vec<Value>) -> EvalResult {
    expect_args("locale-info", &args, 1)?;
    match LocaleInfoItem::from_value(args[0]) {
        Some(LocaleInfoItem::Codeset) => Ok(Value::string("UTF-8")),
        Some(LocaleInfoItem::Days) => Ok(Value::vector(vec![
            Value::string("Sunday"),
            Value::string("Monday"),
            Value::string("Tuesday"),
            Value::string("Wednesday"),
            Value::string("Thursday"),
            Value::string("Friday"),
            Value::string("Saturday"),
        ])),
        Some(LocaleInfoItem::Months) => Ok(Value::vector(vec![
            Value::string("January"),
            Value::string("February"),
            Value::string("March"),
            Value::string("April"),
            Value::string("May"),
            Value::string("June"),
            Value::string("July"),
            Value::string("August"),
            Value::string("September"),
            Value::string("October"),
            Value::string("November"),
            Value::string("December"),
        ])),
        Some(LocaleInfoItem::Paper) => {
            Ok(Value::list(vec![Value::fixnum(210), Value::fixnum(297)]))
        }
        None => Ok(Value::NIL),
    }
}

/// `(display-line-numbers-update-width)` -- compatibility no-op in batch mode.
#[cfg(test)]
pub(crate) fn builtin_display_line_numbers_update_width(args: Vec<Value>) -> EvalResult {
    expect_args("display-line-numbers-update-width", &args, 0)?;
    Ok(Value::NIL)
}

// ===========================================================================
// Eval-dependent builtins
// ===========================================================================

// `backtrace-frame` is implemented in elisp in `lisp/subr.el:6703-6718`,
// delegating to `backtrace-frame--internal` (below). No Rust-level
// `backtrace-frame` primitive exists; a previous stub returning
// synthetic canned frames was removed because it never made it to the
// native subr registry (subr.el's defun wins at runtime) and its fixed
// output did not match GNU semantics.

fn expect_threadp_in_state(
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
) -> Result<(), Flow> {
    if threads.thread_id_from_handle(value).is_some() {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("threadp"), *value],
    ))
}

/// `(backtrace--frames-from-thread THREAD)` -- synthetic backtrace frame list.
pub(crate) fn builtin_backtrace_frames_from_thread(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("backtrace--frames-from-thread", &args, 1)?;
    expect_threadp_in_state(&eval.threads, &args[0])?;
    Ok(Value::list(vec![Value::list(vec![
        Value::T,
        Value::symbol("backtrace--frames-from-thread"),
        args[0],
    ])]))
}

/// `(backtrace--locals NFRAMES &optional BASE)` -- batch-compatible helper.
pub(crate) fn builtin_backtrace_locals(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("backtrace--locals", &args, 1)?;
    expect_max_args("backtrace--locals", &args, 2)?;
    let nframes = expect_wholenump(&args[0])? as usize;
    if nframes == 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), Value::fixnum(-1)],
        ));
    }
    let base = args.get(1).copied().unwrap_or(Value::NIL);
    let frame_indices = runtime_backtrace_frame_indices_from_base(eval, base)?;
    if frame_indices.get(nframes).is_none() || frame_indices.get(nframes - 1).is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Activation frame not found!")],
        ));
    }
    Ok(Value::NIL)
}

/// `(backtrace-debug LEVEL FLAG &optional BASE)` -- GNU `Fbacktrace_debug`
/// (`src/eval.c:4016-4029`).
///
/// Sets the `debug_on_exit` bit on the activation frame LEVEL levels down, so
/// that the debugger is entered again when that frame returns.  This is the
/// half of the entry/exit mechanism that Lisp can reach directly;
/// `do_debug_on_call` reaches the same setter from the `debug-on-next-call`
/// side (`src/eval.c:339`).  See `emacs_core::debug_on_call`.
///
/// LEVEL 0 is `backtrace-debug`'s OWN frame: `get_backtrace_starting_at (Qnil)`
/// is `backtrace_top ()` (`src/eval.c:3988`), which is the running subr.
/// Measured under GNU: `(defun f (x) (backtrace-debug 0 t) (* x 3))` debugs
/// `backtrace-debug`'s exit with value `t`, while `(backtrace-debug 1 t)`
/// debugs `f`'s exit with value `(* x 3)`.
///
/// GNU checks LEVEL twice with two different predicates -- see [`expect_fixnum`].
pub(crate) fn builtin_backtrace_debug(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("backtrace-debug", &args, 2)?;
    expect_max_args("backtrace-debug", &args, 3)?;
    // eval.c:4022 `CHECK_FIXNUM (level)`.
    let level = expect_fixnum(&args[0])?;
    let base = args.get(2).copied().unwrap_or(Value::NIL);
    // eval.c:4023 `get_backtrace_frame (level, base)`, whose first act is
    // `CHECK_FIXNAT (nframes)` (eval.c:3987) -- the second, stricter check.
    if level < 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), args[0]],
        ));
    }
    let frame_indices = runtime_backtrace_frame_indices_from_base(eval, base)?;
    // eval.c:4025 `if (backtrace_p (pdl))` -- walking off the end of the
    // backtrace is not an error, it is simply nothing to set.
    if let Some(index) = frame_indices.get(level as usize).copied() {
        eval.set_backtrace_debug_on_exit(index, !args[1].is_nil());
    }
    // eval.c:4028 -- the FLAG is returned, unexamined and uncoerced.
    Ok(args[1])
}

/// `(backtrace-eval EXP NFRAMES &optional BASE)` -- batch-compatible helper.
pub(crate) fn builtin_backtrace_eval(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("backtrace-eval", &args, 2)?;
    expect_max_args("backtrace-eval", &args, 3)?;
    let nframes = expect_wholenump(&args[1])? as usize;
    let base = args.get(2).copied().unwrap_or(Value::NIL);
    let frame_indices = runtime_backtrace_frame_indices_from_base(eval, base)?;
    if frame_indices.get(nframes).is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Activation frame not found!")],
        ));
    }
    eval.eval_value(&args[0])
}

fn runtime_backtrace_indirect_function(
    eval: &super::eval::Context,
    function: Value,
) -> Option<Value> {
    match function.kind() {
        ValueKind::Symbol(symbol) => {
            super::builtins::symbols::resolve_indirect_symbol_by_id_in_obarray(
                &eval.obarray,
                symbol,
            )
            .map(|(_, value)| value)
            .or(Some(function))
        }
        ValueKind::T => Some(function),
        ValueKind::Nil => None,
        _ => Some(function),
    }
}

/// Snapshot of a single backtrace frame extracted from the specpdl.
struct BacktraceFrameSnapshot {
    function: Value,
    args: Vec<Value>,
    debug_on_exit: bool,
    /// `true` mirrors GNU `nargs == UNEVALLED` for special forms; `args`
    /// then holds a single element that is the cons list of un-evaluated
    /// argument forms.
    unevalled: bool,
}

fn backtrace_frame_snapshot_at(
    eval: &super::eval::Context,
    index: usize,
) -> Option<BacktraceFrameSnapshot> {
    let entry = eval.specpdl.get(index)?;
    let (function, args, debug_on_exit, unevalled) = eval.backtrace_entry_values(entry)?;
    Some(BacktraceFrameSnapshot {
        function,
        args: args.into_iter().collect(),
        debug_on_exit,
        unevalled,
    })
}

/// Collect backtrace frame specpdl indices, ordered oldest-first (index 0 = deepest).
fn collect_backtrace_frame_indices(eval: &super::eval::Context) -> Vec<usize> {
    eval.specpdl
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| match entry {
            super::eval::SpecBinding::Backtrace { .. }
            | super::eval::SpecBinding::Backtrace1 { .. }
            | super::eval::SpecBinding::Backtrace2 { .. }
            | super::eval::SpecBinding::BacktraceNative { .. } => Some(index),
            _ => None,
        })
        .collect()
}

fn runtime_backtrace_frame_indices_from_base(
    eval: &super::eval::Context,
    base: Value,
) -> Result<Vec<usize>, Flow> {
    let mut offset = 0usize;
    let mut base_function = base;
    if base.is_cons() {
        let pair_car = base.cons_car();
        let pair_cdr = base.cons_cdr();
        if let Some(raw_offset) = pair_car.as_fixnum() {
            // GNU reads the offset with a bare `XFIXNUM` -- there is no
            // `CHECK_FIXNAT` on the car, unlike the LEVEL argument
            // (`src/eval.c:3966` against `:3986`) -- and spends it in
            // `while (backtrace_p (pdl) && offset-- > 0)` (`src/eval.c:3977`).
            // A negative offset is therefore not an error there, it is a loop
            // that does not run.  Measured, `-Q --batch`, `tmp/l183-p13.el`
            // row 9: `(backtrace-frame 0 '(-1 . f))` answers f's own frame in
            // GNU and signalled `(wrong-type-argument wholenump -1)` here.
            offset = usize::try_from(raw_offset).unwrap_or(0);
            base_function = pair_cdr;
        }
    }

    let frame_indices = collect_backtrace_frame_indices(eval);

    let start_index = if base_function.is_nil() {
        frame_indices.len().checked_sub(1)
    } else {
        let Some(indirect_base) = runtime_backtrace_indirect_function(eval, base_function) else {
            return Ok(Vec::new());
        };
        let mut found = None;
        for (index, specpdl_index) in frame_indices.iter().copied().enumerate().rev() {
            let Some(frame) = backtrace_frame_snapshot_at(eval, specpdl_index) else {
                continue;
            };
            let Some(indirect_frame) = runtime_backtrace_indirect_function(eval, frame.function)
            else {
                continue;
            };
            if eq_value(&indirect_frame, &indirect_base) {
                found = Some(index);
                break;
            }
        }
        found
    };

    let Some(mut index) = start_index else {
        return Ok(Vec::new());
    };

    while offset > 0 {
        if index == 0 {
            return Ok(Vec::new());
        }
        index -= 1;
        offset -= 1;
    }

    Ok(frame_indices.into_iter().take(index + 1).rev().collect())
}

fn runtime_backtrace_frame_flags(frame: &BacktraceFrameSnapshot) -> Value {
    if frame.debug_on_exit {
        Value::list(vec![Value::symbol(":debug-on-exit"), Value::T])
    } else {
        Value::NIL
    }
}

fn apply_backtrace_callback(
    eval: &mut super::eval::Context,
    function: Value,
    frame: &BacktraceFrameSnapshot,
) -> EvalResult {
    // Matches GNU `backtrace_frame_apply` (eval.c:3993-3998).
    // UNEVALLED frames pass `evald=nil` and the single argument
    // slot (the cons list of un-evaluated forms) directly; otherwise
    // pass `evald=t` and a fresh list of the evaluated argument values.
    let (evald, args) = if frame.unevalled {
        let forms = frame.args.first().copied().unwrap_or(Value::NIL);
        (Value::NIL, forms)
    } else {
        (Value::T, Value::list(frame.args.clone()))
    };
    eval.apply(
        function,
        vec![
            evald,
            frame.function,
            args,
            runtime_backtrace_frame_flags(frame),
        ],
    )
}

fn apply_backtrace_callback_at_index(
    eval: &mut super::eval::Context,
    function: Value,
    index: usize,
) -> EvalResult {
    let Some(frame) = backtrace_frame_snapshot_at(eval, index) else {
        return Ok(Value::NIL);
    };

    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(function);
    eval.push_specpdl_root(frame.function);
    for arg in frame.args.iter().copied() {
        eval.push_specpdl_root(arg);
    }

    let result = apply_backtrace_callback(eval, function, &frame);
    eval.restore_specpdl_roots(root_scope);
    result
}

/// `(backtrace-frame--internal FUN NFRAMES BASE)` -- compatibility helper.
///
/// Walks the specpdl backtrace entries and feeds frames through the same
/// callback shape that `subr.el` expects.
pub(crate) fn builtin_backtrace_frame_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("backtrace-frame--internal", &args, 3)?;
    let nframes = expect_wholenump(&args[1])? as usize;
    let frame_indices = runtime_backtrace_frame_indices_from_base(eval, args[2])?;
    let Some(index) = frame_indices.get(nframes).copied() else {
        return Ok(Value::NIL);
    };
    apply_backtrace_callback_at_index(eval, args[0], index)
}

/// `(mapbacktrace FUNCTION &optional BASE)` -- iterate runtime backtrace
/// frames in GNU order, newest first.
pub(crate) fn builtin_mapbacktrace(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("mapbacktrace", &args, 1)?;
    expect_max_args("mapbacktrace", &args, 2)?;
    let base = args.get(1).copied().unwrap_or(Value::NIL);
    let frame_indices = runtime_backtrace_frame_indices_from_base(eval, base)?;
    for index in frame_indices {
        apply_backtrace_callback_at_index(eval, args[0], index)?;
    }
    Ok(Value::NIL)
}

/// `(recursion-depth)` -- return the current depth in recursive edits.
pub(crate) fn builtin_recursion_depth(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("recursion-depth", &args, 0)?;
    Ok(Value::fixnum(eval.recursion_depth() as i64))
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
