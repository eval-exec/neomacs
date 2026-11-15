//! Oracle parity tests for `get-display-property`.
//!
//! GNU `Fget_display_property` (xdisp.c:5877) searches SPEC inside the
//! `display' property via `find_display_property` (xdisp.c:5707) over the
//! TEXT property -- never overlays -- unless PROPERTIES is given. Every row
//! below was attested against the GNU oracle before being pinned.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm_expect};

#[test]
fn oracle_get_display_property_finds_the_spec_in_the_display_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2.0""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(with-temp-buffer (insert \"abcd\") \
         (put-text-property 2 4 'display '((height 2.0))) \
         (get-display-property 2 'height))",
        expect,
    );
    assert_ok_eq("2.0", &oracle, &neovm);
}

#[test]
fn oracle_get_display_property_simple_and_vector_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2.0""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(with-temp-buffer (insert \"abcd\") \
         (put-text-property 2 4 'display '(height 2.0)) \
         (get-display-property 2 'height))",
        expect,
    );
    assert_ok_eq("2.0", &oracle, &neovm);

    let expect = expect_test::expect![[r#""OK 2.0""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(with-temp-buffer (insert \"abcd\") \
         (put-text-property 2 4 'display [nil (height 2.0)]) \
         (get-display-property 2 'height))",
        expect,
    );
    assert_ok_eq("2.0", &oracle, &neovm);
}

#[test]
fn oracle_get_display_property_multi_value_and_valueless_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A longer spec yields the whole value list.
    let expect = expect_test::expect![[r#""OK (2.0 3.0)""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(with-temp-buffer (insert \"abcd\") \
         (put-text-property 2 4 'display '(height 2.0 3.0)) \
         (get-display-property 2 'height))",
        expect,
    );
    assert_ok_eq("(2.0 3.0)", &oracle, &neovm);

    // The cdr must be a cons: `(height)' and `(height . 2.0)' answer nil.
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(with-temp-buffer (insert \"abcd\") \
         (put-text-property 2 4 'display '(height)) \
         (get-display-property 2 'height))",
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(with-temp-buffer (insert \"abcd\") \
         (put-text-property 2 4 'display '(height . 2.0)) \
         (get-display-property 2 'height))",
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_get_display_property_ignores_overlays_and_honors_properties_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU reads Fget_text_property: an overlay carrying `display' is invisible.
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(with-temp-buffer (insert \"abcd\") \
         (overlay-put (make-overlay 1 3) 'display '((height 2.0))) \
         (get-display-property 2 'height))",
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);

    // PROPERTIES non-nil replaces the properties at POSITION.
    let expect = expect_test::expect![[r#""OK 2.0""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(
        "(get-display-property 1 'height nil '((height 2.0)))",
        expect,
    );
    assert_ok_eq("2.0", &oracle, &neovm);
}

#[test]
fn oracle_get_display_property_properties_must_be_a_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 42)""#]];
    let (oracle, neovm) =
        eval_oracle_and_neovm_expect("(get-display-property 1 'height nil 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
