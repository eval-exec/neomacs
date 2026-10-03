use super::*;
use crate::emacs_core::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};
use crate::emacs_core::eval::LispArgVec;
use crate::emacs_core::hashtab::hash_key_to_visible_value;
use crate::emacs_core::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};
use crate::emacs_core::value::{HashProbe, HashTableMakeKeyword, ValueKind, VecLikeType};

// ===========================================================================
// Vector operations
// ===========================================================================

pub(crate) fn builtin_make_vector(args: Vec<Value>) -> EvalResult {
    expect_args("make-vector", &args, 2)?;
    let len = expect_wholenump(&args[0])? as usize;
    Ok(Value::vector(vec![args[1]; len]))
}

pub(crate) fn builtin_vector_slice(_eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    Ok(Value::vector(args.to_vec()))
}

pub(crate) fn builtin_aref(args: Vec<Value>) -> EvalResult {
    expect_args("aref", &args, 2)?;
    builtin_aref_values(args[0], args[1])
}

pub(crate) fn builtin_aref_2(
    _eval: &mut super::eval::Context,
    array: Value,
    index: Value,
) -> EvalResult {
    builtin_aref_values(array, index)
}

pub(crate) fn builtin_aref_values(array: Value, index: Value) -> EvalResult {
    let idx_fixnum = expect_fixnum(&index)?;
    match array.kind() {
        ValueKind::Veclike(VecLikeType::CharTable) => {
            let ch = expect_char_table_index(&index)?;
            super::chartable::ct_lookup(&array, ch)
        }
        // A vector or record slot: no in-band tags (P3.2 L0.8), so slot 0
        // is just slot 0, as in GNU `Faref`. A negative index wraps to a huge
        // one and is out of range.
        ValueKind::Veclike(VecLikeType::Vector | VecLikeType::Record) => {
            let items = array
                .as_vector_data()
                .or_else(|| array.as_record_data())
                .unwrap();
            items
                .get(idx_fixnum as usize)
                .copied()
                .ok_or_else(|| signal(LispCondition::ArgsOutOfRange, vec![array, index]))
        }
        // GNU `Faref`: a bool-vector's bit as t/nil; a negative index wraps
        // to a huge one and is out of range.
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            super::boolvec::bool_vector_ref_value(&array, idx_fixnum as usize)
                .ok_or_else(|| signal(LispCondition::ArgsOutOfRange, vec![array, index]))
        }
        ValueKind::String => {
            let idx = idx_fixnum as usize;
            super::lisp_string_value_char_at(array, idx)
                .map(|cp| Value::fixnum(cp as i64))
                .ok_or_else(|| signal(LispCondition::ArgsOutOfRange, vec![array, index]))
        }
        // In official Emacs, closures support aref for oclosure slot access.
        // The closure vector layout is:
        //   [0]=ARGS  [1]=BODY  [2]=ENV  [3]=nil  [4]=DOCSTRING  [5]=IFORM
        ValueKind::Veclike(VecLikeType::Lambda) => {
            let idx = idx_fixnum as usize;
            let vec = lambda_to_closure_vector(&array);
            vec.get(idx)
                .cloned()
                .ok_or_else(|| signal(LispCondition::ArgsOutOfRange, vec![array, index]))
        }
        // ByteCode closures: [0]=ARGLIST [1]=CODE [2]=ENV/CONSTANTS [3]=DEPTH [4]=DOC
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            // Negative indices wrap to huge `usize`s: out of range, as GNU.
            bytecode_closure_slot(&array, idx_fixnum as usize)
                .ok_or_else(|| signal(LispCondition::ArgsOutOfRange, vec![array, index]))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("arrayp"), array],
        )),
    }
}

pub(crate) fn aset_string_replacement(
    array: &Value,
    index: &Value,
    new_element: &Value,
) -> Result<Value, Flow> {
    if !array.is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *array],
        ));
    };

    let idx_fixnum = expect_fixnum(index)?;
    let string = array.as_lisp_string().expect("string");
    if idx_fixnum < 0 || idx_fixnum as usize >= string.schars() {
        return Err(signal(LispCondition::ArgsOutOfRange, vec![*array, *index]));
    }
    let idx = idx_fixnum as usize;

    let replacement_code = insert_char_code_from_value(new_element)?;
    if !(0..=0x3F_FFFF).contains(&replacement_code) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *new_element],
        ));
    }
    let replacement_code = replacement_code as u32;
    if !string.is_multibyte() && replacement_code > 0xff {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to store non-byte value into unibyte string",
            )],
        ));
    }
    if string.is_multibyte() {
        if replacement_code > 0x7f {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Attempt to store non-ASCII char into multibyte string",
                )],
            ));
        }
        // GNU `Faset` locates the byte with `string_char_to_byte' (src/fns.c),
        // which returns the index unchanged when `SCHARS == SBYTES' and
        // otherwise scans from whichever END is nearer. `LispString` already
        // implements both; the bare-slice converter can implement neither,
        // because a `&[u8]` does not know SCHARS -- so reaching for it made
        // this O(index).
        let byte_pos = string.char_to_byte_pos(idx);
        if string.as_bytes()[byte_pos] > 0x7f {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Attempt to replace non-ASCII char in multibyte string",
                )],
            ));
        }
        array.set_string_byte_same_char_count(byte_pos, replacement_code as u8);
        return Ok(*array);
    }

    // GNU's unibyte `Faset` is one bounds check plus `SSET`.  Rebuilding the
    // complete string here made byte-at-a-time protocol transforms (for
    // example WebSocket masking) quadratic in the frame size.
    array.set_string_byte_same_char_count(idx, replacement_code as u8);
    Ok(*array)
}

/// `aset` for callers that own their argument vector (now only the unit
/// tests).
///
/// Delegates to the slice form: nothing in the body needs ownership, and the
/// VM's hot paths already hold their three arguments on the stack. Requiring a
/// `Vec` there cost a `SmallVec` clone plus a heap allocation per `aset`
/// (`dhrystone`: 370,708 clones and 305,131 `Vec::from_iter` calls).
#[cfg(test)]
pub(crate) fn builtin_aset(args: Vec<Value>) -> EvalResult {
    builtin_aset_args(&args)
}

