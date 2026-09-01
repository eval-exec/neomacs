//! Practical parity for the darktooth theme family.
//!
//! These cases load the default and darker variants through `load-theme`,
//! pin enabled-theme membership, and disable them again.

use std::time::Duration;

use expect_test::expect;

use crate::{AUTOTHEMER_MELPA_PIN, CachedMelpaOracle, DARKTOOTH_THEME_MELPA_PIN, DASH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'darktooth-theme)
(setq custom-safe-themes t)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst dt489-test-tree
  "3e9ac5daf86dd7c0e0cc35999e21cd6090850518")
(defconst dt489-test-manifest
  '(("darktooth-dark-theme.el" . "1ff0e19c6491e945df67d861bf1f132fac3d7261df22e792fbaf8fe2999a6a38")
    ("darktooth-darker-theme.el" . "50ea0aed00bc9d25af0925cba7088b54d93415aec6174ec4cdb162b7a101c965")
    ("darktooth-theme-pkg.el" . "b911e5a97ef4cbd8d0ba9ed0dd667a4034622fb549387cc10668c2387701d35c")
    ("darktooth-theme.el" . "25e3f1fc8e66c25cb5340487825035c4a1cea2f990df5d081b005fb6798c3893")
    ("darktooth.el" . "713d4e121e09bcae65e8c736963b84c62709c83279dd6c183826f462525b599e")))

(defun dt489-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun dt489-test-source-state ()
  (let* ((located (locate-library "darktooth-theme.el"))
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
                         (cons file (dt489-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/darktooth-theme.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car dt489-test-manifest)))
      (error "Unexpected installed darktooth-theme payload: %S"
             (or manifest files)))
    (dolist (entry dt489-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (dt489-test-sha file) expected))
          (error "Unexpected installed darktooth-theme source: %S"
                 (cons entry manifest)))))
    (list :tree dt489-test-tree
          :manifest manifest
          :feature (featurep 'darktooth-theme)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'darktooth-theme package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DARKTOOTH_THEME_MELPA_PIN, "darktooth-theme.el")
        .expect("prepare pinned darktooth-theme source below ./tmp")
        .with_melpa_dependency(AUTOTHEMER_MELPA_PIN)
        .expect("prepare pinned autothemer dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn load_darktooth_enables_the_theme() -> ParityBatchCase {
    ParityBatchCase::value(
        "load_darktooth_enables_the_theme",
        r####"
(let ((before (copy-sequence custom-enabled-themes)))
  (unwind-protect
      (progn
        (load-theme 'darktooth t)
        (list :source (dt489-test-source-state)
              :enabled (copy-sequence custom-enabled-themes)
              :provided (and (custom-theme-p 'darktooth) t)
              :fg (face-foreground 'default nil t)
              :bg (face-background 'default nil t)
              :comment (face-foreground 'font-lock-comment-face nil t)))
    (when (custom-theme-enabled-p 'darktooth)
      (disable-theme 'darktooth))
    (setq custom-enabled-themes (copy-sequence before))))
"####,
        expect![[
            r#"OK (:source (:tree "3e9ac5daf86dd7c0e0cc35999e21cd6090850518" :manifest (("darktooth-dark-theme.el" . "1ff0e19c6491e945df67d861bf1f132fac3d7261df22e792fbaf8fe2999a6a38") ("darktooth-darker-theme.el" . "50ea0aed00bc9d25af0925cba7088b54d93415aec6174ec4cdb162b7a101c965") ("darktooth-theme-pkg.el" . "b911e5a97ef4cbd8d0ba9ed0dd667a4034622fb549387cc10668c2387701d35c") ("darktooth-theme.el" . "25e3f1fc8e66c25cb5340487825035c4a1cea2f990df5d081b005fb6798c3893") ("darktooth.el" . "713d4e121e09bcae65e8c736963b84c62709c83279dd6c183826f462525b599e")) :feature t :version "20251019.304") :enabled (darktooth) :provided t :fg "unspecified-fg" :bg "unspecified-bg" :comment nil)"#
        ]],
    )
}

fn darker_variant_loads_and_disables() -> ParityBatchCase {
    ParityBatchCase::value(
        "darker_variant_loads_and_disables",
        r####"
(let ((before (copy-sequence custom-enabled-themes)))
  (unwind-protect
      (progn
        (require 'darktooth-darker-theme)
        (load-theme 'darktooth-darker t)
        (let ((on (copy-sequence custom-enabled-themes))
              (fg (face-foreground 'default nil t))
              (bg (face-background 'default nil t)))
          (disable-theme 'darktooth-darker)
          (list :source (dt489-test-source-state)
                :on on
                :fg fg
                :bg bg
                :off (and (memq 'darktooth-darker custom-enabled-themes) t))))
    (when (custom-theme-enabled-p 'darktooth-darker)
      (disable-theme 'darktooth-darker))
    (setq custom-enabled-themes (copy-sequence before))))
"####,
        expect![[
            r#"OK (:source (:tree "3e9ac5daf86dd7c0e0cc35999e21cd6090850518" :manifest (("darktooth-dark-theme.el" . "1ff0e19c6491e945df67d861bf1f132fac3d7261df22e792fbaf8fe2999a6a38") ("darktooth-darker-theme.el" . "50ea0aed00bc9d25af0925cba7088b54d93415aec6174ec4cdb162b7a101c965") ("darktooth-theme-pkg.el" . "b911e5a97ef4cbd8d0ba9ed0dd667a4034622fb549387cc10668c2387701d35c") ("darktooth-theme.el" . "25e3f1fc8e66c25cb5340487825035c4a1cea2f990df5d081b005fb6798c3893") ("darktooth.el" . "713d4e121e09bcae65e8c736963b84c62709c83279dd6c183826f462525b599e")) :feature t :version "20251019.304") :on (darktooth-darker) :fg "unspecified-fg" :bg "unspecified-bg" :off nil)"#
        ]],
    )
}

#[test]
fn darktooth_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        load_darktooth_enables_the_theme(),
        darker_variant_loads_and_disables(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "darktooth-theme-rank489",
        "darktooth_theme_parity",
        &cases,
    );
}
