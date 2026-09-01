use super::*;
use crate::emacs_core::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};
use crate::emacs_core::eval::LispArgVec;
use crate::emacs_core::hashtab::hash_key_to_visible_value;
use crate::emacs_core::value::{HashTableMakeKeyword, ValueKind, VecLikeType};

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

fn builtin_aref_values(array: Value, index: Value) -> EvalResult {
    let idx_fixnum = expect_fixnum(&index)?;
    match array.kind() {
        ValueKind::Veclike(VecLikeType::CharTable) => {
            let ch = expect_char_table_index(&index)?;
            super::chartable::ct_lookup(&array, ch)
        }
        ValueKind::Veclike(VecLikeType::Vector) if super::chartable::is_char_table(&array) => {
            let ch = expect_char_table_index(&index)?;
            super::chartable::ct_lookup(&array, ch)
        }
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::Veclike(VecLikeType::Record) => {
            let idx = idx_fixnum as usize;
            let items = array
                .as_vector_data()
                .or_else(|| array.as_record_data())
                .unwrap();
            let is_bool_vector =
                items.len() >= 2 && items[0].as_symbol_name() == Some("--bool-vector--");
            if is_bool_vector {
                return super::chartable::bool_vector_ref_value(&array, idx)
                    .ok_or_else(|| signal(LispCondition::ArgsOutOfRange, vec![array, index]));
            }
            items
                .get(idx)
                .copied()
                .ok_or_else(|| signal(LispCondition::ArgsOutOfRange, vec![array, index]))
        }
        ValueKind::String => {
            let idx = idx_fixnum as usize;
            let string = array.as_lisp_string().expect("string");
            super::lisp_string_char_at(string, idx)
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
            let idx = idx_fixnum as usize;
            let vec = bytecode_to_closure_vector(&array);
            vec.get(idx)
                .cloned()
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
        let byte_pos = crate::emacs_core::emacs_char::char_to_byte_pos(string.as_bytes(), idx);
        if string.as_bytes()[byte_pos] > 0x7f {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Attempt to replace non-ASCII char in multibyte string",
                )],
            ));
        }
        let _ = array.with_lisp_string_mut(|s| {
            s.mutate_bytes(|data| {
                data[byte_pos] = replacement_code as u8;
            });
        });
        return Ok(*array);
    }

    // GNU's unibyte `Faset` is one bounds check plus `SSET`.  Rebuilding the
    // complete string here made byte-at-a-time protocol transforms (for
    // example WebSocket masking) quadratic in the frame size.
    let _ = array.with_lisp_string_mut(|s| {
        s.mutate_bytes(|data| {
            data[idx] = replacement_code as u8;
        });
    });
    Ok(*array)
}

pub(crate) fn builtin_aset(args: Vec<Value>) -> EvalResult {
    expect_args("aset", &args, 3)?;
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
        ValueKind::Veclike(VecLikeType::Vector) if super::chartable::is_char_table(&args[0]) => {
            let ch = expect_char_table_index(&args[1])?;
            super::chartable::builtin_set_char_table_range(
                vec![args[0], Value::fixnum(ch), args[2]],
                None,
            )
        }
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::Veclike(VecLikeType::Record) => {
            let idx = idx_fixnum as usize;
            let items = args[0]
                .as_vector_data()
                .or_else(|| args[0].as_record_data())
                .unwrap();
            let is_bool_vector =
                items.len() >= 2 && items[0].as_symbol_name() == Some("--bool-vector--");
            let bool_len = if is_bool_vector {
                match items.get(1).map(|v| v.kind()) {
                    Some(ValueKind::Fixnum(n)) if n >= 0 => Some(n as usize),
                    _ => None,
                }
            } else {
                None
            };
            let vec_len = items.len();
            if is_bool_vector {
                let len = match bool_len {
                    Some(n) => n,
                    None => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("bool-vector-p"), args[0]],
                        ));
                    }
                };
                if idx >= len {
                    return Err(signal(
                        LispCondition::ArgsOutOfRange,
                        vec![args[0], args[1]],
                    ));
                }
                let store_idx = idx + 2;
                if store_idx >= vec_len {
                    return Err(signal(
                        LispCondition::ArgsOutOfRange,
                        vec![args[0], args[1]],
                    ));
                }
                let val = Value::fixnum(if args[2].is_truthy() { 1 } else { 0 });
                match args[0].veclike_type() {
                    Some(VecLikeType::Vector) => {
                        args[0].set_vector_slot(store_idx, val);
                    }
                    Some(VecLikeType::Record) => {
                        args[0].set_record_slot(store_idx, val);
                    }
                    _ => unreachable!("vector/record path should only reach vectorlike arrays"),
                }
                return Ok(args[2]);
            }
            if idx >= vec_len {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![args[0], args[1]],
                ));
            }
            match args[0].veclike_type() {
                Some(VecLikeType::Vector) => {
                    args[0].set_vector_slot(idx, args[2]);
                }
                Some(VecLikeType::Record) => {
                    args[0].set_record_slot(idx, args[2]);
                }
                _ => unreachable!("vector/record path should only reach vectorlike arrays"),
            }
            Ok(args[2])
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
            ValueKind::Veclike(VecLikeType::Vector) if super::chartable::is_bool_vector(arg) => {
                let len = super::chartable::bool_vector_length(arg).unwrap_or_default();
                for index in 0..usize::try_from(len).unwrap_or_default() {
                    let bit =
                        super::chartable::bool_vector_ref_value(arg, index).ok_or_else(|| {
                            signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("bool-vector-p"), *arg],
                            )
                        })?;
                    result.push(bit);
                }
            }
            ValueKind::Veclike(VecLikeType::Vector) if super::chartable::is_char_table(arg) => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), *arg],
                ));
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
    static HASH_TABLE_TEST_ALIASES: RefCell<HashMap<String, HashTableTestAlias>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone)]