/// `aset` reading its arguments in place.
pub(crate) fn builtin_aset_args(args: &[Value]) -> EvalResult {
    expect_args("aset", args, 3)?;
    // GNU src/data.c:Faset starts with CHECK_FIXNUM (idx) before checking
    // whether ARRAY is mutable by `aset`.
    let idx_fixnum = expect_fixnum(&args[1])?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::CharTable) => {
            let ch = expect_char_table_index(&args[1])?;
            super::chartable::builtin_set_char_table_range(
                vec![args[0], Value::fixnum(ch), args[2]],
                None,
            )
        }
        // A vector or record slot (no in-band tags, P3.2 L0.8).
        ValueKind::Veclike(kind @ (VecLikeType::Vector | VecLikeType::Record)) => {
            let idx = idx_fixnum as usize;
            let len = args[0]
                .as_vector_data()
                .or_else(|| args[0].as_record_data())
                .unwrap()
                .len();
            if idx >= len {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![args[0], args[1]],
                ));
            }
            if kind == VecLikeType::Vector {
                args[0].set_vector_slot(idx, args[2]);
            } else {
                args[0].set_record_slot(idx, args[2]);
            }
            Ok(args[2])
        }
        // GNU `Faset`: any non-nil VALUE stores 1.
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            if super::boolvec::bool_vector_set(&args[0], idx_fixnum as usize, args[2].is_truthy()) {
                Ok(args[2])
            } else {
                Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![args[0], args[1]],
                ))
            }
        }
        ValueKind::String => {
            let _updated = aset_string_replacement(&args[0], &args[1], &args[2])?;
            Ok(args[2])
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("arrayp"), args[0]],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_vconcat(args: Vec<Value>) -> EvalResult {
    builtin_vconcat_slice(&args)
}

pub(crate) fn builtin_vconcat_slice(args: &[Value]) -> EvalResult {
    let mut result = Vec::new();
    for arg in args {
        match arg.kind() {
            ValueKind::Veclike(VecLikeType::BoolVector) => {
                result.extend(super::boolvec::bool_vector_elements(arg).unwrap_or_default());
            }
            ValueKind::Veclike(VecLikeType::CharTable) => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), *arg],
                ));
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                result.extend(arg.as_vector_data().unwrap().clone())
            }
            ValueKind::String => {
                let string = arg.as_lisp_string().expect("string");
                super::for_each_lisp_string_char(string, |cp| {
                    result.push(Value::fixnum(cp as i64));
                });
            }
            ValueKind::Nil => {}
            ValueKind::Cons => result.extend(super::cons_list::collect_proper_list_items(*arg)?),
            ValueKind::Veclike(VecLikeType::Lambda) => result.extend(lambda_to_closure_vector(arg)),
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                result.extend(bytecode_to_closure_vector(arg))
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), *arg],
                ));
            }
        }
    }
    Ok(Value::vector(result))
}

// ===========================================================================
// Hash table operations
// ===========================================================================

thread_local! {
    static HASH_TABLE_TEST_ALIASES: HeapRegistrySlot<HashTableTestRegistry> =
        HeapRegistrySlot::new(HashTableTestRegistry::default());
}

/// A Context owns this registry and exclusively lends it to its mutator. The
/// existing thread-local slot selects that owner; it does not own callback
/// state. Callback accounting belongs to the Context's GC settings cache.
#[derive(Default)]
pub(crate) struct HashTableTestRegistry {
    aliases: HashMap<String, HashTableTestAlias>,
}

/// Entry operands for one synchronous, GC-inhibited user-test activation.
/// The owning mutator pins this on its stack and its Context selects it;
/// independent mutators never share it. Copied Lisp setting words stay alive
/// under GC inhibition. No table or setting references cross Lisp callbacks.
#[derive(Clone, Copy)]
pub(crate) struct HashTestGcInhibitAccounting {
    pub(crate) bytes_at_start: usize,
    pub(crate) charged_bytes_at_start: usize,
    pub(crate) collector_threshold_at_start: usize,
    /// Resolved only when GC-maybe first needs the saved entry operands.
    pub(crate) threshold_at_start: Option<std::num::NonZeroUsize>,
    pub(crate) threshold_setting: Value,
    pub(crate) percentage_setting: Value,
    pub(crate) threshold_overridden: bool,
    pub(crate) memory_full: bool,
    pub(crate) startup_ceiling: bool,
}

#[derive(Clone)]
pub(crate) struct HashTableTestAlias {
    pub(crate) standard_test: Option<HashTableTest>,
    pub(crate) user_cmp_function: Option<Value>,
    pub(crate) user_hash_function: Option<Value>,
}

pub(super) fn reset_collections_thread_locals() {
    HASH_TABLE_TEST_ALIASES.with(|slot| slot.reset(HashTableTestRegistry::default()));
}

/// Root the custom comparison/hash closures registered via
/// `define-hash-table-test`. They live in the Context-owned registry selected by this thread, so
/// without rooting them the GC sweeps a still-referenced closure and the next
/// custom-test `gethash`/`puthash` calls a freed function (use-after-free).
pub(crate) fn collect_hash_table_test_registry_gc_roots(
    registry: &HashTableTestRegistryHandle,
    group: &mut Vec<Value>,
) {
    for alias in registry.borrow().aliases.values() {
        if let Some(f) = alias.user_cmp_function {
            group.push(f);
        }
        if let Some(f) = alias.user_hash_function {
            group.push(f);
        }
    }
}

pub(crate) type HashTableTestRegistryHandle = HeapRegistryHandle<HashTableTestRegistry>;

pub(crate) fn current_hash_table_test_registry_handle() -> HashTableTestRegistryHandle {
    HASH_TABLE_TEST_ALIASES.with(HeapRegistrySlot::current)
}

pub(crate) fn install_hash_table_test_registry_handle(handle: &HashTableTestRegistryHandle) {
    HASH_TABLE_TEST_ALIASES.with(|slot| slot.install(handle));
}

#[cfg(test)]
pub(crate) fn collect_hash_table_test_alias_gc_roots(group: &mut Vec<Value>) {
    collect_hash_table_test_registry_gc_roots(&current_hash_table_test_registry_handle(), group);
}

fn invalid_hash_table_keyword_argument(arg: Value) -> Flow {
    signal(
        "error",
        vec![Value::string("Invalid keyword argument"), arg],
    )
}

fn hash_test_from_designator(value: &Value) -> Option<HashTableTest> {
    HashTableTest::from_symbol_value(value)
}

fn hash_test_from_user_test_pair(test: &Value, hash: &Value) -> Option<HashTableTest> {
    let test_name = test.as_symbol_name()?;
    let hash_name = hash.as_symbol_name()?;
    match (test_name, hash_name) {
        ("eq", "sxhash-eq") => Some(HashTableTest::Eq),
        ("eql", "sxhash-eql") => Some(HashTableTest::Eql),
        ("equal", "sxhash-equal") => Some(HashTableTest::Equal),
        _ => None,
    }
}

fn register_hash_table_test_alias(name: &str, alias: HashTableTestAlias) {
    HASH_TABLE_TEST_ALIASES.with(|slot| slot.borrow_mut().aliases.insert(name.to_string(), alias));
}

