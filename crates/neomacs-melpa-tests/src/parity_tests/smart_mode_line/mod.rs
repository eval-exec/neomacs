use std::time::Duration;

use crate::{CachedMelpaOracle, RICH_MINORITY_MELPA_PIN, SMART_MODE_LINE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const SMART_MODE_LINE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Every workflow enters the documented way: `sml/setup', which installs
/// the smart mode-line format, activates rich-minority for the minor-mode
/// list, applies the requested theme, and adds the identification and
/// position hooks.  A batch editor never redisplays, but the mode line is
/// DATA (`mode-line-format' lists, faces, hook memberships) and
/// `format-mode-line' renders any construct on demand, so the whole
/// surface is observable: the installed format pieces, the rendered line
/// for a file buffer, the directory/buffer-name shortening and prefix
/// replacement rules, the theme faces, and the minor-mode list.
///
/// rich-minority 20240924.2317 is prepared through `with_melpa_dependency'
/// (the package is not otherwise in the lock, so this suite adds its row);
/// both versions are pinned.
const SMART_MODE_LINE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; `sml/setup' calls `load-theme' WITHOUT no-confirm, which runs GNU's
;; `custom-safe-themes' check; in batch the confirmation prompt can never
;; be answered, so the load fails with "Unable to load theme" in BOTH
;; editors.  Trusting the pinned theme files is the documented batch
;; setup for every theme-loading package.
(setq custom-safe-themes t)

(defconst sml-test-upstream-tree
  "f933e4f517b18863773e2103c23f8030d6127e96"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst sml-test-manifest
  '(("smart-mode-line-dark-theme.el"
     . "fc1275617f9c8d1c8351df9667d750a8e3da2658077cfdda2ca281a2ebc914e0")
    ("smart-mode-line-light-theme.el"
     . "45631691477ddee3df12013e718689dafa607771e7fd37ebc6c6eb9529a8ede5")
    ("smart-mode-line-respectful-theme.el"
     . "9b21c848d09ba7df8af217438797336ac99cbbbc87a08dc879e9291673a6a631")
    ("smart-mode-line-pkg.el"
     . "caadd8c67f97de6aa586942a2e236cd9c9b0b831c3ff73fd181189361a3439f5")
    ("smart-mode-line.el"
     . "40ec7e6e2f03ca7e384371762dc3a2ed10a94f4624543bd6e036ce8aea2ecb66"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun sml-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "smart-mode-line.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/smart-mode-line.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed smart-mode-line location: %S" located))
    (dolist (entry sml-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed smart-mode-line source: %S"
                   (car entry))))))
    (list :upstream-tree sml-test-upstream-tree
          :feature (featurep 'smart-mode-line)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'smart-mode-line package-alist))))
          :rich-minority (package-version-join
                          (package-desc-version
                           (cadr (assq 'rich-minority package-alist)))))))

(defun sml-test-faces ()
  "The resolved theme colors of the core mode-line faces."
  (mapcar (lambda (face)
            (list face
                  :foreground (and (facep face)
                                   (face-attribute face :foreground nil t))
                  :background (and (facep face)
                                   (face-attribute face :background nil t))))
          '(sml/global
            sml/line-number
            sml/position-percentage
            sml/prefix
            sml/filename
            sml/fill
            sml/modes)))

(defun sml-test-reset ()
  "Undo the mode-line installation and the theme faces."
  (when (featurep 'smart-mode-line)
    ;; Restore the filtered-out defaults as far as they are recorded.
    (ignore-errors
      (setq-default mode-line-front-space '("")
                    mode-line-mule-info
                    '("" (coding-system-encode-for-write
                          (:eval (coding-system-change-eol-conversion
                                  coding-system-encode-for-write last-coding-system-used)))
                      (:eval (if (display-graphic-p) "" (concat ":" (coding-system-eol-type last-coding-system-used))))
                      utf-8)
                    mode-line-client '("" (:propertize ("" (:eval (if (display-graphic-p) "" (concat "@" client-name)))) help-echo "emacsclient"))
                    mode-line-modified '("--" (:eval (if (buffer-modified-p) (propertize "**" 'face 'error) "--")) (:eval (if buffer-read-only (propertize "%%" 'face 'error) "--")))
                    mode-line-frame-identification '("")
                    mode-line-buffer-identification '("%12b")
                    mode-line-position '((-3 "%p") (line-number-mode (" L%l")) (column-number-mode (" C%c")))
                    mode-line-modes '("%[(" (:eval (propertize (format-mode-line minor-mode-alist) 'face 'mode-line-emphasis)) "%n" mode-name ")%]-" (-3 "%I"))
                    mode-line-end-spaces '("%-")))
    (remove-hook 'after-save-hook 'sml/generate-buffer-identification)
    (remove-hook 'post-command-hook 'sml/generate-position-help)))
"####;

fn smart_mode_line_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SMART_MODE_LINE_MELPA_PIN, "smart-mode-line.el")
        .expect("prepare pinned smart-mode-line source below ./tmp")
        .with_melpa_dependency(RICH_MINORITY_MELPA_PIN)
        .expect("prepare pinned rich-minority dependency")
        .with_prelude(SMART_MODE_LINE_TEST_PRELUDE)
        .with_timeout(SMART_MODE_LINE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed smart-mode-line parity test")
        .into()
}

/// Multi-probe batch for `assert_smart_mode_line_parity` cases (2a).
pub(crate) fn assert_smart_mode_line_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        smart_mode_line_oracle(),
        &name,
        "smart_mode_line_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn smart_mode_line_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_smart_mode_line_batch(&cases);
}

// END generated package batch tests
