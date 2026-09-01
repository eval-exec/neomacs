//! combo_strict_26.rs — probing error paths: wrong arg counts,
//! wrong types, boundary conditions for nil/empty, and unusual
//! argument combinations to surface error-type divergences.
use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_error_org_entry_get_no_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (condition-case e (with-temp-buffer (org-mode) (org-entry-get nil "MISSING")) (error (list :e (car e)))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_set_tags_bad_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (condition-case e (with-temp-buffer (org-mode) (insert "* H\n") (org-set-tags 42)) (error (list :e (car e)))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_todo_wrong_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* TODO Task\n") (goto-char (point-min))
 (condition-case e (org-todo "INVALID-STATE") (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_schedule_past_date() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Scheduled to <2024-03-01 Fri>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case nil (org-schedule nil "<2024-02-30>") (error :bad-date))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_deadline_invalid() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Deadline on <2026-06-15 Mon>\"""#]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case nil (org-deadline nil "not-a-date") (error :bad-date))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_table_get_out_of_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "| a |\n| 1 |\n") (goto-char (point-min)) (forward-line 1)
 (condition-case e (org-table-get "ZZ" 42) (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_element_parse_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
 (condition-case e (org-element-parse-secondary-string "" 'bold) (error (list :e (car e)))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_timestamp_format_bad() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (let ((ts (org-timestamp-from-string "<2024-01-01>")))
 (condition-case nil (org-timestamp-format ts 42) (error :bad-format))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_priorities_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :bad-priority""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case nil (org-priority "A") (error :bad-priority))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_babel_execute_no_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core)
 (with-temp-buffer (org-mode) (condition-case e (org-babel-execute-src-block) (error (list :e (car e))))))"##,
        expect,
    );
}
