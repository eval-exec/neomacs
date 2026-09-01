//! Char-table and bool-vector types.
//!
//! Since we cannot add new `Value` variants, these types are represented using
//! existing `Value` infrastructure:
//!
//! - **Char-table**: A `Value::Vector` whose first element is the tag symbol
//!   `--char-table--`.  The layout is:
//!   `[--char-table-- DEFAULT PARENT SUB-TYPE EXTRA-SLOTS-COUNT ...EXTRA-SLOTS... ASCII-CACHE ...DATA-PAIRS...]`
//!   where DATA-PAIRS are stored as consecutive `(char-code, value)` pairs
//!   starting after the optional ASCII cache.  The cache mirrors GNU Emacs'
//!   `ascii` char-table slot for the hot 0..127 lookup path.
//!
//! - **Bool-vector**: A `Value::Vector` whose first element is the tag symbol
//!   `--bool-vector--`.  The layout is:
//!   `[--bool-vector-- SIZE ...BITS...]`
//!   where SIZE is `Value::fixnum(length)` and each subsequent element is
//!   `Value::fixnum(0)` or `Value::fixnum(1)`.

use super::error::{EvalResult, Flow, signal};
use super::eval::{Context, push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots};
use super::intern::{NIL_SYM_ID, SymId, T_SYM_ID, intern, resolve_sym};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::tagged::header::store_value_atomic;
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// Tag constants
// ---------------------------------------------------------------------------

const CHAR_TABLE_TAG: &str = "--char-table--";
const SUB_CHAR_TABLE_TAG: &str = "--sub-char-table--";
const BOOL_VECTOR_TAG: &str = "--bool-vector--";
const CT_OPTIMIZED_PREFIX_MARKER: &str = "--char-table-optimized-prefix--";

// Char-table fixed-layout indices (after the tag at index 0):
const CT_DEFAULT: usize = 1; // default value
const CT_PARENT: usize = 2; // parent char-table or nil
const CT_SUBTYPE: usize = 3; // sub-type symbol
const CT_EXTRA_COUNT: usize = 4; // number of extra slots
const CT_EXTRA_START: usize = 5; // first extra slot (if any)
/// Maximum valid Unicode code point.
const MAX_CHAR: i64 = 0x3F_FFFF;
const CT_LOGICAL_LENGTH: i64 = MAX_CHAR + 1;
const CT_ASCII_CACHE_LEN: usize = 128;
const CT_ASCII_CACHE_MAGIC: i64 = -7_000_001;
const CT_ASCII_CACHE_PREPARED_MAGIC: i64 = -7_000_002;

const GNU_CHAR_TABLE_STANDARD_SLOTS: usize = 4 + GNU_CHAR_TABLE_CONTENT_BLOCKS_USIZE;
const GNU_CHAR_TABLE_CONTENT_BLOCKS_USIZE: usize = 64;
const GNU_CHAR_TABLE_CONTENT_START: usize = 4;
const GNU_CHAR_TABLE_ASCII_SLOT: usize = 3;
const GNU_CHARTAB_SIZE: [usize; 4] = [64, 16, 32, 128];
const GNU_CHARTAB_CHARS: [i64; 4] = [65_536, 4_096, 128, 1];

// Bool-vector fixed-layout indices:
const BV_SIZE: usize = 1; // logical length

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

/// Return `true` if `v` is a char-table (tagged vector).
pub fn is_char_table(v: &Value) -> bool {
    if v.is_char_table() {
        return true;
    }
    if v.is_vector() {
        let vec = v.as_vector_data().unwrap();
        vec.len() >= CT_EXTRA_START
            && vec[0]
                .as_symbol_id()
                .is_some_and(|id| id == char_table_tag_sym_id())
    } else {
        false
    }
}

fn is_sub_char_table(v: Value) -> bool {
    v.is_sub_char_table()
}

/// Return `true` if `v` is a bool-vector (tagged vector).
pub fn is_bool_vector(v: &Value) -> bool {
    if v.is_vector() {
        let vec = v.as_vector_data().unwrap();
        vec.len() >= 2
            && vec[0]
                .as_symbol_id()
                .is_some_and(|id| resolve_sym(id) == BOOL_VECTOR_TAG)
    } else {
        false
    }
}

/// Return the logical bit length if `v` is a bool-vector.
pub(crate) fn bool_vector_length(v: &Value) -> Option<i64> {
    if !v.is_vector() {
        return None;
    };
    let vec = v.as_vector_data().unwrap();
    if vec.len() < 2
        || vec[0]
            .as_symbol_id()
            .is_none_or(|id| resolve_sym(id) != BOOL_VECTOR_TAG)
    {
        return None;
    }
    Some(match vec[BV_SIZE].kind() {
        ValueKind::Fixnum(n) => n,
        _ => 0,
    })
}

/// Return a bool-vector element as GNU `bool_vector_ref` would expose it.
pub(crate) fn bool_vector_ref_value(v: &Value, index: usize) -> Option<Value> {
    let len = usize::try_from(bool_vector_length(v)?).ok()?;
    if index >= len {
        return None;
    }
    let vec = v.as_vector_data()?;
    let bit = vec.get(index + 2).copied()?;
    let truthy = match bit.kind() {
        ValueKind::Fixnum(n) => n != 0,
        ValueKind::Nil => false,
        _ => bit.is_truthy(),
    };
    Some(Value::bool_val(truthy))
}

/// GNU `XCHAR_TABLE (table)->defalt`: the value characters with no entry of
/// their own fall back to, or nil for a non-char-table.
pub(crate) fn char_table_default(table: &Value) -> Value {
    if !table.is_char_table() {
        return Value::NIL;
    }
    table
        .as_vector_data()
        .and_then(|vec| vec.get(CT_DEFAULT).copied())
        .unwrap_or(Value::NIL)
}

/// Return the logical sequence length if `v` is a char-table.
pub(crate) fn char_table_length(v: &Value) -> Option<i64> {
    if v.is_char_table() {
        return Some(CT_LOGICAL_LENGTH);
    }
    if !v.is_vector() {
        return None;
    };
    let vec = v.as_vector_data().unwrap();
    if vec.len() >= CT_EXTRA_START
        && vec[0]
            .as_symbol_id()
            .is_some_and(|id| id == char_table_tag_sym_id())
    {
        Some(CT_LOGICAL_LENGTH)
    } else {
        None
    }
}

fn chartab_idx(c: i64, depth: usize, min_char: i64) -> usize {
    ((c - min_char) / GNU_CHARTAB_CHARS[depth]) as usize
}

fn make_sub_char_table(depth: usize, min_char: i64, init: Value) -> Value {
    Value::make_sub_char_table(
        depth as i32,
        min_char as i32,
        vec![init; GNU_CHARTAB_SIZE[depth]],
    )
}

fn sub_char_table_contents(value: Value) -> Option<&'static [Value]> {
    value
        .as_sub_char_table_obj()
        .map(|obj| obj.contents.as_slice())
}

fn sub_char_table_set_slot(table: Value, idx: usize, value: Value) {
    let _ = table.with_sub_char_table_mut(|obj| {
        let contents = obj.contents.ensure_owned();
        if idx < contents.len() {
            contents[idx] = value;
        }
    });
}

fn sub_char_table_ref(table: Value, c: i64, is_uniprop: bool) -> Value {
    let Some(obj) = table.as_sub_char_table_obj() else {
        return table;
    };
    let depth = obj.depth as usize;
    let min_char = obj.min_char as i64;
    let idx = chartab_idx(c, depth, min_char);
    let Some(mut val) = obj.contents.get(idx).copied() else {
        return Value::NIL;
    };
    if is_uniprop && uniprop_compressed_string(val).is_some() {
        val = uniprop_table_uncompress(table, idx).unwrap_or(val);
    }
    if is_sub_char_table(val) {
        sub_char_table_ref(val, c, is_uniprop)
    } else {
        val
    }
}

fn char_table_ascii(table: Value) -> Value {
    let Some(obj) = table.as_char_table_obj() else {
        return Value::NIL;
    };
    let mut sub = obj.contents[0];
    if !is_sub_char_table(sub) {
        return sub;
    }
    sub = sub_char_table_contents(sub)
        .and_then(|contents| contents.first().copied())
        .unwrap_or(Value::NIL);
    if !is_sub_char_table(sub) {
        return sub;
    }
    let val = sub_char_table_contents(sub)
        .and_then(|contents| contents.first().copied())
        .unwrap_or(Value::NIL);
    if is_char_code_property_table(&table) && uniprop_compressed_string(val).is_some() {
        uniprop_table_uncompress(sub, 0).unwrap_or(val)
    } else {
        val
    }
}

/// Bumped by every char-table mutation primitive below. Consumers that
/// cache derived per-table state (the syntax parser's flat ASCII entry
/// table) key their cache on (table identity, this tick): any char-table
/// write anywhere invalidates all such caches, which is coarse but cheap -
/// syntax tables are effectively immutable during editing, and a refill is
/// ~18K Ir amortized over thousands of parses.
static CHAR_TABLE_WRITE_TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[inline]
pub(crate) fn char_table_write_tick() -> u64 {
    CHAR_TABLE_WRITE_TICK.load(std::sync::atomic::Ordering::Relaxed)
}

#[inline]
fn bump_char_table_write_tick() {
    CHAR_TABLE_WRITE_TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn set_char_table_ascii(table: Value, value: Value) {
    bump_char_table_write_tick();
    let _ = table.with_char_table_mut(|obj| obj.ascii = value);
}

fn set_char_table_contents(table: Value, idx: usize, value: Value) {
    bump_char_table_write_tick();
    let _ = table.with_char_table_mut(|obj| {
        if idx < obj.contents.len() {
            obj.contents[idx] = value;
        }
    });
}

fn sub_char_table_set(table: Value, c: i64, value: Value, is_uniprop: bool) {
    let Some(obj) = table.as_sub_char_table_obj() else {
        return;
    };
    let depth = obj.depth as usize;
    let min_char = obj.min_char as i64;
    let idx = chartab_idx(c, depth, min_char);
    if depth == 3 {
        sub_char_table_set_slot(table, idx, value);
        return;
    }

    let current = obj.contents.get(idx).copied().unwrap_or(Value::NIL);
    let child = if is_sub_char_table(current) {
        current
    } else if is_uniprop && uniprop_compressed_string(current).is_some() {
        uniprop_table_uncompress(table, idx).unwrap_or(current)
    } else {
        let child = make_sub_char_table(
            depth + 1,
            min_char + idx as i64 * GNU_CHARTAB_CHARS[depth],
            current,
        );
        sub_char_table_set_slot(table, idx, child);
        child
    };
    sub_char_table_set(child, c, value, is_uniprop);
}

fn char_table_set_char_direct(table: Value, c: i64, value: Value) {
    bump_char_table_write_tick();
    let Some(obj) = table.as_char_table_obj() else {
        return;
    };
    if (0..CT_ASCII_CACHE_LEN as i64).contains(&c) && is_sub_char_table(obj.ascii) {
        sub_char_table_set_slot(obj.ascii, c as usize, value);
        return;
    }

    let idx = chartab_idx(c, 0, 0);
    let current = obj.contents[idx];
    let child = if is_sub_char_table(current) {
        current
    } else {
        let child = make_sub_char_table(1, idx as i64 * GNU_CHARTAB_CHARS[0], current);
        set_char_table_contents(table, idx, child);
        child
    };
    sub_char_table_set(child, c, value, is_char_code_property_table(&table));
    if (0..CT_ASCII_CACHE_LEN as i64).contains(&c) {
        set_char_table_ascii(table, char_table_ascii(table));
    }
}

fn sub_char_table_set_range(table: Value, from: i64, to: i64, value: Value, is_uniprop: bool) {
    let Some(obj) = table.as_sub_char_table_obj() else {
        return;
    };
    let depth = obj.depth as usize;
    let min_char = obj.min_char as i64;
    let chars_in_block = GNU_CHARTAB_CHARS[depth];
    let mut i = chartab_idx(from.max(min_char), depth, min_char);
    let mut c = min_char + chars_in_block * i as i64;
    while i < GNU_CHARTAB_SIZE[depth] {
        if c > to {
            break;
        }
        if from <= c && c + chars_in_block - 1 <= to {
            sub_char_table_set_slot(table, i, value);
        } else {
            let current = obj.contents.get(i).copied().unwrap_or(Value::NIL);
            let child = if is_sub_char_table(current) {
                current
            } else if is_uniprop && uniprop_compressed_string(current).is_some() {
                uniprop_table_uncompress(table, i).unwrap_or(current)
            } else {
                let child = make_sub_char_table(depth + 1, c, current);
                sub_char_table_set_slot(table, i, child);
                child
            };
            sub_char_table_set_range(child, from, to, value, is_uniprop);
        }
        i += 1;
        c += chars_in_block;
    }
}

fn char_table_set_range_direct(table: Value, from: i64, to: i64, value: Value) {
    bump_char_table_write_tick();
    if from == to {
        char_table_set_char_direct(table, from, value);
        return;
    }
    let is_uniprop = is_char_code_property_table(&table);
    let start_idx = chartab_idx(from, 0, 0);
    let end_idx = chartab_idx(to, 0, 0);
    for idx in start_idx..=end_idx {
        let c = idx as i64 * GNU_CHARTAB_CHARS[0];
        if c > to {
            break;
        }
        if from <= c && c + GNU_CHARTAB_CHARS[0] - 1 <= to {
            set_char_table_contents(table, idx, value);
        } else {
            let current = table
                .as_char_table_obj()
                .map(|obj| obj.contents[idx])
                .unwrap_or(Value::NIL);
            let child = if is_sub_char_table(current) {
                current
            } else {
                let child = make_sub_char_table(1, c, current);
                set_char_table_contents(table, idx, child);
                child
            };
            sub_char_table_set_range(child, from, to, value, is_uniprop);
        }
    }
    if from < CT_ASCII_CACHE_LEN as i64 {
        set_char_table_ascii(table, char_table_ascii(table));
    }
}

fn uniprop_table_uncompress(table: Value, idx: usize) -> Option<Value> {
    let obj = table.as_sub_char_table_obj()?;
    let depth = obj.depth as usize;
    if depth != 2 {
        return obj.contents.get(idx).copied();
    }
    let compressed = obj.contents.get(idx).copied()?;
    uniprop_compressed_string(compressed)?;
    let min_char = obj.min_char as i64 + idx as i64 * GNU_CHARTAB_CHARS[depth];
    let mut contents = Vec::with_capacity(GNU_CHARTAB_SIZE[3]);
    for offset in 0..GNU_CHARTAB_SIZE[3] {
        contents.push(uniprop_compressed_value_at(compressed, offset as i64).unwrap_or(Value::NIL));
    }
    let child = Value::make_sub_char_table(3, min_char as i32, contents);
    sub_char_table_set_slot(table, idx, child);
    Some(child)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Signal `wrong-type-argument` with a predicate name.
fn wrong_type(pred: &str, got: &Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol(pred), *got],
    )
}

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