pub(crate) fn lookup_hash_table_test_alias(name: &str) -> Option<HashTableTestAlias> {
    HASH_TABLE_TEST_ALIASES.with(|slot| slot.borrow().aliases.get(name).cloned())
}

fn maybe_resize_hash_table_for_insert(table: &mut LispHashTable, inserting_new_key: bool) {
    if !inserting_new_key {
        return;
    }
    let current_size = usize::try_from(table.size.max(0)).unwrap_or(usize::MAX);
    if table.data.len() < current_size {
        return;
    }

    // Match Emacs growth policy: zero-sized tables grow to 6 slots on first
    // insertion; small tables then grow by 4x (up to size 64), larger tables
    // grow by 2x.
    let min_size = 6_i64;
    let base = table.size.max(min_size).min(i64::MAX / 2);
    table.size = if table.size == 0 {
        min_size
    } else if base <= 64 {
        base.saturating_mul(4)
    } else {
        base.saturating_mul(2)
    };
    // `size` is the GNU-visible logical allocation size, not a requirement to
    // reserve every Rust-side index to the same capacity. A single Lisp entry
    // is mirrored across several maps/vectors; eagerly reserving all of them at
    // each logical growth boundary multiplies otherwise-unused capacity. Let
    // each backing collection grow with the entries actually inserted.
}

pub(crate) fn builtin_define_hash_table_test(args: Vec<Value>) -> EvalResult {
    expect_args("define-hash-table-test", &args, 3)?;
    let Some(alias_name) = args[0].as_symbol_name() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };
    let standard_test = hash_test_from_user_test_pair(&args[1], &args[2])
        .or_else(|| hash_test_from_designator(&args[1]));
    // GNU chooses built-in tests only from make-hash-table's :test NAME.
    // A custom NAME always retains both callbacks, even when its comparison
    // designator is eq, eql, or equal. Keep the storage-test classification;
    // user callbacks select the Lisp path independently of that classification.
    let user_test = super::super::hashtab::hash_test_parity_enabled() || standard_test.is_none();
    register_hash_table_test_alias(
        alias_name,
        HashTableTestAlias {
            standard_test,
            user_cmp_function: user_test.then_some(args[1]),
            user_hash_function: user_test.then_some(args[2]),
        },
    );
    Ok(Value::list(vec![args[1], args[2]]))
}

pub(crate) fn builtin_make_hash_table(args: Vec<Value>) -> EvalResult {
    builtin_make_hash_table_slice(&args)
}

