//! Built-in primitive functions.
//!
//! All functions here take pre-evaluated `Vec<Value>` arguments and return `EvalResult`.
//! The evaluator dispatches here after evaluating the argument expressions.

pub(crate) use crate::emacs_core::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::rounding_modes::RoundingMode;
use std::sync::atomic::{AtomicBool, Ordering};

/// Debug flag: when true, log every dispatch_builtin call name.
/// Activated after window-setup-hook completes during startup.
static TRACE_ALL_BUILTINS: AtomicBool = AtomicBool::new(false);

pub(crate) use super::buffer::lisp_string_from_buffer_bytes;
pub(super) use super::error::{EvalResult, Flow, LispCondition, signal};
pub(super) use super::intern::{SymId, intern, resolve_sym};
pub(super) use super::keyboard::pure::{
    KEY_CHAR_CODE_MASK, KEY_CHAR_META, convert_lucid_event_list, describe_single_key_value,
    key_sequence_values,
};
pub(super) use super::value::*;
pub(super) use std::cell::RefCell;
pub(super) use std::collections::{HashMap, HashSet};
pub(crate) use strings::downcase_char_code_emacs_compat;
pub(crate) use strings::upcase_char_code_emacs_compat;

// ---------------------------------------------------------------------------
// Transitional string character iteration
// ---------------------------------------------------------------------------

/// Iterate Emacs character codes from a `LispString`.
///
/// For **multibyte** strings each character is decoded straight from the
/// Emacs-internal bytes via `string_char_unchecked`: standard UTF-8 code points
/// (including real Private-Use-Area glyphs such as nerd-font icons) and the
/// extended `0x3FFF00+byte` sequences for eight-bit raw bytes. There is no
/// in-Unicode "sentinel" remapping — that conflated real PUA characters with
/// raw bytes and corrupted them (issue #131). For **unibyte** strings each byte
/// maps to its value directly (0..255).
pub(crate) fn lisp_string_char_codes(string: &crate::heap_types::LispString) -> Vec<u32> {
    let bytes = string.as_bytes();
    if !string.is_multibyte() {
        return bytes.iter().map(|&b| b as u32).collect();
    }
    let mut out = Vec::with_capacity(string.schars());
    let mut pos = 0;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if byte < 0x80 {
            out.push(byte as u32);
            pos += 1;
            continue;
        }
        let (cp, len) = crate::emacs_core::emacs_char::string_char_unchecked(&bytes[pos..]);
        out.push(cp);
        pos += len;
    }
    out
}

/// Return the character code at character index `idx` in `string`, or
/// `None` if `idx` is out of range. Unlike `lisp_string_char_codes`, this
/// does not allocate a `Vec<u32>` — it walks bytes only as far as needed.
/// Mirrors the byte-level access pattern used by GNU's `Faref` on strings
/// (fns.c:3108-3123).
pub(crate) fn lisp_string_char_at(
    string: &crate::heap_types::LispString,
    idx: usize,
) -> Option<u32> {
    let bytes = string.as_bytes();
    if !string.is_multibyte() {
        return bytes.get(idx).map(|&b| b as u32);
    }
    if idx >= string.schars() {
        return None;
    }
    let byte_pos = crate::emacs_core::emacs_char::char_to_byte_pos(bytes, idx);
    let (cp, _) = crate::emacs_core::emacs_char::string_char_unchecked(&bytes[byte_pos..]);
    Some(cp)
}

/// Iterate character codes via a closure (avoids allocation when possible).
pub(crate) fn for_each_lisp_string_char(
    string: &crate::heap_types::LispString,
    mut f: impl FnMut(u32),
) {
    let bytes = string.as_bytes();
    if !string.is_multibyte() {
        for &b in bytes {
            f(b as u32);
        }
        return;
    }
    let mut pos = 0;
    while pos < bytes.len() {
        let (cp, len) = crate::emacs_core::emacs_char::string_char_unchecked(&bytes[pos..]);
        f(cp);
        pos += len;
    }
}

