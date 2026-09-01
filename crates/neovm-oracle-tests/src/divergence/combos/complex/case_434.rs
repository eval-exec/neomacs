//! Complex combo batch 434 — 16 deeper org-mode probes: org-table
//! deeper, org-archive, org-map-entries, org-sort, org-attach,
//! org-properties, org-drawers, org-date-range, org-effort,
//! org-repeat, org-columns, org-id, org-habit, org-inlinetask,
//! org-entities, org-feed.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// org-table deeper: formulas and field calculations.
#[test]
fn div_cx434_org_table_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| 1 | 2 |\n| 3 | 4 |\n|   |   |\n#+TBLFM: @3$1=@1$1+@2$1::@3$2=@1$2+@2$2\n")
    (org-table-iterate)
    (buffer-string)))
"##,
        expect,
    );
}

/// org-archive: heading archiving.
#[test]
fn div_cx434_org_archive_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-archive)
  (with-temp-buffer
    (org-mode)
    (insert "* Test archivable heading\n")
    (list (fboundp 'org-archive-subtree)
          (fboundp 'org-archive-to-archive-sibling))))
"##,
        expect,
    );
}

/// org-map-entries: mapping over headings.
#[test]
fn div_cx434_org_map_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* H1\n** H2\n*** H3\n")
    (org-map-entries (lambda () (insert "!")))))
"##,
        expect,
    );
}

/// org-sort: sorting entries in org-mode.
#[test]
fn div_cx434_org_sort_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* b\n* a\n* c\n")
    (org-sort-entries t ?a)
    (buffer-string)))
"##,
        expect,
    );
}

/// org-properties: property drawer operations.
#[test]
fn div_cx434_org_properties_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"test\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* heading\n:PROPERTIES:\n:CUSTOM_ID: test\n:END:\n")
    (org-back-to-heading)
    (org-entry-get nil "CUSTOM_ID")))
"##,
        expect,
    );
}

/// org-drawers: drawer operations.
#[test]
fn div_cx434_org_drawers_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* H\n:MYDRAWER:\ncontent\n:END:\n")
    (org-back-to-heading)
    (org-flag-drawer (point-min) (point-max) t)
    (outline-invisible-p 20)))
"##,
        expect,
    );
}

/// org-date-range: date range operations.
#[test]
fn div_cx434_org_date_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (list (fboundp 'org-time-string-to-seconds)
        (fboundp 'org-days-to-time)))
"##,
        expect,
    );
}

/// org-effort: effort estimate operations.
#[test]
fn div_cx434_org_effort_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"2:00\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* heading\n:PROPERTIES:\n:Effort: 2:00\n:END:\n")
    (org-back-to-heading)
    (org-entry-get nil "Effort")))
"##,
        expect,
    );
}

/// org-repeat: repeater cookies.
#[test]
fn div_cx434_org_repeat_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO task SCHEDULED: <2024-01-01 +1w>\n")
    (org-back-to-heading)
    (org-get-scheduled-time (point) "\\+")))
"##,
        expect,
    );
}

/// org-columns: column view operations.
#[test]
fn div_cx434_org_columns_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "* heading\n")
    (list (fboundp 'org-columns)
          (fboundp 'org-columns-edit-value))))
"##,
        expect,
    );
}

/// org-id: ID property operations.
#[test]
fn div_cx434_org_id_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-id)
  (with-temp-buffer
    (org-mode)
    (insert "* heading\n")
    (org-id-get-create)
    (org-id-get)))
"##,
        expect,
    );
}

/// org-habit: habit tracking.
#[test]
fn div_cx434_org_habit_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-habit)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO daily habit\nSCHEDULED: <2024-06-10 .+1d>\n:PROPERTIES:\n:STYLE: habit\n:END:\n")
    (list (fboundp 'org-habit-parse-todo)
          (fboundp 'org-habit-build-graph))))
"##,
        expect,
    );
}

/// org-inlinetask: inline task operations.
#[test]
fn div_cx434_org_inlinetask_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-inlinetask)
  (with-temp-buffer
    (org-mode)
    (let ((org-inlinetask-min-level 15))
      (insert "*************** task\n*************** END\n")
      (list (fboundp 'org-inlinetask-insert-task)
            (org-inlinetask-at-task-p))))
"##,
        expect,
    );
}

/// org-entities: entity normalization.
#[test]
fn div_cx434_org_entities_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-entities-get)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities)
  (list (org-entities-get "agrave")
        (org-entities-get "copy")
        (fboundp 'org-entities-display)))
"##,
        expect,
    );
}

/// org-attach: attachment operations.
#[test]
fn div_cx434_org_attach_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-attach)
  (list (fboundp 'org-attach-attach)
        (fboundp 'org-attach-reveal)
        (boundp 'org-attach-id-dir)))
"##,
        expect,
    );
}

/// org-timer: org-mode timer operations.
#[test]
fn div_cx434_org_timer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-timer)
  (list (fboundp 'org-timer-set-timer)
        (fboundp 'org-timer-pause-or-continue)))
"##,
        expect,
    );
}
