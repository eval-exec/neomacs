//! Strict combo oracle probes, batch 43: boundp sweeps of standard variables
//! (isearch/search, standard hooks, and misc config vars) to find variables
//! Neomacs fails to define. search-spaces-regexp was already found missing;
//! these sweeps look for more.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_i0_isearch_search_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'search-default-mode)
      (boundp 'search-upper-case)
      (boundp 'isearch-lax-whitespace)
      (boundp 'isearch-regexp-lax-whitespace)
      (boundp 'word-search-regexp)
      (boundp 'lazy-highlight-cleanup)
      (boundp 'isearch-repeat-on-direction-change)
      (boundp 'char-fold-symmetric)
      (boundp 'search-exit-option)
      (boundp 'isearch-yank-on-move))
"##,
        expect,
    );
}

#[test]
fn div_i0_standard_hooks_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'find-file-hook)
      (boundp 'pre-command-hook)
      (boundp 'post-command-hook)
      (boundp 'before-save-hook)
      (boundp 'after-save-hook)
      (boundp 'post-self-insert-hook)
      (boundp 'change-major-mode-after-body-hook)
      (boundp 'first-change-hook))
"##,
        expect,
    );
}

#[test]
fn div_i0_window_scroll_functions_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK t
    // Neomacs:   OK nil
    // The standard hook window-scroll-functions is bound in GNU Emacs but
    // void in Neomacs.
    crate::common::assert_oracle_parity_expect(
        r##"
(boundp 'window-scroll-functions)
"##,
        expect,
    );
}

#[test]
fn div_i0_misc_standard_vars_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'text-quoting-style)
      (boundp 'line-move-visual)
      (boundp 'view-read-only)
      (boundp 'enable-recursive-minibuffers)
      (boundp 'char-script-table)
      (boundp 'word-wrap-by-default)
      (boundp 'fast-but-imprecise-scrolling)
      (boundp 'idle-update-delay)
      (boundp 'bidi-display-reordering)
      (boundp 'indicate-unused-lines)
      (boundp 'indicate-buffer-boundaries)
      (boundp 'glyphless-char-display))
"##,
        expect,
    );
}

#[test]
fn div_i0_face_and_display_vars_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'face-font-family-alternatives)
      (boundp 'face-font-registry-alternatives)
      (boundp 'face-remapping-alist)
      (boundp 'face-default-family)
      (boundp 'tty-defined-color-alist)
      (boundp 'w32-default-font)
      (boundp 'font-list-limit)
      (boundp 'x-gtk-use-system-tooltips))
"##,
        expect,
    );
}

#[test]
fn div_i0_coding_charset_vars_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'charset-map)
      (boundp 'coding-system-list-default)
      (boundp 'current-language-environment)
      (boundp 'locale-preferred-coding-systems)
      (boundp 'current-iso639-language)
      (boundp 'language-info-alist)
      (boundp 'input-method-alist)
      (boundp 'coding-category-list))
"##,
        expect,
    );
}
