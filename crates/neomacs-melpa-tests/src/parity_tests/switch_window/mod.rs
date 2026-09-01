//! Practical parity for switch-window's window-selection machinery.
//!
//! The interactive loop reads a key from the keyboard, which a batch
//! editor cannot supply, so the workflows pin everything the loop is
//! built from: the documented configuration surface (defcustoms, the
//! label/background faces), the window enumeration order
//! (`switch-window--list' walks the frame from the top-left window, or
//! from the selected one when relative), the shortcut-key lists derived
//! from the quail keyboard layout, the label assignment
//! (`switch-window--enumerate'/`--label'), and the label buffer a
//! window displays while the prompt is up
//! (`switch-window--create-label-buffer').

use std::time::Duration;

use crate::{CachedMelpaOracle, SWITCH_WINDOW_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst sw1cc-test-upstream-tree
  "5bea09fa13b95375d95fd84ceaef60a503e39a21"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst sw1cc-test-manifest
  '(("switch-window-asciiart.el"
     . "35905fe87633215333e3b6916b67920189f7af0b84779db79c150c54c9f231b1")
    ("switch-window-mvborder.el"
     . "cb15602f6a4e6c812c5d5f9de255457bdfb15df0ce0dc71e8db9352a40ef6172")
    ("switch-window-pkg.el"
     . "67e470ab631ee5d542110e3922fcab79c25398509a26431d6a2c983c55da355e")
    ("switch-window.el"
     . "6351c81d070f0215efce5e0d8d45bcf9bb3e030c6c84cf91fdd018777df9c4d7"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun sw1cc-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "switch-window.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/switch-window.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed switch-window location: %S" located))
    (dolist (entry sw1cc-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed switch-window source: %S"
                   (car entry))))))
    (list :upstream-tree sw1cc-test-upstream-tree
          :feature (featurep 'switch-window)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'switch-window package-alist)))))))

(defun sw1cc-test-window-ids ()
  "Stable identity of each live window: buffer name and edges."
  (mapcar (lambda (window)
            (list (buffer-name (window-buffer window))
                  (window-edges window)))
          (window-list nil nil (frame-first-window))))

(defun sw1cc-test-reset ()
  "Restore the window tree and toggled settings."
  (delete-other-windows)
  (setq switch-window-relative nil
        switch-window-multiple-frames nil
        switch-window-shortcut-style 'quail
        switch-window-qwerty-shortcuts
        '("a" "s" "d" "f" "j" "k" "l")
        switch-window-minibuffer-shortcut nil
        switch-window-input-style 'minibuffer))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SWITCH_WINDOW_MELPA_PIN, "switch-window.el")
        .expect("prepare pinned switch-window source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn switch_window_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "switch_window_package_batch",
        "switch_window_parity",
        &cases,
    );
}
