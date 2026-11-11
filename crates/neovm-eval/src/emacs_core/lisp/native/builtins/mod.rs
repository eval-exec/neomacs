//! Built-in primitive functions.
//!
//! All functions here take pre-evaluated `Vec<Value>` arguments and return `EvalResult`.
//! The evaluator dispatches here after evaluating the argument expressions.

use crate::emacs_core::eval::subr_abi::SubrValueExt;
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::rounding_modes::RoundingMode;
#[doc(hidden)]
pub use neovm_value::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};
use std::sync::atomic::{AtomicBool, Ordering};

/// Debug flag: when true, log every dispatch_builtin call name.
/// Activated after window-setup-hook completes during startup.
static TRACE_ALL_BUILTINS: AtomicBool = AtomicBool::new(false);

pub(crate) use neovm_edit::emacs_core::buffer::lisp_string_from_buffer_bytes;
#[doc(hidden)]
pub use neovm_value::emacs_char::character_code_to_rust_char;
#[doc(hidden)]
pub use neovm_value::emacs_char::downcase_char_code_emacs_compat;
#[doc(hidden)]
pub use neovm_value::emacs_char::upcase_char_code_emacs_compat;
#[doc(hidden)]
pub use neovm_value::error::{EvalResult, Flow, LispCondition, signal};
pub(super) use neovm_value::intern::{SymId, intern, resolve_sym};
pub(super) use neovm_value::keyboard_pure::{
    KEY_CHAR_CODE_MASK, KEY_CHAR_META, convert_lucid_event_list, describe_single_key_value,
    key_sequence_values,
};
pub use neovm_value::value::*;
pub(super) use std::cell::RefCell;
pub(super) use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Transitional string character iteration
// ---------------------------------------------------------------------------

// Moved below the evaluator with the stage-3 split (C7a prep): the
// char-table container's uniprop decoder reads compressed strings through
// this helper. The historical `builtins::lisp_string_char_codes` path keeps
// working through this facade.
pub(crate) use neovm_value::strings_core::lisp_string_char_codes;

/// Return the character code at character index `idx` in `string`, or
/// `None` if `idx` is out of range. Unlike `lisp_string_char_codes`, this
/// does not allocate a `Vec<u32>` — it walks bytes only as far as needed.
/// Mirrors the byte-level access pattern used by GNU's `Faref` on strings
/// (fns.c:3108-3123).
///
/// Moved below the evaluator with the stage-3 split (C12 value-helper
/// move); this facade keeps the historical `builtins::` path working.
pub(crate) use neovm_value::strings_core::lisp_string_char_at;

/// [`lisp_string_char_at`] for the string object `value`: the index goes
/// through the one-entry position cache (GNU `string_char_to_byte`), so a
/// walk over a multibyte string by index -- `aref', `mapcar', `seq-*' --
/// steps from the previous character instead of rescanning.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn lisp_string_value_char_at(value: Value, idx: usize) -> Option<u32> {
    let string = value.as_lisp_string()?;
    // Unibyte or computed all-ASCII text: the byte is the character, and the
    // decoder's frame stays off this path.
    if string.has_one_byte_characters() {
        return string.as_bytes().get(idx).map(|&b| u32::from(b));
    }
    multibyte_string_value_char_at(value, string, idx)
}

#[inline(never)]
fn multibyte_string_value_char_at(
    value: Value,
    string: &neovm_value::heap_types::LispString,
    idx: usize,
) -> Option<u32> {
    if idx >= string.schars() {
        return None;
    }
    let byte_pos = neovm_value::string_pos_cache::string_char_to_byte(value, string, idx);
    let tail = string.as_bytes().get(byte_pos..).unwrap_or_default();
    let (cp, _) = neovm_value::heap_types::decode_declared_multibyte_char(tail);
    Some(cp.code())
}

