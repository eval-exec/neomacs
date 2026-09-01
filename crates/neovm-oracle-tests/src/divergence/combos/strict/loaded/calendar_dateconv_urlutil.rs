//! Strict combo oracle probes, batch 72: calendar date conversions (gregorian/
//! julian/islamic absolute-day math + day-of-week) and URL encoding (hexify/
//! unhex/encode-url).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_o6_calendar_gregorian_absolute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((11 2 2018) 739417 0 \"Sunday\" \"June\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (calendar-gregorian-from-absolute 737000)
      (calendar-absolute-from-gregorian '(6 15 2025))
      (calendar-day-of-week '(6 15 2025))
      (calendar-day-name '(6 15 2025))
      (calendar-month-name 6))
"##,
        &["calendar/calendar.el"],
        expect,
    );
}

#[test]
fn div_o6_calendar_other_calendars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function calendar-islamic-from-absolute)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((abs (calendar-absolute-from-gregorian '(6 15 2025))))
  (list (calendar-julian-from-absolute abs)
        (calendar-islamic-from-absolute abs)
        (calendar-hebrew-from-absolute abs)
        (calendar-chinese-from-absolute abs)
        (calendar-coptic-from-absolute abs)))
"##,
        &["calendar/calendar.el"],
        expect,
    );
}

#[test]
fn div_o6_url_hexify_unhex_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"hello%20world%20%26%20stuff\" \"hello\" \"caf%C3%A9\" \"http://host/path%20with%20spaces\" \"round trip 123\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (url-hexify-string "hello world & stuff")
      (url-unhex-string "%68%65%6c%6c%6f")
      (url-hexify-string "café")
      (url-encode-url "http://host/path with spaces")
      (url-unhex-string (url-hexify-string "round trip 123")))
"##,
        &["url/url-util.el"],
        expect,
    );
}