fn symbol_id_for_check_symbol(value: &Value) -> Result<SymId, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(NIL_SYM_ID),
        ValueKind::T => Ok(T_SYM_ID),
        ValueKind::Symbol(id) => Ok(id),
        _ => value
            .as_symbol_with_pos_sym()
            .and_then(|symbol| symbol_id_for_check_symbol(&symbol).ok())
            .ok_or_else(|| wrong_type("symbolp", value)),
    }
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
fn invalid_range_error(name: &str, obarray: Option<&super::symbol::Obarray>) -> Flow {
    // GNU resolves `text-quoting-style` from the global environment in `doprnt`.
    // When no obarray is threaded in (internal callers that pass valid ranges
    // and never reach this path, plus tests), fall back to the batch/UTF-8
    // default of `curve`, which is what `effective_text_quoting_style` returns
    // for a nil `text-quoting-style`.
    let style = obarray.map_or(
        crate::emacs_core::coding::TextQuotingStyle::Curve,
        crate::emacs_core::coding::effective_text_quoting_style,
    );
    let message = crate::emacs_core::coding::requote_c_error_message(
        &format!("Invalid RANGE argument to `{name}'"),
        style,
    );
    signal("error", vec![Value::string(message)])
}

/// Data-pairs region start index for a char-table vector.
fn ct_data_start(vec: &[Value]) -> usize {
    ct_ascii_cache_range(vec)
        .map(|range| range.end)
        .unwrap_or_else(|| ct_ascii_cache_start(vec))
}

pub(crate) fn char_table_data_start(vec: &[Value]) -> usize {
    ct_data_start(vec)
}

fn ct_ascii_cache_start(vec: &[Value]) -> usize {
    let extra_count = match vec[CT_EXTRA_COUNT].kind() {
        ValueKind::Fixnum(n) => n as usize,
        _ => 0,
    };
    CT_EXTRA_START + extra_count
}

fn ct_ascii_cache_range(vec: &[Value]) -> Option<std::ops::Range<usize>> {
    let start = ct_ascii_cache_start(vec);
    let values_start = start + 1;
    let values_end = values_start + CT_ASCII_CACHE_LEN;
    if vec.len() >= values_end
        && matches!(
            vec[start].as_fixnum(),
            Some(CT_ASCII_CACHE_MAGIC | CT_ASCII_CACHE_PREPARED_MAGIC)
        )
    {
        Some(values_start..values_end)
    } else {
        None
    }
}

fn ct_ascii_cache_magic(vec: &[Value]) -> Option<i64> {
    let start = ct_ascii_cache_start(vec);
    vec.get(start).and_then(|value| value.as_fixnum())
}

pub(crate) fn char_table_ascii_cache_range(vec: &[Value]) -> Option<std::ops::Range<usize>> {
    ct_ascii_cache_range(vec)
}

fn ct_update_ascii_cache(vec: &mut [Value], min: i64, max: i64, value: Value) {
    if min > max || max < 0 || min >= CT_ASCII_CACHE_LEN as i64 {
        return;
    }
    let Some(range) = ct_ascii_cache_range(vec) else {
        return;
    };
    let start = min.max(0) as usize;
    let end = max.min(CT_ASCII_CACHE_LEN as i64 - 1) as usize;
    for ch in start..=end {
        store_value_atomic(&mut vec[range.start + ch], value);
    }
}

fn prepare_uniprop_ascii_cache(table: &Value) {
    if table.is_char_table() {
        if is_char_code_property_table(table) {
            set_char_table_ascii(*table, char_table_ascii(*table));
        }
        return;
    }
    let Some(original) = table.as_vector_data() else {
        return;
    };
    if !is_char_code_property_vec(original) {
        return;
    }
    if ct_ascii_cache_magic(original) == Some(CT_ASCII_CACHE_PREPARED_MAGIC) {
        return;
    }
    let Some(cache_range) = ct_ascii_cache_range(original) else {
        return;
    };

    let mut vec = original.clone();
    for ch in 0..CT_ASCII_CACHE_LEN {
        vec[cache_range.start + ch] = ct_get_char(&vec, ch as i64, true).unwrap_or(Value::NIL);
    }
    vec[cache_range.start - 1] = Value::fixnum(CT_ASCII_CACHE_PREPARED_MAGIC);
    let _ = table.replace_vector_data(vec);
}

fn is_sub_char_table_literal(v: &Value) -> bool {
    if !v.is_vector() {
        return false;
    }
    let vec = v.as_vector_data().unwrap();
    vec.len() >= 3
        && vec[0]
            .as_symbol_id()
            .is_some_and(|id| id == sub_char_table_tag_sym_id())
}

fn sub_char_table_depth_min_contents(v: &Value) -> Option<(usize, i64, Vec<Value>)> {
    if !is_sub_char_table_literal(v) {
        return None;
    }
    let vec = v.as_vector_data().unwrap();
    let depth = vec.get(1)?.as_fixnum()?;
    let min_char = vec.get(2)?.as_fixnum()?;
    if !(1..=3).contains(&depth) || !(0..=MAX_CHAR).contains(&min_char) {
        return None;
    }
    Some((depth as usize, min_char, vec[3..].to_vec()))
}

/// Build the temporary reader representation for GNU `#^^[...]` literals.
///
/// GNU Emacs creates a PVEC_SUB_CHAR_TABLE directly in `lread.c`; NeoVM has no
/// dedicated `Value` variant for it, so the reader keeps a tagged vector long
/// enough for the enclosing `#^[...]` reader path to fold it into the existing
/// sparse char-table representation.
pub(crate) fn make_sub_char_table_from_external_slots(items: &[Value]) -> Result<Value, String> {
    if items.len() < 2 {
        return Err("Invalid size of sub-char-table".to_string());
    }
    let depth = items[0]
        .as_fixnum()
        .ok_or_else(|| "Invalid depth in sub-char-table".to_string())?;
    if !(1..=3).contains(&depth) {
        return Err("Invalid depth in sub-char-table".to_string());
    }
    let min_char = items[1]
        .as_fixnum()
        .ok_or_else(|| "Invalid minimum character in sub-char-table".to_string())?;
    if !(0..=MAX_CHAR).contains(&min_char) {
        return Err("Invalid minimum character in sub-char-table".to_string());
    }

    let expected = 2 + GNU_CHARTAB_SIZE[depth as usize];
    if items.len() != expected {
        return Err("Invalid size in sub-char-table".to_string());
    }

    Ok(Value::make_sub_char_table(
        depth as i32,
        min_char as i32,
        items[2..].to_vec(),
    ))
}

fn char_table_extra_count(vec: &[Value]) -> usize {
    match vec.get(CT_EXTRA_COUNT).map(|v| v.kind()) {
        Some(ValueKind::Fixnum(n)) if n >= 0 => n as usize,
        _ => 0,
    }
}

fn char_table_extra_slot_value(table: &Value, idx: usize) -> Option<Value> {
    if !is_char_table(table) {
        return None;
    }
    if table.is_char_table() {
        return table
            .as_char_table_obj()
            .and_then(|obj| obj.extras.get(idx).copied());
    }
    let vec = table.as_vector_data().unwrap();
    let extra_count = char_table_extra_count(vec);
    (idx < extra_count).then(|| vec[CT_EXTRA_START + idx])
}

fn set_char_table_extra_slot(table: &Value, idx: usize, value: Value) {
    bump_char_table_write_tick();
    if !is_char_table(table) {
        return;
    }
    if table.is_char_table() {
        let _ = table.with_char_table_mut(|obj| {
            if let Some(slot) = obj.extras.ensure_owned().get_mut(idx) {
                *slot = value;
            }
        });
        return;
    }
    let extra_count = char_table_extra_count(table.as_vector_data().unwrap());
    if idx >= extra_count {
        return;
    }
    table.with_vector_data_mut(|vec| {
        store_value_atomic(&mut vec[CT_EXTRA_START + idx], value);
    });
}

// GNU checks a char-table purpose with EQ(purpose, Qchar_code_property_table).
// Comparing by cached SymId matches that and avoids resolving the purpose
// symbol name + a strcmp on every `ct_lookup`.
fn char_code_property_table_sym_id() -> SymId {
    static ID: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| intern("char-code-property-table"))
}

/// Cached SymId of [`CHAR_TABLE_TAG`] — `is_char_table` runs per case-table
/// probe on string/buffer search paths, and resolving + strcmp'ing the tag
/// symbol's name there dominated the check.
fn char_table_tag_sym_id() -> SymId {
    static ID: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| intern(CHAR_TABLE_TAG))
}

/// Cached SymId of [`SUB_CHAR_TABLE_TAG`], same rationale.
fn sub_char_table_tag_sym_id() -> SymId {
    static ID: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| intern(SUB_CHAR_TABLE_TAG))
}

fn is_char_code_property_table(table: &Value) -> bool {
    if !is_char_table(table) {
        return false;
    }
    if table.is_char_table() {
        let obj = table.as_char_table_obj().unwrap();
        return obj.purpose.as_symbol_id() == Some(char_code_property_table_sym_id())
            && obj.extras.len() == 5;
    }
    let vec = table.as_vector_data().unwrap();
    is_char_code_property_vec(vec)
}

fn is_char_code_property_vec(vec: &[Value]) -> bool {
    vec.get(CT_SUBTYPE)
        .is_some_and(|v| v.as_symbol_id() == Some(char_code_property_table_sym_id()))
        && char_table_extra_count(vec) == 5
}

fn uniprop_compressed_string(value: Value) -> Option<Vec<u32>> {
    fn decode_byte8_pairs(codes: impl IntoIterator<Item = u32>) -> Vec<u32> {
        let raw = codes.into_iter().collect::<Vec<_>>();
        let mut decoded = Vec::with_capacity(raw.len());
        let mut pos = 0;
        while pos < raw.len() {
            if matches!(raw[pos], 0xC0 | 0xC1)
                && pos + 1 < raw.len()
                && (raw[pos + 1] & 0xC0) == 0x80
            {
                let byte = if raw[pos] == 0xC0 {
                    raw[pos + 1]
                } else {
                    raw[pos + 1].saturating_add(0x40)
                };
                decoded.push(byte);
                pos += 2;
                continue;
            }
            decoded.push(raw[pos]);
            pos += 1;
        }
        decoded
    }

    let string = value.as_lisp_string()?;
    if string.is_multibyte() {
        let codes = decode_byte8_pairs(
            crate::emacs_core::builtins::lisp_string_char_codes(string)
                .into_iter()
                .map(|code| {
                    crate::emacs_core::emacs_char::char_to_byte_safe(code)
                        .map(u32::from)
                        .unwrap_or(code)
                }),
        );
        return matches!(codes.first(), Some(1 | 2)).then_some(codes);
    }

    let bytes = string.as_bytes();
    let mut raw_codes = Vec::with_capacity(string.schars());
    let mut pos = 0;
    while pos < bytes.len() {
        let code = crate::emacs_core::emacs_char::string_char_advance(bytes, &mut pos);
        raw_codes.push(
            crate::emacs_core::emacs_char::char_to_byte_safe(code)
                .map(u32::from)
                .unwrap_or(code),
        );
    }
    let codes = decode_byte8_pairs(raw_codes);
    matches!(codes.first(), Some(1 | 2)).then_some(codes)
}

fn uniprop_compressed_value_at(value: Value, offset: i64) -> Option<Value> {
    if !(0..GNU_CHARTAB_CHARS[2]).contains(&offset) {
        return None;
    }
    let codes = uniprop_compressed_string(value)?;
    let offset = offset as u32;
    match codes.first().copied() {
        Some(1) => {
            let mut cursor = 1;
            let mut idx = codes.get(cursor).copied()?;
            cursor += 1;
            while cursor < codes.len() && idx < GNU_CHARTAB_CHARS[2] as u32 {
                if idx == offset {
                    let value = codes[cursor] as i64;
                    return Some(if value > 0 {
                        Value::fixnum(value)
                    } else {
                        Value::NIL
                    });
                }
                idx += 1;
                cursor += 1;
            }
            Some(Value::NIL)
        }
        Some(2) => {
            let mut cursor = 1;
            let mut idx = 0_u32;
            while cursor < codes.len() && idx < GNU_CHARTAB_CHARS[2] as u32 {
                let value = codes[cursor] as i64;
                cursor += 1;
                let count = if cursor < codes.len() && codes[cursor] >= 128 {
                    let count = codes[cursor] - 128;
                    cursor += 1;
                    count
                } else {
                    1
                };
                let next = idx.saturating_add(count);
                if offset >= idx && offset < next {
                    return Some(Value::fixnum(value));
                }
                idx = next;
            }
            Some(Value::NIL)
        }
        _ => None,
    }
}

