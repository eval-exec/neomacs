//! Char-tables: GNU `PVEC_CHAR_TABLE` / `PVEC_SUB_CHAR_TABLE`
//! (`chartab.c`), as `CharTableObj` and `SubCharTableObj`: a default, a
//! parent, a purpose, an ASCII cache, 64 top-level contents slots and the
//! extra slots, with sub-char-tables of depth 1..3 below.
//!
//! There are no in-band tagged vectors (P3.2 L0.8): a vector whose slot 0
//! happens to be `--char-table--` (or `--bool-vector--`) is a plain vector,
//! as in GNU. Bool-vectors live in `boolvec`; this module re-exports their
//! builtins for the registration table.
//!
//! The pure container substrate moved below the evaluator with the stage-3
//! split (C7a): `neovm_value::chartab`. This module keeps the
//! evaluator-bound layer — subr entry points, obarray-threaded range
//! validation with GNU's `doprnt` requoting, `map-char-table`'s apply
//! loop, and the lazy `unicode-property-table-internal` loader — and
//! re-exports the container's public surface so every historical
//! `chartable::` path is unchanged.

use super::text_host::TextHost;
use crate::emacs_core::coding::{TextQuotingStyle, effective_text_quoting_style};
use neovm_value::tagged::SymbolMode;
// The variable view is EditHost::Vars; its read methods live on
// EditVariableRead, so method resolution on the projection needs it in scope.
use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::intern::{SymId, intern};
use neovm_value::value::*;

// ---------------------------------------------------------------------------
// Container facade (stage-3 C7a): the moved substrate's public surface
// ---------------------------------------------------------------------------

pub use neovm_value::chartab::{ct_ref, ct_set_single};

