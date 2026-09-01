use std::time::Duration;

use crate::{AC_CAPF_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_CAPF_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ac-capf bridges `completion-at-point-functions' into `auto-complete', so
/// every workflow needs a real buffer displayed in the selected window:
/// auto-complete anchors its prefix overlay and popup menu at point, and
/// `execute-kbd-macro' only reaches the selected window's buffer.  Nothing in
/// ac-capf, auto-complete or the completion machinery is stubbed; the fixtures
/// are ordinary capfs of the kind a major mode or a user configuration
/// installs.
const AC_CAPF_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'thingatpt)

(defun ac-capf-test-path (name)
  "Return the absolute sandbox path of NAME."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ac-capf-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (ac-capf-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun ac-capf-test-read (name)
  "Return the exact contents of sandbox file NAME."
  (with-temp-buffer
    (insert-file-contents (ac-capf-test-path name))
    (buffer-string)))

(defmacro ac-capf-test-with-live-buffer (buffer-form &rest body)
  "Run BODY in BUFFER-FORM's buffer while the selected window displays it."
  `(let ((buffer ,buffer-form))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           ,@body)
       (with-current-buffer buffer
         (ignore-errors (ac-abort))
         (set-buffer-modified-p nil))
       (kill-buffer buffer))))

(defun ac-capf-test-scratch (mode text)
  "Return a fresh MODE buffer holding TEXT with point after it.
`ac-sources' starts out empty so that only the source ac-capf installs can
contribute candidates."
  (let ((buffer (generate-new-buffer "*ac-capf-workflow*")))
    (with-current-buffer buffer
      (funcall mode)
      (setq-local ac-sources nil)
      (insert text))
    buffer))

(defun ac-capf-test-visit (name text)
  "Visit sandbox file NAME holding TEXT with point at the end."
  (let ((buffer (find-file-noselect (ac-capf-test-write name text))))
    (with-current-buffer buffer
      (setq-local ac-sources nil)
      (goto-char (point-max)))
    buffer))

(defun ac-capf-test-menu ()
  "Report every candidate auto-complete built, in menu order."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (popup-item-symbol candidate)
                  (popup-item-document candidate)
                  (popup-item-summary candidate)
                  (text-properties-at 0 candidate)))
          ac-candidates))

(defun ac-capf-test-session ()
  "Report the completion state auto-complete is holding."
  (list :prefix ac-prefix
        :prefix-start (and ac-point (- ac-point (point-min)))
        :common (and (stringp ac-common-part)
                     (substring-no-properties ac-common-part))
        :menu-live (and (ac-menu-live-p) t)
        :selected (and (ac-menu-live-p)
                       (substring-no-properties (popup-selected-item ac-menu)))
        :completing ac-completing))

(defun ac-capf-test-buffer-state ()
  "Report the editing state the user can see."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (- (point) (point-min))
        :mode major-mode
        :auto-complete auto-complete-mode
        :sources ac-sources
        :capfs completion-at-point-functions))
"##;

fn ac_capf_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_CAPF_MELPA_PIN, "ac-capf.el")
        .expect("prepare pinned ac-capf source below ./tmp")
        .with_prelude(AC_CAPF_TEST_PRELUDE)
        .with_timeout(AC_CAPF_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-capf parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_capf_parity` cases (2a).
pub(crate) fn assert_ac_capf_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_capf_oracle(), &name, "ac_capf_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_capf_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_capf_batch(&cases);
}

// END generated package batch tests
