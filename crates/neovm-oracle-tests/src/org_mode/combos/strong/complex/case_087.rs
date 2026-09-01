use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo87_error_org_export_no_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox)
 (with-temp-buffer (org-mode) (condition-case e (org-export-as 'nonexistent nil nil t)
  (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_table_recalc_bad_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :recalc-ok""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "| a | b |\n| 1 | 2 |\n")
  (insert "#+TBLFM: $3=$1+$2\n") (goto-char (point-min))
   (condition-case e (progn (org-table-recalculate t) :recalc-ok) (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_clone_subtree_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* A\n") (goto-char (point-min))
 (condition-case nil (org-clone-subtree-with-time-shift 0) (error :bad-count))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_insert_link_no_desc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ol)
 (with-temp-buffer (org-mode) (condition-case nil (org-insert-link nil "https://x.com" nil)
  (error :bad-args))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_clock_goto_no_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-clock)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case e (org-clock-goto) (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_move_subtree_no_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :cannot-move""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case nil (org-move-subtree-up) (error :cannot-move))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_mark_subtree_no_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-subtree""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (goto-char (point-min))
 (condition-case nil (org-mark-subtree) (error :no-subtree))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_update_statistics_no_cookie() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<marker in no buffer>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case nil (org-update-statistics-cookies nil) (error :no-cookie))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_sort_entries_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-entries""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (condition-case nil (org-sort-entries nil ?a) (error :no-entries))))"##,
        expect,
    );
}
#[test]
fn combo87_error_org_export_file_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :include-error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox)
 (with-temp-buffer (org-mode) (insert "#+INCLUDE: \"/nonexistent-661/file.org\"\n")
 (condition-case nil (org-export-as 'ascii nil nil t) (error :include-error))))"##,
        expect,
    );
}
