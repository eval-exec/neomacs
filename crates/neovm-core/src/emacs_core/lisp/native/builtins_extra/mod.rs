//! Additional built-in functions to improve Emacs Lisp compatibility.
//!
//! These builtins complement the core set in builtins.rs with:
//! - Advanced list operations (cl-lib compatible)
//! - Sequence operations (seq.el compatible)
//! - String utilities (subr-x compatible)
//! - Window/frame operations
//! - Buffer info queries
//! - Format enhancements
//! - Variable operations

use super::error::{EvalResult, Flow, signal};
use super::intern::resolve_sym;
#[cfg(test)]
use super::runtime_identity::{
    PasswdEntry, canonical_full_name, effective_uid, operating_system_release_value,
};
use super::runtime_identity::{
    lookup_full_name_by_login, lookup_full_name_by_uid, lookup_login_by_uid,
};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_fixnum, expect_max_args, expect_min_args};
// storage imports removed — now using emacs_char + LispString directly
use super::value::{Value, ValueKind};
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::num::logic::traits::SignificantBits;
use malachite::base::rounding_modes::RoundingMode;
use malachite::integer::Integer;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_int(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *val],
        )),
    }
}

fn symbol_like_name(value: &Value) -> Option<&str> {
    match value.kind() {
        ValueKind::Nil => Some("nil"),
        ValueKind::T => Some("t"),
        ValueKind::Symbol(id) => Some(resolve_sym(id)),
        _ => None,
    }
}

fn expect_number_or_marker_f64(value: &Value) -> Result<f64, Flow> {
    use crate::emacs_core::value::VecLikeType;
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(value.xfloat()),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            Ok(f64::rounding_from(value.as_bignum().unwrap(), RoundingMode::Nearest).0)
        }
        _ if super::marker::is_marker(value) => {
            Ok(super::marker::marker_position_as_int(value)? as f64)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), *value],
        )),
    }
}

fn list_car_or_signal(value: &Value) -> Result<Value, Flow> {
    match value.kind() {
        ValueKind::Cons => Ok(value.cons_car()),
        ValueKind::Nil => Ok(Value::NIL),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *value],
        )),
    }
}

fn assoc_string_key_name(value: &Value) -> Result<crate::heap_types::LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone()),
        _ => symbol_like_name(value)
            .map(crate::heap_types::LispString::from_utf8)
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), *value],
                )
            }),
    }
}

fn assoc_string_compare(
    key: &Value,
    entry_key: &crate::heap_types::LispString,
    fold_case: bool,
) -> Result<bool, Flow> {
    let key_name = assoc_string_key_name(key)?;
    Ok(assoc_string_equal(entry_key, &key_name, fold_case))
}

fn assoc_string_entry_name(value: &Value) -> Option<crate::heap_types::LispString> {
    match value.kind() {
        ValueKind::String => Some(
            value
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload")
                .clone(),
        ),
        _ => symbol_like_name(value).map(crate::heap_types::LispString::from_utf8),
    }
}

/// Compare two strings for `assoc-string`, mirroring GNU `compare-strings`:
/// characters are compared by code, and a unibyte raw byte is unified with the
/// corresponding multibyte eight-bit character (both decode to the same
/// `0x3FFF00+` code). With folding, Unicode chars use `to_lowercase`; eight-bit
/// chars are caseless. This replaces the storage-String encoding.
fn assoc_string_equal(
    left: &crate::heap_types::LispString,
    right: &crate::heap_types::LispString,
    fold_case: bool,
) -> bool {
    let left_codes = assoc_string_codes(left);
    let right_codes = assoc_string_codes(right);
    if fold_case {
        assoc_string_downcase(&left_codes) == assoc_string_downcase(&right_codes)
    } else {
        left_codes == right_codes
    }
}

