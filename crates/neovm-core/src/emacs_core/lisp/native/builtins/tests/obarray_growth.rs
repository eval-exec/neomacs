//! A custom obarray must grow on the count it already maintains, the way GNU
//! `intern_sym` does (src/lread.c:4696):
//!
//! ```c
//! o->count++;
//! if (o->count > obarray_size (o))
//!   grow_obarray (o);
//! ```
//!
//! Recomputing that count instead means walking every bucket chain -- and
//! allocating a `Vec` per chain just to read its length -- on EVERY intern,
//! which makes filling an obarray quadratic. Interning 32,000 names took
//! 5,427ms against GNU Emacs 31.1's 12.9ms.
//!
//! The bucket-vector length is what pins the policy: it is the only
//! externally visible consequence of the growth decision, and a count that
//! has drifted (or is ignored) shows up here as a vector that never doubled.

use super::{
    builtin_intern_fn, builtin_obarray_clear, intern_soft_impl, obarray_len,
    obarray_symbol_count_for_test,
};
use crate::emacs_core::Context;
use crate::emacs_core::value::Value;

fn intern_into(eval: &mut Context, obarray: Value, name: &str) {
    builtin_intern_fn(eval, vec![Value::string(name), obarray]).expect("intern");
}

/// Growth doubles the bucket vector exactly when the count passes the size,
/// and every symbol survives every rehash.
#[test]
fn a_custom_obarray_grows_on_its_maintained_count() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // Start small so the sweep crosses several doublings inside one test.
    let obarray = Value::obarray(16);

    // GNU grows when count EXCEEDS size, so the vector is still 16 at 16
    // symbols and doubles on the 17th.
    for i in 0..16 {
        intern_into(&mut eval, obarray, &format!("sym-{i}"));
    }
    assert_eq!(
        obarray_len(obarray),
        Some(16),
        "count == size must not grow yet (GNU's test is `count > size`)"
    );
    intern_into(&mut eval, obarray, "sym-16");
    assert_eq!(obarray_len(obarray), Some(32), "the 17th symbol doubles it");

    for i in 17..5000 {
        intern_into(&mut eval, obarray, &format!("sym-{i}"));
    }
    // 5000 symbols: 16 -> 32 -> ... -> 8192 is the first size >= 5000.
    assert_eq!(
        obarray_len(obarray),
        Some(8192),
        "the vector doubled on every count-exceeds-size crossing"
    );
    assert_eq!(
        obarray_symbol_count_for_test(obarray),
        5000,
        "no symbol was dropped by a rehash"
    );

    // Every name still resolves, i.e. each rehash re-bucketed correctly.
    for i in [0usize, 1, 16, 17, 1234, 4999] {
        let found = intern_soft_impl(&eval, &[Value::string(format!("sym-{i}")), obarray])
            .expect("intern-soft");
        assert!(
            !found.is_nil(),
            "sym-{i} was lost across the rehashes that grew the vector"
        );
    }

    // Re-interning an existing name must not count again -- that is what
    // would make the vector grow without the membership growing.
    intern_into(&mut eval, obarray, "sym-1234");
    assert_eq!(obarray_len(obarray), Some(8192));
    assert_eq!(obarray_symbol_count_for_test(obarray), 5000);
}

/// `obarray-clear` resets the count, so the next fill must grow from scratch
/// rather than inheriting a stale one.
#[test]
fn clearing_a_custom_obarray_resets_the_growth_count() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let obarray = Value::obarray(16);
    for i in 0..200 {
        intern_into(&mut eval, obarray, &format!("sym-{i}"));
    }
    assert_eq!(obarray_symbol_count_for_test(obarray), 200);

    builtin_obarray_clear(vec![obarray]).expect("obarray-clear");
    assert_eq!(obarray_symbol_count_for_test(obarray), 0);

    // The vector keeps the size it grew to; what must be reset is the count.
    // If the count had survived the clear, the very first intern would report
    // count > size and rehash an empty table on every insert.
    let size_after_clear = obarray_len(obarray).expect("obarray length");
    for i in 0..16 {
        intern_into(&mut eval, obarray, &format!("fresh-{i}"));
    }
    assert_eq!(
        obarray_len(obarray),
        Some(size_after_clear),
        "16 symbols cannot exceed a vector that already held 200"
    );
    assert_eq!(obarray_symbol_count_for_test(obarray), 16);
}
