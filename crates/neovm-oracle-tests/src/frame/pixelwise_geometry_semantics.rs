//! Oracle parity for pixelwise frame geometry helpers used by child-frame UIs.
//! GNU src/frame.c: `set-frame-size-and-position-pixelwise`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_set_frame_size_and_position_pixelwise_resizes_and_records_position_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (123 46 (0 . 0) 17 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (selected-frame)))
  (set-frame-size-and-position-pixelwise f 123 45 17 19)
  (list (frame-pixel-width f)
        (frame-pixel-height f)
        (frame-position f)
        (frame-parameter f 'left)
        (frame-parameter f 'top)))"#,
        expect,
    );
}
