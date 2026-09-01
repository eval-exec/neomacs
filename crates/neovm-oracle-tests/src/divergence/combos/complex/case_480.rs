/// Batch 480: info, man, woman, helpful, widget, wid-edit deep, customize deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx480_info_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'info)
  (list (fboundp 'info) (boundp 'Info-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_man_background() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'man)
  (list (fboundp 'man) (boundp 'Man-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_create_editable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"default\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (with-temp-buffer
    (let ((w (widget-create 'editable-field "default")))
      (widget-value w))))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_value_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"updated\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (with-temp-buffer
    (let ((w (widget-create 'editable-field "initial")))
      (widget-value-set w "updated")
      (widget-value w))))
"##,
        expect,
    );
}

#[test]
fn div_cx480_customize_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cus-edit)
  (list (fboundp 'customize) (boundp 'custom-buffer-style)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_customize_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cus-edit)
  (list (fboundp 'customize-option) (fboundp 'customize-save-customized)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (widget-match-inline (widget-create 'editable-field "hello") '(hello)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function widget-documentation)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (widget-documentation "test"))
"##,
        expect,
    );
}

#[test]
fn div_cx480_custom_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"custom-theme\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'custom-theme)
  (list (boundp 'custom-theme-load-path) (fboundp 'custom-theme-set-faces)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_custom_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cus-edit)
  (fboundp 'custom-save-all))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_color() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK color""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (widget-type (widget-create 'color "red")))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (widget-value (widget-create 'checkbox nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_radio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (let ((w (widget-create 'radio-button-choice
            '(item "A") '(item "B") '(item "C"))))
    (widget-value w)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_menu_choice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (let ((w (widget-create 'menu-choice '(item "A") '(item "B"))))
    (widget-value w)))
"##,
        expect,
    );
}

#[test]
fn div_cx480_widget_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (link :args nil :value \"click me\" :button-prefix \"\" :button-suffix \"\" :button-overlay #<overlay from 1 to 9 in *scratch*> :from #<marker (moves after insertion) at 1 in *scratch*> :to #<marker at 9 in *scratch*>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (widget-create 'link :button-prefix "" :button-suffix "" "click me"))
"##,
        expect,
    );
}
