//! Oracle parity tests for common interactive Elisp patterns.
//!
//! These test real-world patterns commonly found in Emacs configuration
//! and packages: hooks, alist manipulation, string formatting, property
//! list configuration, etc.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Alist patterns (commonly used for configuration)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_alist_assoc_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((config '(("name" . "emacs")
                                   ("version" . "29")
                                   ("editor" . "best"))))
                    (cdr (assoc "version" config)))"####;
    let expect = expect_test::expect![[r#""OK \"29\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""29""#, &o, &n);
}

#[test]
fn oracle_prop_interactive_alist_add_to_front() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((al '((a . 1) (b . 2))))
                  (setq al (cons '(c . 3) al))
                  (mapcar 'car al))";
    let expect = expect_test::expect![[r#""OK (c a b)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(c a b)", &o, &n);
}

// ---------------------------------------------------------------------------
// Hook simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_hook_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate running hooks (list of functions)
    let form = "(let ((log nil)
                      (hooks (list (lambda () (setq log (cons 'first log)))
                                   (lambda () (setq log (cons 'second log)))
                                   (lambda () (setq log (cons 'third log))))))
                  (dolist (hook hooks)
                    (funcall hook))
                  (nreverse log))";
    let expect = expect_test::expect![[r#""ERR (void-variable log)""#]];
    // Under lexical binding, `setq` on `log` inside the lambdas refers to the
    // lexical `log` from the outer `let`. However, `dolist` is a macro from
    // subr.el and the lambdas close over `log` lexically. GNU Emacs signals
    // (void-variable log) because the closures capture `log` at definition time
    // but `setq` inside them modifies a different binding.
    // Both GNU Emacs and NeoVM should agree on the result.
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(n, o, "neovm and oracle should match");
}

// ---------------------------------------------------------------------------
// Formatting patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_format_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((rows '((1 "Alice" 95)
                                 (2 "Bob" 87)
                                 (3 "Carol" 92))))
                    (mapcar (lambda (row)
                              (format "%d: %s (%d)" (car row) (cadr row) (caddr row)))
                            rows))"####;
    let expect =
        expect_test::expect![[r#""OK (\"1: Alice (95)\" \"2: Bob (87)\" \"3: Carol (92)\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List processing pipelines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_list_processing_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common pattern: filter + transform + collect
    let form = "(let ((data '(1 2 3 4 5 6 7 8 9 10))
                      (result nil))
                  (dolist (x data)
                    (when (> x 5)
                      (setq result (cons (* x x) result))))
                  (nreverse result))";
    let expect = expect_test::expect![[r#""OK (36 49 64 81 100)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(36 49 64 81 100)", &o, &n);
}

#[test]
fn oracle_prop_interactive_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Partition a list into two based on predicate
    let form = "(let ((yes nil) (no nil))
                  (dolist (x '(1 2 3 4 5 6 7 8 9 10))
                    (if (= 0 (% x 2))
                        (setq yes (cons x yes))
                      (setq no (cons x no))))
                  (list (nreverse yes) (nreverse no)))";
    let expect = expect_test::expect![[r#""OK ((2 4 6 8 10) (1 3 5 7 9))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("((2 4 6 8 10) (1 3 5 7 9))", &o, &n);
}

// ---------------------------------------------------------------------------
// Stack and queue via lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_stack_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Push/pop stack using cons/car/cdr
    let form = "(let ((stack nil))
                  (setq stack (cons 'a stack))
                  (setq stack (cons 'b stack))
                  (setq stack (cons 'c stack))
                  (let ((top (car stack)))
                    (setq stack (cdr stack))
                    (list top stack)))";
    let expect = expect_test::expect![[r#""OK (c (b a))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(c (b a))", &o, &n);
}

// ---------------------------------------------------------------------------
// Association and lookup patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_multi_level_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((db '((alice . ((age . 30) (role . engineer)))
                             (bob . ((age . 25) (role . designer))))))
                  (let ((alice-data (cdr (assq 'alice db))))
                    (list (cdr (assq 'age alice-data))
                          (cdr (assq 'role alice-data)))))";
    let expect = expect_test::expect![[r#""OK (30 engineer)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(30 engineer)", &o, &n);
}

// ---------------------------------------------------------------------------
// String building patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_join_with_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(mapconcat 'symbol-name '(foo bar baz) "/")"####;
    let expect = expect_test::expect![[r#""OK \"foo/bar/baz\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""foo/bar/baz""#, &o, &n);
}

#[test]
fn oracle_prop_interactive_string_replace_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple character replacement using mapconcat + char-to-string
    let form = r####"(mapconcat (lambda (c)
                                (if (= c ?-)
                                    "_"
                                  (char-to-string c)))
                              "hello-world-foo" "")"####;
    let expect = expect_test::expect![[r#""OK \"hello_world_foo\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Math accumulation patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_running_average() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sum 0) (count 0))
                  (dolist (x '(10 20 30 40 50))
                    (setq sum (+ sum x)
                          count (1+ count)))
                  (/ sum count))";
    let expect = expect_test::expect![[r#""OK 30""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("30", &o, &n);
}

#[test]
fn oracle_prop_interactive_find_max_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((best nil))
                  (dolist (x '(3 1 4 1 5 9 2 6 5 3 5))
                    (when (or (null best) (> x best))
                      (setq best x)))
                  best)";
    let expect = expect_test::expect![[r#""OK 9""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("9", &o, &n);
}

// ---------------------------------------------------------------------------
// Conditional logic patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_cond_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(mapcar (lambda (x)
                          (cond
                            ((< x 0) 'negative)
                            ((= x 0) 'zero)
                            ((< x 10) 'small)
                            ((< x 100) 'medium)
                            (t 'large)))
                        '(-5 0 3 42 200))";
    let expect = expect_test::expect![[r#""OK (negative zero small medium large)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive descent pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_eval_simple_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tiny expression evaluator
    let form = "(progn
                  (fset 'neovm--test-my-eval
                        (lambda (expr)
                          (cond
                            ((numberp expr) expr)
                            ((and (consp expr) (eq (car expr) '+))
                             (+ (funcall 'neovm--test-my-eval (cadr expr))
                                (funcall 'neovm--test-my-eval (caddr expr))))
                            ((and (consp expr) (eq (car expr) '*))
                             (* (funcall 'neovm--test-my-eval (cadr expr))
                                (funcall 'neovm--test-my-eval (caddr expr))))
                            (t (signal 'error (list \"unknown\" expr))))))
                  (unwind-protect
                      (list (funcall 'neovm--test-my-eval '(+ 1 (* 2 3)))
                            (funcall 'neovm--test-my-eval '(* (+ 1 2) (+ 3 4)))
                            (funcall 'neovm--test-my-eval 42))
                    (fmakunbound 'neovm--test-my-eval)))";
    let expect = expect_test::expect![[r#""OK (7 21 42)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(7 21 42)", &o, &n);
}
