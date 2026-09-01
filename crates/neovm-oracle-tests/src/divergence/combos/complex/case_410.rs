//! Complex combo batch 410 — 20 probes in fresh territory: display-warning,
//! scroll-up/down in batch, window-size-fixed, fit-window-to-buffer,
//! balance-windows-area, window-full-height-p, window-parameter/set,
//! window-margins, window-fringes, display-buffer, pop-to-buffer,
//! replace-buffer-contents, replace-string, read-char-by-name,
//! read-number, y-or-n-p/yes-or-no-p in batch, momentary-string-display,
//! mode-line-format, column-number-mode, and frame-title-format.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// display-warning: issuing warnings via the warning system.
#[test]
fn div_cx410_display_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'warnings)
  (let ((warnings ()))
    (display-warning 'neo-cx410 "test warning" :warning)
    (list (warning-numeric-level :warning)
          (warning-suppress-p 'neo-cx410))))
"##,
        expect,
    );
}

/// scroll-up / scroll-down in batch: may be no-ops.
#[test]
fn div_cx410_scroll_up_down_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (end-of-buffer beginning-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (make-string 200 ?a) "\n" (make-string 200 ?b))
  (list (condition-case e (scroll-up 1) (error (car e)))
        (condition-case e (scroll-down 1) (error (car e)))))
"##,
        expect,
    );
}

/// window-size-fixed: querying and setting fixed window size.
#[test]
fn div_cx410_window_size_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-size-fixed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-size-fixed w)
        (window-size-fixed w t)
        (window-size-fixed w nil)))
"##,
        expect,
    );
}

/// fit-window-to-buffer: resizing window to fit contents.
#[test]
fn div_cx410_fit_window_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "short")
  (condition-case e
      (fit-window-to-buffer (selected-window))
    (error (car e))))
"##,
        expect,
    );
}

/// balance-windows-area: balancing window sizes.
#[test]
fn div_cx410_balance_windows_area() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (balance-windows-area)
  (error (car e)))
"##,
        expect,
    );
}

/// window-full-height-p / window-full-width-p.
#[test]
fn div_cx410_window_full_height_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-full-height-p w)
        (window-full-width-p w)))
"##,
        expect,
    );
}

/// window-parameter / set-window-parameter:
/// window-local parameter storage.
#[test]
fn div_cx410_window_parameter_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (test-val nil (neo-cx410-param . test-val))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (set-window-parameter w 'neo-cx410-param 'test-val)
  (list (window-parameter w 'neo-cx410-param)
        (window-parameter w 'nonexistent)
        (assq 'neo-cx410-param (window-parameters w))))
"##,
        expect,
    );
}

/// window-margins / set-window-margins: display margins.
#[test]
fn div_cx410_window_margins() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2 . 3) 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (set-window-margins w 2 3)
  (list (window-margins w)
        (car (window-margins w))
        (cdr (window-margins w))))
"##,
        expect,
    );
}

/// window-fringes / set-window-fringes: fringe widths.
#[test]
fn div_cx410_window_fringes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 0 nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (set-window-fringes w 5 10 nil)
  (list (window-fringes w)))
"##,
        expect,
    );
}

/// display-buffer / pop-to-buffer: buffer display in batch.
#[test]
fn div_cx410_display_pop_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create "*neo-cx410-display*")))
  (with-current-buffer buf (insert "test"))
  (list (windowp (display-buffer buf))
        (windowp (pop-to-buffer buf))
        (eq (window-buffer (selected-window)) buf)))
"##,
        expect,
    );
}

/// replace-buffer-contents: replacing buffer content with
/// another buffer's content using diff-based replacement.
#[test]
fn div_cx410_replace_buffer_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((src (get-buffer-create "*neo-cx410-src*"))
      (dst (get-buffer-create "*neo-cx410-dst*")))
  (with-current-buffer src (insert "hello world"))
  (with-current-buffer dst (insert "hello there"))
  (prog1 (condition-case e
             (with-current-buffer dst
               (replace-buffer-contents src))
           (error (car e)))
    (kill-buffer src)
    (kill-buffer dst)))
"##,
        expect,
    );
}

/// replace-string / replace-regexp in buffer.
#[test]
fn div_cx410_replace_string_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"X bar X baz X qux\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "foo bar foo baz foo qux")
  (goto-char 1)
  (replace-string "foo" "X" nil (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

/// read-char-by-name with Unicode character names.
#[test]
fn div_cx410_read_char_by_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function void-function \"SNOWMAN\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (char-with-name "SNOWMAN") (error (car e)))
      (condition-case e (char-with-name "LATIN CAPITAL LETTER A") (error (car e)))
      (get-char-code-property ?☃ 'name))
"##,
        expect,
    );
}

/// read-number / read-number-history: reading numbers in batch.
#[test]
fn div_cx410_read_number_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(progn (require 'cus-edit)
  (list (condition-case e (read-number "Enter: " 42) (error (car e)))
        (condition-case e (read-number "Enter: ") (error (car e)))))
"##,
    );
}

/// y-or-n-p / yes-or-no-p in batch mode: should return nil
/// or signal end-of-file.
#[test]
fn div_cx410_y_or_n_p_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(list (condition-case e (y-or-n-p "Test? ") (error (car e)))
      (condition-case e (yes-or-no-p "Test? ") (error (car e))))
"##,
    );
}

/// momentary-string-display: showing a string temporarily.
#[test]
fn div_cx410_momentary_string_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"original content\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "original content")
  (condition-case e
      (momentary-string-display "[displayed]" 3 nil "done")
    (error (car e)))
  (buffer-string))
"##,
        expect,
    );
}

/// mode-line-format / mode-line-position: mode line components.
#[test]
fn div_cx410_mode_line_format_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello\nworld\n")
  (goto-char 8)
  (let ((fmt (format-mode-line mode-line-format)))
    (list (stringp fmt)
          (> (length fmt) 0))))
"##,
        expect,
    );
}

/// column-number-mode / line-number-mode:
/// toggling display modes.
#[test]
fn div_cx410_column_line_number_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc\ndef\nghi")
  (column-number-mode 1)
  (line-number-mode 1)
  (goto-char 5)
  (let ((fmt (format-mode-line mode-line-format)))
    (list (stringp fmt)
          (string-match-p ":" fmt))))
"##,
        expect,
    );
}

/// frame-title-format / icon-title-format.
#[test]
fn div_cx410_frame_title_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-mode-line frame-title-format)
      (format-mode-line icon-title-format))
"##,
        expect,
    );
}

/// with-help-window / print-help-return-message.
#[test]
fn div_cx410_with_help_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (buffer-read-only #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'help-mode)
  (with-temp-buffer
    (help-mode)
    (insert "test help")
    (condition-case e
        (print-help-return-message)
      (error (car e)))
    (buffer-string)))
"##,
        expect,
    );
}
