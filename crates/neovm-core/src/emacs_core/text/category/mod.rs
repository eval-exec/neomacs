//! Character category tables.
//!
//! GNU Emacs stores category semantics on category-table char-tables:
//! - the char-table contents are category-set bool-vectors
//! - extra slot 0 stores the category docstring vector
//! - the current buffer's `category-table` slot selects the active table
//!
//! NeoVM now mirrors that ownership model instead of routing semantics
//! through a parallel Rust-side manager.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use std::cell::RefCell;

use super::error::{EvalResult, Flow, signal};
use super::value::{
    HashKey, HashTableTest, Value, ValueKind, VecLikeType, bool_vector_equal_hash_key,
};

thread_local! {
    static STANDARD_CATEGORY_TABLE_OBJECT: RefCell<Option<Value>> = const { RefCell::new(None) };
}

/// GNU `syms_of_category` (src/category.c:442-500): the two word-boundary
/// category lists are DEFVAR_LISP specials. Without the special flag a
/// lexical-binding `let` of them lands in the lexenv, invisible to the
/// internal `find_symbol_value` reads the regexp matcher performs.
pub fn register_bootstrap_vars(obarray: &mut super::symbol::Obarray) {
    obarray.set_symbol_value("word-combining-categories", Value::NIL);
    obarray.make_special("word-combining-categories");
    obarray.set_symbol_value("word-separating-categories", Value::NIL);
    obarray.make_special("word-separating-categories");
}

pub fn reset_category_thread_locals() {
    STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = None);
}

pub(crate) fn restore_standard_category_table_object(table: Value) {
    STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = Some(table));
}

pub fn collect_category_gc_roots(roots: &mut Vec<Value>) {
    STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| {
        if let Some(v) = *slot.borrow() {
            roots.push(v);
        }
    });
}

// Phase 10D holdout 4: per-buffer category table char-table now lives in
// `Buffer::slots[BUFFER_SLOT_CATEGORY_TABLE.index()]`, mirroring GNU's
// `BVAR(buf, category_table)` storage. The slot is non-Lisp-visible
// (`install_as_forwarder: false`); the symbol `category-table` continues
// to signal void-variable as in GNU. Reads/writes happen exclusively
// through `(category-table)` / `(set-category-table)`.
const CATEGORY_DOCSTRING_SLOT: i64 = 0;
const CATEGORY_SET_HASH_SLOT: i64 = 1;
const CATEGORY_DOCSTRING_COUNT: usize = 95;
const CATEGORY_MIN: i64 = 0x20;
const CATEGORY_MAX: i64 = 0x7e;

fn is_category_letter(ch: char) -> bool {
    (CATEGORY_MIN as u8 as char..=CATEGORY_MAX as u8 as char).contains(&ch)
}

/// GNU's `CHECK_CATEGORY` (category.h): a category must be a fixnum in the
/// printable ASCII range `0x20..=0x7E`; otherwise signal
/// `(wrong-type-argument categoryp VALUE)`.
fn check_category(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) if (CATEGORY_MIN..=CATEGORY_MAX).contains(&c) => Ok(c),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("categoryp"), *value],
        )),
    }
}

fn extract_char_opt(value: &Value, _fn_name: &str) -> Result<Option<char>, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) => Ok(super::builtins::character_code_to_rust_char(c)),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

fn extract_char(value: &Value, fn_name: &str) -> Result<char, Flow> {
    extract_char_opt(value, fn_name)?.ok_or_else(|| {
        signal(
            "error",
            vec![Value::string(format!(
                "{}: Invalid character code",
                fn_name
            ))],
        )
    })
}

fn extract_char_code(value: &Value, _fn_name: &str) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) => Ok(c),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

fn make_empty_category_set() -> EvalResult {
    super::chartable::builtin_make_bool_vector(vec![Value::fixnum(128), Value::NIL])
}

fn clone_vector_value(value: &Value) -> EvalResult {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            Ok(Value::vector(value.as_vector_data().unwrap().clone()))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), *value],
        )),
    }
}

fn category_set_hash(table: Value) -> Result<Value, Flow> {
    let hash_slot = super::chartable::builtin_char_table_extra_slot(vec![
        table,
        Value::fixnum(CATEGORY_SET_HASH_SLOT),
    ])?;
    if hash_slot.is_nil() {
        let hash = Value::hash_table(HashTableTest::Equal);
        super::chartable::builtin_set_char_table_extra_slot(vec![
            table,
            Value::fixnum(CATEGORY_SET_HASH_SLOT),
            hash,
        ])?;
        Ok(hash)
    } else {
        Ok(hash_slot)
    }
}