fn uniprop_compressed_runs(value: Value, start: i64, end: i64) -> Option<Vec<RawEntry>> {
    if end < start || end - start + 1 != GNU_CHARTAB_CHARS[2] {
        return None;
    }
    uniprop_compressed_string(value)?;

    let mut runs = Vec::new();
    let mut run_start = start;
    let mut previous = uniprop_compressed_value_at(value, 0)?;
    for offset in 1..GNU_CHARTAB_CHARS[2] {
        let current = uniprop_compressed_value_at(value, offset)?;
        if !eq_value(&previous, &current) {
            runs.push(RawEntry {
                start: run_start,
                end: start + offset - 1,
                value: previous,
            });
            run_start = start + offset;
            previous = current;
        }
    }
    runs.push(RawEntry {
        start: run_start,
        end,
        value: previous,
    });
    Some(runs)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn flatten_uniprop_compressed_string(vec: &mut Vec<Value>, start: i64, codes: &[u32]) {
    match codes.first().copied() {
        Some(1) => {
            let mut cursor = 1;
            let Some(mut idx) = codes.get(cursor).copied().map(i64::from) else {
                return;
            };
            cursor += 1;
            while cursor < codes.len() && idx < GNU_CHARTAB_CHARS[2] {
                let value = codes[cursor] as i64;
                if value > 0 {
                    ct_set_char(vec, start + idx, Value::fixnum(value));
                }
                idx += 1;
                cursor += 1;
            }
        }
        Some(2) => {
            let mut cursor = 1;
            let mut idx = 0_i64;
            while cursor < codes.len() && idx < GNU_CHARTAB_CHARS[2] {
                let value = codes[cursor] as i64;
                cursor += 1;
                let count = if cursor < codes.len() && codes[cursor] >= 128 {
                    let count = codes[cursor] as i64 - 128;
                    cursor += 1;
                    count
                } else {
                    1
                };
                for _ in 0..count {
                    if idx >= GNU_CHARTAB_CHARS[2] {
                        break;
                    }
                    ct_set_char(vec, start + idx, Value::fixnum(value));
                    idx += 1;
                }
            }
        }
        _ => {}
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn flatten_char_table_slot(
    vec: &mut Vec<Value>,
    value: Value,
    start: i64,
    span: i64,
    is_uniprop: bool,
) {
    if value.is_nil() {
        return;
    }

    if let Some((depth, min_char, contents)) = sub_char_table_depth_min_contents(&value) {
        flatten_sub_char_table(vec, depth, min_char, &contents, is_uniprop);
        return;
    }

    if is_uniprop
        && span == GNU_CHARTAB_CHARS[2]
        && let Some(codes) = uniprop_compressed_string(value)
    {
        flatten_uniprop_compressed_string(vec, start, &codes);
        return;
    }

    let end = (start + span - 1).min(MAX_CHAR);
    if start == end {
        ct_set_char(vec, start, value);
    } else {
        ct_set_range(vec, start, end, value);
    }
}

fn ct_set_range_no_ascii_cache(vec: &mut Vec<Value>, min: i64, max: i64, value: Value) {
    bump_char_table_write_tick();
    if min > max {
        return;
    }
    vec.push(Value::cons(Value::fixnum(min), Value::fixnum(max)));
    vec.push(value);
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn flatten_sub_char_table(
    vec: &mut Vec<Value>,
    depth: usize,
    min_char: i64,
    contents: &[Value],
    is_uniprop: bool,
) {
    if depth > 3 || contents.len() != GNU_CHARTAB_SIZE[depth] {
        return;
    }
    let span = GNU_CHARTAB_CHARS[depth];
    for (idx, value) in contents.iter().copied().enumerate() {
        flatten_char_table_slot(vec, value, min_char + idx as i64 * span, span, is_uniprop);
    }
}

fn maybe_optimize_completed_translation_table(vec: &mut Vec<Value>, extra_slot: i64) {
    if extra_slot != 1 || !vec[CT_SUBTYPE].is_symbol_named("translation-table") {
        return;
    }

    let data_start = ct_data_start(vec);
    let old_slots = vec.len().saturating_sub(data_start);
    if old_slots < 128 {
        return;
    }

    let runs = ct_optimized_local_runs(vec, OptimizeCharTableTest::Eq);
    if runs.len() < 64 {
        return;
    }

    let new_slots = 2 + runs.len() * 2;
    if new_slots <= old_slots + 2 {
        ct_replace_local_entries_with_runs(vec, runs);
    }
}

/// Build a NeoVM char-table from GNU's readable `#^[...]` char-table literal.
///
/// GNU's external order is:
/// `DEFAULT PARENT PURPOSE ASCII CONTENTS[64] EXTRAS...`.
pub(crate) fn make_char_table_from_external_slots(items: &[Value]) -> Result<Value, String> {
    if items.len() < GNU_CHAR_TABLE_STANDARD_SLOTS {
        return Err("Invalid size char-table".to_string());
    }

    let default = items[0];
    let parent = items[1];
    let purpose = items[2];
    let extra_count = items.len() - GNU_CHAR_TABLE_STANDARD_SLOTS;
    let table = Value::make_char_table(purpose, default, extra_count);
    let _ = table.with_char_table_mut(|obj| {
        obj.parent = parent;
        obj.ascii = items[GNU_CHAR_TABLE_ASCII_SLOT];
        obj.contents
            .copy_from_slice(&items[GNU_CHAR_TABLE_CONTENT_START..GNU_CHAR_TABLE_STANDARD_SLOTS]);
        obj.extras
            .ensure_owned()
            .clone_from(&items[GNU_CHAR_TABLE_STANDARD_SLOTS..].to_vec());
    });
    if purpose.as_symbol_id() == Some(char_code_property_table_sym_id()) && extra_count == 5 {
        set_char_table_ascii(table, char_table_ascii(table));
    }
    Ok(table)
}

fn copy_sub_char_table_direct(table: Value) -> Option<Value> {
    let obj = table.as_sub_char_table_obj()?;
    let depth = obj.depth as usize;
    let min_char = obj.min_char;
    let contents: Vec<Value> = obj
        .contents
        .iter()
        .map(|value| {
            if is_sub_char_table(*value) {
                copy_sub_char_table_direct(*value).unwrap_or(*value)
            } else {
                *value
            }
        })
        .collect();
    Some(Value::make_sub_char_table(depth as i32, min_char, contents))
}

pub(crate) fn copy_char_table(table: Value) -> Option<Value> {
    if table.is_char_table() {
        let obj = table.as_char_table_obj()?;
        let copy = Value::make_char_table(obj.purpose, obj.defalt, obj.extras.len());
        let contents = obj.contents.map(|value| {
            if is_sub_char_table(value) {
                copy_sub_char_table_direct(value).unwrap_or(value)
            } else {
                value
            }
        });
        let extras = obj.extras.to_vec();
        let _ = copy.with_char_table_mut(|copy_obj| {
            copy_obj.parent = obj.parent;
            copy_obj.contents = contents;
            copy_obj.extras.ensure_owned().clone_from(&extras);
        });
        set_char_table_ascii(copy, char_table_ascii(copy));
        return Some(copy);
    }

    if is_char_table(&table) && table.is_vector() {
        return table
            .as_vector_data()
            .map(|items| Value::vector(items.clone()));
    }

    None
}

// ---------------------------------------------------------------------------
// Char-table builtins
// ---------------------------------------------------------------------------

/// Create a char-table `Value` directly (for use in bootstrap code).
pub fn make_char_table_value(sub_type: Value, default: Value) -> Value {
    make_char_table_with_extra_slots(sub_type, default, 0)
}

/// Create a char-table with a specified number of extra slots.
pub fn make_char_table_with_extra_slots(sub_type: Value, default: Value, n_extras: i64) -> Value {
    Value::make_char_table(sub_type, default, n_extras.max(0) as usize)
}

/// Look up a single character entry in a char-table Value.
///
/// Mirrors GNU `char_table_ref` / `CHAR_TABLE_REF` (the lookup behind
/// `DISP_CHAR_VECTOR`): returns the char's own entry, else the table default,
/// else the parent's entry, else nil.  Used by the display engine to resolve
/// per-character display-table glyph vectors.  Returns nil for a non-char-table
/// or out-of-range char rather than signalling.
pub fn ct_ref(table: &Value, ch: i64) -> Value {
    if !(0..=MAX_CHAR).contains(&ch) {
        return Value::NIL;
    }
    ct_lookup(table, ch).unwrap_or(Value::NIL)
}

/// Set a single character entry in a char-table Value (for bootstrap code).
/// Panics if `table` is not a char-table Vector.
pub fn ct_set_single(table: &Value, ch: i64, value: Value) {
    bump_char_table_write_tick();
    if table.is_char_table() {
        char_table_set_char_direct(*table, ch, value);
        return;
    }
    if table.is_vector() {
        table.with_vector_data_mut(|vec| {
            ct_set_char(vec, ch, value);
        });
    } else {
        panic!("ct_set_single: expected char-table Vector");
    }
}

/// `(make-char-table SUB-TYPE &optional DEFAULT)` -- create a char-table.
///
/// If SUB-TYPE has a `char-table-extra-slots` property, its value
/// specifies how many extra slots the char-table has (0..10).
pub(crate) fn builtin_make_char_table(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-char-table", &args, 1)?;
    expect_max_args("make-char-table", &args, 2)?;
    let sub_type = args[0];
    let default = if args.len() > 1 { args[1] } else { Value::NIL };
    let sub_type_id = symbol_id_for_check_symbol(&sub_type)?;
    // Read char-table-extra-slots property from the sub-type symbol,
    // matching GNU Emacs chartab.c:Fmake_char_table.
    let n_extras = if let Some(value) = eval
        .obarray
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
    Ok(make_char_table_with_extra_slots(
        sub_type, default, n_extras,
    ))
}

pub(crate) fn fill_char_table_from_fillarray(table: &Value, item: Value) -> Result<(), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        let _ = table.with_char_table_mut(|obj| {
            obj.defalt = item;
            obj.contents.fill(item);
        });
        crate::window::note_char_table_layout_mutation();
        return Ok(());
    }

    let mut vec = table.as_vector_data().unwrap().clone();
    vec[CT_DEFAULT] = item;
    // GNU `fillarray` rewrites the 64 top-level content slots and the default
    // slot, but it does not rewrite the separate ASCII cache slot.
    ct_set_range_no_ascii_cache(&mut vec, 0, MAX_CHAR, item);
    let _ = table.replace_vector_data(vec);
    crate::window::note_char_table_layout_mutation();
    Ok(())
}

/// `(char-table-p OBJ)` -- return t if OBJ is a char-table.
pub(crate) fn builtin_char_table_p(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_set_char_table_range(
    args: Vec<Value>,
    obarray: Option<&super::symbol::Obarray>,
) -> EvalResult {
    expect_args("set-char-table-range", &args, 3)?;
    let table = &args[0];
    let range = &args[1];
    let value = &args[2];

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }

    match range.kind() {
        // nil -> set default
        ValueKind::Nil => {
            if table.is_char_table() {
                let _ = table.with_char_table_mut(|obj| obj.defalt = *value);
                crate::window::note_char_table_layout_mutation();
                return Ok(*value);
            }
            table.with_vector_data_mut(|vec| {
                store_value_atomic(&mut vec[CT_DEFAULT], *value);
            });
        }
        // t -> set all characters, but not the default slot.
        ValueKind::T => {
            if table.is_char_table() {
                set_char_table_ascii(*table, *value);
                let _ = table.with_char_table_mut(|obj| obj.contents.fill(*value));
                crate::window::note_char_table_layout_mutation();
                return Ok(*value);
            }
            let key = Value::cons(Value::fixnum(0), Value::fixnum(MAX_CHAR));
            table.with_vector_data_mut(|vec| {
                ct_push_range_entry(vec, 0, MAX_CHAR, key, *value);
            });
        }
        // Single character
        ValueKind::Fixnum(_) => {
            let ch = match expect_character_code(range) {
                Ok(ch) => ch,
                Err(_) => return Err(invalid_range_error("set-char-table-range", obarray)),
            };
            if table.is_char_table() {
                char_table_set_char_direct(*table, ch, *value);
                crate::window::note_char_table_layout_mutation();
                return Ok(*value);
            }
            table.with_vector_data_mut(|vec| {
                ct_set_char(vec, ch, *value);
            });
        }
        // Range cons (MIN . MAX)
        ValueKind::Cons => {
            let pair_car = range.cons_car();
            let pair_cdr = range.cons_cdr();
            let min = expect_character_code(&pair_car)?;
            let max = expect_character_code(&pair_cdr)?;
            if min <= max {
                if table.is_char_table() {
                    char_table_set_range_direct(*table, min, max, *value);
                    crate::window::note_char_table_layout_mutation();
                    return Ok(*value);
                }
                let key = Value::cons(Value::fixnum(min), Value::fixnum(max));
                table.with_vector_data_mut(|vec| {
                    ct_push_range_entry(vec, min, max, key, *value);
                });
            }
        }
        _ => return Err(invalid_range_error("set-char-table-range", obarray)),
    }

    crate::window::note_char_table_layout_mutation();
    Ok(*value)
}

/// Set a single character entry in the char-table's data pairs.
fn ct_set_char(vec: &mut Vec<Value>, ch: i64, value: Value) {
    ct_update_ascii_cache(vec, ch, ch, value);
    vec.push(Value::fixnum(ch));
    vec.push(value);
}

/// Set a range entry in the char-table's data pairs.
/// The range is stored as a `Cons(min . max)` key.
fn ct_set_range(vec: &mut Vec<Value>, min: i64, max: i64, value: Value) {
    bump_char_table_write_tick();
    // Store an internal range key, not the caller's cons.  GNU's char-table
    // storage records bounds; Lisp-visible range conses from `map-char-table`
    // are reusable mutable objects.
    let key = Value::cons(Value::fixnum(min), Value::fixnum(max));
    ct_push_range_entry(vec, min, max, key, value);
}

fn ct_push_range_entry(vec: &mut Vec<Value>, min: i64, max: i64, key: Value, value: Value) {
    ct_update_ascii_cache(vec, min, max, value);
    vec.push(key);
    vec.push(value);
}

fn ct_optimized_prefix_range(vec: &[Value], data_start: usize) -> Option<std::ops::Range<usize>> {
    if vec.len() < data_start + 2 || !vec[data_start].is_symbol_named(CT_OPTIMIZED_PREFIX_MARKER) {
        return None;
    }
    let pair_count = usize::try_from(vec[data_start + 1].as_fixnum()?).ok()?;
    let prefix_start = data_start + 2;
    let prefix_end = prefix_start.checked_add(pair_count.checked_mul(2)?)?;
    if prefix_end <= vec.len() {
        Some(prefix_start..prefix_end)
    } else {
        None
    }
}

fn ct_entry_value_for_char(value: Value, min: i64, max: i64, ch: i64, is_uniprop: bool) -> Value {
    if is_uniprop
        && max - min + 1 == GNU_CHARTAB_CHARS[2]
        && let Some(decoded) = uniprop_compressed_value_at(value, ch - min)
    {
        decoded
    } else {
        value
    }
}

fn ct_get_char_from_sorted_prefix(
    vec: &[Value],
    prefix: std::ops::Range<usize>,
    ch: i64,
    is_uniprop: bool,
) -> Option<Value> {
    let mut lo = 0usize;
    let mut hi = prefix.len() / 2;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let idx = prefix.start + mid * 2;
        let (min, max) = key_span(vec[idx])?;
        if ch < min {
            hi = mid;
        } else if ch > max {
            lo = mid + 1;
        } else {
            return Some(ct_entry_value_for_char(
                vec[idx + 1],
                min,
                max,
                ch,
                is_uniprop,
            ));
        }
    }
    None
}

/// Look up a single character in the data pairs (no parent/default fallback).
/// The last assignment that covers the character wins, matching GNU Emacs
/// `set-char-table-range` overwrite semantics for both single-char and range
/// entries.
fn ct_get_char(vec: &[Value], ch: i64, is_uniprop: bool) -> Option<Value> {
    let start = ct_data_start(vec);
    let len = vec.len();
    if len < start + 2 {
        return None;
    }
    let optimized_prefix = ct_optimized_prefix_range(vec, start);
    let reverse_stop = optimized_prefix.as_ref().map_or(start, |range| range.end);
    // Scan right-to-left so the first match seen is the most recently
    // pushed entry — matching the "last assignment wins" semantic of
    // `set-char-table-range` without needing to scan every pair on
    // every call. The hot font-lock/syntax-ppss path pounds this
    // function millions of times per fontification; the old
    // unconditional O(N) scan was the dominant cost on a 147-char
    // *scratch* buffer (see commit note).
    let mut i = len; // walk backwards two slots at a time
    while i >= reverse_stop + 2 {
        i -= 2;
        let key = vec[i];
        match key.kind() {
            ValueKind::Fixnum(existing) => {
                if existing == ch {
                    return Some(vec[i + 1]);
                }
            }
            ValueKind::Cons => {
                let pair_car = key.cons_car();
                let pair_cdr = key.cons_cdr();
                if let (Some(min), Some(max)) = (pair_car.as_fixnum(), pair_cdr.as_fixnum())
                    && ch >= min
                    && ch <= max
                {
                    return Some(ct_entry_value_for_char(
                        vec[i + 1],
                        min,
                        max,
                        ch,
                        is_uniprop,
                    ));
                }
            }
            _ => {}
        }
    }
    if let Some(prefix) = optimized_prefix {
        return ct_get_char_from_sorted_prefix(vec, prefix, ch, is_uniprop);
    }
    None
}

fn ct_lookup_ascii_cached(table: &Value, ch: i64) -> Option<Value> {
    if !(0..CT_ASCII_CACHE_LEN as i64).contains(&ch) {
        return None;
    }
    if table.is_char_table() {
        let mut current = *table;
        loop {
            let obj = current.as_char_table_obj()?;
            let value = if is_sub_char_table(obj.ascii) {
                sub_char_table_contents(obj.ascii)
                    .and_then(|contents| contents.get(ch as usize).copied())
                    .unwrap_or(Value::NIL)
            } else {
                obj.ascii
            };
            if !value.is_nil() {
                return Some(value);
            }
            if !obj.defalt.is_nil() {
                return Some(obj.defalt);
            }
            if !is_char_table(&obj.parent) {
                return Some(Value::NIL);
            }
            current = obj.parent;
        }
    }
    let ch = ch as usize;
    let mut current = *table;
    loop {
        let vec_ref = current.as_vector_data()?;
        if is_char_code_property_vec(vec_ref)
            && ct_ascii_cache_magic(vec_ref) == Some(CT_ASCII_CACHE_MAGIC)
        {
            return None;
        }
        let cache_range = ct_ascii_cache_range(vec_ref)?;

        let value = vec_ref[cache_range.start + ch];
        if !value.is_nil() {
            return Some(value);
        }

        let default = vec_ref[CT_DEFAULT];
        if !default.is_nil() {
            return Some(default);
        }

        let parent = vec_ref[CT_PARENT];
        if !is_char_table(&parent) {
            return Some(Value::NIL);
        }
        current = parent;
    }
}

/// `(char-table-range CHAR-TABLE RANGE)` -- look up a value.
///
/// RANGE may be:
/// - a character -- look up that character (with parent fallback)
/// - `nil` -- return the default value
pub(crate) fn builtin_char_table_range(
    args: Vec<Value>,
    obarray: Option<&super::symbol::Obarray>,
) -> EvalResult {
    expect_args("char-table-range", &args, 2)?;
    let table = &args[0];
    let range = &args[1];

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }

    match range.kind() {
        ValueKind::Nil => {
            // Return the default value.
            if table.is_char_table() {
                return Ok(table.as_char_table_obj().unwrap().defalt);
            }
            let vec = table.as_vector_data().unwrap();
            Ok(vec[CT_DEFAULT])
        }
        ValueKind::Fixnum(_) => {
            let ch = match expect_character_code(range) {
                Ok(ch) => ch,
                Err(_) => return Err(invalid_range_error("char-table-range", obarray)),
            };
            ct_lookup(table, ch)
        }
        ValueKind::Cons => {
            let pair_car = range.cons_car();
            let pair_cdr = range.cons_cdr();
            let from = expect_character_code(&pair_car)?;
            let _to = expect_character_code(&pair_cdr)?;
            let (value, _run_from, _run_to) = ct_lookup_and_range(table, from)?;
            Ok(value)
        }
        _ => Err(invalid_range_error("char-table-range", obarray)),
    }
}

