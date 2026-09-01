use std::time::Duration;

use crate::{ACE_JUMP_ZAP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_JUMP_ZAP_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-jump-zap is zap-to-char driven by ace-jump labels: the command hands
/// over to `ace-jump-char-mode', and the key the user presses next both picks
/// the target and triggers the kill, through `ace-jump-mode-before-jump-hook'
/// and `ace-jump-mode-end-hook'.  Every workflow therefore types real keys with
/// `execute-kbd-macro' into a buffer displayed in the selected window, and
/// watches the session from `post-command-hook', which sees the labels while
/// they are still on screen.  Nothing is stubbed, and the package's own hooks
/// and advice stay installed.
const ACE_JUMP_ZAP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst ajz-test-recipe
  (concat
   "Pasta with tomato sauce\n"
   "  - 400 g tomatoes, peeled\n"
   "  - 2 cloves of garlic\n"
   "  - olive oil, salt, pepper\n"
   "Simmer for 20 minutes, then serve.\n"))

(defmacro ajz-test-with-live-buffer (&rest body)
  "Run BODY in a real, window-displayed buffer so typed keys reach it."
  `(let ((buffer (generate-new-buffer "*ace-jump-zap-workflow*"))
         ;; A previous batch case may finish with an interactive command.
         ;; Give each workflow the same command-loop baseline it has in a
         ;; fresh editor while retaining one process for package setup.
         (this-command nil))
     (unwind-protect
         (progn
           (ajz/reset)
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (global-set-key (kbd "M-z") 'ace-jump-zap-to-char)
           (global-set-key (kbd "M-Z") 'ace-jump-zap-up-to-char)
           (global-set-key (kbd "C-c z") 'ace-jump-zap-to-char-dwim)
           (global-set-key (kbd "C-c Z") 'ace-jump-zap-up-to-char-dwim)
           ,@body)
       (ajz/reset)
       (kill-buffer buffer))))

(defun ajz-test-labels ()
  "Return (LABEL . POSITION) for every zap candidate, ordered by label."
  (sort
   (delq nil
         (mapcar (lambda (overlay)
                   (let ((display (overlay-get overlay 'display)))
                     (and (overlay-get overlay 'aj-data)
                          (cons (substring display 0 1) (overlay-start overlay)))))
                 (overlays-in (point-min) (point-max))))
   (lambda (a b) (string< (car a) (car b)))))

(defvar ajz-test-captured-labels nil)

(defun ajz-test-capture-labels ()
  "Record the labels while they are still on screen.
Meant to be added to `ace-jump-mode-before-jump-hook' beside the
package's own `ajz/maybe-zap-start'."
  (setq ajz-test-captured-labels (ajz-test-labels)))

(defvar ajz-test-trace nil)

(defun ajz-test-record ()
  "Record the zap state after a command; for `post-command-hook'."
  (push (list this-command
              (key-description (this-command-keys))
              (point)
              ajz/zapping
              ajz/to-char
              ajz/saved-point
              (ajz-test-labels))
        ajz-test-trace))

(defmacro ajz-test-tracing (&rest body)
  "Run BODY recording the zap state after every command it executes."
  `(let ((ajz-test-trace nil))
     (add-hook 'post-command-hook #'ajz-test-record)
     (unwind-protect
         (let ((result (progn ,@body)))
           (cons (nreverse ajz-test-trace) result))
       (remove-hook 'post-command-hook #'ajz-test-record))))

(defun ajz-test-state ()
  "Everything a zap can change, in one list."
  (list (buffer-string)
        (point)
        (mark t)
        mark-active
        ajz/zapping
        ajz/to-char
        ajz/saved-point
        (length (overlays-in (point-min) (point-max)))
        kill-ring))
"##;

fn ace_jump_zap_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_JUMP_ZAP_MELPA_PIN, "ace-jump-zap.el")
        .expect("prepare pinned ace-jump-zap source below ./tmp")
        .with_prelude(ACE_JUMP_ZAP_TEST_PRELUDE)
        .with_timeout(ACE_JUMP_ZAP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-jump-zap parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_jump_zap_parity` cases (2a).
pub(crate) fn assert_ace_jump_zap_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_jump_zap_oracle(), &name, "ace_jump_zap_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_jump_zap_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_jump_zap_batch(&cases);
}

// END generated package batch tests
