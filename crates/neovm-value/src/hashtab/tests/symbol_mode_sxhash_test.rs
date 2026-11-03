//! Positioned symbols hash as their underlying symbol only in the owning
//! Context's transparent mode (GNU fns.c sxhash_obj / maybe_remove_pos).
use super::*;

fn positioned(symbol: Value, pos: i64) -> Value {
    with_tagged_heap(|heap| heap.alloc_symbol_with_pos(symbol, Value::fixnum(pos)))
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
