//! Coverage for low-coverage subsystems: abbrev, timer, gc/alloc, float
//! transcendentals, category. Deterministic probes (gc/timer compare structure,
//! not runtime-varying values).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- abbrev -----------------------------------------------------------------

#[test]
fn div_ts_abbrev_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"bar \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (make-abbrev-table)))
  (define-abbrev tbl "foo" "bar")
  (with-temp-buffer
    (abbrev-table-put tbl 'neo-local t)
    (set (make-local-variable 'local-abbrev-table) tbl)
    (abbrev-mode 1)
    (insert "foo ")
    (expand-abbrev)
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_ts_abbrev_case_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"bar\" \"Bar\" \"BAR\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (make-abbrev-table)))
  (define-abbrev tbl "foo" "bar")
  (mapcar (lambda (w)
            (with-temp-buffer
              (set (make-local-variable 'local-abbrev-table) tbl)
              (abbrev-mode 1)
              (insert w " ") (expand-abbrev)
              (buffer-substring 1 (1- (point-max)))))
          '("foo" "Foo" "FOO")))
"##,
        expect,
    );
}

#[test]
fn div_ts_abbrev_table_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t abc \"def\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (make-abbrev-table)))
  (define-abbrev tbl "abc" "def")
  (list (abbrev-table-p tbl)
        (abbrev-symbol "abc" tbl)
        (abbrev-expansion "abc" tbl)))
"##,
        expect,
    );
}

// --- timer ------------------------------------------------------------------

#[test]
fn div_ts_timer_create_cancel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tt (run-with-timer 0 nil (lambda () nil))))
  (list (timerp tt)
        (prog1 (if (memq tt timer-list) t nil) (cancel-timer tt))
        (if (memq tt timer-list) t nil)))
"##,
        expect,
    );
}

#[test]
fn div_ts_timer_relative_time_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((rt (timer-relative-time (current-time) 5 0)))
  (list (consp rt) (<= (length rt) 4)))
"##,
        expect,
    );
}

#[test]
fn div_ts_idle_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tt (run-with-idle-timer 0 nil (lambda () nil))))
  (prog1 (timerp tt) (cancel-timer tt)))
"##,
        expect,
    );
}

// (gc / alloc tests intentionally omitted per request.)

// --- float transcendentals --------------------------------------------------

#[test]
fn div_ts_float_trig() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0.0 1.0 0.0 1000000 -1000000)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sin 0.0) (cos 0.0) (tan 0.0)
      (round (* 1e6 (sin (/ pi 2))))
      (round (* 1e6 (cos pi))))
"##,
        expect,
    );
}

#[test]
fn div_ts_float_sqrt_exp_log() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1.4142135623730951 4.0 2.718281828459045 1.0 2.0 1.4142135623730951)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sqrt 2.0) (sqrt 16.0) (exp 1.0)
      (log (exp 1.0)) (log 100.0 10.0) (expt 2.0 0.5))
"##,
        expect,
    );
}

#[test]
fn div_ts_float_inverse_and_domain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1570796 785398 -0.0e+NaN -1.0e+INF 0.0e+NaN)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (round (* 1e6 (asin 1.0))) (round (* 1e6 (atan 1.0)))
      (condition-case e (sqrt -1.0) (error (car e)))
      (condition-case e (log 0) (error (car e)))
      (condition-case e (asin 2.0) (error (car e))))
"##,
        expect,
    );
}

// --- category ---------------------------------------------------------------

#[test]
fn div_ts_category_table_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (category-table)))
  (list (category-table-p ct)
        (category-table-p (standard-category-table))
        (eq ct (standard-category-table))))
"##,
        expect,
    );
}

#[test]
fn div_ts_char_category_classify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-category)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-category ?a)
      (char-category ?A)
      (char-category ?1)
      (char-category ?\s)
      (char-category ?\x4e2d))
"##,
        expect,
    );
}

#[test]
fn div_ts_category_modify_and_member() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-in-category-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (copy-category-table (standard-category-table))))
  (modify-category-entry ?x ?l ct t)
  (list (char-in-category-p ?x ?l ct)
        (category-docstring ?l ct)))
"##,
        expect,
    );
}

#[test]
fn div_ts_category_set_mnemonics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments make-category-set 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((ct (copy-category-table (standard-category-table)))
       (cs (make-category-set "l" ct)))
  (list (stringp (category-set-mnemonics cs))
        (category-set-mnemonics cs)))
"##,
        expect,
    );
}
