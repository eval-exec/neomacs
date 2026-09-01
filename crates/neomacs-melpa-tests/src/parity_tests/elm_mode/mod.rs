//! Practical parity for elm-mode editing, imports, and compile argv.
//!
//! These cases activate Elm mode, indent a module, sort imports, and
//! build the `elm make` command without running the compiler.

use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, ELM_MODE_MELPA_PIN, F_MELPA_PIN, REFORMATTER_MELPA_PIN, S_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'elm-mode)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")
(setq elm-format-on-save nil
      elm-sort-imports-on-save nil
      elm-tags-on-save nil)

(defconst elm485-test-tree
  "7417338bc7051ecebc99ba3c2134f4b478187490")
(defconst elm485-test-manifest
  '(("elm-defuns.el" . "40241fcd925563d9c9dc35d6c409a14f7c7c2803e02754b26c027c81d38f15fe")
    ("elm-font-lock.el" . "6d5e2c4d3b24cf7330c8b3e6dbb2e794e0be59e28c684d73c21c8923d78d4caa")
    ("elm-format.el" . "bd06b97cae55a0229c3109d376bed03303e62c666815f383d62d9d85cd1dd93e")
    ("elm-imenu.el" . "fa6e6a02f12ae5a12bd751a81a8116f5ed220beccae62db427f0db3b1bc2af18")
    ("elm-indent-simple.el" . "5f6b9704466252d8d0ef11f7a19382d2abd7fdcd25dd3e460463e7037f85c01b")
    ("elm-indent.el" . "a7db67dba819ab9d1e7ec5ead57da346f6c6840c8439919b7d7d945ed12b5b3d")
    ("elm-interactive.el" . "26182931659237e16e989eb855f8f0c6c988adb2d67f95367c5bae1036eb107d")
    ("elm-mode-pkg.el" . "eaaa212406e436be5874ed92174879f678fa30f691d00d7984dd00297523e9c9")
    ("elm-mode.el" . "7703a0dd63af7eb3e116ac9bcd4d53db425b539c8fd8195c07b0a4ff37940012")
    ("elm-tags.el" . "aa2a873cef5b97c566dbbd19229fdc62db5557af46d39665b5a6367a21a3437c")
    ("elm-util.el" . "7eb0a47d2a7b1cda73ce50b7b1ed8c582d3a850331692eb64fac3f7b872a2ee4")))

(defun elm485-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun elm485-test-source-state ()
  (let* ((located (locate-library "elm-mode.el"))
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
                         (cons file (elm485-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/elm-mode.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car elm485-test-manifest)))
      (error "Unexpected installed elm-mode payload: %S"
             (or manifest files)))
    (dolist (entry elm485-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (elm485-test-sha file) expected))
          (error "Unexpected installed elm-mode source: %S"
                 (cons entry manifest)))))
    (list :tree elm485-test-tree
          :manifest manifest
          :feature (featurep 'elm-mode)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'elm-mode package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ELM_MODE_MELPA_PIN, "elm-mode.el")
        .expect("prepare pinned elm-mode source below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_melpa_dependency(REFORMATTER_MELPA_PIN)
        .expect("prepare pinned reformatter dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn opens_elm_file_in_major_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_elm_file_in_major_mode",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "elm-open"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "Main.elm" root))
       buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (write-region "module Main exposing (..)\n" nil path nil 'silent)
        (setq buf (find-file-noselect path))
        (with-current-buffer buf
          (list :source (elm485-test-source-state)
                :auto (cdr (assoc "\\.elm\\'" auto-mode-alist))
                :mode major-mode
                :comment-start comment-start
                :format (key-binding (kbd "C-c C-f"))
                :compile (key-binding (kbd "C-c C-c")))))
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "7417338bc7051ecebc99ba3c2134f4b478187490" :manifest (("elm-defuns.el" . "40241fcd925563d9c9dc35d6c409a14f7c7c2803e02754b26c027c81d38f15fe") ("elm-font-lock.el" . "6d5e2c4d3b24cf7330c8b3e6dbb2e794e0be59e28c684d73c21c8923d78d4caa") ("elm-format.el" . "bd06b97cae55a0229c3109d376bed03303e62c666815f383d62d9d85cd1dd93e") ("elm-imenu.el" . "fa6e6a02f12ae5a12bd751a81a8116f5ed220beccae62db427f0db3b1bc2af18") ("elm-indent-simple.el" . "5f6b9704466252d8d0ef11f7a19382d2abd7fdcd25dd3e460463e7037f85c01b") ("elm-indent.el" . "a7db67dba819ab9d1e7ec5ead57da346f6c6840c8439919b7d7d945ed12b5b3d") ("elm-interactive.el" . "26182931659237e16e989eb855f8f0c6c988adb2d67f95367c5bae1036eb107d") ("elm-mode-pkg.el" . "eaaa212406e436be5874ed92174879f678fa30f691d00d7984dd00297523e9c9") ("elm-mode.el" . "7703a0dd63af7eb3e116ac9bcd4d53db425b539c8fd8195c07b0a4ff37940012") ("elm-tags.el" . "aa2a873cef5b97c566dbbd19229fdc62db5557af46d39665b5a6367a21a3437c") ("elm-util.el" . "7eb0a47d2a7b1cda73ce50b7b1ed8c582d3a850331692eb64fac3f7b872a2ee4")) :feature t :version "20250401.915") :auto elm-mode :mode elm-mode :comment-start "--" :format elm-format-buffer :compile elm-compile-buffer)"#
        ]],
    )
}

