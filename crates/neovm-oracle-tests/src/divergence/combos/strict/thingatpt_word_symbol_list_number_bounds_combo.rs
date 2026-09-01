//! Strict combo oracle probes, batch 232: thingatpt. thing-at-point over
//! word/symbol/number/list/line/whitespace, bounds-of-thing-at-point,
//! beginning/end-of-thing, and forward-thing navigation.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_thing_at_point_word_symbol_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'thingatpt)
(with-temp-buffer
  (insert "hello world 42 symbol-x")
  (goto-char 3)
  (list (thing-at-point 'word)
        (bounds-of-thing-at-point 'word)
        (progn (forward-thing 'word) (thing-at-point 'word))
        (progn (goto-char 14) (thing-at-point 'number))
        (progn (goto-char 17) (thing-at-point 'symbol))
        (beginning-of-thing 'word)))
"##;
    let expect = expect_test::expect![[r#""OK (\"hello\" (1 . 6) \"hello\" 42 \"symbol-x\" 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_thing_at_point_list_sexp_line_url() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'thingatpt)
(with-temp-buffer
  (insert "before (list a b c) after\nhttp://example.com end")
  (goto-char 10)
  (list (thing-at-point 'list)
        (thing-at-point 'sexp)
        (bounds-of-thing-at-point 'list)
        (progn (goto-char 26) (thing-at-point 'url))
        (progn (forward-line 0) (thing-at-point 'line))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"(list a b c)\" \"list\" (8 . 20) nil \"before (list a b c) after\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_thing_at_point_symbol_defun_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'thingatpt)
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun probe-fn (a b)\n  \"doc\"\n  body)")
  (goto-char 8)
  (list (thing-at-point 'symbol)
        (thing-at-point 'defun)
        (bounds-of-thing-at-point 'defun)
        (progn (goto-char 22) (thing-at-point 'word))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"probe-fn\" \"(defun probe-fn (a b)\\n  \\\"doc\\\"\\n  body)\" (1 . 38) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
