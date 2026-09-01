use std::time::Duration;

use crate::{ACE_JUMP_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_JUMP_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-jump-mode is driven entirely by keys: a command paints one labelled
/// overlay per candidate, installs an `overriding-local-map', and the next key
/// the command loop reads decides where point lands.  Every workflow therefore
/// types real keys with `execute-kbd-macro' into a buffer that is displayed in
/// the selected window, and observes the session through `post-command-hook',
/// which sees the labels while they are still on screen.  Nothing in the
/// package is stubbed.
const ACE_JUMP_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst aj-test-prose
  (concat
   "The quick brown fox jumps over the lazy dog.\n"
   "Pack my box with five dozen liquor jugs.\n"
   "How vexingly quick daft zebras jump!\n"
   "Quiet quails quibble by the Quarry gate.\n"))

(defmacro aj-test-with-live-buffer (&rest body)
  "Run BODY in a real, window-displayed buffer so typed keys reach it."
  `(let ((buffer (generate-new-buffer "*ace-jump-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (global-set-key (kbd "C-c SPC") 'ace-jump-mode)
           (global-set-key (kbd "C-x SPC") 'ace-jump-mode-pop-mark)
           ,@body)
       (kill-buffer buffer))))

(defun aj-test-overlays ()
  "Describe every overlay ace-jump put in the current buffer, ordered."
  (sort
   (mapcar
    (lambda (overlay)
      (list (overlay-start overlay)
            (overlay-end overlay)
            (overlay-get overlay 'display)
            (overlay-get overlay 'face)))
    (overlays-in (point-min) (point-max)))
   (lambda (a b) (or (< (car a) (car b))
                     (and (= (car a) (car b)) (< (cadr a) (cadr b)))))))

(defun aj-test-workflow-buffers ()
  (sort (cl-remove-if-not
         (lambda (buffer) (string-prefix-p "*ace-jump-" (buffer-name buffer)))
         (buffer-list))
        (lambda (a b) (string< (buffer-name a) (buffer-name b)))))

(defvar aj-test-labels nil)

(defun aj-test-capture-labels ()
  "Record the labels of every workflow buffer.
Meant for `ace-jump-mode-before-jump-hook', which runs while the
overlays of every visual area are still on screen."
  (setq aj-test-labels
        (mapcar (lambda (buffer)
                  (cons (buffer-name buffer)
                        (with-current-buffer buffer (aj-test-overlays))))
                (aj-test-workflow-buffers))))

(defun aj-test-mark-ring ()
  "Describe `ace-jump-mode-mark-ring' as (OFFSET . BUFFER-NAME) entries."
  (mapcar (lambda (position)
            (cons (aj-position-offset position)
                  (buffer-name (aj-position-buffer position))))
          ace-jump-mode-mark-ring))

(defvar aj-test-trace nil)

(defun aj-test-record ()
  "Record the ace-jump state after a command; for `post-command-hook'."
  (push (list this-command
              (key-description (this-command-keys))
              (point)
              ace-jump-current-mode
              ace-jump-mode
              (aj-test-overlays))
        aj-test-trace))

(defmacro aj-test-tracing (&rest body)
  "Run BODY recording the ace-jump state after every command it executes."
  `(let ((aj-test-trace nil)
         (ace-jump-mode-mark-ring nil))
     (add-hook 'post-command-hook #'aj-test-record)
     (unwind-protect
         (let ((result (progn ,@body)))
           (cons (nreverse aj-test-trace) result))
       (remove-hook 'post-command-hook #'aj-test-record))))

(defun aj-test-last-message ()
  (with-current-buffer (get-buffer-create "*Messages*")
    (car (last (split-string (buffer-string) "\n" t)))))
"##;

fn ace_jump_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_JUMP_MODE_MELPA_PIN, "ace-jump-mode.el")
        .expect("prepare pinned ace-jump-mode source below ./tmp")
        .with_prelude(ACE_JUMP_MODE_TEST_PRELUDE)
        .with_timeout(ACE_JUMP_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-jump-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_jump_mode_parity` cases (2a).
pub(crate) fn assert_ace_jump_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_jump_mode_oracle(), &name, "ace_jump_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_jump_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_jump_mode_batch(&cases);
}

// END generated package batch tests
