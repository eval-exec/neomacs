//! Complex combo batch 406 — 20 probes in fresh divergence territory:
//! line-move-visual with display, forward-button/button navigation,
//! defcustom type validation, indent-region, align-regexp, sort-lines
//! with multibyte, face-differs-from-default-p, face-bold/italic/underline-p,
//! color-defined-p, window-state-get/put, window-at, line-end-position
//! with multibyte, decode-char/encode-char various charsets,
//! multibyte-string-p, string-to-multibyte/unibyte roundtrip,
//! unibyte-char-to-multibyte, format-CSV, and char-after with
//! combining characters.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// line-move-visual with display property: visual line movement
/// should account for display glyph width across lines.
#[test]
fn div_cx406_line_move_visual_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (13 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc def ghi\njkl mno pqr")
  (put-text-property 3 4 'display "XXXXXXXX")
  (list (progn (goto-char 1) (line-move-visual 1) (point))
        (progn (line-move-visual 0) (current-column))))
"##,
        expect,
    );
}

/// forward-button/backward-button navigation: button.el
/// may not be fully supported in Neomacs.
#[test]
fn div_cx406_forward_backward_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#<marker (moves after insertion) in no buffer> #<marker (moves after insertion) in no buffer> #<marker (moves after insertion) in no buffer>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'button)
  (with-temp-buffer
    (insert "before ")
    (insert-text-button "Btn1" 'action (lambda (b) (message "btn1")))
    (insert " middle ")
    (insert-text-button "Btn2" 'action (lambda (b) (message "btn2")))
    (insert " after")
    (goto-char 1)
    (let (results)
      (push (condition-case e (forward-button 1) (error (car e))) results)
      (push (condition-case e (forward-button 1) (error (car e))) results)
      (push (condition-case e (forward-button -1) (error (car e))) results)
      (nreverse results))))
"##,
        expect,
    );
}

/// defcustom type validation: Neomacs may not validate
/// :type constraints on custom variables.
#[test]
fn div_cx406_defcustom_type_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((funcall #'(closure (t) nil \"default\"))) ((funcall #'(closure (t) nil \"default\"))) ((funcall #'(closure (t) nil 42))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defcustom neo-cx406-opt "default"
    "test option"
    :type 'string)
  (defcustom neo-cx406-num 42
    "test number"
    :type 'integer)
  (list (custom-variable-p 'neo-cx406-opt)
        (get 'neo-cx406-opt 'standard-value)
        (get 'neo-cx406-num 'standard-value)))
"##,
        expect,
    );
}

/// indent-region with different major modes.
#[test]
fn div_cx406_indent_region_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"indented\\nmore\\nless\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (text-mode)
  (insert "  indented\n    more\n  less")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

/// align-regexp: aligning text columns with regex.
#[test]
fn div_cx406_align_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"foo\t1\\nbar\t23\\nbaz\t456\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "foo 1\nbar 23\nbaz 456")
  (align-regexp (point-min) (point-max) "\\(\\s-+\\)[0-9]+")
  (buffer-string))
"##,
        expect,
    );
}

