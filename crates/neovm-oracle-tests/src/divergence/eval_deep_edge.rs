//! Divergence tests: deep eval edge cases, throw/catch, save-excursion, match data.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_catch_throw_with_nested_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cleanup)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((result nil))
  (catch 'done
    (unwind-protect
        (throw 'done 42)
      (push 'cleanup result)))
  result)"#,
        expect,
    );
}

#[test]
fn divergence_throw_across_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(catch 'outer
  (condition-case err
      (throw 'outer 99)
    (error (list 'caught err))))"#,
        expect,
    );
}

#[test]
fn divergence_save_excursion_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"in other buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf1 (current-buffer)))
  (save-excursion
    (set-buffer (get-buffer-create " *se-test*"))
    (insert "in other buffer")
    (set-buffer buf1))
  (list (eq (current-buffer) buf1)
        (with-current-buffer " *se-test*"
          (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_save_restriction_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (narrow-to-region 3 7)
  (save-restriction
    (widen)
    (narrow-to-region 1 4)
    (list (point-min) (point-max)))
  (list (point-min) (point-max)))"#,
        expect,
    );
}

#[test]
fn divergence_save_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"28.2\" (8 12 8 10) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (string-match "\\([0-9]+\\)\\.[0-9]+" "version 28.2")
  (let ((m (match-data)))
    (save-match-data
      (string-match "foo" "barfoo"))
      (list (match-string 0 "version 28.2")
            m
            (eq m (match-data)))))"#,
        expect,
    );
}

#[test]
fn divergence_match_data_across_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"123\" 4 7 nil \"abcNUMdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abc123def")
  (goto-char 1)
  (re-search-forward "[0-9]+")
  (list (match-string 0)
        (match-beginning 0)
        (match-end 0)
        (replace-match "NUM")
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_dynamic_binding_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-dyn-cross 0)
  (let ((my-dyn-cross 42))
    (with-current-buffer (get-buffer-create " *dyn-cross*")
      (list my-dyn-cross
            (default-value 'my-dyn-cross)))))"#,
        expect,
    );
}

#[test]
fn divergence_deep_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (done done)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-recurse (n)
    (if (<= n 0) 'done (my-recurse (1- n))))
  (list (my-recurse 100)
        (my-recurse 500)))"#,
        expect,
    );
}

#[test]
fn divergence_mutual_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-even? (n) (if (= n 0) t (my-odd? (1- n))))
  (defun my-odd? (n) (if (= n 0) nil (my-even? (1- n))))
  (list (my-even? 10) (my-odd? 10)
        (my-even? 11) (my-odd? 11)))"#,
        expect,
    );
}

#[test]
fn divergence_defmacro_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (if t (progn (list 1 2 3)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro my-when (cond &rest body)
    (list 'if cond (cons 'progn body)))
  (macroexpand '(my-when t (list 1 2 3))))"#,
        expect,
    );
}

#[test]
fn divergence_apply_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 13 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((add3 (apply-partially #'+ 3)))
  (list (funcall add3 4)
        (funcall add3 10)
        (funcall add3 0)))"#,
        expect,
    );
}

#[test]
fn divergence_compose_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((double (lambda (x) (* x 2)))
        (inc (lambda (x) (1+ x)))
        (compose (lambda (f g) (lambda (x) (funcall f (funcall g x))))))
  (list (funcall (funcall compose double inc) 5)
        (funcall (funcall compose inc double) 5)))"#,
        expect,
    );
}
