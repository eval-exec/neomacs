//! Divergence tests: kmacro, macro-counter, macro-ring.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_kmacro_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable kmacro-counter)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'kmacro-start-macro)
  (fboundp 'kmacro-end-macro)
  (fboundp 'kmacro-call-macro)
  (fboundp 'kmacro-insert-counter)
  (boundp 'kmacro-counter)
  (integerp kmacro-counter))"#,
        expect,
    );
}

#[test]
fn divergence_kmacro_ring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable kmacro-ring)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'kmacro-ring)
  (listp kmacro-ring)
  (fboundp 'kmacro-cycle-ring-next)
  (fboundp 'kmacro-cycle-ring-previous))"#,
        expect,
    );
}

#[test]
fn divergence_keyboard_macros_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'last-kbd-macro)
  (boundp 'defining-kbd-macro)
  (booleanp defining-kbd-macro)
  (fboundp 'end-kbd-macro)
  (fboundp 'call-last-kbd-macro))"#,
        expect,
    );
}

#[test]
fn divergence_edmacro_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'edmacro-parse-keys)
  (fboundp 'edmacro-format-keys)
  (featurep 'edmacro))"#,
        expect,
    );
}

#[test]
fn divergence_repeat_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'repeat-complex-command)
  (fboundp 'repeat)
  (boundp 'repeat-mode)
  (featurep 'repeat))"#,
        expect,
    );
}

#[test]
fn divergence_recentf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'recentf-mode)
  (fboundp 'recentf-add-file)
  (featurep 'recentf))"#,
        expect,
    );
}

#[test]
fn divergence_savehist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'savehist-mode)
  (boundp 'savehist-file)
  (featurep 'savehist))"#,
        expect,
    );
}

#[test]
fn divergence_autorevert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'auto-revert-mode)
  (fboundp 'global-auto-revert-mode)
  (featurep 'autorevert))"#,
        expect,
    );
}

#[test]
fn divergence_saveplace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'save-place-mode)
  (featurep 'saveplace))"#,
        expect,
    );
}

#[test]
fn divergence_desktop_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'desktop-save-mode)
  (fboundp 'desktop-save)
  (featurep 'desktop))"#,
        expect,
    );
}
