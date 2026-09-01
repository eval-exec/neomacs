//! Divergence tests: text properties sticky, category, face deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_text_property_any() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 6 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'face 'bold)
  (list (text-property-any 1 12 'face 'bold)
        (text-property-not-all 1 12 'face nil)
        (text-property-any 1 12 'face nil)
        (text-property-not-all 1 6 'face nil))) "#,
        expect,
    );
}

#[test]
fn divergence_next_single_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 nil 6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'face 'bold)
  (list (next-single-property-change 1 'face)
        (next-single-property-change 6 'face)
        (previous-single-property-change 12 'face)
        (previous-single-property-change 7 'face))) "#,
        expect,
    );
}

#[test]
fn divergence_text_property_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil test-val test-val nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abcdef")
  (put-text-property 2 4 'test-prop 'test-val)
  (list (get-text-property 1 'test-prop)
        (get-text-property 2 'test-prop)
        (get-text-property 3 'test-prop)
        (get-text-property 4 'test-prop)
        (get-text-property 5 'test-prop)
        (get-text-property 6 'test-prop))) "#,
        expect,
    );
}

#[test]
fn divergence_set_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold bold (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (set-text-properties 1 6 '(face bold))
  (list (get-text-property 1 'face)
        (get-text-property 3 'face)
        (get-text-property 5 'face)
        (text-properties-at 1))) "#,
        expect,
    );
}

#[test]
fn divergence_remove_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (put-text-property 1 6 'face 'bold)
  (remove-text-properties 1 6 '(face))
  (list (get-text-property 1 'face)
        (text-properties-at 1))) "#,
        expect,
    );
}

#[test]
fn divergence_add_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold italic italic italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (add-text-properties 1 3 '(face bold))
  (add-text-properties 3 6 '(face italic))
  (list (get-text-property 1 'face)
        (get-text-property 2 'face)
        (get-text-property 3 'face)
        (get-text-property 4 'face)
        (get-text-property 5 'face))) "#,
        expect,
    );
}

#[test]
fn divergence_sticky_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (put-text-property 1 6 'rear-nonsticky t)
  (put-text-property 1 6 'front-sticky nil)
  (list (get-text-property 1 'rear-nonsticky)
        (get-text-property 1 'front-sticky))) "#,
        expect,
    );
}

#[test]
fn divergence_field_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (greeting nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'field 'greeting)
  (list (get-text-property 1 'field)
        (get-text-property 6 'field)
        (fboundp 'field-beginning)
        (fboundp 'field-end))) "#,
        expect,
    );
}

#[test]
fn divergence_invisible_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'invisible t)
  (list (get-text-property 1 'invisible)
        (get-text-property 6 'invisible)
        (get-text-property 7 'invisible)
        (boundp 'buffer-invisibility-spec))) "#,
        expect,
    );
}

#[test]
fn divergence_intangible_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'intangible t)
  (list (get-text-property 1 'intangible)
        (get-text-property 6 'intangible)
        (get-text-property 7 'intangible))) "#,
        expect,
    );
}
