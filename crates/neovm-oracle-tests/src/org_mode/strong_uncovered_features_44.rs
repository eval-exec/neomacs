//! Strong uncovered-features-44 oracle tests — org-timestamp, org-planning, org-schedule.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-from-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-timestamp-from-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ts (org-timestamp-from-string "<2026-01-15 Wed 10:30>")))
  (list (org-element-property :year-start ts)
        (org-element-property :month-start ts)
        (org-element-property :day-start ts)
        (org-element-property :hour-start ts)
        (org-element-property :minute-start ts)
        (org-element-property :dayofweek ts)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-timestamp-from-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ts (org-timestamp-from-string "<2026-01-15 Wed 10:30>")))
  (org-timestamp-format ts "%Y-%m-%d %H:%M"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-to-time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_to_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-timestamp-to-time)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t (org-timestamp-to-time (org-timestamp-from-string "<2026-01-15 Wed>"))))
  (list (nth 0 t) (nth 1 t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-up/down-day
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_ud() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed>")
  (goto-char (point-min))
  (search-forward "<2026")
  (backward-char 2)
  (let ((d1 (org-element-property :day-start (org-element-context))))
    (org-timestamp-up-day)
    (let ((d2 (org-element-property :day-start (org-element-context))))
      (org-timestamp-down-day)
      (let ((d3 (org-element-property :day-start (org-element-context))))
        (list d1 d2 d3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-schedule
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_schedule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\nSCHEDULED: <2026-01-15 Thu>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-schedule nil "<2026-01-15>")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-deadline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_deadline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\nDEADLINE: <2026-01-20 Tue>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-deadline nil "<2026-01-20>")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-time-stamp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-max))
  (org-time-stamp nil)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-time-stamp-inactive
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-max))
  (org-time-stamp-inactive nil)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-repeat
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" \"+1m\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 +1w>\n* TODO M\nDEADLINE: <2026-01-20 +1m>\n* TODO N")
  (goto-char (point-min))
  (let ((r1 (org-get-repeat)))
    (forward-line 2)
    (let ((r2 (org-get-repeat)))
      (forward-line 2)
      (list r1 r2 (org-get-repeat)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-scheduled-time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_sched_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed>")
  (goto-char (point-min))
  (let ((t (org-get-scheduled-time nil)))
    (list (nth 0 t) (nth 1 t))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-deadline-time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_dead_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nDEADLINE: <2026-01-20 Mon>")
  (goto-char (point-min))
  (let ((t (org-get-deadline-time nil)))
    (list (nth 0 t) (nth 1 t))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-parse-time-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_parse_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-parse-time-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-parse-time-string "<2026-01-15 Wed 10:30>")
        (org-parse-time-string "[2026-01-20 Mon]")
        (org-parse-time-string "<2026-01-25>"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-fix-decoded-time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_fix_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-fix-decoded-time)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-fix-decoded-time '(0 30 10 15 1 2026))
        (org-fix-decoded-time '(0 0 0 1 1 2026)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-time-stamp-to-now
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_to_now() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-time-stamp-to-now)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-time-stamp-to-now "<2026-01-15>")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-days-to-iso-week
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_iso_week() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-days-to-iso-week)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-days-to-iso-week 0)
        (org-days-to-iso-week 1)
        (org-days-to-iso-week 7))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-today
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_today() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-today)""#]];
    crate::common::assert_oracle_parity_expect(r##"(org-today)"##, expect);
}

// ═══════════════════════════════════════════════════════════════════════
// org-current-time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_current_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-current-time)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t (org-current-time)))
  (list (nth 0 t) (nth 1 t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-float-year
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_float_year() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-float-year)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-float-year 2026)
        (org-float-year 2000)
        (org-float-year 1900))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-date-to-day
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_date_to_day() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-date-to-day)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-date-to-day "2026-01-15")
        (org-date-to-day "2026-06-01")
        (org-date-to-day "2026-12-31"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-day-to-date
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_day_to_date() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-day-to-date)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-day-to-date (org-date-to-day "2026-01-15"))
        (org-day-to-date (org-date-to-day "2026-06-01")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-time-string-to-seconds
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_ts_to_sec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-time-string-to-seconds)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-time-string-to-seconds "1:30")
        (org-time-string-to-seconds "0:45")
        (org-time-string-to-seconds "2:15:30"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-minutes-to-hh:mm-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf44_min_to_hhmm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-minutes-to-hh:mm-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-minutes-to-hh:mm-string 90)
        (org-minutes-to-hh:mm-string 45)
        (org-minutes-to-hh:mm-string 150))"##,
        expect,
    );
}
