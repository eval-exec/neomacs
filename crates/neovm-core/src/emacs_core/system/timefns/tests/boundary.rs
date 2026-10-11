use super::*;
use crate::emacs_core::format::builtin_format_time_string;
use std::process::Command;

/// GNU 31.1's answer to FORM, saved as `tsb-NAME.expect` beside this file.
///
/// `UPDATE_EXPECT=1` refreshes the fixture from the GNU binary in `EMACS`
/// (default `~/.local/bin/emacs`), run through the campaign's Lisp sandbox.
fn gnu_expectation(name: &str, form: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/emacs_core/system/timefns/tests")
        .join(format!("tsb-{name}.expect"));
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let sandbox = neomacs_infra::workspace_root()
            .ancestors()
            .map(|root| root.join("tmp/v10x/sandbox-run.sh"))
            .find(|path| path.is_file())
            .expect("GNU oracle refresh requires the Lisp sandbox");
        let emacs = std::env::var_os("EMACS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                    .join(".local/bin/emacs")
            });
        let output = Command::new(sandbox)
            .arg(emacs)
            .args(["-Q", "--batch", "--eval"])
            .arg(format!("(prin1 {form})"))
            .output()
            .expect("start the GNU oracle");
        assert!(
            output.status.success(),
            "GNU oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(&path, &output.stdout).unwrap();
    }
    std::fs::read_to_string(path).unwrap().trim().to_owned()
}

#[test]
fn nul_time_zone_matches_gnu() {
    let expected = gnu_expectation(
        "nul-time-zone",
        r#"(list (decode-time 0 "UTC\0junk") (format-time-string "%Z %H" 0 "EST5\0x") (current-time-zone 0 "JST-9\0") (decode-time 0 '(3600 "AB\0C")) (encode-time 0 0 0 1 1 1970 "UTC\0junk") (current-time-string 0 "UTC\0junk"))"#,
    );
    let _lock = tz_test_lock();
    reset_tz_rule();
    let zero = Value::fixnum(0);
    let values = vec![
        builtin_decode_time(vec![zero, Value::string("UTC\0junk")]).unwrap(),
        builtin_format_time_string(vec![Value::string("%Z %H"), zero, Value::string("EST5\0x")])
            .unwrap(),
        builtin_current_time_zone(vec![zero, Value::string("JST-9\0")]).unwrap(),
        builtin_decode_time(vec![
            zero,
            Value::list(vec![Value::fixnum(3600), Value::string("AB\0C")]),
        ])
        .unwrap(),
        builtin_encode_time(vec![
            zero,
            zero,
            zero,
            Value::fixnum(1),
            Value::fixnum(1),
            Value::fixnum(1970),
            Value::string("UTC\0junk"),
        ])
        .unwrap(),
        builtin_current_time_string(vec![zero, Value::string("UTC\0junk")]).unwrap(),
    ];
    assert_eq!(
        crate::emacs_core::print::print_value(&Value::list(values)),
        expected
    );
}

#[test]
fn calendar_field_overflow_matches_gnu() {
    let expected = gnu_expectation(
        "calendar-field-overflow",
        r#"(condition-case e (encode-time 0 0 0 4294967297 1 2000 t) (error e))"#,
    );
    let _lock = tz_test_lock();
    reset_tz_rule();
    let args = vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(4_294_967_297),
        Value::fixnum(1),
        Value::fixnum(2000),
        Value::T,
    ];
    assert_eq!(condition_case_print(builtin_encode_time(args)), expected);
}

#[test]
fn legacy_time_seconds_overflow_matches_gnu() {
    let expected = gnu_expectation(
        "legacy-time-overflow",
        r#"(condition-case e (decode-time '(2305843009213693951 0) t) (error e))"#,
    );
    let _lock = tz_test_lock();
    reset_tz_rule();
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::list(vec![
                Value::fixnum(2_305_843_009_213_693_951),
                Value::fixnum(0)
            ]),
            Value::T
        ])),
        expected
    );
}

#[test]
fn encode_time_clamps_numeric_zone_like_gnu() {
    let expected = gnu_expectation(
        "encode-numeric-zone",
        "(list (encode-time 0 0 0 1 1 2000 most-positive-fixnum) (encode-time 0 0 0 1 1 2000 most-negative-fixnum))",
    );
    let _lock = tz_test_lock();
    reset_tz_rule();
    let values = [2_305_843_009_213_693_951, -2_305_843_009_213_693_952].map(|zone| {
        builtin_encode_time(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(1),
            Value::fixnum(2000),
            Value::fixnum(zone),
        ])
        .unwrap()
    });
    assert_eq!(
        crate::emacs_core::print::print_value(&Value::list(values.to_vec())),
        expected
    );
}

#[cfg(unix)]
#[test]
fn encode_time_extreme_year_matches_gnu_without_hanging() {
    // Fixture refreshed by standalone sandboxed GNU, avoiding a nested scope
    // or an oracle subprocess inheriting the test-local CPU soft limit.
    let expected = include_str!("tsb-calendar-year-overflow.expect").trim();
    let _lock = tz_test_lock();
    reset_tz_rule();
    let args = vec![Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1),
        Value::fixnum(1_i64 << 40),
    ])];
    let _cpu_limit = super::cpu_soft_limit::CpuSoftLimit::two_more_seconds()
        .expect("bound only this isolated nextest test process");
    assert_eq!(condition_case_print(builtin_encode_time(args)), expected);
}

#[cfg(unix)]
#[test]
fn encode_time_extreme_month_matches_gnu_without_hanging() {
    let expected = include_str!("tsb-calendar-month-overflow.expect").trim();
    let _lock = tz_test_lock();
    reset_tz_rule();
    let args = vec![Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1_i64 << 40),
        Value::fixnum(2000),
    ])];
    let _cpu_limit = super::cpu_soft_limit::CpuSoftLimit::two_more_seconds()
        .expect("bound only this isolated nextest test process");
    assert_eq!(condition_case_print(builtin_encode_time(args)), expected);
}