/// Recursive char-table lookup: check own entries, then default, then parent.
///
/// This matches GNU Emacs semantics:
/// 1. Look up the character in the char-table's data pairs
/// 2. If the local entry is nil or absent, use the char-table's default value
/// 3. If default is nil, recursively check the parent char-table
pub(crate) fn ct_lookup(table: &Value, ch: i64) -> EvalResult {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if let Some(value) = ct_lookup_ascii_cached(table, ch) {
        return Ok(value);
    }
    if table.is_char_table() {
        let obj = table.as_char_table_obj().unwrap();
        let idx = chartab_idx(ch, 0, 0);
        let mut val = obj.contents[idx];
        if is_sub_char_table(val) {
            val = sub_char_table_ref(val, ch, is_char_code_property_table(table));
        }
        if !val.is_nil() {
            return Ok(val);
        }
        if !obj.defalt.is_nil() {
            return Ok(obj.defalt);
        }
        if is_char_table(&obj.parent) {
            return ct_lookup(&obj.parent, ch);
        }
        return Ok(Value::NIL);
    }
    // Borrow the Vec instead of cloning — the 115K clones/sec we used to
    // do in font-lock's syntax-ppss path each allocated a ~50+-entry Vec
    // and nuked syntax-table reading throughput. GNU's `CHAR_TABLE_REF`
    // is direct array indexing; the closest we can do without reshaping
    // the table is to index without copying.
    let vec_ref = table.as_vector_data().unwrap();

    if let Some(val) = ct_get_char(vec_ref, ch, is_char_code_property_vec(vec_ref))
        && !val.is_nil()
    {
        return Ok(val);
    }

    let default = vec_ref[CT_DEFAULT];
    let parent = vec_ref[CT_PARENT];

    let value = if !default.is_nil() {
        default
    } else if is_char_table(&parent) {
        ct_lookup(&parent, ch)?
    } else {
        Value::NIL
    };
    Ok(value)
}

/// Translate character `c` through translation `table`.
///
/// Mirrors GNU `translate_char` (character.c:151). If `table` is a
/// char-table, look up `c`; if the entry is a character, that's the
/// translation. If `table` is a list, fold left through all tables.
/// Returns `c` unchanged if no translation applies.
pub(crate) fn translate_char(table: &Value, c: i64) -> i64 {
    if is_char_table(table) {
        match ct_lookup(table, c) {
            Ok(val) => match val.kind() {
                ValueKind::Fixnum(n)
                    if (0..=crate::emacs_core::emacs_char::MAX_CHAR as i64).contains(&n) =>
                {
                    n
                }
                _ => c,
            },
            Err(_) => c,
        }
    } else if table.is_cons() {
        let mut result = c;
        let mut cur = *table;
        while cur.is_cons() {
            let car = cur.cons_car();
            result = translate_char(&car, result);
            cur = cur.cons_cdr();
        }
        result
    } else {
        c
    }
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

/// The deepest PLAIN (non-sub-table) cell of `table`'s local tree containing
/// `ch`: the raw slot value (before defalt/parent resolution) and the cell's
/// INCLUSIVE character span. O(tree depth); never materializes runs.
fn ct_local_plain_cell_at(table: &Value, ch: i64) -> (Value, i64, i64) {
    let Some(obj) = table.as_char_table_obj() else {
        return (Value::NIL, 0, MAX_CHAR);
    };
    let is_uniprop = is_char_code_property_table(table);
    let idx = (ch / GNU_CHARTAB_CHARS[0]) as usize;
    let mut start = idx as i64 * GNU_CHARTAB_CHARS[0];
    let mut end = (start + GNU_CHARTAB_CHARS[0] - 1).min(MAX_CHAR);
    let Some(slot) = obj.contents.get(idx).copied() else {
        return (Value::NIL, start, end);
    };
    let mut containing = *table;
    let mut containing_idx = idx;
    let mut current = slot;
    loop {
        if is_uniprop && uniprop_compressed_string(current).is_some() {
            current = uniprop_table_uncompress(containing, containing_idx).unwrap_or(current);
        }
        let Some(sub) = current.as_sub_char_table_obj() else {
            return (current, start, end);
        };
        let depth = sub.depth as usize;
        let min_char = sub.min_char as i64;
        let span = GNU_CHARTAB_CHARS[depth];
        let sub_idx = chartab_idx(ch, depth, min_char);
        start = min_char + sub_idx as i64 * span;
        end = (start + span - 1).min(MAX_CHAR);
        let Some(next) = sub.contents.get(sub_idx).copied() else {
            return (Value::NIL, start, end);
        };
        containing = current;
        containing_idx = sub_idx;
        current = next;
    }
}

/// Effective value at `ch` (local, else defalt, else parent — same resolution
/// order as `ct_lookup`) together with the INCLUSIVE span over which this
/// resolution is uniform. A nil local cell that resolves through the parent
/// intersects the local span with the parent's, mirroring GNU
/// `char_table_ref_and_range`'s per-level from/to narrowing (chartab.c).
fn ct_effective_value_span_at(table: &Value, ch: i64) -> (Value, i64, i64) {
    let Some(obj) = table.as_char_table_obj() else {
        return (Value::NIL, 0, MAX_CHAR);
    };
    let (value, start, end) = ct_local_plain_cell_at(table, ch);
    let resolved = if value.is_nil() && !obj.defalt.is_nil() {
        obj.defalt
    } else {
        value
    };
    if !resolved.is_nil() || !obj.parent.is_char_table() {
        return (resolved, start, end);
    }
    let (parent_value, parent_start, parent_end) = ct_effective_value_span_at(&obj.parent, ch);
    (parent_value, start.max(parent_start), end.min(parent_end))
}

fn ct_lookup_and_range(table: &Value, ch: i64) -> Result<(Value, i64, i64), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if !table.is_char_table() {
        // Legacy vector representation: tiny fixed layout, the run
        // materialization is cheap and keeps the old semantics exactly.
        for run in ct_effective_runs(table) {
            if ch >= run.start && ch <= run.end {
                return Ok((run.value, run.start, run.end));
            }
        }
        return Ok((Value::NIL, 0, MAX_CHAR));
    }
    // GNU char_table_ref_and_range shape: resolve the value at ch by tree
    // descent, then extend the range outward by WHOLE plain cells while the
    // effective value stays eq (the same merge predicate the run
    // materializer used). A point query is O(depth); the extent costs one
    // descent per DISTINCT cell in the run — a uniform table extends by
    // 65536-char top cells, never by materializing every entry. The previous
    // implementation enumerated the ENTIRE table (all sub-tables plus the
    // parent chain) per query, which made subword-mode word motion — five
    // queries per word against its whole-range boundary table — unusably
    // slow.
    let (value, mut from, mut to) = ct_effective_value_span_at(table, ch);
    while from > 0 {
        let (left_value, left_start, _) = ct_effective_value_span_at(table, from - 1);
        if eq_value(&left_value, &value) {
            from = left_start;
        } else {
            break;
        }
    }
    while to < MAX_CHAR {
        let (right_value, _, right_end) = ct_effective_value_span_at(table, to + 1);
        if eq_value(&right_value, &value) {
            to = right_end;
        } else {
            break;
        }
    }
    Ok((value, from, to))
}

