//! Divergence tests: faces deep - face remapping, face inheritance, theme.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_face_all_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (consp (face-all-attributes 'default))
  (consp (face-all-attributes 'bold))
  (plist-get (face-all-attributes 'default) :family))"#,
        expect,
    );
}

#[test]
fn divergence_face_remapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'face-remap-add-relative)
  (fboundp 'face-remap-remove-relative)
  (listp (get 'default 'face-remapping)))"#,
        expect,
    );
}

#[test]
fn divergence_face_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((default) (bold default) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (member 'default (face-list))
  (member 'bold (face-list))
  (>= (length (face-list)) 10))"#,
        expect,
    );
}

#[test]
fn divergence_face_underline_colors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t unspecified)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (face-attribute 'underline :underline)
  (face-attribute 'highlight :background))"#,
        expect,
    );
}

#[test]
fn divergence_face_realized() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'face-spec-recalc)
  (fboundp 'face-spec-set)
  (facep 'my-test-face-xyz))"#,
        expect,
    );
}

#[test]
fn divergence_make_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (make-face 'my-test-face-123)
  (list (facep 'my-test-face-123)
        (face-attribute 'my-test-face-123 :family)))"#,
        expect,
    );
}

#[test]
fn divergence_face_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defface my-inherited-face '((t :inherit bold)) "test")
  (list (facep 'my-inherited-face)
        (plist-get (face-all-attributes 'my-inherited-face) :inherit)))"#,
        expect,
    );
}

#[test]
fn divergence_custom_theme_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'custom-set-faces)
  (fboundp 'custom-set-variables)
  (fboundp 'custom-theme-p)
  (fboundp 'load-theme)
  (fboundp 'enable-theme)
  (fboundp 'disable-theme))"#,
        expect,
    );
}

#[test]
fn divergence_custom_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'customize-set-variable)
  (fboundp 'customize-save-variable)
  (fboundp 'custom-variable-p)
  (boundp 'custom-file)
  (or (null custom-file) (stringp custom-file)))"#,
        expect,
    );
}

#[test]
fn divergence_defcustom_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((funcall #'(closure (t) nil 42))) 42 integer)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defcustom my-custom-var-xyz 42 "A test variable" :type 'integer)
  (list (custom-variable-p 'my-custom-var-xyz)
        my-custom-var-xyz
        (get 'my-custom-var-xyz 'custom-type)))"#,
        expect,
    );
}