fn assoc_string_codes(value: &crate::heap_types::LispString) -> Vec<u32> {
    let bytes = value.as_bytes();
    if value.is_multibyte() {
        let mut codes = Vec::new();
        let mut pos = 0;
        while pos < bytes.len() {
            let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
            codes.push(code);
            pos += len.max(1);
        }
        codes
    } else {
        // A unibyte string's bytes ARE its characters; bytes >= 0x80 are eight-bit
        // characters (matching how compare-strings unifies a unibyte raw byte with
        // the corresponding multibyte eight-bit char).
        bytes
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    b as u32
                } else {
                    crate::emacs_core::emacs_char::byte8_to_char(b)
                }
            })
            .collect()
    }
}

fn assoc_string_downcase(codes: &[u32]) -> Vec<u32> {
    codes
        .iter()
        .flat_map(|&code| match char::from_u32(code) {
            Some(c) => c.to_lowercase().map(|lc| lc as u32).collect::<Vec<_>>(),
            None => vec![code],
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Advanced list operations
// ---------------------------------------------------------------------------

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn remove_list_equal(args: Vec<Value>) -> EvalResult {
    expect_args("remove", &args, 2)?;
    let target = &args[0];
    let list_val = &args[1];

    let mut result = Vec::new();
    let mut cursor = *list_val;
    loop {
        match cursor.kind() {
            ValueKind::Nil => break,
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if !super::value::equal_value(&pair_car, target, 0) {
                    result.push(pair_car);
                }
                cursor = pair_cdr;
            }
            _ => break,
        }
    }
    Ok(Value::list(result))
}

/// `(take N LIST)` — first N elements.
pub(crate) fn builtin_take(args: Vec<Value>) -> EvalResult {
    expect_args("take", &args, 2)?;
    let n = expect_int(&args[0])?;
    if n <= 0 {
        return Ok(Value::NIL);
    }
    let list = &args[1];
    if !list.is_nil() && !list.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *list],
        ));
    }

    let mut result = Vec::new();
    let mut cursor = *list;
    for _ in 0..(n as usize) {
        match cursor.kind() {
            ValueKind::Nil => break,
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                result.push(pair_car);
                cursor = pair_cdr;
            }
            _tail => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), cursor],
                ));
            }
        }
    }
    Ok(Value::list(result))
}

// ---------------------------------------------------------------------------
// String utilities (subr-x compatible)
// ---------------------------------------------------------------------------

/// `(string-search NEEDLE HAYSTACK &optional START)`.
///
/// START is a character position (not byte offset).  The return value
/// is also a character position, matching GNU Emacs semantics.
pub(crate) fn builtin_string_search(args: Vec<Value>) -> EvalResult {
    expect_min_args("string-search", &args, 2)?;
    expect_max_args("string-search", &args, 3)?;
    let needle_ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let haystack_ls = args[1].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[1]],
        )
    })?;
    let char_len = haystack_ls.schars();
    let start_char = match args.get(2) {
        None => 0,
        Some(start) if start.is_nil() => 0,
        Some(start) => {
            let n = expect_fixnum(start)?;
            if n < 0 || n as usize > char_len {
                return Err(signal(LispCondition::ArgsOutOfRange, vec![*start]));
            }
            n as usize
        }
    };

    if needle_ls.schars() > haystack_ls.schars() - start_char {
        return Ok(Value::NIL);
    }

    let haystack_bytes = haystack_ls.as_bytes();
    let start_byte = if haystack_ls.is_multibyte() {
        crate::emacs_core::emacs_char::char_to_byte_pos(haystack_bytes, start_char)
    } else {
        start_char
    };
    let search_in = &haystack_bytes[start_byte..];
    let needle_bytes_storage;
    let needle_bytes = if string_search_direct_bytes(haystack_ls, needle_ls) {
        if haystack_ls.is_multibyte()
            && needle_ls.is_multibyte()
            && haystack_ls.schars() == haystack_ls.sbytes()
            && needle_ls.schars() != needle_ls.sbytes()
        {
            return Ok(Value::NIL);
        }
        needle_ls.as_bytes()
    } else if haystack_ls.is_multibyte() {
        needle_bytes_storage =
            crate::emacs_core::emacs_char::str_to_multibyte(needle_ls.as_bytes());
        &needle_bytes_storage
    } else {
        if multibyte_string_has_non_ascii_non_raw_byte(needle_ls) {
            return Ok(Value::NIL);
        }
        needle_bytes_storage = crate::emacs_core::emacs_char::str_as_unibyte(needle_ls.as_bytes());
        &needle_bytes_storage
    };

    if let Some(byte_pos) = find_subsequence(search_in, needle_bytes) {
        let abs_byte = start_byte + byte_pos;
        let char_pos = if haystack_ls.is_multibyte() {
            crate::emacs_core::emacs_char::byte_to_char_pos(haystack_bytes, abs_byte)
        } else {
            abs_byte
        };
        Ok(Value::fixnum(char_pos as i64))
    } else {
        Ok(Value::NIL)
    }
}

