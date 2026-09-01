//! Divergence tests: edge cases with boundary values, nil, empty, limits.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_empty_string_boundary_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t 0 t 0 0 t \"\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (string= \"\" \"\")
  (string< \"\" \"a\")
  (not (string< \"a\" \"\"))
  (length \"\")
  (string-empty-p \"\")
  (string-blank-p \"\")
  (string-blank-p \"   \")
  (not (string-empty-p \"a\"))
  (substring \"hello\" 0 0)
  (string= (substring \"hello\" 0 0) \"\")) ",
        expect,
    );
}

#[test]
fn divergence_nil_args_to_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (0 nil (1 2) (1 2) nil nil nil nil nil nil nil nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (length nil)
  (append nil nil)
  (append nil '(1 2))
  (append '(1 2) nil)
  (nreverse nil)
  (reverse nil)
  (car nil)
  (cdr nil)
  (nth 0 nil)
  (assoc 'x nil)
  (member 1 nil)
  (mapcar #'1+ nil)
  (concat)) ",
        expect,
    );
}

#[test]
fn divergence_numeric_boundary_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (= (expt 2 63) 9223372036854775808)
  (< most-negative-fixnum 0)
  (> most-positive-fixnum 0)
  (= (+ 1 most-positive-fixnum) (1+ most-positive-fixnum))
  (<= most-negative-fixnum 0)
  (= (abs most-negative-fixnum) (- most-negative-fixnum))
  (zerop 0)
  (not (zerop 1))
  (not (zerop -1))
  (= (max 1 2 3) 3)
  (= (min -1 -2 -3) -3)) ",
        expect,
    );
}

#[test]
fn divergence_buffer_empty_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (= (point-min) 1)
  (= (point-max) 1)
  (= (buffer-size) 0)
  (eobp)
  (bobp)
  (string= (buffer-string) \"\")
  (= (line-number-at-pos) 1)
  (string= (buffer-name) \"*scratch*\")
  (buffer-modified-p)) ",
        expect,
    );
}

#[test]
fn divergence_string_multibyte_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((empty \"\")
        (single \"x\")
        (mb \"\\u4e16\"))
  (list (= (length empty) 0)
        (= (string-bytes empty) 0)
        (= (length single) 1)
        (= (string-bytes single) 1)
        (= (length mb) 1)
        (>= (string-bytes mb) 2)
        (string= (substring mb 0 0) empty)
        (string= (substring mb 0 1) mb)
        (multibyte-string-p mb)
        (not (multibyte-string-p single)))) ",
        expect,
    );
}

#[test]
fn divergence_symbol_nil_t_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (null nil)
  (not nil)
  (null t)
  (not t)
  (booleanp nil)
  (booleanp t)
  (not (booleanp 0))
  (symbolp nil)
  (symbolp t)
  (eq nil nil)
  (eq t t)
  (not (eq nil t))) ",
        expect,
    );
}

#[test]
fn divergence_regex_empty_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"abc\")
  (goto-char 1)
  (let ((r1 (re-search-forward \"\\\\(\")))
    (let ((r2 (re-search-forward \"\\\\)\" nil t)))
      (list r1 r2
            (match-beginning 0) (match-end 0)
            (match-string 0)
            (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_hash_table_empty_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t t t t nil equal)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((ht (make-hash-table :test 'equal)))
  (list (hash-table-count ht)
        (= (hash-table-count ht) 0)
        (not (gethash \"key\" ht))
        (eq (gethash \"key\" ht nil) nil)
        (eq (gethash \"key\" ht 'missing) 'missing)
        (maphash (lambda (k v) (list k v)) ht)
        (hash-table-test ht))) ",
        expect,
    );
}

#[test]
fn divergence_vector_empty_and_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 1 42 t 1 2 [42] [42 1 2] nil (42) t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((v0 [])
        (v1 [42])
        (v2 [1 2]))
  (list (length v0)
        (length v1)
        (aref v1 0)
        (= (aref v1 0) 42)
        (aref v2 0) (aref v2 1)
        (vconcat v0 v1)
        (vconcat v1 v2)
        (append v0 nil)
        (append v1 nil)
        (equal (append v1 nil) '(42)))) ",
        expect,
    );
}

#[test]
fn divergence_char_boundary_codepoints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (= ?A 65)
  (= ?a 97)
  (= ?0 48)
  (= ?\\n 10)
  (= ?\\t 9)
  (= ?  32)
  (char-or-string-p ?A)
  (char-or-string-p \"A\")
  (not (char-or-string-p 65))
  (= (char-after ?A) 65)) ",
        expect,
    );
}
