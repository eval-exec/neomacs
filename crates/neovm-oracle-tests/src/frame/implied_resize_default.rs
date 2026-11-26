//! GNU frame.c:7684–7692 selects this default by compiled window-system profile,
//! including in batch mode: both the NS and GTK3 references use only tab bars.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_frame_inhibit_implied_resize_uses_ns_and_gtk3_build_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        "frame-inhibit-implied-resize",
        expect_test::expect![[r#""OK (tab-bar-lines)""#]],
    );
}

#[test]
fn oracle_frame_inhibit_implied_resize_remains_special_and_restores_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(list frame-inhibit-implied-resize
      (special-variable-p 'frame-inhibit-implied-resize)
      (let ((frame-inhibit-implied-resize nil))
        frame-inhibit-implied-resize)
      frame-inhibit-implied-resize)
"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK ((tab-bar-lines) t nil (tab-bar-lines))""#]],
    );
}