pub(crate) fn builtin_make_hash_table_slice(args: &[Value]) -> EvalResult {
    if !args.len().is_multiple_of(2) {
        return Err(signal(
            "error",
            vec![Value::string("Odd number of arguments")],
        ));
    }

    let mut test_arg = Value::NIL;
    let mut weakness_arg = Value::NIL;
    let mut size_arg = Value::NIL;

    let mut i = args.len();
    while i >= 2 {
        i -= 1;
        let arg = args[i];
        i -= 1;
        let kw = args[i];
        match HashTableMakeKeyword::from_symbol_value(&kw) {
            Some(HashTableMakeKeyword::Test) => test_arg = arg,
            Some(HashTableMakeKeyword::Weakness) => weakness_arg = arg,
            Some(HashTableMakeKeyword::Size) => size_arg = arg,
            Some(
                HashTableMakeKeyword::RehashThreshold
                | HashTableMakeKeyword::RehashSize
                | HashTableMakeKeyword::Purecopy,
            ) => {}
            None => return Err(invalid_hash_table_keyword_argument(kw)),
        }
    }

    let (test, test_name) = match test_arg.kind() {
        ValueKind::Nil => (HashTableTest::Eql, None),
        _ => {
            let Some(name) = test_arg.as_symbol_name() else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), test_arg],
                ));
            };
            let test = match HashTableTest::from_symbol_name(name) {
                Some(test)
                    if !super::super::hashtab::hash_test_parity_enabled()
                        || test_arg.as_symbol_id() == Some(intern(test.name())) =>
                {
                    test
                }
                _ => {
                    if let Some(alias) = lookup_hash_table_test_alias(name) {
                        alias.standard_test.unwrap_or(HashTableTest::Equal)
                    } else {
                        return Err(signal(
                            "error",
                            vec![Value::string("Invalid hash table test"), test_arg],
                        ));
                    }
                }
            };
            let test_name = if super::super::hashtab::hash_test_parity_enabled() {
                test_arg.as_symbol_id()
            } else {
                Some(intern(name))
            };
            (test, test_name)
        }
    };

    let size = match size_arg.kind() {
        ValueKind::Nil => 0,
        ValueKind::Fixnum(n) if n >= 0 => n,
        _ => {
            return Err(signal(
                "error",
                vec![Value::string("Invalid hash table size"), size_arg],
            ));
        }
    };

    let weakness = match weakness_arg.kind() {
        ValueKind::Nil => None,
        ValueKind::T => Some(HashTableWeakness::KeyAndValue),
        _ => {
            let Some(name) = weakness_arg.as_symbol_name() else {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid hash table weakness"), weakness_arg],
                ));
            };
            match HashTableWeakness::from_symbol_name(name) {
                Some(weakness) => Some(weakness),
                None => {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid hash table weakness"), weakness_arg],
                    ));
                }
            }
        }
    };

    let table = Value::hash_table_with_options(test, size, weakness, 1.5, 0.8125);
    if table.is_hash_table() {
        let _ = table.with_hash_table_mut(|ht| {
            ht.test_name = test_name;
            if let Some(name_id) = test_name
                && !(super::super::hashtab::hash_test_parity_enabled()
                    && HashTableTest::from_symbol_name(resolve_sym(name_id))
                        .is_some_and(|test| name_id == intern(test.name())))
                && let Some(alias) = lookup_hash_table_test_alias(resolve_sym(name_id))
            {
                ht.user_cmp_function = alias.user_cmp_function;
                ht.user_hash_function = alias.user_hash_function;
            }
        });
    }
    Ok(table)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gethash(args: Vec<Value>) -> EvalResult {
    builtin_gethash_with_symbols(args, false)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gethash_with_symbols(
    args: Vec<Value>,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    expect_min_args("gethash", &args, 2)?;
    let default = args.get(2).copied().unwrap_or(Value::NIL);
    builtin_gethash_values(args[0], args[1], default, symbols_with_pos_enabled)
}

pub(crate) fn builtin_gethash_3(
    eval: &mut super::eval::Context,
    key_value: Value,
    table: Value,
    default: Value,
) -> EvalResult {
    if let Some(result) = builtin_gethash_user_defined(eval, key_value, table, default)? {
        return Ok(result);
    }
    builtin_gethash_values(key_value, table, default, eval.symbols_with_pos_enabled)
}

fn table_user_defined_test(table: &LispHashTable) -> Option<(Value, Value)> {
    Some((table.user_cmp_function?, table.user_hash_function?))
}

/// Whether `table` is a hash table whose test runs Lisp (made with
/// `define-hash-table-test`): exactly the tables `gethash`/`puthash` send
/// through their user-defined path before the plain lookup.
pub(crate) fn hash_table_has_user_test(table: Value) -> bool {
    table
        .as_hash_table()
        .is_some_and(|ht| table_user_defined_test(ht).is_some())
}

fn check_mutable_hash_table(table: Value) -> Result<(), Flow> {
    if table.as_hash_table().is_some_and(|ht| !ht.mutable) {
        return Err(signal(
            "error",
            vec![Value::string("hash table test modifies table"), table],
        ));
    }
    Ok(())
}

fn hash_table_user_defined_call(
    eval: &mut super::eval::Context,
    table: Value,
    function: Value,
    args: impl Into<LispArgVec>,
) -> EvalResult {
    if super::super::hashtab::hash_test_parity_enabled() {
        return super::super::hashtab::with_user_test_guard(eval, table, |eval| {
            eval.apply(function, args)
        });
    }
    if table.as_hash_table().is_some_and(|ht| !ht.mutable) {
        return eval.apply(function, args);
    }

    let _ = table.with_hash_table_mut(|ht| ht.mutable = false);
    let result = eval.apply(function, args);
    let _ = table.with_hash_table_mut(|ht| ht.mutable = true);
    result
}

fn hash_table_user_hash(
    eval: &mut super::eval::Context,
    table: Value,
    hash_function: Value,
    key: Value,
) -> EvalResult {
    let mut args = LispArgVec::new();
    args.push(key);
    let hash = hash_table_user_defined_call(eval, table, hash_function, args)?;
    Ok(match hash.kind() {
        ValueKind::Fixnum(n) => Value::fixnum(n),
        _ => Value::fixnum(super::super::hashtab::sxhash_for(
            &hash,
            HashTableTest::Equal,
        )),
    })
}

fn hash_table_user_keys_equal(
    eval: &mut super::eval::Context,
    table: Value,
    cmp_function: Value,
    a: Value,
    b: Value,
) -> EvalResult {
    let mut args = LispArgVec::new();
    args.push(a);
    args.push(b);
    hash_table_user_defined_call(eval, table, cmp_function, args)
}

/// The user-defined hash of a key already stored in TABLE, asking the user's
/// function only the first time.
///
/// GNU keeps every key's hash in the table and compares integers; ours had to
/// call the user's Lisp hash function once per candidate, so a lookup cost one
/// Lisp call per entry and grew with the table. Recording the answer makes a
/// lookup one call plus integer compares.
fn hash_table_stored_user_hash(
    eval: &mut super::eval::Context,
    table: Value,
    hash_function: Value,
    stored_key: &HashKey,
    candidate: Value,
) -> Result<Value, Flow> {
    if let Some(cached) = table
        .as_hash_table()
        .and_then(|ht| ht.data.user_hash(stored_key))
    {
        return Ok(Value::fixnum(cached));
    }
    let hash = hash_table_user_hash(eval, table, hash_function, candidate)?;
    if let Some(bits) = hash.as_fixnum() {
        let key = stored_key.clone();
        let _ = table.with_hash_table_mut(|ht| ht.data.set_user_hash(key, bits));
    }
    Ok(hash)
}

/// The stored keys whose remembered hash equals WANTED, or `None` when some
/// live key has never been hashed.
///
/// With every hash remembered, a lookup needs no snapshot of the table and no
/// clone of it: the keys that could match are found by comparing integers, and
/// only those few are handed to the user's equality function. The first
/// lookup after a key is stored still takes the slow path below, which is what
/// fills this in.
fn user_test_candidates_by_hash(table: Value, wanted: i64) -> (Vec<(HashKey, Value)>, bool) {
    let Some(ht) = table.as_hash_table() else {
        return (Vec::new(), true);
    };
    // GNU reaches the keys that hash alike through a bucket vector and a
    // next-chain (`hash_find_with_hash', fns.c:5086-5100) rather than by
    // walking the table, which is why its lookup is flat in the table size.
    // `user_candidates' is that bucket.
    //
    // The `contains_key' filter is deliberate belt-and-braces: every removal
    // path drops the memo, so a bucket should never name a dead key, but if
    // one ever did this turns the bug into a slower answer rather than a
    // wrong one.
    let candidates = ht
        .data
        .user_candidates(wanted)
        .iter()
        .filter(|key| ht.data.contains_key(key))
        .map(|key| (key.clone(), hash_key_to_visible_value(ht, key)))
        .collect();
    (candidates, ht.data.user_hash_incomplete())
}

fn builtin_gethash_user_defined(
    eval: &mut super::eval::Context,
    key_value: Value,
    table: Value,
    default: Value,
) -> Result<Option<Value>, Flow> {
    let ValueKind::Veclike(VecLikeType::HashTable) = table.kind() else {
        return Ok(None);
    };
    let ht_ref = table.as_hash_table().unwrap();
    let Some((cmp_function, hash_function)) = table_user_defined_test(ht_ref) else {
        return Ok(None);
    };
    let wanted = hash_table_user_hash(eval, table, hash_function, key_value)?;
    if let Some(wanted_bits) = wanted.as_fixnum() {
        // Whatever hashes are already remembered answer without cloning the
        // table or rooting a copy of every entry: only the keys that hash
        // alike reach the user's equality function. A key first seen here has
        // no remembered hash, so a miss among these falls through to the walk
        // below, which records one as it goes.
        let (candidates, unknown) = user_test_candidates_by_hash(table, wanted_bits);
        if !candidates.is_empty() {
            let root_scope = eval.save_specpdl_roots();
            for (_, candidate) in &candidates {
                eval.push_specpdl_root(*candidate);
            }
            let matched = (|| -> Result<Option<Value>, Flow> {
                for (key, candidate) in &candidates {
                    if hash_table_user_keys_equal(eval, table, cmp_function, key_value, *candidate)?
                        .is_truthy()
                    {
                        return Ok(table
                            .as_hash_table()
                            .and_then(|ht| ht.data.get(key).copied()));
                    }
                }
                Ok(None)
            })();
            eval.restore_specpdl_roots(root_scope);
            let matched = matched?;
            if matched.is_some() || !unknown {
                return Ok(Some(matched.unwrap_or(default)));
            }
        } else if !unknown {
            return Ok(Some(default));
        }
    }

    let ht = table.as_hash_table().unwrap().clone();
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(hash_snapshot_root_holder(&ht));
    let result = (|| -> Result<Option<Value>, Flow> {
        let wanted_hash = wanted;
        for key in ht.live_hash_keys_in_slot_order() {
            if !ht.data.contains_key(key) {
                continue;
            }
            let candidate = hash_key_to_visible_value(&ht, key);
            if hash_table_stored_user_hash(eval, table, hash_function, key, candidate)?
                != wanted_hash
            {
                continue;
            }
            if hash_table_user_keys_equal(eval, table, cmp_function, key_value, candidate)?
                .is_truthy()
            {
                return Ok(Some(ht.data.get(key).copied().unwrap_or(default)));
            }
        }
        Ok(Some(default))
    })();
    eval.restore_specpdl_roots(root_scope);
    result
}

/// Thread every live (visible-key . value) of a hash-table snapshot onto one
/// heap list: a SINGLE root keeps the whole snapshot alive while user-defined
/// hash/equality functions run arbitrary Lisp that may remhash entries from
/// the live (rooted) table — after which the snapshot's copies would be
/// unreachable and a GC would free them mid-iteration.
fn hash_snapshot_root_holder(ht: &LispHashTable) -> Value {
    let mut holder = Value::NIL;
    for key in ht.live_hash_keys_in_slot_order() {
        if let Some(value) = ht.data.get(key) {
            holder = Value::cons(
                hash_key_to_visible_value(ht, key),
                Value::cons(*value, holder),
            );
        }
    }
    holder
}

pub(crate) fn builtin_gethash_values(
    key_value: Value,
    table: Value,
    default: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    match table.kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let ht = table.as_hash_table().unwrap();
            Ok(ht
                .data
                .try_lookup(key_value, ht.test, symbols_with_pos_enabled)?
                .cloned()
                .unwrap_or(default))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), table],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_puthash(args: Vec<Value>) -> EvalResult {
    builtin_puthash_with_symbols(args, false)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_puthash_with_symbols(
    args: Vec<Value>,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    expect_args("puthash", &args, 3)?;
    builtin_puthash_values(args[0], args[1], args[2], symbols_with_pos_enabled)
}

pub(crate) fn builtin_puthash_3(
    eval: &mut super::eval::Context,
    key_value: Value,
    value: Value,
    table: Value,
) -> EvalResult {
    if builtin_puthash_user_defined(eval, key_value, value, table)?.is_some() {
        return Ok(value);
    }
    builtin_puthash_values(key_value, value, table, eval.symbols_with_pos_enabled)
}

fn builtin_puthash_user_defined(
    eval: &mut super::eval::Context,
    key_value: Value,
    value: Value,
    table: Value,
) -> Result<Option<()>, Flow> {
    let ValueKind::Veclike(VecLikeType::HashTable) = table.kind() else {
        return Ok(None);
    };
    let ht_ref = table.as_hash_table().unwrap();
    check_mutable_hash_table(table)?;
    let Some((cmp_function, hash_function)) = table_user_defined_test(ht_ref) else {
        return Ok(None);
    };
    let wanted = hash_table_user_hash(eval, table, hash_function, key_value)?;
    // As in the lookup: the hashes already remembered decide which stored keys
    // can match, without cloning the table or rooting a copy of every entry.
    if let Some(wanted_bits) = wanted.as_fixnum() {
        let (candidates, unknown) = user_test_candidates_by_hash(table, wanted_bits);
        let root_scope = eval.save_specpdl_roots();
        for (_, candidate) in &candidates {
            eval.push_specpdl_root(*candidate);
        }
        let matched = (|| -> Result<Option<HashKey>, Flow> {
            for (key, candidate) in &candidates {
                if hash_table_user_keys_equal(eval, table, cmp_function, key_value, *candidate)?
                    .is_truthy()
                {
                    return Ok(Some(key.clone()));
                }
            }
            Ok(None)
        })();
        eval.restore_specpdl_roots(root_scope);
        let matched = matched?;
        if matched.is_some() || !unknown {
            let storage_key = matched.unwrap_or_else(|| key_value.to_hash_key(&HashTableTest::Eq));
            let _ = table.with_hash_table_mut(|ht| {
                if let Some(slot) = ht.data.get_mut(&storage_key) {
                    *slot = value;
                } else {
                    maybe_resize_hash_table_for_insert(ht, true);
                    ht.insert(storage_key.clone(), key_value, value);
                }
                // Remember the new key's hash, so a later lookup finds it
                // without asking the user's function again.
                ht.data.set_user_hash(storage_key, wanted_bits);
            });
            return Ok(Some(()));
        }
    }

    let ht_snapshot = table.as_hash_table().unwrap().clone();
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(hash_snapshot_root_holder(&ht_snapshot));
    let existing_key = (|| -> Result<Option<HashKey>, Flow> {
        let wanted_hash = wanted;
        for key in ht_snapshot.live_hash_keys_in_slot_order() {
            if !ht_snapshot.data.contains_key(key) {
                continue;
            }
            let candidate = hash_key_to_visible_value(&ht_snapshot, key);
            if hash_table_stored_user_hash(eval, table, hash_function, key, candidate)?
                != wanted_hash
            {
                continue;
            }
            if hash_table_user_keys_equal(eval, table, cmp_function, key_value, candidate)?
                .is_truthy()
            {
                return Ok(Some(key.clone()));
            }
        }
        Ok(None)
    })();
    eval.restore_specpdl_roots(root_scope);
    let existing_key = existing_key?;

    let storage_key = existing_key.unwrap_or_else(|| key_value.to_hash_key(&HashTableTest::Eq));
    let _ = table.with_hash_table_mut(|ht| {
        if let Some(slot) = ht.data.get_mut(&storage_key) {
            *slot = value;
        } else {
            maybe_resize_hash_table_for_insert(ht, true);
            ht.insert(storage_key.clone(), key_value, value);
        }
        // Record here too, not only on the fast arm above. `unknown' is
        // derived from "every live key has a remembered hash", so a single
        // insert that skipped this would make every later lookup take the
        // full walk for the rest of the table's life.
        if let Some(wanted_bits) = wanted.as_fixnum() {
            ht.data.set_user_hash(storage_key, wanted_bits);
        }
    });
    Ok(Some(()))
}

fn builtin_puthash_values(
    key_value: Value,
    value: Value,
    table: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    match table.kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            check_mutable_hash_table(table)?;
            let test = table.as_hash_table().unwrap().test;
            table
                .with_hash_table_mut(|ht| -> Result<(), Flow> {
                    match ht
                        .data
                        .try_probe_for_insert(key_value, test, symbols_with_pos_enabled)?
                    {
                        HashProbe::Found(slot) => {
                            if let Some(stored) = ht.data.slot_value_mut(slot) {
                                *stored = value;
                            }
                        }
                        HashProbe::Absent(hash) => {
                            maybe_resize_hash_table_for_insert(ht, true);
                            let key = key_value.to_hash_key_swp(&test, symbols_with_pos_enabled);
                            ht.data.insert_absent(hash, key, key_value, value);
                        }
                    }
                    Ok(())
                })
                .unwrap_or(Ok(()))?;
            Ok(value)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), table],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_remhash(args: Vec<Value>) -> EvalResult {
    builtin_remhash_with_symbols(args, false)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_remhash_with_symbols(
    args: Vec<Value>,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    expect_args("remhash", &args, 2)?;
    builtin_remhash_values(args[0], args[1], symbols_with_pos_enabled)
}

pub(crate) fn builtin_remhash_2(
    eval: &mut super::eval::Context,
    key_value: Value,
    table: Value,
) -> EvalResult {
    if builtin_remhash_user_defined(eval, key_value, table)?.is_some() {
        return Ok(Value::NIL);
    }
    builtin_remhash_values(key_value, table, eval.symbols_with_pos_enabled)
}

fn builtin_remhash_user_defined(
    eval: &mut super::eval::Context,
    key_value: Value,
    table: Value,
) -> Result<Option<()>, Flow> {
    let ValueKind::Veclike(VecLikeType::HashTable) = table.kind() else {
        return Ok(None);
    };
    let ht_ref = table.as_hash_table().unwrap();
    let Some((cmp_function, hash_function)) = table_user_defined_test(ht_ref) else {
        return Ok(None);
    };
    check_mutable_hash_table(table)?;
    // Same fast arm as `gethash'/`puthash': the remembered hashes pick the
    // candidates, so only the keys that hash alike reach the user's equality
    // function and the table is neither cloned nor snapshot-rooted. Without
    // this, `remhash' walked and re-hashed every entry -- it was the most
    // expensive of the three by an order of magnitude.
    let wanted = hash_table_user_hash(eval, table, hash_function, key_value)?;
    if let Some(wanted_bits) = wanted.as_fixnum() {
        let (candidates, unknown) = user_test_candidates_by_hash(table, wanted_bits);
        let root_scope = eval.save_specpdl_roots();
        for (_, candidate) in &candidates {
            eval.push_specpdl_root(*candidate);
        }
        let matched = (|| -> Result<Option<HashKey>, Flow> {
            for (key, candidate) in &candidates {
                if hash_table_user_keys_equal(eval, table, cmp_function, key_value, *candidate)?
                    .is_truthy()
                {
                    return Ok(Some(key.clone()));
                }
            }
            Ok(None)
        })();
        eval.restore_specpdl_roots(root_scope);
        if let Some(storage_key) = matched? {
            let _ = table.with_hash_table_mut(|ht| {
                let _ = ht.data.remove(&storage_key);
            });
            return Ok(Some(()));
        }
        if !unknown {
            return Ok(Some(()));
        }
    }

    let ht_snapshot = table.as_hash_table().unwrap().clone();
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(hash_snapshot_root_holder(&ht_snapshot));
    let existing_key = (|| -> Result<Option<HashKey>, Flow> {
        let wanted_hash = wanted;
        let mut existing_key = None;
        for key in ht_snapshot.live_hash_keys_in_slot_order() {
            if !ht_snapshot.data.contains_key(key) {
                continue;
            }
            let candidate = hash_key_to_visible_value(&ht_snapshot, key);
            if hash_table_stored_user_hash(eval, table, hash_function, key, candidate)?
                != wanted_hash
            {
                continue;
            }
            if hash_table_user_keys_equal(eval, table, cmp_function, key_value, candidate)?
                .is_truthy()
            {
                existing_key = Some(key.clone());
                break;
            }
        }
        Ok(existing_key)
    })();
    eval.restore_specpdl_roots(root_scope);
    let existing_key = existing_key?;

    if let Some(storage_key) = existing_key {
        let _ = table.with_hash_table_mut(|ht| {
            let _ = ht.data.remove(&storage_key);
        });
    }
    Ok(Some(()))
}

pub(crate) fn builtin_remhash_values(
    key_value: Value,
    table: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    match table.kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            check_mutable_hash_table(table)?;
            let test = table.as_hash_table().unwrap().test;
            table
                .with_hash_table_mut(|ht| {
                    ht.data
                        .try_remove_by_value(key_value, test, symbols_with_pos_enabled)
                        .map(|_| ())
                })
                .unwrap_or(Ok(()))?;
            Ok(Value::NIL)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), table],
        )),
    }
}

