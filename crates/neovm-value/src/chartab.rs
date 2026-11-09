//! Char-table container substrate (GNU `chartab.c`): the pure Value-tree
//! machinery for `PVEC_CHAR_TABLE` / `PVEC_SUB_CHAR_TABLE` — make, ref/set,
//! range writes, copy, atomic/effective runs, external slots, and the
//! uniprop compression decoders — moved below the evaluator with the
//! stage-3 split (C7a). The evaluator-bound subrs, the obarray-quoted
//! error paths, and the `map-char-table` / Unicode-property entry points
//! stay in `emacs_core::text::chartable`, which re-exports this module's
//! public surface so historical paths are unchanged.
//!
//! No evaluator item is named here: scratch GC roots go through the typed
//! `scratch_bridge` hooks (the plist-core route), and interning uses the
//! value-layer interner.

use crate::error::{EvalResult, Flow, LispCondition, expect_args, signal};
use crate::intern::{SymId, intern};
use crate::tagged::SymbolMode;
use crate::value::scratch_bridge::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::value::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum valid Unicode code point.
const MAX_CHAR: i64 = 0x3F_FFFF;
const CT_LOGICAL_LENGTH: i64 = MAX_CHAR + 1;
const CT_ASCII_CACHE_LEN: usize = 128;
const GNU_CHAR_TABLE_STANDARD_SLOTS: usize = 4 + GNU_CHAR_TABLE_CONTENT_BLOCKS_USIZE;
const GNU_CHAR_TABLE_CONTENT_BLOCKS_USIZE: usize = 64;
const GNU_CHAR_TABLE_CONTENT_START: usize = 4;
const GNU_CHAR_TABLE_ASCII_SLOT: usize = 3;
const GNU_CHARTAB_SIZE: [usize; 4] = [64, 16, 32, 128];
const GNU_CHARTAB_CHARS: [i64; 4] = [65_536, 4_096, 128, 1];
// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