/// Iterate character codes via a closure (avoids allocation when possible).
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn for_each_lisp_string_char(
    string: &neovm_value::heap_types::LispString,
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
    for _ in 0..string.schars() {
        let tail = bytes.get(pos..).unwrap_or_default();
        let (cp, len) = neovm_value::heap_types::decode_declared_multibyte_char(tail);
        f(cp.code());
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

#[doc(hidden)]
pub fn expect_character_code(value: &Value) -> Result<i64, Flow> {
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
pub(super) fn expect_integer_or_marker_eval(
    eval: &super::eval::Context,
    value: &Value,
) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if neovm_edit::emacs_core::marker::is_marker(value) => {
            neovm_edit::emacs_core::marker::marker_position_as_int_with_buffers(
                &eval.edit.buffers,
                value,
            )
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

/// Extract a non-negative integer, signaling `wholenump` on failure.
#[doc(hidden)]
pub fn expect_wholenump(value: &Value) -> Result<i64, Flow> {
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
        _ if neovm_edit::emacs_core::marker::is_marker(value) => Ok(NumberOrMarker::Int(
            neovm_edit::emacs_core::marker::marker_position_as_int(value)?,
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
        _ if neovm_edit::emacs_core::marker::is_marker(value) => Ok(NumberOrMarker::Int(
            neovm_edit::emacs_core::marker::marker_position_as_int_with_buffers(
                &eval.edit.buffers,
                value,
            )?,
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

// Re-export sibling modules so submodules can use `super::eval`, `neovm_edit::emacs_core::marker`, etc.
pub(super) use super::autoload;
pub(super) use super::builtins_extra;
pub(super) use super::eval;
pub(super) use super::load;
pub(super) use super::print;
pub(super) use super::subr_info;
pub(super) use crate::emacs_core::eval::kbd;
pub(super) use crate::emacs_core::eval::keymap_lookup as keymap;
pub(super) use neovm_edit::emacs_core::editfns;
pub(super) use neovm_edit::emacs_core::marker;
pub(super) use neovm_edit::emacs_core::navigation;
pub(super) use neovm_edit::emacs_core::textprop;
pub(super) use neovm_text::emacs_core::ccl;
#[doc(hidden)]
pub use neovm_text::emacs_core::charset;
pub(super) use neovm_text::emacs_core::chartable;
pub(super) use neovm_text::emacs_core::regex;
pub(super) use neovm_value::boolvec;
pub(super) use neovm_value::error;
pub(super) use neovm_value::value;

// --- Submodules ---
#[doc(hidden)]
pub mod arithmetic;
mod buffer_text_backend;
#[doc(hidden)]
pub mod collections;
#[cfg(feature = "test-support")]
pub mod cons_list;
#[cfg(not(feature = "test-support"))]
mod cons_list;
pub(crate) mod from_value;
#[doc(hidden)]
pub mod leaves;
#[doc(hidden)]
pub mod misc_pure;
#[doc(hidden)]
pub mod strings;
#[doc(hidden)]
pub mod types;

pub use arithmetic::*;
pub use buffer_text_backend::*;
pub use collections::*;
pub(crate) use cons_list::LIST_SCAN_BUDGET;
pub use cons_list::lambda_to_closure_vector;
pub use cons_list::*;
pub use from_value::*;
#[doc(hidden)]
pub use misc_pure::*;
pub use neovm_value::value::lambda_params_to_value;
pub use neovm_value::value::parse_lambda_params_from_value;
#[doc(hidden)]
pub use strings::*;
#[doc(hidden)]
pub use types::*;

// `pub(crate)` so the R2 JIT Tier-A CallBuiltinSym read shim
// (`jit::compile::neovm_jit_cbsym_read`) can DELEGATE to the GC-free buffer
// primitive bodies (`builtin_point_0`, `builtin_char_after`, ...) by name
// instead of reimplementing them (matches the sibling `navigation`/`editfns`/
// `search` modules, already crate-visible).
mod byte_code;
#[doc(hidden)]
pub mod file_notify;
// The compiled-in backend's three primitive names; GNU's one-`#ifdef` rule
// means a test may ask which watch primitives this build can answer without
// naming one OS (see `file_notify/platform/mod.rs`).
#[cfg(test)]
pub(crate) use file_notify::{BACKEND_SUBRS, REMOVE_WATCH_SEMANTICS, RemoveWatchSemantics};
pub(crate) mod fringe_bitmap;
pub(crate) mod fringe_standard_bitmaps;
#[cfg(not(feature = "test-support"))]
pub(crate) mod hash_user_callback;
#[cfg(feature = "test-support")]
pub mod hash_user_callback;
pub(crate) mod higher_order;
#[doc(hidden)]
pub mod hooks;
#[doc(hidden)]
pub mod keymaps;
#[doc(hidden)]
pub mod lcms;
#[doc(hidden)]
pub mod maphash;
#[doc(hidden)]
pub mod misc_eval;
#[doc(hidden)]
pub mod search;
mod stubs;
#[doc(hidden)]
pub use byte_code::builtin_byte_code;
#[cfg(feature = "test-support")]
pub use stubs::FACE_ATTRIBUTES_VECTOR_LEN;
#[cfg(not(feature = "test-support"))]
pub(crate) use stubs::FACE_ATTRIBUTES_VECTOR_LEN;
pub(crate) use stubs::{
    swap_world_state as swap_selection_world_state,
    world_state_placeholder as selection_world_state_placeholder,
};

#[doc(hidden)]
pub mod symbols;
#[cfg(feature = "test-support")]
pub mod treesit;
#[cfg(not(feature = "test-support"))]
mod treesit;

pub use file_notify::*;
pub use higher_order::*;
/// Test-support facade for the re-homed sort fixtures: the product's own
/// resolver and sort entry point, without publishing predicate/proof types.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod higher_order_test_support {
    pub use super::higher_order::{
        CapturedSortPredicate, builtin_sort_slice, resolve_sort_predicate_for_test,
    };
    // Owned-instance rooting surface for the re-homed root-domain oracle
    // (peer 12:57Z (a)): the cross-crate build only; eval's own test build
    // keeps the production pub(super) visibility.
    #[cfg(feature = "test-support")]
    pub use super::higher_order::{SortRootGuard, SortRoots, ValueRooting};
}
#[doc(hidden)]
pub use hooks::*;
#[doc(hidden)]
pub use keymaps::*;
#[doc(hidden)]
pub use maphash::*;
#[doc(hidden)]
pub use misc_eval::*;
pub use neovm_edit::emacs_core::buffer::*;
#[doc(hidden)]
pub use search::*;
pub use stubs::*;
pub use symbols::*;
pub use treesit::*;

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
#[doc(hidden)]
pub fn expect_lisp_string(value: &Value) -> Result<&neovm_value::heap_types::LispString, Flow> {
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
#[doc(hidden)]
pub fn expect_string_lossy(value: &Value) -> Result<String, Flow> {
    expect_lisp_string(value).map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
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
            .as_bare_symbol_id()
            .map(neovm_value::intern::resolve_lisp_visible_symbol_name)
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
/// Moved below the evaluator with the stage-3 split (C12 value-helper
/// move); this facade keeps the historical `builtins::` path working.
pub(crate) use neovm_value::strings_core::plain_str_to_lisp_string;

/// Test-only convenience: decode a string Value to a lossy `String` (valid
/// Unicode preserved, raw eight-bit -> U+FFFD). No longer produces a storage
/// string; production code uses `as_lisp_string` for byte-faithful access.
/// Test/test-support-gated so this lossy helper can never re-enter a
/// production path (issue #131).
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn lisp_string_to_runtime_string(value: Value) -> String {
    value
        .as_lisp_string()
        .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .expect("ValueKind::String must carry LispString payload")
}

// Search / regex builtins are defined at the end of this file.

/// Try to dispatch a builtin function by name. Returns None if not a known builtin.
#[doc(hidden)]
pub fn dispatch_builtin(
    eval: &mut super::eval::Context,
    name: &str,
    args: Vec<Value>,
) -> Option<EvalResult> {
    dispatch_builtin_by_id(eval, intern(name), args)
}

/// Try to dispatch a builtin function by its canonical symbol id.
#[doc(hidden)]
pub fn dispatch_builtin_by_id(
    eval: &mut super::eval::Context,
    sym_id: SymId,
    args: Vec<Value>,
) -> Option<EvalResult> {
    eval.dispatch_subr_value(<Value as SubrValueExt>::subr_from_sym_id(sym_id), args)
}

#[doc(hidden)]
pub use super::subr::{
    FixedMin1, FixedMin2, FixedMin3, FixedMin4, FixedMin5, FixedMin6, NativeFn, NoEvalPlaceholder,
    NoEvalPolicy, SubrArity, SubrSpec,
};
use neovm_value::intern::WellKnown;

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
            if args.len() == 2 && args[1].as_bare_symbol_name() == Some("window") {
                Value::NIL
            } else {
                return None;
            }
        }
        NoEvalPolicy::Native | NoEvalPolicy::RequiresEvalState => return None,
    };
    Some(Ok(value))
}

/// Test-support: dispatch a pure/stateless builtin with a caller-supplied
/// Context (the moved core tests construct one thread-lifetime fixture per
/// thread through the typed thread-exit leak wrapper).  The old cfg(test)
/// `dispatch_builtin_without_eval_state` copy was dead and removed: it held
/// an owning Context in a thread_local, which can only be dropped inside
/// TLS destruction (peer ruling 11:5xZ G5).
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn dispatch_pure_builtin_for_test(
    ctx: &mut crate::emacs_core::eval::Context,
    name: &str,
    args: Vec<Value>,
) -> Option<EvalResult> {
    let sym_id = intern(name);
    let policy = no_eval_policy_for(sym_id);
    if let Some(result) = dispatch_builtin_stateless_placeholder(policy, &args) {
        return Some(result);
    }
    if policy == NoEvalPolicy::RequiresEvalState {
        return None;
    }
    dispatch_builtin_by_id(ctx, sym_id, args)
}

#[cfg(test)]
#[path = "tests/fixed_arity_hot_subrs_test.rs"]
mod fixed_arity_hot_subrs;
#[cfg(test)]
#[path = "tests/replace_region_contents_test.rs"]
mod replace_region_contents_test;

#[cfg(test)]
#[path = "tests/closure_slot_identity_test.rs"]
mod closure_slot_identity_test;
#[cfg(test)]
#[path = "tests/make_byte_code_literals_test.rs"]
mod make_byte_code_literals_test;
#[cfg(test)]
#[path = "tests/make_closure_instance_test.rs"]
mod make_closure_instance_test;
#[cfg(test)]
#[path = "tests/tsb_literal_size.rs"]
mod tsb_literal_size_test;

#[cfg(test)]
#[path = "tests/obarray_growth_test.rs"]
mod obarray_growth_test;
#[cfg(test)]
#[path = "tests/obarray_order_test.rs"]
mod obarray_order_test;

#[cfg(test)]
#[path = "tests/assoc_callbacks_test.rs"]
mod assoc_callbacks_test;
#[cfg(test)]
#[path = "tests/predicate_walk_gc_test.rs"]
mod predicate_walk_gc_test;

// -----------------------------------------------------------------------
// Wrapper functions for builtins that need tracing or non-standard access
// -----------------------------------------------------------------------

/// `run-hooks`, with the debug trace around it.
///
/// The trace payload is an owned `String` per hook name, so building it cost
/// three allocations and a UTF-8 conversion on a call that usually finds the
/// hook nil and returns -- more than half of the 873 Ir this primitive spent
/// per call, and `run-hooks` fires on every command and most buffer changes.
/// Build it only when a subscriber is listening at DEBUG; the untraced path
/// is the bare primitive.
#[doc(hidden)]
pub fn run_hooks_traced(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return builtin_run_hooks(eval, args);
    }
    let hook_names: Vec<String> = args
        .iter()
        .filter_map(|a| a.as_symbol_name(symbol_mode).map(|s| s.to_string()))
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

#[doc(hidden)]
pub fn load_traced(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return builtin_load(eval, args);
    }
    let file_name = args.first().map(|a| format!("{}", a)).unwrap_or_default();
    tracing::debug!(file = %file_name, "load called");
    let result = builtin_load(eval, args);
    tracing::debug!(file = %file_name, ok = result.is_ok(), "load returned");
    result
}

#[doc(hidden)]
pub fn message_traced(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return builtin_message(eval, args);
    }
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

#[doc(hidden)]
pub fn coding_system_aliases(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_coding_system_aliases(&eval.coding_systems, args, mode)
}
#[doc(hidden)]
pub fn coding_system_plist(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_coding_system_plist(&eval.coding_systems, args, mode)
}
#[doc(hidden)]
pub fn coding_system_put(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_coding_system_put(&mut eval.coding_systems, args, mode)
}
#[doc(hidden)]
pub fn coding_system_base(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_coding_system_base(&eval.coding_systems, args, mode)
}
#[doc(hidden)]
pub fn coding_system_eol_type(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_coding_system_eol_type(&eval.coding_systems, args, mode)
}
#[doc(hidden)]
pub fn detect_coding_string(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    neovm_text::emacs_core::coding::builtin_detect_coding_string_in_context(eval, args)
}
#[doc(hidden)]
pub fn detect_coding_region(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    neovm_text::emacs_core::coding::builtin_detect_coding_region_in_context(eval, args)
}
#[doc(hidden)]
pub fn keyboard_coding_system(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    neovm_text::emacs_core::coding::builtin_keyboard_coding_system(&eval.coding_systems, args)
}
#[doc(hidden)]
pub fn terminal_coding_system(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    neovm_text::emacs_core::coding::builtin_terminal_coding_system(&eval.coding_systems, args)
}
#[doc(hidden)]
pub fn coding_system_priority_list(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    neovm_text::emacs_core::coding::builtin_coding_system_priority_list(&eval.coding_systems, args)
}

#[doc(hidden)]
pub fn coding_system_p(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_coding_system_p(&eval.coding_systems, args, mode)
}
#[doc(hidden)]
pub fn check_coding_system(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_check_coding_system(&eval.coding_systems, args, mode)
}
#[doc(hidden)]
pub fn check_coding_systems_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    neovm_text::emacs_core::coding::builtin_check_coding_systems_region(eval, args)
}
#[doc(hidden)]
pub fn define_coding_system_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = eval.symbol_mode();
    let result = neovm_text::emacs_core::coding::builtin_define_coding_system_internal(
        &mut eval.coding_systems,
        args.clone(),
        mode,
    )?;
    // SAFETY: the completed coding-system registration is recorded only in
    // symbol value/property cells. This helper is noncalling and preserves
    // canonical dispatch metadata and manager storage identity.
    let owner = &mut eval.owner;
    unsafe {
        eval.obarray.with_legacy_value_write(|obarray| {
            let mut vars = super::eval::VariableStore::new(obarray, Some(owner));
            neovm_text::emacs_core::coding::record_lisp_define_coding_system_internal(
                &mut vars, &args, mode,
            );
        });
    }
    Ok(result)
}
#[doc(hidden)]
pub fn define_coding_system_alias(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    let result = neovm_text::emacs_core::coding::builtin_define_coding_system_alias(
        &mut eval.coding_systems,
        args.clone(),
        mode,
    )?;
    // SAFETY: the completed coding-system registration is recorded only in
    // symbol value/property cells. This helper is noncalling and preserves
    // canonical dispatch metadata and manager storage identity.
    let owner = &mut eval.owner;
    unsafe {
        eval.obarray.with_legacy_value_write(|obarray| {
            let mut vars = super::eval::VariableStore::new(obarray, Some(owner));
            neovm_text::emacs_core::coding::record_lisp_define_coding_system_alias(
                &mut vars, &args, mode,
            );
        });
    }
    Ok(result)
}
#[doc(hidden)]
pub fn set_coding_system_priority(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let mode = eval.symbol_mode();
    let result = neovm_text::emacs_core::coding::builtin_set_coding_system_priority(
        &mut eval.coding_systems,
        args,
        mode,
    )?;
    // GNU `Fset_coding_system_priority` also rebuilds the `coding-category-list`
    // variable (coding.c) from the reordered category priorities.
    let categories =
        neovm_text::emacs_core::coding::coding_category_priority_list(&eval.coding_systems, mode);
    let list = Value::list(categories.into_iter().map(Value::symbol).collect());
    eval.set_variable("coding-category-list", list);
    Ok(result)
}
#[doc(hidden)]
pub fn set_keyboard_coding_system_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_set_keyboard_coding_system_internal(
        &mut eval.coding_systems,
        args,
        mode,
    )
}
#[doc(hidden)]
pub fn set_safe_terminal_coding_system_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_set_safe_terminal_coding_system_internal(
        &mut eval.coding_systems,
        args,
        mode,
    )
}
#[doc(hidden)]
pub fn set_terminal_coding_system_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = eval.symbol_mode();
    neovm_text::emacs_core::coding::builtin_set_terminal_coding_system_internal(
        &mut eval.coding_systems,
        args,
        mode,
    )
}

#[doc(hidden)]
pub fn region_noncontiguous_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![Value::symbol("region-beginning")]),
        Value::list(vec![Value::symbol("region-end")]),
        Value::list(vec![Value::symbol("region-noncontiguous-p")]),
    ])
}

