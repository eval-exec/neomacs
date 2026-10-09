//! Initial-frame measurement keeps GNU's absolute-pixel stretch contract.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn initial_frame_absolute_pixel_stretch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn (erase-buffer)
             (insert (propertize " " 'display '(space :width (50))))
             (list (car (window-text-pixel-size nil 1 2 t))
                   (car (buffer-text-pixel-size nil nil t))))"#,
        expect_test::expect![[r#""OK (50 50)""#]],
    );
}

#[test]
fn stretch_width_precedes_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn (erase-buffer)
             (insert (propertize " " 'display '(space :width 2 :align-to (50))))
             (list (car (window-text-pixel-size nil 1 2 t))
                   (car (buffer-text-pixel-size nil nil t))))"#,
        expect_test::expect![[r#""OK (2 2)""#]],
    );
}
