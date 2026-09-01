use std::time::Duration;

use crate::{CachedMelpaOracle, SHRINK_PATH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SHRINK_PATH_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const SHRINK_PATH_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'shrink-path)

(defun neomacs-shrink-path-test-root (name)
  "Return NAME below this oracle process's deterministic sandbox."
  (file-name-as-directory
   (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-shrink-path-test-write (file contents)
  "Create FILE's parent and write CONTENTS exactly."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents)))

(defun neomacs-shrink-path-test-with-home (name workflow)
  "Run WORKFLOW with an isolated ROOT and HOME derived from NAME.
WORKFLOW receives ROOT and HOME.  Restore the exact previous HOME and remove
the complete fixture tree even when fixture creation or WORKFLOW signals."
  (let* ((root (neomacs-shrink-path-test-root name))
         (home (expand-file-name "home" root))
         (previous-home (getenv "HOME"))
         (abbreviated-home-dir nil))
    (when (file-directory-p root)
      (delete-directory root t))
    (unwind-protect
        (progn
          (make-directory home t)
          (setenv "HOME" home)
          (funcall workflow root home))
      (setenv "HOME" previous-home)
      (when (file-directory-p root)
        (delete-directory root t)))))

(defun neomacs-shrink-path-test-face-spans (text)
  "Return every face span in TEXT with zero-based bounds."
  (let ((position 0)
        result)
    (while (< position (length text))
      (let ((next (next-single-property-change
                   position 'face text (length text)))
            (face (get-text-property position 'face text)))
        (push (list position next
                    (substring-no-properties text position next)
                    face)
              result)
        (setq position next)))
    (nreverse result)))

(defun neomacs-shrink-path-test-relative (paths root)
  "Make absolute PATHS relative to ROOT while preserving list shape."
  (if (listp paths)
      (mapcar (lambda (path) (file-relative-name path root)) paths)
    (file-relative-name paths root)))
"##;

fn shrink_path_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SHRINK_PATH_MELPA_PIN, "shrink-path.el")
        .expect("prepare exact shallow Shrink Path source below ./tmp")
        .with_prelude(SHRINK_PATH_TEST_PRELUDE)
        .with_timeout(SHRINK_PATH_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Shrink Path parity test")
        .into()
}

fn assert_shrink_path_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        shrink_path_oracle(),
        &current_test_name(),
        "shrink_path_parity",
        cases,
    );
}

#[test]
fn shrink_path_package_batch() {
    assert_shrink_path_batch(&workflows::workflow_batch_cases());
}
