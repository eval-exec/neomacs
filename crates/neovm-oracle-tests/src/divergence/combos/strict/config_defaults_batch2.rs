//! Strict combo oracle probes, batch 21: another defaults sweep —
//! coding/charset defaults, search/replace config, kill-ring/undo limits,
//! cursor/display config, comment syntax defaults, fill/indent config, and
//! gc/read/eval config. The defaults class has been the richest divergence
//! source.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_f6_coding_charset_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (utf-8-unix t (utf-8-unix . utf-8-unix) utf-8 utf-8 179)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (default-value 'buffer-file-coding-system)
      (coding-system-p (default-value 'buffer-file-coding-system))
      default-process-coding-system
      (coding-system-base (car default-process-coding-system))
      (coding-system-base (cdr default-process-coding-system))
      (length (charset-priority-list)))
"##,
        expect,
    );
}

#[test]
fn div_f6_search_replace_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-variable search-nonincremental-instead-forward)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list case-fold-search
      case-replace
      (default-value 'search-upper-case)
      (default-value 'isearch-lazy-highlight)
      (default-value 'isearch-lazy-count)
      (default-value 'search-nonincremental-instead-forward))
"##,
        expect,
    );
}

#[test]
fn div_f6_kill_ring_undo_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (120 0 gui-select-text gui-selection-value 160000 240000 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list kill-ring-max
      (length kill-ring)
      (default-value 'interprogram-cut-function)
      (default-value 'interprogram-paste-function)
      undo-limit
      undo-strong-limit
      (default-value 'undo-in-region))
"##,
        expect,
    );
}

#[test]
fn div_f6_cursor_display_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 0.5 nil t arrow)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (default-value 'cursor-type)
      (default-value 'cursor-in-non-selected-windows)
      (default-value 'blink-cursor-interval)
      (default-value 'x-stretch-cursor)
      (default-value 'visible-cursor)
      (default-value 'void-text-area-pointer))
"##,
        expect,
    );
}

#[test]
fn div_f6_comment_syntax_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\";\" \"\" 40 nil lisp-comment-indent nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list comment-start
      comment-end
      comment-column
      comment-multi-line
      comment-indent-function
      (default-value 'comment-start-skip))
"##,
        expect,
    );
}

#[test]
fn div_f6_fill_indent_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 8 70 nil t \"[-–!|#%;>*·•‣⁃◦ \t]*\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list indent-tabs-mode
      tab-width
      fill-column
      fill-prefix
      adaptive-fill-mode
      (default-value 'adaptive-fill-regexp)
      (default-value 'colon-double-space))
"##,
        expect,
    );
}

#[test]
fn div_f6_gc_read_eval_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (800000 1.0 t 4 12 read 1600)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list gc-cons-threshold
      gc-cons-percentage
      (default-value 'read-circle)
      (default-value 'eval-expression-print-level)
      (default-value 'eval-expression-print-length)
      (default-value 'load-read-function)
      (default-value 'max-lisp-eval-depth))
"##,
        expect,
    );
}
