mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn batch_keyboard_macro_drives_character_readers_like_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping batch keyboard-macro character-input audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (defun neovm-test-read-char ()
    (interactive)
    (setq neovm-test-read-char-result
          (list (read-char "char: ")
                (key-description (this-command-keys)))))
  (defun neovm-test-read-event ()
    (interactive)
    (setq neovm-test-read-event-result
          (list (read-event "event: ")
                (key-description (this-command-keys)))))
  (defun neovm-test-read-char-exclusive ()
    (interactive)
    (setq neovm-test-read-char-exclusive-result
          (list (read-char-exclusive "exclusive: ")
                (key-description (this-command-keys)))))
  (global-set-key (kbd "C-c c") #'neovm-test-read-char)
  (global-set-key (kbd "C-c e") #'neovm-test-read-event)
  (global-set-key (kbd "C-c x") #'neovm-test-read-char-exclusive)
  (execute-kbd-macro (kbd "C-c c a"))
  (execute-kbd-macro (kbd "C-c e b"))
  (execute-kbd-macro (kbd "C-c x c"))
  (list neovm-test-read-char-result
        neovm-test-read-event-result
        neovm-test-read-char-exclusive-result))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");

    assert_eq!(
        neovm, gnu,
        "batch keyboard-macro character reader semantics differ from GNU Emacs"
    );
}

#[test]
fn minibuffer_recursive_command_starts_with_fresh_command_keys_like_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping minibuffer recursive command-key audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (defvar neovm-minibuffer-command-keys-log nil)
  (defun neovm-minibuffer-command-keys-record ()
    (when (minibufferp)
      (push (list this-command
                  (key-description (this-command-keys))
                  (key-description (this-single-command-keys))
                  (key-description (this-single-command-raw-keys)))
            neovm-minibuffer-command-keys-log)))
  (defun neovm-read-char-from-minibuffer-command ()
    (interactive)
    (minibuffer-with-setup-hook
        (lambda ()
          (add-hook 'post-command-hook
                    #'neovm-minibuffer-command-keys-record nil t))
      (read-char-from-minibuffer "Character: ")))
  (global-set-key (kbd "C-c r") #'neovm-read-char-from-minibuffer-command)
  (execute-kbd-macro (kbd "C-c r g"))
  (nreverse neovm-minibuffer-command-keys-log))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");

    assert_eq!(
        neovm, gnu,
        "minibuffer recursive command-key lifecycle differs from GNU Emacs"
    );
}

#[test]
fn pre_command_hook_sees_the_previous_macro_command_like_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping keyboard-macro command-history audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (defvar neovm-command-lifecycle-log nil)
  (defun neovm-command-lifecycle-a () (interactive))
  (defun neovm-command-lifecycle-b () (interactive))
  (defun neovm-command-lifecycle-record ()
    (when (memq this-command
                '(neovm-command-lifecycle-a neovm-command-lifecycle-b))
      (push (list this-command real-this-command
                  last-command real-last-command)
            neovm-command-lifecycle-log)))
  (global-set-key (kbd "C-c a") #'neovm-command-lifecycle-a)
  (global-set-key (kbd "C-c b") #'neovm-command-lifecycle-b)
  (add-hook 'pre-command-hook #'neovm-command-lifecycle-record)
  (execute-kbd-macro (kbd "C-c a C-c b"))
  (nreverse neovm-command-lifecycle-log))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");

    assert_eq!(
        neovm, gnu,
        "pre-command-hook command-history lifecycle differs from GNU Emacs"
    );
}

#[test]
fn batch_keyboard_macro_prefixes_do_not_create_echo_area_buffers_like_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping batch key-prefix echo audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (defun neovm-batch-prefix-command () (interactive))
  (let ((global-map (copy-keymap global-map)))
    (global-set-key (kbd "C-c p") #'neovm-batch-prefix-command)
    (execute-kbd-macro (kbd "C-c p")))
  (list (get-buffer " *Echo Area 0*")
        (get-buffer " *Echo Area 1*")
        (current-message)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");

    assert_eq!(
        neovm, gnu,
        "batch keyboard-macro prefix echo lifecycle differs from GNU Emacs"
    );
}