pub(crate) struct HashTableTestAlias {
    pub(crate) standard_test: Option<HashTableTest>,
    pub(crate) user_cmp_function: Option<Value>,
    pub(crate) user_hash_function: Option<Value>,
}

pub(super) fn reset_collections_thread_locals() {
    HASH_TABLE_TEST_ALIASES.with(|slot| slot.borrow_mut().clear());
}

/// Root the custom comparison/hash closures registered via
/// `define-hash-table-test`. They live only in this thread-local registry, so
/// without rooting them the GC sweeps a still-referenced closure and the next
/// custom-test `gethash`/`puthash` calls a freed function (use-after-free).
pub(crate) fn collect_hash_table_test_alias_gc_roots(group: &mut Vec<Value>) {
    HASH_TABLE_TEST_ALIASES.with(|slot| {
        for alias in slot.borrow().values() {
            if let Some(f) = alias.user_cmp_function {
                group.push(f);
            }
            if let Some(f) = alias.user_hash_function {
                group.push(f);
            }
        }
    });
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
    HASH_TABLE_TEST_ALIASES.with(|slot| slot.borrow_mut().insert(name.to_string(), alias));
}

pub(crate) fn lookup_hash_table_test_alias(name: &str) -> Option<HashTableTestAlias> {
    HASH_TABLE_TEST_ALIASES.with(|slot| slot.borrow().get(name).cloned())
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
    register_hash_table_test_alias(
        alias_name,
        HashTableTestAlias {
            standard_test,
            user_cmp_function: standard_test.is_none().then_some(args[1]),
            user_hash_function: standard_test.is_none().then_some(args[2]),
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
                Some(test) => test,
                None => {
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
            (test, Some(intern(name)))
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
    let ht = ht_ref.clone();
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(hash_snapshot_root_holder(&ht));
    let result = (|| -> Result<Option<Value>, Flow> {
        let wanted_hash = hash_table_user_hash(eval, table, hash_function, key_value)?;
        for key in ht.live_hash_keys_in_slot_order() {
            if !ht.data.contains_key(key) {
                continue;
            }
            let candidate = hash_key_to_visible_value(&ht, key);
            if hash_table_user_hash(eval, table, hash_function, candidate)? != wanted_hash {
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

fn builtin_gethash_values(
    key_value: Value,
    table: Value,
    default: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    match table.kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let ht = table.as_hash_table().unwrap();
            let key = key_value.to_hash_key_swp(&ht.test, symbols_with_pos_enabled);
            Ok(ht.data.get(&key).cloned().unwrap_or(default))
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
    let ht_snapshot = ht_ref.clone();
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(hash_snapshot_root_holder(&ht_snapshot));
    let existing_key = (|| -> Result<Option<HashKey>, Flow> {
        let wanted_hash = hash_table_user_hash(eval, table, hash_function, key_value)?;
        for key in ht_snapshot.live_hash_keys_in_slot_order() {
            if !ht_snapshot.data.contains_key(key) {
                continue;
            }
            let candidate = hash_key_to_visible_value(&ht_snapshot, key);
            if hash_table_user_hash(eval, table, hash_function, candidate)? != wanted_hash {
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
            ht.insert(storage_key, key_value, value);
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
            let key = key_value.to_hash_key_swp(&test, symbols_with_pos_enabled);
            let _ = table.with_hash_table_mut(|ht| {
                if let Some(slot) = ht.data.get_mut(&key) {
                    *slot = value;
                } else {
                    maybe_resize_hash_table_for_insert(ht, true);
                    ht.insert(key, key_value, value);
                }
            });
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
    let ht_snapshot = ht_ref.clone();
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(hash_snapshot_root_holder(&ht_snapshot));
    let existing_key = (|| -> Result<Option<HashKey>, Flow> {
        let wanted_hash = hash_table_user_hash(eval, table, hash_function, key_value)?;
        let mut existing_key = None;
        for key in ht_snapshot.live_hash_keys_in_slot_order() {
            if !ht_snapshot.data.contains_key(key) {
                continue;
            }
            let candidate = hash_key_to_visible_value(&ht_snapshot, key);
            if hash_table_user_hash(eval, table, hash_function, candidate)? != wanted_hash {
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
            let key = key_value.to_hash_key_swp(&test, symbols_with_pos_enabled);
            let _ = table.with_hash_table_mut(|ht| {
                let _ = ht.data.remove(&key);
            });
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
    let codes = super::lisp_string_char_codes(string);
    let first = codes.into_iter().next().unwrap_or(0);
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
pub(crate) fn builtin_plist_get_slice(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("plist-get", args, 2)?;
    expect_max_args("plist-get", args, 3)?;
    if args.get(2).is_none_or(|value| value.is_nil()) {
        return Ok(crate::emacs_core::plist::plist_get_swp(
            args[0],
            &args[1],
            eval.symbols_with_pos_enabled,
        )
        .unwrap_or(Value::NIL));
    }
    builtin_plist_get_with_ctx(eval, args.to_vec())
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
    let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
    let mut cursor = plist;
    let mut safe_tail = crate::emacs_core::plist::SafeTailGuard::new(cursor);
    let plist_result = loop {
        match cursor.kind() {
            ValueKind::Cons => {
                eval.set_specpdl_root_slot(&cursor_slot, cursor);
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if !pair_cdr.is_cons() {
                    break Ok(Value::NIL);
                }
                match eval.apply2(predicate, pair_car, prop) {
                    Ok(value) if value.is_truthy() => break Ok(pair_cdr.cons_car()),
                    Ok(_) => {
                        cursor = pair_cdr.cons_cdr();
                        if safe_tail.found_cycle_after_advance(cursor) {
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
    let mut cursor = plist;
    let mut prev = Value::NIL;
    let plist_result = loop {
        match cursor.kind() {
            ValueKind::Cons => {
                eval.set_specpdl_root_slot(&cursor_slot, cursor);
                eval.set_specpdl_root_slot(&prev_slot, prev);
                let entry_key = cursor.cons_car();
                let entry_rest = cursor.cons_cdr();
                if !entry_rest.is_cons() {
                    break Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("plistp"), plist],
                    ));
                }

                match eval.apply2(predicate, entry_key, key) {
                    Ok(value) if value.is_truthy() => {
                        entry_rest.set_car(new_val);
                        break Ok(plist);
                    }
                    Ok(_) => {
                        prev = cursor;
                        cursor = entry_rest.cons_cdr();
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

    // Root the moving cursor across the predicate calls (see plist_get).
    let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
    let mut cursor = plist;
    let plist_result = loop {
        match cursor.kind() {
            ValueKind::Cons => {
                eval.set_specpdl_root_slot(&cursor_slot, cursor);
                let entry_key = cursor.cons_car();
                let entry_rest = cursor.cons_cdr();

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
                // GNU, only dotted tails signal plistp.
                match entry_rest.kind() {
                    ValueKind::Cons => {
                        cursor = entry_rest.cons_cdr();
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
    // non-nil improper tail (dotted list) signals `plistp`.
    let mut cursor = plist;
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
