use std::time::Duration;

use crate::{CachedMelpaOracle, FLYCHECK_MELPA_PIN, Z3_MODE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const Z3_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const Z3_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; z3-mode captures this value while defining its Flycheck checker.
(setq z3-solver-cmd "z3-parity")

(defun neomacs-melpa-z3-mode--write-executable (path body)
  (with-temp-file path
    (insert "#!/bin/sh\nset -eu\n")
    (insert body))
  (set-file-modes path #o700)
  path)

(defun neomacs-melpa-z3-mode--file-string (path)
  (when (file-exists-p path)
    (with-temp-buffer
      (insert-file-contents path)
      (buffer-string))))

(defun neomacs-melpa-z3-mode--face-runs ()
  (font-lock-ensure)
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push
           (list
            (buffer-substring-no-properties position next)
            face position next)
           runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-melpa-z3-mode--face-segments (needle)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (let ((start (match-beginning 0))
          (end (match-end 0))
          segments)
      (let ((position start))
        (while (< position end)
          (let ((next
                 (min
                  end
                  (next-single-property-change position 'face nil end))))
            (push
             (list
              (buffer-substring-no-properties position next)
              (get-text-property position 'face)
              (- position start)
              (- next start))
             segments)
            (setq position next))))
      (list needle start end (nreverse segments)))))

(defun neomacs-melpa-z3-mode--wait-for-flycheck ()
  (let ((finished nil)
        (rounds 0))
    (add-hook
     'flycheck-after-syntax-check-hook
     (lambda () (setq finished t)) nil t)
    (flycheck-buffer)
    (while (and (not finished) (< rounds 600))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds)))
    (unless finished
      (error "Timed out waiting for z3-mode Flycheck: %S"
             flycheck-last-status-change))))

(defun neomacs-melpa-z3-mode--diagnostics ()
  (mapcar
   (lambda (diagnostic)
     (list
      (flycheck-error-line diagnostic)
      (flycheck-error-column diagnostic)
      (flycheck-error-level diagnostic)
      (flycheck-error-checker diagnostic)
      (flycheck-error-message diagnostic)))
   flycheck-current-errors))
"##;

fn z3_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(Z3_MODE_MELPA_PIN, "z3-mode.el")
        .expect("prepare pinned z3-mode source below ./tmp")
        .with_melpa_dependency(FLYCHECK_MELPA_PIN)
        .expect("prepare pinned Flycheck dependency below ./tmp")
        .with_prelude(Z3_MODE_TEST_PRELUDE)
        .with_timeout(Z3_MODE_TEST_TIMEOUT)
}

#[test]
fn z3_mode_package_batch() {
    assert_oracle_batch_cases(
        z3_mode_oracle(),
        "z3_mode_package_batch",
        "z3_mode_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
