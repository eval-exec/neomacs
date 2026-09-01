use std::time::Duration;

use crate::{CachedMelpaOracle, GIT_GUTTER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GIT_GUTTER_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const GIT_GUTTER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'git-gutter)

(defun neomacs-git-gutter-test-git (root &rest arguments)
  "Run Git with ARGUMENTS in ROOT and return trimmed output."
  (let ((default-directory root))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (zerop status)
          (error "git %S failed (%s): %s" arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-git-gutter-test-write (file contents)
  "Write CONTENTS to FILE as UTF-8 Unix text."
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file file (insert contents))))

(defun neomacs-git-gutter-test-fixture (name)
  "Create a deterministic repository for NAME and return its paths."
  (let* ((root (file-name-as-directory
                (expand-file-name
                 (concat "git-gutter-" name)
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (file (expand-file-name "release.txt" root)))
    (when (file-exists-p root) (delete-directory root t))
    (make-directory root t)
    (neomacs-git-gutter-test-git root "init" "--quiet" "--initial-branch=main")
    (neomacs-git-gutter-test-git root "config" "core.hooksPath" "/dev/null")
    (neomacs-git-gutter-test-write
     file
     "# Release\nowner: platform\n\nsteps:\n- validate\n- publish\n\nnotes:\n- legacy\nend\n")
    (let ((process-environment (copy-sequence process-environment)))
      (setenv "GIT_AUTHOR_NAME" "Gutter Parity")
      (setenv "GIT_AUTHOR_EMAIL" "gutter@example.test")
      (setenv "GIT_COMMITTER_NAME" "Gutter Parity")
      (setenv "GIT_COMMITTER_EMAIL" "gutter@example.test")
      (setenv "GIT_AUTHOR_DATE" "2024-01-02T03:04:05+0000")
      (setenv "GIT_COMMITTER_DATE" "2024-01-02T03:04:05+0000")
      (neomacs-git-gutter-test-git root "add" "release.txt")
      (neomacs-git-gutter-test-git
       root "commit" "--quiet" "--no-gpg-sign" "-m" "Baseline release"))
    (neomacs-git-gutter-test-write
     file
     "# Release\nowner: delivery\n\nsteps:\n- validate\n- notify\n- publish\n\nnotes:\nend\n")
    (list :root root :file file)))

(defun neomacs-git-gutter-test-wait ()
  "Wait for the current buffer's asynchronous gutter update."
  (let ((limit 300)
        (process-name (git-gutter:diff-process-buffer (git-gutter:base-file))))
    (while (and (> limit 0)
                (or (get-buffer process-name) (not git-gutter:enabled)))
      (accept-process-output nil 0.02)
      (setq limit (1- limit)))
    (when (or (get-buffer process-name) (not git-gutter:enabled))
      (error "git-gutter update timed out"))))

(defun neomacs-git-gutter-test-hunks ()
  "Describe current hunks in source order."
  (mapcar
   (lambda (hunk)
     (list :type (git-gutter-hunk-type hunk)
           :start (git-gutter-hunk-start-line hunk)
           :end (git-gutter-hunk-end-line hunk)
           :content (git-gutter-hunk-content hunk)))
   git-gutter:diffinfos))

(defun neomacs-git-gutter-test-overlays ()
  "Describe rendered gutter overlays in buffer order."
  (mapcar
   (lambda (overlay)
     (let* ((before (overlay-get overlay 'before-string))
            (display (and before (get-text-property 0 'display before)))
            (rendered (and display (cadr display))))
       (list :line (line-number-at-pos (overlay-start overlay))
             :rendered (and rendered (substring-no-properties rendered))
             :faces (and rendered
                         (cl-loop for index below (length rendered)
                                  collect (get-text-property index 'face rendered))))))
   (sort (cl-remove-if-not
          (lambda (overlay) (overlay-get overlay 'git-gutter))
          (overlays-in (point-min) (point-max)))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun neomacs-git-gutter-test-run (name function)
  "Run FUNCTION in a visible visited repository file for NAME."
  (let ((process-environment (copy-sequence process-environment))
        fixture buffer result)
    (setenv "LC_ALL" "C")
    (setenv "LANG" "C")
    (setenv "TZ" "UTC")
    (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
    (setenv "GIT_CONFIG_NOSYSTEM" "1")
    (setq fixture (neomacs-git-gutter-test-fixture name)
          buffer (find-file-noselect (plist-get fixture :file)))
    (unwind-protect
        (setq result
              (save-window-excursion
                (set-window-buffer (selected-window) buffer)
                (with-current-buffer buffer
                  (let ((default-directory (plist-get fixture :root))
                        (git-gutter:verbosity 0)
                        (git-gutter:update-interval 0))
                    (funcall function fixture)))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when git-gutter-mode (git-gutter-mode -1))
          (set-buffer-modified-p nil))
        (kill-buffer buffer))
      (set-window-margins (selected-window) 0 nil)
      (when (and fixture (file-exists-p (plist-get fixture :root)))
        (delete-directory (plist-get fixture :root) t)))
    result))
"####;

fn git_gutter_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_GUTTER_MELPA_PIN, "git-gutter.el")
        .expect("prepare exact shallow Git Gutter source below ./tmp")
        .with_prelude(GIT_GUTTER_TEST_PRELUDE)
        .with_timeout(GIT_GUTTER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Git Gutter parity test")
        .into()
}

fn assert_git_gutter_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        git_gutter_oracle(),
        &current_test_name(),
        "git_gutter_parity",
        cases,
    );
}

#[test]
fn git_gutter_package_batch() {
    assert_git_gutter_batch(&workflows::workflow_batch_cases());
}
