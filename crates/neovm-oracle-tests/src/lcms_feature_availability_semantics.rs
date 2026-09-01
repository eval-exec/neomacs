//! Oracle parity tests for GNU LCMS feature-gated primitives.
//!
//! GNU implements these in `src/lcms.c` under `#ifdef HAVE_LCMS2`; when the
//! local GNU binary is built without LCMS2, the symbols are not fbound.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_lcms_primitives_follow_gnu_build_feature_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (fboundp 'lcms2-available-p)
 (fboundp 'lcms-temp->white-point)
 (condition-case err
     (lcms-temp->white-point 6500)
   (error (cons (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[r#""OK (t t (0.9501657210094707 1.0 1.087653624622009))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
