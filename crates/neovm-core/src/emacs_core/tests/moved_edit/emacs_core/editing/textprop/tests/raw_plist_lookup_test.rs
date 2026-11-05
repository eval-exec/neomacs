//! Focused raw-plist semantic and observation regressions.

use neovm_edit::buffer::{
    Buffer, BufferId, BufferManager, CharLen, CharPos0, CharRange, EmacsBytePos, EmacsByteRange,
    LispCharPos1,
};
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_edit::emacs_core::edit_host::{EditHost, EditVariableRead};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_max_args, expect_min_args};
use neovm_value::intern::SymId;
use neovm_value::intern::WellKnown;
use neovm_value::intern::{NIL_SYM_ID, T_SYM_ID};
use neovm_value::symbol::CanonicalAccess;
use neovm_value::symbol::Obarray;
use neovm_value::tagged::value::SymbolMode;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use strum::IntoStaticStr;

use super::plist_get_value as get;

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

use neovm_value::tagged::collection_reads::capture;

#[test]
fn raw_plist_literal_absent_nil_duplicate_and_malformed_results() {
    let _eval = context();
    let key = Value::fixnum(1);
    let other = Value::fixnum(2);
    let value = Value::fixnum(10);
    assert_eq!(get(Value::NIL, key), None);
    assert_eq!(get(Value::fixnum(99), key), None);
    assert_eq!(
        get(Value::list(vec![key, Value::NIL]), key),
        Some(Value::NIL)
    );
    assert_eq!(get(Value::list(vec![key, value]), other), None);
    assert_eq!(
        get(Value::list(vec![key, value, key, Value::fixnum(20)]), key),
        Some(value)
    );
    assert_eq!(get(Value::cons(key, Value::NIL), key), None);
    assert_eq!(get(Value::cons(key, Value::fixnum(99)), key), None);
    let dotted = Value::cons(key, Value::cons(value, Value::fixnum(99)));
    assert_eq!(get(dotted, key), Some(value));
    assert_eq!(get(dotted, other), None);
    assert_eq!(get(Value::list(vec![other, value, key]), key), None);
}

#[test]
fn raw_plist_keeps_exact_positioned_key_identity() {
    let mut eval = context();
    let bare = Value::symbol("raw-plist-positioned-key");
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    let stored = unsafe { eval.tagged_heap_mut() }.alloc_symbol_with_pos(bare, Value::fixnum(3));
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    let distinct = unsafe { eval.tagged_heap_mut() }.alloc_symbol_with_pos(bare, Value::fixnum(7));
    let plist = Value::list(vec![stored, Value::fixnum(10)]);
    assert_eq!(get(plist, stored), Some(Value::fixnum(10)));
    assert_eq!(get(plist, distinct), None);
    assert_eq!(get(plist, bare), None);
}

#[test]
fn raw_plist_first_hit_observes_both_key_fields_and_value() {
    let _eval = context();
    for mutation in [
        FirstReadMutation::KeyCar,
        FirstReadMutation::KeyCdr,
        FirstReadMutation::ValueCar,
    ] {
        let plist = TwoPairs::new();
        let (answer, reads) = capture(|| get(plist.first_key_cell, Value::fixnum(1)));
        assert_eq!(answer, Some(Value::fixnum(10)));
        let reads = reads.expect("native lookup retains a coherent certificate");
        assert!(reads.unchanged());
        match mutation {
            FirstReadMutation::KeyCar => plist.first_key_cell.set_car(Value::fixnum(99)),
            FirstReadMutation::KeyCdr => plist.first_key_cell.set_cdr(Value::NIL),
            FirstReadMutation::ValueCar => plist.first_value_cell.set_car(Value::fixnum(99)),
        }
        assert!(!reads.unchanged());
    }
}

