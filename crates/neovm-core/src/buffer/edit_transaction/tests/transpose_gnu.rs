//! Transposition observations refreshed only from GNU Emacs 31.1.
use crate::test_utils::runtime_startup_eval_one;

// GNU: editfns.c:4467-4479 (point), 4631-4641 (gap), 4782-4797
// (LEAVE-MARKERS), insdel.c:413-455 (byte-coordinate recomputation).
use std::path::PathBuf;

fn assert_gnu(name: &str, form: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root
        .join("src/buffer/edit_transaction/tests")
        .join(format!("{name}.expect"));
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = root
            .join("../../tmp")
            .join(format!("transpose-{name}-{}.el", std::process::id()));
        std::fs::write(
            &script,
            format!(
                "(let ((print-length nil) (print-level nil)) (princ \"OK \") (prin1 {form}))\n"
            ),
        )
        .unwrap();
        let emacs = std::env::var_os("EMACS")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin/emacs")
            });
        // Campaign refreshes supply their PID/memory sandbox; ordinary fixture
        // refreshes remain portable to checkouts without campaign tooling.
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
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(&fixture, output.stdout).unwrap();
    }
    let expected = std::fs::read_to_string(fixture).unwrap();
    assert_eq!(runtime_startup_eval_one(form), expected.trim_end());
}

#[test]
fn transpose_leave_markers_rebuilds_byte_positions() {
    assert_gnu(
        "transpose-markers",
        r#"(list
(with-temp-buffer (insert "a中") (let ((m (copy-marker 2))) (transpose-regions 1 2 2 3 t) (list (buffer-string) (char-after 2) (position-bytes 2) (marker-position m))))
(with-temp-buffer (insert "中ab") (let ((m (copy-marker 2))) (transpose-regions 1 2 2 3 t) (delete-region 2 3) (list (buffer-string) (marker-position m)))))"#,
    );
}

#[test]
fn transpose_leave_markers_preserves_point() {
    assert_gnu(
        "transpose-point",
        r#"(let ((results nil)) (dolist (pos '(1 2 3 4 5 6 7 8 9)) (with-temp-buffer (insert "abcdefgh") (goto-char pos) (transpose-regions 1 3 5 7 t) (push (point) results))) (nreverse results))"#,
    );
}
