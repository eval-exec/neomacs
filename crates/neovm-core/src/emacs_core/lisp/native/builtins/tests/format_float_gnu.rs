//! GNU editfns.c:3840-4170 float-format regressions. Fixtures come from GNU only.

const CASES: &[(&str, &str, &str)] = &[
    (
        "precision",
        r#"(list (length (format "%.65536f" 0.1)) (length (format "%.70000e" 0.1)) (length (format "%.70000g" 0.1)) (length (format "%.70000f" 5)) (length (format "%#.70000g" 1.0)) (length (format "%.70000f" 1.0e+INF)))"#,
        include_str!("format_float_gnu/precision.expect"),
    ),
    (
        "signs",
        r#"(list (format "%+f" -0.0) (format "% g" -0.0) (format "%+e" -0.0) (format "%+f" 1.0e+INF) (format "% f" 0.0e+NaN) (format "%05f|" 1.0e+INF) (format "%+08f|" -1.0e+INF) (format "%010.3e" -0.0e+NaN) (format "%+g" 0.0e+NaN) (format "% 08.3f" 3.14159) (format "%08f" -0.0) (format "%#g" 12.0))"#,
        include_str!("format_float_gnu/signs.expect"),
    ),
    (
        "integers",
        r#"(list (format "%.0f" 9007199254740993) (format "%.0f" (1- (expt 2 64))) (format "%.20e" most-positive-fixnum) (format "%f" most-positive-fixnum) (format "%.20g" (1- (expt 2 63))) (format "%.0f" (- 1 (expt 2 63))) (format "%.0f" (expt 2 64)) (format "%.0f" (1+ (expt 2 64))) (format "%.0f" (- (expt 2 63))))"#,
        include_str!("format_float_gnu/integers.expect"),
    ),
];

fn oracle(name: &str, program: &str, frozen: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return frozen.trim_end().to_owned();
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input = root.join(format!(
        "../../tmp/gdl-float-{name}-{}.el",
        std::process::id()
    ));
    std::fs::write(
        &input,
        format!("(let ((print-length nil) (print-level nil)) (prin1 {program}))"),
    )
    .expect("GNU input");
    let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    let out = std::process::Command::new(emacs)
        .args(["-Q", "--batch", "-l"])
        .arg(input)
        .output()
        .expect("GNU oracle");
    assert!(
        out.status.success(),
        "GNU {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = String::from_utf8(out.stdout).expect("GNU UTF8");
    let fixture = root
        .join("src/emacs_core/lisp/native/builtins/tests/format_float_gnu")
        .join(format!("{name}.expect"));
    let expected = format!("{}\n", expected.trim_end());
    if std::fs::read_to_string(&fixture).ok().as_deref() != Some(expected.as_str()) {
        std::fs::write(fixture, &expected).expect("GNU fixture");
    }
    expected.trim_end().to_owned()
}

fn assert_case(index: usize) {
    crate::test_utils::init_test_tracing();
    let (name, form, frozen) = CASES[index];
    let expected = oracle(name, form, frozen);
    let mut ctx = crate::emacs_core::eval::Context::new();
    let value = ctx.eval_str(form).expect("format evaluation");
    assert_eq!(crate::emacs_core::print::print_value(&value), expected);
}

#[test]
fn float_format_precision_above_u16_matches_gnu() {
    assert_case(0);
}
#[test]
fn float_format_signs_and_padding_match_gnu() {
    assert_case(1);
}
#[test]
fn float_format_integer_precision_matches_gnu() {
    assert_case(2);
}
