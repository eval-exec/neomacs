use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo98_org_export_with_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* H\n:PROPERTIES:\n:EXPORT_FILE_NAME: test.txt\n:END:\nBody.\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t))) (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo98_org_block_property_drawer_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\nBody.\n") (goto-char (point-min))
 (let* ((t (org-element-parse-buffer)) (h (car (org-element-map t 'headline #'identity)))
  (pds (org-element-map h 'property-drawer #'identity)))
  (list :has-prop-drawer (> (length pds) 0) :contents-begin (org-element-property :contents-begin (car pds))
   :contents-end (org-element-property :contents-end (car pds)))))"##,
        expect,
    );
}
#[test]
fn combo98_org_todo_state_with_logging() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (:buffer #(\"* DONE Task\\nCLOSED: [2026-06-15 Mon 12:00]\\n\" 0 11 (org-todo-head \"TODO\"))))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((org-log-done 'time)) (insert "* TODO Task\n") (goto-char (point-min))
  (let ((r '())) (org-todo "DONE")
   (push (list :todo (org-get-todo-state)) r) (push (list :buffer (buffer-string)) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo98_org_entities_user_math() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 77)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (list
 :times (nth 6 (org-entity-get "times")) :div (nth 6 (org-entity-get "div"))
 :pm (nth 6 (org-entity-get "pm")) :infty (nth 6 (org-entity-get "infty")))))"##,
        expect,
    );
}
#[test]
fn combo98_org_babel_var_complex_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+begin_src emacs-lisp :results value :var data='((a . 1) (b . 2) (c . 3))\n")
  (insert "(cdr (assoc 'b data))\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo98_org_table_sum_count_average() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:sum \"\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a |\n|---|\n| 1 |\n| 2 |\n| 3 |\n| 4 |\n|   |\n")
 (insert "#+TBLFM: @>$1=vsum(@2..@-1)\n") (let ((r '())) (goto-char (point-min))
  (org-table-recalculate t) (org-table-align) (goto-char (point-min)) (forward-line 5)
  (push (list :sum (org-table-get nil nil)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo98_org_agenda_reminder_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:appt-fbound t :reminder-fbound nil :time-warn-fbound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :appt-fbound (fboundp 'org-agenda-to-appt) :reminder-fbound (boundp 'appt-message-warning-time)
 :time-warn-fbound (boundp 'org-agenda-to-appt-time-warning)))"##,
        expect,
    );
}
#[test]
fn combo98_org_list_to_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:items 4 :plain-lists 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "- item one\n- item two\n  - nested\n- item three\n") (goto-char (point-min))
 (list :items (length (org-element-map (org-element-parse-buffer) 'item #'identity))
  :plain-lists (length (org-element-map (org-element-parse-buffer) 'plain-list #'identity))))"##,
        expect,
    );
}
#[test]
fn combo98_org_move_past_drawer_and_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:at-body nil :head \"H\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\nSCHEDULED: <2024-01-01>\n:PROPERTIES:\n:A: 1\n:END:\nBody.\n")
 (goto-char (point-min)) (search-forward "Body.")
 (list :at-body (org-at-heading-p) :head (org-get-heading t t t t)))"##,
        expect,
    );
}
#[test]
fn combo98_org_sort_by_property_numeric() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* Z\n:PROPERTIES:\n:ORD: 3\n:END:\n* A\n:PROPERTIES:\n:ORD: 1\n:END:\n* M\n:PROPERTIES:\n:ORD: 2\n:END:\n")
 (goto-char (point-min)) (org-sort-entries nil ?r ?p "ORD" nil #'string<)
 (list :sorted (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
  (org-element-map (org-element-parse-buffer) 'headline #'identity))))"##,
        expect,
    );
}
