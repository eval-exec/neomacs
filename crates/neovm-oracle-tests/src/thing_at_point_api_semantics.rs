//! Oracle parity tests for GNU `thingatpt.el` public API semantics.
//!
//! These tests exercise direct `thing-at-point`, `bounds-of-thing-at-point`,
//! provider alist dispatch, and NO-PROPERTIES behavior rather than only the
//! lower-level movement primitives used to implement things.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_thing_at_point_word_symbol_and_line_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'thingatpt)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "alpha-beta gamma_delta\nsecond line\n")
    (goto-char (+ (point-min) 2))
    (let ((word1 (thing-at-point 'word t))
          (symbol1 (thing-at-point 'symbol t))
          (bounds1 (bounds-of-thing-at-point 'word)))
      (search-forward "gamma")
      (let ((word2 (thing-at-point 'word t))
            (symbol2 (thing-at-point 'symbol t)))
        (forward-line 1)
        (let ((line (thing-at-point 'line t))
              (line-bounds (bounds-of-thing-at-point 'line)))
          (list word1 symbol1 bounds1 word2 symbol2 line line-bounds))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"alpha\" \"alpha-beta\" (1 . 6) \"gamma\" \"gamma_delta\" \"second line\\n\" (24 . 36))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_thing_at_point_sexp_list_and_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'thingatpt)
  (list
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "(outer (inner 1 2) tail)")
     (search-forward "inner")
     (list
      (thing-at-point 'symbol t)
      (thing-at-point 'sexp t)
      (bounds-of-thing-at-point 'list)))
   (with-temp-buffer
     (list
      (thing-at-point 'word t)
      (bounds-of-thing-at-point 'word)
      (bounds-of-thing-at-point 'whitespace)))))
"#;

    let expect = expect_test::expect![[r#""ERR (search-failed \"inner\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_thing_at_point_no_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'thingatpt)
  (with-temp-buffer
    (insert (propertize "colored" 'face 'bold 'oracle-prop 17))
    (goto-char (+ (point-min) 2))
    (let ((with-props (thing-at-point 'word nil))
          (without-props (thing-at-point 'word t)))
      (list
       with-props
       (text-properties-at 0 with-props)
       without-props
       (text-properties-at 0 without-props)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"colored\" 0 7 (face bold oracle-prop 17)) (face bold oracle-prop 17) \"colored\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_thing_at_point_provider_alists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'thingatpt)
  (with-temp-buffer
    (insert "abc def ghi")
    (goto-char (+ (point-min) 4))
    (let ((thing-at-point-provider-alist
           '((oracle . (lambda () "provider-text"))))
          (bounds-of-thing-at-point-provider-alist
           '((oracle-bounds . (lambda () (cons 5 8)))))
          (forward-thing-provider-alist
           '((oracle-forward . (lambda (backward)
                                 (if backward
                                     (goto-char (point-min))
                                   (goto-char (point-max)))))
             (oracle-forward . (lambda (_backward)
                                 (goto-char (+ (point-min) 2)))))))
      (list
       (thing-at-point 'oracle t)
       (bounds-of-thing-at-point 'oracle-bounds)
       (progn (forward-thing 'oracle-forward 1) (point))
       (progn (forward-thing 'oracle-forward -1) (point))))))
"#;

    let expect = expect_test::expect![[r#""OK (\"provider-text\" (5 . 8) 3 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
