use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo88_error_org_narrow_subtree_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-subtree""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (condition-case nil (org-narrow-to-subtree) (error :no-subtree))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_babel_tangle_no_tangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-tangle""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-tangle)
 (with-temp-buffer (org-mode) (condition-case nil (org-babel-tangle) (error :no-tangle))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_table_iterate_bad_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (condition-case e (org-table-iterate) (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_sparse_tree_bad_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (condition-case nil (org-match-sparse-tree nil "(unclosed")
  (error :bad-regex))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_export_with_broken_include() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox)
 (with-temp-buffer (org-mode) (insert "#+INCLUDE: \"\"\n") (condition-case e
  (org-export-as 'ascii nil nil t) (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_entry_properties_no_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:count 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (let ((props (org-entry-properties nil t))) (list :count (length props)))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_timestamp_change_no_ts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-timestamp""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (condition-case nil (org-timestamp-change 1 'day) (error :no-timestamp))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_set_effort_bad_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:e error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case e (org-set-effort nil "bad-effort") (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_entity_get_nonexistent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:bogus-1 nil :bogus-2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (list
 :bogus-1 (org-entity-get "this-entity-does-not-exist-661") :bogus-2 (org-entity-get "ZZZZZ")))"##,
        expect,
    );
}
#[test]
fn combo88_error_org_list_make_subtree_no_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-list""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (condition-case nil (org-list-make-subtree) (error :no-list))))"##,
        expect,
    );
}
