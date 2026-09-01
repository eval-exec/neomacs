//! Strict combo oracle probes, batch 60: format-mode-line — the function that
//! evaluates mode-line %-constructs into display text. Covers %b (buffer),
//! %* (read-only/modified markers), %l/%c (line/col), %p/%P (percent), %I
//! (buffer size), %m (mode), %e (errors), %[/%] (recursive-edit), %- (dashes),
//! %n (narrow), and :eval / :propertize forms.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_n0_format_mode_line_basic_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (rename-buffer "probe-ml-buf")
  (list (format-mode-line "%b")
        (format-mode-line "%l")
        (format-mode-line "%c")
        (format-mode-line "%[")
        (format-mode-line "%]")
        (format-mode-line "%-")
        (format-mode-line "%*")
        (format-mode-line "%m")
        (format-mode-line "%e")
        (format-mode-line "%n")))
"##,
        expect,
    );
}

#[test]
fn div_n0_format_mode_line_with_buffer_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world\nsecond line\nthird\n")
  (goto-char 14)
  (rename-buffer "probebuf")
  (setq buffer-read-only t)
  (list (format-mode-line "%b")
        (format-mode-line "%l")
        (format-mode-line "%c")
        (format-mode-line "%p")
        (format-mode-line "%P")
        (format-mode-line "%*")
        (format-mode-line "%I")))
"##,
        expect,
    );
}

#[test]
fn div_n0_format_mode_line_modified_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer (insert "x") (format-mode-line "%*"))
      (with-temp-buffer (insert "x") (set-buffer-modified-p t) (format-mode-line "%*"))
      (with-temp-buffer (insert "x") (set-buffer-modified-p nil) (format-mode-line "%*"))
      (with-temp-buffer (insert "x") (setq buffer-read-only t) (format-mode-line "%*"))
      (with-temp-buffer (insert "x") (setq buffer-read-only t)
                        (set-buffer-modified-p t) (format-mode-line "%*")))
"##,
        expect,
    );
}

#[test]
fn div_n0_format_mode_line_eval_and_propertize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "x")
  (list (format-mode-line '(:eval (concat "A" "B")))
        (format-mode-line '(:propertize "hi" face bold))
        (format-mode-line (list (propertize "tag" 'face 'bold)))
        (format-mode-line "%12l")
        (format-mode-line "%[-")
        (format-mode-line "line %l of total")))
"##,
        expect,
    );
}

#[test]
fn div_n0_format_mode_line_full_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (rename-buffer "probe-full")
  (list (format-mode-line mode-line-format)
        (format-mode-line header-line-format)))
"##,
        expect,
    );
}

#[test]
fn div_n0_format_mode_line_percent_dashes_and_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (make-string 5000 ?x))
  (list (format-mode-line "%-")
        (format-mode-line "%p")
        (format-mode-line "%P")
        (format-mode-line "%I")
        (format-mode-line "%o")))
"##,
        expect,
    );
}
