use std::time::Duration;

use crate::{ACE_MC_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_MC_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ace-mc adds and removes multiple-cursors fake cursors at ace-jump targets.
/// Nothing is stubbed in these workflows: ace-jump really searches the window,
/// really builds its label overlays and its `overriding-local-map`,
/// multiple-cursors really creates the fake cursors and really replays typed
/// commands for each of them, and every key is delivered through
/// `execute-kbd-macro`.  Keys only reach the buffer of the selected window, so
/// the work buffer is displayed rather than merely current.
const ACE_MC_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'multiple-cursors)
(require 'ace-jump-mode)

(setq make-backup-files nil
      create-lockfiles nil
      ace-jump-mode-gray-background nil)

(defmacro ace-mc-test-in-buffer (text &rest body)
  "Run BODY in a window-displayed buffer holding TEXT, with a clean mc state."
  `(let ((buffer (generate-new-buffer "*ace-mc-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (fundamental-mode)
           (insert ,text)
           (goto-char (point-min))
           (global-set-key (kbd "C-c m") #'ace-mc-add-multiple-cursors)
           (global-set-key (kbd "C-c s") #'ace-mc-add-single-cursor)
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (when multiple-cursors-mode (multiple-cursors-mode -1))
           (mc/remove-fake-cursors)
           (set-buffer-modified-p nil))
         (kill-buffer buffer)))))

(defun ace-mc-test-state ()
  "Everything a workflow wants to know after driving keys."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :cursors (sort (mapcar #'overlay-start (mc/all-fake-cursors)) #'<)
        :num (mc/num-cursors)
        :mc-mode (and multiple-cursors-mode t)
        :ace-mode ace-jump-current-mode
        :ace-marking ace-mc-marking
        :overriding (and overriding-local-map t)))

(defun ace-mc-test-labels ()
  "The label characters ace-jump is currently showing, with their positions."
  (sort (delq nil
              (mapcar (lambda (overlay)
                        (and (overlay-get overlay 'aj-data)
                             (cons (overlay-start overlay)
                                   (overlay-get overlay 'display))))
                      (overlays-in (point-min) (point-max))))
        (lambda (a b) (< (car a) (car b)))))

(defvar ace-mc-test-recorded-labels nil
  "Labels ace-jump was showing at each jump, newest first.")

(defun ace-mc-test-record-labels ()
  "Snapshot the live label overlays; runs from `ace-jump-mode-before-jump-hook'."
  (push (ace-mc-test-labels) ace-mc-test-recorded-labels))
"##;

fn ace_mc_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_MC_MELPA_PIN, "ace-mc.el")
        .expect("prepare pinned ace-mc source below ./tmp")
        .with_prelude(ACE_MC_TEST_PRELUDE)
        .with_timeout(ACE_MC_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ace-mc parity test").into()
}

/// Multi-probe batch for `assert_ace_mc_parity` cases (2a).
pub(crate) fn assert_ace_mc_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_mc_oracle(), &name, "ace_mc_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_mc_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_mc_batch(&cases);
}

// END generated package batch tests
