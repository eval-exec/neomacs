//! Oracle parity tests for the full bootstrapped runtime surface.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm_expect};

#[test]
fn oracle_prop_full_bootstrap_core_surface() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (not (null (get 'float 'cl--class)))
  (not (null (get 'integer 'cl--class)))
  (not (null (get 'float 'cl-deftype-satisfies)))
  (not (null (get 'integer 'cl-deftype-satisfies)))
    (coding-system-p 'iso-8859-15)
  (stringp system-configuration-features))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t t t t t t)", &oracle, &neovm);
}