pub(crate) fn builtin_clrhash(args: Vec<Value>) -> EvalResult {
    expect_args("clrhash", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            check_mutable_hash_table(args[0])?;
            let _ = args[0].with_hash_table_mut(|ht| {
                ht.data.clear();
            });
            // Be compatible with GNU Emacs (and XEmacs): return the table.
            Ok(args[0])
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

pub(crate) fn builtin_hash_table_count(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-count", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => Ok(Value::fixnum(
            args[0].as_hash_table().unwrap().data.len() as i64,
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

pub(crate) fn builtin_char_to_string(args: Vec<Value>) -> EvalResult {
    expect_args("char-to-string", &args, 1)?;
    let code = expect_character_code(&args[0])? as u32;
    if code <= 0x7f {
        // ASCII → unibyte
        Ok(Value::heap_string(
            crate::heap_types::LispString::from_unibyte(vec![code as u8]),
        ))
    } else {
        // GNU Fchar_to_string uses CHAR_STRING followed by
        // make_string_from_bytes.  Non-ASCII Unicode, extended Emacs
        // characters, and raw-byte characters therefore all produce
        // multibyte strings containing Emacs-internal bytes.
        let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(buf[..len].to_vec()),
        ))
    }
}

pub(crate) fn builtin_string_to_char(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-char", &args, 1)?;
    let string = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    // The first character only (GNU `Fstring_to_char': `STRING_CHAR' of the
    // data); decoding the whole string made this O(length).
    let first = super::lisp_string_char_at(string, 0).unwrap_or(0);
    Ok(Value::fixnum(first as i64))
}

// ===========================================================================
// Property lists
// ===========================================================================

pub(crate) fn builtin_plist_get(args: Vec<Value>) -> EvalResult {
    builtin_plist_get_eq_swp(args, false)
}

/// `plist-get` taking its arguments as a SLICE, so a call allocates nothing.
///
/// The default native subr shape is `SubrFn::Many`, whose dispatch arm does
/// `args[args_start..args_start + nargs].to_vec()` -- a heap allocation on
/// EVERY call, purely to hand over the arguments. GNU has no equivalent: its
/// subrs receive `Lisp_Object`s directly, or a `Lisp_Object *` for MANY, and
/// nothing conses to make a call.
///
/// `plist-get` is the single hottest builtin in org editing -- 57373 calls,
/// 12.87% of all builtin calls, in a 40-iteration screenful loop -- because
/// org-element stores node properties in plists. Every one of those was a
/// `Vec` allocation for two arguments, feeding the GC cost that shows up as
/// ~8-9% of org (jemalloc alone 3.3%).
///
/// `SubrFn::ManySlice` already existed and is already wired into the
/// dispatcher (`apply`, `funcall`, `sort`, `string-match` use it); this just
/// opts `plist-get` into it. The rare PREDICATE form still needs an owned
/// `Vec` because the predicate can run Lisp that mutates the list mid-walk, so
/// it delegates to the existing entry point.
/// `plist-get` as a three-slot subr (an omitted PREDICATE arrives as nil).
pub(crate) fn builtin_plist_get_3(
    eval: &mut super::eval::Context,
    plist: Value,
    prop: Value,
    predicate: Value,
) -> EvalResult {
    if predicate.is_nil() {
        return Ok(crate::emacs_core::plist::plist_get_swp(
            plist,
            &prop,
            eval.symbols_with_pos_enabled,
        )
        .unwrap_or(Value::NIL));
    }
    builtin_plist_get_with_ctx(eval, vec![plist, prop, predicate])
}

pub(crate) fn builtin_plist_get_with_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("plist-get", &args, 2)?;
    expect_max_args("plist-get", &args, 3)?;
    if args.get(2).is_none_or(|value| value.is_nil()) {
        return builtin_plist_get_eq_swp(args, eval.symbols_with_pos_enabled);
    }

    let plist = args[0];
    let prop = args[1];
    let predicate = args[2];
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(plist);
    eval.push_specpdl_root(prop);
    eval.push_specpdl_root(predicate);

    // The predicate can setcdr the plist mid-walk, unlinking the interior
    // cells this cursor still points at; root the moving cursor in one
    // updatable slot so the remainder stays alive transitively (the GNU
    // equivalent survives via conservative C-stack scanning of the tail).
    // The cycle tortoise is compared by identity, so it is rooted too.
    let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
    let tortoise_slot = eval.push_specpdl_root_slot(Value::NIL);
    let mut cursor = plist;
    let mut cycle = crate::emacs_core::plist::TailCycleCheck::new(cursor);
    let plist_result = loop {
        match cursor.kind() {
            ValueKind::Cons => {
                eval.set_specpdl_root_slot(&cursor_slot, cursor);
                eval.set_specpdl_root_slot(&tortoise_slot, cycle.tortoise());
                if !cursor.cons_cdr().is_cons() {
                    break Ok(Value::NIL);
                }
                match eval.apply2(predicate, cursor.cons_car(), prop) {
                    Ok(value) => {
                        // GNU re-reads XCDR (tail) after the call: the
                        // predicate may have replaced the value cell. GNU
                        // dereferences a non-cons here; plist-get never
                        // signals, so end the walk.
                        let pair_cdr = cursor.cons_cdr();
                        if !pair_cdr.is_cons() {
                            break Ok(Value::NIL);
                        }
                        if value.is_truthy() {
                            break Ok(pair_cdr.cons_car());
                        }
                        cursor = pair_cdr.cons_cdr();
                        // FOR_EACH_TAIL_SAFE: a cycle just ends the walk.
                        if cycle.step(cursor).is_some() {
                            break Ok(Value::NIL);
                        }
                    }
                    Err(err) => break Err(err),
                }
            }
            _ => break Ok(Value::NIL),
        }
    };

    eval.restore_specpdl_roots(roots);
    plist_result
}

