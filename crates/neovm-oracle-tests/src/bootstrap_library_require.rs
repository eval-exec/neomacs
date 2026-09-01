//! Oracle parity tests for bootstrap/runtime library require surface.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_bootstrap_library_require_surface() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (featurep 'cl-lib)
  (condition-case err (require 'cl-lib) (error err))
  (featurep 'cl-lib)
  (fboundp 'cl-subseq)
  (autoloadp (symbol-function 'cl-subseq))
  (featurep 'gv)
  (condition-case err (require 'gv) (error err))
  (featurep 'gv)
  (fboundp 'gv-define-setter)
  (macrop 'gv-define-setter)
  (featurep 'seq)
  (condition-case err (require 'seq) (error err))
  (featurep 'seq)
  (featurep 'cl-generic)
  (condition-case err (require 'cl-generic) (error err))
  (featurep 'cl-generic))"#;
    let expect =
        expect_test::expect![[r#""OK (nil cl-lib t t t nil gv t t t t seq t t cl-generic t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "(nil cl-lib t t t nil gv t t t t seq t t cl-generic t)",
        &oracle,
        &neovm,
    );
}
