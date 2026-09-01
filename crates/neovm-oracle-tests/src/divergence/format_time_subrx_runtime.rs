//! format-time-string deep (ISO week/year, 12h/%p, day names, zero-pad
//! defaults, %F/%T/%z combos, %- dash flag, %^ upcase, %s epoch) and subr-x
//! (thread-first/last, if/when-let, named-let, and-let*, string-*, hash keys/
//! values) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn fts_12h_pm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"02:32:48 PM\" \" 2:32pm\" \"2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%I:%M:%S %p" '(26150 29968) t) (format-time-string "%l:%M%P" '(26150 29968) t) (format-time-string "%-I" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_caret_upper_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"MONDAY APRIL\" \"PM\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%^A %^B" '(26150 29968) t) (format-time-string "%^p" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_combined_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"2024-04-22T14:32:48+0000\" \"Mon, 22 Apr 2024 14:32:48\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%FT%T%z" '(26150 29968) t) (format-time-string "%a, %d %b %Y %H:%M:%S" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_dash_flag_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"4/22/2024\" \"14:32\" \"113\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%-m/%-d/%Y" '(26150 29968) t) (format-time-string "%-H:%-M" '(26150 29968) t) (format-time-string "%-j" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_day_names_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Mon/Monday/Apr/April\" \"Wed/Wednesday\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%a/%A/%b/%B" '(26150 29968) t) (format-time-string "%a/%A" '(25700 30000) t))"##,
        expect,
    );
}

#[test]
fn fts_iso_week_year_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"2024-W17-1\" \"2024113\" \"24\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%G-W%V-%u" '(26150 29968) t) (format-time-string "%Y%j" '(26150 29968) t) (format-time-string "%g" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_seconds_since_epoch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"1713796368\" 1713796368)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%s" '(26150 29968) t) (string-to-number (format-time-string "%s" '(26150 29968) t)))"##,
        expect,
    );
}

#[test]
fn fts_zero_pad_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"143248\" \"04/22\" \"24\" \"20\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%H%M%S" '(26150 29968) t) (format-time-string "%m/%d" '(26150 29968) t) (format-time-string "%y" '(26150 29968) t) (format-time-string "%C" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn and_or_let_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (and-let* ((x 5) ((> x 3)) (y (* x 2))) y)
        (and-let* ((x 5) ((< x 3))) 'never))"##,
        expect,
    );
}

#[test]
fn hash_table_keys_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"a\" \"b\") (1 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'subr-x)
(let ((h (make-hash-table :test 'equal)))
  (puthash "a" 1 h) (puthash "b" 2 h)
  (list (sort (hash-table-keys h) #'string<) (sort (hash-table-values h) #'<)))"##,
        expect,
    );
}

#[test]
fn if_when_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 no 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'subr-x)
(list (if-let ((x 5) (y 10)) (+ x y) 'no)
      (if-let ((x nil)) 'yes 'no)
      (when-let ((a 1) (b 2)) (+ a b))
      (when-let ((a nil)) 'never))"##,
        expect,
    );
}

#[test]
fn named_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 120""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(named-let loop ((n 5) (acc 1)) (if (= n 0) acc (loop (1- n) (* acc n))))"##,
        expect,
    );
}

#[test]
fn string_subr_x() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hi\" t nil 0 \"a,b\" \"x  \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'subr-x)
(list (string-trim "  hi  ") (string-empty-p "") (string-empty-p "x")
      (string-blank-p "  ") (string-join '("a" "b") ",") (string-pad "x" 3))"##,
        expect,
    );
}

#[test]
fn thread_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (16 16 \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'subr-x)
(list (thread-first 5 (+ 3) (* 2)) (thread-last 5 (+ 3) (* 2))
      (thread-first '(1 2 3) (car) (number-to-string)))"##,
        expect,
    );
}