fn string_search_direct_bytes(
    haystack: &crate::heap_types::LispString,
    needle: &crate::heap_types::LispString,
) -> bool {
    if haystack.is_multibyte() {
        needle.is_multibyte()
            || haystack.schars() == haystack.sbytes()
            || needle.as_bytes().is_ascii()
    } else {
        !needle.is_multibyte() || needle.schars() == needle.sbytes()
    }
}

fn multibyte_string_has_non_ascii_non_raw_byte(s: &crate::heap_types::LispString) -> bool {
    let mut pos = 0;
    let bytes = s.as_bytes();
    while pos < bytes.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
        if code > 0x7F && !crate::emacs_core::emacs_char::char_byte8_p(code) {
            return true;
        }
        pos += len;
    }
    false
}

/// Find the first occurrence of `needle` in `haystack` (byte slice search).
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------
// Predicate additions
// ---------------------------------------------------------------------------

/// `(proper-list-p OBJ)` -> length if OBJ is a proper list, nil otherwise.
pub(crate) fn builtin_proper_list_p(args: Vec<Value>) -> EvalResult {
    expect_args("proper-list-p", &args, 1)?;
    match super::value::list_length(&args[0]) {
        Some(len) => Ok(Value::fixnum(len as i64)),
        None => Ok(Value::NIL),
    }
}

/// `(subrp OBJ)` -> t if OBJ is a built-in function.
pub(crate) fn builtin_subrp(args: Vec<Value>) -> EvalResult {
    expect_args("subrp", &args, 1)?;
    Ok(Value::bool_val(args[0].as_subr_id().is_some()))
}

/// `(bare-symbol SYMBOL-OR-SYMBOL-WITH-POS)` -> symbol.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_bare_symbol(args: Vec<Value>) -> EvalResult {
    expect_args("bare-symbol", &args, 1)?;
    bare_symbol_value(args[0])
}

pub(crate) fn builtin_bare_symbol_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    bare_symbol_value(arg)
}

fn bare_symbol_value(arg: Value) -> EvalResult {
    if symbol_like_name(&arg).is_some() {
        Ok(arg)
    } else if arg.is_symbol_with_pos() {
        Ok(arg.as_symbol_with_pos_sym().unwrap())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![
                Value::list(vec![
                    Value::symbol("symbolp"),
                    Value::symbol("symbol-with-pos-p"),
                ]),
                arg,
            ],
        ))
    }
}

/// `(bare-symbol-p OBJECT)` -> t if symbol (including keyword/nil/t).
pub(crate) fn builtin_bare_symbol_p(args: Vec<Value>) -> EvalResult {
    expect_args("bare-symbol-p", &args, 1)?;
    Ok(Value::bool_val(symbol_like_name(&args[0]).is_some()))
}

