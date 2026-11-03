//! Regression tests for LispSymbol plist GNU-parity.
//!
//! All 7 tests pass today: the existing hybrid RAW_SYMBOL_PLIST_PROPERTY
//! overlay mechanism already preserves the user-visible GNU semantics
//! (insertion order, duplicate keys, eq-identity of symbol-plist). The
//! upcoming refactor (P2+P3) deletes the hybrid and makes LispSymbol::plist
//! a direct Value cons list — these tests must continue to pass, serving
//! as regression guards for that restructure.

use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

fn make_ctx() -> Context {
    Context::new()
}

fn eval(ctx: &mut Context, src: &str) -> Value {
    ctx.eval_str(src).expect("eval")
}

fn print_val(value: &Value) -> String {
    crate::emacs_core::print::print_value(value)
}

#[test]
fn plist_put_get_round_trips() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(put 'plist-rt-foo 'color 'red)");
    eval(&mut ctx, "(put 'plist-rt-foo 'size 10)");
    assert_eq!(
        eval(&mut ctx, "(get 'plist-rt-foo 'color)"),
        Value::symbol("red")
    );
    assert_eq!(
        eval(&mut ctx, "(get 'plist-rt-foo 'size)"),
        Value::fixnum(10)
    );
}

#[test]
fn plist_get_missing_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(put 'plist-miss 'a 1)");
    assert_eq!(eval(&mut ctx, "(get 'plist-miss 'nope)"), Value::NIL);
}

#[test]
fn get_does_not_enter_the_plist_walker_when_a_symbol_has_no_entries() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();

    crate::emacs_core::plist::reset_plist_get_walks();
    assert_eq!(eval(&mut ctx, "(get 'plist-empty 'missing)"), Value::NIL);
    assert_eq!(
        crate::emacs_core::plist::plist_get_walks(),
        0,
        "an empty symbol plist should be classified before entering the general cyclic-list walker"
    );
}

#[test]
fn get_skips_non_cons_symbol_plists_without_hiding_the_verbatim_value() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setplist 'plist-malformed 42)");

    crate::emacs_core::plist::reset_plist_get_walks();
    assert_eq!(
        eval(&mut ctx, "(get 'plist-malformed 'missing)"),
        Value::NIL
    );
    assert_eq!(crate::emacs_core::plist::plist_get_walks(), 0);
    assert_eq!(
        eval(&mut ctx, "(symbol-plist 'plist-malformed)"),
        Value::fixnum(42),
        "setplist's verbatim non-cons value remains Lisp-visible"
    );
}

#[test]
fn ordinary_plist_get_does_not_enter_symbol_with_position_comparison_loop() {
    crate::test_utils::init_test_tracing();
    let plist = Value::list(vec![
        Value::symbol("first-property"),
        Value::fixnum(1),
        Value::symbol("target-property"),
        Value::fixnum(2),
    ]);

    crate::emacs_core::plist::reset_symbol_with_pos_plist_comparisons();
    assert_eq!(
        crate::emacs_core::plist::plist_get_swp(plist, &Value::symbol("target-property"), false,),
        Some(Value::fixnum(2))
    );
    assert_eq!(
        crate::emacs_core::plist::symbol_with_pos_plist_comparisons(),
        0,
        "ordinary GNU EQ plist lookup must select its identity-only loop before walking entries"
    );

    crate::emacs_core::plist::reset_symbol_with_pos_plist_comparisons();
    assert_eq!(
        crate::emacs_core::plist::plist_get_swp(plist, &Value::symbol("target-property"), true,),
        Some(Value::fixnum(2))
    );
    assert_eq!(
        crate::emacs_core::plist::symbol_with_pos_plist_comparisons(),
        2,
        "symbol-position transparency must select the compatibility comparison loop"
    );
}

#[test]
fn plist_insertion_order_preserved() {
    // GNU: (a 1 b 2 c 3). HashMap iteration order is arbitrary — fails today.
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setplist 'plist-order nil)");
    eval(&mut ctx, "(put 'plist-order 'a 1)");
    eval(&mut ctx, "(put 'plist-order 'b 2)");
    eval(&mut ctx, "(put 'plist-order 'c 3)");
    let plist = eval(&mut ctx, "(symbol-plist 'plist-order)");
    let printed = print_val(&plist);
    assert_eq!(printed, "(a 1 b 2 c 3)", "plist order drifted: {printed}");
}

#[test]
fn plist_duplicate_keys_preserved_by_setplist() {
    // GNU: (a 1 a 2). HashMap collapses to (a 2). Fails today.
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setplist 'plist-dup '(a 1 a 2))");
    let plist = eval(&mut ctx, "(symbol-plist 'plist-dup)");
    let printed = print_val(&plist);
    assert_eq!(printed, "(a 1 a 2)", "duplicate keys dropped: {printed}");
    assert_eq!(
        eval(&mut ctx, "(plist-get (symbol-plist 'plist-dup) 'a)"),
        Value::fixnum(1),
        "plist-get should return FIRST match"
    );
}

#[test]
fn symbol_plist_returns_eq_identical_pointer() {
    // GNU: two calls to (symbol-plist 'foo) return the SAME cons.
    // HashMap synthesizes a fresh list each call — (eq p1 p2) fails today.
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(put 'plist-eq 'a 1)");
    let first_eq = eval(
        &mut ctx,
        "(eq (symbol-plist 'plist-eq) (symbol-plist 'plist-eq))",
    );
    assert_eq!(first_eq, Value::T, "(eq p (symbol-plist foo)) must be t");
}

#[test]
fn setplist_accepts_and_preserves_arbitrary_list() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setplist 'plist-setp '(x 10 y 20))");
    let plist = eval(&mut ctx, "(symbol-plist 'plist-setp)");
    let printed = print_val(&plist);
    assert_eq!(printed, "(x 10 y 20)");
    assert_eq!(eval(&mut ctx, "(get 'plist-setp 'y)"), Value::fixnum(20));
}

#[test]
fn plist_survives_gc() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(put 'plist-gc 'payload (cons 1 2))");
    let before = eval(&mut ctx, "(get 'plist-gc 'payload)");
    ctx.gc_collect();
    let after = eval(&mut ctx, "(get 'plist-gc 'payload)");
    assert!(
        crate::emacs_core::value::eq_value(&before.cons_car(), &Value::fixnum(1)),
        "car should be 1 before GC"
    );
    assert!(
        crate::emacs_core::value::eq_value(&after.cons_cdr(), &Value::fixnum(2)),
        "cdr should be 2 after GC — GC trace missed the plist value"
    );
}
