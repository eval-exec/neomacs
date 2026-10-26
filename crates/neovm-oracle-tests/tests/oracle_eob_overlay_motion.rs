//! Preserve GNU's intentionally separate batch/compute-motion engine.
use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn batch_eob_overlay_motion_preserves_accessible_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let (results)
             (dolist (narrowed '(nil t))
               (with-temp-buffer
                 (if narrowed
                     (progn (insert "PRE\nabc\ndef\nPOST") (narrow-to-region 5 12))
                   (insert "abc\ndef"))
                 (let ((overlay (make-overlay (point-max) (point-max))))
                   (overlay-put overlay 'after-string "\nX\nY"))
                 (goto-char (point-max))
                 (push (list (vertical-motion 1) (point) (point-min) (point-max)) results)))
             (nreverse results))"#,
        expect_test::expect![[r#""OK ((0 8 1 8) (0 12 5 12))""#]],
    );
}
