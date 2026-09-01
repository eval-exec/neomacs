//! Practical parity for the sublime-themes collection.
//!
//! These cases load Spolsky and Hickey through `load-theme` after the
//! package adds its directory to `custom-theme-load-path`, pin enabled
//! membership and realized faces, then disable the themes again.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SUBLIME_THEMES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'sublime-themes)
(setq custom-safe-themes t)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst st493-test-tree
  "dd367c4fd19753fae34694bc736bb02455bbe897")
(defconst st493-test-manifest
  '(("brin-theme.el" . "9b59e147dbbde5e638ea1cde5ec0a358d5f269d27bd2b893a0947c4a867e14c1")
    ("dorsey-theme.el" . "b3775ba758e7d31f3bb849e7c9e48ff60929a792961a2d536edec8f68c671ca5")
    ("fogus-theme.el" . "3d5ef3d7ed58c9ad321f05360ad8a6b24585b9c49abcee67bdcbb0fe583a6950")
    ("graham-theme.el" . "58c6711a3b568437bab07a30385d34aacf64156cc5137ea20e799984f4227265")
    ("granger-theme.el" . "72a81c54c97b9e5efcc3ea214382615649ebb539cb4f2fe3a46cd12af72c7607")
    ("hickey-theme.el" . "e9776d12e4ccb722a2a732c6e80423331bcb93f02e089ba2a4b02e85de1cf00e")
    ("junio-theme.el" . "3cc2385c39257fed66238921602d8104d8fd6266ad88a006d0a4325336f5ee02")
    ("mccarthy-theme.el" . "3cd28471e80be3bd2657ca3f03fbb2884ab669662271794360866ab60b6cb6e6")
    ("odersky-theme.el" . "e0d42a58c84161a0744ceab595370cbe290949968ab62273aed6212df0ea94b4")
    ("ritchie-theme.el" . "987b709680284a5858d5fe7e4e428463a20dfabe0a6f2a6146b3b8c7c529f08b")
    ("spolsky-theme.el" . "c48551a5fb7b9fc019bf3f61ebf14cf7c9cdca79bcb2a4219195371c02268f11")
    ("sublime-themes-pkg.el" . "1acbe2e91dc7e89c3b008a81768f8e6c2b9ed554e41ca2112d3a3594194c9863")
    ("sublime-themes.el" . "5248f6cc6c4cb3a1c13504baf4f89a21ff7884d348a91c07b0cbbc3785a08304")
    ("wilson-theme.el" . "96998f6f11ef9f551b427b8853d947a7857ea5a578c75aa9c4e7c73fe04d10b4")))