/// Reset all thread-local state in builtins (called from Context::new).
pub(crate) fn reset_builtins_thread_locals() {
    collections::reset_collections_thread_locals();
    stubs::reset_stubs_thread_locals();
    hooks::reset_hooks_thread_locals();
    symbols::reset_symbols_thread_locals();
}

pub use stubs::{NeomacsMonitorInfo, neomacs_monitor_info_snapshot, set_neomacs_monitor_info};

/// Extract an integer, signaling wrong-type-argument if not.
pub(super) fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

pub(super) fn expect_char_table_index(value: &Value) -> Result<i64, Flow> {
    let idx = expect_fixnum(value)?;
    if !(0..=0x3F_FFFF).contains(&idx) {
        maybe_trace_characterp_nil(value, "expect_char_table_index");
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        ));
    }
    Ok(idx)
}

pub(super) fn expect_char_equal_code(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if (0..=KEY_CHAR_CODE_MASK).contains(&n) => Ok(n),
        _other => {
            maybe_trace_characterp_nil(value, "expect_char_equal_code");
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), *value],
            ))
        }
    }
}

pub(super) fn expect_character_code(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) if (0..=0x3F_FFFF).contains(&c) => Ok(c),
        _other => {
            maybe_trace_characterp_nil(value, "expect_character_code");
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), *value],
            ))
        }
    }
}

pub(crate) fn character_code_to_rust_char(code: i64) -> Option<char> {
    let code = code as u32;
    char::from_u32(code).or_else(|| {
        crate::emacs_core::emacs_char::char_byte8_p(code).then(|| {
            char::from_u32(crate::emacs_core::emacs_char::char_to_byte8(code) as u32)
                .expect("raw byte values must be valid Unicode scalars")
        })
    })
}

fn maybe_trace_characterp_nil(value: &Value, source: &str) {
    if !value.is_nil() {
        return;
    }
    if std::env::var("NEOVM_TRACE_CHARACTERP_NIL").unwrap_or_default() != "1" {
        return;
    }
    eprintln!(
        "NEOVM_TRACE_CHARACTERP_NIL source={source}\n{}",
        std::backtrace::Backtrace::force_capture()
    );
}

pub(super) fn char_equal_folded(code: i64) -> Option<String> {
    char::from_u32(code as u32).map(|ch| ch.to_lowercase().collect())
}

/// Extract an integer/marker-ish position value.
///
/// GNU Emacs accepts marker designators anywhere `integer-or-marker-p`
/// is allowed, using the marker's current position.
pub(super) fn expect_integer_or_marker(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => super::marker::marker_position_as_int(value),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

pub(super) fn expect_integer_or_marker_eval(
    eval: &super::eval::Context,
    value: &Value,
) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => {
            super::marker::marker_position_as_int_eval(eval, value)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

/// Extract a non-negative integer, signaling `wholenump` on failure.
pub(super) fn expect_wholenump(value: &Value) -> Result<i64, Flow> {
    let n = match value.kind() {
        ValueKind::Fixnum(n) => n,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("wholenump"), *value],
            ));
        }
    };
    if n < 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *value],
        ));
    }
    Ok(n)
}

#[derive(Clone, Copy, Debug)]
pub(super) enum NumberOrMarker {
    Int(i64),
    Float(f64),
}

pub(super) fn expect_number_or_marker(value: &Value) -> Result<NumberOrMarker, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(NumberOrMarker::Int(n)),
        ValueKind::Float => Ok(NumberOrMarker::Float(value.xfloat())),
        // Bignums lower into f64 for the comparison/numeric path,
        // matching GNU's XFLOATINT behaviour. Callers that need
        // exact arithmetic dispatch on the Value::kind() directly.
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(NumberOrMarker::Float(
            f64::rounding_from(value.as_bignum().unwrap(), RoundingMode::Nearest).0,
        )),
        _ if super::marker::is_marker(value) => Ok(NumberOrMarker::Int(
            super::marker::marker_position_as_int(value)?,
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), *value],
        )),
    }
}