/// `(byteorder)` -> `?l` on little-endian, `?B` on big-endian.
pub(crate) fn builtin_byteorder(args: Vec<Value>) -> EvalResult {
    expect_args("byteorder", &args, 0)?;
    let marker = if cfg!(target_endian = "little") {
        'l'
    } else {
        'B'
    };
    Ok(Value::fixnum(marker as i64))
}

/// `(assoc-string KEY ALIST &optional CASE-FOLD)` -> first matching cell.
pub(crate) fn builtin_assoc_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("assoc-string", &args, 2)?;
    expect_max_args("assoc-string", &args, 3)?;
    let key = args[0];
    let fold_case = args.get(2).is_some_and(|v| v.is_truthy());

    let mut cursor = args[1];
    loop {
        match cursor.kind() {
            ValueKind::Nil => return Ok(Value::NIL),
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                let entry = pair_car;
                cursor = pair_cdr;

                let entry_car = if entry.is_cons() {
                    entry.cons_car()
                } else {
                    entry
                };
                let Some(entry_key) = assoc_string_entry_name(&entry_car) else {
                    continue;
                };
                if assoc_string_compare(&key, &entry_key, fold_case)? {
                    return Ok(entry);
                }
            }
            _ => return Ok(Value::NIL),
        }
    }
}

/// `(car-less-than-car A B)` -> t if `(car A) < (car B)`.
pub(crate) fn builtin_car_less_than_car(args: Vec<Value>) -> EvalResult {
    expect_args("car-less-than-car", &args, 2)?;
    let left = list_car_or_signal(&args[0])?;
    let right = list_car_or_signal(&args[1])?;
    Ok(Value::bool_val(
        expect_number_or_marker_f64(&left)? < expect_number_or_marker_f64(&right)?,
    ))
}

/// `(byte-code-function-p OBJ)` -> t if compiled.
pub(crate) fn builtin_byte_code_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("byte-code-function-p", &args, 1)?;
    Ok(Value::bool_val(args[0].is_bytecode()))
}

/// `(compiled-function-p OBJ)` -> t if compiled function.
/// `(closurep OBJ)` -> t if closure.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_closurep(args: Vec<Value>) -> EvalResult {
    expect_args("closurep", &args, 1)?;
    Ok(Value::bool_val(
        args[0].is_lambda() || args[0].is_bytecode(),
    ))
}

pub(crate) fn builtin_closurep_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_lambda() || arg.is_bytecode()))
}

/// `(natnump OBJ)` -> t if natural number (>= 0).
pub(crate) fn builtin_natnump(args: Vec<Value>) -> EvalResult {
    expect_args("natnump", &args, 1)?;
    let is_nat = match args[0].kind() {
        ValueKind::Fixnum(n) => n >= 0,
        _ if args[0].is_bignum() => args[0]
            .as_bignum()
            .is_some_and(|value| value.significant_bits() == 0 || value > &Integer::from(0)),
        _ => false,
    };
    Ok(Value::bool_val(is_nat))
}

/// `(zerop OBJ)` -> t if zero.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_zerop(args: Vec<Value>) -> EvalResult {
    expect_args("zerop", &args, 1)?;
    let is_zero = match args[0].kind() {
        ValueKind::Fixnum(0) => true,
        ValueKind::Float => args[0].xfloat() == 0.0,
        _ => false,
    };
    Ok(Value::bool_val(is_zero))
}

// ---------------------------------------------------------------------------
// Misc operations
// ---------------------------------------------------------------------------

fn expect_uid_arg(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(uid) if uid >= 0 => Ok(uid),
        _ => Err(signal(
            "error",
            vec![Value::string(
                "Not an in-range integer, integral float, or cons of integers",
            )],
        )),
    }
}

