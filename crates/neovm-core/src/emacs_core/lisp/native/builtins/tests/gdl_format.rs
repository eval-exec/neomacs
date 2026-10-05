//! GNU 31.1 format regressions. Refresh fixtures with UPDATE_EXPECT=1 EMACS=... .
use super::*;

fn assert_gnu(name: &str, form: &str, frozen: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let input = root.join(format!(
            "../../tmp/gdl-format-{name}-{}.el",
            std::process::id()
        ));
        std::fs::write(&input, format!("(prin1 {form})")).expect("GNU probe");
        let output = std::process::Command::new(std::env::var_os("EMACS").expect("EMACS"))
            .args(["-Q", "--batch", "-l"])
            .arg(input)
            .output()
            .expect("GNU execution");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = String::from_utf8(output.stdout).expect("GNU UTF8");
        std::fs::write(
            root.join("src/emacs_core/lisp/native/builtins/tests/gdl_format")
                .join(format!("{name}.expect")),
            &expected,
        )
        .expect("GNU fixture");
        expected
    } else {
        frozen.to_owned()
    };
    let mut ctx = crate::emacs_core::Context::new();
    let value = ctx.eval_str(form).expect("format result");
    assert_eq!(crate::emacs_core::print::print_value(&value), expected);
}

#[test]
fn gdl_nonfinite_radix() {
    assert_gnu(
        "gdl_nonfinite_radix",
        r#"(let (out) (dolist (x '(1.0e+INF -1.0e+INF 0.0e+NaN -0.0e+NaN)) (dolist (fmt '("%x" "%o" "%X" "%b" "%B")) (push (condition-case e (format fmt x) (error e)) out))) (list (nreverse out) (condition-case e (format-message "%x" 1.0e+INF) (error e)) (format "%x" 1e30)))"#,
        include_str!("gdl_format/gdl_nonfinite_radix.expect"),
    );
}

#[test]
fn gdl_width_bound() {
    assert_gnu(
        "gdl_width_bound",
        r#"(list (condition-case e (format "%2305843009213693952s" "a") (error e)) (condition-case e (format "%9223372036854775807d" 12) (error e)) (condition-case e (format "%99999999999999999999s" "a") (error e)))"#,
        include_str!("gdl_format/gdl_width_bound.expect"),
    );
}

#[test]
fn gdl_saturating_counts() {
    assert_gnu(
        "gdl_saturating_counts",
        r#"(list (condition-case e (format "%99999999999999999999$s" 1) (error e)) (condition-case e (format "%18446744073709551616$s" 1) (error e)) (condition-case e (format "%99999999999999999999$s %s" 1 2) (error e)) (format "%.99999999999999999999s" "a") (format "%.18446744073709551616s" "a"))"#,
        include_str!("gdl_format/gdl_saturating_counts.expect"),
    );
}

#[test]
fn gdl_bignum_precision() {
    assert_gnu(
        "gdl_bignum_precision",
        r#"(let ((b (- (expt 2 70)))) (list (format "%.30d" b) (format "%.23d" b) (format "%.30x" b) (format "%#.30x" b) (format "%.5d" -12) (format "%.30d" (- b)) (format "%030d" b)))"#,
        include_str!("gdl_format/gdl_bignum_precision.expect"),
    );
}

#[test]
fn gdl_decimal_float() {
    assert_gnu(
        "gdl_decimal_float",
        r#"(list (format "%.0d" 0.5) (format "%.0d" -0.0) (format "%5.0d|" 0.3) (format "%+.0d" 0.3) (format "%.0i" 0.9) (format "%.0d" 0) (format "%+d" 1.0e+INF) (format "%05d" -1.0e+INF) (format "%d" -0.0e+NaN) (format "%.3d" 1.0e+INF) (format "%.4d" -0.0e+NaN) (format "%.5d" -12.9) (format "%08d" 0.5))"#,
        include_str!("gdl_format/gdl_decimal_float.expect"),
    );
}

#[test]
fn gdl_zero_string_precision() {
    assert_gnu(
        "gdl_zero_string_precision",
        r#"(list (format "%.0s|" "​") (format "%.0s|" "\n") (format "%.0s|" "abc") (format "%3.0s|" (propertize "​" 'face 'bold)) (format "%.1s|" "​a") (format "%.0s|" (unibyte-string 10 255)) (text-properties-at 0 (format "%.0s|" (propertize "​" 'face 'bold))))"#,
        include_str!("gdl_format/gdl_zero_string_precision.expect"),
    );
}
