//! Strict combo oracle probes, batch 330: overlay modification-hooks +
//! invisible + atomic entity. before/after-string overlays, modification-hooks
//! firing, insert-behind/in-front-hooks, and invisible overlay + filter-buffer.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_overlay_modification_hooks_fire() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((fired nil))
    (let ((o (make-overlay 3 7)))
      (overlay-put o 'modification-hooks
                   (list (lambda (ov after-p beg end &optional length)
                           (push (if after-p 'after 'before) fired))))
      (goto-char 5)
      (insert "X")
      (delete-region 3 4))
    (nreverse fired)))
"##;
    let expect = expect_test::expect![[r#""OK (before after before after)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_overlay_insert_in_front_behind_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "MIDDLE")
  (let ((front-fired 0) (behind-fired 0))
    (let ((o (make-overlay 1 6)))
      (overlay-put o 'insert-in-front-hooks
                   (list (lambda (&rest _) (setq front-fired (1+ front-fired)))))
      (overlay-put o 'insert-behind-hooks
                   (list (lambda (&rest _) (setq behind-fired (1+ behind-fired)))))
      (goto-char 1)
      (insert "F")
      (goto-char (point-max))
      (insert "B"))
    (list front-fired behind-fired)))
"##;
    let expect = expect_test::expect![[r#""OK (2 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_overlay_invisible_filter_buffer_atomic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "visible HIDDEN visible")
  (let ((o (make-overlay 8 14)))
    (overlay-put o 'invisible t)
    (list (buffer-substring 1 22)
          (filter-buffer-substring 1 22 nil)
          (filter-buffer-substring 1 22 t)
          (buffer-substring-no-properties 1 22))))
"##;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 22)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