(defun st493-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun st493-test-source-state ()
  (let* ((located (locate-library "sublime-themes.el"))
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
                         (cons file (st493-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/sublime-themes.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car st493-test-manifest)))
      (error "Unexpected installed sublime-themes payload: %S"
             (or manifest files)))
    (dolist (entry st493-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (st493-test-sha file) expected))
          (error "Unexpected installed sublime-themes source: %S"
                 (cons entry manifest)))))
    (list :tree st493-test-tree
          :manifest manifest
          :feature (featurep 'sublime-themes)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'sublime-themes package-alist))))
          :on-path
          (and directory
               (cl-some
                (lambda (p)
                  (and (stringp p)
                       (string-equal
                        (file-truename (file-name-as-directory directory))
                        (file-truename (file-name-as-directory p)))))
                custom-theme-load-path)
               t))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SUBLIME_THEMES_MELPA_PIN, "sublime-themes.el")
        .expect("prepare pinned sublime-themes source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn load_spolsky_enables_faces_and_load_path() -> ParityBatchCase {
    ParityBatchCase::value(
        "load_spolsky_enables_faces_and_load_path",
        r####"
(let ((before (copy-sequence custom-enabled-themes)))
  (unwind-protect
      (progn
        (load-theme 'spolsky t)
        (list :source (st493-test-source-state)
              :enabled (copy-sequence custom-enabled-themes)
              :provided (and (custom-theme-p 'spolsky) t)
              :fg (face-foreground 'default nil t)
              :bg (face-background 'default nil t)
              :comment (face-foreground 'font-lock-comment-face nil t)
              :keyword (face-foreground 'font-lock-keyword-face nil t)
              :string (face-foreground 'font-lock-string-face nil t)))
    (when (custom-theme-enabled-p 'spolsky)
      (disable-theme 'spolsky))
    (setq custom-enabled-themes (copy-sequence before))))
"####,
        expect![[
            r##"OK (:source (:tree "dd367c4fd19753fae34694bc736bb02455bbe897" :manifest (("brin-theme.el" . "9b59e147dbbde5e638ea1cde5ec0a358d5f269d27bd2b893a0947c4a867e14c1") ("dorsey-theme.el" . "b3775ba758e7d31f3bb849e7c9e48ff60929a792961a2d536edec8f68c671ca5") ("fogus-theme.el" . "3d5ef3d7ed58c9ad321f05360ad8a6b24585b9c49abcee67bdcbb0fe583a6950") ("graham-theme.el" . "58c6711a3b568437bab07a30385d34aacf64156cc5137ea20e799984f4227265") ("granger-theme.el" . "72a81c54c97b9e5efcc3ea214382615649ebb539cb4f2fe3a46cd12af72c7607") ("hickey-theme.el" . "e9776d12e4ccb722a2a732c6e80423331bcb93f02e089ba2a4b02e85de1cf00e") ("junio-theme.el" . "3cc2385c39257fed66238921602d8104d8fd6266ad88a006d0a4325336f5ee02") ("mccarthy-theme.el" . "3cd28471e80be3bd2657ca3f03fbb2884ab669662271794360866ab60b6cb6e6") ("odersky-theme.el" . "e0d42a58c84161a0744ceab595370cbe290949968ab62273aed6212df0ea94b4") ("ritchie-theme.el" . "987b709680284a5858d5fe7e4e428463a20dfabe0a6f2a6146b3b8c7c529f08b") ("spolsky-theme.el" . "c48551a5fb7b9fc019bf3f61ebf14cf7c9cdca79bcb2a4219195371c02268f11") ("sublime-themes-pkg.el" . "1acbe2e91dc7e89c3b008a81768f8e6c2b9ed554e41ca2112d3a3594194c9863") ("sublime-themes.el" . "5248f6cc6c4cb3a1c13504baf4f89a21ff7884d348a91c07b0cbbc3785a08304") ("wilson-theme.el" . "96998f6f11ef9f551b427b8853d947a7857ea5a578c75aa9c4e7c73fe04d10b4")) :feature t :version "20170606.1844" :on-path t) :enabled (spolsky) :provided t :fg "#DEDEDE" :bg nil :comment "#8C8C8C" :keyword "#F92672" :string "#EEDC82")"##
        ]],
    )
}

fn load_hickey_then_disable_restores_previous_themes() -> ParityBatchCase {
    ParityBatchCase::value(
        "load_hickey_then_disable_restores_previous_themes",
        r####"
(let ((before (copy-sequence custom-enabled-themes)))
  (unwind-protect
      (progn
        (load-theme 'hickey t)
        (let ((on (copy-sequence custom-enabled-themes))
              (fg (face-foreground 'default nil t))
              (bg (face-background 'default nil t))
              (comment (face-foreground 'font-lock-comment-face nil t)))
          (disable-theme 'hickey)
          (list :source (st493-test-source-state)
                :on on
                :fg fg
                :bg bg
                :comment comment
                :off (and (memq 'hickey custom-enabled-themes) t)
                :after (copy-sequence custom-enabled-themes))))
    (when (custom-theme-enabled-p 'hickey)
      (disable-theme 'hickey))
    (setq custom-enabled-themes (copy-sequence before))))
"####,
        expect![[
            r##"OK (:source (:tree "dd367c4fd19753fae34694bc736bb02455bbe897" :manifest (("brin-theme.el" . "9b59e147dbbde5e638ea1cde5ec0a358d5f269d27bd2b893a0947c4a867e14c1") ("dorsey-theme.el" . "b3775ba758e7d31f3bb849e7c9e48ff60929a792961a2d536edec8f68c671ca5") ("fogus-theme.el" . "3d5ef3d7ed58c9ad321f05360ad8a6b24585b9c49abcee67bdcbb0fe583a6950") ("graham-theme.el" . "58c6711a3b568437bab07a30385d34aacf64156cc5137ea20e799984f4227265") ("granger-theme.el" . "72a81c54c97b9e5efcc3ea214382615649ebb539cb4f2fe3a46cd12af72c7607") ("hickey-theme.el" . "e9776d12e4ccb722a2a732c6e80423331bcb93f02e089ba2a4b02e85de1cf00e") ("junio-theme.el" . "3cc2385c39257fed66238921602d8104d8fd6266ad88a006d0a4325336f5ee02") ("mccarthy-theme.el" . "3cd28471e80be3bd2657ca3f03fbb2884ab669662271794360866ab60b6cb6e6") ("odersky-theme.el" . "e0d42a58c84161a0744ceab595370cbe290949968ab62273aed6212df0ea94b4") ("ritchie-theme.el" . "987b709680284a5858d5fe7e4e428463a20dfabe0a6f2a6146b3b8c7c529f08b") ("spolsky-theme.el" . "c48551a5fb7b9fc019bf3f61ebf14cf7c9cdca79bcb2a4219195371c02268f11") ("sublime-themes-pkg.el" . "1acbe2e91dc7e89c3b008a81768f8e6c2b9ed554e41ca2112d3a3594194c9863") ("sublime-themes.el" . "5248f6cc6c4cb3a1c13504baf4f89a21ff7884d348a91c07b0cbbc3785a08304") ("wilson-theme.el" . "96998f6f11ef9f551b427b8853d947a7857ea5a578c75aa9c4e7c73fe04d10b4")) :feature t :version "20170606.1844" :on-path t) :on (hickey) :fg "#F8F8F2" :bg nil :comment "#505C63" :off nil :after nil)"##
        ]],
    )
}

#[test]
fn sublime_themes_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        load_spolsky_enables_faces_and_load_path(),
        load_hickey_then_disable_restores_previous_themes(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "sublime-themes-rank493",
        "sublime_themes_parity",
        &cases,
    );
}