pub(super) fn expect_number_or_marker_eval(
    eval: &super::eval::Context,
    value: &Value,
) -> Result<NumberOrMarker, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(NumberOrMarker::Int(n)),
        ValueKind::Float => Ok(NumberOrMarker::Float(value.xfloat())),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(NumberOrMarker::Float(
            f64::rounding_from(value.as_bignum().unwrap(), RoundingMode::Nearest).0,
        )),
        _ if super::marker::is_marker(value) => Ok(NumberOrMarker::Int(
            super::marker::marker_position_as_int_eval(eval, value)?,
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), *value],
        )),
    }
}

/// Extract a number as f64.
pub(super) fn expect_number(value: &Value) -> Result<f64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(value.xfloat()),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            Ok(f64::rounding_from(value.as_bignum().unwrap(), RoundingMode::Nearest).0)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *value],
        )),
    }
}

pub(super) fn expect_number_or_marker_f64(value: &Value) -> Result<f64, Flow> {
    match expect_number_or_marker(value)? {
        NumberOrMarker::Int(n) => Ok(n as f64),
        NumberOrMarker::Float(f) => Ok(f),
    }
}

pub(super) fn expect_number_or_marker_f64_eval(
    eval: &super::eval::Context,
    value: &Value,
) -> Result<f64, Flow> {
    match expect_number_or_marker_eval(eval, value)? {
        NumberOrMarker::Int(n) => Ok(n as f64),
        NumberOrMarker::Float(f) => Ok(f),
    }
}

pub(super) fn expect_integer_or_marker_after_number_check(value: &Value) -> Result<i64, Flow> {
    match expect_number_or_marker(value)? {
        NumberOrMarker::Int(n) => Ok(n),
        NumberOrMarker::Float(_) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

pub(super) fn expect_integer_or_marker_after_number_check_eval(
    eval: &super::eval::Context,
    value: &Value,
) -> Result<i64, Flow> {
    match expect_number_or_marker_eval(eval, value)? {
        NumberOrMarker::Int(n) => Ok(n),
        NumberOrMarker::Float(_) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

/// True if any arg is a float (triggers float arithmetic).
pub(super) fn has_float(args: &[Value]) -> bool {
    args.iter().any(|v| v.is_float())
}

pub(super) fn normalize_string_start_arg(
    string: &str,
    start: Option<&Value>,
) -> Result<usize, Flow> {
    let Some(start_val) = start else {
        return Ok(0);
    };
    if start_val.is_nil() {
        return Ok(0);
    }

    let raw_start = expect_fixnum(start_val)?;
    let len = string.chars().count() as i64;
    let normalized = if raw_start < 0 {
        len.checked_add(raw_start)
    } else {
        Some(raw_start)
    };

    let Some(start_idx) = normalized else {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::string(string), Value::fixnum(raw_start)],
        ));
    };

    if !(0..=len).contains(&start_idx) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::string(string), Value::fixnum(raw_start)],
        ));
    }

    let start_char_idx = start_idx as usize;
    if start_char_idx == len as usize {
        return Ok(string.len());
    }

    Ok(string
        .char_indices()
        .nth(start_char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(string.len()))
}

// Re-export sibling modules so submodules can use `super::eval`, `super::marker`, etc.
pub(super) use super::autoload;
pub(super) use super::builtins_extra;
pub(super) use super::ccl;
pub(super) use super::charset;
pub(super) use super::chartable;
pub(super) use super::editfns;
pub(super) use super::error;
pub(super) use super::eval;
pub(super) use super::fileio;
pub(super) use super::kbd;
pub(super) use super::keymap;
pub(super) use super::load;
pub(super) use super::marker;
pub(super) use super::navigation;
pub(super) use super::print;
pub(super) use super::regex;
pub(super) use super::subr_info;
pub(super) use super::terminal;
pub(super) use super::textprop;
pub(super) use super::value;
pub(super) use super::window_cmds;

