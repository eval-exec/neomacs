//! thing-at-point (url/email/word/number/sexp/line/sentence + bounds),
//! replace-regexp-in-string (fixedcase/literal/case-preserve/subexp/\&) and
//! subst-char-in-string in-place, cl-labels/flet/loop-exotic/macrolet/
//! destructuring-bind, pcase app/cl-type/seq, and compose/decompose-string.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn bounds_of_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 . 6) (1 . 6))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world-foo")
  (goto-char 3)
  (list (bounds-of-thing-at-point 'word) (bounds-of-thing-at-point 'symbol)))"##,
        expect,
    );
}

#[test]
fn tap_line_sentence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"First sentence here. Second one too.\\n\" \"First sentence here. Second one too.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "First sentence here. Second one too.\nNext line.")
  (goto-char 5)
  (list (thing-at-point 'line t) (thing-at-point 'sentence t)))"##,
        expect,
    );
}

#[test]
fn tap_number_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"foo\" \"foo\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(foo 42 (bar 3.14))")
  (goto-char 6) (list (thing-at-point 'number) (number-at-point))
  (goto-char 2) (list (thing-at-point 'symbol t) (thing-at-point 'sexp t)))"##,
        expect,
    );
}

#[test]
fn tap_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"https://example.com\" \"foo@bar.org\" \"Visit\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "Visit https://example.com or mail foo@bar.org now.")
  (goto-char 8)
  (list (thing-at-point 'url t) (progn (goto-char 38) (thing-at-point 'email t))
        (progn (goto-char 1) (thing-at-point 'word t))))"##,
        expect,
    );
}

#[test]
fn replace_case_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Dog DOG dog\" \"WORLD\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-replace t) (case-fold-search t))
  (list (replace-regexp-in-string "cat" "dog" "Cat CAT cat")
        (replace-regexp-in-string "hello" "world" "HELLO")))"##,
        expect,
    );
}

#[test]
fn replace_fixedcase_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"BAR BAR BAR\" \"bar\" \"\\\\&\\\\&\\\\&\\\\&\\\\&\\\\&\" \"X\\\\\\\\&Y\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (replace-regexp-in-string "foo" "BAR" "foo Foo FOO")
        (replace-regexp-in-string "foo" "bar" "Foo" t)
        (replace-regexp-in-string "a" "\\\\&\\\\&" "aaa")
        (replace-regexp-in-string "a" "X\\\\&Y" "a" nil t))"##,
        expect,
    );
}

#[test]
fn replace_subexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"5:x 10:y\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(replace-regexp-in-string "\\([a-z]+\\)=\\([0-9]+\\)" "\\2:\\1" "x=5 y=10")"##,
        expect,
    );
}

#[test]
fn subst_char_inplace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a_b_c\" \"a_b_c\" \"a_b_c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (copy-sequence "a-b-c")))
  (list (subst-char-in-string ?- ?_ s) (subst-char-in-string ?- ?_ s t) s))"##,
        expect,
    );
}

#[test]
fn category_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (ascii #(\"abc\" 0 3 (composition ((3 . \"x\")))) 4 (97 98))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-charset ?a)
        (compose-string "abc" 0 3 "x")
        (length (compose-string "test"))
        (string-to-list (decompose-string (compose-string "ab"))))"##,
        expect,
    );
}

#[test]
fn cl_destructuring_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (error \"Malformed argument list ends with: (&rest e)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-destructuring-bind (a (b c) &key d &rest e) '(1 (2 3) :d 4 5 6)
  (list a b c d e))"##,
        expect,
    );
}

#[test]
fn cl_labels_flet() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (120 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-labels ((fact (n) (if (= n 0) 1 (* n (fact (1- n)))))) (fact 5))
      (cl-flet ((dbl (x) (* 2 x)) (inc (x) (1+ x))) (dbl (inc 5))))"##,
        expect,
    );
}

#[test]
fn cl_loop_exotic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 3 5 7 9) 6 ((a 1) (b 2)) (1 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (cl-loop for i from 1 to 10 by 2 collect i)
      (cl-loop for x across [1 2 3] sum x)
      (cl-loop for (k . v) in '((a . 1) (b . 2)) collect (list k v))
      (cl-loop for i below 5 when (cl-oddp i) collect i into odds finally return odds))"##,
        expect,
    );
}

#[test]
fn cl_macrolet_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 25""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-macrolet ((sq (x) `(* ,x ,x)))
  (cl-symbol-macrolet ((five 5)) (sq five)))"##,
        expect,
    );
}

#[test]
fn pcase_app_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (error \"Unknown cl-type pattern: (cl-type string)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (pcase 10 ((app 1+ 11) 'matched) (_ 'no))
      (pcase "hi" ((cl-type string) 'str) (_ 'no))
      (pcase '(1 2 3) ((seq a b c) (+ a b c))))"##,
        expect,
    );
}
