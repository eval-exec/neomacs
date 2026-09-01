//! Practical parity for tao-theme.  The package generates the golden-ratio
//! greyscale palette (dark yin and light yang variants, with a sepia
//! option) and defines two themes from it.  Everything is pure Elisp:
//! the suite asserts the exact scales, palettes, and the face attributes
//! the themes apply.

use std::time::Duration;

use crate::{CachedMelpaOracle, TAO_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

;; Provenance: pinned upstream 33c0d44048afe444e7a8aee30fbc101a00453799.
(defconst tao--test-upstream-tree
  "f7bed42b2c6c5e892f7f3bc83da6abc5a3ca7725"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst tao--test-manifest
  '(("tao-theme.el" . "4d66d185c52e429e814f98265ee34b314bf0ea21a9c0bd020ef406e9f37c15a6"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun tao--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "tao-theme.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/tao-theme.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed tao-theme location: %S" located))
    (dolist (entry tao--test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed tao-theme source: %S"
                   (car entry))))))
    (list :upstream-tree tao--test-upstream-tree
          :feature (featurep 'tao-theme)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'tao-theme package-alist)))))))

(defun tao--test-faces ()
  "Read the attributes of representative faces."
  (list :default-fg (face-attribute 'default :foreground)
        :default-bg (face-attribute 'default :background)
        :link-fg (face-attribute 'link :foreground)
        :show-paren-match (face-attribute 'show-paren-match :foreground)
        :font-lock-keyword (face-attribute 'font-lock-keyword-face :foreground)
        :font-lock-string (face-attribute 'font-lock-string-face :foreground)
        :font-lock-comment (face-attribute 'font-lock-comment-face :foreground)
        :mode-line-fg (face-attribute 'mode-line :foreground)
        :mode-line-bg (face-attribute 'mode-line :background)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TAO_THEME_MELPA_PIN, "tao-theme.el")
        .expect("prepare pinned tao-theme source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn tao_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "tao_theme_package_batch",
        "tao_theme_parity",
        &cases,
    );
}
