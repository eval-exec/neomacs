/// Batch 461: looking-back, skip-chars, scan-lists, time-date deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx461_looking_back_at() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (goto-char 6)
  (list (looking-back "hello" 1)
        (looking-at " world")
        (looking-at "hello")))"##,
        expect,
    );
}

#[test]
fn div_cx461_skip_chars_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 7 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc123xyz")
  (goto-char 1)
  (skip-chars-forward "a-z")
  (list (point)
        (progn (skip-chars-forward "0-9") (point))
        (progn (skip-chars-backward "0-9") (point))
        (progn (skip-chars-forward "a-z") (point))))"##,
        expect,
    );
}

#[test]
fn div_cx461_scan_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (scan-error \"Unbalanced parentheses\" 1 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(a (b c) d)")
  (list (scan-lists 1 1 0)
        (scan-lists 1 -1 0)
        (scan-lists 1 1 1)))"##,
        expect,
    );
}

#[test]
fn div_cx461_scan_sexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(a (b) (c))")
  (list (scan-sexps 1 1) (scan-sexps 1 -1)))"##,
        expect,
    );
}

#[test]
fn div_cx461_time_to_days() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 738886)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil)))
  (list (time-to-day-in-year t1)
        (time-to-days t1)))"##,
        expect,
    );
}

#[test]
fn div_cx461_date_to_day_to_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (739053 (26222 25408))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (date-to-day "(2024-06-16)")
      (date-to-time "2024-06-16"))"##,
        expect,
    );
}

#[test]
fn div_cx461_float_time_nil_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1704085200.0 error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil)))
  (list (float-time t1)
        (condition-case e (float-time t) (error (car e)))))"##,
        expect,
    );
}

#[test]
fn div_cx461_time_seconds_to_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (86400.0 3600.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (time-to-seconds (seconds-to-time 86400))
      (time-to-seconds (seconds-to-time 3600)))"##,
        expect,
    );
}

#[test]
fn div_cx461_date_leap_year() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calendar)
  (list (calendar-leap-year-p 2024)
        (calendar-leap-year-p 2023)
        (calendar-leap-year-p 2000)
        (calendar-leap-year-p 2100))"##,
        expect,
    );
}

#[test]
fn div_cx461_days_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calendar)
  (calendar-date-is-valid-p '(1 1 2024))"##,
        expect,
    );
}

#[test]
fn div_cx461_safe_region_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "hello"))
  (put-text-property 1 4 'face 'bold s)
  (list (text-properties-at 0 s)
        (next-property-change 0 s)
        (previous-property-change 4 s)))"##,
        expect,
    );
}

#[test]
fn div_cx461_syntax_propertize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#'foo #'bar\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (insert "#'foo #'bar")
  (syntax-propertize (point-max))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx461_looking_at_non_greedy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"aab\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "aabb")
  (goto-char 1)
  (looking-at "a*?b")
  (match-string 0))"##,
        expect,
    );
}

#[test]
fn div_cx461_skip_syntax_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(a b c) d (e f)")
  (goto-char 1)
  (skip-syntax-forward " ")
  (list (point)
        (progn (skip-syntax-forward "w(") (point))))"##,
        expect,
    );
}

#[test]
fn div_cx461_capitalize_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Hello World Foo Bar\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world foo bar")
  (capitalize-region (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}