// --- Submodules ---
mod arithmetic;
mod buffer_text_backend;
pub(crate) mod collections;
mod cons_list;
pub(crate) mod from_value;
pub(crate) mod misc_pure;
pub(crate) mod strings;
pub(crate) mod types;

pub(crate) use arithmetic::*;
pub(crate) use buffer_text_backend::*;
pub(crate) use collections::*;
pub use cons_list::lambda_params_to_value;
pub use cons_list::lambda_to_closure_vector;
pub use cons_list::parse_lambda_params_from_value;
pub(crate) use cons_list::*;
pub(crate) use from_value::*;
pub(crate) use misc_pure::*;
pub(crate) use strings::*;
pub(crate) use types::*;

// `pub(crate)` so the R2 JIT Tier-A CallBuiltinSym read shim
// (`jit::compile::neovm_jit_cbsym_read`) can DELEGATE to the GC-free buffer
// primitive bodies (`builtin_point_0`, `builtin_char_after`, ...) by name
// instead of reimplementing them (matches the sibling `navigation`/`editfns`/
// `search` modules, already crate-visible).
mod file_notify;
pub(crate) mod fringe_bitmap;
pub(crate) mod fringe_standard_bitmaps;
pub(crate) mod gnutls;
pub(crate) mod higher_order;
mod hooks;
pub(crate) mod keymaps;
mod lcms;
pub(crate) mod misc_eval;
pub(crate) mod search;
mod stubs;
mod subrs;
pub(crate) mod symbols;
mod treesit;

pub(crate) use super::buffer::*;
pub(crate) use file_notify::*;
pub(crate) use higher_order::*;
pub(crate) use hooks::*;
pub(crate) use keymaps::*;
pub(crate) use misc_eval::*;
pub(crate) use search::*;
pub(crate) use stubs::*;
#[cfg(test)]
pub(crate) use subrs::localized_subr_catalog;
pub(crate) use subrs::register_subrs as init_builtins;
pub(crate) use symbols::*;
pub(crate) use treesit::*;

// ===========================================================================
// Helpers
// ===========================================================================

/// Borrow a string argument's payload, tied to the `Value` place it came from.
///
/// The returned lifetime is elided to `value`'s rather than `'static`
/// (DIVERGENCES.md 163). Almost every caller passes `&args[i]`, so the borrow
/// is tied to the argument slice — which is exactly what keeps the string
/// alive: `apply_internal`'s backtrace frame roots the arguments for the whole
/// subr call, the way GNU's `mark_specpdl` marks `backtrace_args`. Saying
/// `'static` claimed something stronger and stopped the compiler from
/// noticing when a borrow outlived the argument list.
pub(super) fn expect_lisp_string(value: &Value) -> Result<&crate::heap_types::LispString, Flow> {
    value.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
}

/// Validate a string argument and decode it to a Rust `String` for text-only
/// processing (display strings, names, identifiers). Valid Unicode (including
/// real Private-Use glyphs) is preserved exactly; raw eight-bit bytes become
/// U+FFFD. Callers that must preserve raw bytes use `expect_lisp_string`.
pub(super) fn expect_string_lossy(value: &Value) -> Result<String, Flow> {
    expect_lisp_string(value).map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

/// GNU's exact comparison operand: a string passes through unchanged, while a
/// symbol contributes its existing `SYMBOL_NAME` string object.
///
/// Returns the OPERAND, not a borrow of it. GNU's own primitives are written
/// the same way -- `if (SYMBOLP (s1)) s1 = SYMBOL_NAME (s1);` and only then
/// `SDATA (s1)` (`src/fns.c:344-353`) -- and it is what lets this function
/// stop claiming `'static` for a heap string: see [`StringDesignator`].
pub(super) fn expect_string_comparison_operand(
    value: &Value,
) -> Result<from_value::StringDesignator, Flow> {
    match value.kind() {
        ValueKind::String => Ok(from_value::StringDesignator::String(*value)),
        _ => value
            .as_symbol_id()
            .map(crate::emacs_core::intern::resolve_lisp_visible_symbol_name)
            .map(from_value::StringDesignator::SymbolName)
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), *value],
                )
            }),
    }
}

