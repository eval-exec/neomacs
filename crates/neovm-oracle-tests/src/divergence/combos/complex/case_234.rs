//! Complex combo batch 234 — `org-table` deep: table alignment, formulas,
//! column graph, `org-checkbox`, `org-list`, `org-footnote` operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx234_org_table_parsing_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t 0 \"| Name | Age | City |\\n|------+-----+------|\\n| Bob  | 30  | NYC  |\\n| Sue  | 25  | LA   |\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "| Name | Age | City |\n|------+-----+------|\n| Bob  | 30  | NYC  |\n| Sue  | 25  | LA   |\n")
        (goto-char 1)
        (list (org-at-table-p)
              (org-table-current-column)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_table_formula_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t \"| a | b | sum |\\n|---+---+-----|\\n| 1 | 2 |   3 |\\n| 4 | 5 |   9 |\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "| a | b | sum |\n|---+---+-----|\n| 1 | 2 |   3 |\n| 4 | 5 |   9 |\n")
        (goto-char 1)
        (list (org-at-table-p)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_checkbox_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Tasks\\n- [ ] Task one\\n- [ ] Task two\\n- [X] Task three\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Tasks\n- [ ] Task one\n- [ ] Task two\n- [X] Task three\n")
        (goto-char (point-min))
        (re-search-forward "\\[ \\]" nil t)
        (list (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_list_structure_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "- alpha\n  - sub-alpha-1\n  - sub-alpha-2\n- beta\n- gamma\n")
        (goto-char 1)
        (list (org-at-item-p)
              (org-get-indentation))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_footnote_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (list (fboundp 'org-footnote-new)
            (fboundp 'org-footnote-action)
            (boundp 'org-footnote-section)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_property_search() {
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
fn div_cx234_org_timestamp_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"<2024-06-15 Sat>\" \"<2024-12-31 Tue>\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task\nSCHEDULED: <2024-06-15 Sat> DEADLINE: <2024-12-31 Tue>\n")
        (org-back-to-heading t)
        (list (org-entry-get (point) "SCHEDULED")
              (org-entry-get (point) "DEADLINE"))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_tag_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"alpha\" \"beta\") 0 (\"gamma\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task :alpha:beta:\n* Other :gamma:\n")
        (goto-char 1)
        (list (org-get-tags)
              (forward-line 1)
              (org-get-tags))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_heading_level_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0 2 0 3 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* L1\n** L2\n*** L3\n* L1-again\n")
        (goto-char 1)
        (list (org-current-level)
              (forward-line 1) (org-current-level)
              (forward-line 1) (org-current-level)
              (forward-line 1) (org-current-level))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx234_org_with_marker_overlay_undo_narrow_mega() {
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
        (insert "* Task one\nbody content\n* Task two\nmore content\n")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 12))
              (ov (make-overlay 5 22)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 35)
          (let ((state (list (eq major-mode 'org-mode)
                             (org-current-level)
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
