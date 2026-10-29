//! Category word wrapping in a real test-owned terminal, including kinsoku.
#![cfg(unix)]
use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;
use std::os::unix::process::CommandExt;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[path = "../src/tty_test_terminal.rs"]
mod tty_test_terminal;
use tty_test_terminal::TestTerminal;

const SUPPORT: &str = r#"
(defvar wrap623-counter 0)
(defun wrap623-row (text enabled &optional setup)
 (switch-to-buffer (get-buffer-create (format "*wrap623-%d*" (setq wrap623-counter (1+ wrap623-counter)))))
 (insert text)
 (set-category-table (copy-category-table))
 (setq-local word-wrap t truncate-lines nil truncate-partial-width-windows nil word-wrap-by-category enabled)
 (when setup (funcall setup))
 (goto-char 1)
 (set-window-start (selected-window) 1 t)
 (let ((noninteractive nil)) (redisplay t) (vertical-motion 1))
 (point))
"#;

#[test]
fn category_word_wrap_cjk_uses_the_real_terminal_row_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r#"(progn {SUPPORT}
(let ((text (concat "supertag v0.4 " (make-string 60 ?提))))
 (list (window-body-width) (char-width ?提)
       (wrap623-row text nil) (wrap623-row text t)
       (wrap623-row text t (lambda () (modify-category-entry ?提 ?| nil t))))))"#
    );
    assert_interactive_parity(&form, "(24 2 15 19 15)");
}

#[test]
fn category_word_wrap_custom_ascii_table_obeys_kinsoku() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r#"(progn {SUPPORT}
(let ((text (concat "supertag v0.4 " (make-string 60 ?x)))
      (closing (concat (make-string 23 ?x) "!" (make-string 20 ?x)))
      (opening (concat (make-string 22 ?x) "?" (make-string 20 ?x))))
 (list (wrap623-row text t)
       (wrap623-row text t (lambda () (modify-category-entry ?x ?|)))
       (wrap623-row closing t (lambda () (modify-category-entry ?x ?|) (modify-category-entry ?! ?|)))
       (wrap623-row closing t (lambda () (modify-category-entry ?x ?|) (modify-category-entry ?! ?|) (modify-category-entry ?! ?>)))
       (wrap623-row opening t (lambda () (modify-category-entry ?x ?|) (modify-category-entry ?? ?|)))
       (wrap623-row opening t (lambda () (modify-category-entry ?x ?|) (modify-category-entry ?? ?|) (modify-category-entry ?? ?<))))))"#
    );
    assert_interactive_parity(&form, "(15 24 24 23 24 23)");
}

// GNU --batch does not perform interactive word wrapping, even if a form
// creates a terminal frame. Reuse the owned PTY/drainer and run each editor's
// actual event loop; the Lisp probe writes its result independently of ANSI.
fn assert_interactive_parity(form: &str, expected: &str) {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("probe.el");
    fs::write(
        &script,
        format!(
            r#";;; -*- lexical-binding: t; -*-
(run-with-timer 0.2 nil
 (lambda ()
  (condition-case err
   (let ((result {form}))
    (with-temp-file (getenv "CATEGORY_WRAP_RESULT") (prin1 result (current-buffer)))
    (kill-emacs 0))
   (error (with-temp-file (getenv "CATEGORY_WRAP_RESULT") (prin1 err (current-buffer)))
          (kill-emacs 1)))))
"#
        ),
    )
    .unwrap();
    for (label, binary) in [
        ("GNU", common::oracle_emacs_path()),
        ("Neomacs", common::neomacs_binary_path().into()),
    ] {
        let result = temp.path().join(format!("{label}.result"));
        run_editor(&binary, &script, &result);
        assert_eq!(fs::read_to_string(&result).unwrap(), expected, "{label}");
    }
}

fn run_editor(binary: &Path, script: &Path, result: &Path) {
    let terminal = TestTerminal::with_size(12, 24);
    let slave = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&terminal.name)
        .unwrap();
    let mut command = Command::new(binary);
    command
        .args(["-nw", "-Q", "--load"])
        .arg(script)
        .env("TERM", "screen-256color")
        .env("HOME", result.parent().unwrap())
        .env("CATEGORY_WRAP_RESULT", result)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::from(slave));
    // SAFETY: the child only performs session/TTY syscalls before exec; no
    // allocation, shared lock or Rust runtime state is accessed in the hook.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = terminal.finish_with_output();
    assert!(
        status.is_some_and(|status| status.success()),
        "{}: {:?}\n{}",
        binary.display(),
        status,
        String::from_utf8_lossy(&output)
    );
}