/// Build a `LispString` from a plain (sentinel-free) Rust `&str`, preserving the
/// caller's multibyteness choice.
///
/// Every Lisp-visible string built from a Rust `&str` (doc strings, parsed file
/// data, filenames, pdump payloads, printer output) goes through here: the str
/// carries no storage-String sentinels, so its bytes are already in Emacs
/// internal form and become the `LispString` directly. The legacy
/// storage-decode round-trip that this replaced has been retired (issue #131);
/// the storage codec now survives only inside the buffer-text/runtime-reader
/// layer (`storage_string_to_buffer_bytes`), which is unrelated to the
/// Lisp-string path.
pub(crate) fn plain_str_to_lisp_string(
    text: &str,
    multibyte: bool,
) -> crate::heap_types::LispString {
    if multibyte {
        crate::heap_types::LispString::from_emacs_bytes(text.as_bytes().to_vec())
    } else {
        crate::heap_types::LispString::from_unibyte(text.as_bytes().to_vec())
    }
}

/// Test-only convenience: decode a string Value to a lossy `String` (valid
/// Unicode preserved, raw eight-bit -> U+FFFD). No longer produces a storage
/// string; production code uses `as_lisp_string` for byte-faithful access.
/// `#[cfg(test)]`-gated so this lossy helper can never re-enter a production
/// path (issue #131).
#[cfg(test)]
pub(crate) fn lisp_string_to_runtime_string(value: Value) -> String {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .expect("ValueKind::String must carry LispString payload")
}

// Search / regex builtins are defined at the end of this file.

/// Try to dispatch a builtin function by name. Returns None if not a known builtin.
pub(crate) fn dispatch_builtin(
    eval: &mut super::eval::Context,
    name: &str,
    args: Vec<Value>,
) -> Option<EvalResult> {
    dispatch_builtin_by_id(eval, intern(name), args)
}

/// Try to dispatch a builtin function by its canonical symbol id.
pub(crate) fn dispatch_builtin_by_id(
    eval: &mut super::eval::Context,
    sym_id: SymId,
    args: Vec<Value>,
) -> Option<EvalResult> {
    eval.dispatch_subr_value(Value::subr_from_sym_id(sym_id), args)
}