fn intern_category_set(table: Value, category_set: Value) -> EvalResult {
    let hash = category_set_hash(table)?;
    let Some(hash_ref) = hash.as_hash_table() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), hash],
        ));
    };
    let key = category_set.to_hash_key(&hash_ref.test);
    if let Some(existing) = hash_ref.key_snapshot(&key) {
        return Ok(*existing);
    }

    let _ = hash.with_hash_table_mut(|hash_table| {
        hash_table.insert(key, category_set, Value::NIL);
    });
    Ok(category_set)
}

fn category_set_bits(category_set: &Value) -> Option<u128> {
    match bool_vector_equal_hash_key(category_set)? {
        HashKey::BoolVec(parts) if parts.0 == 128 => Some(parts.1),
        _ => None,
    }
}

fn make_category_set_from_bits(bits: u128) -> Value {
    let mut values = Vec::with_capacity(130);
    values.push(Value::symbol("--bool-vector--"));
    values.push(Value::fixnum(128));
    for index in 0..128 {
        values.push(Value::fixnum(if bits & (1_u128 << index) == 0 {
            0
        } else {
            1
        }));
    }
    Value::vector(values)
}

fn intern_category_set_bits(table: Value, bits: u128) -> EvalResult {
    let hash = category_set_hash(table)?;
    let Some(hash_ref) = hash.as_hash_table() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), hash],
        ));
    };
    let key = HashKey::BoolVec(Box::new((128, bits)));
    if let Some(existing) = hash_ref.key_snapshot(&key) {
        return Ok(*existing);
    }

    let category_set = make_category_set_from_bits(bits);
    let _ = hash.with_hash_table_mut(|hash_table| {
        hash_table.insert(key, category_set, Value::NIL);
    });
    Ok(category_set)
}

fn category_set_with_member(
    table: Value,
    existing: Value,
    category: char,
    present: bool,
) -> EvalResult {
    if let Some(bits) = category_set_bits(&existing) {
        let bit = 1_u128 << (category as u32);
        let updated_bits = if present { bits | bit } else { bits & !bit };
        return intern_category_set_bits(table, updated_bits);
    }

    let updated = clone_vector_value(&existing)?;
    set_category_set_member(&updated, category, present)?;
    intern_category_set(table, updated)
}

fn is_category_table_value(value: &Value) -> Result<bool, Flow> {
    let is_char_table = super::chartable::builtin_char_table_p(vec![*value])?;
    if !is_char_table.is_truthy() {
        return Ok(false);
    }
    let subtype = super::chartable::builtin_char_table_subtype(vec![*value])?;
    Ok(subtype.is_symbol_named("category-table"))
}

fn make_category_table_object() -> EvalResult {
    let default = make_empty_category_set()?;
    let table = super::chartable::make_char_table_with_extra_slots(
        Value::symbol("category-table"),
        default,
        2,
    );
    super::chartable::builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(CATEGORY_DOCSTRING_SLOT),
        Value::vector(vec![Value::NIL; CATEGORY_DOCSTRING_COUNT]),
    ])?;
    super::chartable::builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(CATEGORY_SET_HASH_SLOT),
        Value::NIL,
    ])?;
    Ok(table)
}

pub(crate) fn ensure_standard_category_table_object() -> EvalResult {
    STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| {
        if let Some(table) = slot.borrow().as_ref() {
            return Ok(*table);
        }

        let table = make_category_table_object()?;
        *slot.borrow_mut() = Some(table);
        Ok(table)
    })
}

