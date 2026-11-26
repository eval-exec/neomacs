use std::time::Duration;

use crate::{APP_MONOCHROME_THEMES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APP_MONOCHROME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const APP_MONOCHROME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-app-monochrome-test-primary-face (value)
  (cond
   ((and (symbolp value) (facep value)) value)
   ((listp value)
    (cl-find-if
     (lambda (candidate)
       (and (symbolp candidate) (facep candidate)))
     value))))

(defun neomacs-app-monochrome-test-after-compilation-header ()
  "Move past a compilation start banner.
The banner clock can contain the same text as a diagnostic (`8:2`
inside `11:28:21`). Point stays at `point-min` when there is no banner,
so a source buffer is unchanged. Searching from here lands on the
diagnostic, ahead of a finish line that can repeat the clock."
  (goto-char (point-min))
  ;; Point rests at the end of the banner, after its clock. A failed
  ;; search leaves point at `point-min`.
  (re-search-forward "^Compilation started at .*" nil t))

(defun neomacs-app-monochrome-test-face-at
    (needle attributes &optional occurrence)
  (save-excursion
    (neomacs-app-monochrome-test-after-compilation-header)
    (dotimes (_ (or occurrence 1))
      (search-forward needle))
    (let* ((position (match-beginning 0))
           (text-face
            (or
             (get-char-property position 'face)
             (get-char-property position 'font-lock-face)))
           (face
            (neomacs-app-monochrome-test-primary-face text-face)))
      (list
       needle
       (copy-tree text-face)
       (and
        face
        (mapcar
         (lambda (attribute)
           (list
            attribute
            (copy-tree
             (face-attribute face attribute nil 'default))))
         attributes))))))

(defun neomacs-app-monochrome-test-palette (requests)
  (mapcar
   (lambda (request)
     (list
      (car request)
      (cadr request)
      (copy-tree
       (face-attribute
        (car request)
        (cadr request)
        nil
        'default))))
   requests))

(defun neomacs-app-monochrome-test-line ()
  (buffer-substring-no-properties
   (line-beginning-position)
   (line-end-position)))

(defun neomacs-app-monochrome-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun neomacs-app-monochrome-test-cleanup (root)
  (dolist
      (theme
       '(app-monochrome-themes-light-theme
         app-monochrome-themes-dark-theme))
    (when
        (custom-theme-enabled-p theme)
      (disable-theme theme)))
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer))
          (directory
           (with-current-buffer buffer
             (and
              (derived-mode-p 'dired-mode)
              default-directory)))
          (name (buffer-name buffer)))
      (when
          (or
           (and file (string-prefix-p root file))
           (and directory (string-prefix-p root directory))
           (string-prefix-p "*app-monochrome-" name))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (ignore-errors
          (kill-buffer buffer)))))
  (when
      (file-exists-p root)
    (delete-directory root t)))

(defun neomacs-app-monochrome-test-complete-compilation (buffer)
  "Wait for BUFFER's process, force one complete compilation parse, fontify.
`compilation-filter' parses output as it arrives and `font-lock-ensure'
reads whatever `compilation--parsed' happens to cover; if a previous
parse ran while a diagnostic line was still partly inserted, the
line-number face can be absent even though the text is present (both
editors can lose it under load).  Wait for the exit annotation so the
buffer is complete, flush the incremental parse marker, fontify once,
and then assert the contract this fixture snapshots: every diagnostic
line carries its parsed face.  A violation signals instead of returning
a silently incomplete snapshot."
  (with-current-buffer buffer
    (let ((process (get-buffer-process buffer))
          (deadline (+ (float-time) 30.0)))
      (while (and process (process-live-p process)
                  (< (float-time) deadline))
        (accept-process-output process 0.05))
      (when (and process (process-live-p process))
        (error "compilation process did not finish")))
    ;; `compilation-handle-exit' writes the terminal annotation last.
    (let ((deadline (+ (float-time) 30.0)))
      (while (and (< (float-time) deadline)
                  (not (save-excursion
                         (goto-char (point-max))
                         (re-search-backward
                          "^Compilation \\(finished\\|exited abnormally\\)"
                          nil t))))
        (accept-process-output nil 0.05)))
    (compilation--flush-parse (point-min) (point-max))
    (font-lock-ensure)
    (dolist (needle '("4:8" "8:2"))
      (save-excursion
        (neomacs-app-monochrome-test-after-compilation-header)
        (unless (search-forward needle nil t)
          (error "compilation diagnostic %s is missing" needle))
        (unless (get-text-property (match-beginning 0) 'font-lock-face)
          (error "compilation diagnostic %s has no parsed face" needle))))))
"####;

fn app_monochrome_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APP_MONOCHROME_THEMES_MELPA_PIN, "app-monochrome-themes.el")
        .expect("prepare pinned app-monochrome-themes source below ./tmp")
        .with_prelude(APP_MONOCHROME_TEST_PRELUDE)
        .with_timeout(APP_MONOCHROME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed app-monochrome-themes parity test")
        .into()
}

/// Multi-probe batch for `assert_app_monochrome_parity` cases (2a).
pub(crate) fn assert_app_monochrome_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        app_monochrome_oracle(),
        &name,
        "app_monochrome_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn app_monochrome_compilation_header_clock_is_not_a_diagnostic() {
    assert_app_monochrome_batch(&[workflows::app_monochrome_header_clock_case()]);
}

#[test]
fn app_monochrome_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_app_monochrome_batch(&cases);
}

// END generated package batch tests
