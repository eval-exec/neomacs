//! Oracle parity tests for advanced `catch`/`throw` patterns:
//! nested catch, throw across function boundaries, catch as
//! control flow, and interaction with unwind-protect.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// catch / throw basics revisited with complex values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_complex_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // throw can carry complex data
    let form = r#"(catch 'done
                    (throw 'done
                           (list 'result
                                 '(nested data)
                                 (cons "key" "val"))))"#;
    let expect = expect_test::expect![[r#""OK (result (nested data) (\"key\" . \"val\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_catch_no_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When no throw happens, catch returns the body value
    let form = "(catch 'tag (+ 1 2 3))";
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("6", &o, &n);
}

// ---------------------------------------------------------------------------
// Nested catch with different tags
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_nested_different_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(catch 'outer
                  (list 'before
                        (catch 'inner
                          (throw 'inner 'inner-result))
                        'after))";
    let expect = expect_test::expect![r#""OK (before inner-result after)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_catch_nested_throw_to_outer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // throw to outer catch skips inner catch
    let form = "(catch 'outer
                  (catch 'inner
                    (throw 'outer 'escaped)))";
    let expect = expect_test::expect![[r#""OK escaped""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("escaped", &o, &n);
}

#[test]
fn oracle_prop_catch_same_tag_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Same tag nested: throw goes to innermost
    let form = "(catch 'tag
                  (list 'outer
                        (catch 'tag
                          (throw 'tag 'inner-val))
                        'after-inner))";
    let expect = expect_test::expect![r#""OK (outer inner-val after-inner)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// throw across function boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_across_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((bail (lambda ()
                      (throw 'escape 'bailed))))
                  (catch 'escape
                    (list 'before
                          (funcall bail)
                          'never-reached)))";
    let expect = expect_test::expect![[r#""OK bailed""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("bailed", &o, &n);
}

#[test]
fn oracle_prop_catch_throw_deep_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // throw unwinds through multiple function calls
    let form = "(let ((f3 (lambda () (throw 'done 'from-f3)))
                      (f2 (lambda (fn) (funcall fn)))
                      (f1 (lambda (fn2 fn3)
                            (funcall fn2 fn3))))
                  (catch 'done
                    (funcall f1 f2 f3)))";
    let expect = expect_test::expect![[r#""OK from-f3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("from-f3", &o, &n);
}

// ---------------------------------------------------------------------------
// catch/throw as control flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_early_return_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common pattern: use catch/throw as early return from loop
    let form = "(catch 'found
                  (dolist (x '(3 7 2 9 4 8))
                    (when (> x 8)
                      (throw 'found (list 'found x))))
                  'not-found)";
    let expect = expect_test::expect![r#""OK (found 9)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_catch_break_nested_loops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Break out of nested loops
    let form = "(catch 'break
                  (let ((result nil))
                    (dolist (x '(1 2 3))
                      (dolist (y '(10 20 30))
                        (when (= (* x y) 60)
                          (throw 'break (list x y)))))
                    'not-found))";
    let expect = expect_test::expect![r#""OK (2 30)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_catch_accumulate_then_bail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Accumulate results until a condition, then bail
    let form = "(let ((items '(1 2 3 -1 4 5)))
                  (catch 'error
                    (let ((sum 0))
                      (dolist (item items)
                        (if (< item 0)
                            (throw 'error
                                   (list 'negative-found sum))
                          (setq sum (+ sum item))))
                      sum)))";
    let expect = expect_test::expect![r#""OK (negative-found 6)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// catch/throw with unwind-protect
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unwind-protect cleanup runs even on throw
    let form = "(let ((log nil))
                  (catch 'exit
                    (unwind-protect
                        (progn
                          (setq log (cons 'body log))
                          (throw 'exit 'done))
                      (setq log (cons 'cleanup log))))
                  (nreverse log))";
    let expect = expect_test::expect![[r#""OK (body cleanup)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(body cleanup)", &o, &n);
}

#[test]
fn oracle_prop_catch_throw_nested_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple unwind-protect layers with throw
    let form = "(let ((log nil))
                  (let ((result
                         (catch 'exit
                           (unwind-protect
                               (unwind-protect
                                   (progn
                                     (setq log (cons 'deep log))
                                     (throw 'exit 42))
                                 (setq log (cons 'inner-cleanup log)))
                             (setq log (cons 'outer-cleanup log))))))
                    (list result (nreverse log))))";
    let expect = expect_test::expect![r#""OK (42 (deep inner-cleanup outer-cleanup))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-tag catch for control flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_multi_tag_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Different tags for different exit reasons
    let form = "(let ((process-items
                       (lambda (items)
                         (catch 'error
                           (catch 'done
                             (let ((result nil))
                               (dolist (item items)
                                 (cond
                                   ((eq item 'stop)
                                    (throw 'done
                                           (nreverse result)))
                                   ((eq item 'fail)
                                    (throw 'error
                                           (list 'failed
                                                 (nreverse result))))
                                   (t (setq result
                                            (cons (* item item)
                                                  result)))))
                               (nreverse result)))))))
                  (list
                    (funcall process-items '(1 2 3 4 5))
                    (funcall process-items '(1 2 3 stop 4 5))
                    (funcall process-items '(1 2 fail 3 4))))";
    let expect = expect_test::expect![r#""OK ((1 4 9 16 25) (1 4 9) (failed (1 4)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