fn deep_copy_category_table(source: &Value) -> EvalResult {
    if !is_category_table_value(source)? {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("category-table-p"), *source],
        ));
    }

    // GNU `copy_category_table` starts from `copy_char_table`, which
    // duplicates the char-table structure before copying the category
    // payloads. Building a fresh category table object here avoids
    // aliasing the original table's nested chartable vectors.
    let copy = make_category_table_object()?;
    let default = super::chartable::builtin_char_table_range(vec![*source, Value::NIL], None)?;
    if default.is_vector() {
        super::chartable::builtin_set_char_table_range(
            vec![copy, Value::NIL, clone_vector_value(&default)?],
            None,
        )?;
    }

    let docstrings = super::chartable::builtin_char_table_extra_slot(vec![
        *source,
        Value::fixnum(CATEGORY_DOCSTRING_SLOT),
    ])?;
    super::chartable::builtin_set_char_table_extra_slot(vec![
        copy,
        Value::fixnum(CATEGORY_DOCSTRING_SLOT),
        clone_vector_value(&docstrings)?,
    ])?;
    let set_hash = super::chartable::builtin_char_table_extra_slot(vec![
        *source,
        Value::fixnum(CATEGORY_SET_HASH_SLOT),
    ])?;
    super::chartable::builtin_set_char_table_extra_slot(vec![
        copy,
        Value::fixnum(CATEGORY_SET_HASH_SLOT),
        set_hash,
    ])?;

    for (key, value) in super::chartable::char_table_local_entries(source)? {
        let copied = if value.is_vector() {
            clone_vector_value(&value)?
        } else {
            value
        };
        super::chartable::builtin_set_char_table_range(vec![copy, key, copied], None)?;
    }

    Ok(copy)
}

fn category_doc_index(category: char) -> usize {
    (category as usize) - (CATEGORY_MIN as usize)
}

fn category_docstrings(table: Value) -> Result<Value, Flow> {
    super::chartable::builtin_char_table_extra_slot(vec![
        table,
        Value::fixnum(CATEGORY_DOCSTRING_SLOT),
    ])
}

fn category_docstring_in_table(table: Value, category: char) -> Result<Value, Flow> {
    let docs = category_docstrings(table)?;
    if !docs.is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), docs],
        ));
    };
    let docs = docs.as_vector_data().unwrap().clone();
    Ok(docs
        .get(category_doc_index(category))
        .copied()
        .unwrap_or(Value::NIL))
}

fn set_category_docstring_in_table(
    table: Value,
    category: char,
    docstring: Value,
) -> Result<(), Flow> {
    let docs = category_docstrings(table)?;
    if !docs.is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), docs],
        ));
    };
    let idx = category_doc_index(category);
    if docs.as_vector_data().is_some_and(|vec| idx < vec.len()) {
        let _ = docs.set_vector_slot(idx, docstring);
    }
    Ok(())
}

fn current_buffer_category_table_in_buffers(
    buffers: &mut crate::buffer::BufferManager,
) -> Result<Value, Flow> {
    use crate::buffer::buffer::BUFFER_SLOT_CATEGORY_TABLE;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Mirrors GNU `Fcategory_table` (`category.c:184-189`):
    //     return BVAR (current_buffer, category_table);
    let table = buf.slots[BUFFER_SLOT_CATEGORY_TABLE.index()];
    if !table.is_nil() {
        return Ok(table);
    }

    // Slot unset: seed from the standard category table —
    // matches GNU `reset_buffer` cloning the standard table into
    // a fresh buffer.
    let fallback = ensure_standard_category_table_object()?;
    buf.slots[BUFFER_SLOT_CATEGORY_TABLE.index()] = fallback;
    Ok(fallback)
}

fn check_category_table_in_buffers(
    buffers: &mut crate::buffer::BufferManager,
    table: Option<Value>,
) -> Result<Value, Flow> {
    match table {
        None => current_buffer_category_table_in_buffers(buffers),
        Some(t) if t.is_nil() => current_buffer_category_table_in_buffers(buffers),
        Some(table) => {
            if !is_category_table_value(&table)? {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("category-table-p"), table],
                ));
            }
            Ok(table)
        }
    }
}

fn check_category_table(
    eval: &mut super::eval::Context,
    table: Option<Value>,
) -> Result<Value, Flow> {
    check_category_table_in_buffers(&mut eval.buffers, table)
}

fn set_current_buffer_category_table_in_buffers(
    buffers: &mut crate::buffer::BufferManager,
    table: Value,
) -> Result<(), Flow> {
    use crate::buffer::buffer::BUFFER_SLOT_CATEGORY_TABLE;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // Mirrors GNU `Fset_category_table` (`category.c:262-275`):
    //     bset_category_table (current_buffer, table);
    //     SET_PER_BUFFER_VALUE_P (current_buffer,
    //                             PER_BUFFER_VAR_IDX (category_table), 1);
    buf.slots[BUFFER_SLOT_CATEGORY_TABLE.index()] = table;
    buf.set_slot_local_flag(BUFFER_SLOT_CATEGORY_TABLE, true);
    Ok(())
}

