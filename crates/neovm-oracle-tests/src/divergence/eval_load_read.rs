//! Divergence tests: eval-region, eval-buffer, eval-defun, loaddefs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eval_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((result nil))
  (with-temp-buffer
    (insert "(setq my-eval-test 42)")
    (eval-region (point-min) (point-max)))
  my-eval-test)"#,
        expect,
    );
}

#[test]
fn divergence_eval_buffer_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "(+ 1 2 3)")
  (eval-buffer (current-buffer)))"#,
        expect,
    );
}

#[test]
fn divergence_eval_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (25 100)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-eval-defun-test (x) (* x x))
  (list (my-eval-defun-test 5)
        (my-eval-defun-test 10)))"#,
        expect,
    );
}

#[test]
fn divergence_eval_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 (1 2 3) t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (eval '(+ 1 2))
  (eval '(list 1 2 3))
  (eval 't)
  (eval 'nil))"#,
        expect,
    );
}

#[test]
fn divergence_eval_lexical_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 42))
  (list (eval 'x)
        (eval 'x t)
        (let ((x 99))
          (eval 'x))))"#,
        expect,
    );
}

#[test]
fn divergence_load_suffixes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (\".elc\" \".el\") (\".el\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (listp load-suffixes)
  (member ".elc" load-suffixes)
  (member ".el" load-suffixes)
  (listp load-file-rep-suffixes))"#,
        expect,
    );
}

#[test]
fn divergence_load_source_file_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'load-file)
  (fboundp 'load-library)
  (fboundp 'locate-library)
  (stringp (locate-library "subr")))"#,
        expect,
    );
}

#[test]
fn divergence_read_from_string_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((a b c) . 7) ((d e) . 13) (a b c) 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "(a b c) (d e)" 0)
  (read-from-string "(a b c) (d e)" 8)
  (car (read-from-string "(a b c) (d e)" 0))
  (cdr (read-from-string "(a b c) (d e)" 0)))"#,
        expect,
    );
}

#[test]
fn divergence_read_multiple_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b) (c d) (e f))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((str "(a b) (c d) (e f)")
        (p1 (read-from-string str 0))
        (p2 (read-from-string str (cdr p1)))
        (p3 (read-from-string str (cdr p2))))
  (list (car p1) (car p2) (car p3)))"#,
        expect,
    );
}

#[test]
fn divergence_standard_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'read-from-minibuffer)
  (fboundp 'read-string)
  (fboundp 'read-number)
  (fboundp 'read-regexp))"#,
        expect,
    );
}
