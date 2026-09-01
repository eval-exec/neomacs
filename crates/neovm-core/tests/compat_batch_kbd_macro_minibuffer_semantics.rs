mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn batch_keyboard_macro_drives_the_minibuffer_like_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping batch keyboard-macro minibuffer audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (defun neovm-test-read-string ()
    (interactive)
    (setq neovm-test-read-string-result (read-string "Value: ")))
  (global-set-key (kbd "C-c C-a") #'neovm-test-read-string)
  (execute-kbd-macro
   (vconcat (kbd "C-c C-a")
            (string-to-vector "hello")
            [?\r]))
  neovm-test-read-string-result)"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");

    assert_eq!(
        neovm, gnu,
        "batch keyboard-macro minibuffer semantics differ from GNU Emacs"
    );
}

#[test]
fn dynamically_bound_minibuffer_hooks_observe_the_real_minibuffer_lifecycle() {
    if !oracle_enabled() {
        eprintln!(
            "skipping dynamically bound minibuffer hook audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (defvar neovm-test-minibuffer-hook-log nil)
  (defvar neovm-test-minibuffer-result nil)
  (setq neovm-test-minibuffer-hook-log nil
        neovm-test-minibuffer-result nil)
  (defun neovm-test-read-with-hooks ()
    (interactive)
    (setq neovm-test-minibuffer-result (read-string "Value: ")))
  (global-set-key (kbd "C-c C-a") #'neovm-test-read-with-hooks)
  (let ((minibuffer-setup-hook
         (list (lambda ()
                 (push (list 'setup (minibuffer-prompt) (minibufferp))
                       neovm-test-minibuffer-hook-log))))
        (minibuffer-exit-hook
         (list (lambda ()
                 (push (list 'exit (minibuffer-prompt) (minibufferp))
                       neovm-test-minibuffer-hook-log)))))
    (execute-kbd-macro
     (vconcat (kbd "C-c C-a")
              (string-to-vector "hello")
              [?\r])))
  (list (special-variable-p 'minibuffer-setup-hook)
        (special-variable-p 'minibuffer-exit-hook)
        (nreverse neovm-test-minibuffer-hook-log)
        neovm-test-minibuffer-result))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");

    assert_eq!(
        neovm, gnu,
        "dynamically bound minibuffer hook lifecycle differs from GNU Emacs"
    );
}