/// sort-lines with multibyte strings.
#[test]
fn div_cx406_sort_lines_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"alpha\\ncafé\\nüber\\nβeta\\n世界\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café\nalpha\nβeta\nüber\n世界\n")
  (sort-lines nil (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

/// face-differs-from-default-p for various faces.
#[test]
fn div_cx406_face_differs_from_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-face 'neo-cx406-test)))
  (set-face-attribute f nil :foreground "red")
  (list (face-differs-from-default-p 'bold)
        (face-differs-from-default-p 'italic)
        (face-differs-from-default-p 'neo-cx406-test)
        (face-differs-from-default-p 'default)))
"##,
        expect,
    );
}

/// face-bold-p / face-italic-p / face-underline-p for faces.
#[test]
fn div_cx406_face_bold_italic_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((bold extra-bold ultra-bold) (italic oblique) nil (bold extra-bold ultra-bold) (italic oblique) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-face 'neo-cx406-f2)))
  (set-face-attribute f nil :weight 'bold :slant 'italic :underline t)
  (list (face-bold-p 'bold)
        (face-italic-p 'italic)
        (face-underline-p 'default)
        (face-bold-p 'neo-cx406-f2)
        (face-italic-p 'neo-cx406-f2)
        (face-underline-p 'neo-cx406-f2)))
"##,
        expect,
    );
}

/// color-defined-p for various color names.
#[test]
fn div_cx406_color_defined_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (color-defined-p "red")
      (color-defined-p "#ff0000")
      (color-defined-p "nonexistent-color-name-xyzzy")
      (color-defined-p "AliceBlue"))
"##,
        expect,
    );
}

/// window-state-get / window-state-put roundtrip.
#[test]
fn div_cx406_window_state_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "test")
  (let ((state (window-state-get (selected-window))))
    (list (window-state-put state (selected-window))
          (buffer-string))))
"##,
        expect,
    );
}

/// window-at with different coordinates in batch.
#[test]
fn div_cx406_window_at() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#<window 1 on *scratch*> t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (window-at 0 0)
      (windowp (window-at 0 0))
      (eq (window-at 0 0) (selected-window)))
"##,
        expect,
    );
}

/// line-end-position / line-beginning-position with multibyte
/// characters in the buffer.
#[test]
fn div_cx406_line_end_begin_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (16 16 16 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界\nabc\nαβγ\n")
  (list (line-beginning-position 2)
        (line-end-position 1)
        (line-end-position 2)
        (line-end-position 3)))
"##,
        expect,
    );
}

/// decode-char / encode-char with various charsets beyond eight-bit.
#[test]
fn div_cx406_decode_encode_char_charsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65 65 nil 105)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (decode-char 'ascii 65) (error (car e)))
      (condition-case e (encode-char ?A 'ascii) (error (car e)))
      (condition-case e (decode-char 'latin-iso8859-1 233) (error (car e)))
      (condition-case e (encode-char ?é 'latin-iso8859-1) (error (car e))))
"##,
        expect,
    );
}

/// multibyte-string-p / string-multibyte with various strings.
#[test]
fn div_cx406_multibyte_string_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s1 "abc")
      (s2 "café")
      (s3 (string-make-multibyte "abc"))
      (s4 (string-to-multibyte "abc")))
  (list (multibyte-string-p s1)
        (multibyte-string-p s2)
        (multibyte-string-p s3)
        (multibyte-string-p s4)))
"##,
        expect,
    );
}

/// string-to-multibyte / string-to-unibyte roundtrip.
#[test]
fn div_cx406_string_to_multibyte_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 6 4 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string 200 201 65 66))
       (mb (string-to-multibyte raw))
       (back (string-to-unibyte mb)))
  (list (string-bytes raw)
        (string-bytes mb)
        (string-bytes back)
        (equal raw back)
        (string= raw back)))
"##,
        expect,
    );
}

/// unibyte-char-to-multibyte / multibyte-char-to-unibyte.
#[test]
fn div_cx406_unibyte_multibyte_char_convert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4194248 200 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (unibyte-char-to-multibyte 200)
      (multibyte-char-to-unibyte (unibyte-char-to-multibyte 200))
      (condition-case e (multibyte-char-to-unibyte ?é) (error (car e))))
"##,
        expect,
    );
}

/// format CSV / CSV parsing: CSV format may not be supported.
#[test]
fn div_cx406_format_csv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (format-csv-print '((a b c) (1 2 3))) (error (car e)))
      (condition-case e (format-csv-print nil) (error (car e))))
"##,
        expect,
    );
}

/// char-after with combining characters (zero-width).
#[test]
fn div_cx406_char_after_combining() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 769 98 768 99 770)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a\u0301b\u0300c\u0302")
  (list (char-after 1) (char-after 2) (char-after 3)
        (char-after 4) (char-after 5) (char-after 6)))
"##,
        expect,
    );
}

/// compare-strings with case-insensitive multibyte.
#[test]
fn div_cx406_compare_strings_casefold_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (compare-strings "café" nil nil "CAFÉ" nil nil t)
        (compare-strings "αβγ" nil nil "ΑΒΓ" nil nil t)
        (string-equal-ignore-case "café" "CAFÉ")
        (string-equal-ignore-case "straße" "STRASSE")))
"##,
        expect,
    );
}
