//! Divergence tests: package system, ELPA, package management deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_package_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'package-initialize)
  (fboundp 'package-refresh-contents)
  (fboundp 'package-install)
  (fboundp 'package-delete)
  (fboundp 'package-list-packages)
  (featurep 'package))"#,
        expect,
    );
}

#[test]
fn divergence_package_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable package-archives)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'package-archives)
  (listp package-archives)
  (boundp 'package-archive-priorities)
  (listp package-archive-priorities)
  (boundp 'package-load-list)
  (listp package-load-list)) "#,
        expect,
    );
}

#[test]
fn divergence_package_desc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'package-desc-p)
  (fboundp 'package-desc-name)
  (fboundp 'package-desc-version)
  (fboundp 'package-desc-summary)) "#,
        expect,
    );
}

#[test]
fn divergence_package_activation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'package-activate)
  (fboundp 'package-built-in-p)
  (fboundp 'package--builtin-versions)
  (fboundp 'package--dependencies)) "#,
        expect,
    );
}

#[test]
fn divergence_use_package() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'use-package)
  (featurep 'use-package)
  (boundp 'use-package-always-ensure)
  (boundp 'use-package-verbose)) "#,
        expect,
    );
}

#[test]
fn divergence_package_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'package-menu-mark-install)
  (fboundp 'package-menu-mark-delete)
  (fboundp 'package-menu-mark-unmark)
  (fboundp 'package-menu-refresh)
  (featurep 'package))"#,
        expect,
    );
}

#[test]
fn divergence_elpa_melpa() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable package-archives)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (assoc "gnu" package-archives)
  (assoc "melpa" package-archives)
  (assoc "nongnu" package-archives)
  (listp package-archives)) "#,
        expect,
    );
}

#[test]
fn divergence_package_vc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'package-vc-install)
  (fboundp 'package-vc-install-from-checkout)
  (fboundp 'package-vc-delete)
  (featurep 'package-vc)) "#,
        expect,
    );
}

#[test]
fn divergence_package_quick() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable package-selected-packages)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'package-install-selected-packages)
  (fboundp 'package-autoremove)
  (boundp 'package-selected-packages)
  (listp package-selected-packages)) "#,
        expect,
    );
}

#[test]
fn divergence_ensure_packages() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'package--ensure-init-file)
  (fboundp 'package--save-selected-packages)
  (fboundp 'package-archive-base)
  (fboundp 'package-compute-transaction)) "#,
        expect,
    );
}
