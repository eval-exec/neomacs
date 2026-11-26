//! Positioned symbols hash as their underlying symbol only in the owning
//! Context's transparent mode (GNU fns.c sxhash_obj / maybe_remove_pos).
use super::*;
use std::hash::{Hash, Hasher};

fn positioned(symbol: Value, pos: i64) -> Value {
    unsafe { with_tagged_heap(|heap| heap.alloc_symbol_with_pos(symbol, Value::fixnum(pos))) }
}

#[test]
fn sxhash_positioned_builtin_modes_are_explicit() {
    crate::test_utils::init_test_tracing();
    let symbol = Value::symbol("sxhash-positioned-builtin");
    let first = positioned(symbol, 3);
    let second = positioned(symbol, 9);
    let hashes: [fn(Vec<Value>, SymbolMode) -> EvalResult; 4] = [
        builtin_sxhash_eq_with_mode,
        builtin_sxhash_eql_with_mode,
        builtin_sxhash_equal_with_mode,
        builtin_sxhash_equal_including_properties_with_mode,
    ];
    for hash in hashes {
        let bare = hash(vec![symbol], SymbolMode::Positioned).unwrap();
        assert_eq!(hash(vec![first], SymbolMode::Positioned).unwrap(), bare);
        assert_eq!(hash(vec![second], SymbolMode::Positioned).unwrap(), bare);
        // Bare-mode SWPs remain opaque objects; do not globally strip positions.
        assert_eq!(
            hash(vec![first], SymbolMode::Bare).unwrap(),
            hash(vec![first], SymbolMode::Bare).unwrap()
        );
        assert!(hash(vec![], SymbolMode::Positioned).is_err());
    }
}

#[test]
fn sxhash_positioned_recurses_through_lists_vectors_records_and_fallback() {
    crate::test_utils::init_test_tracing();
    let symbol = Value::symbol("sxhash-positioned-nested");
    let first = positioned(symbol, 3);
    let second = positioned(symbol, 9);
    let shapes: [fn(Value) -> Value; 3] = [
        |v| Value::list(vec![Value::fixnum(7), Value::cons(v, Value::NIL)]),
        |v| Value::vector(vec![Value::fixnum(7), v]),
        |v| Value::make_record(vec![Value::symbol("sxhash-positioned-record"), v]),
    ];
    for shape in shapes {
        let plain = shape(symbol);
        let a = shape(first);
        let b = shape(second);
        for value in [a, b] {
            assert_eq!(
                sxhash_for(&value, HashTableTest::Equal, SymbolMode::Positioned),
                sxhash_for(&plain, HashTableTest::Equal, SymbolMode::Positioned)
            );
            // Exercise the recursive fallback independently of the depth-limited
            // GNU-compatible dispatcher that normally handles these sequences.
            assert_eq!(
                fallback_sxhash_emacs_uint(&value, HashTableTest::Equal, SymbolMode::Positioned),
                fallback_sxhash_emacs_uint(&plain, HashTableTest::Equal, SymbolMode::Positioned)
            );
        }
    }
}

#[test]
fn treesit_node_sxhash_keeps_private_slot_hash_direct_nested_and_fallback() {
    crate::test_utils::init_test_tracing();
    let parser = Value::make_record(vec![Value::symbol("treesit-parser"), Value::fixnum(7)]);
    let slots = vec![Value::symbol("treesit-node"), Value::fixnum(19), parser];
    let old_record = Value::make_record(slots.clone());
    let first = Value::make_treesit_node(slots.clone());
    let second = Value::make_treesit_node(slots);
    assert_ne!(first.bits(), second.bits());
    let shapes: [fn(Value) -> Value; 3] = [
        |v| v,
        |v| Value::vector(vec![Value::fixnum(7), v]),
        |v| Value::list(vec![v, Value::fixnum(7)]),
    ];
    for mode in [SymbolMode::Bare, SymbolMode::Positioned] {
        for shape in shapes {
            let old = shape(old_record);
            let a = shape(first);
            let b = shape(second);
            for value in [a, b] {
                assert_eq!(
                    sxhash_for(&value, HashTableTest::Equal, mode),
                    sxhash_for(&old, HashTableTest::Equal, mode),
                );
                // Check the independent legacy fallback as well as the Lisp
                // entry point's normal depth-limited structural dispatcher.
                assert_eq!(
                    fallback_sxhash_emacs_uint(&value, HashTableTest::Equal, mode),
                    fallback_sxhash_emacs_uint(&old, HashTableTest::Equal, mode),
                );
            }
        }
    }
}

fn symbol_name_fallback_hash(name: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    4_u8.hash(&mut hasher);
    name.hash(&mut hasher);
    hasher.finish() & SXHASH_FALLBACK_NONNEG_MASK
}

fn raw_symbol_name_fallback_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    4_u8.hash(&mut hasher);
    bytes.hash(&mut hasher);
    hasher.finish() & SXHASH_FALLBACK_NONNEG_MASK
}

/// A repeated equal-hash of a UTF-8 symbol must reuse the recorded `&str`.
/// `as_utf8_str` is `from_utf8` over the whole name, and
/// `resolve_sym_lisp_string` reads the registry only on a miss.
#[test]
fn repeated_equal_hash_of_a_utf8_symbol_skips_name_validation() {
    crate::test_utils::init_test_tracing();
    let symbol = Value::symbol("equal-hash-utf8-cache-probe");
    let expected = symbol_name_fallback_hash("equal-hash-utf8-cache-probe");
    assert_eq!(
        fallback_sxhash_emacs_uint(&symbol, HashTableTest::Equal, SymbolMode::Bare),
        expected
    );

    reset_symbol_equal_hash_name_validations();
    crate::intern::reset_resolve_sym_lisp_string_registry_reads();
    for _ in 0..64 {
        assert_eq!(
            fallback_sxhash_emacs_uint(&symbol, HashTableTest::Equal, SymbolMode::Bare),
            expected
        );
    }
    assert_eq!(symbol_equal_hash_name_validations(), 0);
    assert_eq!(crate::intern::resolve_sym_lisp_string_registry_reads(), 0);
}

/// A raw-byte name hashes its bytes. After the first record, repeats must
/// not scan those bytes with `from_utf8` or re-read the registry.
#[test]
fn repeated_equal_hash_of_a_raw_symbol_skips_name_validation() {
    crate::test_utils::init_test_tracing();
    let bytes = [0xFF, 0xFE, 0x80];
    let name = crate::heap_types::LispString::from_unibyte(bytes.to_vec());
    let id = crate::intern::intern_lisp_string(&name);
    let symbol = Value::from_sym_id(id);
    let expected = raw_symbol_name_fallback_hash(&bytes);
    assert_eq!(
        fallback_sxhash_emacs_uint(&symbol, HashTableTest::Equal, SymbolMode::Bare),
        expected
    );

    reset_symbol_equal_hash_name_validations();
    crate::intern::reset_resolve_sym_lisp_string_registry_reads();
    for _ in 0..64 {
        assert_eq!(
            fallback_sxhash_emacs_uint(&symbol, HashTableTest::Equal, SymbolMode::Bare),
            expected
        );
    }
    assert_eq!(symbol_equal_hash_name_validations(), 0);
    assert_eq!(crate::intern::resolve_sym_lisp_string_registry_reads(), 0);
}
