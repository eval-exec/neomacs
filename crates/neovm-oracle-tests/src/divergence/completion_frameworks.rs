//! Divergence tests: completions, corfu, vertico, ido deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_vertico() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'vertico-mode)
  (featurep 'vertico)
  (fboundp 'vertico-next)
  (fboundp 'vertico-previous)) "#,
        expect,
    );
}

#[test]
fn divergence_consult() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'consult-buffer)
  (fboundp 'consult-line)
  (fboundp 'consult-ripgrep)
  (featurep 'consult)) "#,
        expect,
    );
}

#[test]
fn divergence_marginalia() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'marginalia-mode)
  (featurep 'marginalia)) "#,
        expect,
    );
}

#[test]
fn divergence_orderless() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'orderless-filter)
  (featurep 'orderless)) "#,
        expect,
    );
}

#[test]
fn divergence_embark() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'embark-act)
  (fboundp 'embark-dwim)
  (featurep 'embark)) "#,
        expect,
    );
}

#[test]
fn divergence_ido() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ido-mode)
  (fboundp 'ido-find-file)
  (fboundp 'ido-switch-buffer)
  (featurep 'ido)) "#,
        expect,
    );
}

#[test]
fn divergence_ivy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ivy-mode)
  (fboundp 'counsel-M-x)
  (featurep 'ivy)
  (featurep 'counsel)) "#,
        expect,
    );
}

#[test]
fn divergence_helm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'helm-M-x)
  (fboundp 'helm-find-files)
  (fboundp 'helm-buffers-list)
  (featurep 'helm)) "#,
        expect,
    );
}

#[test]
fn divergence_perspective() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'persp-mode)
  (featurep 'perspective)
  (fboundp 'eyebrowse-mode)
  (featurep 'eyebrowse)) "#,
        expect,
    );
}

#[test]
fn divergence_magit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'magit-stage-file)
  (fboundp 'magit-unstage-file)
  (fboundp 'magit-commit-create)
  (fboundp 'magit-push-current)
  (fboundp 'magit-pull-from-upstream)) "#,
        expect,
    );
}