fn category_set_contains(category_set: &Value, category: char) -> Result<bool, Flow> {
    if !category_set.is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("categorysetp"), *category_set],
        ));
    };
    let vec = category_set.as_vector_data().unwrap();
    let bit_idx = 2 + (category as usize);
    Ok(vec
        .get(bit_idx)
        .and_then(|v| v.as_fixnum())
        .is_some_and(|n| n != 0))
}

pub(crate) fn char_has_category_in_table(
    table: Value,
    ch: char,
    category: u8,
) -> Result<bool, Flow> {
    let category_set = super::chartable::ct_lookup(&table, ch as i64)?;
    category_set_contains(&category_set, category as char)
}

pub(crate) fn active_category_table_for_buffer(
    buffer: Option<&crate::buffer::Buffer>,
) -> Result<Value, Flow> {
    if let Some(buffer) = buffer {
        let table = buffer.slots[crate::buffer::buffer::BUFFER_SLOT_CATEGORY_TABLE.index()];
        if !table.is_nil() {
            return Ok(table);
        }
    }
    ensure_standard_category_table_object()
}

fn set_category_set_member(
    category_set: &Value,
    category: char,
    present: bool,
) -> Result<(), Flow> {
    if !category_set.is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("categorysetp"), *category_set],
        ));
    };
    let bit_idx = 2 + (category as usize);
    if category_set
        .as_vector_data()
        .is_some_and(|vec| bit_idx < vec.len())
    {
        let _ = category_set.set_vector_slot(bit_idx, Value::fixnum(if present { 1 } else { 0 }));
    }
    Ok(())
}

pub(crate) fn builtin_category_table_p(args: Vec<Value>) -> EvalResult {
    expect_args("category-table-p", &args, 1)?;
    Ok(Value::bool_val(is_category_table_value(&args[0])?))
}

pub(crate) fn builtin_make_category_table(args: Vec<Value>) -> EvalResult {
    expect_max_args("make-category-table", &args, 0)?;
    make_category_table_object()
}

pub(crate) fn builtin_copy_category_table(args: Vec<Value>) -> EvalResult {
    expect_max_args("copy-category-table", &args, 1)?;

    let source = match args.first() {
        None => ensure_standard_category_table_object()?,
        Some(t) if t.is_nil() => ensure_standard_category_table_object()?,
        Some(table) => {
            if !is_category_table_value(table)? {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("category-table-p"), *table],
                ));
            }
            *table
        }
    };

    deep_copy_category_table(&source)
}

pub(crate) fn builtin_make_category_set(args: Vec<Value>) -> EvalResult {
    expect_args("make-category-set", &args, 1)?;

    let Some(string) = args[0].as_lisp_string() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    };

    // GNU's `Fmake_category_set` rejects multibyte strings outright, then walks
    // the raw bytes of the unibyte string, validating each with `CHECK_CATEGORY`.
    if string.is_multibyte() {
        return Err(signal(
            "error",
            vec![Value::string("Multibyte string in ‘make-category-set’")],
        ));
    }

    let mut bits = vec![Value::fixnum(0); 128];
    for &byte in string.as_bytes() {
        let category = check_category(&Value::fixnum(i64::from(byte)))?;
        bits[category as usize] = Value::fixnum(1);
    }

    let mut vec = Vec::with_capacity(130);
    vec.push(Value::symbol("--bool-vector--"));
    vec.push(Value::fixnum(128));
    vec.extend(bits);
    Ok(Value::vector(vec))
}

pub(crate) fn builtin_category_set_mnemonics(args: Vec<Value>) -> EvalResult {
    expect_args("category-set-mnemonics", &args, 1)?;

    if !&args[0].is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("categorysetp"), args[0]],
        ));
    };

    let bits = args[0].as_vector_data().unwrap();
    let valid_shape =
        bits.len() >= 130 && bits[0].is_symbol_named("--bool-vector--") && bits[1].is_fixnum();
    if !valid_shape {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("categorysetp"), args[0]],
        ));
    }

    let mut out = String::new();
    for idx in CATEGORY_MIN as usize..=CATEGORY_MAX as usize {
        let is_set = match bits.get(2 + idx) {
            None => false,
            Some(v) if v.is_nil() => false,
            Some(v) if v.as_fixnum() == Some(0) => false,
            _ => true,
        };
        if is_set {
            out.push(idx as u8 as char);
        }
    }

    Ok(Value::string(&out))
}

