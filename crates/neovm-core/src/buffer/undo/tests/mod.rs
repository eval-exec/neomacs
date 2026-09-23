use super::*;

fn c(pos: usize) -> CharPos0 {
    CharPos0::new(pos)
}

fn clen(len: usize) -> CharLen {
    CharLen::new(len)
}

/// The recorded list, newest first, as a flat vector.
///
/// The tests below are about what the RECORDER produces, so they read the
/// list itself.  There is no "pop a group" step here any more: grouping was
/// a replay-side idea, and replay is `primitive-undo' (lisp/simple.el:3645),
/// which is Lisp.  DIVERGENCES.md 150.
fn entries(mut list: Value) -> Vec<Value> {
    let mut out = Vec::new();
    while list.is_cons() {
        out.push(list.cons_car());
        list = list.cons_cdr();
    }
    out
}

#[test]
fn basic_insert_undo() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    undo_list_record_insert(&mut list, c(0), clen(5));
    undo_list_record_insert(&mut list, c(5), clen(3));
    undo_list_boundary(&mut list);

    // The second insert merges with the first, so the whole list is the
    // boundary and one (1 . 9) record.
    assert!(undo_list_has_trailing_boundary(&list));
    let recorded = entries(list);
    assert_eq!(recorded.len(), 2);
    assert!(recorded[0].is_nil());
    let entry = recorded[1];
    assert!(entry.is_cons());
    assert_eq!(entry.cons_car(), Value::fixnum(1));
    assert_eq!(entry.cons_cdr(), Value::fixnum(9));
}

#[test]
fn delete_records_text() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    undo_list_record_delete(
        &mut list,
        c(3),
        crate::heap_types::LispString::from_unibyte(b"hello".to_vec()),
        c(3),
    );
    undo_list_boundary(&mut list);

    let recorded = entries(list);
    assert_eq!(recorded.len(), 2);
    assert!(recorded[0].is_nil());
    let entry = recorded[1];
    assert!(entry.is_cons());
    let car = entry.cons_car();
    assert!(car.is_string());
    // POS should be positive (4) because pt==beg
    assert_eq!(entry.cons_cdr(), Value::fixnum(4));
}

#[test]
fn boundary_separates_groups() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    undo_list_record_insert(&mut list, c(0), clen(1));
    undo_list_boundary(&mut list);
    undo_list_record_insert(&mut list, c(1), clen(1));
    undo_list_boundary(&mut list);

    // (nil (2 . 3) nil (1 . 2)): one boundary between the two records, and
    // one in front, exactly as GNU's `record_boundary' leaves it.
    let recorded = entries(list);
    assert_eq!(recorded.len(), 4);
    assert!(recorded[0].is_nil());
    assert!(recorded[2].is_nil());

    let entry = recorded[1];
    assert!(entry.is_cons());
    assert_eq!(entry.cons_car(), Value::fixnum(2)); // 1+1
    assert_eq!(entry.cons_cdr(), Value::fixnum(3)); // 1+1+1

    let entry = recorded[3];
    assert!(entry.is_cons());
    assert_eq!(entry.cons_car(), Value::fixnum(1)); // 0+1
    assert_eq!(entry.cons_cdr(), Value::fixnum(2)); // 0+1+1
}

#[test]
fn disabled_records_nothing() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::T;
    undo_list_record_insert(&mut list, c(0), clen(5));
    assert!(undo_list_is_disabled(&list));
}

#[test]
fn cursor_move_dedup() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    undo_list_record_point(&mut list, c(5));
    undo_list_record_point(&mut list, c(5));
    undo_list_record_point(&mut list, c(5));
    // Should only have one entry
    assert!(list.is_cons());
    assert_eq!(list.cons_car(), Value::fixnum(6));
    assert!(list.cons_cdr().is_nil());

    undo_list_record_point(&mut list, c(10));
    // Now should have two entries
    assert!(list.is_cons());
    assert_eq!(list.cons_car(), Value::fixnum(11));
}

