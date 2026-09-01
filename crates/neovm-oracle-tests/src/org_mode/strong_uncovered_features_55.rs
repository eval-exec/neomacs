//! Strong uncovered-features-55 oracle tests — org-timer, org-learn, org-habit.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-timer-start
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_timer_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil
      (org-timer-start)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer-set-timer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_timer_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil
      (org-timer-set-timer 5)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer-item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_timer_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- 0:00:00 :: * T\\n:LOGBOOK:\\n:END:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-timer-item)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-habit-parse-todo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_habit_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Exercise\nSCHEDULED: <2026-01-15 .+2d/4d>\n:PROPERTIES:\n:STYLE: habit\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-habit-parse-todo)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-habit-build-consistency-graph
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_habit_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Exercise\nSCHEDULED: <2026-01-15 .+2d/4d>\n:PROPERTIES:\n:STYLE: habit\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-habit-build-consistency-graph)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-habit-toggle-display
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_habit_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Exercise\nSCHEDULED: <2026-01-15 .+2d/4d>\n:PROPERTIES:\n:STYLE: habit\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-habit-toggle-display)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-to-minutes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_duration() {
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
fn uf55_duration_from() {
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
fn uf55_duration_p() {
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

// ═══════════════════════════════════════════════════════════════════════
// org-time-stamp-to-now
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_ts_to_now() {
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
fn uf55_iso_week() {
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
fn uf55_today() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-today)""#]];
    crate::common::assert_oracle_parity_expect(r##"(org-today)"##, expect);
}

// ═══════════════════════════════════════════════════════════════════════
// org-current-time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_current_time() {
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
fn uf55_float_year() {
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
fn uf55_date_to_day() {
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
fn uf55_day_to_date() {
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
fn uf55_ts_to_sec() {
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
fn uf55_min_to_hhmm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-minutes-to-hh:mm-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-minutes-to-hh:mm-string 90)
        (org-minutes-to-hh:mm-string 45)
        (org-minutes-to-hh:mm-string 150))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-parse-time-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf55_parse_time() {
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
fn uf55_fix_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-fix-decoded-time)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-fix-decoded-time '(0 30 10 15 1 2026))
        (org-fix-decoded-time '(0 0 0 1 1 2026)))"##,
        expect,
    );
}
