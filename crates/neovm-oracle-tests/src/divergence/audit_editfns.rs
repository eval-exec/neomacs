//! Source-audit divergences: editfns / format / field / count-lines / markers.
//!
//! From a direct GNU src/editfns.c (indent.c, etc.) vs neovm-core audit:
//! missing `format-spec` / `filter-buffer-substring`; format edge cases (%d on
//! float, %05.2d, %4c multibyte, %.0g); current-column ignores display
//! property; count-lines missing selective-display branch; position-bytes
//! clamps to narrowed end; field scan ignores stickiness; save-mark-and-excursion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_ae_format_spec_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"x\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (format-spec "%a" '((97 . "x"))) (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_ae_filter_buffer_substring_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hell\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer (insert "hello") (filter-buffer-substring 1 5))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_ae_format_precision_width_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"   03\"""#]];
    crate::common::assert_oracle_parity_expect(r##"(format "%05.2d" 3)"##, expect);
}

#[test]
fn div_ae_format_d_on_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"    3\" \"00003\")""#]];
    // GNU rewrites %d on a float to %f semantics (3.9 -> 3 with width 5).
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%5d" 3.9) (format "%05d" 3.9))"##,
        expect,
    );
}

#[test]
fn div_ae_format_c_multibyte_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"  ꀀ\" \"ꀀ\" t)""#]];
    // %c of a non-ASCII char + width/zero-flag interaction.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%4c" 40960)
      (format "%c" 40960)
      (multibyte-string-p (format "%c" 40960)))
"##,
        expect,
    );
}

#[test]
fn div_ae_format_g_zero_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0\" \"1.\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%.0g" 0.0) (format "%#.0g" 1.0))"##,
        expect,
    );
}

#[test]
fn div_ae_current_column_ignores_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    // GNU current-column honors the `display` text property (glyph width).
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "x")
  (put-text-property 1 2 'display "abc")
  (current-column))
"##,
        expect,
    );
}

#[test]
fn div_ae_count_lines_selective_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    // selective-display = t: \r[^\n] counts as a line boundary.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((selective-display t))
  (with-temp-buffer
    (insert "ab\rcd\nef")
    (count-lines 1 (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_ae_position_bytes_under_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 5)""#]];
    // GNU position-bytes works on full-buffer positions even when narrowed.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc\ndef")
  (narrow-to-region 1 4)
  (list (position-bytes 6) (condition-case e (position-bytes 5) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_ae_field_bounds_rear_nonsticky() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5)""#]];
    // field scan should honor rear-nonsticky at the field boundary.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAAABBBB")
  (put-text-property 1 5 'field 'a)
  (put-text-property 5 9 'field 'b)
  (put-text-property 4 5 'rear-nonsticky '(field))
  (list (field-beginning 6 nil) (field-end 4 nil)))
"##,
        expect,
    );
}

#[test]
fn div_ae_save_mark_and_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "abcdefgh")
      (save-mark-and-excursion
        (push-mark 3 t)
        (length mark-ring)))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_ae_mark_marker_relocation_on_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    // The mark-marker should relocate on insert like a real marker.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (set-marker (mark-marker) 3)
  (goto-char 3)
  (insert "X")
  (marker-position (mark-marker)))
"##,
        expect,
    );
}

#[test]
fn div_ae_move_to_column_with_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'display (vector ?x ?x ?x ?x ?x ?x))
  (move-to-column 5)
  (current-column))
"##,
        expect,
    );
}
