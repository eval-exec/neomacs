//! Practical parity for the gruber-darker theme.
//!
//! These cases load and enable the theme, pin registered face colors
//! and the theme variable, then disable it and restore the previous
//! theme list.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GRUBER_DARKER_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'gruber-darker-theme)
(setq custom-safe-themes t)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst gdt483-test-tree
  "c552f2da20e44cf1790cf699e437e6e442cf9c61")
(defconst gdt483-test-manifest
  '(("gruber-darker-theme-pkg.el" . "3b7377131e5fb6a709f0faee4d9dd652685dc8b3d1f85314cc5917e58853ce36")
    ("gruber-darker-theme.el" . "01a9797244146bbae39b18ef37e6f2ca5bebded90d9fe3a2f342a9e863aaa4fd")))

(defun gdt483-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun gdt483-test-source-state ()
  (let* ((located (locate-library "gruber-darker-theme.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (gdt483-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/gruber-darker-theme.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car gdt483-test-manifest)))
      (error "Unexpected installed gruber-darker-theme payload: %S"
             (or manifest files)))
    (dolist (entry gdt483-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (gdt483-test-sha file) expected))
          (error "Unexpected installed gruber-darker-theme source: %S"
                 (cons entry manifest)))))
    (list :tree gdt483-test-tree
          :manifest manifest
          :feature (featurep 'gruber-darker-theme)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'gruber-darker-theme package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GRUBER_DARKER_THEME_MELPA_PIN, "gruber-darker-theme.el")
        .expect("prepare pinned gruber-darker-theme source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn load_theme_registers_faces_and_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "load_theme_registers_faces_and_variables",
        r####"
(let ((before (copy-sequence custom-enabled-themes)))
  (unwind-protect
      (progn
        (load-theme 'gruber-darker t)
        (list :source (gdt483-test-source-state)
              :enabled (copy-sequence custom-enabled-themes)
              :provided (custom-theme-p 'gruber-darker)
              :bg (frame-parameter nil 'background-mode)
              :default-fg (face-foreground 'default nil t)
              :default-bg (face-background 'default nil t)
              :comment (face-foreground 'font-lock-comment-face nil t)
              :keyword (face-foreground 'font-lock-keyword-face nil t)
              :string (face-foreground 'font-lock-string-face nil t)
              :typo (cadr (assq 'frame-brackground-mode
                                (get 'gruber-darker 'theme-settings)))))
    (disable-theme 'gruber-darker)
    (setq custom-enabled-themes (copy-sequence before))))
"####,
        expect![[
            r##"OK (:source (:tree "c552f2da20e44cf1790cf699e437e6e442cf9c61" :manifest (("gruber-darker-theme-pkg.el" . "3b7377131e5fb6a709f0faee4d9dd652685dc8b3d1f85314cc5917e58853ce36") ("gruber-darker-theme.el" . "01a9797244146bbae39b18ef37e6f2ca5bebded90d9fe3a2f342a9e863aaa4fd")) :feature t :version "20231026.2031") :enabled (gruber-darker) :provided (gruber-darker user changed) :bg dark :default-fg "#e4e4ef" :default-bg "#181818" :comment "#cc8c3c" :keyword "#ffdd33" :string "#73c936" :typo nil)"##
        ]],
    )
}

fn disable_theme_removes_it_from_enabled_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "disable_theme_removes_it_from_enabled_list",
        r####"
(let ((before (copy-sequence custom-enabled-themes)))
  (unwind-protect
      (progn
        (load-theme 'gruber-darker t)
        (let ((on (memq 'gruber-darker custom-enabled-themes)))
          (disable-theme 'gruber-darker)
          (list :source (gdt483-test-source-state)
                :on (and on t)
                :off (and (memq 'gruber-darker custom-enabled-themes) t)
                :after (copy-sequence custom-enabled-themes))))
    (when (custom-theme-enabled-p 'gruber-darker)
      (disable-theme 'gruber-darker))
    (setq custom-enabled-themes (copy-sequence before))))
"####,
        expect![[
            r#"OK (:source (:tree "c552f2da20e44cf1790cf699e437e6e442cf9c61" :manifest (("gruber-darker-theme-pkg.el" . "3b7377131e5fb6a709f0faee4d9dd652685dc8b3d1f85314cc5917e58853ce36") ("gruber-darker-theme.el" . "01a9797244146bbae39b18ef37e6f2ca5bebded90d9fe3a2f342a9e863aaa4fd")) :feature t :version "20231026.2031") :on t :off nil :after nil)"#
        ]],
    )
}

#[test]
fn gruber_darker_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        load_theme_registers_faces_and_variables(),
        disable_theme_removes_it_from_enabled_list(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "gruber-darker-theme-rank483",
        "gruber_darker_theme_parity",
        &cases,
    );
}
