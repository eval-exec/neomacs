use super::*;

use neovm_value::heap_types::LispString;
use neovm_value::value::scratch_bridge::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};

use neovm_edit::buffer::undo::*;
use neovm_value::value::Value;
use neovm_value::value::positions::CharLen;
use neovm_value::value::positions::CharPos0;

/// The limits can only come from the variables: a `undo-limit' holding
/// something GNU's `DEFVAR_INT' slot could never hold (`src/data.c:1475-1483'
/// signals rather than storing it) yields no limits at all, so nothing is
/// truncated with an invented number.
#[test]
fn undo_limits_refuse_to_be_read_from_a_non_integer_variable() {
    crate::test_utils::init_test_tracing();

    let sane = TestUndoVariables::sizes(160_000, 240_000);
    assert!(UndoLimits::read(&sane).is_some());

    let broken = TestUndoVariables {
        limit: Value::string("nope"),
        strong_limit: Value::fixnum(240_000),
        outer: Value::NIL,
        outer_function: Value::NIL,
    };
    assert!(UndoLimits::read(&broken).is_none());
}

/// `undo-outer-limit-function' is offered the record only when the limit is
/// exceeded AND a function exists (`src/undo.c:352-356').
#[test]
fn outer_limit_function_is_offered_only_when_both_halves_are_present() {
    crate::test_utils::init_test_tracing();

    let function = Value::symbol("undo-outer-limit-truncate");
    let armed = TestUndoVariables {
        limit: Value::fixnum(160_000),
        strong_limit: Value::fixnum(240_000),
        outer: Value::fixnum(100),
        outer_function: function,
    };
    let limits = UndoLimits::read(&armed).expect("limits");
    assert_eq!(limits.outer_limit_function_for(101), Some(function));
    assert_eq!(limits.outer_limit_function_for(100), None);

    let no_function = TestUndoVariables {
        outer_function: Value::NIL,
        ..TestUndoVariables::sizes(160_000, 240_000)
    };
    let limits = UndoLimits::read(&TestUndoVariables {
        outer: Value::fixnum(100),
        ..no_function
    })
    .expect("limits");
    assert_eq!(limits.outer_limit_function_for(101), None);
}

/// A malformed `buffer-undo-list' -- an atom that is neither `t' nor a list --
/// is cleared, GNU's "There's nothing we decided to keep" arm
/// (`src/undo.c:414-416').
#[test]
fn truncation_clears_a_list_with_nothing_worth_keeping() {
    crate::test_utils::init_test_tracing();
    let limits = UndoLimits::read(&TestUndoVariables::sizes(1, 1)).expect("limits");
    assert!(truncate_undo_list(Value::fixnum(5), &limits).is_nil());
}

/// An undo list that fits is returned untouched, however many groups it has.
#[test]
fn truncation_leaves_a_list_that_fits_alone() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    for start in 0..10 {
        undo_list_record_insert(&mut list, c(start * 5), clen(5));
        undo_list_boundary(&mut list);
    }
    let before = crate::emacs_core::print::print_value(&list);

    let limits = UndoLimits::read(&TestUndoVariables::sizes(160_000, 240_000)).expect("limits");
    let after = truncate_undo_list(list, &limits);

    assert_eq!(crate::emacs_core::print::print_value(&after), before);
}

fn c(pos: usize) -> CharPos0 {
    CharPos0::new(pos)
}

fn clen(len: usize) -> CharLen {
    CharLen::new(len)
}

/// A stand-in for the four variables `truncate_undo_list' reads.
struct TestUndoVariables {
    limit: Value,
    strong_limit: Value,
    outer: Value,
    outer_function: Value,
}

impl TestUndoVariables {
    fn sizes(limit: i64, strong_limit: i64) -> Self {
        Self {
            limit: Value::fixnum(limit),
            strong_limit: Value::fixnum(strong_limit),
            outer: Value::NIL,
            outer_function: Value::NIL,
        }
    }
}

impl UndoLimitBindings for TestUndoVariables {
    fn undo_limit(&self) -> Value {
        self.limit
    }
    fn undo_strong_limit(&self) -> Value {
        self.strong_limit
    }
    fn undo_outer_limit(&self) -> Value {
        self.outer
    }
    fn undo_outer_limit_function(&self) -> Value {
        self.outer_function
    }
}
