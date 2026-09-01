use std::time::Duration;

use crate::{CachedMelpaOracle, ELPY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ELPY_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const ELPY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'elpy)

(defun neomacs-elpy-test-root (name)
  "Return NAME below this oracle process's deterministic sandbox."
  (file-name-as-directory
   (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-elpy-test-write (file contents)
  "Create FILE's parent directory and write CONTENTS exactly."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents)))

(defun neomacs-elpy-test-with-root (name workflow)
  "Run WORKFLOW with a clean fixture ROOT derived from NAME."
  (let ((root (neomacs-elpy-test-root name)))
    (when (file-directory-p root)
      (delete-directory root t))
    (unwind-protect
        (progn
          (make-directory root t)
          (funcall workflow root))
      (when (file-directory-p root)
        (delete-directory root t)))))

(defun neomacs-elpy-test-relative (path root)
  "Return PATH relative to ROOT, preserving nil."
  (and path (file-relative-name path root)))

(defun neomacs-elpy-test-buffer-state ()
  "Return exact text, point, line, column, mark, and active-region state."
  (list :text (buffer-string)
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :mark (mark t)
        :active mark-active
        :region (and mark-active
                     (buffer-substring-no-properties
                      (region-beginning) (region-end)))))

(defun neomacs-elpy-test-overlay-ranges (overlays)
  "Return OVERLAYS as sorted start/end/text rows."
  (sort (mapcar (lambda (overlay)
                  (list (overlay-start overlay)
                        (overlay-end overlay)
                        (buffer-substring-no-properties
                         (overlay-start overlay)
                         (overlay-end overlay))))
                overlays)
        (lambda (left right) (< (car left) (car right)))))
"##;

fn elpy_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ELPY_MELPA_PIN, "elpy.el")
        .expect("prepare exact shallow Elpy source below ./tmp")
        .with_prelude(ELPY_TEST_PRELUDE)
        .with_timeout(ELPY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Elpy parity test")
        .into()
}

fn assert_elpy_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(elpy_oracle(), &current_test_name(), "elpy_parity", cases);
}

#[test]
fn elpy_package_batch() {
    assert_elpy_batch(&workflows::workflow_batch_cases());
}
