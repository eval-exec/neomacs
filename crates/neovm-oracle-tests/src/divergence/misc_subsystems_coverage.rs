//! Cross-subsystem coverage — subsystems confirmed faithful (0 divergences).
//!
//! Consolidates probes across many subsystems that turned out to match GNU
//! exactly: threads/mutex, weak hash tables, subr-x, thread-first/last, pcase,
//! seq/map, object printed forms, format edge cases, print-circle, time,
//! abbrev, thing-at-point. Kept as regression/coverage.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cov_threads_mutex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (featurep 'threads)
      (threadp (make-thread (lambda () (sleep-for 0.01))))
      (mutexp (make-mutex))
      (condition-variable-p (make-condition-variable (make-mutex))))
"##,
        expect,
    );
}

#[test]
fn div_cov_weak_hash_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t key)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (hash-table-p (make-hash-table :weakness 'key))
      (hash-table-p (make-hash-table :weakness 'value))
      (hash-table-p (make-hash-table :weakness 'key-and-value))
      (hash-table-weakness (make-hash-table :weakness 'key)))
"##,
        expect,
    );
}

#[test]
fn div_cov_subr_x_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"hi\" \"hi\" \"hi\" \"hi   \" \"   hi\" \"cba\" \"ab\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-trim "  hi  ")
      (string-trim-left "  hi")
      (string-trim-right "hi  ")
      (string-pad "hi" 5)
      (string-pad "hi" 5 32 t)
      (string-reverse "abc")
      (string-chop-newline "ab\n"))
"##,
        expect,
    );
}

#[test]
fn div_cov_thread_first_last_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 7 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'subr-x)
  (list (fboundp 'thread-first) (fboundp 'thread-last)
        (thread-first 5 (1+) (1+))
        (thread-last 5 (- 1) (- 1))))
"##,
        expect,
    );
}

#[test]
fn div_cov_pcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (big other str)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (pcase 5 ((or 1 2 3) 'small) ((pred (>= 5)) 'big) (_ 'other))
      (pcase 10 ((or 1 2 3) 'small) ((pred (>= 5)) 'big) (_ 'other))
      (pcase "x" ((pred stringp) 'str) (_ 'other)))
"##,
        expect,
    );
}

#[test]
fn div_cov_seq_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 1 (1 2 3) ((nil 2 4) (t 1 3 5)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'seq)
  (list (seq-max '(3 1 4 1 5))
        (seq-min '(3 1 4))
        (seq-uniq '(1 1 2 2 3))
        (seq-group-by #'oddp '(1 2 3 4 5))))
"##,
        expect,
    );
}

#[test]
fn div_cov_object_print_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK (\"#s(hash-table data (a 1))\" 0 \"#<subr +>\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string (let ((h (make-hash-table))) (puthash 'a 1 h) h))
      (string-match-p "#&8" (prin1-to-string (make-bool-vector 8 nil)))
      (prin1-to-string (symbol-function '+)))
"##,
        expect,
    );
}

#[test]
fn div_cov_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#1=(1 . #1#)\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x (list 1)) (print-circle t))
  (setcdr x x)
  (prin1-to-string x))
"##,
        expect,
    );
}

#[test]
fn div_cov_format_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1010\" \"0100\" \"0XFF\" \"4\" \"+3.14\" \"00042\" \"3.14    |\" \"0.0001\" \"0.3333333333\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%b" 10)
      (format "%#o" 64)
      (format "%#X" 255)
      (format "%.0f" 3.7)
      (format "%+.2f" 3.14159)
      (format "%05d" 42)
      (format "%-8.2f|" 3.14)
      (format "%g" 0.0001)
      (format "%.10g" (/ 1.0 3)))
"##,
        expect,
    );
}

#[test]
fn div_cov_time_and_abbrev_and_thingatpt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"2024-01-15 12:59:30 015 02 1 Monday\" \"expanded \" \"https://x.example\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-time-string "%Y-%m-%d %H:%M:%S %j %U %w %A"
                          (encode-time 30 59 12 15 1 2024 nil))
      (with-temp-buffer
        (abbrev-mode 1)
        (define-abbrev global-abbrev-table "neoabbr2" "expanded")
        (insert "neoabbr2 ")
        (expand-abbrev)
        (buffer-string))
      (with-temp-buffer
        (insert "foo bar https://x.example")
        (goto-char 9)
        (thing-at-point 'url)))
"##,
        expect,
    );
}
