//! Oracle parity tests for `string-width` and `char-width`.
//!
//! Covers: ASCII strings, CJK double-width characters, mixed content,
//! char-width for various character types, tab characters,
//! control characters, and a complex truncate-to-display-width function.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-width for pure ASCII strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (string-width "")
  (string-width "a")
  (string-width "hello")
  (string-width "Hello, World!")
  (string-width "abcdefghijklmnopqrstuvwxyz")
  (string-width " ")
  (string-width "   ")
  ;; All printable ASCII
  (string-width "!@#$%^&*()_+-=[]{}|;':\",./<>?"))"#;
    let expect = expect_test::expect![[r#""OK (0 1 5 13 26 1 3 29)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width for CJK characters (double-width)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CJK ideographs are typically double-width in a terminal/display
    let form = r#"(list
  (string-width "\u4e16")
  (string-width "\u4e16\u754c")
  (string-width "\u4f60\u597d")
  (string-width "\u6771\u4eac\u90fd")
  ;; Longer CJK text
  (string-width "\u4e2d\u6587\u5b57\u7b26\u4e32\u6d4b\u8bd5")
  ;; Japanese Hiragana (also double-width)
  (string-width "\u3053\u3093\u306b\u3061\u306f")
  ;; Korean Hangul (double-width)
  (string-width "\uc548\ub155\ud558\uc138\uc694"))"#;
    let expect = expect_test::expect![[r#""OK (2 4 4 6 14 10 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width for mixed ASCII + CJK content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Mixed: ASCII chars are width 1, CJK are width 2
  (string-width "hello\u4e16\u754c")
  (string-width "A\u4e2dB\u6587C")
  (string-width "test:\u6d4b\u8bd5")
  ;; CJK surrounded by ASCII
  (string-width "[\u6771\u4eac]")
  ;; Verify additivity
  (let ((s1 "abc")
        (s2 "\u4e16\u754c"))
    (list (string-width s1)
          (string-width s2)
          (string-width (concat s1 s2))
          (= (string-width (concat s1 s2))
             (+ (string-width s1) (string-width s2))))))"#;
    let expect = expect_test::expect![[r#""OK (9 7 9 6 (3 4 7 t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-width for various character types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; ASCII characters: width 1
  (char-width ?a)
  (char-width ?Z)
  (char-width ?0)
  (char-width ?!)
  (char-width ?\s)
  ;; CJK ideographs: width 2
  (char-width ?\u4e16)
  (char-width ?\u4e2d)
  ;; Tab: typically width varies but char-width returns a fixed value
  (char-width ?\t)
  ;; Newline
  (char-width ?\n)
  ;; Some Latin-1 supplement characters
  (char-width ?\u00e9)
  (char-width ?\u00f1))"#;
    let expect = expect_test::expect![[r#""OK (1 1 1 1 1 2 2 8 0 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width for strings containing tab characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_tabs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Single tab
  (string-width "\t")
  ;; Tab + text
  (string-width "\thello")
  ;; Multiple tabs
  (string-width "\t\t")
  ;; Tab between words
  (string-width "a\tb")
  ;; Tab at end
  (string-width "hello\t")
  ;; Compare with spaces
  (let ((tab-w (string-width "\t"))
        (space-w (string-width " ")))
    (list tab-w space-w)))"#;
    let expect = expect_test::expect![[r#""OK (8 13 16 10 13 (8 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width for control characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_control_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; NUL character
  (string-width (string 0))
  ;; BEL (bell)
  (string-width (string 7))
  ;; BS (backspace)
  (string-width (string 8))
  ;; ESC
  (string-width (string 27))
  ;; DEL
  (string-width (string 127))
  ;; Newline
  (string-width "\n")
  ;; Carriage return
  (string-width "\r")
  ;; Mixed control + normal
  (string-width (concat "abc" (string 0) "def")))"#;
    let expect = expect_test::expect![[r#""OK (2 2 2 2 2 0 2 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width with combining characters and accented text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_combining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic Latin with combining acute accent (U+0301)
  ;; "e" + combining acute = visual "e" with accent, but string-width counts columns
  (string-width (string ?e ?\u0301))
  ;; Multiple combining marks
  (string-width (string ?a ?\u0300 ?\u0301))
  ;; Pre-composed vs decomposed
  (string-width "\u00e9")
  ;; Fullwidth forms
  (string-width "\uff21")
  (string-width "\uff41")
  ;; Halfwidth Katakana (width 1)
  (string-width "\uff71")
  ;; Compare lengths vs widths for mixed content
  (let ((s "A\u4e2d\u6587B"))
    (list (length s)
          (string-width s))))"#;
    let expect = expect_test::expect![[r#""OK (1 1 1 2 2 1 (4 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: truncate string to a given display width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_truncate_to_display_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a function that truncates a string to fit within
    // a maximum display width, appending an ellipsis indicator if truncated.
    // Must handle variable-width characters correctly.
    let form = r#"(progn
  (fset 'neovm--test-truncate-display
    (lambda (str max-width ellipsis)
      (if (<= (string-width str) max-width)
          str
        (let* ((ellipsis-width (string-width ellipsis))
               (target-width (- max-width ellipsis-width))
               (i 0)
               (current-width 0)
               (len (length str)))
          ;; Walk characters, accumulating width
          (while (and (< i len)
                      (<= (+ current-width (char-width (aref str i)))
                          target-width))
            (setq current-width (+ current-width (char-width (aref str i))))
            (setq i (1+ i)))
          (concat (substring str 0 i) ellipsis)))))

  (unwind-protect
      (list
        ;; ASCII: fits
        (funcall 'neovm--test-truncate-display "hello" 10 "...")
        ;; ASCII: needs truncation
        (funcall 'neovm--test-truncate-display "hello world" 8 "...")
        ;; CJK: each char is width 2
        (funcall 'neovm--test-truncate-display
                 "\u4e16\u754c\u4f60\u597d\u4e2d\u6587" 8 "..")
        ;; Mixed: ASCII + CJK
        (funcall 'neovm--test-truncate-display
                 "Hi\u4e16\u754cTest" 8 "..")
        ;; Exact fit
        (funcall 'neovm--test-truncate-display "abcde" 5 "...")
        ;; Very narrow: only ellipsis fits
        (funcall 'neovm--test-truncate-display
                 "\u4e16\u754c\u4f60\u597d" 3 "..")
        ;; Verify widths of results
        (let* ((result (funcall 'neovm--test-truncate-display
                                "Hello \u4e16\u754c World" 10 "..."))
               (w (string-width result)))
          (list result w (<= w 10))))
    (fmakunbound 'neovm--test-truncate-display)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello...\" \"世界你..\" \"Hi世界..\" \"abcde\" \"..\" (\"Hello ...\" 9 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: column-aligned table formatter using string-width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_column_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a function that pads strings to a fixed display width,
    // handling CJK characters properly.
    let form = r#"(progn
  (fset 'neovm--test-pad-to-width
    (lambda (str target-width)
      (let ((w (string-width str)))
        (if (>= w target-width)
            str
          (concat str (make-string (- target-width w) ?\s))))))

  (fset 'neovm--test-format-row
    (lambda (cols widths)
      (let ((parts nil)
            (c cols)
            (w widths))
        (while (and c w)
          (setq parts (cons (funcall 'neovm--test-pad-to-width
                                     (car c) (car w))
                            parts))
          (setq c (cdr c) w (cdr w)))
        (mapconcat #'identity (nreverse parts) " | "))))

  (unwind-protect
      (let ((widths '(10 8 6)))
        (list
          ;; All ASCII
          (funcall 'neovm--test-format-row '("Name" "City" "Age") widths)
          (funcall 'neovm--test-format-row '("Alice" "Boston" "30") widths)
          ;; With CJK (takes 2 columns each)
          (funcall 'neovm--test-format-row
                   '("\u5f20\u4e09" "\u4e1c\u4eac" "25") widths)
          ;; Mixed
          (funcall 'neovm--test-format-row
                   '("Bob\u5f20" "LA" "42") widths)
          ;; Verify alignment: all rows should have same string-width
          (let ((rows (list
                       (funcall 'neovm--test-format-row '("A" "B" "C") widths)
                       (funcall 'neovm--test-format-row '("\u4e2d" "\u6587" "X") widths))))
            (list (string-width (car rows))
                  (string-width (cadr rows))
                  (= (string-width (car rows))
                     (string-width (cadr rows)))))))
    (fmakunbound 'neovm--test-pad-to-width)
    (fmakunbound 'neovm--test-format-row)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Name       | City     | Age   \" \"Alice      | Boston   | 30    \" \"张三       | 东京     | 25    \" \"Bob张      | LA       | 42    \" (30 30 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
