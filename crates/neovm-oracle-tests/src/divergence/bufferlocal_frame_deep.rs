//! Divergence tests: buffer local variables, frame parameters deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buffer_local_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 t 42)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (set (make-local-variable 'test-blocal-xxx) 42)
  (list test-blocal-xxx
        (local-variable-p 'test-blocal-xxx)
        (buffer-local-value 'test-blocal-xxx (current-buffer)))) ",
        expect,
    );
}

#[test]
fn divergence_buffer_local_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq test-blocal-default-xxx 10)
  (set (make-local-variable 'test-blocal-default-xxx) 20)
  (list test-blocal-default-xxx
        (default-value 'test-blocal-default-xxx)
        (local-variable-p 'test-blocal-default-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_buffer_local_kill() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq test-blocal-kill-xxx 100)
  (set (make-local-variable 'test-blocal-kill-xxx) 200)
  (kill-local-variable 'test-blocal-kill-xxx)
  (list test-blocal-kill-xxx
        (local-variable-p 'test-blocal-kill-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_buffer_locals_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'buffer-local-variables)
  (listp (buffer-local-variables))
  (fboundp 'buffer-local-value)
  (fboundp 'buffer-bound-p)
  (fboundp 'default-boundp)) ",
        expect,
    );
}

#[test]
fn divergence_make_variable_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'make-variable-buffer-local)
  (fboundp 'make-local-variable)
  (fboundp 'kill-local-variable)
  (fboundp 'local-variable-p)
  (fboundp 'default-value)
  (fboundp 'set-default)) ",
        expect,
    );
}

#[test]
fn divergence_frame_params_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (t (name . \"F1\") (width . 80) (height . 25) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((params (frame-parameters)))
  (list (listp params)
        (assq 'name params)
        (assq 'width params)
        (assq 'height params)
        (assq 'fullscreen params))) ",
        expect,
    );
}

#[test]
fn divergence_frame_terminal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'frame-terminal)
  (fboundp 'terminal-name)
  (fboundp 'terminal-list)
  (fboundp 'terminal-live-p)) ",
        expect,
    );
}

#[test]
fn divergence_frame_focus() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'select-frame)
  (fboundp 'selected-frame)
  (fboundp 'redirect-frame-focus)
  (fboundp 'frame-focus)) ",
        expect,
    );
}

#[test]
fn divergence_frame_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'make-frame)
  (fboundp 'make-frame-on-display)
  (fboundp 'delete-frame)
  (fboundp 'frame-list)
  (fboundp 'next-frame)) ",
        expect,
    );
}

#[test]
fn divergence_frame_parameters_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'modify-frame-parameters)
  (fboundp 'set-frame-parameter)
  (fboundp 'frame-parameter)
  (fboundp 'frame-parameters)) ",
        expect,
    );
}
