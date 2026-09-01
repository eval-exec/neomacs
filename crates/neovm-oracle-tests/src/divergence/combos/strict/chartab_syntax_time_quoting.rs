//! Strict combo oracle probes, batch 8: char-table extra slots per subtype,
//! syntax-table mutation + char-syntax/syntax-after, fixed-time arithmetic,
//! shell-string quoting/unquoting, frame/window parameter defaults, format
//! padding/precision combos, and buffer-local default values.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_e3_char_table_extra_slots_per_subtype() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-table-extra-slots)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-table-extra-slots (make-char-table 'category-table))
      (char-table-extra-slots (make-char-table 'syntax-table))
      (char-table-extra-slots (make-char-table 'char-display-table))
      (char-table-extra-slots (make-char-table 'display-table)))
"##,
        expect,
    );
}

#[test]
fn div_e3_char_syntax_after_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (46 40 41 119 (1) (0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (modify-syntax-entry ?/ ".")
  (modify-syntax-entry ?\( "()")
  (modify-syntax-entry ?\) ")(")
  (insert "/ ()")
  (list (char-syntax ?/)
        (char-syntax ?\()
        (char-syntax ?\))
        (char-syntax ?a)
        (syntax-after 1)
        (syntax-after 2)))
"##,
        expect,
    );
}

#[test]
fn div_e3_time_arithmetic_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((14445 17280) 946684860 946681200 t t nil 946684800.0 (0 100 0 0) 946684800)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((base (encode-time 0 0 0 1 1 2000 0)))
  (list base
        (time-add base 60)
        (time-subtract base 3600)
        (time-less-p base (time-add base 1))
        (time-equal-p base base)
        (time-less-p (time-add base 1) base)
        (float-time base)
        (seconds-to-time 100)
        (time-convert base 'integer)))
"##,
        expect,
    );
}

#[test]
fn div_e3_combine_quote_split_unquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"a b c\" \"\\\"a b\\\" c\" \"a \\\"b\\\\\\\"c\\\"\" (\"a\" \"b\" \"c\") (\"a b\" \"c\") (\"a\\\\\" \"b\" \"c\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (combine-and-quote-strings '("a" "b" "c"))
      (combine-and-quote-strings '("a b" "c"))
      (combine-and-quote-strings '("a" "b\"c"))
      (split-string-and-unquote "a b c")
      (split-string-and-unquote "\"a b\" c")
      (split-string-and-unquote "a\\ b c"))
"##,
        expect,
    );
}

#[test]
fn div_e3_frame_parameter_more_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0 nil nil nil nil nil \"F1\" dark nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (frame-parameter nil 'border-width)
      (frame-parameter nil 'tab-bar-lines)
      (frame-parameter nil 'right-divider-width)
      (frame-parameter nil 'bottom-divider-width)
      (frame-parameter nil 'cursor-type)
      (frame-parameter nil 'auto-raise)
      (frame-parameter nil 'auto-lower)
      (frame-parameter nil 'name)
      (frame-parameter nil 'background-mode)
      (frame-parameter nil 'window-id))
"##,
        expect,
    );
}

#[test]
fn div_e3_window_parameter_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil nil nil ((quit-restore other (#<buffer *scratch*> 1 #<marker at 1 in *scratch*> 80) #<window 1 on *scratch*> #<killed buffer>)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wp2*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (list (window-parameter nil 'no-other-window)
              (window-parameter nil 'window-side)
              (window-parameter nil 'window-slot)
              (window-parameters)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_e3_format_misc_padding_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"3.14 |\" \"0003.142\" \"+0000042\" \"   3.100\" \"left      |     right|\" \"\" \"   ab|\" \"ab   |\" \"  0042\" \"+7    |\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%-5.2f|" 3.14159)
      (format "%08.3f" 3.14159)
      (format "%+08d" 42)
      (format "% 8.3f" 3.1)
      (format "%-10s|%10s|" "left" "right")
      (format "%.0s" "anything")
      (format "%5.2s|" "abcdef")
      (format "%-5.2s|" "abcdef")
      (format "%6.4d" 42)
      (format "%-+6d|" 7))
"##,
        expect,
    );
}

#[test]
fn div_e3_buffer_local_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 70 t t nil nil 0 nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (default-value 'tab-width)
      (default-value 'fill-column)
      (default-value 'case-fold-search)
      (default-value 'indent-tabs-mode)
      (default-value 'truncate-lines)
      (default-value 'word-wrap)
      (default-value 'left-margin)
      (default-value 'left-fringe-width)
      (default-value 'fringes-outside-margins)
      (default-value 'enable-multibyte-characters))
"##,
        expect,
    );
}
