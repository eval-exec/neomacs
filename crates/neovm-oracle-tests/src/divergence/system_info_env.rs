//! Divergence tests: env vars, locale, system info deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_environment_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'getenv)
  (fboundp 'setenv)
  (stringp (getenv "HOME"))
  (stringp (getenv "PATH"))
  (listp process-environment)) "#,
        expect,
    );
}

#[test]
fn divergence_locale() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'locale-info)
  (fboundp 'set-locale-environment)
  (boundp 'system-messages-locale)
  (boundp 'system-time-locale)
  (fboundp 'current-locale)) "#,
        expect,
    );
}

#[test]
fn divergence_system_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'system-name)
  (stringp (system-name))
  (fboundp 'emacs-pid)
  (integerp (emacs-pid))
  (fboundp 'emacs-version)
  (stringp (emacs-version))
  (fboundp 'emacs-build-time)
  (fboundp 'emacs-build-number)) "#,
        expect,
    );
}

#[test]
fn divergence_configuration_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'configuration-options)
  (fboundp 'system-configuration)
  (stringp system-configuration)
  (fboundp 'system-configuration-features)
  (stringp system-configuration-features)) "#,
        expect,
    );
}

#[test]
fn divergence_user_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'user-login-name)
  (fboundp 'user-full-name)
  (fboundp 'user-real-login-name)
  (fboundp 'user-real-uid)
  (stringp user-login-name)
  (stringp (user-full-name))
  (integerp (user-real-uid))) "#,
        expect,
    );
}

#[test]
fn divergence_path_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'exec-path)
  (listp exec-path)
  (boundp 'load-path)
  (listp load-path)
  (boundp 'exec-suffixes)
  (listp exec-suffixes)) "#,
        expect,
    );
}

#[test]
fn divergence_data_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'data-directory)
  (stringp data-directory)
  (boundp 'user-emacs-directory)
  (stringp user-emacs-directory)
  (boundp 'user-init-file)
  (boundp 'user-emacs-directory)) "#,
        expect,
    );
}

#[test]
fn divergence_invocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'invocation-name)
  (stringp invocation-name)
  (boundp 'invocation-directory)
  (stringp invocation-directory)
  (boundp 'command-line-args)
  (listp command-line-args)) "#,
        expect,
    );
}

#[test]
fn divergence_memory_info_func() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'memory-use-counts)
  (fboundp 'memory-limit)
  (fboundp 'gc-cons-threshold)
  (integerp gc-cons-threshold)
  (fboundp 'garbage-collect)
  (listp (garbage-collect))) "#,
        expect,
    );
}

#[test]
fn divergence_feature_checks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (featurep 'emacs)) "#,
        expect,
    );
}