fn key_span(key: Value) -> Option<(i64, i64)> {
    match key.kind() {
        ValueKind::Fixnum(ch) => Some((ch, ch)),
        ValueKind::Cons => {
            let start = key.cons_car().as_fixnum()?;
            let end = key.cons_cdr().as_fixnum()?;
            Some((start, end))
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct LocalAtomicRun {
    value: Option<Value>,
    start: i64,
    end: i64,
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn refine_atomic_boundary(start: i64, end: i64, ch: i64, lo: &mut i64, hi: &mut i64) {
    let domain_end = MAX_CHAR.saturating_add(1);
    let start = start.clamp(0, domain_end);
    let end_exclusive = end.saturating_add(1).clamp(0, domain_end);
    for boundary in [start, end_exclusive] {
        if boundary <= ch {
            *lo = (*lo).max(boundary);
        } else {
            *hi = (*hi).min(boundary);
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn ct_lookup_atomic_range(table: &Value, ch: i64) -> Result<(Value, i64, i64), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        for run in ct_effective_runs(table) {
            if ch >= run.start && ch <= run.end {
                return Ok((run.value, run.start, run.end));
            }
        }
        return Ok((Value::NIL, 0, MAX_CHAR));
    }
    if !(0..=MAX_CHAR).contains(&ch) {
        return Ok((Value::NIL, 0, MAX_CHAR));
    }

    let vec = table.as_vector_data().unwrap();
    let start = ct_data_start(vec);
    let mut lo = 0;
    let mut hi = MAX_CHAR.saturating_add(1);
    let mut found_local = false;
    let mut local_value = Value::NIL;

    let mut i = vec.len();
    while i >= start + 2 {
        i -= 2;
        if let Some((entry_start, entry_end)) = key_span(vec[i]) {
            refine_atomic_boundary(entry_start, entry_end, ch, &mut lo, &mut hi);
            if !found_local && ch >= entry_start && ch <= entry_end {
                found_local = true;
                local_value = vec[i + 1];
                break;
            }
        }
    }

    let atomic_end = hi.saturating_sub(1).min(MAX_CHAR);
    if found_local && !local_value.is_nil() {
        return Ok((local_value, lo, atomic_end));
    }

    let default = vec[CT_DEFAULT];
    if !default.is_nil() {
        return Ok((default, lo, atomic_end));
    }

    let parent = vec[CT_PARENT];
    if is_char_table(&parent) {
        let (parent_value, parent_start, parent_end) = ct_lookup_atomic_range(&parent, ch)?;
        return Ok((
            parent_value,
            lo.max(parent_start),
            atomic_end.min(parent_end),
        ));
    }

    Ok((Value::NIL, lo, atomic_end))
}

fn append_atomic_run(out: &mut Vec<(Value, i64, i64)>, value: Value, start: i64, end: i64) {
    if start > end {
        return;
    }
    if let Some((last_value, _last_start, last_end)) = out.last_mut()
        && *last_end + 1 == start
        && *last_value == value
    {
        *last_end = end;
        return;
    }
    out.push((value, start, end));
}

fn ct_local_atomic_runs(
    table: &Value,
    requested_start: i64,
    requested_end: i64,
) -> (Vec<LocalAtomicRun>, Value, Value) {
    if table.is_char_table() {
        let obj = table.as_char_table_obj().unwrap();
        let runs = clipped_runs(
            ct_local_direct_runs_in_range(table, requested_start, requested_end),
            requested_start,
            requested_end,
        )
        .into_iter()
        .map(|run| LocalAtomicRun {
            value: Some(run.value),
            start: run.start,
            end: run.end,
        })
        .collect();
        return (runs, obj.defalt, obj.parent);
    }
    let vec = table.as_vector_data().unwrap();
    let default = vec[CT_DEFAULT];
    let parent = vec[CT_PARENT];
    let start = requested_start.max(0);
    let end = requested_end.min(MAX_CHAR);
    if start > end {
        return (Vec::new(), default, parent);
    }

    let end_exclusive = end.saturating_add(1);
    let data_start = ct_data_start(vec);
    let mut boundaries = vec![start, end_exclusive];
    let mut spans = Vec::new();
    let mut i = data_start;
    while i + 1 < vec.len() {
        if let Some((entry_start, entry_end)) = key_span(vec[i]) {
            let span_start = entry_start.max(start);
            let span_end = entry_end.min(end);
            if span_start <= span_end {
                let span_end_exclusive = span_end.saturating_add(1);
                boundaries.push(span_start);
                boundaries.push(span_end_exclusive);
                spans.push((span_start, span_end_exclusive, vec[i + 1]));
            }
        }
        i += 2;
    }

    boundaries.sort_unstable();
    boundaries.dedup();

    let mut values = vec![None; boundaries.len().saturating_sub(1)];
    for (span_start, span_end_exclusive, value) in spans {
        let Ok(first) = boundaries.binary_search(&span_start) else {
            continue;
        };
        let Ok(last) = boundaries.binary_search(&span_end_exclusive) else {
            continue;
        };
        for slot in values.iter_mut().take(last).skip(first) {
            *slot = Some(value);
        }
    }

    let runs = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| LocalAtomicRun {
            value,
            start: boundaries[index],
            end: boundaries[index + 1].saturating_sub(1),
        })
        .collect();
    (runs, default, parent)
}

/// GNU `char-table-ref-and-range`-style helper used by subsystems that need
/// the effective value together with the maximal contiguous run covering `ch`.
pub(crate) fn char_table_ref_and_range(table: &Value, ch: i64) -> Result<(Value, i64, i64), Flow> {
    ct_lookup_and_range(table, ch)
}

/// Return the effective value and a contiguous range around `ch` where no local
/// char-table assignment boundary occurs.
///
/// This range may be smaller than GNU's maximal `char-table-ref-and-range`
/// result, but every character in it has the same effective value.  It is for
/// bulk mutators that only need a correct split point and would otherwise pay to
/// rebuild the full effective run list for each cursor step.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn char_table_ref_and_atomic_range(
    table: &Value,
    ch: i64,
) -> Result<(Value, i64, i64), Flow> {
    ct_lookup_atomic_range(table, ch)
}

/// Return effective atomic runs for a caller-supplied range.
///
/// This has the same local-entry shadowing and default/parent fallback rules as
/// `char_table_ref_and_atomic_range`, but scans the table entries once for the
/// whole requested range.  Bulk mutators can then update each returned run
/// without repeating a reverse lookup from every cursor position.
pub(crate) fn char_table_atomic_runs_in_range(
    table: &Value,
    start: i64,
    end: i64,
) -> Result<Vec<(Value, i64, i64)>, Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    let start = start.max(0);
    let end = end.min(MAX_CHAR);
    if start > end {
        return Ok(Vec::new());
    }

    let (local_runs, default, parent) = ct_local_atomic_runs(table, start, end);
    let mut out = Vec::with_capacity(local_runs.len());
    for run in local_runs {
        if let Some(value) = run.value
            && !value.is_nil()
        {
            append_atomic_run(&mut out, value, run.start, run.end);
            continue;
        }
        if !default.is_nil() {
            append_atomic_run(&mut out, default, run.start, run.end);
        } else if is_char_table(&parent) {
            for (value, child_start, child_end) in
                char_table_atomic_runs_in_range(&parent, run.start, run.end)?
            {
                append_atomic_run(&mut out, value, child_start, child_end);
            }
        } else {
            append_atomic_run(&mut out, Value::NIL, run.start, run.end);
        }
    }
    Ok(out)
}

/// `(char-table-parent CHAR-TABLE)` -- return the parent table (or nil).
pub(crate) fn builtin_char_table_parent(args: Vec<Value>) -> EvalResult {
    expect_args("char-table-parent", &args, 1)?;
    let table = &args[0];
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        Ok(table.as_char_table_obj().unwrap().parent)
    } else {
        let vec = table.as_vector_data().unwrap();
        Ok(vec[CT_PARENT])
    }
}

/// Return the sparse local `(key . value)` entries stored directly in a char-table.
///
/// Keys are either character codes (fixnums) or range conses `(FROM . TO)`.
/// Parent/default fallback is intentionally not applied here; callers that need
/// effective values should use `ct_lookup`.
pub(crate) fn char_table_local_entries(table: &Value) -> Result<Vec<(Value, Value)>, Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        return Ok(ct_collect_raw_entries_for_table(*table, false)
            .into_iter()
            .map(|run| (run_key(run.start, run.end), run.value))
            .collect());
    }
    let vec = table.as_vector_data().unwrap();
    let start = ct_data_start(vec);
    let mut out = Vec::new();
    let mut i = start;
    while i + 1 < vec.len() {
        match vec[i].kind() {
            ValueKind::Fixnum(_) | ValueKind::Cons => out.push((vec[i], vec[i + 1])),
            _ => {}
        }
        i += 2;
    }
    Ok(out)
}

/// `(set-char-table-parent CHAR-TABLE PARENT)` -- set the parent table.
pub(crate) fn builtin_set_char_table_parent(args: Vec<Value>) -> EvalResult {
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
            cursor = if cursor.is_char_table() {
                cursor.as_char_table_obj().unwrap().parent
            } else {
                let vec = cursor.as_vector_data().unwrap();
                vec[CT_PARENT]
            };
        }
    }

    if table.is_char_table() {
        let _ = table.with_char_table_mut(|obj| obj.parent = *parent);
    } else {
        let _ = table.set_vector_slot(CT_PARENT, *parent);
    }
    crate::window::note_char_table_layout_mutation();
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
pub(crate) fn for_each_char_table_mapping(
    table: &Value,
    mut f: impl FnMut(Value, Value) -> Result<(), Flow>,
) -> Result<(), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }

    let shared_range = Value::cons(Value::fixnum(0), Value::fixnum(MAX_CHAR));
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(shared_range);
    let result = (|| {
        let mut last_emitted_end = None;
        for run in ct_map_char_table_runs(table) {
            shared_range.set_car(Value::fixnum(run.start));
            shared_range.set_cdr(Value::fixnum(run.end));
            if run.value.is_nil() {
                continue;
            }
            let key = if run.start == run.end {
                Value::fixnum(run.start)
            } else {
                shared_range
            };
            let value = decode_unicode_property_map_value(*table, run.value);
            f(key, value)?;
            last_emitted_end = Some(run.end);
        }

        // GNU map_char_table keeps mutating the same range cons while it
        // walks the final nil span after the last non-nil value.  Lisp code
        // that retained a range key observes that final mutation.
        if let Some(end) = last_emitted_end
            && end < MAX_CHAR
        {
            shared_range.set_car(Value::fixnum(end + 1));
            shared_range.set_cdr(Value::fixnum(MAX_CHAR));
        }
        Ok(())
    })();
    restore_scratch_gc_roots(saved);
    result
}

pub(crate) fn builtin_map_char_table(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("map-char-table", &args, 2)?;
    let func = args[0];
    let table = args[1];
    for_each_char_table_mapping(&table, |key, value| {
        let _ = eval.apply(func, vec![key, value])?;
        Ok(())
    })?;
    Ok(Value::NIL)
}

/// Resolve a char-table into non-overlapping effective runs, including nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn ct_resolved_entries(table: &Value) -> Vec<(Value, Value)> {
    ct_effective_runs(table)
        .into_iter()
        .filter(|run| !run.value.is_nil())
        .map(|run| (run_key(run.start, run.end), run.value))
        .collect()
}

fn ct_local_direct_runs(table: &Value) -> Vec<EffectiveRun> {
    ct_local_direct_runs_in_range(table, 0, MAX_CHAR)
}

/// Ranged variant of ct_local_direct_runs: for real char-tables only the
/// slots intersecting [win_start, win_end] are enumerated (the returned runs
/// may extend past the window at both edges; callers clip). Legacy vector
/// tables fall back to the full enumeration — they are 128-slot structures
/// where pruning buys nothing.
fn ct_local_direct_runs_in_range(table: &Value, win_start: i64, win_end: i64) -> Vec<EffectiveRun> {
    if table.is_char_table() {
        let obj = table.as_char_table_obj().unwrap();
        let raws = ct_collect_raw_entries_for_table_in_range(*table, true, win_start, win_end);
        let mut runs = Vec::new();
        for raw in raws {
            let value = if raw.value.is_nil() && !obj.defalt.is_nil() {
                obj.defalt
            } else {
                raw.value
            };
            push_effective_run(&mut runs, raw.start, raw.end, value);
        }
        return if runs.is_empty() {
            vec![EffectiveRun {
                start: 0,
                end: MAX_CHAR,
                value: Value::NIL,
            }]
        } else {
            runs
        };
    }
    if !table.is_vector() {
        return vec![EffectiveRun {
            start: 0,
            end: MAX_CHAR,
            value: Value::NIL,
        }];
    }
    let vec = table.as_vector_data().unwrap().clone();
    let raws = ct_collect_raw_entries(&vec, is_char_code_property_vec(&vec));
    let default = vec[CT_DEFAULT];
    let domain_end = MAX_CHAR.saturating_add(1);

    let mut boundaries = BTreeSet::new();
    let mut starts: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    let mut ends: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    boundaries.insert(0);
    boundaries.insert(domain_end);
    for (idx, raw) in raws.iter().enumerate() {
        let start = raw.start.clamp(0, domain_end);
        let end_exclusive = raw.end.saturating_add(1).clamp(0, domain_end);
        boundaries.insert(start);
        boundaries.insert(end_exclusive);
        starts.entry(start).or_default().push(idx);
        ends.entry(end_exclusive).or_default().push(idx);
    }

    let mut runs = Vec::new();
    let mut active_raws = BTreeSet::new();
    for window in boundaries.into_iter().collect::<Vec<_>>().windows(2) {
        let start = window[0];
        let end_exclusive = window[1];
        if let Some(indices) = ends.get(&start) {
            for idx in indices {
                active_raws.remove(idx);
            }
        }
        if let Some(indices) = starts.get(&start) {
            for idx in indices {
                active_raws.insert(*idx);
            }
        }
        if start > MAX_CHAR || end_exclusive <= start {
            continue;
        }
        let end = end_exclusive.saturating_sub(1).min(MAX_CHAR);
        let local = active_raws.iter().next_back().map(|idx| raws[*idx].value);
        let value = match local {
            Some(local) if !local.is_nil() => local,
            _ if !default.is_nil() => default,
            _ => Value::NIL,
        };
        push_effective_run(&mut runs, start, end, value);
    }

    if runs.is_empty() {
        vec![EffectiveRun {
            start: 0,
            end: MAX_CHAR,
            value: Value::NIL,
        }]
    } else {
        runs
    }
}

fn push_effective_run(runs: &mut Vec<EffectiveRun>, start: i64, end: i64, value: Value) {
    if start > end {
        return;
    }
    if let Some(previous) = runs.last_mut()
        && previous.end.saturating_add(1) == start
        && eq_value(&previous.value, &value)
    {
        previous.end = end;
        return;
    }
    runs.push(EffectiveRun { start, end, value });
}

fn ct_ascii_initial_value(vec: &[Value]) -> Value {
    ct_ascii_cache_range(vec)
        .and_then(|range| vec.get(range.start).copied())
        .unwrap_or(Value::NIL)
}

