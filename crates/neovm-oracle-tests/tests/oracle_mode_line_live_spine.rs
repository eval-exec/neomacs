//! Active GNU mode-line oracle: batch GNU does not enter display_mode_element.

#[path = "../src/common.rs"]
mod common;

use std::path::Path;
use std::process::Command;

use common::oracle_sandbox::{OracleSandbox, project_root};
use common::return_if_neovm_enable_oracle_proptest_not_set;

fn run_active_gnu(form: &str) -> String {
    let requested = std::env::var("NEOVM_FORCE_ORACLE_PATH").unwrap_or_else(|_| "emacs".into());
    let reference = neomacs_parity_reference::attest(
        Path::new(&requested),
        neomacs_parity_reference::AttestationDepth::Fingerprint,
    )
    .expect("active mode-line oracle must use the pinned GNU reference");
    tracing::debug!(reference = %reference.stamp(), "active mode-line oracle attested");
    let artifacts = OracleSandbox::create_fixture_tempdir().expect("oracle artifact directory");
    let display = neomacs_infra::display::start_xvfb(artifacts.path())
        .expect("active GNU mode-line oracle requires Xvfb");
    let result_path = artifacts.path().join("mode-line-result.txt");
    let sandbox = OracleSandbox::new(form, &[], &project_root().join("lisp"))
        .expect("mode-line oracle sandbox");
    let mut command = Command::new(reference.executable());
    sandbox.configure(&mut command);
    command
        .envs(display.env().iter().map(|(key, value)| (key, value)))
        .env_remove("WAYLAND_DISPLAY")
        .env("EMACSNATIVELOADPATH", "/dev/null")
        .env("NO_AT_BRIDGE", "1")
        .env("NEOVM_ACTIVE_MODE_LINE_RESULT", &result_path)
        .args([
            "-Q",
            "--eval",
            r#"(progn
                 (condition-case err
                     (let ((form (with-temp-buffer
                                   (insert-file-contents
                                    (getenv "NEOVM_ORACLE_FORM_FILE"))
                                   (read (current-buffer)))))
                       (with-temp-file (getenv "NEOVM_ACTIVE_MODE_LINE_RESULT")
                         (princ "OK " (current-buffer))
                         (prin1 (eval form t) (current-buffer))))
                   (error (kill-emacs 1)))
                 (kill-emacs 0))"#,
        ]);
    let output = command.output().expect("run active GNU mode-line oracle");
    assert!(
        output.status.success(),
        "active GNU mode-line oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(result_path).expect("active GNU mode-line oracle result")
}

fn assert_active_mode_line_parity(form: &str, expected: expect_test::Expect) {
    let mode = std::env::var("NEOVM_ORACLE_MODE")
        .unwrap_or_else(|_| "snapshot".into())
        .to_ascii_lowercase();
    match mode.as_str() {
        "snapshot" | "snap" | "expected" => {
            let neovm = common::run_neovm_eval(form).expect("Neomacs mode-line oracle result");
            expected.assert_eq(&format!("{neovm:?}"));
        }
        "refresh" | "bless" | "update" => {
            let gnu = run_active_gnu(form);
            expected.assert_eq(&format!("{gnu:?}"));
        }
        "verify" | "live" => {
            let gnu = run_active_gnu(form);
            if mode == "verify" {
                expected.assert_eq(&format!("{gnu:?}"));
            }
            let neovm = common::run_neovm_eval(form).expect("Neomacs mode-line oracle result");
            assert_eq!(neovm, gnu, "active mode-line parity for {form}");
        }
        other => panic!("unknown NEOVM_ORACLE_MODE={other:?}"),
    }
}

#[test]
fn format_mode_line_detached_tail_follows_live_spine() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" 2)""#]];
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (setq u34-mode-line-oracle-format (list "A"
                             '(:eval (progn (setcdr (cdr u34-mode-line-oracle-format) nil)
                                            (garbage-collect)
                                            ""))
                             (concat "B")))
             (list (format-mode-line u34-mode-line-oracle-format 0) (length u34-mode-line-oracle-format)))"#,
        expect,
    );
}

#[test]
fn format_mode_line_detached_tail_preserves_accumulated_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" 73 2)""#]];
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (setq u34-mode-line-oracle-format
                   (list '(:propertize "A" u34-mode-line-property 73)
                         '(:eval (progn (setcdr (cdr u34-mode-line-oracle-format) nil)
                                        (garbage-collect)
                                        ""))
                         (concat "B")))
             (let ((rendered (format-mode-line u34-mode-line-oracle-format)))
               (list (substring-no-properties rendered)
                     (get-text-property 0 'u34-mode-line-property rendered)
                     (length u34-mode-line-oracle-format))))"#,
        expect,
    );
}

#[test]
fn format_mode_line_replaced_tail_follows_live_spine() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AC\" 3)""#]];
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (setq u34-mode-line-oracle-format (list "A"
                             '(:eval (progn (setcdr (cdr u34-mode-line-oracle-format) (list (concat "C")))
                                            (garbage-collect)
                                            ""))
                             (concat "B")))
             (list (format-mode-line u34-mode-line-oracle-format 0) (length u34-mode-line-oracle-format)))"#,
        expect,
    );
}
