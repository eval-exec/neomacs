//! Oracle parity tests for core special-form semantics that are easy to miss:
//! `quote`, `function`, `defconst`, `save-current-buffer`, `interactive`, and `inline`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::eval_oracle_and_neovm;

#[test]
fn oracle_prop_special_forms_semantics_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (quote (a b c))
  '(1 . 2)
  (eq 'foo (quote foo))
  (equal '(1 (2 3) "x") (quote (1 (2 3) "x")))
  (quote nil)
  (quote t))"#;
    let expect = expect_test::expect![[r#""OK ((a b c) (1 . 2) t t nil t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_special_forms_semantics_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (funcall (function (lambda (x) (+ x 1))) 41)
  (funcall (function car) '(9 8 7))
  (functionp (function car))
  (function 1)
  (function '(1 2 3)))"#;
    let expect = expect_test::expect![[r#""OK (42 9 t 1 '(1 2 3))""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_special_forms_semantics_defconst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (makunbound 'neovm--oracle-special-defconst)
  (let ((first (progn
                 (defconst neovm--oracle-special-defconst 10 "doc")
                 neovm--oracle-special-defconst))
        (second (progn
                  (defconst neovm--oracle-special-defconst 20 "doc2")
                  neovm--oracle-special-defconst))
        (is-bound (boundp 'neovm--oracle-special-defconst)))
    (makunbound 'neovm--oracle-special-defconst)
    (list first second is-bound)))"#;
    let expect = expect_test::expect![[r#""OK (10 20 t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_special_forms_semantics_save_current_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((orig (current-buffer))
      (a (generate-new-buffer " *neovm-oracle-scb-a*"))
      (b (generate-new-buffer " *neovm-oracle-scb-b*")))
  (unwind-protect
      (progn
        (set-buffer a)
        (insert "A")
        (set-buffer b)
        (insert "B")
        (set-buffer orig)
        (list
         (save-current-buffer
           (set-buffer a)
           (list (eq (current-buffer) a)
                 (buffer-string)))
         (eq (current-buffer) orig)
         (save-current-buffer
           (set-buffer b)
           (insert "!")
           (buffer-string))
         (with-current-buffer b (buffer-string))
         (eq (current-buffer) orig)))
    (kill-buffer a)
    (kill-buffer b)))"#;
    let expect = expect_test::expect![[r#""OK ((t \"A\") t \"B!\" \"B!\" t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_special_forms_semantics_interactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--oracle-interactive-cmd
        (lambda (n)
          (interactive "p")
          n))
  (let ((result
         (list
          (commandp 'neovm--oracle-interactive-cmd)
          (interactive-form 'neovm--oracle-interactive-cmd)
          (funcall 'neovm--oracle-interactive-cmd 7))))
    (fmakunbound 'neovm--oracle-interactive-cmd)
    result))"#;
    let expect = expect_test::expect![[r#""OK (t (interactive \"p\") 7)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_special_forms_semantics_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (condition-case err
      (progn (inline foo) 'ok)
    (error (list (car err) (cadr err))))
  (inline 'foo)
  (progn
    (defvar foo 1)
    (inline foo)
    'ok))"#;
    let expect = expect_test::expect![[r#""OK ((void-variable foo) foo ok)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_special_forms_semantics_progn_prog1_eval_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (list
   (progn)
   (prog1 (progn (push 'first log) 'value)
     (push 'second log)
     (push 'third log))
   (nreverse log)
   (condition-case err
       (eval '(progn . bad-tail))
     (error (list (car err) (cdr err))))
   (condition-case err
       (eval '(prog1))
     (error (list (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil value (first second third) (wrong-type-argument (listp bad-tail)) (wrong-number-of-arguments (prog1 0)))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_special_forms_semantics_quote_function_arity_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (eval '(quote a b))
   (error (list (car err) (cdr err))))
 (condition-case err
     (eval '(quote))
   (error (list (car err) (cdr err))))
 (condition-case err
     (eval '(function (lambda () t) extra))
   (error (list (car err) (cdr err))))
 (condition-case err
     (eval '(function))
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments '2) (wrong-number-of-arguments '0) (wrong-number-of-arguments #'2) (wrong-number-of-arguments #'0))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_eval_lexical_environment_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (eval 'x '((x . 42)))
 (condition-case err
     (eval 'x nil)
   (error (list (car err) (cdr err))))
 (eval '(let ((x 1))
          (let ((f (lambda () x)))
            (let ((x 2))
              (funcall f))))
       t)
 (eval '(let ((x 1))
          (let ((f (lambda () x)))
            (let ((x 2))
              (funcall f))))
       nil))
"#;
    let expect = expect_test::expect![[r#""OK (42 (void-variable (x)) 1 2)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}

#[test]
fn oracle_prop_eval_invalid_function_position_does_not_evaluate_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (list
   (condition-case err
       (eval '((prog1 (lambda (x) x) (push 'fun log))
               (prog1 1 (push 'arg log))))
     (error (list (car err) (cdr err))))
   log))
"#;
    let expect = expect_test::expect![[
        r#""OK ((invalid-function ((prog1 (lambda (x) x) (push 'fun log)))) nil)""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "oracle parity mismatch for form: {form}");
}