#[test]
fn raw_plist_late_hit_and_miss_observe_the_traversed_path() {
    let _eval = context();
    for target in [Value::fixnum(2), Value::fixnum(3)] {
        for mutation in [
            PathMutation::FirstKeyCar,
            PathMutation::FirstKeyCdr,
            PathMutation::FirstValueCdr,
            PathMutation::LastKeyCar,
            PathMutation::LastKeyCdr,
            PathMutation::LastValueCar,
            PathMutation::LastValueCdr,
        ] {
            let plist = TwoPairs::new();
            let (answer, reads) = capture(|| get(plist.first_key_cell, target));
            let expected = (target == Value::fixnum(2)).then_some(Value::fixnum(20));
            assert_eq!(answer, expected);
            let reads = reads.expect("native lookup retains a coherent certificate");
            assert!(reads.unchanged());
            match mutation {
                PathMutation::FirstKeyCar => plist.first_key_cell.set_car(Value::fixnum(99)),
                PathMutation::FirstKeyCdr => plist.first_key_cell.set_cdr(Value::NIL),
                PathMutation::FirstValueCdr => plist.first_value_cell.set_cdr(Value::NIL),
                PathMutation::LastKeyCar => plist.last_key_cell.set_car(Value::fixnum(99)),
                PathMutation::LastKeyCdr => plist.last_key_cell.set_cdr(Value::NIL),
                PathMutation::LastValueCar => plist.last_value_cell.set_car(Value::fixnum(99)),
                PathMutation::LastValueCdr => plist.last_value_cell.set_cdr(Value::T),
            }
            assert!(!reads.unchanged());
        }
    }
}

#[test]
fn raw_plist_first_hit_does_not_observe_the_following_key_cell() {
    let _eval = context();
    let plist = TwoPairs::new();
    let (answer, reads) = capture(|| get(plist.first_key_cell, Value::fixnum(1)));
    assert_eq!(answer, Some(Value::fixnum(10)));
    let reads = reads.expect("native lookup retains a coherent certificate");
    plist.last_key_cell.set_car(Value::fixnum(99));
    assert!(
        reads.unchanged(),
        "unread following cons is not a dependency"
    );
}

#[test]
fn raw_plist_nested_reuse_imports_key_dependencies() {
    let _eval = context();
    for mutation in [FirstReadMutation::KeyCar, FirstReadMutation::KeyCdr] {
        let plist = TwoPairs::new();
        let (_, inner) = capture(|| get(plist.first_key_cell, Value::fixnum(1)));
        let inner = inner.expect("native lookup retains a coherent certificate");
        let (_, outer) = capture(|| assert!(inner.unchanged_and_observe()));
        let outer = outer.expect("nested import retains a coherent certificate");
        match mutation {
            FirstReadMutation::KeyCar => plist.first_key_cell.set_car(Value::fixnum(99)),
            FirstReadMutation::KeyCdr => plist.first_key_cell.set_cdr(Value::NIL),
            FirstReadMutation::ValueCar => plist.first_value_cell.set_car(Value::fixnum(99)),
        }
        assert!(!inner.unchanged());
        assert!(!outer.unchanged());
    }
}

#[test]
fn raw_plist_early_hit_remains_valid_on_a_cyclic_tail() {
    let _eval = context();
    let value_cell = Value::cons(Value::fixnum(10), Value::NIL);
    let plist = Value::cons(Value::fixnum(1), value_cell);
    value_cell.set_cdr(plist);
    assert_eq!(get(plist, Value::fixnum(1)), Some(Value::fixnum(10)));
    // Deliberately do not query a missing key: old/new raw traversal does not
    // terminate there. Safe public plist_get cycle behavior is a separate API.
}

struct TwoPairs {
    first_key_cell: Value,
    first_value_cell: Value,
    last_key_cell: Value,
    last_value_cell: Value,
}

impl TwoPairs {
    fn new() -> Self {
        let last_value_cell = Value::cons(Value::fixnum(20), Value::NIL);
        let last_key_cell = Value::cons(Value::fixnum(2), last_value_cell);
        let first_value_cell = Value::cons(Value::fixnum(10), last_key_cell);
        let first_key_cell = Value::cons(Value::fixnum(1), first_value_cell);
        Self {
            first_key_cell,
            first_value_cell,
            last_key_cell,
            last_value_cell,
        }
    }
}

fn context() -> Context {
    crate::test_utils::init_test_tracing();
    crate::test_utils::new_context()
}

#[derive(Clone, Copy)]
enum FirstReadMutation {
    KeyCar,
    KeyCdr,
    ValueCar,
}

#[derive(Clone, Copy)]
enum PathMutation {
    FirstKeyCar,
    FirstKeyCdr,
    FirstValueCdr,
    LastKeyCar,
    LastKeyCdr,
    LastValueCar,
    LastValueCdr,
}
