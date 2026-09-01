//! Practical parity for string-edit-at-point's escape-free string
//! editing.  The whole flow is batch-observable: the popup is just a
//! window split plus a buffer, so the workflows drive the real
//! `string-edit-at-point' -> edit -> `string-edit-at-point-conclude'
//! round trip and pin the re-escaped original buffer, plus the abort
//! path, the string detection helpers, and the escape transforms.

use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, STRING_EDIT_AT_POINT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst sep879-test-upstream-tree
  "56dce032374cbd78a8e95dbe7778c7f60edc82a3"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst sep879-test-manifest
  '(("string-edit-at-point-pkg.el"
     . "597a1d5dc8ba2b5c9be8ccfb0c9af855184d3786816b66508ebc677e5a3ca601")
    ("string-edit-at-point.el"
     . "bf36e3a3211fbb143fd2cde41ae9e0c3d589d6d020dedb170a27a79ea6c97e5c"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun sep879-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "string-edit-at-point.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/string-edit-at-point.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed string-edit-at-point location: %S"
             located))
    (dolist (entry sep879-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed string-edit source: %S"
                   (car entry))))))
    (list :upstream-tree sep879-test-upstream-tree
          :feature (featurep 'string-edit-at-point)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'string-edit-at-point package-alist))))
          :dash (package-version-join
                 (package-desc-version
                  (cadr (assq 'dash package-alist)))))))

(defun sep879-test-reset ()
  "Delete extra windows, kill popup and fixture buffers."
  (delete-other-windows)
  (dolist (buffer (buffer-list))
    (let ((name (buffer-name buffer)))
      (when (or (string-prefix-p "*string-edit-at-point" name)
                (string-prefix-p "sep-fixture" name))
        (unless (eq buffer (current-buffer))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (ignore-errors (kill-buffer buffer)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(STRING_EDIT_AT_POINT_MELPA_PIN, "string-edit-at-point.el")
        .expect("prepare pinned string-edit-at-point source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn string_edit_at_point_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "string_edit_at_point_package_batch",
        "string_edit_at_point_parity",
        &cases,
    );
}