#[doc(hidden)]
pub fn goto_char_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("goto-char--read-natnum-interactive"),
        Value::string("Go to char: "),
    ])
}

#[doc(hidden)]
pub fn insert_char_interactive_spec() -> Value {
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

#[doc(hidden)]
pub fn rename_buffer_interactive_spec() -> Value {
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

// Fixed interactive-spec spelling set for `self-insert-command`. The registry
// rebuilds this form per invocation; each entry previously re-interned its
// spelling. GNU's cmds.c defsubr carries the same fixed spec.
static SELF_INSERT_SPEC_LIST: WellKnown = WellKnown::List;
static PREFIX_NUMERIC_VALUE: WellKnown = WellKnown::PrefixNumericValue;
static CURRENT_PREFIX_ARG: WellKnown = WellKnown::CurrentPrefixArg;
static LAST_COMMAND_EVENT_SPEC: WellKnown = WellKnown::LastCommandEvent;

#[doc(hidden)]
pub fn self_insert_command_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol(&SELF_INSERT_SPEC_LIST),
        Value::list(vec![
            Value::symbol(&PREFIX_NUMERIC_VALUE),
            Value::symbol(&CURRENT_PREFIX_ARG),
        ]),
        Value::symbol(&LAST_COMMAND_EVENT_SPEC),
    ])
}

