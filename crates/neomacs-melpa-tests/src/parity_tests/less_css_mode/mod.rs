//! Practical parity for less-css-mode editing and compile command.
//!
//! These cases activate the mode on a `.less` file, indent nested
//! rules, font-lock variables and mixins, and record the `lessc`
//! compile command without running it.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LESS_CSS_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'less-css-mode)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst lcm476-test-tree
  "0a8c6f6140b203f1f0b45a597c17a0852278d2d0")
(defconst lcm476-test-manifest
  '(("less-css-mode-pkg.el" . "83dede1bad293a1a59f0a4ef0a2163f5bc5bc0b825af41f65eb50333ac955ce7")
    ("less-css-mode.el" . "799333a765ca451f52027169d86b5e19a9c1eee8915491bb67d3b2ce036eea96")))

(defun lcm476-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun lcm476-test-source-state ()
  (let* ((located (locate-library "less-css-mode.el"))
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
                         (cons file (lcm476-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/less-css-mode.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car lcm476-test-manifest)))
      (error "Unexpected installed less-css-mode payload: %S"
             (or manifest files)))
    (dolist (entry lcm476-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (lcm476-test-sha file) expected))
          (error "Unexpected installed less-css-mode source: %S"
                 (cons entry manifest)))))
    (list :tree lcm476-test-tree
          :manifest manifest
          :feature (featurep 'less-css-mode)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'less-css-mode package-alist)))))))

(defun lcm476-test-face-at (needle)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (get-text-property (match-beginning 0) 'face)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LESS_CSS_MODE_MELPA_PIN, "less-css-mode.el")
        .expect("prepare pinned less-css-mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn opens_less_file_in_derived_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_less_file_in_derived_mode",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "lcm-open"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "theme.less" root))
       buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (write-region "@color: #f00;\n" nil path nil 'silent)
        (setq buf (find-file-noselect path))
        (with-current-buffer buf
          (list :source (lcm476-test-source-state)
                :auto (cdr (assoc "\\.less\\'" auto-mode-alist))
                :mode major-mode
                :derived (derived-mode-p 'css-mode)
                :comment-start comment-start
                :compile (key-binding (kbd "C-c C-c")))))
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "0a8c6f6140b203f1f0b45a597c17a0852278d2d0" :manifest (("less-css-mode-pkg.el" . "83dede1bad293a1a59f0a4ef0a2163f5bc5bc0b825af41f65eb50333ac955ce7") ("less-css-mode.el" . "799333a765ca451f52027169d86b5e19a9c1eee8915491bb67d3b2ce036eea96")) :feature t :version "20161001.453") :auto less-css-mode :mode less-css-mode :derived css-mode :comment-start "//" :compile less-css-compile)"#
        ]],
    )
}

fn indents_nested_rules_and_fontifies_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "indents_nested_rules_and_fontifies_variables",
        r####"
(with-temp-buffer
  (let ((delay-mode-hooks t))
    (less-css-mode))
  (insert "@color: #f00;\n.box {\ncolor: @color;\n.mixin;\n&:hover { color: café; }\n}\n")
  (indent-region (point-min) (point-max))
  (font-lock-ensure)
  (list :source (lcm476-test-source-state)
        :text (buffer-substring-no-properties (point-min) (point-max))
        :var (lcm476-test-face-at "@color")
        :mixin (lcm476-test-face-at ".mixin")
        :amp (lcm476-test-face-at "&")))
"####,
        expect![[
            r#"OK (:source (:tree "0a8c6f6140b203f1f0b45a597c17a0852278d2d0" :manifest (("less-css-mode-pkg.el" . "83dede1bad293a1a59f0a4ef0a2163f5bc5bc0b825af41f65eb50333ac955ce7") ("less-css-mode.el" . "799333a765ca451f52027169d86b5e19a9c1eee8915491bb67d3b2ce036eea96")) :feature t :version "20161001.453") :text "@color: #f00;\n.box {\n    color: @color;\n    .mixin;\n    &:hover { color: café; }\n}\n" :var font-lock-constant-face :mixin font-lock-keyword-face :amp font-lock-preprocessor-face)"#
        ]],
    )
}

fn compile_command_targets_output_css_without_running_lessc() -> ParityBatchCase {
    ParityBatchCase::value(
        "compile_command_targets_output_css_without_running_lessc",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "lcm-compile"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "theme.less" root))
       (less-css-output-directory (expand-file-name "out" root))
       captured buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory (expand-file-name "out" root) t)
        (write-region "@color: #f00;\n" nil path nil 'silent)
        (setq buf (find-file-noselect path))
        (with-current-buffer buf
          (less-css-mode)
          (cl-letf (((symbol-function 'compile)
                     (lambda (command)
                       (setq captured command)
                       (current-buffer))))
            (less-css-compile))
          (list :source (lcm476-test-source-state)
                :command captured
                :output (less-css--output-path)
                :maybe (less-css-compile-maybe))))
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "0a8c6f6140b203f1f0b45a597c17a0852278d2d0" :manifest (("less-css-mode-pkg.el" . "83dede1bad293a1a59f0a4ef0a2163f5bc5bc0b825af41f65eb50333ac955ce7") ("less-css-mode.el" . "799333a765ca451f52027169d86b5e19a9c1eee8915491bb67d3b2ce036eea96")) :feature t :version "20161001.453") :command "lessc --no-color [ORACLE-SANDBOX]/lcm-compile/theme.less [ORACLE-SANDBOX]/lcm-compile/out/theme.css" :output "[ORACLE-SANDBOX]/lcm-compile/out/theme.css" :maybe nil)"#
        ]],
    )
}

#[test]
fn less_css_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        opens_less_file_in_derived_mode(),
        indents_nested_rules_and_fontifies_variables(),
        compile_command_targets_output_css_without_running_lessc(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "less-css-mode-rank476",
        "less_css_mode_parity",
        &cases,
    );
}
