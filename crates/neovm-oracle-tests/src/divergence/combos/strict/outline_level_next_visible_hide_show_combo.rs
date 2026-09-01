//! Strict combo oracle probes, batch 245: outline navigation. outline-mode
//! heading levels, outline-next/previous-visible-heading, outline-forward/
//! backward-same-level, and outline-level.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_outline_level_next_previous_visible_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'outline)
(with-temp-buffer
  (outline-mode)
  (insert "* H1\n** H1.1\ntext\n* H2\n** H2.1\n*** H2.1.1\nmore\n* H3\n")
  (goto-char 1)
  (let ((lvl1 (outline-level))
        (next1 (progn (outline-next-visible-heading 1) (point)))
        (lvl-next1 (outline-level))
        (next2 (progn (outline-next-visible-heading 1) (point)))
        (prev (progn (outline-previous-visible-heading 1) (point))))
    (list lvl1 next1 lvl-next1 next2 prev)))
"##;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 0 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_outline_forward_backward_same_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'outline)
(with-temp-buffer
  (outline-mode)
  (insert "* A\n** A.1\n* B\n** B.1\n* C\n")
  (goto-char 1)
  (let ((fwd (progn (outline-forward-same-level 1) (point)))
        (back (progn (outline-backward-same-level 1) (point))))
    (list fwd back)))
"##;
    let expect = expect_test::expect![[r#""OK (12 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_outline_hide_body_show_all_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'outline)
(with-temp-buffer
  (outline-mode)
  (insert "* H1\nbody1\n** H1.1\nbody2\n* H2\nbody3\n")
  (goto-char (point-min))
  (let ((visible-count (count-lines (point-min) (point-max))))
    (outline-hide-body)
    (let ((hidden-text (buffer-string)))
      (outline-show-all)
      (list visible-count
            (> (length hidden-text) 0)
            (string-match "body1" (buffer-string))))))
"##;
    let expect = expect_test::expect![[r#""OK (6 t 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
