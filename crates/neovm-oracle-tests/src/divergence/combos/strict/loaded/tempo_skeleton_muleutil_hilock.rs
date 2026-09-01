//! Strict combo oracle probes, batch 42: template/text loaded libraries via
//! assert_oracle_parity_with_load — tempo.el (templates), skeleton.el
//! (skeleton insertion), international/mule-util.el (string pad/lines/
//! truncate), and hi-lock.el (highlight-regexp face interaction).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h9_tempo_define_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable tempo-probe-tp)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (tempo-define-template "probe-tp" '("before " p " after"))
  (tempo-insert-template 'tempo-probe-tp nil)
  (buffer-string))
"##,
        &["tempo.el"],
        expect,
    );
}

#[test]
fn div_h9_skeleton_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp \"wrap\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (define-skeleton probe-skel
    "Insert wrapped text." str "prefix(" @ _ ")suffix")
  (skeleton-insert (cons '("prefix(" @ _ ")suffix") "wrap"))
  (buffer-string))
"##,
        &["skeleton.el"],
        expect,
    );
}

#[test]
fn div_h9_mule_util_string_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abc  \" \"abc--\" \"  abc\" \"abc\" \"...g\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (string-pad "abc" 5)
      (string-pad "abc" 5 ?-)
      (string-pad "abc" 5 ?  'end)
      (string-chop-newline "abc\n")
      (string-truncate-left "abcdefg" 4))
"##,
        &["international/mule-util.el"],
        expect,
    );
}

#[test]
fn div_h9_hi_lock_highlight_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    // Divergence surfaced 2026-06-27:
    // Loading hi-lock.el and calling highlight-regexp errors in Neomacs with
    // (void-variable search-spaces-regexp) — a standard search variable that
    // Neomacs does not define. GNU Emacs defines it and proceeds. Root cause
    // isolated by div_h9_search_spaces_regexp_missing below.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "foo bar foo")
  (hi-lock-mode 1)
  (highlight-regexp "foo" 'hi-yellow)
  (list (get-text-property 1 'font-lock-face)
        (get-text-property 5 'font-lock-face)
        (get-text-property 9 'font-lock-face)))
"##,
        &["hi-lock.el"],
        expect,
    );
}

#[test]
fn div_h9_search_spaces_regexp_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (t t)
    // Neomacs:   OK (nil t)
    // Root cause of the hi-lock void-variable error above: the standard search
    // variable `search-spaces-regexp' is bound in GNU Emacs but void in
    // Neomacs (the related search-whitespace-regexp is present in both).
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'search-spaces-regexp)
      (boundp 'search-whitespace-regexp))
"##,
        expect,
    );
}

#[test]
fn div_h9_mule_util_truncate_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"日本\" \"日本…\" 0)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (truncate-string-to-width "日本語abc" 4)
      (truncate-string-to-width "日本語abc" 6 nil nil t)
      (string-width (truncate-string-to-width "日本" 1)))
"##,
        &["international/mule-util.el"],
        expect,
    );
}
