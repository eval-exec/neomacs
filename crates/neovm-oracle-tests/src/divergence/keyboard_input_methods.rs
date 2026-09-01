//! Divergence tests: keyboard input, key translation, input methods deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_key_translation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'keyboard-translate)
  (fboundp 'local-set-key)
  (fboundp 'global-set-key)
  (fboundp 'define-key))"#,
        expect,
    );
}

#[test]
fn divergence_input_methods() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'activate-input-method)
  (fboundp 'deactivate-input-method)
  (fboundp 'current-input-method)
  (boundp 'current-input-method)
  (featurep 'leim))"#,
        expect,
    );
}

#[test]
fn divergence_quail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'quail-select-package)
  (fboundp 'quail-set-keyboard-layout)
  (featurep 'quail))"#,
        expect,
    );
}

#[test]
fn divergence_input_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'input-decode-map)
  (fboundp 'local-function-key-map)
  (fboundp 'function-key-map)
  (boundp 'input-decode-map))"#,
        expect,
    );
}

#[test]
fn divergence_key_maps_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t ((97 . foo) keymap) (keymap))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map (make-sparse-keymap)))
  (define-key map "a" 'foo)
  (set-keymap-parent map (make-sparse-keymap))
  (list (keymapp map)
        (cdr map)
        (keymap-parent map))) "#,
        expect,
    );
}

#[test]
fn divergence_event_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'eventp)
  (fboundp 'event-start)
  (fboundp 'event-end)
  (fboundp 'event-basic-type)
  (fboundp 'event-modifiers)
  (fboundp 'read-event)
  (fboundp 'read-key))"#,
        expect,
    );
}

#[test]
fn divergence_recent_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'recent-keys)
  (fboundp 'this-command-keys)
  (fboundp 'this-command-keys-vector)
  (fboundp 'clear-this-command-keys))"#,
        expect,
    );
}

#[test]
fn divergence_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'key-description)
  (fboundp 'describe-buffer-bindings)
  (fboundp 'where-is-internal)
  (stringp (key-description [?a ?b]))) "#,
        expect,
    );
}

#[test]
fn divergence_parse_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'event-convert-list)
  (fboundp 'event-apply-modifier)
  (fboundp 'event-apply-hyper-modifier))"#,
        expect,
    );
}

#[test]
fn divergence_keyboard_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'set-keyboard-coding-system)
  (fboundp 'keyboard-coding-system)
  (boundp 'keyboard-coding-system))"#,
        expect,
    );
}