/// Return `true` if `v` is a char-table.
#[inline]
#[doc(hidden)]
pub fn is_char_table(v: &Value) -> bool {
    v.is_char_table()
}
fn is_sub_char_table(v: Value) -> bool {
    v.is_sub_char_table()
}
/// GNU `XCHAR_TABLE (table)->defalt`: the value characters with no entry of
/// their own fall back to, or nil for a non-char-table.
#[doc(hidden)]
pub fn char_table_default(table: &Value) -> Value {
    table
        .as_char_table_obj()
        .map_or(Value::NIL, |obj| obj.defalt.get())
}
/// Return the logical sequence length if `v` is a char-table.
#[doc(hidden)]
pub fn char_table_length(v: &Value) -> Option<i64> {
    v.is_char_table().then_some(CT_LOGICAL_LENGTH)
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
        obj.set_contents(idx, value);
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
fn char_table_ascii(table: Value, mode: SymbolMode) -> Value {
    let Some(obj) = table.as_char_table_obj() else {
        return Value::NIL;
    };
    let mut sub = obj.contents[0].get();
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
    if is_char_code_property_table(&table, mode) && uniprop_compressed_string(val).is_some() {
        uniprop_table_uncompress(sub, 0).unwrap_or(val)
    } else {
        val
    }
}
#[doc(hidden)]
pub fn set_char_table_ascii(table: Value, value: Value) {
    bump_char_table_write_tick();
    let _ = table.with_char_table_mut(|obj| obj.set_ascii(value));
}
#[doc(hidden)]
pub fn set_char_table_contents(table: Value, idx: usize, value: Value) {
    bump_char_table_write_tick();
    let _ = table.with_char_table_mut(|obj| {
        obj.set_contents(idx, value);
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
#[doc(hidden)]
pub fn char_table_set_char_direct(table: Value, c: i64, value: Value, mode: SymbolMode) {
    bump_char_table_write_tick();
    let Some(obj) = table.as_char_table_obj() else {
        return;
    };
    if (0..CT_ASCII_CACHE_LEN as i64).contains(&c) && is_sub_char_table(obj.ascii.get()) {
        sub_char_table_set_slot(obj.ascii.get(), c as usize, value);
        return;
    }

    let idx = chartab_idx(c, 0, 0);
    let current = obj.contents[idx].get();
    let child = if is_sub_char_table(current) {
        current
    } else {
        let child = make_sub_char_table(1, idx as i64 * GNU_CHARTAB_CHARS[0], current);
        set_char_table_contents(table, idx, child);
        child
    };
    sub_char_table_set(child, c, value, is_char_code_property_table(&table, mode));
    if (0..CT_ASCII_CACHE_LEN as i64).contains(&c) {
        set_char_table_ascii(table, char_table_ascii(table, mode));
    }
}
fn sub_char_table_set_range(table: Value, from: i64, to: i64, value: Value, is_uniprop: bool) {
    // Scalars only: every read of `contents` below re-derives its shared
    // borrow. The write arms take `&mut` to this same object's backing, which
    // invalidates any shared tag captured before them (Stacked Borrows pops
    // the sibling), so a loop-long `obj` reference would be a stale tag on the
    // next iteration.
    let (depth, min_char) = match table.as_sub_char_table_obj() {
        Some(obj) => (obj.depth as usize, obj.min_char as i64),
        None => return,
    };
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
            let current = table
                .as_sub_char_table_obj()
                .and_then(|obj| obj.contents.get(i).copied())
                .unwrap_or(Value::NIL);
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
#[doc(hidden)]
pub fn char_table_set_range_direct(
    table: Value,
    from: i64,
    to: i64,
    value: Value,
    mode: SymbolMode,
) {
    bump_char_table_write_tick();
    if from == to {
        char_table_set_char_direct(table, from, value, mode);
        return;
    }
    let is_uniprop = is_char_code_property_table(&table, mode);
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
                .map(|obj| obj.contents[idx].get())
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
        set_char_table_ascii(table, char_table_ascii(table, mode));
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
#[doc(hidden)]
pub fn wrong_type(pred: &str, got: &Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol(pred), *got],
    )
}
#[doc(hidden)]
pub fn prepare_uniprop_ascii_cache(table: &Value, mode: SymbolMode) {
    if is_char_code_property_table(table, mode) {
        set_char_table_ascii(*table, char_table_ascii(*table, mode));
    }
}
/// GNU's sub-char-table literal diagnostics (src/lread.c:3322-3333).
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[doc(hidden)]
pub enum SubCharTableReadError {
    #[error("Invalid size of sub-char-table")]
    MissingHeader,
    #[error("Invalid depth in sub-char-table")]
    Depth,
    #[error("Invalid size in sub-char-table")]
    SlotCount,
    #[error("Invalid minimum character in sub-char-table")]
    MinimumCharacter,
}
/// Validated external GNU depth code; no mutator state is stored here.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, num_enum::TryFromPrimitive, num_enum::IntoPrimitive,
)]
#[repr(u8)]
enum SubCharTableReadDepth {
    First = 1,
    Second = 2,
    Third = 3,
}
impl SubCharTableReadDepth {
    /// External GNU slot counts, src/chartab.c:30-35; not enum ordinals.
    const fn slots(self) -> usize {
        match self {
            Self::First => 16,
            Self::Second => 32,
            Self::Third => 128,
        }
    }
}
/// Build a sub-char-table from GNU's readable `#^^[DEPTH MIN-CHAR ...]`
/// literal, as `lread.c` does.
#[doc(hidden)]
pub fn make_sub_char_table_from_external_slots(
    items: &[Value],
) -> Result<Value, SubCharTableReadError> {
    if items.len() < 2 {
        return Err(SubCharTableReadError::MissingHeader);
    }
    let depth = items[0]
        .as_fixnum()
        .and_then(|code| u8::try_from(code).ok())
        .and_then(|code| SubCharTableReadDepth::try_from(code).ok())
        .ok_or(SubCharTableReadError::Depth)?;
    if items.len() != 2 + depth.slots() {
        return Err(SubCharTableReadError::SlotCount);
    }
    let min_char = items[1]
        .as_fixnum()
        .filter(|code| (0..=MAX_CHAR).contains(code))
        .ok_or(SubCharTableReadError::MinimumCharacter)?;
    let depth_code: u8 = depth.into();
    Ok(Value::make_sub_char_table(
        i32::from(depth_code),
        min_char as i32,
        items[2..].to_vec(),
    ))
}
#[doc(hidden)]
pub fn char_table_extra_slot_value(table: &Value, idx: usize) -> Option<Value> {
    table
        .as_char_table_obj()
        .and_then(|obj| obj.extras.get(idx).copied())
}
#[doc(hidden)]
pub fn set_char_table_extra_slot(table: &Value, idx: usize, value: Value) {
    bump_char_table_write_tick();
    let _ = table.with_char_table_mut(|obj| {
        obj.set_extra(idx, value);
    });
}
// GNU checks a char-table purpose with EQ(purpose, Qchar_code_property_table).
// The typed accessor honors symbols-with-pos-enabled while keeping the
// cheap bare-tag path and cached SymId comparison for canonical purposes.
fn char_code_property_table_sym_id() -> SymId {
    static ID: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| intern("char-code-property-table"))
}
/// The retired in-band tags (P3.2 L0.8) as the JIT's inline `aref` used to
/// compare them: the `--char-table--` and `--bool-vector--` symbols' bits,
/// and the length from which a tagged vector was a char-table. Only the
/// measurement knob that re-emits the retired slot-0 test reads this.
#[doc(hidden)]
pub fn inline_vector_tag_shape() -> (u64, u64, usize) {
    (
        Value::from_sym_id(intern("--char-table--")).bits() as u64,
        Value::from_sym_id(intern("--bool-vector--")).bits() as u64,
        5,
    )
}
#[doc(hidden)]
pub fn is_char_code_property_table(table: &Value, mode: SymbolMode) -> bool {
    table.as_char_table_obj().is_some_and(|obj| {
        obj.purpose.get().as_symbol_id(mode) == Some(char_code_property_table_sym_id())
            && obj.extras.len() == 5
    })
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
            crate::strings_core::lisp_string_char_codes(string)
                .into_iter()
                .map(|code| {
                    crate::emacs_char::char_to_byte_safe(code)
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
        let code = crate::emacs_char::string_char_advance(bytes, &mut pos);
        raw_codes.push(
            crate::emacs_char::char_to_byte_safe(code.code())
                .map(u32::from)
                .unwrap_or(code.code()),
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
/// Build a NeoVM char-table from GNU's readable `#^[...]` char-table literal.
///
/// GNU's external order is:
/// `DEFAULT PARENT PURPOSE ASCII CONTENTS[64] EXTRAS...`.
#[doc(hidden)]
pub fn make_char_table_from_external_slots(items: &[Value]) -> Result<Value, String> {
    make_char_table_from_external_slots_with_symbols(items, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn make_char_table_from_external_slots_with_symbols(
    items: &[Value],
    mode: SymbolMode,
) -> Result<Value, String> {
    if items.len() < GNU_CHAR_TABLE_STANDARD_SLOTS {
        return Err("Invalid size char-table".to_string());
    }

    let default = items[0];
    let parent = items[1];
    let purpose = items[2];
    let extra_count = items.len() - GNU_CHAR_TABLE_STANDARD_SLOTS;
    let table = Value::make_char_table(purpose, default, extra_count);
    let _ = table.with_char_table_mut(|obj| {
        obj.set_parent(parent);
        obj.set_ascii(items[GNU_CHAR_TABLE_ASCII_SLOT]);
        for (index, &value) in items[GNU_CHAR_TABLE_CONTENT_START..GNU_CHAR_TABLE_STANDARD_SLOTS]
            .iter()
            .enumerate()
        {
            obj.set_contents(index, value);
        }
        obj.copy_extras(&items[GNU_CHAR_TABLE_STANDARD_SLOTS..]);
    });
    if purpose.as_symbol_id(mode) == Some(char_code_property_table_sym_id()) && extra_count == 5 {
        set_char_table_ascii(table, char_table_ascii(table, mode));
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
#[doc(hidden)]
pub fn copy_char_table_with_symbols(table: Value, mode: SymbolMode) -> Option<Value> {
    let obj = table.as_char_table_obj()?;
    let copy = Value::make_char_table(obj.purpose.get(), obj.defalt.get(), obj.extras.len());
    let contents = obj.contents.each_ref().map(|cell| {
        let value = cell.get();
        if is_sub_char_table(value) {
            copy_sub_char_table_direct(value).unwrap_or(value)
        } else {
            value
        }
    });
    let extras = obj.extras.to_vec();
    let _ = copy.with_char_table_mut(|copy_obj| {
        copy_obj.set_parent(obj.parent.get());
        copy_obj.copy_contents(&contents);
        copy_obj.copy_extras(&extras);
    });
    set_char_table_ascii(copy, char_table_ascii(copy, mode));
    Some(copy)
}
// ---------------------------------------------------------------------------
// Char-table builtins
// ---------------------------------------------------------------------------

/// Create a char-table `Value` directly (for use in bootstrap code).
#[doc(hidden)]
pub fn make_char_table_value(sub_type: Value, default: Value) -> Value {
    make_char_table_with_extra_slots(sub_type, default, 0)
}
/// Create a char-table with a specified number of extra slots.
#[doc(hidden)]
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
#[doc(hidden)]
pub fn ct_ref(table: &Value, ch: i64) -> Value {
    ct_ref_with_symbols(table, ch, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn ct_ref_with_symbols(table: &Value, ch: i64, mode: SymbolMode) -> Value {
    if !(0..=MAX_CHAR).contains(&ch) {
        return Value::NIL;
    }
    ct_lookup_with_symbols(table, ch, mode).unwrap_or(Value::NIL)
}
/// Set a single character entry in a char-table Value (for bootstrap code).
/// Panics if `table` is not a char-table Vector.
#[doc(hidden)]
pub fn ct_set_single(table: &Value, ch: i64, value: Value) {
    ct_set_single_with_symbols(table, ch, value, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn ct_set_single_with_symbols(table: &Value, ch: i64, value: Value, mode: SymbolMode) {
    bump_char_table_write_tick();
    assert!(
        table.is_char_table(),
        "ct_set_single: expected a char-table"
    );
    char_table_set_char_direct(*table, ch, value, mode);
}
fn ct_lookup_ascii_cached(mut obj: &crate::tagged::header::CharTableObj, ch: i64) -> Option<Value> {
    if !(0..CT_ASCII_CACHE_LEN as i64).contains(&ch) {
        return None;
    }
    loop {
        // A single observing projection covers both the type and contents.
        // No Lisp callback or write separates these adjacent reads.
        let value = if let Some(ascii) = obj.ascii.get().as_sub_char_table_obj() {
            ascii
                .contents
                .get(ch as usize)
                .copied()
                .unwrap_or(Value::NIL)
        } else {
            obj.ascii.get()
        };
        if !value.is_nil() {
            return Some(value);
        }
        if !obj.defalt.get().is_nil() {
            return Some(obj.defalt.get());
        }
        match obj.parent.get().as_char_table_obj() {
            Some(parent) => obj = parent,
            None => return Some(Value::NIL),
        }
    }
}

/// Batch form of [`ct_lookup_ascii_cached`] for the 128 ASCII codes: fill
/// `out[cp]` with exactly the raw value `ct_lookup (table, cp)` resolves, in
/// one walk of the ascii-slot/default/parent chain (GNU
/// `CHAR_TABLE_REF_ASCII`, lisp.h:2133-2146).
///
/// Returns `false` and leaves `out` untouched when `table` is not a
/// char-table; callers fall back to per-character lookup.
#[doc(hidden)]
pub fn ct_ascii_raw_values(table: &Value, out: &mut [Value; CT_ASCII_CACHE_LEN]) -> bool {
    let Some(mut obj) = table.as_char_table_obj() else {
        return false;
    };
    let mut resolved = [false; CT_ASCII_CACHE_LEN];
    let mut remaining = CT_ASCII_CACHE_LEN;
    loop {
        // Same resolution order as `ct_lookup_ascii_cached`: this table's
        // ASCII slot, then its default, then the parent chain.
        let ascii = obj.ascii.get();
        if let Some(sub) = ascii.as_sub_char_table_obj() {
            for (cp, slot) in resolved.iter_mut().enumerate() {
                if !*slot {
                    let value = sub.contents.get(cp).copied().unwrap_or(Value::NIL);
                    if !value.is_nil() {
                        out[cp] = value;
                        *slot = true;
                        remaining -= 1;
                    }
                }
            }
        } else if !ascii.is_nil() {
            for (cp, slot) in resolved.iter_mut().enumerate() {
                if !*slot {
                    out[cp] = ascii;
                    *slot = true;
                    remaining -= 1;
                }
            }
        }
        if remaining == 0 {
            break;
        }
        let defalt = obj.defalt.get();
        if !defalt.is_nil() {
            for (cp, slot) in resolved.iter_mut().enumerate() {
                if !*slot {
                    out[cp] = defalt;
                    *slot = true;
                    remaining -= 1;
                }
            }
        }
        if remaining == 0 {
            break;
        }
        match obj.parent.get().as_char_table_obj() {
            Some(parent) => obj = parent,
            None => break,
        }
    }
    for (cp, slot) in resolved.iter().enumerate() {
        if !*slot {
            out[cp] = Value::NIL;
        }
    }
    true
}
// Keep the existing explicit Bare lookup policy zero-sized at the shared
// lookup seam, so it needs no runtime policy argument. Semantic callers still
// pass their captured SymbolMode through the same lookup body.
trait CtLookupPolicy: Copy {
    fn symbol_mode(self) -> SymbolMode;
}
impl CtLookupPolicy for SymbolMode {
    fn symbol_mode(self) -> SymbolMode {
        self
    }
}
#[derive(Clone, Copy, Debug)]
struct BareCtLookup;
impl CtLookupPolicy for BareCtLookup {
    fn symbol_mode(self) -> SymbolMode {
        SymbolMode::Bare
    }
}
/// Recursive char-table lookup: check own entries, then default, then parent.
///
/// This matches GNU Emacs semantics:
/// 1. Look up the character in the char-table's data pairs
/// 2. If the local entry is nil or absent, use the char-table's default value
/// 3. If default is nil, recursively check the parent char-table
#[doc(hidden)]
pub fn ct_lookup(table: &Value, ch: i64) -> EvalResult {
    ct_lookup_impl(table, ch, BareCtLookup)
}
#[doc(hidden)]
pub fn ct_lookup_with_symbols(table: &Value, ch: i64, mode: SymbolMode) -> EvalResult {
    ct_lookup_impl(table, ch, mode)
}

/// `char-table-subtype`: the table's PURPOSE symbol (GNU
/// `Fchar_table_subtype`, chartab.c). Pure Value in/out.
#[doc(hidden)]
pub fn builtin_char_table_subtype(args: Vec<Value>) -> EvalResult {
    expect_args("char-table-subtype", &args, 1)?;
    let table = &args[0];
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    Ok(table.as_char_table_obj().unwrap().purpose.get())
}
fn ct_lookup_impl<P: CtLookupPolicy>(table: &Value, ch: i64, policy: P) -> EvalResult {
    let Some(obj) = table.as_char_table_obj() else {
        return Err(wrong_type("char-table-p", table));
    };
    if let Some(value) = ct_lookup_ascii_cached(obj, ch) {
        return Ok(value);
    }
    let idx = chartab_idx(ch, 0, 0);
    let mut val = obj.contents[idx].get();
    if is_sub_char_table(val) {
        let is_uniprop = obj.purpose.get().as_symbol_id(policy.symbol_mode())
            == Some(char_code_property_table_sym_id())
            && obj.extras.len() == 5;
        val = sub_char_table_ref(val, ch, is_uniprop);
    }
    if !val.is_nil() {
        return Ok(val);
    }
    if !obj.defalt.get().is_nil() {
        return Ok(obj.defalt.get());
    }
    if is_char_table(&obj.parent.get()) {
        return ct_lookup_impl(&obj.parent.get(), ch, policy);
    }
    Ok(Value::NIL)
}
#[doc(hidden)]
pub fn translate_char_with_symbols(table: &Value, c: i64, mode: SymbolMode) -> i64 {
    if is_char_table(table) {
        match ct_lookup_with_symbols(table, c, mode) {
            Ok(val) => match val.kind() {
                ValueKind::Fixnum(n) if (0..=crate::emacs_char::MAX_CHAR as i64).contains(&n) => n,
                _ => c,
            },
            Err(_) => c,
        }
    } else if table.is_cons() {
        let mut result = c;
        let mut cur = *table;
        while cur.is_cons() {
            let car = cur.cons_car();
            result = translate_char_with_symbols(&car, result, mode);
            cur = cur.cons_cdr();
        }
        result
    } else {
        c
    }
}
/// The deepest PLAIN (non-sub-table) cell of `table`'s local tree containing
/// `ch`: the raw slot value (before defalt/parent resolution) and the cell's
/// INCLUSIVE character span. O(tree depth); never materializes runs.
fn ct_local_plain_cell_at(table: &Value, ch: i64, mode: SymbolMode) -> (Value, i64, i64) {
    let Some(obj) = table.as_char_table_obj() else {
        return (Value::NIL, 0, MAX_CHAR);
    };
    let is_uniprop = is_char_code_property_table(table, mode);
    let idx = (ch / GNU_CHARTAB_CHARS[0]) as usize;
    let mut start = idx as i64 * GNU_CHARTAB_CHARS[0];
    let mut end = (start + GNU_CHARTAB_CHARS[0] - 1).min(MAX_CHAR);
    let Some(slot) = obj.contents.get(idx).map(|cell| cell.get()) else {
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
fn ct_effective_value_span_at(table: &Value, ch: i64, mode: SymbolMode) -> (Value, i64, i64) {
    let Some(obj) = table.as_char_table_obj() else {
        return (Value::NIL, 0, MAX_CHAR);
    };
    let (value, start, end) = ct_local_plain_cell_at(table, ch, mode);
    let resolved = if value.is_nil() && !obj.defalt.get().is_nil() {
        obj.defalt.get()
    } else {
        value
    };
    if !resolved.is_nil() || !obj.parent.get().is_char_table() {
        return (resolved, start, end);
    }
    let (parent_value, parent_start, parent_end) =
        ct_effective_value_span_at(&obj.parent.get(), ch, mode);
    (parent_value, start.max(parent_start), end.min(parent_end))
}
#[doc(hidden)]
pub fn ct_lookup_and_range(
    table: &Value,
    ch: i64,
    mode: SymbolMode,
) -> Result<(Value, i64, i64), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    if !table.is_char_table() {
        // Legacy vector representation: tiny fixed layout, the run
        // materialization is cheap and keeps the old semantics exactly.
        for run in ct_effective_runs(table, mode) {
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
    let (value, mut from, mut to) = ct_effective_value_span_at(table, ch, mode);
    while from > 0 {
        let (left_value, left_start, _) = ct_effective_value_span_at(table, from - 1, mode);
        if eq_value_swp(&left_value, &value, mode == SymbolMode::Positioned) {
            from = left_start;
        } else {
            break;
        }
    }
    while to < MAX_CHAR {
        let (right_value, _, right_end) = ct_effective_value_span_at(table, to + 1, mode);
        if eq_value_swp(&right_value, &value, mode == SymbolMode::Positioned) {
            to = right_end;
        } else {
            break;
        }
    }
    Ok((value, from, to))
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
fn ct_lookup_atomic_range(
    table: &Value,
    ch: i64,
    mode: SymbolMode,
) -> Result<(Value, i64, i64), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    for run in ct_effective_runs(table, mode) {
        if ch >= run.start && ch <= run.end {
            return Ok((run.value, run.start, run.end));
        }
    }
    Ok((Value::NIL, 0, MAX_CHAR))
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
    mode: SymbolMode,
) -> (Vec<LocalAtomicRun>, Value, Value) {
    let obj = table.as_char_table_obj().unwrap();
    let runs = clipped_runs(
        ct_local_direct_runs_in_range(table, requested_start, requested_end, mode),
        requested_start,
        requested_end,
        mode,
    )
    .into_iter()
    .map(|run| LocalAtomicRun {
        value: Some(run.value),
        start: run.start,
        end: run.end,
    })
    .collect();
    (runs, obj.defalt.get(), obj.parent.get())
}
/// GNU `char-table-ref-and-range`-style helper used by subsystems that need
/// the effective value together with the maximal contiguous run covering `ch`.
#[doc(hidden)]
pub fn char_table_ref_and_range_with_symbols(
    table: &Value,
    ch: i64,
    mode: SymbolMode,
) -> Result<(Value, i64, i64), Flow> {
    ct_lookup_and_range(table, ch, mode)
}
/// Return the effective value and a contiguous range around `ch` where no local
/// char-table assignment boundary occurs.
///
/// This range may be smaller than GNU's maximal `char-table-ref-and-range`
/// result, but every character in it has the same effective value.  It is for
/// bulk mutators that only need a correct split point and would otherwise pay to
/// rebuild the full effective run list for each cursor step.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn char_table_ref_and_atomic_range(table: &Value, ch: i64) -> Result<(Value, i64, i64), Flow> {
    char_table_ref_and_atomic_range_with_symbols(table, ch, SymbolMode::Bare)
}
fn char_table_ref_and_atomic_range_with_symbols(
    table: &Value,
    ch: i64,
    mode: SymbolMode,
) -> Result<(Value, i64, i64), Flow> {
    ct_lookup_atomic_range(table, ch, mode)
}
/// Return effective atomic runs for a caller-supplied range.
///
/// This has the same local-entry shadowing and default/parent fallback rules as
/// `char_table_ref_and_atomic_range`, but scans the table entries once for the
/// whole requested range.  Bulk mutators can then update each returned run
/// without repeating a reverse lookup from every cursor position.
#[doc(hidden)]
pub fn char_table_atomic_runs_in_range(
    table: &Value,
    start: i64,
    end: i64,
) -> Result<Vec<(Value, i64, i64)>, Flow> {
    char_table_atomic_runs_in_range_with_symbols(table, start, end, SymbolMode::Bare)
}
fn char_table_atomic_runs_in_range_with_symbols(
    table: &Value,
    start: i64,
    end: i64,
    mode: SymbolMode,
) -> Result<Vec<(Value, i64, i64)>, Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    let start = start.max(0);
    let end = end.min(MAX_CHAR);
    if start > end {
        return Ok(Vec::new());
    }

    let (local_runs, default, parent) = ct_local_atomic_runs(table, start, end, mode);
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
                char_table_atomic_runs_in_range_with_symbols(&parent, run.start, run.end, mode)?
            {
                append_atomic_run(&mut out, value, child_start, child_end);
            }
        } else {
            append_atomic_run(&mut out, Value::NIL, run.start, run.end);
        }
    }
    Ok(out)
}
/// Return the sparse local `(key . value)` entries stored directly in a char-table.
///
/// Keys are either character codes (fixnums) or range conses `(FROM . TO)`.
/// Parent/default fallback is intentionally not applied here; callers that need
/// effective values should use `ct_lookup`.
#[doc(hidden)]
pub fn char_table_local_entries(table: &Value) -> Result<Vec<(Value, Value)>, Flow> {
    char_table_local_entries_with_symbols(table, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn char_table_local_entries_with_symbols(
    table: &Value,
    mode: SymbolMode,
) -> Result<Vec<(Value, Value)>, Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    Ok(ct_collect_raw_entries_for_table(*table, false, mode)
        .into_iter()
        .map(|run| (run_key(run.start, run.end), run.value))
        .collect())
}
#[doc(hidden)]
pub fn for_each_char_table_mapping_with_symbols(
    table: &Value,
    mut f: impl FnMut(Value, Value) -> Result<(), Flow>,
    mode: SymbolMode,
) -> Result<(), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }

    let shared_range = Value::cons(Value::fixnum(0), Value::fixnum(MAX_CHAR));
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(shared_range);
    let result = (|| {
        let mut last_emitted_end = None;
        for run in ct_map_char_table_runs(table, mode) {
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
            let value = decode_unicode_property_map_value(*table, run.value, mode);
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
/// Resolve a char-table into non-overlapping effective runs, including nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn ct_resolved_entries(table: &Value) -> Vec<(Value, Value)> {
    ct_resolved_entries_with_symbols(table, SymbolMode::Bare)
}
fn ct_resolved_entries_with_symbols(table: &Value, mode: SymbolMode) -> Vec<(Value, Value)> {
    ct_effective_runs(table, mode)
        .into_iter()
        .filter(|run| !run.value.is_nil())
        .map(|run| (run_key(run.start, run.end), run.value))
        .collect()
}
fn ct_local_direct_runs(table: &Value, mode: SymbolMode) -> Vec<EffectiveRun> {
    ct_local_direct_runs_in_range(table, 0, MAX_CHAR, mode)
}
/// Ranged variant of ct_local_direct_runs: for real char-tables only the
/// slots intersecting [win_start, win_end] are enumerated (the returned runs
/// may extend past the window at both edges; callers clip). Legacy vector
/// tables fall back to the full enumeration — they are 128-slot structures
/// where pruning buys nothing.
fn ct_local_direct_runs_in_range(
    table: &Value,
    win_start: i64,
    win_end: i64,
    mode: SymbolMode,
) -> Vec<EffectiveRun> {
    let obj = table.as_char_table_obj().unwrap();
    let raws = ct_collect_raw_entries_for_table_in_range(*table, true, win_start, win_end, mode);
    let mut runs = Vec::new();
    for raw in raws {
        let value = if raw.value.is_nil() && !obj.defalt.get().is_nil() {
            obj.defalt.get()
        } else {
            raw.value
        };
        push_effective_run(&mut runs, raw.start, raw.end, value, mode);
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
fn push_effective_run(
    runs: &mut Vec<EffectiveRun>,
    start: i64,
    end: i64,
    value: Value,
    mode: SymbolMode,
) {
    if start > end {
        return;
    }
    if let Some(previous) = runs.last_mut()
        && previous.end.saturating_add(1) == start
        && eq_value_swp(&previous.value, &value, mode == SymbolMode::Positioned)
    {
        previous.end = end;
        return;
    }
    runs.push(EffectiveRun { start, end, value });
}
fn clipped_runs(
    runs: impl IntoIterator<Item = EffectiveRun>,
    from: i64,
    to: i64,
    mode: SymbolMode,
) -> Vec<EffectiveRun> {
    if from > to {
        return Vec::new();
    }
    let mut clipped = Vec::new();
    for run in runs {
        let start = run.start.max(from);
        let end = run.end.min(to);
        if start <= end {
            push_effective_run(&mut clipped, start, end, run.value, mode);
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
    mode: SymbolMode,
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
            push_effective_run(out, start, end, run.value, mode);
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
    mode: SymbolMode,
) -> bool {
    let parent_runs = clipped_runs(
        ct_local_direct_runs_in_range(&parent, from, to, mode),
        from,
        to,
        mode,
    );
    let Some(last) = parent_runs.last().copied() else {
        return true;
    };
    let suppress_last = eq_value_swp(
        &last.value,
        &next_local_value,
        mode == SymbolMode::Positioned,
    );
    let emit_len = parent_runs.len() - usize::from(suppress_last);
    for run in parent_runs.into_iter().take(emit_len) {
        if !run.value.is_nil() {
            push_effective_run(out, run.start, run.end, run.value, mode);
        }
    }
    !suppress_last
}
fn push_parent_tail_runs(
    out: &mut Vec<EffectiveRun>,
    parent: Value,
    from: i64,
    to: i64,
    mode: SymbolMode,
) {
    for run in clipped_runs(ct_map_char_table_runs(&parent, mode), from, to, mode) {
        if !run.value.is_nil() {
            push_effective_run(out, run.start, run.end, run.value, mode);
        }
    }
}
fn ct_map_char_table_runs(table: &Value, mode: SymbolMode) -> Vec<EffectiveRun> {
    if !is_char_table(table) {
        return Vec::new();
    }
    let obj = table.as_char_table_obj().unwrap();
    let parent = obj.parent.get();
    let local_runs = ct_local_direct_runs(table, mode);
    let mut out = Vec::new();
    let mut val = obj.ascii.get();
    let mut from = 0;

    for run in local_runs {
        if eq_value_swp(&val, &run.value, mode == SymbolMode::Positioned) {
            continue;
        }

        let mut different_value = true;
        let has_previous_span = from < run.start;
        if val.is_nil() && is_char_table(&parent) && has_previous_span {
            different_value = push_parent_direct_span_runs(
                &mut out,
                parent,
                from,
                run.start - 1,
                run.value,
                mode,
            );
        }
        if !val.is_nil() && different_value && has_previous_span {
            push_effective_run(&mut out, from, run.start - 1, val, mode);
        }
        val = run.value;
        from = run.start;
    }

    if val.is_nil() {
        if is_char_table(&parent) {
            push_parent_tail_runs(&mut out, parent, from, MAX_CHAR, mode);
        }
    } else {
        push_effective_run(&mut out, from, MAX_CHAR, val, mode);
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
#[doc(hidden)]
pub enum OptimizeCharTableTest {
    Equal,
    Eq,
}
fn optimize_values_equal(
    a: Value,
    b: Value,
    test: OptimizeCharTableTest,
    mode: SymbolMode,
) -> bool {
    match test {
        OptimizeCharTableTest::Equal => equal_value(&a, &b, 100),
        OptimizeCharTableTest::Eq => eq_value_swp(&a, &b, mode == SymbolMode::Positioned),
    }
}
/// Stored child values for display dependency capture, without Lisp allocation
/// or expanding ranges into individual character codes. Includes defaults,
/// parent tables and extra slots; callers distinguish tables from glyph vectors.
#[doc(hidden)]
pub fn display_dependency_children(table: Value) -> Option<Vec<Value>> {
    if let Some(table) = table.as_char_table_obj() {
        let mut values = vec![table.defalt.get(), table.parent.get(), table.ascii.get()];
        values.extend(table.contents.iter().map(|cell| cell.get()));
        values.extend_from_slice(table.extras.as_slice());
        return Some(values);
    }
    table
        .as_sub_char_table_obj()
        .map(|table| table.contents.as_slice().to_vec())
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
    // Scalars only, like `sub_char_table_set_range`: the uncompress arm below
    // can write this same table's backing, so each slot read re-derives its
    // shared borrow instead of iterating through one captured reference.
    let (depth, min_char) = match table.as_sub_char_table_obj() {
        Some(obj) => (obj.depth as usize, obj.min_char as i64),
        None => return,
    };
    let span = GNU_CHARTAB_CHARS[depth];
    for idx in 0..GNU_CHARTAB_SIZE[depth] {
        let Some(mut value) = table
            .as_sub_char_table_obj()
            .and_then(|obj| obj.contents.get(idx).copied())
        else {
            break;
        };
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
fn ct_collect_raw_entries_for_table(
    table: Value,
    include_nil: bool,
    mode: SymbolMode,
) -> Vec<RawEntry> {
    ct_collect_raw_entries_for_table_in_range(table, include_nil, 0, MAX_CHAR, mode)
}
/// Ranged variant of the top-level raw-entry collection; see
/// collect_sub_char_table_raw_entries_in_range for the pruning contract.
fn ct_collect_raw_entries_for_table_in_range(
    table: Value,
    include_nil: bool,
    win_start: i64,
    win_end: i64,
    mode: SymbolMode,
) -> Vec<RawEntry> {
    let Some(obj) = table.as_char_table_obj() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let is_uniprop = is_char_code_property_table(&table, mode);
    for (idx, value) in obj.contents.iter().map(|cell| cell.get()).enumerate() {
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
#[doc(hidden)]
pub fn optimize_char_table_with_symbols(
    table: &Value,
    test: OptimizeCharTableTest,
    mode: SymbolMode,
) -> Result<(), Flow> {
    if !is_char_table(table) {
        return Err(wrong_type("char-table-p", table));
    }
    for idx in 0..GNU_CHARTAB_SIZE[0] {
        let value = table.as_char_table_obj().unwrap().contents[idx].get();
        if is_sub_char_table(value) {
            let optimized = optimize_sub_char_table_direct(value, test, mode);
            set_char_table_contents(*table, idx, optimized);
        }
    }
    set_char_table_ascii(*table, char_table_ascii(*table, mode));
    Ok(())
}
fn optimize_sub_char_table_direct(
    table: Value,
    test: OptimizeCharTableTest,
    mode: SymbolMode,
) -> Value {
    let Some(obj) = table.as_sub_char_table_obj() else {
        return table;
    };
    let depth = obj.depth as usize;
    let Some(mut first) = obj.contents.first().copied() else {
        return table;
    };
    if is_sub_char_table(first) {
        first = optimize_sub_char_table_direct(first, test, mode);
        sub_char_table_set_slot(table, 0, first);
    }
    let mut optimizable = !is_sub_char_table(first);
    for idx in 1..GNU_CHARTAB_SIZE[depth] {
        let mut current = table
            .as_sub_char_table_obj()
            .and_then(|obj| obj.contents.get(idx).copied())
            .unwrap_or(Value::NIL);
        if is_sub_char_table(current) {
            current = optimize_sub_char_table_direct(current, test, mode);
            sub_char_table_set_slot(table, idx, current);
        }
        if optimizable && !optimize_values_equal(current, first, test, mode) {
            optimizable = false;
        }
    }
    if optimizable { first } else { table }
}
fn ct_effective_runs(table: &Value, mode: SymbolMode) -> Vec<EffectiveRun> {
    let obj = table.as_char_table_obj().unwrap();
    let local_runs = ct_local_direct_runs(table, mode);
    if !is_char_table(&obj.parent.get()) {
        return local_runs;
    }
    let parent_runs = ct_effective_runs(&obj.parent.get(), mode);
    let mut out = Vec::new();
    let mut parent_idx = 0usize;
    for run in local_runs {
        if !run.value.is_nil() {
            push_effective_run(&mut out, run.start, run.end, run.value, mode);
            continue;
        }
        push_clipped_parent_runs_from_slice(
            &mut out,
            &parent_runs,
            &mut parent_idx,
            run.start,
            run.end,
            mode,
        );
    }
    out
}
fn run_key(start: i64, end: i64) -> Value {
    if start == end {
        Value::fixnum(start)
    } else {
        Value::cons(Value::fixnum(start), Value::fixnum(end))
    }
}
/// Whether some character at or above `from` may look up (as `ct_lookup`
/// does: its own entry, else the table's default, else the parent's) a value
/// satisfying `pred`.  Visits the table's slots in place, allocating
/// nothing.  Over-approximates: the default or the parent counts even when no
/// entry falls back to it.  A char-code property table, whose values are
/// stored compressed, and anything that is not a char-table answer `true`.
#[doc(hidden)]
pub fn char_table_may_hold_value_from(
    table: &Value,
    from: i64,
    pred: &mut dyn FnMut(Value) -> bool,
) -> bool {
    char_table_may_hold_value_from_with_symbols(table, from, pred, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn char_table_may_hold_value_from_with_symbols(
    table: &Value,
    from: i64,
    pred: &mut dyn FnMut(Value) -> bool,
    mode: SymbolMode,
) -> bool {
    if !is_char_table(table) || is_char_code_property_table(table, mode) {
        return true;
    }
    let Some(obj) = table.as_char_table_obj() else {
        return ct_effective_runs(table, mode)
            .iter()
            .any(|run| run.end >= from && pred(run.value));
    };
    let fallback = if !obj.defalt.get().is_nil() {
        pred(obj.defalt.get())
    } else {
        is_char_table(&obj.parent.get())
            && char_table_may_hold_value_from_with_symbols(&obj.parent.get(), from, pred, mode)
    };
    fallback
        || obj
            .contents
            .iter()
            .map(|cell| cell.get())
            .enumerate()
            .any(|(idx, value)| {
                let start = idx as i64 * GNU_CHARTAB_CHARS[0];
                let end = (start + GNU_CHARTAB_CHARS[0] - 1).min(MAX_CHAR);
                end >= from && slot_may_hold_value_from(value, from, pred)
            })
}
/// [`char_table_may_hold_value_from`] for one slot: a leaf value, or a
/// sub-char-table whose slots are visited in turn.
fn slot_may_hold_value_from(value: Value, from: i64, pred: &mut dyn FnMut(Value) -> bool) -> bool {
    if !is_sub_char_table(value) {
        return !value.is_nil() && pred(value);
    }
    let Some(obj) = value.as_sub_char_table_obj() else {
        return true;
    };
    let min_char = obj.min_char as i64;
    let span = GNU_CHARTAB_CHARS[obj.depth as usize];
    obj.contents
        .iter()
        .copied()
        .enumerate()
        .any(|(idx, child)| {
            let end = (min_char + idx as i64 * span + span - 1).min(MAX_CHAR);
            end >= from && slot_may_hold_value_from(child, from, pred)
        })
}
#[doc(hidden)]
pub fn for_each_non_nil_char_table_run<F>(table: &Value, f: F)
where
    F: FnMut(Value, Value),
{
    for_each_non_nil_char_table_run_with_symbols(table, f, SymbolMode::Bare)
}
#[doc(hidden)]
pub fn for_each_non_nil_char_table_run_with_symbols<F>(table: &Value, mut f: F, mode: SymbolMode)
where
    F: FnMut(Value, Value),
{
    if !is_char_table(table) {
        return;
    }

    for run in ct_effective_runs(table, mode) {
        if run.value.is_nil() {
            continue;
        }
        f(run_key(run.start, run.end), run.value);
    }
}
#[doc(hidden)]
pub fn sub_char_table_external_slots(table: &Value) -> Option<(i64, i64, Vec<Value>)> {
    let obj = table.as_sub_char_table_obj()?;
    Some((
        obj.depth as i64,
        obj.min_char as i64,
        obj.contents.as_slice().to_vec(),
    ))
}
#[doc(hidden)]
pub fn char_table_external_slots(table: &Value) -> Option<Vec<Value>> {
    if !is_char_table(table) {
        return None;
    }

    table.char_table_external_slots()
}
/// `(char-table-subtype TABLE)` -- return the sub-type symbol.
/// `char-table-p` + subtype check against a symbol name, without the
/// builtin arg plumbing (two Vec allocs) or the purpose symbol's
/// name-string parse the old callers paid per call.
#[doc(hidden)]
pub fn char_table_has_subtype_named_with_symbols(
    value: &Value,
    name: &str,
    mode: SymbolMode,
) -> bool {
    value
        .as_char_table_obj()
        .is_some_and(|table| table.purpose.get().is_symbol_named(name, mode))
}
#[doc(hidden)]
pub fn assq_cell_eq(key: Value, list: Value, mode: SymbolMode) -> Result<Value, Flow> {
    let mut cursor = list;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return Ok(Value::NIL),
            ValueKind::Cons => {
                let entry = cursor.cons_car();
                if entry.is_cons()
                    && eq_value_swp(&entry.cons_car(), &key, mode == SymbolMode::Positioned)
                {
                    return Ok(entry);
                }
                cursor = cursor.cons_cdr();
            }
            _ => return Err(wrong_type("listp", &list)),
        }
    }
}
#[doc(hidden)]
pub fn decode_uniprop_run_length(table: Value, value: Value) -> Value {
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
fn decode_unicode_property_map_value(table: Value, value: Value, mode: SymbolMode) -> Value {
    if !is_char_code_property_table(&table, mode) {
        return value;
    }

    match char_table_extra_slot_value(&table, 1).map(|v| v.kind()) {
        Some(ValueKind::Fixnum(0)) => decode_uniprop_run_length(table, value),
        _ => value,
    }
}
