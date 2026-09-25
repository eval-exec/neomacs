//! Oracle parity for `line-number-at-pos` under `selective-display`
//! (GNU `Fline_number_at_pos` -> `count_lines` -> `display_count_lines`,
//! src/xdisp.c): while `selective-display` is non-nil and not an integer, a
//! `\r` ends a line too.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_line_number_at_pos_counts_carriage_returns_under_selective_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (insert "a\rb\nc\rd\ne\r")
  (let ((at (lambda () (mapcar #'line-number-at-pos '(1 2 3 4 5 6 7 8 9 10 11)))))
    (list (funcall at)
          (progn (setq selective-display t) (funcall at))
          (progn (setq selective-display 2) (funcall at))
          (progn (setq selective-display 'hide) (funcall at))
          (progn (setq selective-display t)
                 (narrow-to-region 3 9)
                 (list (line-number-at-pos 9)
                       (line-number-at-pos 9 t)
                       (line-number-at-pos 5)))
          (progn (widen)
                 (list (count-lines 1 (point-max))
                       (progn (goto-char 1) (forward-line 2) (point))
                       (line-number-at-pos))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 1 1 1 2 2 2 2 3 3 3) (1 1 2 2 3 3 4 4 5 5 6) (1 1 1 1 2 2 2 2 3 3 3) (1 1 2 2 3 3 4 4 5 5 6) (4 5 2) (5 9 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
