use neovm_edit::buffer::undo::*;

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