#[doc(hidden)]
pub fn delete_process_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![Value::symbol("quote"), Value::symbol("message")]),
    ])
}

#[doc(hidden)]
pub fn kill_process_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-process-name"),
            Value::string("Kill process"),
        ]),
    ])
}

#[doc(hidden)]
pub fn signal_process_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-string"),
            Value::string("Process (name or number): "),
        ]),
        Value::list(vec![Value::symbol("read-signal-name")]),
    ])
}

#[doc(hidden)]
pub fn set_file_modes_interactive_spec() -> Value {
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

#[doc(hidden)]
pub fn set_frame_height_interactive_spec() -> Value {
    set_frame_property_interactive_spec("Frame height: ", "frame-height")
}

#[doc(hidden)]
pub fn set_frame_width_interactive_spec() -> Value {
    set_frame_property_interactive_spec("Frame width: ", "frame-width")
}

#[doc(hidden)]
pub fn lossage_size_interactive_spec() -> Value {
    Value::list(vec![
        Value::symbol("list"),
        Value::list(vec![
            Value::symbol("read-number"),
            Value::string("Set maximum keystrokes to: "),
            Value::list(vec![Value::symbol("lossage-size")]),
        ]),
    ])
}

#[doc(hidden)]
pub fn transpose_regions_interactive_spec() -> Value {
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
pub(crate) fn vm_profile_reset(_eval: &mut super::eval::Context, _args: Vec<Value>) -> EvalResult {
    crate::emacs_core::bytecode::vm::vm_profile::reset();
    Ok(Value::NIL)
}

/// Diagnostics-only (feature `vm-profile`): dump the VM profiler histograms to
/// stderr with an optional LABEL (string). Returns nil. Pairs with
/// `neovm--vm-profile-reset` for a reset → workload → dump batch session.
#[cfg(feature = "vm-profile")]
pub(crate) fn vm_profile_dump(_eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
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
#[doc(hidden)]
pub fn neovm_internal_panic(_eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let message = args
        .first()
        .and_then(|v| v.as_lisp_string())
        .map(|ls| String::from_utf8_lossy(ls.as_bytes()).into_owned())
        .unwrap_or_else(|| "neovm--internal-panic".to_string());
    panic!("{message}");
}

#[cfg(test)]
#[path = "tests/positioned_symbol_test.rs"]
mod positioned_symbol_test;

/// Deferred carrier reset: the integer-width policy binding is native (a
/// `&'static` descriptor plus a `Weak`), so it may wait for reuse.
pub(crate) fn reset_carrier_builtins_thread_locals() {
    arithmetic::reset_carrier_tls();
}

/// Carrier park: replace the builtins registries that hold heap `Value`s
/// (hash-table test closures, file-notify callbacks, window-configuration
/// snapshots) with empty handles. The selection stubs and lossage size stay
/// with the World and are never touched here.
pub(crate) fn park_carrier_builtins_thread_locals() {
    collections::park_collections_thread_locals();
    file_notify::park_file_notify_thread_locals();
    hooks::park_hooks_thread_locals();
}

/// Debug-only carrier TLS fingerprint for the non-World-carried builtins TLS.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    arithmetic::carrier_tls_fingerprint(h);
    collections::carrier_tls_fingerprint(h);
    file_notify::carrier_tls_fingerprint(h);
    hooks::carrier_tls_fingerprint(h);
}