fn sorts_imports_and_indents_a_module() -> ParityBatchCase {
    ParityBatchCase::value(
        "sorts_imports_and_indents_a_module",
        r####"
(with-temp-buffer
  (let ((delay-mode-hooks t))
    (elm-mode))
  (insert "module Café exposing (..)\n\nimport Html exposing (..)\nimport Browser\n\nmain =\nHtml.text \"café\"\n")
  (elm-sort-imports)
  (indent-region (point-min) (point-max))
  (list :source (elm485-test-source-state)
        :text (buffer-substring-no-properties (point-min) (point-max))))
"####,
        expect![[
            r#"OK (:source (:tree "7417338bc7051ecebc99ba3c2134f4b478187490" :manifest (("elm-defuns.el" . "40241fcd925563d9c9dc35d6c409a14f7c7c2803e02754b26c027c81d38f15fe") ("elm-font-lock.el" . "6d5e2c4d3b24cf7330c8b3e6dbb2e794e0be59e28c684d73c21c8923d78d4caa") ("elm-format.el" . "bd06b97cae55a0229c3109d376bed03303e62c666815f383d62d9d85cd1dd93e") ("elm-imenu.el" . "fa6e6a02f12ae5a12bd751a81a8116f5ed220beccae62db427f0db3b1bc2af18") ("elm-indent-simple.el" . "5f6b9704466252d8d0ef11f7a19382d2abd7fdcd25dd3e460463e7037f85c01b") ("elm-indent.el" . "a7db67dba819ab9d1e7ec5ead57da346f6c6840c8439919b7d7d945ed12b5b3d") ("elm-interactive.el" . "26182931659237e16e989eb855f8f0c6c988adb2d67f95367c5bae1036eb107d") ("elm-mode-pkg.el" . "eaaa212406e436be5874ed92174879f678fa30f691d00d7984dd00297523e9c9") ("elm-mode.el" . "7703a0dd63af7eb3e116ac9bcd4d53db425b539c8fd8195c07b0a4ff37940012") ("elm-tags.el" . "aa2a873cef5b97c566dbbd19229fdc62db5557af46d39665b5a6367a21a3437c") ("elm-util.el" . "7eb0a47d2a7b1cda73ce50b7b1ed8c582d3a850331692eb64fac3f7b872a2ee4")) :feature t :version "20250401.915") :text "module Café exposing (..)\n\nimport Browser\nimport Html exposing (..)\n\nmain =\nHtml.text \"café\"\n")"#
        ]],
    )
}

fn compile_command_includes_file_and_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "compile_command_includes_file_and_output",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "elm-compile"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "Main.elm" root)))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (write-region "module Main exposing (..)\n" nil path nil 'silent)
        (list :source (elm485-test-source-state)
              :command (elm-compile--command path (expand-file-name "out.js" root))
              :format-args (list elm-format-command elm-format-elm-version)))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "7417338bc7051ecebc99ba3c2134f4b478187490" :manifest (("elm-defuns.el" . "40241fcd925563d9c9dc35d6c409a14f7c7c2803e02754b26c027c81d38f15fe") ("elm-font-lock.el" . "6d5e2c4d3b24cf7330c8b3e6dbb2e794e0be59e28c684d73c21c8923d78d4caa") ("elm-format.el" . "bd06b97cae55a0229c3109d376bed03303e62c666815f383d62d9d85cd1dd93e") ("elm-imenu.el" . "fa6e6a02f12ae5a12bd751a81a8116f5ed220beccae62db427f0db3b1bc2af18") ("elm-indent-simple.el" . "5f6b9704466252d8d0ef11f7a19382d2abd7fdcd25dd3e460463e7037f85c01b") ("elm-indent.el" . "a7db67dba819ab9d1e7ec5ead57da346f6c6840c8439919b7d7d945ed12b5b3d") ("elm-interactive.el" . "26182931659237e16e989eb855f8f0c6c988adb2d67f95367c5bae1036eb107d") ("elm-mode-pkg.el" . "eaaa212406e436be5874ed92174879f678fa30f691d00d7984dd00297523e9c9") ("elm-mode.el" . "7703a0dd63af7eb3e116ac9bcd4d53db425b539c8fd8195c07b0a4ff37940012") ("elm-tags.el" . "aa2a873cef5b97c566dbbd19229fdc62db5557af46d39665b5a6367a21a3437c") ("elm-util.el" . "7eb0a47d2a7b1cda73ce50b7b1ed8c582d3a850331692eb64fac3f7b872a2ee4")) :feature t :version "20250401.915") :command "elm make [ORACLE-SANDBOX]/elm-compile/Main.elm --output\\=[ORACLE-SANDBOX]/elm-compile/out.js" :format-args ("elm-format" "0.19"))"#
        ]],
    )
}

#[test]
fn elm_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        opens_elm_file_in_major_mode(),
        sorts_imports_and_indents_a_module(),
        compile_command_includes_file_and_output(),
    ];
    assert_oracle_batch_cases(oracle(), "elm-mode-rank485", "elm_mode_parity", &cases);
}
