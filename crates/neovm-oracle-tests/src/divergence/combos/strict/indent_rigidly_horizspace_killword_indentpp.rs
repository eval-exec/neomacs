//! Strict combo oracle probes, batch 96: indent-rigidly (shift text), delete-
//! horizontal-space, kill-word/backward-kill-word, and indent-pp.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_r0_indent_rigidly_and_horiz_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"   line1\\n   line2\\n   line3\\n\" \"  hello   world  \")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (insert "line1\nline2\nline3\n")
        (indent-rigidly 1 15 3)
        (buffer-string))
      (with-temp-buffer
        (insert "  hello   world  ")
        (goto-char 5)
        (delete-horizontal-space)
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_r0_kill_word_and_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\" three four\" \"one two \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (insert "one two three four")
        (goto-char 1)
        (kill-word 2)
        (buffer-string))
      (with-temp-buffer
        (insert "one two three four")
        (goto-char 19)
        (backward-kill-word 2)
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_r0_indent_pp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function indent-pp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "((a . 1)\n(b . (2 3))\n(c . 4))")
  (goto-char 1)
  (indent-pp)
  (buffer-string))
"##,
        expect,
    );
}
