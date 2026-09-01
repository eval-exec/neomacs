//! Alternative-calendar conversion parity (pure-Elisp algorithms over the core
//! engine): Islamic, Hebrew, Chinese, French-Revolutionary, Persian, Coptic/
//! Ethiopic, Bahai, Mayan long-count, astronomical Julian day, and ISO
//! from/to absolute date.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn astro_julian_day() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2460476.5 2460476)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-julian)
  (list (calendar-astro-from-absolute 739052)
        (floor (calendar-astro-from-absolute 739052)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn bahai() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((5 12 181))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-bahai)
  (list (calendar-bahai-from-absolute 739052))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn chinese() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((78 41 5 10))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-china)
  (list (calendar-chinese-from-absolute 739052))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn coptic_ethiopic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((10 8 1740) (10 8 2016))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-coptic)
  (list (calendar-coptic-from-absolute 739052)
        (calendar-ethiopic-from-absolute 739052))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn french() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((9 28 232) 739151)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-french)
  (list (calendar-french-from-absolute 739052)
        (calendar-french-to-absolute '(1 1 233)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn hebrew() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((3 9 5784) 739162)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-hebrew)
  (list (calendar-hebrew-from-absolute 739052)
        (calendar-hebrew-to-absolute '(7 1 5785)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn islamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((12 8 1445) 739075)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-islam)
  (list (calendar-islamic-from-absolute 739052)
        (calendar-islamic-to-absolute '(1 1 1446)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn iso_calendar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((24 6 2024) 739059)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-iso)
  (list (calendar-iso-from-absolute 739052)
        (calendar-iso-to-absolute '(25 6 2024)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn mayan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((13 0 11 11 14))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-mayan)
  (list (calendar-mayan-long-count-from-absolute 739052))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn persian() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((3 26 1403) 738965)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'cal-persia)
  (list (calendar-persian-from-absolute 739052)
        (calendar-persian-to-absolute '(1 1 1403)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}
