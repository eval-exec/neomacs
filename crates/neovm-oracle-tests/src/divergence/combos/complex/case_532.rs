/// Batch 532: terminal ops, display capabilities, and fontset operations.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx532_terminal_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"initial_terminal\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((term (frame-terminal (selected-frame))))
  (terminal-name term))
"##,
        expect,
    );
}

#[test]
fn div_cx532_terminal_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((term (frame-terminal (selected-frame))))
  (terminal-live-p term))
"##,
        expect,
    );
}

#[test]
fn div_cx532_terminal_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK value""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((term (frame-terminal (selected-frame))))
  (set-terminal-parameter term 'test 'value)
  (terminal-parameter term 'test))
"##,
        expect,
    );
}

#[test]
fn div_cx532_device_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((term (frame-terminal (selected-frame))))
  (device-class term))
"##,
        expect,
    );
}

#[test]
fn div_cx532_display_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function display-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (display-type) (display-supports-face-attributes-p '(:weight bold)))
"##,
        expect,
    );
}

#[test]
fn div_cx532_display_visual() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK static-gray""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(display-visual-class)
"##,
        expect,
    );
}

#[test]
fn div_cx532_display_color_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (display-color-cells) (display-color-p))
"##,
        expect,
    );
}

#[test]
fn div_cx532_display_pixel_dim() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (display-pixel-width) (display-pixel-height))
"##,
        expect,
    );
}

#[test]
fn div_cx532_display_mm_dim() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (display-mm-width) (display-mm-height))
"##,
        expect,
    );
}

#[test]
fn div_cx532_display_save_under() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK not-useful""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(display-save-under)
"##,
        expect,
    );
}

#[test]
fn div_cx532_display_backing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK not-useful""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(display-backing-store)
"##,
        expect,
    );
}

#[test]
fn div_cx532_fontset_font() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (fontset-font (fontset-default) 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx532_fontset_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"-*-*-*-*-*-*-*-*-*-*-*-*-fontset-default\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (fontset-list)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx532_fontset_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (new-fontset "cx532-fs" (fontset-spec "Monospace" 'ascii))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx532_frame_fontset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"tty\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (frame-parameter (selected-frame) 'font)
  (error (car e)))
"##,
        expect,
    );
}
