//! Oracle parity tests for GNU delayed warning helper semantics.
//!
//! GNU implements `delay-warning` and `collapse-delayed-warnings` in
//! `lisp/subr.el`.  These helpers mutate `delayed-warnings-list`; exact
//! ordering and duplicate collapse behavior matters during startup.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_delay_warning_pushes_full_warning_records() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((delayed-warnings-list nil))
  (list
   (delay-warning 'alpha "one")
   delayed-warnings-list
   (delay-warning 'beta "two" :warning "*buf*")
   delayed-warnings-list
   (delay-warning 'gamma "three" nil nil)
   delayed-warnings-list))
"#;

    let expect = expect_test::expect![[
        r#""OK (((alpha \"one\" nil nil)) ((alpha \"one\" nil nil)) ((beta \"two\" :warning \"*buf*\") (alpha \"one\" nil nil)) ((beta \"two\" :warning \"*buf*\") (alpha \"one\" nil nil)) ((gamma \"three\" nil nil) (beta \"two\" :warning \"*buf*\") (alpha \"one\" nil nil)) ((gamma \"three\" nil nil) (beta \"two\" :warning \"*buf*\") (alpha \"one\" nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_collapse_delayed_warnings_only_merges_adjacent_duplicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((delayed-warnings-list
       '((alpha "one" :warning nil)
         (alpha "one" :warning nil)
         (beta "two" nil "*buf*")
         (alpha "one" :warning nil)
         (gamma "three" nil nil)
         (gamma "three" nil nil)
         (gamma "three" nil nil))))
  (list
   (collapse-delayed-warnings)
   delayed-warnings-list))
"#;

    let expect = expect_test::expect![[
        r#""OK (((alpha \"one [2 times]\" :warning nil) (beta \"two\" nil \"*buf*\") (alpha \"one\" :warning nil) (gamma \"three [3 times]\" nil nil)) ((alpha \"one [2 times]\" :warning nil) (beta \"two\" nil \"*buf*\") (alpha \"one\" :warning nil) (gamma \"three [3 times]\" nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
