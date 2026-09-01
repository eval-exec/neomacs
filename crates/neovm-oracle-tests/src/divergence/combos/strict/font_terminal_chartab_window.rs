//! Strict combo oracle probes, batch 17: uninterned (#:) symbol read,
//! char-table nil/parent range, font-spec introspection, terminal parameter
//! ops, charset/coding-system priority lists, and window set-then-get metrics.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_f2_read_uninterned_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"foo\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s1 (read "#:foo"))
      (s2 (read "#:foo")))
  (list (symbolp s1)
        (symbol-name s1)
        (eq s1 s2)
        (eq s1 (intern "foo"))))
"##,
        expect,
    );
}

#[test]
fn div_f2_char_table_range_parent_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Invalid RANGE argument to ‘char-table-range’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'category-table 'default)))
  (set-char-table-range ct ?a 'a-val)
  (set-char-table-range ct '(?a . ?z) 'range-val)
  (list (char-table-range ct ?a)
        (char-table-range ct ?b)
        (char-table-range ct ?A)
        (char-table-range ct nil)
        (char-table-range ct t)))
"##,
        expect,
    );
}

#[test]
fn div_f2_font_spec_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function font-spec-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((fs (font-spec :family "Monospace" :weight 'normal :slant 'italic)))
  (list (fontp fs 'font-spec)
        (fontp fs)
        (font-get fs :family)
        (font-get fs :weight)
        (font-get fs :slant)
        (font-spec-p fs)))
"##,
        expect,
    );
}

#[test]
fn div_f2_terminal_parameter_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function terminalp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((term (car (terminal-list))))
  (list (terminal-live-p term)
        (terminalp term)
        (progn (set-terminal-parameter term 'probe-param 42)
               (terminal-parameter term 'probe-param))
        (terminal-parameter term 'nonexistent-probe-param)
        (length (terminal-list))))
"##,
        expect,
    );
}

#[test]
fn div_f2_charset_priority_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function charset-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length (charset-list))
      (charsetp 'ascii)
      (charsetp 'unicode)
      (char-charset ?a)
      (char-charset ?あ)
      (memq 'ascii (charset-priority-list)))
"##,
        expect,
    );
}

#[test]
fn div_f2_coding_system_priority_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (20 (utf-8 iso-2022-7bit iso-latin-1 iso-2022-7bit-lock iso-2022-8bit-ss2 emacs-mule raw-text iso-2022-jp in-is13194-devanagari chinese-iso-8bit utf-8-auto utf-8-with-signature utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le japanese-shift-jis chinese-big5 undecided) utf-8 utf-8)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length (coding-system-priority-list))
      (memq 'utf-8 (coding-system-priority-list))
      (car (coding-system-priority-list))
      (coding-system-base (car (coding-system-priority-list))))
"##,
        expect,
    );
}

#[test]
fn div_f2_window_set_get_metrics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 . 2) (0 0 nil nil) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wsgm*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (set-window-margins nil 3 2)
        (set-window-fringes nil 5 6 nil)
        (set-window-hscroll nil 4)
        (list (window-margins)
              (window-fringes)
              (window-hscroll)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f2_window_body_width_ignores_margins() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 75""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK 75
    // Neomacs:   OK 80
    // window-body-width does not subtract left+right margins: with margins
    // (3 . 2) set, GNU reports 75 (80 - 3 - 2) while Neomacs reports 80.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wbm*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (set-window-margins nil 3 2)
        (window-body-width))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}
