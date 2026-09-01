//! Divergence tests: final batch - remaining Emacs subsystems.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_decode_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments decode-time 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((decoded (decode-time 0 0 12 1 1 2024 t)))
  (list (nth 0 decoded)
        (nth 1 decoded)
        (nth 2 decoded)
        (nth 3 decoded)
        (nth 4 decoded)
        (nth 5 decoded)))"#,
        expect,
    );
}

#[test]
fn divergence_format_time_string_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"2024-01-01\" \"1704085200\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((time (encode-time 0 0 0 1 1 2024)))
  (list (stringp (format-time-string "%Y-%m-%d %H:%M:%S" time))
        (format-time-string "%Y-%m-%d" time)
        (format-time-string "%s" time)))"#,
        expect,
    );
}

#[test]
fn divergence_time_add_sub() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 1704128400.0 1704132000.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((t1 (encode-time 0 0 12 1 1 2024))
         (t2 (time-add t1 3600))
         (t3 (time-subtract t2 t1)))
  (list (= (float-time t3) 3600.0)
        (float-time t1)
        (float-time t2)))"#,
        expect,
    );
}

#[test]
fn divergence_current_time_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((tz (current-time-zone)))
  (list (integerp (car tz))
        (stringp (cadr tz))))"#,
        expect,
    );
}

#[test]
fn divergence_seconds_to_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 1704067200.0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((time (seconds-to-time 1704067200)))
  (list (consp time)
        (float-time time)
        (>= (float-time time) 0)))"#,
        expect,
    );
}

#[test]
fn divergence_time_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((t1 (current-time))
        (t2 (copy-sequence (current-time))))
  (list (time-equal-p t1 t1)
        (time-less-p t1 (time-add t1 1))))"#,
        expect,
    );
}

#[test]
fn divergence_days_to_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((time (days-to-time 1)))
  (list (consp time)
        (= (float-time time) 86400.0)))"#,
        expect,
    );
}

#[test]
fn divergence_time_parse_date() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'parse-time-string)
  (fboundp 'date-to-time)
  (fboundp 'format-seconds))"#,
        expect,
    );
}

#[test]
fn divergence_timer_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'run-at-time)
  (fboundp 'run-with-timer)
  (fboundp 'run-with-idle-timer)
  (fboundp 'cancel-timer))"#,
        expect,
    );
}

#[test]
fn divergence_process_list_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (listp (process-list))
  (>= (length (process-list)) 0))"#,
        expect,
    );
}
