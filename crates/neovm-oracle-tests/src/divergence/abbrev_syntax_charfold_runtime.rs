//! Abbrev (define-abbrev :count/:case-fixed/hook with no-self-insert,
//! abbrev-table-get/put, :enable-function), syntax (string-to-syntax,
//! syntax-after/class-to-char, matching-paren, syntax-table text property),
//! pcase or/and/guard/seq/map deep; plus the char-fold accent-folding divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn ab2_abbrev_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"expanded\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((fired nil))
  (define-abbrev-table 'neo-at4-xyz '())
  (defun neo-ab-hook-xyz () (setq fired t))
  (put 'neo-ab-hook-xyz 'no-self-insert t)
  (define-abbrev neo-at4-xyz "x" "expanded" 'neo-ab-hook-xyz)
  (with-temp-buffer
    (setq local-abbrev-table neo-at4-xyz) (abbrev-mode 1)
    (insert "x") (expand-abbrev)
    (list (buffer-string) fired)))"##,
        expect,
    );
}

#[test]
fn ab2_abbrev_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"foobar\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(define-abbrev-table 'neo-at3-xyz '())
(define-abbrev neo-at3-xyz "foo" "foobar" nil :count 0 :case-fixed t)
(list (abbrev-expansion "foo" neo-at3-xyz)
      (abbrev-get (intern "foo" neo-at3-xyz) :case-fixed)
      (abbrev-table-p neo-at3-xyz))"##,
        expect,
    );
}

#[test]
fn ab2_abbrev_table_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (ignore 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(define-abbrev-table 'neo-at5-xyz '() "doc" :enable-function #'ignore)
(list (abbrev-table-get neo-at5-xyz :enable-function)
      (progn (abbrev-table-put neo-at5-xyz :custom-prop 42)
             (abbrev-table-get neo-at5-xyz :custom-prop)))"##,
        expect,
    );
}

#[test]
fn ab2_matching_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (41 40 93 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (matching-paren ?\() (matching-paren ?\)) (matching-paren ?\[)
        (matching-paren ?a))"##,
        expect,
    );
}

#[test]
fn ab2_pcase_or_and_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (matched (1 2) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (pcase 5 ((or 1 2 5) 'matched) (_ 'no))
      (pcase '(1 . 2) ((and `(,a . ,b) (guard (= a 1))) (list a b)) (_ 'no))
      (pcase "test" ((and (pred stringp) s (guard (> (length s) 2))) (length s))))"##,
        expect,
    );
}

#[test]
fn ab2_pcase_seq_map_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Unknown map pattern: (map x y)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (pcase '(1 2 3 4) ((seq a b &rest r) (list a b r)))
      (pcase '((x . 1) (y . 2)) ((map x y) (list x y))))"##,
        expect,
    );
}

#[test]
fn ab2_string_to_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((2) (0) (4 . 41) (7) 119)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-to-syntax "w") (string-to-syntax "-")
        (string-to-syntax "()") (string-to-syntax "\"") (syntax-class-to-char 2))"##,
        expect,
    );
}

#[test]
fn ab2_syntax_after_before() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (40 41 40)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(foo)")
  (list (syntax-class-to-char (car (syntax-after 1)))
        (syntax-class-to-char (car (syntax-after 5)))
        (char-syntax ?\()))"##,
        expect,
    );
}

#[test]
fn ab2_syntax_propertize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (34 (2) (1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc\"def\"ghi")
  (put-text-property 4 9 'syntax-table (string-to-syntax "|"))
  (list (syntax-class-to-char (car (syntax-after 4)))
        (string-to-syntax "w") (string-to-syntax ".")))"##,
        expect,
    );
}

#[test]
fn divergence_char_fold_accent_incomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'char-fold)
(let ((case-fold-search t))
  (list (with-temp-buffer (insert "naïve") (goto-char (point-min))
          (and (re-search-forward (char-fold-to-regexp "naive") nil t) t))
        (with-temp-buffer (insert "café") (goto-char (point-min))
          (and (re-search-forward (char-fold-to-regexp "cafe") nil t) t))
        (with-temp-buffer (insert "Renée") (goto-char (point-min))
          (and (re-search-forward (char-fold-to-regexp "renee") nil t) t))))"##,
        expect,
    );
}
