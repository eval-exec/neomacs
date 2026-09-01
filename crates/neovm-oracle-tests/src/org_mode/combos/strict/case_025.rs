//! combo_strict_25.rs — probing for actual behavioral divergences
//! by calling fboundp-checked functions with concrete args.
use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_call_org_timer_item_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-timer)
 (let ((r '())) (condition-case nil (progn (org-timer-item 1) (push (list :ok t) r)) (error (push :err r))) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_shiftcontrol_up_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after-move (\"A\" \"C\" \"B\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* A\n** B\n** C\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "** B") (beginning-of-line)
  (condition-case nil (org-shiftmetadown) (error nil))
  (push (list :after-move (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_toggle_archive_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:tags (\"ARCHIVE\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* Task\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil (org-toggle-archive-tag)
  (error nil)) (push (list :tags (org-get-tags)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_set_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:val \"value\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* H\n")
 (let ((r '())) (goto-char (point-min)) (org-set-property "KEY" "value")
  (push (list :val (org-entry-get nil "KEY")) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_todo_previous_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init \"TODO\") (:1 #(\"DONE\" 0 4 (org-todo-head \"TODO\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* TODO Task\n")
 (let ((r '())) (goto-char (point-min)) (push (list :init (org-get-todo-state)) r)
  (org-todo) (push (list :1 (org-get-todo-state)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_priority_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* Task\n")
 (let ((r '())) (goto-char (point-min)) (org-priority ?B)
  (push (list :priority-char (org-get-priority (point))) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_cycle_list_bullet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after \"+ item\\n  - sub\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "- item\n  - sub\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil (progn (org-cycle-list-bullet)
  (push (list :after (buffer-string)) r)) (error (push :err r))) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_delete_indentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after \"* A\\n** B\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* A\n** B\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil (org-delete-indentation) (error nil))
  (push (list :after (buffer-string)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_transpose_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after \"word2 word1\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "word1 word2\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil (transpose-words 1) (error nil))
  (push (list :after (buffer-string)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_call_org_align_tag_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:tags (\"tag1\" \"tag2\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* A :tag1:tag2:\n")
 (let ((r '())) (goto-char (point-min)) (push (list :tags (org-get-tags)) r) (nreverse r)))"##,
        expect,
    );
}
