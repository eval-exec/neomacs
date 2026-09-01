use std::time::Duration;

use crate::{ACE_JUMP_BUFFER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_JUMP_BUFFER_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-jump-buffer shows a real `bs' menu and reads a real avy key, so every
/// workflow types the command's key binding and the jump key through
/// `execute-kbd-macro'.  Keys only reach the buffer of the *selected window*,
/// which is why the workspace displays a buffer instead of merely making it
/// current.
///
/// The menu is torn down by the jump itself, so it is snapshotted from
/// `avy-translate-char-function' - avy's documented hook for rewriting the key
/// the user pressed.  The observer returns the character unchanged, so avy's
/// real overlays, real key reading and real dispatch all still happen; nothing
/// in ace-jump-buffer, avy or bs is stubbed.
const ACE_JUMP_BUFFER_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'bs)

;; A realistic working set: prose, code, Unicode names and a name with a
;; space, each in a distinct major mode so the "same-mode" filter has
;; something to reject.
(defconst ajb-test-buffer-specs
  '(("notes.org" text-mode "* Roadmap\n** Ship the résumé exporter\n")
    ("project plan.md" text-mode "# Plan\n\n- [ ] 日本語 locale review\n")
    ("server.py" prog-mode "def main():\n    return 0\n")
    ("résumé.tex" text-mode "\\documentclass{article}\n")))

(defmacro ajb-test-with-workspace (&rest body)
  "Create the workspace buffers, display the first one, run BODY, clean up."
  (declare (indent 0))
  `(let ((buffers
          (mapcar (lambda (spec)
                    (let ((buffer (generate-new-buffer (car spec))))
                      (with-current-buffer buffer
                        (funcall (nth 1 spec))
                        (insert (nth 2 spec)))
                      buffer))
                  ajb-test-buffer-specs)))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) (car buffers))
           (set-buffer (car buffers))
           (global-set-key (kbd "C-c j") #'ace-jump-buffer)
           (global-set-key (kbd "C-c o") #'ace-jump-buffer-other-window)
           (global-set-key (kbd "C-c 1") #'ace-jump-buffer-in-one-window)
           (global-set-key (kbd "C-c c") #'ace-jump-buffer-with-configuration)
           ,@body)
       (dolist (buffer buffers)
         (when (buffer-live-p buffer) (kill-buffer buffer)))
       (when (get-buffer "*buffer-selection*")
         (kill-buffer "*buffer-selection*")))))

(defun ajb-test-menu-snapshot ()
  "Snapshot the `*buffer-selection*' menu while avy's overlays are still up."
  (with-current-buffer "*buffer-selection*"
    (list :mode major-mode
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :window-buffer (buffer-name (window-buffer (selected-window)))
          :header-lines bs-header-lines-length
          :max-height bs-max-window-height
          :sort bs-buffer-sort-function
          :overlays
          (mapcar (lambda (overlay)
                    (list (line-number-at-pos (overlay-start overlay))
                          (overlay-start overlay)
                          (overlay-end overlay)
                          (overlay-get overlay 'display)
                          (buffer-name
                           (window-buffer (overlay-get overlay 'window)))))
                  (sort (overlays-in (point-min) (point-max))
                        (lambda (a b) (< (overlay-start a) (overlay-start b))))))))

(defun ajb-test-labels (snapshot)
  "Return the avy label assigned to each menu line as (LINE . LABEL)."
  (mapcar (lambda (overlay)
            (cons (nth 0 overlay)
                  (substring-no-properties (nth 3 overlay))))
          (plist-get snapshot :overlays)))

(defun ajb-test-windows ()
  "Return the buffer shown in every live window, in window order."
  (mapcar (lambda (window) (buffer-name (window-buffer window)))
          (window-list nil 'never)))

(defun ajb-test-visible-buffers ()
  "Return `buffer-list' without Emacs' internal space-prefixed buffers."
  (seq-remove (lambda (name) (string-prefix-p " " name))
              (mapcar #'buffer-name (buffer-list))))
"###;

fn ace_jump_buffer_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_JUMP_BUFFER_MELPA_PIN, "ace-jump-buffer.el")
        .expect("prepare pinned ace-jump-buffer source below ./tmp")
        .with_prelude(ACE_JUMP_BUFFER_TEST_PRELUDE)
        .with_timeout(ACE_JUMP_BUFFER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-jump-buffer parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_jump_buffer_parity` cases (2a).
pub(crate) fn assert_ace_jump_buffer_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ace_jump_buffer_oracle(),
        &name,
        "ace_jump_buffer_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ace_jump_buffer_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_jump_buffer_batch(&cases);
}

// END generated package batch tests