use super::subr::{
    FixedMin1, FixedMin2, FixedMin3, FixedMin4, FixedMin5, FixedMin6, NativeFn, NoEvalPlaceholder,
    NoEvalPolicy, SubrArity, SubrSpec,
};

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn no_eval_policy_for(sym_id: SymId) -> NoEvalPolicy {
    super::subr::no_eval_policy(sym_id)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn dispatch_builtin_stateless_placeholder(
    policy: NoEvalPolicy,
    args: &[Value],
) -> Option<EvalResult> {
    let value = match policy {
        NoEvalPolicy::Placeholder(NoEvalPlaceholder::Nil) => Value::NIL,
        NoEvalPolicy::Placeholder(NoEvalPlaceholder::FixnumZero) => Value::fixnum(0),
        NoEvalPolicy::Placeholder(NoEvalPlaceholder::WindowLineHeight) => {
            if args.len() == 2 && args[1].as_symbol_name() == Some("window") {
                Value::NIL
            } else {
                return None;
            }
        }
        NoEvalPolicy::Native | NoEvalPolicy::RequiresEvalState => return None,
    };
    Some(Ok(value))
}

#[cfg(test)]
pub(crate) fn dispatch_builtin_without_eval_state(
    name: &str,
    args: Vec<Value>,
) -> Option<EvalResult> {
    use crate::emacs_core::eval::Context;

    thread_local! {
        static CTX: std::cell::RefCell<Context> = std::cell::RefCell::new(Context::new());
    }

    CTX.with(|cell| {
        let ctx = &mut *cell.borrow_mut();
        let sym_id = intern(name);
        let policy = no_eval_policy_for(sym_id);
        if let Some(result) = dispatch_builtin_stateless_placeholder(policy, &args) {
            return Some(result);
        }
        if policy == NoEvalPolicy::RequiresEvalState {
            return None;
        }
        dispatch_builtin_by_id(ctx, sym_id, args)
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/fixed_arity_hot_subrs.rs"]
mod fixed_arity_hot_subrs;
#[cfg(test)]
#[path = "tests/replace_region_contents.rs"]
mod replace_region_contents_test;

#[cfg(test)]
#[path = "tests/obarray_order.rs"]
mod obarray_order_test;

#[cfg(test)]
#[path = "tests/lisp_only_predicates_and_aliases.rs"]
mod lisp_only_predicates_and_aliases_test;

#[cfg(test)]
#[path = "tests/lisp_only_undo_commands.rs"]
mod lisp_only_undo_commands_test;

#[cfg(test)]
#[path = "tests/process_launchers_are_lisp_only.rs"]
mod process_launchers_are_lisp_only_test;

#[cfg(test)]
#[path = "tests/lisp_only_misc_names.rs"]
mod lisp_only_misc_names_test;

#[cfg(test)]
#[path = "tests/lisp_only_window_frame_names.rs"]
mod lisp_only_window_frame_names_test;

#[cfg(test)]
#[path = "tests/rust_subrs_shadowed_by_lisp.rs"]
mod rust_subrs_shadowed_by_lisp_test;

// -----------------------------------------------------------------------
// Wrapper functions for builtins that need tracing or non-standard access
// -----------------------------------------------------------------------

fn run_hooks_traced(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let hook_names: Vec<String> = args
        .iter()
        .filter_map(|a| a.as_symbol_name().map(|s| s.to_string()))
        .collect();
    let dominated_by_noise = hook_names
        .iter()
        .all(|h| h == "custom-define-hook" || h == "change-major-mode-hook");
    tracing::debug!(hooks = ?hook_names, noisy = dominated_by_noise, "run-hooks called");
    let result = builtin_run_hooks(eval, args);
    tracing::debug!(hooks = ?hook_names, noisy = dominated_by_noise, "run-hooks returned");
    if hook_names.iter().any(|h| h == "window-setup-hook") {
        tracing::debug!("Enabling post-startup builtin tracing");
        TRACE_ALL_BUILTINS.store(true, Ordering::Relaxed);
    }
    result
}

fn load_traced(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let file_name = args.first().map(|a| format!("{}", a)).unwrap_or_default();
    tracing::debug!(file = %file_name, "load called");
    let result = builtin_load(eval, args);
    tracing::debug!(file = %file_name, ok = result.is_ok(), "load returned");
    result
}

fn message_traced(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let msg_preview: String = args
        .first()
        .map(|a| {
            let s = format!("{}", a);
            if s.len() > 120 {
                format!("{}...", &s[..120])
            } else {
                s
            }
        })
        .unwrap_or_default();
    tracing::debug!(msg = %msg_preview, "message");
    builtin_message(eval, args)
}

fn coding_system_aliases(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_coding_system_aliases(&eval.coding_systems, args)
}
fn coding_system_plist(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_coding_system_plist(&eval.coding_systems, args)
}
fn coding_system_put(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_coding_system_put(&mut eval.coding_systems, args)
}
fn coding_system_base(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_coding_system_base(&eval.coding_systems, args)
}
fn coding_system_eol_type(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_coding_system_eol_type(&eval.coding_systems, args)
}
fn detect_coding_string(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_detect_coding_string(&eval.coding_systems, args)
}
fn detect_coding_region(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_detect_coding_region(&eval.coding_systems, &eval.buffers, args)
}
fn keyboard_coding_system(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_keyboard_coding_system(&eval.coding_systems, args)
}
fn terminal_coding_system(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_terminal_coding_system(&eval.coding_systems, args)
}
fn coding_system_priority_list(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_coding_system_priority_list(&eval.coding_systems, args)
}

fn coding_system_p(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_coding_system_p(&eval.coding_systems, args)
}
fn check_coding_system(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_check_coding_system(&eval.coding_systems, args)
}
fn check_coding_systems_region(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::coding::builtin_check_coding_systems_region(eval, args)
}
fn define_coding_system_internal(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let result = super::coding::builtin_define_coding_system_internal(
        &mut eval.coding_systems,
        args.clone(),
    )?;
    super::coding::record_lisp_define_coding_system_internal(&mut eval.obarray, &args);
    Ok(result)
}
fn define_coding_system_alias(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let result =
        super::coding::builtin_define_coding_system_alias(&mut eval.coding_systems, args.clone())?;
    super::coding::record_lisp_define_coding_system_alias(&mut eval.obarray, &args);
    Ok(result)
}
fn set_coding_system_priority(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let result = super::coding::builtin_set_coding_system_priority(&mut eval.coding_systems, args)?;
    // GNU `Fset_coding_system_priority` also rebuilds the `coding-category-list`
    // variable (coding.c) from the reordered category priorities.
    let categories = super::coding::coding_category_priority_list(&eval.coding_systems);
    let list = Value::list(categories.into_iter().map(Value::symbol).collect());
    eval.set_variable("coding-category-list", list);
    Ok(result)
}
fn set_keyboard_coding_system_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::coding::builtin_set_keyboard_coding_system_internal(&mut eval.coding_systems, args)
}
fn set_safe_terminal_coding_system_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::coding::builtin_set_safe_terminal_coding_system_internal(&mut eval.coding_systems, args)
}
fn set_terminal_coding_system_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::coding::builtin_set_terminal_coding_system_internal(&mut eval.coding_systems, args)
}

fn region_noncontiguous_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![Value::symbol("region-beginning")]),
        Value::list(vec![Value::symbol("region-end")]),
        Value::list(vec![Value::symbol("region-noncontiguous-p")]),
    ])
}

