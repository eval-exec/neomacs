//! Strict combo oracle probes, batch 188: line-position APIs. line-beginning/
//! end-position with relative N (0/1/-1), line-number-at-pos, count-lines
//! (incl final-newline and no-final-newline edges), and forward-line return
//! value.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_line_beginning_end_position_relative_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\nline4")
  (goto-char 1)
  (forward-line 2)
  (list (line-beginning-position)
        (line-end-position)
        (line-beginning-position 0)
        (line-beginning-position 1)
        (line-end-position 1)
        (line-beginning-position -1)
        (line-end-position -1)
        (line-number-at-pos)))
"##;
    let expect = expect_test::expect![[r#""OK (13 18 7 13 18 1 6 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_count_lines_final_newline_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "a\nb\nc\n")
        (count-lines (point-min) (point-max)))
      (with-temp-buffer
        (insert "a\nb\nc")
        (count-lines (point-min) (point-max)))
      (with-temp-buffer
        (insert "")
        (count-lines (point-min) (point-max)))
      (with-temp-buffer
        (insert "single line")
        (count-lines (point-min) (point-max)))
      (with-temp-buffer
        (insert "a\n\n\nc\n")
        (count-lines (point-min) (point-max)))
      (with-temp-buffer
        (insert "a\nb\nc\n")
        (line-number-at-pos (point-max))))
"##;
    let expect = expect_test::expect![[r#""OK (3 3 0 1 4 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_forward_line_return_value_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\n")
  (goto-char 1)
  (let ((r1 (forward-line 0))
        (p1 (point)))
    (forward-line 1)
    (let ((r2 (forward-line 1))
          (p2 (point)))
      (let ((r-end (forward-line 100)))
        (list r1 p1 r2 p2 r-end (point) (line-number-at-pos))))))
"##;
    let expect = expect_test::expect![[r#""OK (0 1 0 13 99 19 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