#[test]
fn no_double_boundary() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    undo_list_record_insert(&mut list, c(0), clen(1));
    undo_list_boundary(&mut list);
    undo_list_boundary(&mut list);
    undo_list_boundary(&mut list);
    // Only one boundary after the insert: (nil (1 . 2)), not three nils.
    assert!(undo_list_has_trailing_boundary(&list));
    let recorded = entries(list);
    assert_eq!(recorded.len(), 2);
    assert!(recorded[0].is_nil());
    assert!(recorded[1].is_cons());
}

/// GNU `record_insert` (src/undo.c:98-112) coalesces a new insertion into the
/// newest record in exactly one direction: when that record is a `(BEG . END)`
/// insertion whose END equals the new insertion's BEG.  There is no reverse
/// rule -- an insertion that ENDS where the newest record BEGINS stays its own
/// record, because `primitive-undo` replays the records in order and each
/// later record's positions are read against the buffer the earlier deletions
/// already reshaped.
///
/// Descending edits are the ordinary case: `tide-apply-edits` walks a
/// TypeScript `textChanges` list back-to-front so earlier positions stay valid,
/// which produces exactly this shape.  Verified on GNU Emacs -Q --batch: two
/// one-character insertions at 20 then 19 leave `((19 . 20) (20 . 21))`.
#[test]
fn descending_adjacent_inserts_stay_separate_records_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    // Insert one character at 1-indexed 20, then one at 1-indexed 19.
    undo_list_record_insert(&mut list, c(19), clen(1));
    undo_list_record_insert(&mut list, c(18), clen(1));

    let newest = list.cons_car();
    assert_eq!(newest.cons_car(), Value::fixnum(19));
    assert_eq!(newest.cons_cdr(), Value::fixnum(20));

    let older = list.cons_cdr().cons_car();
    assert_eq!(older.cons_car(), Value::fixnum(20));
    assert_eq!(older.cons_cdr(), Value::fixnum(21));

    assert!(list.cons_cdr().cons_cdr().is_nil());
}

#[test]
fn to_value_produces_list() {
    crate::test_utils::init_test_tracing();
    let mut list = Value::NIL;
    undo_list_record_insert(&mut list, c(0), clen(5));
    undo_list_boundary(&mut list);
    assert!(list.is_list());
}

#[test]
fn undoing_flag_not_needed() {
    crate::test_utils::init_test_tracing();
    // The undoing flag is now tracked on Buffer, not in the undo list itself.
    // This test just verifies that disabled lists don't record.
    let mut list = Value::T; // disabled
    undo_list_record_insert(&mut list, c(0), clen(5));
    assert!(undo_list_is_disabled(&list));

    let mut list2 = Value::NIL; // enabled
    undo_list_record_insert(&mut list2, c(0), clen(5));
    assert!(!list2.is_nil());
}

// ---------------------------------------------------------------------------
// Truncation byte accounting and limit domain
// ---------------------------------------------------------------------------

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

/// GNU charges one cons for the chain link, a second for a cons-shaped record,
/// and `sizeof (struct Lisp_String) - 1 + SCHARS' for a saved deletion string
/// (`src/undo.c:334-342') -- SCHARS is *characters*, not bytes.
///
/// Measured under GNU Emacs 31.0.90 --batch through
/// `undo-outer-limit-function': a lone `("x" x100 . -1)' record reports size
/// 163, and the same record holding 100 two-byte characters also reports 163.
#[test]
fn undo_group_size_matches_gnus_string_accounting() {
    crate::test_utils::init_test_tracing();

    let unibyte = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![b'x'; 100]));
    let unibyte_list = Value::list(vec![Value::cons(unibyte, Value::fixnum(-1))]);
    assert_eq!(undo_first_group_bytes(unibyte_list), 163);

    let multibyte = Value::heap_string(crate::heap_types::LispString::new("é".repeat(100), true));
    let multibyte_list = Value::list(vec![Value::cons(multibyte, Value::fixnum(-1))]);
    assert_eq!(undo_first_group_bytes(multibyte_list), 163);
}

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