/// `(user-login-name &optional UID)` -> string or nil.
pub(crate) fn builtin_user_login_name(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("user-login-name", &args, 1)?;
    if let Some(uid_arg) = args.first().filter(|uid| !uid.is_nil()) {
        let uid = expect_uid_arg(uid_arg)?;
        return Ok(match lookup_login_by_uid(uid) {
            Some(name) => Value::string(name),
            None => Value::NIL,
        });
    }

    if ctx
        .obarray()
        .symbol_value("user-login-name")
        .is_none_or(|value| value.is_nil())
    {
        super::runtime_identity::install(ctx);
    }

    ctx.eval_symbol_by_id(super::intern::intern("user-login-name"))
}

/// `(user-real-login-name)` -> string.
pub(crate) fn builtin_user_real_login_name(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("user-real-login-name", &args, 0)?;
    if ctx
        .obarray()
        .symbol_value("user-login-name")
        .is_none_or(|value| value.is_nil())
    {
        super::runtime_identity::install(ctx);
    }
    ctx.eval_symbol_by_id(super::intern::intern("user-real-login-name"))
}

/// `(user-full-name &optional UID-OR-LOGIN)` -> string or nil.
pub(crate) fn builtin_user_full_name(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("user-full-name", &args, 1)?;
    if let Some(target) = args.first() {
        if target.is_nil() {
            return ctx.eval_symbol_by_id(super::intern::intern("user-full-name"));
        }

        return Ok(match target.kind() {
            ValueKind::Fixnum(uid) => {
                if uid < 0 {
                    return Err(signal(
                        "error",
                        vec![Value::string(
                            "Not an in-range integer, integral float, or cons of integers",
                        )],
                    ));
                }
                lookup_full_name_by_uid(uid)
                    .map(Value::string)
                    .unwrap_or(Value::NIL)
            }
            ValueKind::String => {
                let login = target
                    .as_lisp_string()
                    .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                    .expect("ValueKind::String must carry LispString payload");
                lookup_full_name_by_login(&login)
                    .map(Value::string)
                    .unwrap_or(Value::NIL)
            }
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string(
                        "Not an in-range integer, integral float, or cons of integers",
                    )],
                ));
            }
        });
    }

    ctx.eval_symbol_by_id(super::intern::intern("user-full-name"))
}

// `fixnump` and `bignump` are not Rust subrs: GNU defines them in
// `lisp/subr.el` and so must we, otherwise `(subrp (symbol-function
// 'fixnump))` returns the wrong value.

/// `(system-name)` -> string.
/// GNU editfns.c:1283 — the zero-argument function observes the same
/// Lisp-owned runtime identity used by `frame-title-format`.
pub(crate) fn builtin_system_name(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("system-name", &args, 0)?;
    super::runtime_identity::refresh_system_name(ctx);
    ctx.eval_symbol_by_id(super::intern::intern("system-name"))
}

pub(crate) fn system_configuration_value() -> Value {
    Value::string(gnu_system_configuration())
}

pub(crate) fn system_configuration_options_value() -> Value {
    Value::string("")
}

pub(crate) fn system_configuration_features_value() -> Value {
    let mut features = vec!["PDUMPER".to_string(), "THREADS".to_string()];
    if cfg!(target_os = "linux") {
        features.push("DBUS".to_string());
    }
    features.sort_unstable();
    features.dedup();
    Value::string(features.join(" "))
}

fn gnu_system_configuration() -> String {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return "x86_64-pc-linux-gnu".to_string();
    }
    option_env!("TARGET")
        .map(str::to_owned)
        .unwrap_or_else(fallback_system_configuration)
}

fn fallback_system_configuration() -> String {
    let arch = std::env::consts::ARCH;
    let os = match std::env::consts::OS {
        "linux" => "unknown-linux-gnu",
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        other => other,
    };
    format!("{arch}-{os}")
}

/// `(emacs-version)` -> string.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_emacs_version(args: Vec<Value>) -> EvalResult {
    expect_max_args("emacs-version", &args, 1)?;
    if args.first().is_some_and(|arg| !arg.is_nil()) {
        return Ok(Value::NIL);
    }
    Ok(Value::string(
        "GNU Emacs 31.0.50 (build 1, x86_64-pc-linux-gnu) [NeoVM 0.1.0 (Neomacs)]\n Copyright (C) 2026 Free Software Foundation, Inc.",
    ))
}

