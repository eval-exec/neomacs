//! The counting sort that rebuilds GNU's obarray iteration order must produce
//! exactly what the comparison sort it replaced produced: bucket index
//! ascending, newest symbol first inside a bucket.

use super::symbols::{
    GNU_INITIAL_OBARRAY_SIZE, global_obarray_symbol_ids_in_bucket_order, obarray_hash_lisp_string,
};
use crate::emacs_core::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

/// The algorithm this replaced: key every member by `(bucket, membership
/// position)` and sort bucket ascending, position descending.
fn reference_bucket_order(obarray: &Obarray, len: usize) -> Vec<SymId> {
    let mut entries: Vec<_> = obarray
        .global_member_ids()
        .enumerate()
        .map(|(order, id)| {
            let name = crate::emacs_core::intern::resolve_sym_lisp_string(id);
            (obarray_hash_lisp_string(name, len), order, id)
        })
        .collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
    entries.into_iter().map(|(_, _, id)| id).collect()
}

fn assert_matches_reference(eval: &Context, label: &str) {
    let obarray = eval.obarray();
    let ordered = global_obarray_symbol_ids_in_bucket_order(obarray, Value::NIL);
    let reference = reference_bucket_order(obarray, GNU_INITIAL_OBARRAY_SIZE);
    assert_eq!(
        ordered.len(),
        reference.len(),
        "{label}: every member appears exactly once"
    );
    assert_eq!(
        ordered.as_ref(),
        reference.as_slice(),
        "{label}: counting sort must reproduce the comparison order"
    );
}

#[test]
fn counting_sort_reproduces_the_comparison_bucket_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_matches_reference(&eval, "startup obarray");

    // Interning bumps the membership epoch, so the order is rebuilt: the two
    // algorithms must still agree once new symbols land in arbitrary buckets.
    eval.eval_str(
        "(progn (intern \"obarray-order-probe-a\")
                (intern \"obarray-order-probe-bb\")
                (intern \"obarray-order-probe-ccc\")
                (setq obarray-order-probe-value 1))",
    )
    .expect("intern probe symbols");
    assert_matches_reference(&eval, "after interning");

    // Uninterning removes members from the middle of their buckets.
    eval.eval_str("(unintern \"obarray-order-probe-bb\" obarray)")
        .expect("unintern a probe symbol");
    assert_matches_reference(&eval, "after uninterning");
}

/// `mapatoms` and completion consume the same snapshot, so a symbol interned
/// after the first enumeration must appear in the next one.
#[test]
fn a_symbol_interned_after_an_enumeration_joins_the_next_one() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let before = global_obarray_symbol_ids_in_bucket_order(eval.obarray(), Value::NIL);
    let probe = crate::emacs_core::intern::intern("obarray-order-late-probe");
    assert!(!before.contains(&probe), "probe is not a member yet");

    eval.eval_str("(setq obarray-order-late-probe 7)")
        .expect("promote the probe to global membership");
    let after = global_obarray_symbol_ids_in_bucket_order(eval.obarray(), Value::NIL);
    assert!(after.contains(&probe), "probe joined the enumeration");
    assert_matches_reference(&eval, "with the late probe");
}