fn clipped_runs(
    runs: impl IntoIterator<Item = EffectiveRun>,
    from: i64,
    to: i64,
) -> Vec<EffectiveRun> {
    if from > to {
        return Vec::new();
    }
    let mut clipped = Vec::new();
    for run in runs {
        let start = run.start.max(from);
        let end = run.end.min(to);
        if start <= end {
            push_effective_run(&mut clipped, start, end, run.value);
        }
    }
    clipped
}

fn push_clipped_parent_runs_from_slice(
    out: &mut Vec<EffectiveRun>,
    parent_runs: &[EffectiveRun],
    parent_idx: &mut usize,
    from: i64,
    to: i64,
) {
    if from > to {
        return;
    }
    while *parent_idx < parent_runs.len() && parent_runs[*parent_idx].end < from {
        *parent_idx += 1;
    }

    let mut idx = *parent_idx;
    while let Some(run) = parent_runs.get(idx).copied() {
        if run.start > to {
            break;
        }
        let start = run.start.max(from);
        let end = run.end.min(to);
        if start <= end {
            push_effective_run(out, start, end, run.value);
        }
        if run.end >= to {
            break;
        }
        idx += 1;
    }
}

fn push_parent_direct_span_runs(
    out: &mut Vec<EffectiveRun>,
    parent: Value,
    from: i64,
    to: i64,
    next_local_value: Value,
) -> bool {
    let parent_runs = clipped_runs(ct_local_direct_runs_in_range(&parent, from, to), from, to);
    let Some(last) = parent_runs.last().copied() else {
        return true;
    };
    let suppress_last = eq_value(&last.value, &next_local_value);
    let emit_len = parent_runs.len() - usize::from(suppress_last);
    for run in parent_runs.into_iter().take(emit_len) {
        if !run.value.is_nil() {
            push_effective_run(out, run.start, run.end, run.value);
        }
    }
    !suppress_last
}

fn push_parent_tail_runs(out: &mut Vec<EffectiveRun>, parent: Value, from: i64, to: i64) {
    for run in clipped_runs(ct_map_char_table_runs(&parent), from, to) {
        if !run.value.is_nil() {
            push_effective_run(out, run.start, run.end, run.value);
        }
    }
}

fn ct_map_char_table_runs(table: &Value) -> Vec<EffectiveRun> {
    if !is_char_table(table) {
        return Vec::new();
    }
    let parent = if table.is_char_table() {
        table.as_char_table_obj().unwrap().parent
    } else {
        table.as_vector_data().unwrap()[CT_PARENT]
    };
    let local_runs = ct_local_direct_runs(table);
    let mut out = Vec::new();
    let mut val = if table.is_char_table() {
        table.as_char_table_obj().unwrap().ascii
    } else {
        ct_ascii_initial_value(table.as_vector_data().unwrap())
    };
    let mut from = 0;

    for run in local_runs {
        if eq_value(&val, &run.value) {
            continue;
        }

        let mut different_value = true;
        let has_previous_span = from < run.start;
        if val.is_nil() && is_char_table(&parent) && has_previous_span {
            different_value =
                push_parent_direct_span_runs(&mut out, parent, from, run.start - 1, run.value);
        }
        if !val.is_nil() && different_value && has_previous_span {
            push_effective_run(&mut out, from, run.start - 1, val);
        }
        val = run.value;
        from = run.start;
    }

    if val.is_nil() {
        if is_char_table(&parent) {
            push_parent_tail_runs(&mut out, parent, from, MAX_CHAR);
        }
    } else {
        push_effective_run(&mut out, from, MAX_CHAR, val);
    }

    out
}

#[derive(Clone, Copy)]
struct RawEntry {
    start: i64,
    end: i64,
    value: Value,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EffectiveRun {
    start: i64,
    end: i64,
    value: Value,
}

#[derive(Clone, Copy)]
pub(crate) enum OptimizeCharTableTest {
    Equal,
    Eq,
}

fn optimize_values_equal(a: Value, b: Value, test: OptimizeCharTableTest) -> bool {
    match test {
        OptimizeCharTableTest::Equal => equal_value(&a, &b, 100),
        OptimizeCharTableTest::Eq => eq_value(&a, &b),
    }
}

fn ct_collect_raw_entries(vec: &[Value], is_uniprop: bool) -> Vec<RawEntry> {
    let start = ct_data_start(vec);
    let mut raws = Vec::new();
    let mut i = start;
    while i + 1 < vec.len() {
        match vec[i].kind() {
            ValueKind::Fixnum(ch) => raws.push(RawEntry {
                start: ch,
                end: ch,
                value: vec[i + 1],
            }),
            ValueKind::Cons => {
                let pair_car = vec[i].cons_car();
                let pair_cdr = vec[i].cons_cdr();
                if let (Some(min), Some(max)) = (pair_car.as_fixnum(), pair_cdr.as_fixnum()) {
                    if is_uniprop
                        && let Some(mut decoded) = uniprop_compressed_runs(vec[i + 1], min, max)
                    {
                        raws.append(&mut decoded);
                    } else {
                        raws.push(RawEntry {
                            start: min,
                            end: max,
                            value: vec[i + 1],
                        });
                    }
                }
            }
            _ => {}
        }
        i += 2;
    }
    raws
}

/// Ranged sub-char-table walk: descends only into slots whose
/// character span intersects [win_start, win_end]. Mirrors GNU
/// map_sub_char_table's from/to pruning (chartab.c) — a narrow query must not
/// enumerate the whole table. Entries straddling the window edges are emitted
/// whole (callers clip), preserving ascending order.
fn collect_sub_char_table_raw_entries_in_range(
    out: &mut Vec<RawEntry>,
    table: Value,
    is_uniprop: bool,
    win_start: i64,
    win_end: i64,
) {
    let Some(obj) = table.as_sub_char_table_obj() else {
        return;
    };
    let depth = obj.depth as usize;
    let min_char = obj.min_char as i64;
    let span = GNU_CHARTAB_CHARS[depth];
    for (idx, mut value) in obj.contents.iter().copied().enumerate() {
        let start = min_char + idx as i64 * span;
        let end = (start + span - 1).min(MAX_CHAR);
        if end < win_start || start > win_end {
            continue;
        }
        if is_uniprop
            && uniprop_compressed_string(value).is_some()
            && let Some(child) = uniprop_table_uncompress(table, idx)
        {
            value = child;
        }
        if is_sub_char_table(value) {
            collect_sub_char_table_raw_entries_in_range(out, value, is_uniprop, win_start, win_end);
        } else {
            out.push(RawEntry { start, end, value });
        }
    }
}

fn ct_collect_raw_entries_for_table(table: Value, include_nil: bool) -> Vec<RawEntry> {
    ct_collect_raw_entries_for_table_in_range(table, include_nil, 0, MAX_CHAR)
}

/// Ranged variant of the top-level raw-entry collection; see
/// collect_sub_char_table_raw_entries_in_range for the pruning contract.
fn ct_collect_raw_entries_for_table_in_range(
    table: Value,
    include_nil: bool,
    win_start: i64,
    win_end: i64,
) -> Vec<RawEntry> {
    let Some(obj) = table.as_char_table_obj() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let is_uniprop = is_char_code_property_table(&table);
    for (idx, value) in obj.contents.iter().copied().enumerate() {
        let start = idx as i64 * GNU_CHARTAB_CHARS[0];
        let end = (start + GNU_CHARTAB_CHARS[0] - 1).min(MAX_CHAR);
        if end < win_start || start > win_end {
            continue;
        }
        if is_sub_char_table(value) {
            collect_sub_char_table_raw_entries_in_range(
                &mut out, value, is_uniprop, win_start, win_end,
            );
        } else if include_nil || !value.is_nil() {
            out.push(RawEntry { start, end, value });
        }
    }
    out
}

fn ct_collect_local_raw_entries(vec: &[Value]) -> Vec<RawEntry> {
    ct_collect_raw_entries(vec, false)
}

fn append_optimized_raw_run(
    runs: &mut Vec<RawEntry>,
    start: i64,
    end: i64,
    value: Value,
    test: OptimizeCharTableTest,
) {
    if start > end || value.is_nil() {
        return;
    }
    if let Some(previous) = runs.last_mut()
        && previous.end.saturating_add(1) == start
        && optimize_values_equal(previous.value, value, test)
    {
        previous.end = end;
        return;
    }
    runs.push(RawEntry { start, end, value });
}

fn ct_optimized_local_runs(vec: &[Value], test: OptimizeCharTableTest) -> Vec<RawEntry> {
    let raws = ct_collect_raw_entries(vec, is_char_code_property_vec(vec));
    if raws.is_empty() {
        return Vec::new();
    }

    let domain_end = MAX_CHAR.saturating_add(1);
    let mut boundaries = BTreeSet::new();
    let mut starts: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    let mut ends: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    boundaries.insert(0);
    boundaries.insert(domain_end);
    for (idx, raw) in raws.iter().enumerate() {
        let start = raw.start.clamp(0, domain_end);
        let end_exclusive = raw.end.saturating_add(1).clamp(0, domain_end);
        if start >= end_exclusive {
            continue;
        }
        boundaries.insert(start);
        boundaries.insert(end_exclusive);
        starts.entry(start).or_default().push(idx);
        ends.entry(end_exclusive).or_default().push(idx);
    }

    let mut runs = Vec::new();
    let mut active_raws = BTreeSet::new();
    let boundary_vec = boundaries.into_iter().collect::<Vec<_>>();
    for window in boundary_vec.windows(2) {
        let start = window[0];
        let end_exclusive = window[1];
        if let Some(indices) = ends.get(&start) {
            for idx in indices {
                active_raws.remove(idx);
            }
        }
        if let Some(indices) = starts.get(&start) {
            for idx in indices {
                active_raws.insert(*idx);
            }
        }
        if start > MAX_CHAR || end_exclusive <= start {
            continue;
        }
        let Some(local_idx) = active_raws.iter().next_back().copied() else {
            continue;
        };
        append_optimized_raw_run(
            &mut runs,
            start,
            end_exclusive.saturating_sub(1).min(MAX_CHAR),
            raws[local_idx].value,
            test,
        );
    }
    runs
}

fn ct_clear_ascii_cache(vec: &mut [Value]) {
    if let Some(range) = ct_ascii_cache_range(vec) {
        for slot in range {
            vec[slot] = Value::NIL;
        }
    }
}

fn ct_replace_local_entries_with_runs(vec: &mut Vec<Value>, runs: Vec<RawEntry>) {
    let data_start = ct_data_start(vec);
    let pair_count = runs.len();
    vec.truncate(data_start);
    ct_clear_ascii_cache(vec);
    if pair_count > 0 {
        vec.push(Value::symbol(CT_OPTIMIZED_PREFIX_MARKER));
        vec.push(Value::fixnum(pair_count as i64));
    }
    for run in runs {
        if run.start == run.end {
            ct_set_char(vec, run.start, run.value);
        } else {
            ct_set_range(vec, run.start, run.end, run.value);
        }
    }
}

pub(crate) fn optimize_char_table(table: &Value, test: OptimizeCharTableTest) -> Result<(), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        for idx in 0..GNU_CHARTAB_SIZE[0] {
            let value = table.as_char_table_obj().unwrap().contents[idx];
            if is_sub_char_table(value) {
                let optimized = optimize_sub_char_table_direct(value, test);
                set_char_table_contents(*table, idx, optimized);
            }
        }
        set_char_table_ascii(*table, char_table_ascii(*table));
        return Ok(());
    }
    table.with_vector_data_mut(|vec| {
        let runs = ct_optimized_local_runs(vec, test);
        ct_replace_local_entries_with_runs(vec, runs);
    });
    Ok(())
}

fn optimize_sub_char_table_direct(table: Value, test: OptimizeCharTableTest) -> Value {
    let Some(obj) = table.as_sub_char_table_obj() else {
        return table;
    };
    let depth = obj.depth as usize;
    let Some(mut first) = obj.contents.first().copied() else {
        return table;
    };
    if is_sub_char_table(first) {
        first = optimize_sub_char_table_direct(first, test);
        sub_char_table_set_slot(table, 0, first);
    }
    let mut optimizable = !is_sub_char_table(first);
    for idx in 1..GNU_CHARTAB_SIZE[depth] {
        let mut current = table
            .as_sub_char_table_obj()
            .and_then(|obj| obj.contents.get(idx).copied())
            .unwrap_or(Value::NIL);
        if is_sub_char_table(current) {
            current = optimize_sub_char_table_direct(current, test);
            sub_char_table_set_slot(table, idx, current);
        }
        if optimizable && !optimize_values_equal(current, first, test) {
            optimizable = false;
        }
    }
    if optimizable { first } else { table }
}