fn builtin_plist_get_eq_swp(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_min_args("plist-get", &args, 2)?;
    expect_max_args("plist-get", &args, 3)?;
    if args.get(2).is_some_and(|value| !value.is_nil()) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[2]],
        ));
    }
    Ok(
        crate::emacs_core::plist::plist_get_swp(args[0], &args[1], symbols_with_pos_enabled)
            .unwrap_or(Value::NIL),
    )
}

pub(crate) fn builtin_plist_put(args: Vec<Value>) -> EvalResult {
    builtin_plist_put_eq_swp(args, false)
}

pub(crate) fn builtin_plist_put_with_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("plist-put", &args, 3)?;
    expect_max_args("plist-put", &args, 4)?;
    if args.get(3).is_none_or(|value| value.is_nil()) {
        return builtin_plist_put_eq_swp(args, eval.symbols_with_pos_enabled);
    }

    let plist = args[0];
    let key = args[1];
    let new_val = args[2];
    let predicate = args[3];
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(plist);
    eval.push_specpdl_root(key);
    eval.push_specpdl_root(new_val);
    eval.push_specpdl_root(predicate);

    // Root the moving cursor and the trailing prev cell across the
    // predicate calls (see plist_get above); prev is written back to on
    // the append path, so a freed prev would be a write to swept memory.
    let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
    let prev_slot = eval.push_specpdl_root_slot(Value::NIL);
    let tortoise_slot = eval.push_specpdl_root_slot(Value::NIL);
    let mut cursor = plist;
    let mut prev = Value::NIL;
    let mut cycle = crate::emacs_core::plist::TailCycleCheck::new(cursor);
    let plist_result = loop {
        match cursor.kind() {
            ValueKind::Cons => {
                eval.set_specpdl_root_slot(&cursor_slot, cursor);
                eval.set_specpdl_root_slot(&prev_slot, prev);
                eval.set_specpdl_root_slot(&tortoise_slot, cycle.tortoise());
                if !cursor.cons_cdr().is_cons() {
                    break Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("plistp"), plist],
                    ));
                }

                match eval.apply2(predicate, cursor.cons_car(), key) {
                    Ok(value) => {
                        // GNU re-reads XCDR (tail) after the call: the
                        // predicate may have replaced the value cell.
                        let entry_rest = cursor.cons_cdr();
                        if value.is_truthy() {
                            // Fsetcar (XCDR (tail), val) checks the cell.
                            if !entry_rest.is_cons() {
                                break Err(signal(
                                    LispCondition::WrongTypeArgument,
                                    vec![Value::symbol("consp"), entry_rest],
                                ));
                            }
                            entry_rest.set_car(new_val);
                            break Ok(plist);
                        }
                        // GNU dereferences a non-cons here; treat it as the
                        // dotted tail it now is.
                        if !entry_rest.is_cons() {
                            break Err(signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("plistp"), plist],
                            ));
                        }
                        prev = cursor;
                        cursor = entry_rest.cons_cdr();
                        if let Some(cycle_tail) = cycle.step(cursor) {
                            break Err(signal(LispCondition::CircularList, vec![cycle_tail]));
                        }
                    }
                    Err(err) => break Err(err),
                }
            }
            ValueKind::Nil => {
                let new_cell = Value::cons(key, Value::cons(new_val, Value::NIL));
                if prev.is_nil() {
                    break Ok(new_cell);
                }
                prev.cons_cdr().set_cdr(new_cell);
                break Ok(plist);
            }
            _ => {
                break Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("plistp"), plist],
                ));
            }
        }
    };

    eval.restore_specpdl_roots(roots);
    plist_result
}

