//! Complex combo batch 144 — `org-capture` / `org-clock` / `org-agenda`
//! real-world flows with markers, deadlines, and scheduling.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx144_org_agenda_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org-agenda)
      (list (fboundp 'org-agenda)
            (fboundp 'org-agenda-list)
            (boundp 'org-agenda-files)
            (boundp 'org-agenda-buffer-name)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_clock_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (list (fboundp 'org-clock-in)
            (fboundp 'org-clock-out)
            (fboundp 'org-clock-cancel)
            (boundp 'org-clock-history)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_capture_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org-capture)
      (list (fboundp 'org-capture)
            (boundp 'org-capture-templates)
            (boundp 'org-capture-bookmark)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_deadline_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"<2024-12-31 Mon>\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task with deadline\nDEADLINE: <2024-12-31 Mon>\n")
        (org-back-to-heading t)
        (list (org-entry-get (point) "DEADLINE"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_schedule_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"<2024-06-15 Sat>\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Scheduled task\nSCHEDULED: <2024-06-15 Sat>\n")
        (org-back-to-heading t)
        (list (org-entry-get (point) "SCHEDULED"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_tag_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"alpha\" \"beta\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task :alpha:beta:\n")
        (org-back-to-heading t)
        (list (org-get-tags))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_priority_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* [#A] Important task\n")
        (org-back-to-heading t)
        (list (org-entry-get (point) "PRIORITY"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_property_drawer_full_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0:30\" nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task\n:PROPERTIES:\n:Effort: 0:30\n:Priority: A\n:Tags: foo,bar\n:END:\n")
        (org-back-to-heading t)
        (list (org-entry-get (point) "Effort")
              (org-entry-get (point) "Priority")
              (org-entry-get (point) "Tags")
              (org-entry-get (point) "Missing"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_todo_state_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TODO\" 0 \"DONE\" 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* TODO Task one\n* DONE Task two\n* NEXT Task three\n")
        (goto-char 1)
        (list (org-get-todo-state)
              (forward-line 1) (org-get-todo-state)
              (forward-line 1) (org-get-todo-state))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_table_alignment_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t \"| Name | Age |\\n|------+-----|\\n| Bob  | 30  |\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "| Name | Age |\n|------+-----|\n| Bob  | 30  |\n")
        (goto-char 1)
        (let ((in-table (org-at-table-p)))
          (list in-table (buffer-string)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_sparse_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t \"* Task A\\n** Sub A1\\ncontent alpha\\n** Sub A2\\ncontent beta\\n* Task B\\ncontent gamma\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task A\n** Sub A1\ncontent alpha\n** Sub A2\ncontent beta\n* Task B\ncontent gamma\n")
        (goto-char 1)
        (condition-case err
            (org-occur "alpha")
          (error :err))
        (list (eq major-mode 'org-mode)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx144_org_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (buffer-enable-undo)
        (org-mode)
        (insert "* Heading one\nbody content\n* Heading two\nmore content\n")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 12))
              (ov (make-overlay 4 22)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 35)
          (let ((state (list (eq major-mode 'org-mode)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
