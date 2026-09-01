//! Complex combo batch 153 — `isearch` / `replace` / `occur` / `query-replace`
//! state, `isearch-filter-predicate`, and `multi-occur`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx153_isearch_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'isearch-forward)
      (fboundp 'isearch-backward)
      (boundp 'isearch-recursive-edit)
      (boundp 'search-upper-case)
      (boundp 'search-whitespace-regexp))
"##,
        expect,
    );
}

#[test]
fn div_cx153_isearch_lax_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'isearch-lax-whitespace)
      (boundp 'isearch-regexp-lax-whitespace)
      (boundp 'search-default-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx153_occur_basic_buffer_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"alpha line\\nbeta line\\ngamma line\\nalpha again\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha line\nbeta line\ngamma line\nalpha again\n")
      (goto-char 1)
      (occur "alpha")
      (let ((occur-buf (get-buffer "*Occur*")))
        (prog1 (when occur-buf (buffer-string))
          (when occur-buf (kill-buffer occur-buf)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx153_query_replace_count_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ALPHA ALPHA ALPHA ALPHA ALPHA\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "alpha alpha alpha alpha alpha")
  (goto-char 1)
  (let ((search-regexp "alpha")
        (replacement "ALPHA"))
    (while (search-forward "alpha" nil t)
      (replace-match "ALPHA" nil nil))
  (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx153_perform_replace_with_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"let foo = bar; let baz = qux;\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "var foo = bar; var baz = qux;")
  (goto-char 1)
  (while (re-search-forward "\\bvar \\b" nil t)
    (replace-match "let "))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx153_multi_isearch_buffers_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'misearch)
      (list (fboundp 'multi-isearch-buffers)
            (boundp 'multi-isearch-buffer-list)
            (boundp 'lazy-highlight-cleanup)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx153_occur_edit_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'occur)
      (list (fboundp 'occur-edit-mode)
            (fboundp 'occur-mode-display-occurrence)
            (boundp 'occur-mode-hook)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx153_replace_string_preserves_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'replace-replace-char)
      (boundp 'replace-lax-whitespace)
      (boundp 'replace-char-spacing))
"##,
        expect,
    );
}

#[test]
fn div_cx153_word_search_forward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"\\\\<the\\\\>\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "the theme park is there")
  (goto-char 1)
  (let ((first (word-search-forward "the")))
    (let ((second (word-search-forward "the")))
      (list first second
            (buffer-substring-no-properties 1 5)
            (buffer-substring-no-properties 11 15)))))
"##,
        expect,
    );
}

#[test]
fn div_cx153_search_ring_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"alpha\" \"beta\" \"gamma\") (\"\\\\balpha\\\\b\" \"\\\\bbeta\\\\b\") \"alpha\" \"\\\\balpha\\\\b\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((search-ring '("alpha" "beta" "gamma"))
      (regexp-search-ring '("\\balpha\\b" "\\bbeta\\b")))
  (list search-ring
        regexp-search-ring
        (car search-ring)
        (car regexp-search-ring)
        (boundp 'search-ring-yank-pointer)))
"##,
        expect,
    );
}

#[test]
fn div_cx153_isearch_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "alpha 123 beta 456 gamma 789 delta 012 epsilon")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 12))
        (ov (make-overlay 5 20)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 4 45)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "\\b[a-z]+\\b" nil t)
      (replace-match (upcase (match-string 0))))
    (let ((state (list (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (point-min) (point-max)
                       (text-properties-at 1))))
      (undo) (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
