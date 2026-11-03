//! Regression tests: `sxhash-eql` / `sxhash-equal` and `eql`/`equal` hash
//! tables must hash bignums by their *numeric value*, mirroring GNU
//! `sxhash_bignum` (sign seed + folded magnitude limbs), not by heap address.
//!
//! Before the fix, two numerically-equal bignums (distinct heap allocations)
//! produced different hashes (and different `eq`-pointer hash keys), so
//! `(= (sxhash-eql (expt 2 100)) (sxhash-eql (expt 2 100)))` was nil and a
//! `gethash` of an equal bignum key missed.  GNU returns `t` / the stored
//! value in every case below (verified with `emacs --batch`).

use super::*;
use crate::emacs_core::builtins::{builtin_gethash, builtin_make_hash_table, builtin_puthash};
use malachite::integer::Integer;

/// `2^n` as a *freshly allocated* heap bignum, so two calls produce distinct
/// objects (the whole point of the bug -- identity hashing would split them).
fn pow2(n: u32) -> Value {
    Value::bignum(Integer::from(1u64) << n)
}

fn make_table(test: &str) -> Value {
    builtin_make_hash_table(vec![Value::keyword("test"), Value::symbol(test)]).unwrap()
}

#[test]
fn sxhash_eql_of_equal_bignums_is_equal() {
    crate::test_utils::init_test_tracing();
    // GNU: (= (sxhash-eql (expt 2 100)) (sxhash-eql (expt 2 100))) => t
    let a = builtin_sxhash_eql(vec![pow2(100)]).unwrap();
    let b = builtin_sxhash_eql(vec![pow2(100)]).unwrap();
    assert_eq!(a, b);
}

#[test]
fn sxhash_equal_of_equal_bignums_is_equal() {
    crate::test_utils::init_test_tracing();
    let a = builtin_sxhash_equal(vec![pow2(100)]).unwrap();
    let b = builtin_sxhash_equal(vec![pow2(100)]).unwrap();
    assert_eq!(a, b);
}

#[test]
fn sxhash_equal_of_bignum_nested_in_list_is_equal() {
    crate::test_utils::init_test_tracing();
    // (= (sxhash-equal (list (expt 2 70))) (sxhash-equal (list (expt 2 70)))) => t
    let list_a = Value::list(vec![pow2(70)]);
    let list_b = Value::list(vec![pow2(70)]);
    let a = builtin_sxhash_equal(vec![list_a]).unwrap();
    let b = builtin_sxhash_equal(vec![list_b]).unwrap();
    assert_eq!(a, b);
}

#[test]
fn sxhash_eql_seeds_sign_so_negation_differs() {
    crate::test_utils::init_test_tracing();
    // GNU seeds with the sign bit, so a bignum and its negation hash
    // differently: (= (sxhash-eql (expt 2 100)) (sxhash-eql (- (expt 2 100)))) => nil
    let pos = builtin_sxhash_eql(vec![pow2(100)]).unwrap();
    let neg = builtin_sxhash_eql(vec![Value::bignum(-(Integer::from(1u64) << 100u32))]).unwrap();
    assert_ne!(pos, neg);
}

#[test]
fn eql_hash_table_finds_equal_bignum_key() {
    crate::test_utils::init_test_tracing();
    // GNU: puthash then gethash of an *equal* (distinct alloc) bignum => v
    let table = make_table("eql");
    builtin_puthash(vec![pow2(100), Value::symbol("v"), table]).unwrap();
    let got = builtin_gethash(vec![pow2(100), table]).unwrap();
    assert_eq!(got, Value::symbol("v"));
}

#[test]
fn eql_hash_table_finds_negative_bignum_key() {
    crate::test_utils::init_test_tracing();
    let table = make_table("eql");
    let key1 = Value::bignum(-(Integer::from(1u64) << 100u32));
    let key2 = Value::bignum(-(Integer::from(1u64) << 100u32));
    builtin_puthash(vec![key1, Value::symbol("n"), table]).unwrap();
    let got = builtin_gethash(vec![key2, table]).unwrap();
    assert_eq!(got, Value::symbol("n"));
}

#[test]
fn equal_hash_table_finds_bignum_nested_in_list_key() {
    crate::test_utils::init_test_tracing();
    // GNU: (puthash (list (expt 2 70)) 'x h) ; (gethash (list (expt 2 70)) h) => x
    let table = make_table("equal");
    builtin_puthash(vec![Value::list(vec![pow2(70)]), Value::symbol("x"), table]).unwrap();
    let got = builtin_gethash(vec![Value::list(vec![pow2(70)]), table]).unwrap();
    assert_eq!(got, Value::symbol("x"));
}

#[test]
fn eql_hash_table_distinguishes_different_bignum_keys() {
    crate::test_utils::init_test_tracing();
    // Distinct values must NOT collide; gethash of a different value misses.
    let table = make_table("eql");
    builtin_puthash(vec![pow2(100), Value::symbol("a"), table]).unwrap();
    let missing = builtin_gethash(vec![pow2(101), table]).unwrap();
    assert_eq!(missing, Value::NIL);
}

#[test]
fn sxhash_bignum_matches_gnu_oracle_fixnum_values() {
    crate::test_utils::init_test_tracing();
    // Exact GNU `emacs --batch` oracle values: the limb-folding must match
    // `sxhash_bignum` bit-for-bit, not merely be self-consistent.
    //   (sxhash-eql (expt 2 100))      => 85899345920
    //   (sxhash-equal (expt 2 70))     => 80
    //   (sxhash-eql (- (expt 2 100)))  => 85899346240
    assert_eq!(
        builtin_sxhash_eql(vec![pow2(100)]).unwrap(),
        Value::fixnum(85899345920)
    );
    assert_eq!(
        builtin_sxhash_equal(vec![pow2(70)]).unwrap(),
        Value::fixnum(80)
    );
    assert_eq!(
        builtin_sxhash_eql(vec![Value::bignum(-(Integer::from(1u64) << 100u32))]).unwrap(),
        Value::fixnum(85899346240)
    );
}

/// `sxhash-equal` of a record (any `cl-defstruct`/`record`) must return an
/// integer, not PANIC.  The equal-hash vector arm collapsed Vector+Record and
/// called `as_vector_data().unwrap()`, which is `None` for records -> process
/// abort.  GNU: `(integerp (sxhash-equal (record 'foo 1 2)))` => t.
#[test]
fn sxhash_equal_of_record_does_not_panic() {
    use crate::emacs_core::builtins::symbols::builtin_record;
    crate::test_utils::init_test_tracing();
    let rec = builtin_record(vec![
        Value::symbol("foo"),
        Value::fixnum(1),
        Value::fixnum(2),
    ])
    .unwrap();
    let h = builtin_sxhash_equal(vec![rec]).unwrap();
    assert!(
        h.is_fixnum(),
        "sxhash-equal of a record must be a fixnum, got {h:?}"
    );
    // A record nested inside a list must also hash without panicking.
    let rec2 = builtin_record(vec![Value::symbol("bar"), Value::fixnum(9)]).unwrap();
    let nested = Value::make_cons(rec2, Value::NIL);
    assert!(builtin_sxhash_equal(vec![nested]).unwrap().is_fixnum());
}
