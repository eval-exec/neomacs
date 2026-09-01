//! Strong uncovered-features-50 oracle tests — org-clock complex, org-archive, org-collect.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-clock-sum
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 150""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\n:END:\n* B\n:LOGBOOK:\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:30] =>  1:30\n:END:")
  (org-clock-sum))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-sum-current-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-sum-current-entry))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-clock-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-clock-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-clock-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-timestamps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-timestamps)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-timestamps))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-scheduled
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_scheduled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-scheduled)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15>\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-scheduled))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-deadline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_deadline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-deadline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nDEADLINE: <2026-01-20>\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-deadline))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-effort
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_effort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-effort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:EFFORT: 2h\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-effort))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-state)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-state))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-category
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-category)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CATEGORY: default\n* T\n:PROPERTIES:\n:CATEGORY: custom\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-category))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-get-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_clock_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-get-heading)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Heading :tag:\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-get-heading))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-collect-keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"Test\") (\"AUTHOR\" \"Me\") (\"DATE\" \"2026-01-15\") (\"OPTIONS\" \"toc:nil\") (\"FILETAGS\" \":t1:t2:\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Me\n#+DATE: 2026-01-15\n#+OPTIONS: toc:nil\n#+FILETAGS: :t1:t2:")
  (org-collect-keywords '("TITLE" "AUTHOR" "DATE" "OPTIONS" "FILETAGS")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-collect-keywords with multiple values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_collect_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"TITLE\" \"T1\" \"T2\") (\"AUTHOR\" \"A\" \"B\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T1\n#+TITLE: T2\n#+AUTHOR: A\n#+AUTHOR: B")
  (org-collect-keywords '("TITLE" "AUTHOR")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-collect-keywords with categories
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_collect_cat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"CATEGORY\" \"default\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CATEGORY: default\n* H1\n:PROPERTIES:\n:CATEGORY: custom\n:END:")
  (org-collect-keywords '("CATEGORY")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-archive-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_archive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H1\\n** TODO T1\\n* H2\\n** TODO T2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** TODO T1\n* H2\n** TODO T2")
  (goto-char (point-min))
  (search-forward "T1")
  (beginning-of-line)
  (condition-case nil
      (org-archive-subtree)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-archive-to-archive-sibling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_archive_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* H1\\n** Archive                                                          :ARCHIVE:\\n*** TODO T1\\n:PROPERTIES:\\n:ARCHIVE_TIME: 2026-06-15 Mon 12:00\\n:END:\\n* Archive :archive:\\n* H2\\n** TODO T2\"""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** TODO T1\n* Archive :archive:\n* H2\n** TODO T2")
  (goto-char (point-min))
  (search-forward "T1")
  (beginning-of-line)
  (condition-case nil
      (org-archive-to-archive-sibling)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-archive-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_archive_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"ARCHIVE\") \"* T                                                                 :ARCHIVE:\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-toggle-archive-tag)
  (list (org-get-tags) (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-archive-set-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_archive_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-archive-set-tag)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-archive-set-tag)
  (list (org-get-tags) (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-to-minutes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_duration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-duration-to-minutes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-duration-to-minutes "1:30")
        (org-duration-to-minutes "2h30min")
        (org-duration-to-minutes "1d 2h")
        (org-duration-to-minutes "90min"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-from-minutes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_duration_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-duration-from-minutes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-duration-from-minutes 90)
        (org-duration-from-minutes 150)
        (org-duration-from-minutes 1500))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf50_duration_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-duration-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-duration-p "1:30")
        (org-duration-p "2h30min")
        (org-duration-p "invalid")
        (org-duration-p "90min"))"##,
        expect,
    );
}