fn ct_effective_runs(table: &Value) -> Vec<EffectiveRun> {
    if table.is_char_table() {
        let obj = table.as_char_table_obj().unwrap();
        let local_runs = ct_local_direct_runs(table);
        if !is_char_table(&obj.parent) {
            return local_runs;
        }
        let parent_runs = ct_effective_runs(&obj.parent);
        let mut out = Vec::new();
        let mut parent_idx = 0usize;
        for run in local_runs {
            if !run.value.is_nil() {
                push_effective_run(&mut out, run.start, run.end, run.value);
                continue;
            }
            push_clipped_parent_runs_from_slice(
                &mut out,
                &parent_runs,
                &mut parent_idx,
                run.start,
                run.end,
            );
        }
        return out;
    }
    if !table.is_vector() {
        return vec![EffectiveRun {
            start: 0,
            end: MAX_CHAR,
            value: Value::NIL,
        }];
    };
    let vec = table.as_vector_data().unwrap();
    let raws = ct_collect_raw_entries(vec, is_char_code_property_vec(vec));
    let default = vec[CT_DEFAULT];
    let parent = vec[CT_PARENT];
    let domain_end = MAX_CHAR.saturating_add(1);
    let parent_runs = if is_char_table(&parent) {
        ct_effective_runs(&parent)
    } else {
        vec![EffectiveRun {
            start: 0,
            end: MAX_CHAR,
            value: Value::NIL,
        }]
    };

    let mut boundaries = BTreeSet::new();
    let mut starts: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    let mut ends: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    boundaries.insert(0);
    boundaries.insert(domain_end);
    for (idx, raw) in raws.iter().enumerate() {
        let end_exclusive = raw.end.saturating_add(1).min(domain_end);
        boundaries.insert(raw.start);
        boundaries.insert(end_exclusive);
        starts.entry(raw.start).or_default().push(idx);
        ends.entry(end_exclusive).or_default().push(idx);
    }
    for run in &parent_runs {
        boundaries.insert(run.start);
        boundaries.insert(run.end.saturating_add(1).min(domain_end));
    }

    let boundary_vec = boundaries.into_iter().collect::<Vec<_>>();
    let mut runs: Vec<EffectiveRun> = Vec::new();
    let mut active_raws = BTreeSet::new();
    let mut parent_idx = 0usize;

    for window in boundary_vec.windows(2) {
        let start = window[0];
        let end_exclusive = window[1];
        if let Some(indices) = ends.get(&start) {
            for idx in indices {
                active_raws.remove(idx);
            }
        }
        if let Some(indices) = starts.get(&start) {
            for idx in indices {
                active_raws.insert(*idx);
            }
        }
        if start > MAX_CHAR || end_exclusive <= start {
            continue;
        }
        let end = end_exclusive.saturating_sub(1).min(MAX_CHAR);
        while parent_idx + 1 < parent_runs.len() && start > parent_runs[parent_idx].end {
            parent_idx += 1;
        }
        let local = active_raws.iter().next_back().map(|idx| raws[*idx].value);
        let value = match local {
            Some(local) if !local.is_nil() => local,
            _ if !default.is_nil() => default,
            _ => parent_runs
                .get(parent_idx)
                .filter(|run| start >= run.start && start <= run.end)
                .map(|run| run.value)
                .unwrap_or(Value::NIL),
        };
        if let Some(previous) = runs.last_mut()
            && previous.end.saturating_add(1) == start
            && eq_value(&previous.value, &value)
        {
            previous.end = end;
        } else {
            runs.push(EffectiveRun { start, end, value });
        }
    }

    if runs.is_empty() {
        vec![EffectiveRun {
            start: 0,
            end: MAX_CHAR,
            value: Value::NIL,
        }]
    } else {
        runs
    }
}

fn run_key(start: i64, end: i64) -> Value {
    if start == end {
        Value::fixnum(start)
    } else {
        Value::cons(Value::fixnum(start), Value::fixnum(end))
    }
}

pub(crate) fn for_each_non_nil_char_table_run<F>(table: &Value, mut f: F)
where
    F: FnMut(Value, Value),
{
    if !is_char_table(table) {
        return;
    }

    for run in ct_effective_runs(table) {
        if run.value.is_nil() {
            continue;
        }
        f(run_key(run.start, run.end), run.value);
    }
}

const GNU_CHAR_TABLE_CONTENT_BLOCKS: i64 = 64;
const GNU_CHAR_TABLE_BLOCK_CHARS: i64 = 1 << 16;

fn raw_entry_overlaps(raw: &RawEntry, start: i64, end: i64) -> bool {
    raw.start <= end && raw.end >= start
}

fn local_raw_value_at(raws: &[RawEntry], ch: i64) -> Value {
    raws.iter()
        .rev()
        .find(|raw| ch >= raw.start && ch <= raw.end)
        .map(|raw| raw.value)
        .unwrap_or(Value::NIL)
}

fn local_uniform_value(raws: &[RawEntry], start: i64, end: i64) -> Option<Value> {
    if start > end {
        return Some(Value::NIL);
    }
    if !raws.iter().any(|raw| raw_entry_overlaps(raw, start, end)) {
        return Some(Value::NIL);
    }

    let mut boundaries = BTreeSet::new();
    let domain_end = MAX_CHAR.saturating_add(1);
    boundaries.insert(start.clamp(0, domain_end));
    boundaries.insert(end.saturating_add(1).clamp(0, domain_end));
    for raw in raws
        .iter()
        .filter(|raw| raw_entry_overlaps(raw, start, end))
    {
        boundaries.insert(raw.start.max(start).clamp(0, domain_end));
        boundaries.insert(raw.end.saturating_add(1).min(end.saturating_add(1)));
    }

    let mut value = None;
    for window in boundaries.into_iter().collect::<Vec<_>>().windows(2) {
        let segment_start = window[0];
        if segment_start > end || window[1] <= segment_start {
            continue;
        }
        let segment_value = local_raw_value_at(raws, segment_start);
        match value {
            Some(previous) if !eq_value(&previous, &segment_value) => return None,
            Some(_) => {}
            None => value = Some(segment_value),
        }
    }
    value.or(Some(Value::NIL))
}

fn make_sub_char_table_literal(depth: usize, min_char: i64, contents: Vec<Value>) -> Value {
    let mut values = Vec::with_capacity(contents.len() + 3);
    values.push(Value::symbol(SUB_CHAR_TABLE_TAG));
    values.push(Value::fixnum(depth as i64));
    values.push(Value::fixnum(min_char));
    values.extend(contents);
    Value::vector(values)
}

fn external_subtree_for_span(
    raws: &[RawEntry],
    depth: usize,
    min_char: i64,
    start: i64,
    end: i64,
) -> Value {
    if let Some(value) = local_uniform_value(raws, start, end) {
        return value;
    }

    let child_span = GNU_CHARTAB_CHARS[depth];
    let mut contents = Vec::with_capacity(GNU_CHARTAB_SIZE[depth]);
    for idx in 0..GNU_CHARTAB_SIZE[depth] {
        let child_start = min_char + idx as i64 * child_span;
        let child_end = (child_start + child_span - 1).min(MAX_CHAR);
        let child = if depth == 3 {
            local_uniform_value(raws, child_start, child_end).unwrap_or(Value::NIL)
        } else {
            external_subtree_for_span(raws, depth + 1, child_start, child_start, child_end)
        };
        contents.push(child);
    }
    make_sub_char_table_literal(depth, min_char, contents)
}

fn external_ascii_slot(raws: &[RawEntry]) -> Value {
    external_subtree_for_span(raws, 3, 0, 0, 127)
}

fn external_ascii_slot_from_cache(vec: &[Value], raws: &[RawEntry]) -> Value {
    let Some(range) = ct_ascii_cache_range(vec) else {
        return external_ascii_slot(raws);
    };
    let values = vec[range].to_vec();
    if let Some(first) = values.first().copied()
        && values.iter().all(|value| eq_value(value, &first))
    {
        return first;
    }
    make_sub_char_table_literal(3, 0, values)
}

pub(crate) fn sub_char_table_external_slots(table: &Value) -> Option<(i64, i64, Vec<Value>)> {
    if table.is_sub_char_table() {
        let obj = table.as_sub_char_table_obj()?;
        return Some((
            obj.depth as i64,
            obj.min_char as i64,
            obj.contents.as_slice().to_vec(),
        ));
    }
    let (depth, min_char, contents) = sub_char_table_depth_min_contents(table)?;
    Some((depth as i64, min_char, contents))
}

pub(crate) fn char_table_external_slots(table: &Value) -> Option<Vec<Value>> {
    if !is_char_table(table) {
        return None;
    }

    if table.is_char_table() {
        return table.char_table_external_slots();
    }
    if !table.is_vector() {
        return None;
    };
    let vec = table.as_vector_data().unwrap().clone();
    let raws = ct_collect_local_raw_entries(&vec);
    let extra_count = match vec[CT_EXTRA_COUNT].kind() {
        ValueKind::Fixnum(n) if n >= 0 => n as usize,
        _ => 0,
    };

    let mut slots = Vec::with_capacity(4 + GNU_CHAR_TABLE_CONTENT_BLOCKS as usize + extra_count);
    slots.push(vec[CT_DEFAULT]);
    slots.push(vec[CT_PARENT]);
    slots.push(vec[CT_SUBTYPE]);
    slots.push(external_ascii_slot_from_cache(&vec, &raws));

    for idx in 0..GNU_CHAR_TABLE_CONTENT_BLOCKS {
        let start = idx * GNU_CHAR_TABLE_BLOCK_CHARS;
        let end = (start + GNU_CHAR_TABLE_BLOCK_CHARS - 1).min(MAX_CHAR);
        slots.push(external_subtree_for_span(&raws, 1, start, start, end));
    }

    for extra_idx in 0..extra_count {
        slots.push(vec[CT_EXTRA_START + extra_idx]);
    }

    Some(slots)
}

/// `(char-table-extra-slot TABLE N)` -- get extra slot N (0-based).
pub(crate) fn builtin_char_table_extra_slot(args: Vec<Value>) -> EvalResult {
    expect_args("char-table-extra-slot", &args, 2)?;
    let table = &args[0];
    let n = expect_fixnump(&args[1])?;

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        let obj = table.as_char_table_obj().unwrap();
        if n < 0 || n as usize >= obj.extras.len() {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![args[0], args[1]],
            ));
        }
        return Ok(obj.extras[n as usize]);
    }
    let v = table.as_vector_data().unwrap();
    let extra_count = match v[CT_EXTRA_COUNT].kind() {
        ValueKind::Fixnum(c) => c,
        _ => 0,
    };

    if n < 0 || n >= extra_count {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], args[1]],
        ));
    }

    Ok(v[CT_EXTRA_START + n as usize])
}

/// `(set-char-table-extra-slot TABLE N VALUE)` -- set extra slot N.
pub(crate) fn builtin_set_char_table_extra_slot(args: Vec<Value>) -> EvalResult {
    expect_args("set-char-table-extra-slot", &args, 3)?;
    let table = &args[0];
    let n = expect_fixnump(&args[1])?;
    let value = &args[2];

    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        let extra_len = table.as_char_table_obj().unwrap().extras.len();
        if n < 0 || n as usize >= extra_len {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![args[0], args[1]],
            ));
        }
        let _ = table.with_char_table_mut(|obj| obj.extras.ensure_owned()[n as usize] = *value);
        crate::window::note_char_table_layout_mutation();
        return Ok(*value);
    }
    let v = table.as_vector_data().unwrap();
    let extra_count = match v[CT_EXTRA_COUNT].kind() {
        ValueKind::Fixnum(c) => c,
        _ => 0,
    };

    if n < 0 || n >= extra_count {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], args[1]],
        ));
    }

    let slot_idx = CT_EXTRA_START + n as usize;
    table.with_vector_data_mut(|vec| {
        store_value_atomic(&mut vec[slot_idx], *value);
        maybe_optimize_completed_translation_table(vec, n);
    });
    crate::window::note_char_table_layout_mutation();
    Ok(*value)
}

/// `(char-table-subtype TABLE)` -- return the sub-type symbol.
/// `char-table-p` + subtype check against a symbol name, without the
/// builtin arg plumbing (two Vec allocs) or the purpose symbol's
/// name-string parse the old callers paid per call.
pub(crate) fn char_table_has_subtype_named(value: &Value, name: &str) -> bool {
    if !is_char_table(value) {
        return false;
    }
    let purpose = if value.is_char_table() {
        match value.as_char_table_obj() {
            Some(table) => table.purpose,
            None => return false,
        }
    } else {
        match value.as_vector_data() {
            Some(vec) => vec[CT_SUBTYPE],
            None => return false,
        }
    };
    purpose.is_symbol_named(name)
}

pub(crate) fn builtin_char_table_subtype(args: Vec<Value>) -> EvalResult {
    expect_args("char-table-subtype", &args, 1)?;
    let table = &args[0];
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if table.is_char_table() {
        Ok(table.as_char_table_obj().unwrap().purpose)
    } else {
        let vec = table.as_vector_data().unwrap();
        Ok(vec[CT_SUBTYPE])
    }
}

fn assq_cell_eq(key: Value, list: Value) -> Result<Value, Flow> {
    let mut cursor = list;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return Ok(Value::NIL),
            ValueKind::Cons => {
                let entry = cursor.cons_car();
                if entry.is_cons() && eq_value(&entry.cons_car(), &key) {
                    return Ok(entry);
                }
                cursor = cursor.cons_cdr();
            }
            _ => return Err(wrong_type("listp", &list)),
        }
    }
}

fn char_code_property_cell(eval: &Context, prop: Value) -> Result<Value, Flow> {
    let alist = eval
        .obarray
        .symbol_value("char-code-property-alist")
        .copied()
        .unwrap_or(Value::NIL);
    assq_cell_eq(prop, alist)
}

