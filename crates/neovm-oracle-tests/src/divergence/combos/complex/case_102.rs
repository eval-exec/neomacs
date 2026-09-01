//! Complex combo batch 102 — org / org-roam / org-babel / org-agenda /
//! markdown / tex / rst availability and basic parsing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx102_org_availability_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (list (fboundp 'org-mode)
            (fboundp 'org-agenda)
            (boundp 'org-directory)
            (boundp 'org-agenda-files)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_org_basic_buffer_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 15 \"Sub heading\" 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Top heading\n** Sub heading\nSome text content.\n")
        (list (eq major-mode 'org-mode)
              (org-back-to-heading t)
              (org-get-heading)
              (point))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_org_property_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0:30\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task\n:PROPERTIES:\n:Effort: 0:30\n:Priority: A\n:END:\n")
        (org-back-to-heading t)
        (list (org-entry-get (point) "Effort")
              (org-entry-get (point) "Priority")
              (org-entry-get (point) "Missing"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_org_babel_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ob)
      (list (fboundp 'org-babel-execute-src-block)
            (fboundp 'org-babel-get-src-block-info)
            (boundp 'org-babel-load-languages)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_org_capture_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org-capture)
      (list (fboundp 'org-capture)
            (boundp 'org-capture-templates)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_markdown_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'markdown-mode)
      (list (fboundp 'markdown-mode)
            (boundp 'markdown-command)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_tex_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tex-mode)
      (list (fboundp 'tex-mode)
            (fboundp 'latex-mode)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_rst_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'rst)
      (list (fboundp 'rst-mode)
            (fboundp 'rst-compile)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_org_agenda_buffer_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org-agenda)
      (list (fboundp 'org-agenda-list)
            (fboundp 'org-todo-list)
            (boundp 'org-agenda-buffer-name)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_org_table_basic_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t 1 \"| Name | Age |\\n|------+-----|\\n| Bob  | 30  |\\n\")""#
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
        (re-search-forward "^|" nil t)
        (list (org-at-table-p)
              (org-table-current-column)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx102_org_clock_availability() {
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
fn div_cx102_org_with_marker_overlay_undo_narrow_mega() {
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
        (insert "* Task 1\nContent 1\n* Task 2\nContent 2\n")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 12))
              (ov (make-overlay 5 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 22)
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

#[test]
fn div_cx102_org_typed_lists_and_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TODO\" \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* TODO [#A] Important task\n* DONE [#C] Done task\n")
        (goto-char 1)
        (list (org-get-todo-state)
              (org-entry-get (point) "PRIORITY"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