#[doc(hidden)]
pub use neovm_value::chartab::{
    OptimizeCharTableTest, builtin_char_table_subtype, char_table_atomic_runs_in_range,
    char_table_default, char_table_external_slots, char_table_extra_slot_value,
    char_table_has_subtype_named_with_symbols, char_table_length, char_table_local_entries,
    char_table_local_entries_with_symbols, char_table_may_hold_value_from,
    char_table_may_hold_value_from_with_symbols, char_table_ref_and_range_with_symbols,
    char_table_set_char_direct, char_table_set_range_direct, copy_char_table_with_symbols,
    ct_ascii_raw_values, ct_lookup, ct_lookup_and_range, ct_lookup_with_symbols,
    ct_ref_with_symbols, ct_set_single_with_symbols, decode_uniprop_run_length,
    display_dependency_children, for_each_char_table_mapping_with_symbols,
    for_each_non_nil_char_table_run, for_each_non_nil_char_table_run_with_symbols,
    inline_vector_tag_shape, is_char_code_property_table, is_char_table,
    make_char_table_from_external_slots, make_char_table_from_external_slots_with_symbols,
    make_char_table_value, make_char_table_with_extra_slots,
    make_sub_char_table_from_external_slots, optimize_char_table_with_symbols,
    set_char_table_ascii, set_char_table_extra_slot, sub_char_table_external_slots,
    translate_char_with_symbols, wrong_type,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum valid Unicode code point.
const MAX_CHAR: i64 = 0x3F_FFFF;
#[doc(hidden)]
pub use neovm_value::boolvec::{bool_vector_length, is_bool_vector};
#[doc(hidden)]
pub use neovm_value::value::{bump_char_table_write_tick, char_table_write_tick};
/// Extract an integer (Int or Char), signal otherwise.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(wrong_type("integerp", value)),
    }
}
/// Extract a fixnum, signaling with GNU's `fixnump` predicate name.
fn expect_fixnump(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(wrong_type("fixnump", value)),
    }
}
/// Extract a non-negative integer (for index-like args), signaling with
/// `wholenump` on any mismatch.
fn expect_wholenump(value: &Value) -> Result<i64, Flow> {
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
fn symbol_id_for_check_symbol(
    value: &Value,
    mode: neovm_value::tagged::value::SymbolMode,
) -> Result<SymId, Flow> {
    value
        .as_symbol_id(mode)
        .ok_or_else(|| wrong_type("symbolp", value))
}
fn expect_character_code(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(ch) if (0..=MAX_CHAR).contains(&ch) => Ok(ch),
        _ => Err(wrong_type("characterp", value)),
    }
}
/// Signal GNU's `error ("Invalid RANGE argument to `%s'")`
/// (`src/chartab.c:604,637`).  GNU routes every C-level `error()` message
/// through `doprnt` (`src/doprnt.c:490-505`), which requotes the grave accent
/// and apostrophe per `text-quoting-style`; in batch/UTF-8 the effective style
/// is `curve`, so the literal `` ` `` / `'` become ‘ / ’.  We reproduce that
/// requoting here from the active obarray's `text-quoting-style` rather than
/// emitting the raw ASCII quotes, so the captured error data matches GNU
/// byte-for-byte.
fn invalid_range_error(name: &str, style: TextQuotingStyle) -> Flow {
    let message = crate::emacs_core::coding::requote_c_error_message(
        &format!("Invalid RANGE argument to `{name}'"),
        style,
    );
    signal("error", vec![Value::string(message)])
}
/// The view-carrying error path: resolve the style lazily (only when the
/// range is invalid) exactly as before, from the caller's variable view.
fn invalid_range_error_with_vars<'a, V: EditVariableRead<'a>>(
    name: &str,
    obarray: Option<V>,
    mode: SymbolMode,
) -> Flow {
    let style = obarray.map_or(TextQuotingStyle::Curve, |vars| {
        effective_text_quoting_style(vars, mode)
    });
    invalid_range_error(name, style)
}
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn copy_char_table(table: Value) -> Option<Value> {
    copy_char_table_with_symbols(table, SymbolMode::Bare)
}
/// `(make-char-table SUB-TYPE &optional DEFAULT)` -- create a char-table.
///
/// If SUB-TYPE has a `char-table-extra-slots` property, its value
/// specifies how many extra slots the char-table has (0..10).
#[doc(hidden)]
pub fn builtin_make_char_table<H: TextHost>(eval: &mut H, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-char-table", &args, 1)?;
    expect_max_args("make-char-table", &args, 2)?;
    let sub_type = args[0];
    let default = if args.len() > 1 { args[1] } else { Value::NIL };
    let sub_type_id = symbol_id_for_check_symbol(&sub_type, eval.symbol_mode())?;
    // Read char-table-extra-slots property from the sub-type symbol,
    // matching GNU Emacs chartab.c:Fmake_char_table.
    let n_extras = if let Some(value) = eval
        .variables()
        .obarray()
        .get_property_id(sub_type_id, intern("char-table-extra-slots"))
    {
        if value.is_nil() {
            0
        } else {
            expect_wholenump(&value)?
        }
    } else {
        0
    };
    let n_extras = neovm_value::alloc::CharTableExtras::try_from(Value::fixnum(n_extras))?;
    Value::try_char_table(sub_type, default, n_extras)
}
#[doc(hidden)]
pub fn fill_char_table_from_fillarray(table: &Value, item: Value) -> Result<(), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    bump_char_table_write_tick();
    // GNU `fillarray` rewrites the 64 top-level content slots and the default
    // slot, but it does not rewrite the separate ASCII cache slot.
    let _ = table.with_char_table_mut(|obj| {
        obj.set_default(item);
        obj.fill_contents(item);
    });
    crate::emacs_core::substrate_hooks::note_char_table_layout_mutation();
    Ok(())
}
/// `(char-table-p OBJ)` -- return t if OBJ is a char-table.
#[doc(hidden)]
pub fn builtin_char_table_p(args: Vec<Value>) -> EvalResult {
    expect_args("char-table-p", &args, 1)?;
    Ok(Value::bool_val(is_char_table(&args[0])))
}
/// `(set-char-table-range CHAR-TABLE RANGE VALUE)` -- set entries.
///
/// RANGE may be:
/// - a character (integer/char) -- set that single entry
/// - a cons `(MIN . MAX)` -- set all characters MIN..=MAX
/// - `nil` -- set the default value
/// - `t` -- set all character entries while leaving the default slot alone
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_set_char_table_range(
    args: Vec<Value>,
    style: Option<TextQuotingStyle>,
) -> EvalResult {
    builtin_set_char_table_range_with_style(args, style, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn builtin_set_char_table_range_in_state<H: TextHost>(
    eval: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_char_table_range_with_symbols(args, Some(eval.variables()), eval.symbol_mode())
}
#[doc(hidden)]
pub fn builtin_set_char_table_range_with_symbols<'a, V: EditVariableRead<'a>>(
    args: Vec<Value>,
    obarray: Option<V>,
    mode: SymbolMode,
) -> EvalResult {
    set_char_table_range_impl(
        args,
        |name| invalid_range_error_with_vars(name, obarray, mode),
        mode,
    )
}
/// The no-view entry point: callers without an evaluator (core internals,
/// tests) pass a pre-resolved style or `None` for the batch/UTF-8 `curve`
/// default. Avoids forcing a `V` at `None` call sites.
#[doc(hidden)]
pub fn builtin_set_char_table_range_with_style(
    args: Vec<Value>,
    style: Option<TextQuotingStyle>,
    mode: SymbolMode,
) -> EvalResult {
    set_char_table_range_impl(
        args,
        |name| invalid_range_error(name, style.unwrap_or(TextQuotingStyle::Curve)),
        mode,
    )
}
fn set_char_table_range_impl<E: Fn(&str) -> Flow>(
    args: Vec<Value>,
    invalid_range: E,
    mode: SymbolMode,
) -> EvalResult {
    expect_args("set-char-table-range", &args, 3)?;
    // Every branch below writes the table (including the nil-range default,
    // which does not reach the tick-bumping slot primitives).
    bump_char_table_write_tick();
    let table = &args[0];
    let range = &args[1];
    let value = &args[2];

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }

    match mode.unwrap(*range).kind() {
        // nil -> set default
        ValueKind::Nil => {
            let _ = table.with_char_table_mut(|obj| obj.set_default(*value));
        }
        // t -> set all characters, but not the default slot.
        ValueKind::T => {
            set_char_table_ascii(*table, *value);
            let _ = table.with_char_table_mut(|obj| obj.fill_contents(*value));
        }
        // Single character
        ValueKind::Fixnum(_) => {
            let ch = match expect_character_code(range) {
                Ok(ch) => ch,
                Err(_) => return Err(invalid_range("set-char-table-range")),
            };
            char_table_set_char_direct(*table, ch, *value, mode);
        }
        // Range cons (MIN . MAX)
        ValueKind::Cons => {
            let pair_car = range.cons_car();
            let pair_cdr = range.cons_cdr();
            let min = expect_character_code(&pair_car)?;
            let max = expect_character_code(&pair_cdr)?;
            if min <= max {
                char_table_set_range_direct(*table, min, max, *value, mode);
            }
        }
        _ => return Err(invalid_range("set-char-table-range")),
    }

    crate::emacs_core::substrate_hooks::note_char_table_layout_mutation();
    Ok(*value)
}
#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// `(char-table-range CHAR-TABLE RANGE)` -- look up a value.
///
/// RANGE may be:
/// - a character -- look up that character (with parent fallback)
/// - `nil` -- return the default value
pub(crate) fn builtin_char_table_range(
    args: Vec<Value>,
    style: Option<TextQuotingStyle>,
) -> EvalResult {
    builtin_char_table_range_with_style(args, style, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn builtin_char_table_range_in_state<H: TextHost>(
    eval: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    builtin_char_table_range_with_symbols(args, Some(eval.variables()), eval.symbol_mode())
}
#[doc(hidden)]
pub fn builtin_char_table_range_with_symbols<'a, V: EditVariableRead<'a>>(
    args: Vec<Value>,
    obarray: Option<V>,
    mode: SymbolMode,
) -> EvalResult {
    char_table_range_impl(
        args,
        |name| invalid_range_error_with_vars(name, obarray, mode),
        mode,
    )
}
/// The no-view entry point; see [`builtin_set_char_table_range_with_style`].
#[doc(hidden)]
pub fn builtin_char_table_range_with_style(
    args: Vec<Value>,
    style: Option<TextQuotingStyle>,
    mode: SymbolMode,
) -> EvalResult {
    char_table_range_impl(
        args,
        |name| invalid_range_error(name, style.unwrap_or(TextQuotingStyle::Curve)),
        mode,
    )
}
fn char_table_range_impl<E: Fn(&str) -> Flow>(
    args: Vec<Value>,
    invalid_range: E,
    mode: SymbolMode,
) -> EvalResult {
    expect_args("char-table-range", &args, 2)?;
    let table = &args[0];
    let range = &args[1];

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }

    match mode.unwrap(*range).kind() {
        // The default value.
        ValueKind::Nil => Ok(char_table_default(table)),
        ValueKind::Fixnum(_) => {
            let ch = match expect_character_code(range) {
                Ok(ch) => ch,
                Err(_) => return Err(invalid_range("char-table-range")),
            };
            ct_lookup_with_symbols(table, ch, mode)
        }
        ValueKind::Cons => {
            let pair_car = range.cons_car();
            let pair_cdr = range.cons_cdr();
            let from = expect_character_code(&pair_car)?;
            let _to = expect_character_code(&pair_cdr)?;
            let (value, _run_from, _run_to) = ct_lookup_and_range(table, from, mode)?;
            Ok(value)
        }
        _ => Err(invalid_range("char-table-range")),
    }
}
/// Translate character `c` through translation `table`.
///
/// Mirrors GNU `translate_char` (character.c:151). If `table` is a
/// char-table, look up `c`; if the entry is a character, that's the
/// translation. If `table` is a list, fold left through all tables.
/// Returns `c` unchanged if no translation applies.
#[cfg(test)]
pub(crate) fn translate_char(table: &Value, c: i64) -> i64 {
    translate_char_with_symbols(table, c, SymbolMode::Bare)
}
/// Return the unified character code for `c`, given the value `val`
/// retrieved from `Vchar_unify_table`.
///
/// Mirrors GNU `maybe_unify_char` (charset.c:1606). `val` may be:
///   * nil — return `c` unchanged
///   * a fixnum — that fixnum is the unified code
///   * a charset symbol — would normally trigger `load_charset` and a
///     re-lookup. Neomacs lacks the full charset/decoder infrastructure
///     today, so we treat this case as identity. Once charsets are
///     implemented this branch should re-lookup through
///     `Vchar_unify_table` after `load_charset`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn maybe_unify_char(c: i64, val: &Value) -> i64 {
    if let Some(n) = val.as_fixnum()
        && (0..=MAX_CHAR).contains(&n)
    {
        return n;
    }
    // nil, or charset-symbol fallback — TODO: full charset support.
    c
}
/// `(char-table-parent CHAR-TABLE)` -- return the parent table (or nil).
#[doc(hidden)]
pub fn builtin_char_table_parent(args: Vec<Value>) -> EvalResult {
    expect_args("char-table-parent", &args, 1)?;
    let table = &args[0];
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    Ok(table.as_char_table_obj().unwrap().parent.get())
}
/// `(set-char-table-parent CHAR-TABLE PARENT)` -- set the parent table.
#[doc(hidden)]
pub fn builtin_set_char_table_parent(args: Vec<Value>) -> EvalResult {
    expect_args("set-char-table-parent", &args, 2)?;
    bump_char_table_write_tick();
    let table = &args[0];
    let parent = &args[1];
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }

    // parent must be nil or a char-table.
    if !parent.is_nil() && !is_char_table(parent) {
        return Err(wrong_type("char-table-p", parent));
    }

    if !parent.is_nil() {
        let mut cursor = *parent;
        while is_char_table(&cursor) {
            if cursor.bits() == table.bits() {
                return Err(signal(
                    "error",
                    vec![Value::string(
                        "Attempt to make a chartable be its own parent",
                    )],
                ));
            }
            cursor = cursor.as_char_table_obj().unwrap().parent.get();
        }
    }

    let _ = table.with_char_table_mut(|obj| obj.set_parent(*parent));
    crate::emacs_core::substrate_hooks::note_char_table_layout_mutation();
    Ok(*parent)
}
/// `(map-char-table FUNCTION CHAR-TABLE)` -- call FUNCTION for each
/// entry with a non-nil value.  FUNCTION receives `(KEY VALUE)` where
/// KEY is either a character (integer) or a cons `(FROM . TO)` for ranges.
///
/// GNU Emacs passes a shared mutable cons cell for range keys; if Lisp code
/// retains those keys, later internal mutations are observable.  Mirror that
/// behavior instead of materializing fresh range objects.
/// Returns nil.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn for_each_char_table_mapping(
    table: &Value,
    f: impl FnMut(Value, Value) -> Result<(), Flow>,
) -> Result<(), Flow> {
    for_each_char_table_mapping_with_symbols(table, f, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn builtin_map_char_table<H: TextHost>(eval: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("map-char-table", &args, 2)?;
    let func = args[0];
    let table = args[1];
    let mode = eval.symbol_mode();
    for_each_char_table_mapping_with_symbols(
        &table,
        |key, value| {
            let _ = eval.apply(func, &[key, value])?;
            Ok(())
        },
        mode,
    )?;
    Ok(Value::NIL)
}
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn optimize_char_table(table: &Value, test: OptimizeCharTableTest) -> Result<(), Flow> {
    optimize_char_table_with_symbols(table, test, SymbolMode::Bare)
}
/// `(char-table-extra-slot TABLE N)` -- get extra slot N (0-based).
#[doc(hidden)]
pub fn builtin_char_table_extra_slot(args: Vec<Value>) -> EvalResult {
    expect_args("char-table-extra-slot", &args, 2)?;
    let table = &args[0];
    let n = expect_fixnump(&args[1])?;

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    let obj = table.as_char_table_obj().unwrap();
    if n < 0 || n as usize >= obj.extras.len() {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], args[1]],
        ));
    }
    Ok(obj.extras[n as usize])
}
/// `(set-char-table-extra-slot TABLE N VALUE)` -- set extra slot N.
#[doc(hidden)]
pub fn builtin_set_char_table_extra_slot(args: Vec<Value>) -> EvalResult {
    expect_args("set-char-table-extra-slot", &args, 3)?;
    bump_char_table_write_tick();
    let table = &args[0];
    let n = expect_fixnump(&args[1])?;
    let value = &args[2];

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    let extra_len = table.as_char_table_obj().unwrap().extras.len();
    if n < 0 || n as usize >= extra_len {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], args[1]],
        ));
    }
    let _ = table.with_char_table_mut(|obj| obj.set_extra(n as usize, *value));
    crate::emacs_core::substrate_hooks::note_char_table_layout_mutation();
    Ok(*value)
}
fn expect_character(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if (0..=MAX_CHAR).contains(&n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}
fn invalid_unicode_property_table() -> Flow {
    signal(
        "error",
        vec![Value::string("Invalid Unicode property table")],
    )
}
/// `(get-unicode-property-internal CHAR-TABLE CH)`.
///
/// This mirrors GNU's C fast path for Unicode property tables: `CHAR-TABLE`
/// must have purpose `char-code-property-table` and five extra slots; a fixnum
/// decoder in extra slot 1 selects the built-in run-length decoder.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_get_unicode_property_internal(args: Vec<Value>) -> EvalResult {
    builtin_get_unicode_property_internal_with_symbols(args, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn builtin_get_unicode_property_internal_in_state<H: TextHost>(
    eval: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_unicode_property_internal_with_symbols(args, eval.symbol_mode())
}
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_get_unicode_property_internal_with_symbols(
    args: Vec<Value>,
    mode: SymbolMode,
) -> EvalResult {
    expect_args("get-unicode-property-internal", &args, 2)?;
    let table = args[0];
    let ch = expect_character(&args[1])?;

    if !is_char_table(&table) {
        return Err(wrong_type("char-table-p", &table));
    }
    if !is_char_code_property_table(&table, mode) {
        return Err(invalid_unicode_property_table());
    }

    let decoder = char_table_extra_slot_value(&table, 1).unwrap_or(Value::NIL);
    let value = ct_lookup_with_symbols(&table, ch, mode)?;
    match decoder.kind() {
        ValueKind::Fixnum(0) => Ok(decode_uniprop_run_length(table, value)),
        ValueKind::Nil => Ok(value),
        ValueKind::Fixnum(_) => Err(invalid_unicode_property_table()),
        _ => Ok(value),
    }
}
/// Encode VALUE as an element of TABLE whose elements are characters.
///
/// Mirrors GNU's `uniprop_encode_value_character` (chartab.c): nil is allowed,
/// otherwise the value must be a valid character; a non-character signals
/// `(wrong-type-argument integerp VALUE)`.
fn encode_uniprop_value_character(value: Value) -> Result<Value, Flow> {
    if value.is_nil() {
        return Ok(value);
    }
    match value.kind() {
        ValueKind::Fixnum(n) if (0..=MAX_CHAR).contains(&n) => Ok(value),
        _ => Err(wrong_type("integerp", &value)),
    }
}
/// Encode VALUE as an element of TABLE that uses run-length compression.
///
/// Mirrors GNU's `uniprop_encode_value_run_length` (chartab.c): the value must
/// already appear in the value vector (extra slot 4); a value not found signals
/// `(wrong-type-argument "Unicode property value" VALUE)`.  The stored element
/// is the fixnum index into the value vector.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn encode_uniprop_value_run_length(
    table: Value,
    value: Value,
    mode: SymbolMode,
) -> Result<Value, Flow> {
    let value_table = char_table_extra_slot_value(&table, 4)
        .filter(|v| v.is_vector())
        .and_then(|v| v.as_vector_data());
    let found = value_table.and_then(|values| {
        values
            .iter()
            .position(|entry| eq_value_swp(entry, &value, mode == SymbolMode::Positioned))
    });
    match found {
        Some(index) => Ok(Value::fixnum(index as i64)),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::string("Unicode property value"), value],
        )),
    }
}
/// Encode VALUE as an element of TABLE that uses run-length compression and
/// contains numbers as elements.
///
/// Mirrors GNU's `uniprop_encode_value_numeric` (chartab.c): the value must be a
/// fixnum (`(wrong-type-argument fixnump VALUE)` otherwise); the stored element
/// is its fixnum index into the value vector (extra slot 4), appending the value
/// to that vector if it is not already present.
fn encode_uniprop_value_numeric(
    table: Value,
    value: Value,
    mode: SymbolMode,
) -> Result<Value, Flow> {
    let _ = expect_fixnump(&value)?;
    let mut value_table: Vec<Value> = char_table_extra_slot_value(&table, 4)
        .filter(|v| v.is_vector())
        .and_then(|v| v.as_vector_data())
        .map(|values| values.to_vec())
        .unwrap_or_default();
    if let Some(index) = value_table
        .iter()
        .position(|entry| eq_value_swp(entry, &value, mode == SymbolMode::Positioned))
    {
        return Ok(Value::fixnum(index as i64));
    }
    let index = value_table.len();
    value_table.push(value);
    set_char_table_extra_slot(&table, 4, Value::vector(value_table));
    Ok(Value::fixnum(index as i64))
}
/// `(put-unicode-property-internal CHAR-TABLE CH VALUE)`.
///
/// Mirrors GNU's `Fput_unicode_property_internal` (chartab.c): validate and
/// encode VALUE via the table's encoder (selected by extra slot 2), then store
/// the encoded element with `CHAR_TABLE_SET`.  Returns nil.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_put_unicode_property_internal(args: Vec<Value>) -> EvalResult {
    builtin_put_unicode_property_internal_with_symbols(args, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn builtin_put_unicode_property_internal_in_state<H: TextHost>(
    eval: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    builtin_put_unicode_property_internal_with_symbols(args, eval.symbol_mode())
}
pub(crate) fn builtin_put_unicode_property_internal_with_symbols(
    args: Vec<Value>,
    mode: SymbolMode,
) -> EvalResult {
    expect_args("put-unicode-property-internal", &args, 3)?;
    let table = args[0];
    let ch = expect_character(&args[1])?;
    let value = args[2];

    if !is_char_table(&table) {
        return Err(wrong_type("char-table-p", &table));
    }
    if !is_char_code_property_table(&table, mode) {
        return Err(invalid_unicode_property_table());
    }

    // The encoder index lives in extra slot 2 (a fixnum or nil); nil and any
    // out-of-range index mean "no encoding" (GNU's `uniprop_get_encoder`).
    let encoded = match char_table_extra_slot_value(&table, 2).map(|v| v.kind()) {
        Some(ValueKind::Fixnum(0)) => encode_uniprop_value_character(value)?,
        Some(ValueKind::Fixnum(1)) => encode_uniprop_value_run_length(table, value, mode)?,
        Some(ValueKind::Fixnum(2)) => encode_uniprop_value_numeric(table, value, mode)?,
        _ => value,
    };

    ct_set_single_with_symbols(&table, ch, encoded, mode);
    Ok(Value::NIL)
}
// ---------------------------------------------------------------------------
// Bool-vector builtins (`boolvec`, both representations)
// ---------------------------------------------------------------------------

#[doc(hidden)]
pub use neovm_value::boolvec::{
    builtin_bool_vector, builtin_bool_vector_count_consecutive,
    builtin_bool_vector_count_population, builtin_bool_vector_exclusive_or,
    builtin_bool_vector_intersection, builtin_bool_vector_not, builtin_bool_vector_p,
    builtin_bool_vector_set_difference, builtin_bool_vector_subsetp, builtin_bool_vector_union,
    builtin_make_bool_vector,
};
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/collection_lookup_capture_test.rs"]
mod collection_lookup_capture;

#[cfg(test)]
#[path = "tests/chartable_test.rs"]
mod tests;
