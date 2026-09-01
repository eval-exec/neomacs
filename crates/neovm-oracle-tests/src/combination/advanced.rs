//! Advanced combination oracle tests: complex multi-feature interactions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Interpreter / evaluator patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_lisp_interpreter_in_lisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // This depends on GNU's dumped subr runtime (`cadr`, `caddr`, ...),
    // so it must be checked against the bootstrapped runtime surface.
    let form = "(progn
  (fset 'neovm--mini-eval
    (lambda (expr env)
      (cond
        ((numberp expr) expr)
        ((symbolp expr)
         (let ((binding (assq expr env)))
           (if binding (cdr binding)
             (signal 'void-variable (list expr)))))
        ((eq (car expr) 'quote) (cadr expr))
        ((eq (car expr) '+)
         (+ (funcall 'neovm--mini-eval (cadr expr) env)
            (funcall 'neovm--mini-eval (caddr expr) env)))
        ((eq (car expr) '*)
         (* (funcall 'neovm--mini-eval (cadr expr) env)
            (funcall 'neovm--mini-eval (caddr expr) env)))
        ((eq (car expr) 'if)
         (if (funcall 'neovm--mini-eval (cadr expr) env)
             (funcall 'neovm--mini-eval (caddr expr) env)
           (funcall 'neovm--mini-eval
                    (or (cadddr expr) nil) env)))
        ((eq (car expr) 'let1)
         (let* ((var (caadr expr))
                (val (funcall 'neovm--mini-eval
                              (cadadr expr) env))
                (new-env (cons (cons var val) env)))
           (funcall 'neovm--mini-eval (caddr expr) new-env)))
        (t (signal 'error (list \"unknown form\" expr))))))
  (unwind-protect
      (list
        (funcall 'neovm--mini-eval '(+ 1 2) nil)
        (funcall 'neovm--mini-eval
                 '(let1 (x 10) (+ x (* x 2))) nil)
        (funcall 'neovm--mini-eval
                 '(if 1 (quote yes) (quote no)) nil)
        (funcall 'neovm--mini-eval
                 '(let1 (a 3)
                    (let1 (b 4)
                      (+ (* a a) (* b b)))) nil))
    (fmakunbound 'neovm--mini-eval)))";
    let expect = expect_test::expect![[r#""OK (3 30 yes 25)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(3 30 yes 25)", &o, &n);
}

// ---------------------------------------------------------------------------
// Iterator / generator patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_closure_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a closure-based iterator over a range
    let form = "(let ((make-range-iter
                       (lambda (start end)
                         (let ((current start))
                           (lambda ()
                             (if (> current end) nil
                               (prog1 current
                                 (setq current (1+ current)))))))))
                  (let ((iter (funcall make-range-iter 1 5))
                        (result nil))
                    (let ((val (funcall iter)))
                      (while val
                        (setq result (cons val result))
                        (setq val (funcall iter))))
                    (nreverse result)))";
    let expect = expect_test::expect![[r#""OK (1 2 3 4 5)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 2 3 4 5)", &o, &n);
}

// ---------------------------------------------------------------------------
// Observer / event patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_observer_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Observer pattern: register callbacks, fire events
    let form = "(let ((listeners nil)
                      (event-log nil))
                  (let ((add-listener
                         (lambda (fn)
                           (setq listeners (cons fn listeners))))
                        (fire
                         (lambda (event)
                           (dolist (fn listeners)
                             (funcall fn event)))))
                    (funcall add-listener
                             (lambda (e)
                               (setq event-log
                                     (cons (list 'A e) event-log))))
                    (funcall add-listener
                             (lambda (e)
                               (setq event-log
                                     (cons (list 'B e) event-log))))
                    (funcall fire 'click)
                    (funcall fire 'hover)
                    (nreverse event-log)))";
    let expect = expect_test::expect![[r#""OK ((B click) (A click) (B hover) (A hover))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Association list as environment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_env_chain_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Scoped environment lookup (inner shadows outer)
    let form = "(let ((lookup (lambda (key env)
                                (let ((binding (assq key env)))
                                  (if binding (cdr binding) nil)))))
                  (let ((outer '((x . 1) (y . 2)))
                        (inner '((x . 10) (z . 30))))
                    (let ((env (append inner outer)))
                      (list (funcall lookup 'x env)
                            (funcall lookup 'y env)
                            (funcall lookup 'z env)
                            (funcall lookup 'w env)))))";
    let expect = expect_test::expect![[r#""OK (10 2 30 nil)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(10 2 30 nil)", &o, &n);
}

// ---------------------------------------------------------------------------
// Matrix operations via nested lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_matrix_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  (fset 'neovm--test-transpose
    (lambda (matrix)
      (if (null (car matrix)) nil
        (cons (mapcar 'car matrix)
              (funcall 'neovm--test-transpose
                       (mapcar 'cdr matrix))))))
  (unwind-protect
      (funcall 'neovm--test-transpose
               '((1 2 3) (4 5 6) (7 8 9)))
    (fmakunbound 'neovm--test-transpose)))";
    let expect = expect_test::expect![[r#""OK ((1 4 7) (2 5 8) (3 6 9))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("((1 4 7) (2 5 8) (3 6 9))", &o, &n);
}

#[test]
fn oracle_prop_adv_matrix_multiply_row_col() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dot product of two vectors
    let form = "(let ((dot (lambda (a b)
                              (let ((sum 0))
                                (while (and a b)
                                  (setq sum (+ sum (* (car a) (car b)))
                                        a (cdr a)
                                        b (cdr b)))
                                sum))))
                  (funcall dot '(1 2 3) '(4 5 6)))";
    let expect = expect_test::expect![[r#""OK 32""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("32", &o, &n);
}

// ---------------------------------------------------------------------------
// Complex string processing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple word tokenizer via buffer operations
    let form = r#"(with-temp-buffer
                    (insert "hello world foo bar")
                    (goto-char (point-min))
                    (let ((words nil))
                      (while (re-search-forward "\\b\\w+\\b" nil t)
                        (setq words (cons (match-string 0) words)))
                      (nreverse words)))"#;
    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\" \"foo\" \"bar\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trampolining to avoid stack overflow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_trampoline_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trampoline: return thunks to avoid deep recursion
    let form = "(let ((trampoline
                       (lambda (fn)
                         (let ((result (funcall fn)))
                           (while (functionp result)
                             (setq result (funcall result)))
                           result))))
                  (fset 'neovm--test-tramp-sum
                    (lambda (n acc)
                      (if (<= n 0) acc
                        (lambda ()
                          (funcall 'neovm--test-tramp-sum
                                   (1- n) (+ acc n))))))
                  (unwind-protect
                      (funcall trampoline
                               (lambda ()
                                 (funcall 'neovm--test-tramp-sum
                                          100 0)))
                    (fmakunbound 'neovm--test-tramp-sum)))";
    let expect = expect_test::expect![[r#""OK 5050""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("5050", &o, &n);
}

// ---------------------------------------------------------------------------
// Continuation-passing style
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_cps_factorial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  (fset 'neovm--test-cps-fact
    (lambda (n k)
      (if (<= n 1)
          (funcall k 1)
        (funcall 'neovm--test-cps-fact
                 (1- n)
                 (lambda (result)
                   (funcall k (* n result)))))))
  (unwind-protect
      (funcall 'neovm--test-cps-fact
               10
               (lambda (x) x))
    (fmakunbound 'neovm--test-cps-fact)))";
    let expect = expect_test::expect![[r#""OK 3628800""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("3628800", &o, &n);
}

// ---------------------------------------------------------------------------
// Church encoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adv_church_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church encoding of natural numbers
    let form = "(let ((zero (lambda (f) (lambda (x) x)))
                      (succ (lambda (n)
                              (lambda (f)
                                (lambda (x)
                                  (funcall f
                                    (funcall (funcall n f) x))))))
                      (to-int (lambda (n)
                                (funcall (funcall n '1+) 0))))
                  (let* ((one (funcall succ zero))
                         (two (funcall succ one))
                         (three (funcall succ two)))
                    (list (funcall to-int zero)
                          (funcall to-int one)
                          (funcall to-int two)
                          (funcall to-int three))))";
    let expect = expect_test::expect![[r#""OK (0 1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(0 1 2 3)", &o, &n);
}

// ---------------------------------------------------------------------------
// Proptest
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_adv_pipeline_proptest(
        a in -50i64..50i64,
        b in -50i64..50i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((pipeline
                    (lambda (fns val)
                      (let ((result val))
                        (dolist (f fns result)
                          (setq result (funcall f result)))))))
               (funcall pipeline
                        (list (lambda (x) (+ x {}))
                              (lambda (x) (* x 2)))
                        {}))",
            a, b
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
