use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires release executable with matching pdump"]
fn batch_error_exits_nonzero_and_reports_only_to_stderr() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let output = Command::new(binary)
        .current_dir(&root)
        .env("RUST_LOG", "off")
        .args([
            "-Q", "--batch", "--eval",
            "(add-hook 'kill-emacs-hook (lambda () (princ (if (and (get-buffer \"*Messages*\") (with-current-buffer \"*Messages*\" (string-match-p \"batch-startup-probe-error\" (buffer-string)))) \"error-was-logged\" \"error-not-logged\"))))",
            "--eval", "(error \"batch-startup-probe-error\")",
        ])
        .output()
        .expect("run batch startup");
    assert_eq!(output.status.code(), Some(255), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line == "batch-startup-probe-error"),
        "{stderr}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "error-not-logged");
}