fn builtin_plist_put_eq_swp(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_min_args("plist-put", &args, 3)?;
    expect_max_args("plist-put", &args, 4)?;
    if args.get(3).is_some_and(|value| !value.is_nil()) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[3]],
        ));
    }
    let plist = args[0];
    let key = args[1];
    let new_val = args[2];

    if plist.is_nil() {
        return Ok(Value::list(vec![key, new_val]));
    }

    let mut cursor = plist;
    let mut last_value_cell: Option<Value> = None;
    let mut cycle = crate::emacs_core::plist::TailCycleCheck::new(plist);

    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                let entry_key = cursor.cons_car();
                let entry_rest = cursor.cons_cdr();

                match entry_rest.kind() {
                    ValueKind::Cons => {
                        if eq_value_swp(&entry_key, &key, symbols_with_pos_enabled) {
                            entry_rest.set_car(new_val);
                            return Ok(plist);
                        }
                        let value_cell = entry_rest;
                        cursor = entry_rest.cons_cdr();
                        last_value_cell = Some(value_cell);
                        // GNU plist_put walks with FOR_EACH_TAIL.
                        if let Some(cycle_tail) = cycle.step(cursor) {
                            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
                        }
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("plistp"), plist],
                        ));
                    }
                }
            }
            ValueKind::Nil => {
                if let Some(value_cell) = last_value_cell {
                    let new_tail = Value::cons(key, Value::cons(new_val, Value::NIL));
                    value_cell.set_cdr(new_tail);
                    return Ok(plist);
                }
                return Ok(Value::list(vec![key, new_val]));
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("plistp"), plist],
                ));
            }
        }
    }
}

