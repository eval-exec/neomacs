//! Oracle parity tests for `insert-char`: basic insertion, COUNT,
//! INHERIT flag, Unicode characters, positional insertion, and
//! complex patterns building structured output via insert-char.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic single character insertion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_basic_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert-char ?A)
      (insert-char ?B)
      (insert-char ?C)
      (list (buffer-string) (buffer-size) (point)))"#;
    let expect = expect_test::expect![[r#""OK (\"ABC\" 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-char with COUNT argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_with_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert-char ?= 40)
      (insert-char ?\n 1)
      (insert-char ?X 5)
      (insert-char ?\n 1)
      (insert-char ?= 40)
      (list (buffer-string)
            (buffer-size)
            (count-lines (point-min) (point-max))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"========================================\\nXXXXX\\n========================================\" 87 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-char with COUNT and INHERIT
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_with_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When INHERIT is non-nil, text properties from adjacent text
    // are inherited.  We verify the insertion itself is correct and
    // that text-property inheritance does not change the string content.
    let form = r#"(with-temp-buffer
      (insert (propertize "hello" 'face 'bold))
      (goto-char (point-max))
      (insert-char ?! 3 t)
      (let ((result-str (buffer-string))
            (face-at-end (get-text-property (- (point-max) 1) 'face)))
        (list result-str face-at-end (buffer-size))))"#;
    let expect = expect_test::expect![[r#""OK (#(\"hello!!!\" 0 8 (face bold)) bold 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-char with zero count
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_zero_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "before")
      (insert-char ?Z 0)
      (insert "after")
      (list (buffer-string) (buffer-size) (point)))"#;
    let expect = expect_test::expect![[r#""OK (\"beforeafter\" 11 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unicode characters: CJK, accented, mathematical symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert various Unicode characters by codepoint:
    // #x4e16 = CJK "world", #xe9 = e-acute, #x3b1 = Greek alpha,
    // #x2211 = summation sign, #x1f600 = grinning face emoji
    let form = r#"(with-temp-buffer
      (insert-char #x4e16 1)
      (insert-char #x754c 1)
      (insert-char ?\  1)
      (insert-char #xe9 3)
      (insert-char ?\  1)
      (insert-char #x3b1 1)
      (insert-char #x3b2 1)
      (insert-char #x3b3 1)
      (insert-char ?\  1)
      (insert-char #x2211 2)
      (let ((content (buffer-string)))
        (list content
              (length content)
              (string-bytes content))))"#;
    let expect = expect_test::expect![[r#""OK (\"世界 ééé αβγ ∑∑\" 13 27)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-char at different buffer positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert at beginning, middle, and end, verifying that point
    // advances correctly and surrounding text is preserved.
    let form = r#"(with-temp-buffer
      (insert "ABCDE")
      ;; Insert at beginning
      (goto-char (point-min))
      (insert-char ?[ 1)
      ;; Insert at end
      (goto-char (point-max))
      (insert-char ?] 1)
      ;; Insert in middle (between C and D, now at pos 5 since we added '[')
      (goto-char 5)
      (insert-char ?| 3)
      ;; Insert at new point-min
      (goto-char (point-min))
      (insert-char ?> 1)
      (list (buffer-string) (buffer-size) (point)))"#;
    let expect = expect_test::expect![[r#""OK (\">[ABC|||DE]\" 11 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build a ruler/grid using insert-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_build_ruler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a bordered grid with numbered columns, using only insert-char
    // and insert for the column numbers.
    let form = r#"(with-temp-buffer
      (let ((width 10)
            (height 5))
        ;; Top border
        (insert-char ?+ 1)
        (insert-char ?- width)
        (insert-char ?+ 1)
        (insert-char ?\n 1)
        ;; Body rows
        (let ((row 0))
          (while (< row height)
            (insert-char ?| 1)
            (if (= (% row 2) 0)
                (progn
                  (insert-char ?. width))
              (insert-char ?\  width))
            (insert-char ?| 1)
            (insert-char ?\n 1)
            (setq row (1+ row))))
        ;; Bottom border
        (insert-char ?+ 1)
        (insert-char ?- width)
        (insert-char ?+ 1)
        (insert-char ?\n 1)
        ;; Column ruler below
        (insert-char ?\  1)
        (let ((col 0))
          (while (< col width)
            (insert (number-to-string (% col 10)))
            (setq col (1+ col))))
        (insert-char ?\n 1)
        (list (buffer-string)
              (count-lines (point-min) (point-max)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"+----------+\\n|..........|\\n|          |\\n|..........|\\n|          |\\n|..........|\\n+----------+\\n 0123456789\\n\" 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: repeat-char dispatch table with unwind-protect
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dispatch table mapping symbol names to repeat-char operations.
    // Each entry specifies a char and a count.  Apply them in sequence to
    // construct a visual "barcode" pattern.  Uses fset + fmakunbound cleanup.
    let form = r#"(progn
  (fset 'neovm--test-ic-dispatch
    (lambda (instructions)
      (with-temp-buffer
        (dolist (instr instructions)
          (let ((ch (car instr))
                (count (cadr instr)))
            (insert-char ch count)))
        (buffer-string))))

  (unwind-protect
      (let ((barcode-spec
             '((?# 3) (?\  2) (?# 1) (?\  1) (?# 4) (?\  2) (?# 2)
               (?\  1) (?# 1) (?\  3) (?# 5)))
            (separator-spec
             '((?- 25)))
            (header-spec
             '((?* 3) (?\  1) (?B 1) (?A 1) (?R 1) (?C 1) (?O 1)
               (?D 1) (?E 1) (?\  1) (?* 3))))
        (list
         (funcall 'neovm--test-ic-dispatch header-spec)
         (funcall 'neovm--test-ic-dispatch separator-spec)
         (funcall 'neovm--test-ic-dispatch barcode-spec)
         (funcall 'neovm--test-ic-dispatch separator-spec)
         (length barcode-spec)))
    (fmakunbound 'neovm--test-ic-dispatch)))"#;
    let expect = expect_test::expect![[
        r####""OK (\"*** BARCODE ***\" \"-------------------------\" \"###  # ####  ## #   #####\" \"-------------------------\" 11)""####
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-char with large count for stress / edge
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_large_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert-char ?A 500)
      (insert-char ?B 500)
      (let ((str (buffer-string)))
        (list (length str)
              (string= (substring str 0 3) "AAA")
              (string= (substring str 499 502) "ABB")
              (string= (substring str 997 1000) "BBB"))))"#;
    let expect = expect_test::expect![[r#""OK (1000 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
