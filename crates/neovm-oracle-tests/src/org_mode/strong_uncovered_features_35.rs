//! Strong uncovered-features-35 oracle tests — org-clock, org-archive, org-feed.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-clock-sum
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_clock_sum() {
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
fn uf35_clock_current() {
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
fn uf35_clock_string() {
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
fn uf35_clock_timestamps() {
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
fn uf35_clock_scheduled() {
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
fn uf35_clock_deadline() {
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
fn uf35_clock_effort() {
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
fn uf35_clock_state() {
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
fn uf35_clock_category() {
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
fn uf35_clock_heading() {
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
// org-archive-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_archive() {
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
fn uf35_archive_sibling() {
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
fn uf35_archive_tag() {
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
fn uf35_archive_set_tag() {
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
// org-feed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-update
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-update "test-feed")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-update-all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_update_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-update-all)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-parse-atom-feed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_atom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-parse-atom-feed "<feed><entry><title>T</title></entry></feed>")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-parse-rss-feed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_rss() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-parse-rss-feed "<rss><channel><item><title>T</title></item></channel></rss>")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-parse-feed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-parse-feed "<rss><channel><item><title>T</title></item></channel></rss>")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-get-entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-get-entries '(:url "http://example.com/feed.xml"))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-add-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_add() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* Feed\\n** Entries\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Feed\n** Entries")
  (condition-case nil
      (org-feed-add-entry '(:url "http://example.com/feed.xml") '((:title . "T") (:link . "http://example.com/1")))
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-format-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-format-entry '((:title . "T") (:link . "http://example.com/1") (:description . "Desc")))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-read-elfeed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_elfeed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-read-elfeed)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-read-rss
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_read_rss() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-read-rss "http://example.com/feed.xml")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-feed-read-atom
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf35_feed_read_atom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-feed-read-atom "http://example.com/feed.xml")
  (error nil))"##,
        expect,
    );
}
