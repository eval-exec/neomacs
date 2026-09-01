//! Divergence tests: macro expansion stress - defmacro*, cl-macrolet, nested.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_defmacro_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro my-swap (a b)
    (let ((tmp (make-symbol "tmp")))
      (list 'let (list (list tmp a))
            (list 'setq a b)
            (list 'setq b tmp))))
  (let ((x 1) (y 2))
    (my-swap x y)
    (list x y)))"#,
        expect,
    );
}

#[test]
fn divergence_defmacro_nested_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro my-add1 (x) (list '+ x 1))
  (defmacro my-add2 (x) (list 'my-add1 (list 'my-add1 x)))
  (my-add2 5))"#,
        expect,
    );
}

#[test]
fn divergence_macro_expansion_and_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 43""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro my-let1 (var val &rest body)
    (list 'let (list (list var val)) (cons 'progn body)))
  (macroexpand '(my-let1 x 42 (+ x 1)))
  (my-let1 x 42 (+ x 1)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_macro_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((setq x (+ x 1)) (setq x (- x 3)) (if (memql 42 lst) (with-no-warnings lst) (setq lst (cons 42 lst))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (macroexpand '(cl-incf x))
  (macroexpand '(cl-decf x 3))
  (macroexpand '(cl-pushnew 42 lst)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(let ((form (macroexpand '(cl-loop for i below 3 collect i))))
  (list (consp form)
        (eq (car form) 'cl-block)))"#,
        expect,
    );
}

#[test]
fn divergence_pcase_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Unknown list pattern: (list a b)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'pcase)
(list
  (pcase 42
    (1 'one)
    (42 'forty-two)
    (_ 'other))
  (pcase '(1 2)
    ((list a b) (+ a b))
    (_ 0)))"#,
        expect,
    );
}

#[test]
fn divergence_pcase_guard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable it)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'pcase)
(list
  (pcase 5
    ((and (pred integerp) (guard (> it 3))) 'big)
    (_ 'small))
  (pcase "hello"
    ((pred stringp) 'string)
    (_ 'other)))"#,
        expect,
    );
}

#[test]
fn divergence_pcase_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unknown string pattern: (string \\\"hello \\\" (let rest (rx (+ any))))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'pcase)
(pcase "hello world"
  ((string "hello " (let rest (rx (+ any))))
   rest)
  (_ 'no-match))"#,
        expect,
    );
}

#[test]
fn divergence_threading_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"thread-first\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'thread-first)
(list
  (thread-first 5
    (+ 3)
    (* 2)
    (- 1))
  (thread-last '(1 2 3)
    (mapcar #'1+)
    (-filter (lambda (x) (> x 2))))))"#,
        expect,
    );
}

#[test]
fn deficiency_compare_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (compare-strings "abc" 0 3 "abc" 0 3)
  (compare-strings "abc" 0 3 "ABC" 0 3)
  (compare-strings "abc" 0 3 "ABC" 0 3 t))"#,
        expect,
    );
}
