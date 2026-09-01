/// Batch 501: targeted characterization — display column with various display property values.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx501_display_col_short_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcde")
  (put-text-property 2 3 'display "X")
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_long_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcde")
  (put-text-property 2 3 'display "XXXXX")
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcde")
  (put-text-property 2 3 'display "")
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a\nb\nc")
  (put-text-property 2 3 'display "XXX")
  (goto-char 3)
  (current-column))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (let ((ov (make-overlay 2 3)))
    (overlay-put ov 'display "ZZ"))
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_both_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 1 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcd")
  (put-text-property 2 3 'display "TT")
  (let ((ov (make-overlay 3 4)))
    (overlay-put ov 'display "UU"))
  (list (current-column) (progn (goto-char 2) (current-column)) (progn (goto-char 3) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_space_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (put-text-property 2 3 'display '(space :width 5))
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_space_align() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (put-text-property 2 3 'display '(space :align-to 10))
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_relative_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (put-text-property 2 3 'display '(space :relative-width 2))
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_multiple_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a b c")
  (put-text-property 2 3 'display "XX")
  (put-text-property 4 5 'display "YYYY")
  (list (current-column) (progn (goto-char 2) (current-column)) (progn (goto-char 4) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_invisible_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcde")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'invisible t))
  (list (current-column) (progn (goto-char 2) (current-column)) (progn (goto-char 4) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_image_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (put-text-property 2 3 'display '(image :type xbm :data ""))
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_nil_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (put-text-property 2 3 'display nil)
  (list (current-column) (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_move_to_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a b c")
  (put-text-property 2 3 'display "XXXX")
  (list (progn (move-to-column 3) (point))
        (progn (move-to-column 5) (point))
        (progn (move-to-column 8) (point))))"##,
        expect,
    );
}

#[test]
fn div_cx501_display_col_move_to_column_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcde")
  (let ((ov (make-overlay 2 3)))
    (overlay-put ov 'display "XX"))
  (list (progn (move-to-column 3) (point))
        (progn (move-to-column 5) (point))))"##,
        expect,
    );
}