fn goto_char_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("goto-char--read-natnum-interactive"),
        Value::string("Go to char: "),
    ])
}

fn insert_char_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-char-by-name"),
            Value::string("Insert character (Unicode name or hex): "),
        ]),
        Value::list(vec![
            Value::symbol("prefix-numeric-value"),
            Value::symbol("current-prefix-arg"),
        ]),
        Value::T,
    ])
}

fn rename_buffer_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-string"),
            Value::string("Rename buffer (to new name): "),
            Value::NIL,
            Value::list(vec![
                Value::symbol("quote"),
                Value::symbol("buffer-name-history"),
            ]),
            Value::list(vec![
                Value::symbol("buffer-name"),
                Value::list(vec![Value::symbol("current-buffer")]),
            ]),
        ]),
        Value::symbol("current-prefix-arg"),
    ])
}

fn self_insert_command_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("prefix-numeric-value"),
            Value::symbol("current-prefix-arg"),
        ]),
        Value::symbol("last-command-event"),
    ])
}

fn delete_process_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![Value::symbol("quote"), Value::symbol("message")]),
    ])
}

fn kill_process_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-process-name"),
            Value::string("Kill process"),
        ]),
    ])
}

fn signal_process_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-string"),
            Value::string("Process (name or number): "),
        ]),
        Value::list(vec![Value::symbol("read-signal-name")]),
    ])
}

fn set_file_modes_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("let"),
        Value::list(vec![Value::list(vec![
            Value::symbol("file"),
            Value::list(vec![
                Value::symbol("read-file-name"),
                Value::string("File: "),
            ]),
        ])]),
        Value::list(vec![
            Value::symbol("list"),
            Value::symbol("file"),
            Value::list(vec![
                Value::symbol("read-file-modes"),
                Value::NIL,
                Value::symbol("file"),
            ]),
        ]),
    ])
}

fn set_frame_property_interactive_spec(prompt: &'static str, getter: &'static str) -> Value {
    Value::list(vec![
        Value::symbol("set-frame-property--interactive"),
        Value::string(prompt),
        Value::list(vec![Value::symbol(getter)]),
    ])
}

