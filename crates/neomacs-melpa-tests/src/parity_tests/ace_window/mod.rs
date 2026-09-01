use std::time::Duration;

use crate::{ACE_WINDOW_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_WINDOW_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-window's user value is window state, so every workflow builds a real
/// multi-window layout with distinct buffers and then presses the label keys
/// through `execute-kbd-macro'.  avy reads those keys with `read-key', which a
/// keyboard macro feeds, so nothing about the selection is stubbed.
/// `aw-test-layout' renders the whole frame in `aw-window<' order -- the same
/// order ace-window numbers its labels in -- so the snapshots read as a
/// picture of the frame.
const ACE_WINDOW_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

;; `aw-window-list' drops every window whose frame lives on the terminal named
;; "initial_terminal".  That is exactly the terminal a --batch job runs on, so
;; without this the package would see zero candidate windows and the workflows
;; would observe the absence of a terminal rather than ace-window.  Answer the
;; terminal's name the way a real session answers it, and nothing else.
(fset 'terminal-name
      (lambda (&optional _terminal)
        "/dev/pts/0"))

(defvar aw-test-buffers nil)

(defun aw-test-layout ()
  "Render every window in `aw-window<' order, which is also label order."
  (let ((selected (selected-window)))
    (mapcar
     (lambda (w)
       (list :edges (window-edges w)
             :buffer (buffer-name (window-buffer w))
             :point (window-point w)
             :selected (and (eq w selected) t)))
     (sort (window-list nil 'no-minibuffer) #'aw-window<))))

(defun aw-test-labels ()
  "The label a user would press for each window ace-window is offering."
  (cl-mapcar
   (lambda (key w)
     (list :key (char-to-string key)
           :edges (window-edges w)
           :buffer (buffer-name (window-buffer w))))
   aw-keys
   (aw-window-list)))

(defun aw-test-buffer (name text)
  (let ((buffer (generate-new-buffer name)))
    (push buffer aw-test-buffers)
    (with-current-buffer buffer
      (insert text)
      (goto-char (point-min))
      (set-buffer-modified-p nil))
    buffer))

(defun aw-test-kill-buffers ()
  (dolist (buffer aw-test-buffers)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (setq aw-test-buffers nil))

(defun aw-test-session ()
  "Build the three-window editing session every workflow starts from.

  +---------------+-----------+
  | ledger.el     |           |
  +---------------+ notes.org |
  | *build-log*   |           |
  +---------------+-----------+"
  (aw-test-kill-buffers)
  (delete-other-windows)
  (let* ((ledger
          (aw-test-buffer
           "ledger.el"
           "(defun settle (invoice)\n  (message \"settled %s\" invoice))\n"))
         (log
          (aw-test-buffer
           "*build-log*"
           "make check\nsrc/settle.el:12:7: error: invoice mismatch\n"))
         (notes
          (aw-test-buffer
           "notes.org"
           "* Release\n** TODO cut the branch\n")))
    (set-window-buffer (selected-window) ledger)
    (let* ((left (selected-window))
           (right (split-window-right))
           (bottom (split-window-below nil left)))
      (set-window-buffer right notes)
      (set-window-buffer bottom log)
      (select-window left)
      (list left bottom right))))

(defun aw-test-cleanup ()
  (when ace-window-display-mode
    (ace-window-display-mode -1))
  (aw-test-kill-buffers)
  (delete-other-windows))
"####;

fn ace_window_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_WINDOW_MELPA_PIN, "ace-window.el")
        .expect("prepare pinned ace-window source below ./tmp")
        .with_prelude(ACE_WINDOW_TEST_PRELUDE)
        .with_timeout(ACE_WINDOW_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-window parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_window_parity` cases (2a).
pub(crate) fn assert_ace_window_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_window_oracle(), &name, "ace_window_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_window_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_window_batch(&cases);
}

// END generated package batch tests
