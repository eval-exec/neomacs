//! GNU print.c:1193-1209, 1222-1284: validated domains and readable output.
use super::*;
use std::path::PathBuf;

#[test]
fn validated_float_format_rejects_overflowing_or_malformed_domains() {
    crate::test_utils::init_test_tracing();
    for notation in ["e", "f", "g"] {
        assert!(FloatOutputFormat::try_from(Value::string(format!("%.15{notation}"))).is_ok());
        assert!(FloatOutputFormat::try_from(Value::string(format!("%.16{notation}"))).is_err());
    }
    let huge_precision = format!("%.{}g", "9".repeat(4096));
    let padded_overflow = format!("%.{}16g", "0".repeat(4096));
    for malformed in [
        huge_precision.as_str(),
        padded_overflow.as_str(),
        "%g",
        "%.0g",
        "%.0e",
        "%.3G",
        "%.3g trailing",
    ] {
        assert!(
            FloatOutputFormat::try_from(Value::string(malformed)).is_err(),
            "{malformed}"
        );
    }
    assert!(FloatOutputFormat::try_from(Value::NIL).is_err());
    let omitted = FloatOutputFormat::try_from(Value::string("%.f")).expect("omitted precision");
    let zero = FloatOutputFormat::try_from(Value::string("%.0f")).expect("explicit zero");
    assert_eq!(omitted.precision, FloatPrecision::Omitted);
    assert_eq!(zero.precision, FloatPrecision::Explicit(0));
    assert_eq!(omitted.notation, FloatNotation::Fixed);
}

const PROGRAM: &str = r#"(mapcar
  (lambda (spec)
    (let ((float-output-format spec))
      (list spec (mapcar #'number-to-string '(1.5 -0.0 1.0e+INF -1.0e+INF 0.0e+NaN)))))
  '("%.f" "%.0f" "%.g" "%.3e" "%.99999999999999999g" "%.00000000000000000000003g"))"#;

fn gnu_expected() -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return include_str!("float_format_units.expect").to_owned();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input = root.join(format!(
        "../../tmp/float-format-units-{}.el",
        std::process::id()
    ));
    std::fs::write(&input, format!("(prin1 {PROGRAM})\n")).expect("GNU probe");
    let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    let mut command = match std::env::var_os("NEOMACS_PROBE_SANDBOX") {
        Some(sandbox) => {
            let mut command = std::process::Command::new(sandbox);
            command.arg(emacs);
            command
        }
        None => std::process::Command::new(emacs),
    };
    let output = command
        .args(["-Q", "--batch", "-l"])
        .arg(input)
        .output()
        .expect("GNU oracle");
    assert!(
        output.status.success(),
        "GNU: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = String::from_utf8(output.stdout).expect("GNU output");
    std::fs::write(
        root.join("src/emacs_core/lisp/print/tests/float_format_units.expect"),
        &expected,
    )
    .expect("refresh");
    expected
}

#[test]
fn finite_and_nonfinite_format_boundaries_match_saved_gnu() {
    crate::test_utils::init_test_tracing();
    let expected = gnu_expected();
    let mut context = crate::emacs_core::Context::new();
    let rows = context
        .eval_str(&format!("(quote {expected})"))
        .expect("GNU fixture");
    let values = [1.5, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN];
    for row in crate::emacs_core::value::list_to_vec(&rows).expect("rows") {
        let fields = crate::emacs_core::value::list_to_vec(&row).expect("row");
        let outputs = crate::emacs_core::value::list_to_vec(&fields[1]).expect("outputs");
        assert_eq!(outputs.len(), values.len());
        for (value, expected) in values.into_iter().zip(outputs) {
            assert_eq!(
                format_float_with_output_format(value, Some(fields[0])),
                expected
                    .as_lisp_string()
                    .expect("GNU string")
                    .as_utf8_str()
                    .expect("ASCII")
            );
        }
    }
}
