//! GNU-backed buffer primitive regressions. Refresh cached expectations only
//! from GNU with UPDATE_EXPECT=1; conditions are ordinary Lisp result lists.
use crate::test_utils::runtime_startup_eval_one;

fn gnu_fixture(case: &str, form: &str, cached: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return cached.trim_end().to_owned();
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("../../tmp").join(format!(
        "gdn-primitives-fixture-{case}-{}.el",
        std::process::id()
    ));
    std::fs::write(&script, format!("(prin1 {form})\n")).expect("GNU fixture input");
    let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    // Campaign refreshes provide the PID/memory sandbox; the helper also works
    // in ordinary checkouts that do not contain the campaign tooling.
    let mut command = if let Some(sandbox) = std::env::var_os("NEOVM_LISP_SANDBOX") {
        let mut command = std::process::Command::new(sandbox);
        command.arg(emacs);
        command
    } else {
        std::process::Command::new(emacs)
    };
    let output = command
        .args(["-Q", "--batch", "-l"])
        .arg(script)
        .output()
        .expect("GNU oracle");
    assert!(
        output.status.success(),
        "GNU {case}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let transcript = format!(
        "OK {}",
        String::from_utf8(output.stdout)
            .expect("GNU UTF-8")
            .trim_end()
    );
    std::fs::write(
        root.join("src/emacs_core/editing/buffer/tests/gdn_primitives_cases")
            .join(format!("{case}.expect")),
        format!("{transcript}\n"),
    )
    .expect("GNU fixture output");
    transcript
}

#[test]
fn gdn_buf14() {
    let form = include_str!("gdn_primitives_cases/buf14.el");
    let expected = gnu_fixture(
        "buf14",
        form,
        include_str!("gdn_primitives_cases/buf14.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn gdn_buf15() {
    let form = include_str!("gdn_primitives_cases/buf15.el");
    let expected = gnu_fixture(
        "buf15",
        form,
        include_str!("gdn_primitives_cases/buf15.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn gdn_buf16() {
    let form = include_str!("gdn_primitives_cases/buf16.el");
    let expected = gnu_fixture(
        "buf16",
        form,
        include_str!("gdn_primitives_cases/buf16.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn gdn_buf18() {
    let form = include_str!("gdn_primitives_cases/buf18.el");
    let expected = gnu_fixture(
        "buf18",
        form,
        include_str!("gdn_primitives_cases/buf18.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}
