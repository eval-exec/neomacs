//! GNU-backed circular traversal regressions. Fixtures are refreshed only
//! from GNU with UPDATE_EXPECT=1. The binary oracle twin additionally uses
//! NEOVM_ORACLE_MODE=refresh to refresh its inline GNU expectations.
use crate::test_utils::runtime_startup_eval_one;

fn gnu_fixture(case: &str, form: &str, cached: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return cached.trim_end().to_owned();
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = root
        .join("../../tmp")
        .join(format!("circular-fixture-{case}-{}.el", std::process::id(),));
    std::fs::write(&script, format!("(prin1 {form})\n")).expect("GNU fixture input");
    let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    // Campaign probes supply the PID/memory sandbox; ordinary fixture refreshes
    // remain portable to checkouts that do not contain the campaign tooling.
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
        root.join("src/emacs_core/runtime/eval/tests/circular_lists_cases")
            .join(format!("{case}.expect")),
        format!("{transcript}\n"),
    )
    .expect("GNU fixture output");
    transcript
}

#[test]
fn circular_lists_rassoc_matches_gnu() {
    let form = include_str!("circular_lists_cases/rassoc.el");
    let expected = gnu_fixture(
        "rassoc",
        form,
        include_str!("circular_lists_cases/rassoc.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn circular_lists_concat_matches_gnu() {
    let form = include_str!("circular_lists_cases/concat.el");
    let expected = gnu_fixture(
        "concat",
        form,
        include_str!("circular_lists_cases/concat.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn circular_lists_apply_matches_gnu() {
    let form = include_str!("circular_lists_cases/apply.el");
    let expected = gnu_fixture(
        "apply",
        form,
        include_str!("circular_lists_cases/apply.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn circular_lists_eval_matches_gnu() {
    let form = include_str!("circular_lists_cases/eval.el");
    let expected = gnu_fixture(
        "eval",
        form,
        include_str!("circular_lists_cases/eval.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn circular_lists_cycle_tails_matches_gnu() {
    let form = include_str!("circular_lists_cases/cycle_tails.el");
    let expected = gnu_fixture(
        "cycle_tails",
        form,
        include_str!("circular_lists_cases/cycle_tails.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn circular_lists_compiled_matches_gnu() {
    let form = include_str!("circular_lists_cases/compiled.el");
    let expected = gnu_fixture(
        "compiled",
        form,
        include_str!("circular_lists_cases/compiled.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn circular_lists_initializer_callbacks_match_gnu() {
    let form = include_str!("circular_lists_cases/init_callbacks.el");
    let expected = gnu_fixture(
        "init_callbacks",
        form,
        include_str!("circular_lists_cases/init_callbacks.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}
