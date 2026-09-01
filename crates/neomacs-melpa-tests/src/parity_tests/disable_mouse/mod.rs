//! Practical parity for disable-mouse.  The package binds every mouse
//! event name to a handler in two keymaps; everything is data:
//! the generated binding lists, the maps' coverage, the buffer-local
//! mode, and the handler's delegation to `disable-mouse-command'.

use std::time::Duration;

use crate::{CachedMelpaOracle, DISABLE_MOUSE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst dm93a-test-upstream-tree
  "e6555acae15270b6a2e54a32d5c2f217d67e16c3"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst dm93a-test-manifest
  '(("disable-mouse-pkg.el"
     . "d4429aeffdd21621d21f7b9fa51af1ae533e56d4ba9750fe2131e923a56c6600")
    ("disable-mouse.el"
     . "d88b34043141e96b5e9806f83fd66633736f92456a541fb2628da21713454aff"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun dm93a-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "disable-mouse.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/disable-mouse.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed disable-mouse location: %S" located))
    (dolist (entry dm93a-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed disable-mouse source: %S"
                   (car entry))))))
    (list :upstream-tree dm93a-test-upstream-tree
          :feature (featurep 'disable-mouse)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'disable-mouse package-alist)))))))

(defun dm93a-test-reset ()
  "Turn both modes off."
  (when (bound-and-true-p disable-mouse-mode)
    (disable-mouse-mode -1))
  (when (bound-and-true-p disable-mouse-global-mode)
    (disable-mouse-global-mode -1)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DISABLE_MOUSE_MELPA_PIN, "disable-mouse.el")
        .expect("prepare pinned disable-mouse source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn disable_mouse_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "disable_mouse_package_batch",
        "disable_mouse_parity",
        &cases,
    );
}