/// `(emacs-pid)` -> integer.
pub(crate) fn builtin_emacs_pid(args: Vec<Value>) -> EvalResult {
    expect_args("emacs-pid", &args, 0)?;
    Ok(Value::fixnum(crate::host::process::id() as i64))
}

fn gc_bucket(name: &str, counts: &[i64]) -> Value {
    let mut items = Vec::with_capacity(counts.len() + 1);
    items.push(Value::symbol(name));
    items.extend(counts.iter().copied().map(Value::fixnum));
    Value::list(items)
}

/// Build the GC stats list (shared by eval and vm garbage-collect paths).
pub(crate) fn builtin_garbage_collect_stats() -> EvalResult {
    let counts = Value::memory_use_counts_snapshot();
    let conses = counts[0].max(0);
    let floats = counts[1].max(0);
    let vector_cells = counts[2].max(0);
    let symbols = counts[3].max(0);
    let string_chars = counts[4].max(0);
    let intervals = counts[5].max(0);
    let strings = counts[6].max(0);

    Ok(Value::list(vec![
        gc_bucket("conses", &[16, conses, 0]),
        gc_bucket("symbols", &[48, symbols, 0]),
        gc_bucket("strings", &[32, strings, 0]),
        gc_bucket("string-bytes", &[1, string_chars]),
        gc_bucket("vectors", &[16, vector_cells]),
        gc_bucket("vector-slots", &[8, vector_cells, 0]),
        gc_bucket("floats", &[8, floats, 0]),
        gc_bucket("intervals", &[56, intervals, 0]),
        gc_bucket("buffers", &[992, 0]),
    ]))
}

/// `(memory-use-counts)` -> list of runtime allocation counters:
/// `(CONS FLOATS VECTOR-CELLS SYMBOLS STRING-CHARS INTERVALS STRINGS)`.
pub(crate) fn builtin_memory_use_counts(args: Vec<Value>) -> EvalResult {
    expect_args("memory-use-counts", &args, 0)?;
    let counts = Value::memory_use_counts_snapshot();
    Ok(Value::list(
        counts.iter().map(|count| Value::fixnum(*count)).collect(),
    ))
}

fn layout_stat_pair(name: &str, value: usize) -> Value {
    let value = i64::try_from(value).unwrap_or(i64::MAX);
    Value::cons(Value::symbol(name), Value::fixnum(value))
}

fn arena_layout_value(stats: &crate::tagged::gc::ArenaLayoutStats) -> Value {
    Value::list(vec![
        layout_stat_pair("pages", stats.pages),
        layout_stat_pair("page-bytes", stats.page_bytes),
        layout_stat_pair("slot-bytes", stats.slot_bytes),
        layout_stat_pair("slots-per-page", stats.slots_per_page),
        layout_stat_pair("allocated-slots", stats.allocated_slots),
        layout_stat_pair("tenured-slots", stats.tenured_slots),
        layout_stat_pair("young-slots", stats.young_slots),
        layout_stat_pair("bumped-slots", stats.bumped_slots),
        layout_stat_pair("reclaimed-slots", stats.reclaimed_slots),
        layout_stat_pair("never-used-slots", stats.never_used_slots),
        layout_stat_pair("empty-pages", stats.empty_pages),
        layout_stat_pair("partial-pages", stats.partial_pages),
        layout_stat_pair("full-pages", stats.full_pages),
        layout_stat_pair("retired-pages", stats.retired_pages),
        layout_stat_pair("occupied-slot-bytes", stats.occupied_slot_bytes),
        layout_stat_pair("object-struct-bytes", stats.object_struct_bytes),
        layout_stat_pair("payload-logical-bytes", stats.payload_logical_bytes),
        layout_stat_pair("payload-capacity-bytes", stats.payload_capacity_bytes),
        layout_stat_pair("owned-payloads", stats.owned_payloads),
        layout_stat_pair("mapped-payloads", stats.mapped_payloads),
    ])
}