/// `(unicode-property-table-internal PROP)`.
///
/// GNU's `chartab.c:uniprop_table` lazily loads `international/<file>` when
/// `char-code-property-alist` stores a string, then the public primitive returns
/// the alist cdr even for property tables whose decoder is Lisp rather than the
/// C fast-path decoder.  That distinction matters for `name`/`old-name`, whose
/// generated tables use byte-code decoder functions in extra slots.
pub(crate) fn builtin_unicode_property_table_internal(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("unicode-property-table-internal", &args, 1)?;
    let prop = args[0];
    let mut cell = char_code_property_cell(eval, prop)?;
    if cell.is_nil() {
        return Ok(Value::NIL);
    }

    let table = cell.cons_cdr();
    if table.is_string() {
        let Some(file_name) = table
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        else {
            return Ok(table);
        };
        let load_name = Value::string(format!("international/{file_name}"));
        let _ = crate::emacs_core::load::builtin_load_in_vm_runtime(
            eval,
            &[load_name, Value::T, Value::T, Value::T, Value::T],
        )?;
        cell = char_code_property_cell(eval, prop)?;
        if cell.is_nil() {
            return Ok(Value::NIL);
        }
    }

    let table = cell.cons_cdr();
    prepare_uniprop_ascii_cache(&table);
    Ok(table)
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

fn decode_uniprop_run_length(table: Value, value: Value) -> Value {
    let Some(ValueKind::Fixnum(index)) = Some(value.kind()) else {
        return value;
    };
    if index < 0 {
        return value;
    }
    let Some(value_table) = char_table_extra_slot_value(&table, 4) else {
        return value;
    };
    if !value_table.is_vector() {
        return value;
    }
    value_table
        .as_vector_data()
        .and_then(|values| values.get(index as usize).copied())
        .unwrap_or(value)
}

fn decode_unicode_property_map_value(table: Value, value: Value) -> Value {
    if !is_char_code_property_table(&table) {
        return value;
    }

    match char_table_extra_slot_value(&table, 1).map(|v| v.kind()) {
        Some(ValueKind::Fixnum(0)) => decode_uniprop_run_length(table, value),
        _ => value,
    }
}

/// `(get-unicode-property-internal CHAR-TABLE CH)`.
///
/// This mirrors GNU's C fast path for Unicode property tables: `CHAR-TABLE`
/// must have purpose `char-code-property-table` and five extra slots; a fixnum
/// decoder in extra slot 1 selects the built-in run-length decoder.
pub(crate) fn builtin_get_unicode_property_internal(args: Vec<Value>) -> EvalResult {
    expect_args("get-unicode-property-internal", &args, 2)?;
    let table = args[0];
    let ch = expect_character(&args[1])?;

    if !is_char_table(&table) {
        return Err(wrong_type("char-table-p", &table));
    }
    if !is_char_code_property_table(&table) {
        return Err(invalid_unicode_property_table());
    }

    let decoder = char_table_extra_slot_value(&table, 1).unwrap_or(Value::NIL);
    let value = ct_lookup(&table, ch)?;
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
fn encode_uniprop_value_run_length(table: Value, value: Value) -> Result<Value, Flow> {
    let value_table = char_table_extra_slot_value(&table, 4)
        .filter(|v| v.is_vector())
        .and_then(|v| v.as_vector_data());
    let found =
        value_table.and_then(|values| values.iter().position(|entry| eq_value(entry, &value)));
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
fn encode_uniprop_value_numeric(table: Value, value: Value) -> Result<Value, Flow> {
    let _ = expect_fixnump(&value)?;
    let mut value_table: Vec<Value> = char_table_extra_slot_value(&table, 4)
        .filter(|v| v.is_vector())
        .and_then(|v| v.as_vector_data())
        .map(|values| values.to_vec())
        .unwrap_or_default();
    if let Some(index) = value_table.iter().position(|entry| eq_value(entry, &value)) {
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
pub(crate) fn builtin_put_unicode_property_internal(args: Vec<Value>) -> EvalResult {
    expect_args("put-unicode-property-internal", &args, 3)?;
    let table = args[0];
    let ch = expect_character(&args[1])?;
    let value = args[2];

    if !is_char_table(&table) {
        return Err(wrong_type("char-table-p", &table));
    }
    if !is_char_code_property_table(&table) {
        return Err(invalid_unicode_property_table());
    }

    // The encoder index lives in extra slot 2 (a fixnum or nil); nil and any
    // out-of-range index mean "no encoding" (GNU's `uniprop_get_encoder`).
    let encoded = match char_table_extra_slot_value(&table, 2).map(|v| v.kind()) {
        Some(ValueKind::Fixnum(0)) => encode_uniprop_value_character(value)?,
        Some(ValueKind::Fixnum(1)) => encode_uniprop_value_run_length(table, value)?,
        Some(ValueKind::Fixnum(2)) => encode_uniprop_value_numeric(table, value)?,
        _ => value,
    };

    ct_set_single(&table, ch, encoded);
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// Bool-vector builtins
// ---------------------------------------------------------------------------

/// `(make-bool-vector LENGTH INIT)` -- create a bool vector of LENGTH bits,
/// each initialized to INIT (nil or non-nil).
pub(crate) fn builtin_make_bool_vector(args: Vec<Value>) -> EvalResult {
    expect_args("make-bool-vector", &args, 2)?;
    let length = expect_wholenump(&args[0])?;
    let init_val = if args[1].is_truthy() {
        Value::fixnum(1)
    } else {
        Value::fixnum(0)
    };
    let len = length as usize;
    let mut vec = Vec::with_capacity(2 + len);
    vec.push(Value::symbol(BOOL_VECTOR_TAG));
    vec.push(Value::fixnum(length));
    for _ in 0..len {
        vec.push(init_val);
    }
    Ok(Value::vector(vec))
}

/// `(bool-vector &rest OBJECTS)` -- create a bool-vector from OBJECTS
/// truthiness.
pub(crate) fn builtin_bool_vector(args: Vec<Value>) -> EvalResult {
    let bits: Vec<bool> = args.into_iter().map(|v| v.is_truthy()).collect();
    Ok(bool_vector_from_bits(&bits))
}

/// `(bool-vector-p OBJ)` -- return t if OBJ is a bool-vector.
pub(crate) fn builtin_bool_vector_p(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-p", &args, 1)?;
    Ok(Value::bool_val(is_bool_vector(&args[0])))
}

/// Helper: extract a bool-vector's length.
fn bv_length(vec: &[Value]) -> i64 {
    match vec[BV_SIZE].kind() {
        ValueKind::Fixnum(n) => n,
        _ => 0,
    }
}

/// Helper: extract the bits of a bool-vector as a `Vec<bool>`.
fn bv_bits(vec: &[Value]) -> Vec<bool> {
    let len = bv_length(vec) as usize;
    let mut bits = Vec::with_capacity(len);
    for i in 0..len {
        let v = &vec[2 + i];
        bits.push(v.as_fixnum().is_some_and(|n| n != 0));
    }
    bits
}

/// `(bool-vector-count-population BV)` -- count the number of true values.
pub(crate) fn builtin_bool_vector_count_population(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-count-population", &args, 1)?;
    let (bits, _len) = extract_bv_bits(&args[0])?;
    let count = bits.iter().filter(|&&b| b).count();
    Ok(Value::fixnum(count as i64))
}

fn extract_bv_bits(value: &Value) -> Result<(Vec<bool>, i64), Flow> {
    if !is_bool_vector(value) {
        return Err(wrong_type("bool-vector-p", value));
    }
    let vec = value.as_vector_data().unwrap().clone();
    let len = bv_length(&vec);
    let bits = bv_bits(&vec);
    Ok((bits, len))
}

/// Build a bool-vector `Value` from a slice of bools.
pub(crate) fn bool_vector_from_bits(bits: &[bool]) -> Value {
    let len = bits.len();
    let mut vec = Vec::with_capacity(2 + len);
    vec.push(Value::symbol(BOOL_VECTOR_TAG));
    vec.push(Value::fixnum(len as i64));
    for &b in bits {
        vec.push(Value::fixnum(if b { 1 } else { 0 }));
    }
    Value::vector(vec)
}

/// GNU's `NILP (dest)`: for the bool-vector set ops the optional destination is
/// a real target only when it is supplied AND non-nil. An omitted arg and an
/// explicit `nil` are identical (optional args default to nil), so both must
/// allocate a fresh bool-vector rather than type-check nil as the destination.
fn optional_bv_dest(args: &[Value], index: usize) -> Option<Value> {
    args.get(index).copied().filter(|v| !v.is_nil())
}

/// `(bool-vector-intersection A B &optional C)` -- bitwise AND.
/// If C is provided, store result in C and return C; otherwise return a new
/// bool-vector.
pub(crate) fn builtin_bool_vector_intersection(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-intersection", &args, 2)?;
    expect_max_args("bool-vector-intersection", &args, 3)?;
    let (bits_a, len_a) = extract_bv_bits(&args[0])?;
    let (bits_b, len_b) = extract_bv_bits(&args[1])?;
    if len_a != len_b {
        return Err(signal(
            LispCondition::WrongLengthArgument,
            vec![Value::fixnum(len_a), Value::fixnum(len_b)],
        ));
    }
    let result_bits: Vec<bool> = bits_a
        .iter()
        .zip(bits_b.iter())
        .map(|(&a, &b)| a && b)
        .collect();

    if let Some(dest) = optional_bv_dest(&args, 2) {
        let changed = store_bv_result_with_expected_lengths(&dest, &result_bits, &[len_a, len_b])?;
        Ok(if changed { dest } else { Value::NIL })
    } else {
        Ok(bool_vector_from_bits(&result_bits))
    }
}

/// `(bool-vector-union A B &optional C)` -- bitwise OR.
pub(crate) fn builtin_bool_vector_union(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-union", &args, 2)?;
    expect_max_args("bool-vector-union", &args, 3)?;
    let (bits_a, len_a) = extract_bv_bits(&args[0])?;
    let (bits_b, len_b) = extract_bv_bits(&args[1])?;
    if len_a != len_b {
        return Err(signal(
            LispCondition::WrongLengthArgument,
            vec![Value::fixnum(len_a), Value::fixnum(len_b)],
        ));
    }
    let result_bits: Vec<bool> = bits_a
        .iter()
        .zip(bits_b.iter())
        .map(|(&a, &b)| a || b)
        .collect();

    if let Some(dest) = optional_bv_dest(&args, 2) {
        let changed = store_bv_result_with_expected_lengths(&dest, &result_bits, &[len_a, len_b])?;
        Ok(if changed { dest } else { Value::NIL })
    } else {
        Ok(bool_vector_from_bits(&result_bits))
    }
}

/// `(bool-vector-exclusive-or A B &optional C)` -- bitwise XOR.
pub(crate) fn builtin_bool_vector_exclusive_or(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-exclusive-or", &args, 2)?;
    expect_max_args("bool-vector-exclusive-or", &args, 3)?;
    let (bits_a, len_a) = extract_bv_bits(&args[0])?;
    let (bits_b, len_b) = extract_bv_bits(&args[1])?;
    if len_a != len_b {
        return Err(signal(
            LispCondition::WrongLengthArgument,
            vec![Value::fixnum(len_a), Value::fixnum(len_b)],
        ));
    }
    let result_bits: Vec<bool> = bits_a
        .iter()
        .zip(bits_b.iter())
        .map(|(&a, &b)| a ^ b)
        .collect();

    if let Some(dest) = optional_bv_dest(&args, 2) {
        let changed = store_bv_result_with_expected_lengths(&dest, &result_bits, &[len_a, len_b])?;
        Ok(if changed { dest } else { Value::NIL })
    } else {
        Ok(bool_vector_from_bits(&result_bits))
    }
}

/// `(bool-vector-not A &optional B)` -- bitwise NOT.
///
/// If B is provided, store result in B and return B; otherwise return a new
/// bool-vector.
pub(crate) fn builtin_bool_vector_not(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-not", &args, 1)?;
    expect_max_args("bool-vector-not", &args, 2)?;
    let (bits, len_a) = extract_bv_bits(&args[0])?;
    let result_bits: Vec<bool> = bits.into_iter().map(|b| !b).collect();
    if let Some(dest) = optional_bv_dest(&args, 1) {
        store_bv_result_with_expected_lengths(&dest, &result_bits, &[len_a])?;
        Ok(dest)
    } else {
        Ok(bool_vector_from_bits(&result_bits))
    }
}

/// `(bool-vector-set-difference A B &optional C)` -- `A & (not B)`.
pub(crate) fn builtin_bool_vector_set_difference(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-set-difference", &args, 2)?;
    expect_max_args("bool-vector-set-difference", &args, 3)?;
    let (bits_a, len_a) = extract_bv_bits(&args[0])?;
    let (bits_b, len_b) = extract_bv_bits(&args[1])?;
    if len_a != len_b {
        return Err(signal(
            LispCondition::WrongLengthArgument,
            vec![Value::fixnum(len_a), Value::fixnum(len_b)],
        ));
    }
    let result_bits: Vec<bool> = bits_a
        .iter()
        .zip(bits_b.iter())
        .map(|(&a, &b)| a && !b)
        .collect();
    if let Some(dest) = optional_bv_dest(&args, 2) {
        let changed = store_bv_result_with_expected_lengths(&dest, &result_bits, &[len_a, len_b])?;
        Ok(if changed { dest } else { Value::NIL })
    } else {
        Ok(bool_vector_from_bits(&result_bits))
    }
}

/// `(bool-vector-count-consecutive BV BOOL START)` -- count matching bits from
/// START until the first non-matching bit or the end.
pub(crate) fn builtin_bool_vector_count_consecutive(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-count-consecutive", &args, 3)?;
    let (bits, len) = extract_bv_bits(&args[0])?;
    let target = args[1].is_truthy();
    let start = expect_wholenump(&args[2])?;
    if start > len {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], Value::fixnum(start)],
        ));
    }
    let mut count = 0usize;
    for bit in bits.iter().skip(start as usize) {
        if *bit != target {
            break;
        }
        count += 1;
    }
    Ok(Value::fixnum(count as i64))
}

/// `(bool-vector-subsetp A B)` -- return t if every true bit in A is also true
/// in B.
pub(crate) fn builtin_bool_vector_subsetp(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-subsetp", &args, 2)?;
    let (bits_a, len_a) = extract_bv_bits(&args[0])?;
    let (bits_b, len_b) = extract_bv_bits(&args[1])?;
    if len_a != len_b {
        return Err(signal(
            LispCondition::WrongLengthArgument,
            vec![
                Value::fixnum(len_a),
                Value::fixnum(len_b),
                Value::fixnum(len_b),
            ],
        ));
    }
    let is_subset = bits_a.iter().zip(bits_b.iter()).all(|(&a, &b)| !a || b);
    Ok(Value::bool_val(is_subset))
}

/// Store bits into an existing bool-vector (for the optional dest argument).
fn store_bv_result_with_expected_lengths(
    dest: &Value,
    bits: &[bool],
    expected_lengths: &[i64],
) -> Result<bool, Flow> {
    if !is_bool_vector(dest) {
        return Err(wrong_type("bool-vector-p", dest));
    }
    let v = dest.as_vector_data().unwrap().clone();
    let len = bv_length(&v) as usize;
    if len != bits.len() {
        let mut payload: Vec<Value> = expected_lengths
            .iter()
            .copied()
            .map(Value::fixnum)
            .collect();
        payload.push(Value::fixnum(len as i64));
        return Err(signal(LispCondition::WrongLengthArgument, payload));
    }
    let mut slots = dest
        .as_vector_data()
        .map(|items| items.to_vec())
        .unwrap_or_default();
    let changed = bits.iter().enumerate().any(|(i, &b)| {
        let current = slots
            .get(2 + i)
            .copied()
            .map(|value| value.as_fixnum().is_some_and(|n| n != 0))
            .unwrap_or(false);
        current != b
    });
    if !changed {
        return Ok(false);
    }
    for (i, &b) in bits.iter().enumerate() {
        slots[2 + i] = Value::fixnum(if b { 1 } else { 0 });
    }
    let _ = dest.replace_vector_data(slots);
    Ok(true)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