pub(crate) fn builtin_modify_category_entry(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("modify-category-entry", &args, 2)?;
    expect_max_args("modify-category-entry", &args, 4)?;

    let category = extract_char(&args[1], "modify-category-entry")?;
    if !is_category_letter(category) {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Invalid category character '{}': must be 0x20..0x7E",
                category
            ))],
        ));
    }

    let table = check_category_table(eval, args.get(2).copied())?;
    if category_docstring_in_table(table, category)?.is_nil() {
        return Err(signal(
            "error",
            vec![Value::string(format!("Undefined category: {}", category))],
        ));
    }
    let reset = args.get(3).is_some_and(|v| v.is_truthy());

    let (start, end) = match args[0].kind() {
        ValueKind::Cons => {
            let car = args[0].cons_car();
            let cdr = args[0].cons_cdr();
            (
                extract_char_code(&car, "modify-category-entry")?,
                extract_char_code(&cdr, "modify-category-entry")?,
            )
        }
        _ => {
            let ch = extract_char_code(&args[0], "modify-category-entry")?;
            (ch, ch)
        }
    };

    if start > end {
        return Ok(Value::NIL);
    }

    if start == end && (0..=crate::emacs_core::emacs_char::MAX_CHAR as i64).contains(&start) {
        let existing = super::chartable::ct_lookup(&table, start)?;
        let has_category = category_set_contains(&existing, category)?;
        if has_category == reset {
            let updated = category_set_with_member(table, existing, category, !reset)?;
            super::chartable::builtin_set_char_table_range(
                vec![table, Value::fixnum(start), updated],
                None,
            )?;
        }
        return Ok(Value::NIL);
    }

    for (existing, cursor, to) in
        super::chartable::char_table_atomic_runs_in_range(&table, start, end)?
    {
        let has_category = category_set_contains(&existing, category)?;
        if has_category == reset {
            let updated = category_set_with_member(table, existing, category, !reset)?;
            let chunk_end = to.min(end);
            let key = if cursor == chunk_end {
                Value::fixnum(cursor)
            } else {
                Value::cons(Value::fixnum(cursor), Value::fixnum(chunk_end))
            };
            super::chartable::builtin_set_char_table_range(vec![table, key, updated], None)?;
        }
    }

    Ok(Value::NIL)
}

pub(crate) fn builtin_define_category(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("define-category", &args, 2)?;
    expect_max_args("define-category", &args, 3)?;

    let category_code = check_category(&args[0])?;
    let category = char::from_u32(category_code as u32).expect("category in ASCII graphic range");
    let docstring = match args[1].kind() {
        ValueKind::String => args[1],
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[1]],
            ));
        }
    };
    let table = check_category_table(eval, args.get(2).copied())?;
    if !category_docstring_in_table(table, category)?.is_nil() {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Category ‘{}’ is already defined",
                category
            ))],
        ));
    }

    set_category_docstring_in_table(table, category, docstring)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_category_docstring(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("category-docstring", &args, 1)?;
    expect_max_args("category-docstring", &args, 2)?;

    let category = extract_char(&args[0], "category-docstring")?;
    let table = check_category_table(eval, args.get(1).copied())?;
    category_docstring_in_table(table, category)
}

pub(crate) fn builtin_get_unused_category(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("get-unused-category", &args, 1)?;

    let table = check_category_table(eval, args.first().copied())?;
    for code in CATEGORY_MIN..=CATEGORY_MAX {
        let category = char::from_u32(code as u32).expect("ASCII category code");
        if category_docstring_in_table(table, category)?.is_nil() {
            return Ok(Value::char(category));
        }
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_char_category_set(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("char-category-set", &args, 1)?;
    let _ = extract_char_code(&args[0], "char-category-set")?;
    let table = current_buffer_category_table_in_buffers(&mut eval.buffers)?;
    super::chartable::builtin_char_table_range(vec![table, args[0]], None)
}

pub(crate) fn builtin_category_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_category_table_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_category_table_in_buffers(
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("category-table", &args, 0)?;
    current_buffer_category_table_in_buffers(buffers)
}

pub(crate) fn builtin_standard_category_table(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("standard-category-table", &args, 0)?;
    ensure_standard_category_table_object()
}

pub(crate) fn builtin_set_category_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_category_table_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_set_category_table_in_buffers(
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-category-table", &args, 1)?;

    let installed = check_category_table_in_buffers(buffers, args.first().copied())?;
    set_current_buffer_category_table_in_buffers(buffers, installed)?;
    Ok(installed)
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
