//! Oracle parity tests for GNU `text-property-search` semantics.
//!
//! GNU implements these APIs in `lisp/emacs-lisp/text-property-search.el` as
//! Lisp over `next-single-property-change`/`previous-single-property-change`.
//! The important compatibility points are predicate interpretation, returned
//! `prop-match` structure fields, `not-current`, and point movement.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_text_property_search_forward_distinct_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'text-property-search)
  (with-temp-buffer
    (insert "aaa bbb ccc ddd")
    (add-text-properties 1 4 '(face bold))
    (add-text-properties 5 8 '(face italic))
    (add-text-properties 9 12 '(face italic))
    (goto-char (point-min))
    (let (out)
      (while-let ((match (text-property-search-forward 'face)))
        (push (list (prop-match-beginning match)
                    (prop-match-end match)
                    (prop-match-value match)
                    (point))
              out))
      (nreverse out))))
"#;

    let expect = expect_test::expect![[r#""OK ((1 4 bold 4) (5 8 italic 8) (9 12 italic 12))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_property_search_value_and_predicate_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'text-property-search)
  (with-temp-buffer
    (insert "zero one two three")
    (add-text-properties 1 5 '(token number))
    (add-text-properties 6 9 '(token word))
    (add-text-properties 10 13 '(token word))
    (add-text-properties 14 19 '(token symbol))
    (let ((equal-word nil)
          (not-word nil)
          (member-token nil))
      (goto-char (point-min))
      (setq equal-word (text-property-search-forward 'token 'word t))
      (goto-char (point-min))
      (setq not-word (text-property-search-forward 'token 'word nil))
      (goto-char (point-min))
      (setq member-token
            (text-property-search-forward
             'token '(word symbol)
             (lambda (want actual) (memq actual want))))
      (list
       (list (prop-match-beginning equal-word)
             (prop-match-end equal-word)
             (prop-match-value equal-word))
       (list (prop-match-beginning not-word)
             (prop-match-end not-word)
             (prop-match-value not-word))
       (list (prop-match-beginning member-token)
             (prop-match-end member-token)
             (prop-match-value member-token))))))
"#;

    let expect = expect_test::expect![[r#""OK ((6 9 word) (1 6 number) (6 9 word))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_property_search_backward_and_not_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'text-property-search)
  (with-temp-buffer
    (insert "alpha beta gamma delta")
    (add-text-properties 1 6 '(field prompt))
    (add-text-properties 7 11 '(field input))
    (add-text-properties 12 17 '(field input))
    (add-text-properties 18 23 '(field prompt))
    (goto-char 15)
    (let ((current (text-property-search-backward 'field 'input t))
          skipped)
      (goto-char 15)
      (setq skipped (text-property-search-backward 'field 'input t t))
      (list
       (list (prop-match-beginning current)
             (prop-match-end current)
             (prop-match-value current)
             (point))
       (and skipped
            (list (prop-match-beginning skipped)
                  (prop-match-end skipped)
                  (prop-match-value skipped)
                  (point)))))))
"#;

    let expect = expect_test::expect![[r#""OK ((12 15 input 7) (7 11 input 7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_property_search_miss_restores_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'text-property-search)
  (with-temp-buffer
    (insert "plain text")
    (goto-char 4)
    (let ((forward (text-property-search-forward 'face 'bold t))
          (after-forward (point))
          backward after-backward)
      (goto-char 7)
      (setq backward (text-property-search-backward 'face 'bold t))
      (setq after-backward (point))
      (list forward after-forward backward after-backward))))
"#;

    let expect = expect_test::expect![[r#""OK (nil 4 nil 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