/// `(neomacs--heap-layout-stats)` -> diagnostics alist describing exact GC
/// arena occupancy and known directly-owned payload capacities.
///
/// This is deliberately Neomacs-internal rather than a GNU compatibility
/// primitive. Pair it with an OS RSS/PSS sample: the difference exposes
/// memory owned by registries, evaluator/display state, allocator metadata,
/// and nested payloads that do not live in the tagged heap's arenas.
pub(crate) fn builtin_neomacs_heap_layout_stats(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs--heap-layout-stats", &args, 0)?;
    let stats = crate::tagged::gc::with_tagged_heap(|heap| heap.layout_stats());

    let cons = Value::list(vec![
        layout_stat_pair("pages", stats.cons.pages),
        layout_stat_pair("page-bytes", stats.cons.page_bytes),
        layout_stat_pair("capacity-slots", stats.cons.capacity_slots),
        layout_stat_pair("bumped-slots", stats.cons.bumped_slots),
        layout_stat_pair("live-slots", stats.cons.live_slots),
        layout_stat_pair("reclaimed-slots", stats.cons.reclaimed_slots),
        layout_stat_pair("never-used-slots", stats.cons.never_used_slots),
        layout_stat_pair("empty-pages", stats.cons.empty_pages),
        layout_stat_pair("partial-pages", stats.cons.partial_pages),
        layout_stat_pair("full-pages", stats.cons.full_pages),
        layout_stat_pair("occupied-bytes", stats.cons.occupied_bytes),
    ]);
    let arenas = Value::list(
        stats
            .arenas
            .iter()
            .map(|arena| Value::cons(Value::symbol(arena.class), arena_layout_value(arena)))
            .collect(),
    );
    let mapped = Value::list(vec![
        layout_stat_pair("conses", stats.mapped.conses),
        layout_stat_pair("floats", stats.mapped.floats),
        layout_stat_pair("strings", stats.mapped.strings),
        layout_stat_pair("veclikes", stats.mapped.veclikes),
        layout_stat_pair("object-image-bytes", stats.mapped.object_image_bytes),
        layout_stat_pair(
            "copied-string-payloads",
            stats.mapped.copied_string_payloads,
        ),
        layout_stat_pair(
            "copied-string-capacity-bytes",
            stats.mapped.copied_string_capacity_bytes,
        ),
        layout_stat_pair(
            "copied-veclike-payloads",
            stats.mapped.copied_veclike_payloads,
        ),
        layout_stat_pair(
            "copied-veclike-capacity-bytes",
            stats.mapped.copied_veclike_capacity_bytes,
        ),
    ]);
    let boxed = Value::list(
        stats
            .boxed
            .iter()
            .map(|kind| {
                Value::cons(
                    Value::symbol(kind.class),
                    Value::list(vec![
                        layout_stat_pair("objects", kind.objects),
                        layout_stat_pair("tenured-objects", kind.tenured_objects),
                        layout_stat_pair("known-bytes", kind.known_bytes),
                    ]),
                )
            })
            .collect(),
    );

    Ok(Value::list(vec![
        layout_stat_pair("allocated-objects", stats.allocated_objects),
        layout_stat_pair("managed-live-bytes", stats.managed_live_bytes),
        layout_stat_pair("page-backing-bytes", stats.page_backing_bytes),
        layout_stat_pair(
            "known-payload-capacity-bytes",
            stats.known_payload_capacity_bytes,
        ),
        Value::cons(Value::symbol("cons"), cons),
        Value::cons(Value::symbol("arenas"), arenas),
        Value::cons(Value::symbol("mapped"), mapped),
        Value::cons(Value::symbol("boxed"), boxed),
    ]))
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
