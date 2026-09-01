//! Calendar/solar (absolute<->gregorian, day-of-week, leap year, last-day,
//! iso/julian, sunrise-sunset) and keymap (define-key/lookup-key, parent
//! inheritance, where-is-internal, prefix keymaps) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn cal_absolute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (730120 (1 1 2000) 6 61)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calendar)
(list (calendar-absolute-from-gregorian '(1 1 2000))
      (calendar-gregorian-from-absolute 730120)
      (calendar-day-of-week '(6 15 2024))
      (calendar-day-number '(3 1 2024)))"##,
        expect,
    );
}

#[test]
fn cal_dayname_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 (1 1 2024) 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calendar)
(list (calendar-day-of-week '(1 1 2024))
      (calendar-nth-named-day 1 1 1 2024)
      (calendar-week-end-day))"##,
        expect,
    );
}

#[test]
fn cal_iso_julian() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((17 3 2024) (4 11 2024))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cal-iso)
(require 'cal-julian)
(list (calendar-iso-from-absolute 739000)
      (calendar-julian-from-absolute 739000))"##,
        expect,
    );
}

#[test]
fn cal_leap_dst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil 29 28 \"Saturday\" \"June\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calendar)
(list (calendar-leap-year-p 2024) (calendar-leap-year-p 2100)
      (calendar-last-day-of-month 2 2024) (calendar-last-day-of-month 2 2023)
      (calendar-day-name '(6 15 2024)) (calendar-month-name 6))"##,
        expect,
    );
}

#[test]
fn solar_sunrise() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'solar)
(let ((calendar-latitude 40.0) (calendar-longitude -74.0) (calendar-time-zone -300)
      (calendar-standard-time-zone-name "EST") (calendar-daylight-time-zone-name "EDT"))
  (let ((s (solar-sunrise-sunset '(6 15 2024))))
    (list (numberp (caar s)) (numberp (caadr s)))))"##,
        expect,
    );
}

#[test]
fn keymap_define_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cmd-a cmd-b nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((m (make-sparse-keymap)))
  (define-key m (kbd "C-c a") 'cmd-a)
  (define-key m (kbd "C-c C-b") 'cmd-b)
  (list (lookup-key m (kbd "C-c a")) (lookup-key m (kbd "C-c C-b"))
        (lookup-key m (kbd "C-c z")) (keymapp m)))"##,
        expect,
    );
}

#[test]
fn keymap_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (parent-a child-b t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((parent (make-sparse-keymap)) (child (make-sparse-keymap)))
  (define-key parent (kbd "a") 'parent-a)
  (set-keymap-parent child parent)
  (define-key child (kbd "b") 'child-b)
  (list (lookup-key child (kbd "a")) (lookup-key child (kbd "b"))
        (eq (keymap-parent child) parent)))"##,
        expect,
    );
}

#[test]
fn keymap_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t cx)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((m (make-keymap)))
  (define-key m (kbd "C-c") (make-sparse-keymap))
  (define-key m (kbd "C-c x") 'cx)
  (list (keymapp (lookup-key m (kbd "C-c"))) (lookup-key m (kbd "C-c x"))))"##,
        expect,
    );
}

#[test]
fn keymap_where_is() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"x\" \"y\") [121])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((m (make-sparse-keymap)))
  (define-key m [?x] 'foo) (define-key m [?y] 'foo)
  (list (sort (mapcar #'key-description (where-is-internal 'foo m)) #'string<)
        (where-is-internal 'foo m t)))"##,
        expect,
    );
}