pub(crate) fn builtin_plist_member(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let predicate = args
        .get(2)
        .and_then(|value| if value.is_nil() { None } else { Some(*value) });
    if predicate.is_none() {
        return plist_member_eq_swp(args, eval.symbols_with_pos_enabled);
    }

    expect_args_range("plist-member", &args, 2, 3)?;
    let plist = args[0];
    let prop = args[1];

    // Root Values that survive across eval.apply() in the loop.
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(plist);
    eval.push_specpdl_root(prop);
    if let Some(p) = predicate {
        eval.push_specpdl_root(p);
    }

    // Root the moving cursor and the cycle tortoise across the predicate
    // calls (see plist_get).
    let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
    let tortoise_slot = eval.push_specpdl_root_slot(Value::NIL);
    let mut cursor = plist;
    let mut cycle = crate::emacs_core::plist::TailCycleCheck::new(cursor);
    let plist_result = loop {
        match cursor.kind() {
            ValueKind::Cons => {
                eval.set_specpdl_root_slot(&cursor_slot, cursor);
                eval.set_specpdl_root_slot(&tortoise_slot, cycle.tortoise());
                let entry_key = cursor.cons_car();

                let matches = if let Some(predicate) = &predicate {
                    match eval.apply2(*predicate, entry_key, prop) {
                        Ok(v) => v.is_truthy(),
                        Err(e) => {
                            break Err(e);
                        }
                    }
                } else {
                    eq_value(&entry_key, &prop)
                };
                if matches {
                    break Ok(cursor);
                }

                // See `plist_member_eq` for the nil-terminator
                // rule: an unpaired last key is a valid end per
                // GNU, only dotted tails signal plistp. GNU reads
                // XCDR (tail) after the predicate call.
                let entry_rest = cursor.cons_cdr();
                match entry_rest.kind() {
                    ValueKind::Cons => {
                        cursor = entry_rest.cons_cdr();
                        if let Some(cycle_tail) = cycle.step(cursor) {
                            break Err(signal(LispCondition::CircularList, vec![cycle_tail]));
                        }
                    }
                    ValueKind::Nil => {
                        break Ok(Value::NIL);
                    }
                    _ => {
                        break Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("plistp"), plist],
                        ));
                    }
                }
            }
            ValueKind::Nil => break Ok(Value::NIL),
            _ => {
                break Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("plistp"), plist],
                ));
            }
        }
    };
    eval.restore_specpdl_roots(roots);
    plist_result
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn plist_member_eq(args: Vec<Value>) -> EvalResult {
    plist_member_eq_swp(args, false)
}

pub(crate) fn plist_member_eq_swp(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args_range("plist-member", &args, 2, 3)?;
    let plist = args[0];
    let prop = args[1];
    if args.get(2).is_some_and(|value| !value.is_nil()) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[2]],
        ));
    }

    // Mirrors GNU's `Fplist_member` / `plist_member_eq` (fns.c). Walks
    // the plist two elements at a time looking for PROP. A nil tail at
    // any step ends the walk cleanly and returns nil (not-found),
    // matching GNU `FOR_EACH_TAIL`'s implicit break on non-cons. Only a
    // non-nil improper tail (dotted list) signals `plistp`. GNU walks with
    // FOR_EACH_TAIL, so a circular plist signals `circular-list`.
    let mut cursor = plist;
    let mut cycle = crate::emacs_core::plist::TailCycleCheck::new(plist);
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                let entry_key = cursor.cons_car();
                let entry_rest = cursor.cons_cdr();

                if eq_value_swp(&entry_key, &prop, symbols_with_pos_enabled) {
                    return Ok(cursor);
                }

                match entry_rest.kind() {
                    ValueKind::Cons => {
                        cursor = entry_rest.cons_cdr();
                        if let Some(cycle_tail) = cycle.step(cursor) {
                            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
                        }
                    }
                    ValueKind::Nil => {
                        // Unpaired last key: valid end of plist per
                        // GNU; return not-found.
                        return Ok(Value::NIL);
                    }
                    _ => {
                        // Dotted tail after a key: malformed plist.
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("plistp"), plist],
                        ));
                    }
                }
            }
            ValueKind::Nil => return Ok(Value::NIL),
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("plistp"), plist],
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/aset_string_in_place.rs"]
mod aset_string_in_place_test;

#[cfg(test)]
#[path = "tests/gc_tls_collections.rs"]
mod gc_tls_ownership_tests;
