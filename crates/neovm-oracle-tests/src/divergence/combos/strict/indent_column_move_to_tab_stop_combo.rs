//! Strict combo oracle probes, batch 237: indentation + column APIs.
//! indent-to, current-indentation, move-to-column, current-column, and
//! tab-to-tab-stop / indent-relative positioning.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_indent_to_current_indentation_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "line one\n  line two\n    line three")
  (list (current-indentation)
        (progn (goto-char 1) (current-column))
        (progn (goto-char 12) (current-column))
        (progn (goto-char 1) (indent-to 10) (current-column))
        (with-temp-buffer
          (insert "x")
          (indent-to 5 1)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""OK (4 0 2 10 \"x    \")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_move_to_column_force_tab_to_tab_stop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "abcdef")
  (goto-char 1)
  (let ((c3 (progn (move-to-column 3) (current-column)))
        (c0 (progn (move-to-column 0) (current-column)))
        (cforce (progn (move-to-column 8 t) (current-column))))
    (list c3 c0 cforce)))
"##;
    let expect = expect_test::expect![[r#""OK (3 0 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_indent_relative_line_prefix_indent_rigidly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "  hello\n  world")
  (goto-char (point-min))
  (forward-line 1)
  (let ((rigid (progn (indent-rigidly (line-beginning-position) (line-end-position) 2)
                      (current-indentation))))
    (list rigid
          (buffer-substring (line-beginning-position) (line-end-position))
          (progn (indent-rigidly (line-beginning-position) (line-end-position) -2)
                 (current-indentation)))))
"##;
    let expect = expect_test::expect![[r#""OK (4 \"    world\" 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
