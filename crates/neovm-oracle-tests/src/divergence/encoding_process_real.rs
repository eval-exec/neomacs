//! Divergence tests: real encoding/decoding behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_encode_decode_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello World\" 11)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"Hello World\")
  (encode-coding-region 1 12 'utf-8)
  (list (buffer-string)
        (length (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_coding_system_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 iso-latin-1 no-conversion t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (coding-system-base 'utf-8)
  (coding-system-base 'utf-8-dos)
  (coding-system-base 'latin-1)
  (coding-system-base 'no-conversion)
  (coding-system-p 'utf-8)
  (coding-system-p 'nonexistent-cs-xxx)) ",
        expect,
    );
}

#[test]
fn divergence_coding_system_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((cs (coding-system-priority-list)))
  (list (car cs)
        (>= (length cs) 1)
        (eq (car cs) 'utf-8))) ",
        expect,
    );
}

#[test]
fn divergence_charset_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil 65 65)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (charsetp 'ascii)
  (charsetp 'unicode)
  (charsetp 'nonexistent-xxx)
  (encode-char ?A 'ascii)
  (decode-char 'ascii 65)) ",
        expect,
    );
}

#[test]
fn divergence_string_encode_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"café\" \"café\" t 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((s \"caf\\u00e9\"))
  (list (encode-coding-string s 'utf-8)
        (decode-coding-string (encode-coding-string s 'utf-8) 'utf-8)
        (string= s (decode-coding-string (encode-coding-string s 'utf-8) 'utf-8))
        (length (encode-coding-string s 'utf-8)))) ",
        expect,
    );
}

#[test]
fn divergence_process_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (stringp (getenv \"HOME\"))
  (stringp (getenv \"PATH\"))
  (stringp (getenv \"SHELL\"))
  (> (length (getenv \"PATH\")) 10)
  (> (length process-environment) 5)) ",
        expect,
    );
}

#[test]
fn divergence_shell_command_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((out (shell-command-to-string \"echo hello\")))
  (list (string-trim out)
        (string= (string-trim out) \"hello\"))) ",
        expect,
    );
}

#[test]
fn divergence_call_process_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((result (with-temp-buffer
                 (insert \"hello\")
                 (call-process-region (point-min) (point-max)
                                      \"cat\" t t)
                 (buffer-string))))
  (list (string-trim result)
        (string= (string-trim result) \"hello\"))) ",
        expect,
    );
}

#[test]
fn divergence_process_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (listp (process-list))
  (<= (length (process-list)) 0)
  (processp nil)) ",
        expect,
    );
}

#[test]
fn divergence_system_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable emacs-pid)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (stringp system-name)
  (stringp system-configuration)
  (memq system-type '(gnu/linux gnu darwin windows-nt))
  (integerp emacs-pid)
  (> emacs-pid 0)
  (stringp emacs-version)
  (>= (length emacs-version) 5)
  (integerp emacs-major-version)) ",
        expect,
    );
}
