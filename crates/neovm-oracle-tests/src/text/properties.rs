//! Oracle parity tests for text properties.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_propertize_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(get-text-property 0 'face (propertize "hello" 'face 'bold))"#,
        expect,
    );
}

#[test]
fn oracle_prop_put_text_property_and_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((s (copy-sequence "hello")))
                    (put-text-property 0 3 'face 'italic s)
                    (list (get-text-property 0 'face s)
                          (get-text-property 3 'face s)))"####;
    let expect = expect_test::expect![[r#""OK (italic nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_properties_at() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(text-properties-at 0 (propertize "hi" 'a 1 'b 2))"####;
    let expect = expect_test::expect![[r#""OK (a 1 b 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_properties_at_bignum_position_saturates_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs textprop.c:validate_interval_range uses
    // CHECK_FIXNUM_COERCE_MARKER, whose buffer.c:fix_position accepts bignums
    // and saturates them before the range check.
    let form =
        r####"(text-properties-at 1000000000000000000000000000000 (propertize "hi" 'a 1))"####;
    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range 1000000000000000000000000000000 1000000000000000000000000000000)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_next_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((s (concat (propertize "abc" 'face 'bold) "def")))
                    (next-property-change 0 s))"####;
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_next_property_change_bignum_position_saturates_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs textprop.c:Fnext_property_change also validates POSITION via
    // validate_interval_range, so bignums are saturated by buffer.c:fix_position
    // before the object range check.
    let form =
        r####"(next-property-change 1000000000000000000000000000000 (propertize "hi" 'a 1))"####;
    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range 1000000000000000000000000000000 1000000000000000000000000000000)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_previous_property_change_bignum_position_saturates_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs textprop.c:Fprevious_property_change shares the same
    // validate_interval_range path for POSITION.
    let form = r####"(previous-property-change 1000000000000000000000000000000 (propertize "hi" 'a 1))"####;
    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range 1000000000000000000000000000000 1000000000000000000000000000000)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_propertize_multiple_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((s (propertize "test" 'face 'bold 'help-echo "tip")))
                    (list (get-text-property 0 'face s)
                          (get-text-property 0 'help-echo s)))"####;
    let expect = expect_test::expect![[r#""OK (bold \"tip\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_remove_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((s (propertize "hello" 'face 'bold)))
                    (remove-text-properties 0 5 '(face nil) s)
                    (get-text-property 0 'face s))"####;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_buffer_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert (propertize "hello" 'face 'bold))
                    (get-text-property 1 'face))"####;
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_propertize_preserves_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(string-equal "hello" (propertize "hello" 'face 'bold))"####;
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}
