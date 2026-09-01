//! Deep combo: condition-case + error symbols + hierarchy + signal + handler dispatch.
//! Tests error signaling and handler selection with nested condition-case.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_condition_case_parent_error_catches_child() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (test-child \"child message\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (define-error 'test-parent \"Test parent error\")\n\
         (define-error 'test-child \"Test child error\" 'test-parent)\n\
         (condition-case err\n\
         (signal 'test-child '(\"child message\"))\n\
         (test-parent\n\
         (list (car err) (cadr err)))))",
        expect,
    );
}

#[test]
fn deficiency_nested_condition_case_inner_handler_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (error \"re-signaled\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (condition-case outer\n\
         (condition-case inner\n\
         (signal 'error '(\"inner catch\"))\n\
         (error\n\
         (signal 'error '(\"re-signaled\"))))\n\
         (error\n\
         (list (car outer) (cadr outer)))))",
        expect,
    );
}

#[test]
fn deficiency_error_with_buffer_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" test-error)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ecb\"))\n\
         (cleanup-log nil))\n\
         (with-current-buffer buf\n\
         (insert \"DATA\")\n\
         (put-text-property 1 5 'src 'original))\n\
         (condition-case err\n\
         (with-current-buffer buf\n\
         (goto-char 3)\n\
         (insert \"XXX\")\n\
         (signal 'test-error '(\"abort\")))\n\
         (test-error\n\
         (push (list (car err) (cadr err)\n\
         (with-current-buffer buf (buffer-string)))\n\
         cleanup-log)))\n\
         cleanup-log)\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_condition_case_no_match_propagates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (wrong-type-argument \"expected integer\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (condition-case outer\n\
         (condition-case inner\n\
         (signal 'wrong-type-argument '(\"expected integer\"))\n\
         (void-variable\n\
         'should-not-reach))\n\
         (wrong-type-argument\n\
         (list (car outer) (cadr outer)))))",
        expect,
    );
}

#[test]
fn deficiency_signal_with_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (condition-case err\n\
         (signal 'args-out-of-range '(\"my-vector\" 5 10))\n\
         (args-out-of-range\n\
         (list (car err) (cdr err))))",
        expect,
    );
}

#[test]
fn deficiency_user_error_vs_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((user-error \"user message\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((user-caught nil)\n\
         (error-caught nil))\n\
         (condition-case err\n\
         (user-error \"user message\")\n\
         (error (setq error-caught (list (car err) (cadr err)))))\n\
         (list error-caught user-caught)))",
        expect,
    );
}

#[test]
fn deficiency_error_in_unwind_protect_still_runs_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" test-error)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((log nil))\n\
         (condition-case err\n\
         (unwind-protect\n\
         (progn\n\
         (push 'body log)\n\
         (signal 'test-error '(\"boom\")))\n\
         (push 'cleanup log))\n\
         (test-error\n\
         (push (car err) log)))\n\
         (nreverse log)))",
        expect,
    );
}

#[test]
fn deficiency_error_message_formatting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (condition-case err\n\
         (signal 'wrong-number-of-arguments '(my-fn 3 2))\n\
         (wrong-number-of-arguments\n\
         (error-message-string err)))",
        expect,
    );
}

#[test]
fn deficiency_condition_case_with_buffer_mods_and_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" test-error)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ecm\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCC\")\n\
         (let ((m6 (copy-marker 6)))\n\
         (put-text-property 1 5 'zone 'a)\n\
         (put-text-property 5 9 'zone 'b)\n\
         (put-text-property 9 13 'zone 'c)\n\
         (condition-case err\n\
         (progn\n\
         (goto-char 5)\n\
         (insert \"XXXX\")\n\
         (signal 'test-error '(\"mid-edit\")))\n\
         (test-error\n\
         (list (car err) (cadr err)\n\
         (buffer-string)\n\
         (marker-position m6)\n\
         (get-text-property 5 'zone))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_error_during_format_in_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (error \"Format specifier doesn’t match argument type\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (condition-case err\n\
         (format \"Hello %s %d\" \"world\" 'not-a-number)\n\
         (error\n\
         (list (car err) (cadr err)))))",
        expect,
    );
}
