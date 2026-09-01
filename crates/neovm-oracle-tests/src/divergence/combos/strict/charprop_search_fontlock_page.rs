//! Strict combo oracle probes, batch 55: char-property-search-* (overlay-aware
//! property search), font-lock-fontify-buffer (actual face assignment in
//! emacs-lisp-mode), and page motion (forward-page / page-count-lines).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_k2_char_property_search_overlay_aware() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-property-search-forward)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaaabbbbcccc")
  (put-text-property 1 5 'face 'a)
  (let ((o (make-overlay 5 9)))
    (overlay-put o 'face 'b))
  (put-text-property 9 13 'face 'c)
  (goto-char 1)
  (list (progn (char-property-search-forward 'face 'b t) (point))
        (get-text-property 5 'face)
        (get-char-property 5 'face)
        (progn (goto-char 13) (char-property-search-backward 'face 'a) (point))))
"##,
        expect,
    );
}

#[test]
fn div_k2_font_lock_fontify_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (font-lock-keyword-face font-lock-function-name-face nil font-lock-doc-face)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo ()\n  \"docstring\"\n  bar)\n")
  (font-lock-fontify-buffer)
  (list (get-text-property 2 'face)
        (get-text-property 8 'face)
        (get-text-property 16 'face)
        (get-text-property 26 'face)))
"##,
        expect,
    );
}

#[test]
fn div_k2_page_motion_and_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (13 26 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "page1 line\n\f\npage2 line\n\f\npage3 line\n")
  (goto-char 1)
  (let ((p1 (progn (forward-page 1) (point))))
    (forward-page 1)
    (list p1 (point) (count-lines (point-min) (point-max)))))
"##,
        expect,
    );
}
