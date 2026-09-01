//! combo_strict_27.rs — deeper error combos and edge calls
use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_error_org_id_get_no_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-id)
 (with-temp-buffer (org-mode) (condition-case e (org-id-get) (error (list :e (car e))))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_attachment_dir_no_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-attach)
 (with-temp-buffer (org-mode) (condition-case nil (org-attach-dir) (error :no-dir))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_property_inheritance_bad_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case nil (org-entry-get nil "MISSING" 'bad-flag) (error :bad-flag))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_babel_open_at_point_no_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-link""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (condition-case nil (org-open-at-point nil) (error :no-link))))"##,
        expect,
    );
}
#[test]
fn strict_error_org_set_property_illegal_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
 (condition-case nil (org-entry-put nil ":BAD:KEY" "val") (error :bad-property))))"##,
        expect,
    );
}
#[test]
fn strict_combined_table_recalc_then_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:export-ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "| a | b | c |\n| 1 | 2 |   |\n| 3 | 4 |   |\n")
  (insert "#+TBLFM: $3=$1+$2\n") (goto-char (point-min)) (org-table-recalculate t)
  (let ((r '())) (condition-case nil (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :export-ok (> (length out) 0)) r)) (error nil))
  (nreverse r))))"##,
        expect,
    );
}
#[test]
fn strict_combined_clock_then_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:export-ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock) (require 'ox-ascii)
 (let ((org-clock-persist nil) (org-export-show-temporary-export-buffer nil))
  (insert "* Task\n") (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (org-set-property "STATUS" "done") (insert "#+BEGIN: clocktable :maxlevel 2 :scope file\n#+END:\n")
  (goto-char (point-min)) (search-forward "#+BEGIN:") (beginning-of-line) (org-dblock-update)
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :export-ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn strict_combined_tag_todo_sort_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* TODO C :work:\n* DONE A :urgent:\n* TODO B :home:\n* DONE D :work:\n")
 (let ((r '())) (goto-char (point-min)) (org-sort-entries nil ?a)
  (push (list :sorted-alpha (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
   (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (goto-char (point-min)) (org-sort-entries nil ?o)
  (push (list :sorted-todo (mapcar (lambda (h) (list (substring-no-properties (org-element-property :raw-value h))
   (org-element-property :todo-keyword h))) (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_combined_fold_sparse_reveal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:overview-heads 4) (:after-heads 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** A1\nBody.\n** A2\nBody.\n* B\nBody.\n")
 (let ((r '())) (goto-char (point-min)) (org-overview)
  (push (list :overview-heads (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (org-match-sparse-tree nil "A1") (org-remove-occur-highlights)
  (org-show-all) (push (list :after-heads (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_combined_narrow_copy_paste_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-paste ((1 \"A\") (2 \"B\") (1 \"C\") (1 \"B\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nBody.\n* C\nBody.\n")
 (let ((r '())) (goto-char (point-min)) (org-narrow-to-subtree) (goto-char (point-min))
  (search-forward "** B") (beginning-of-line) (org-copy-subtree)
  (widen) (goto-char (point-max)) (org-paste-subtree 1)
  (push (list :after-paste (mapcar (lambda (h) (list (org-element-property :level h)
   (substring-no-properties (org-element-property :raw-value h))))
   (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
