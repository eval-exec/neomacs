//! Strict combo oracle probes, batch 191: overlay deep. make-overlay +
//! overlay-put/get (face, priority, window), overlay-start/end, overlays-at/in
//! with priority-sorted ordering, move-overlay, and delete-overlay.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_overlay_put_get_at_in_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ")
  (let ((o1 (make-overlay 3 7))
        (o2 (make-overlay 8 12))
        (o3 (make-overlay 5 15)))
    (overlay-put o1 'face 'bold)
    (overlay-put o1 'priority 5)
    (overlay-put o2 'face 'italic)
    (overlay-put o2 'priority 10)
    (overlay-put o3 'priority 1)
    (list (overlay-start o1)
          (overlay-end o1)
          (overlay-get o1 'face)
          (overlay-get o1 'priority)
          (length (overlays-at 5))
          (length (overlays-in 4 12))
          (mapcar #'overlay-start (sort (overlays-in 1 20)
                                        (lambda (a b) (< (overlay-start a) (overlay-start b)))))
          (overlayp o1)
          (overlayp 'not-overlay))))
"##;
    let expect = expect_test::expect![[r#""OK (3 7 bold 5 2 3 (3 5 8) t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_overlay_move_delete_reinsert_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((o1 (make-overlay 2 4))
        (o2 (make-overlay 5 7)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (list (length (overlays-in 1 10))
          (progn (move-overlay o1 8 9) (overlay-start o1))
          (progn (move-overlay o1 1 3) (overlay-start o1))
          (length (overlays-at 8))
          (progn (delete-overlay o1) (length (overlays-at 2)))
          (length (overlays-in 1 10))
          (overlay-buffer o2))))
"##;
    let expect = expect_test::expect![[r#""OK (2 8 1 0 0 1 #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_overlay_before_after_string_nested_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "MAIN")
  (let ((o (make-overlay 2 4)))
    (overlay-put o 'before-string "<<<")
    (overlay-put o 'after-string ">>>")
    (overlay-put o 'invisible t)
    (list (overlay-get o 'before-string)
          (overlay-get o 'after-string)
          (overlay-get o 'invisible)
          (make-overlay 4 4)
          (let ((zero-len (make-overlay 4 4)))
            (list (overlay-start zero-len) (overlay-end zero-len))))))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"<<<\" \">>>\" t #<overlay in no buffer> (4 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