fn set_frame_height_interactive_spec() -> Value {
    set_frame_property_interactive_spec("Frame height: ", "frame-height")
}

fn set_frame_width_interactive_spec() -> Value {
    set_frame_property_interactive_spec("Frame width: ", "frame-width")
}

fn lossage_size_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-number"),
            Value::string("Set maximum keystrokes to: "),
            Value::list(vec![Value::symbol("lossage-size")]),
        ]),
    ])
}

fn transpose_regions_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("if"),
        Value::list(vec![
            Value::symbol("<"),
            Value::list(vec![Value::symbol("length"), Value::symbol("mark-ring")]),
            Value::fixnum(2),
        ]),
        Value::list(vec![
            Value::symbol("error"),
            Value::string("Other region must be marked before transposing two regions"),
        ]),
        Value::list(vec![
            Value::symbol("let*"),
            Value::list(vec![
                Value::list(vec![
                    Value::symbol("num"),
                    Value::list(vec![
                        Value::symbol("if"),
                        Value::symbol("current-prefix-arg"),
                        Value::list(vec![
                            Value::symbol("prefix-numeric-value"),
                            Value::symbol("current-prefix-arg"),
                        ]),
                        Value::fixnum(0),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("ring-length"),
                    Value::list(vec![Value::symbol("length"), Value::symbol("mark-ring")]),
                ]),
                Value::list(vec![
                    Value::symbol("eltnum"),
                    Value::list(vec![
                        Value::symbol("mod"),
                        Value::symbol("num"),
                        Value::symbol("ring-length"),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("eltnum2"),
                    Value::list(vec![
                        Value::symbol("mod"),
                        Value::list(vec![Value::symbol("1+"), Value::symbol("num")]),
                        Value::symbol("ring-length"),
                    ]),
                ]),
            ]),
            Value::list(vec![
                Value::symbol("list"),
                Value::list(vec![Value::symbol("point")]),
                Value::list(vec![Value::symbol("mark")]),
                Value::list(vec![
                    Value::symbol("elt"),
                    Value::symbol("mark-ring"),
                    Value::symbol("eltnum"),
                ]),
                Value::list(vec![
                    Value::symbol("elt"),
                    Value::symbol("mark-ring"),
                    Value::symbol("eltnum2"),
                ]),
            ]),
        ]),
    ])
}

/// Diagnostics-only (feature `vm-profile`): clear the VM profiler histograms
/// (OP-MIX + SUBR-MIX + the Op::Call/CallBuiltinSym entry split). Call before a
/// measured batch editing session so loadup/startup traffic is excluded.
#[cfg(feature = "vm-profile")]
fn vm_profile_reset(_eval: &mut super::eval::Context, _args: Vec<Value>) -> EvalResult {
    crate::emacs_core::bytecode::vm::vm_profile::reset();
    Ok(Value::NIL)
}

/// Diagnostics-only (feature `vm-profile`): dump the VM profiler histograms to
/// stderr with an optional LABEL (string). Returns nil. Pairs with
/// `neovm--vm-profile-reset` for a reset → workload → dump batch session.
#[cfg(feature = "vm-profile")]
fn vm_profile_dump(_eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let label = args
        .first()
        .map(|v| format!("{v}").trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "batch".to_string());
    crate::emacs_core::bytecode::vm::vm_profile::dump(&label);
    Ok(Value::NIL)
}

/// Internal test hook: panic with the optional MESSAGE argument. Exists so
/// panic-containment tests (the module ABI today, JIT shims next) can
/// originate a HOST-code panic from Lisp: a foreign Rust module's own panic
/// cannot cross its statically linked std into our `catch_unwind`, and no
/// legitimate Lisp input panics the evaluator on demand.
fn neovm_internal_panic(_eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let message = args
        .first()
        .and_then(|v| v.as_lisp_string())
        .map(|ls| String::from_utf8_lossy(ls.as_bytes()).into_owned())
        .unwrap_or_else(|| "neovm--internal-panic".to_string());
    panic!("{message}");
}
