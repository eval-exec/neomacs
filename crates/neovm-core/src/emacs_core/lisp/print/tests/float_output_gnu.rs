//! GNU print.c:1222-1284 float-output-format precision and readability.
//! Each evaluator owns its printer settings; no cross-mutator cache is used.
use super::super::*;
use std::path::PathBuf;

const PROGRAM: &str = r#"(let (rows)
  (dolist (spec '("%.3g" "%.1g" "%.2g" "%.5g" "%.10g" "%.g" "%.0g" "%g" "%.3e" "%.3f" "%.0f" "%.f" "%.e" "%.16g"))
    (let ((float-output-format spec))
      (dolist (x '(3.14159 100.0 0.0001234 1e100 100000.0 5e-324 0.0001 1e10 12345.0 15.0 0.1 1e9 1e21 -0.0 -0.00001 9.9995))
        (push (list spec (number-to-string x) (prin1-to-string x)
                    (format "%s" x) (format "%S" x)) rows))))
  (nreverse rows))"#;

fn expectation() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/emacs_core/lisp/print/tests/float_output_gnu.expect");
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tmp/float-oracle-{}.el", std::process::id()));
        std::fs::write(&script, format!("(prin1 {PROGRAM})\n")).unwrap();
        let mut command = crate::test_utils::gnu_probe_command();
        let output = command
            .args(["-Q", "--batch", "-l"])
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = String::from_utf8(output.stdout).unwrap();
        std::fs::write(&path, format!("{}\n", expected.trim_end())).unwrap();
        expected
    } else {
        std::fs::read_to_string(path).expect("refresh fixture with UPDATE_EXPECT=1")
    }
}

#[test]
fn float_output_gnu_oracle() {
    crate::test_utils::init_test_tracing();
    let expected = expectation();
    let mut context = crate::test_utils::runtime_startup_context();
    let value = context.eval_str(PROGRAM).expect("oracle evaluation");
    assert_eq!(print_value(&value), expected.trim_end());
}

#[test]
fn general_float_rounding_preserves_exponent() {
    crate::test_utils::init_test_tracing();
    let expected = expectation();
    let mut context = crate::emacs_core::Context::new();
    let rows = context.eval_str(&format!("(quote {expected})")).unwrap();
    let rows = crate::emacs_core::value::list_to_vec(&rows).unwrap();
    let values = [
        3.14159, 100.0, 0.0001234, 1e100, 100000.0, 5e-324, 0.0001, 1e10, 12345.0, 15.0, 0.1, 1e9,
        1e21, -0.0, -0.00001, 9.9995,
    ];
    for (row_index, row) in rows.into_iter().enumerate() {
        let fields = crate::emacs_core::value::list_to_vec(&row).unwrap();
        let value = values[row_index % values.len()];
        let actual = format_float_with_output_format(value, Some(fields[0]));
        assert_eq!(
            actual,
            fields[1].as_lisp_string().unwrap().as_utf8_str().unwrap(),
            "row {row_index}: {value}"
        );
        assert!(actual.parse::<f64>().is_ok(), "readable output: {actual}");
    }
}
