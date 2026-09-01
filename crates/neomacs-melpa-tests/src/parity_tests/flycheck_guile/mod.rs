//! Practical parity for flycheck-guile's Guile checker.
//!
//! These cases register the checker, report Geiser verify status, build
//! `guild compile` argv from warnings and load-paths, parse planted
//! compiler output through the checker's patterns and column filter,
//! and reject a non-Guile Geiser implementation.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FLYCHECK_GUILE_MELPA_PIN, FLYCHECK_MELPA_PIN, GEISER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'flycheck)
(require 'flycheck-guile)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")
(setq flycheck-check-syntax-automatically nil
      flycheck-display-errors-function nil
      flycheck-help-echo-function nil)
(defvar geiser-mode nil)
(defvar geiser-impl--implementation nil)

(defconst fg462-test-tree
  "c989f6576568bef96bf11e97df5ecae2b4eee5a0")
(defconst fg462-test-manifest
  '(("flycheck-guile-pkg.el" . "c075e3e2eefda4b7c06dbccf377f1ceb56f8b19e9546e685988da14049ba8c02")
    ("flycheck-guile.el" . "4365e7f26af89e7746d3235f2327388451123fa48a5e562ca9d8a5d3ca8ae1a3")))

(defun fg462-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun fg462-test-source-state ()
  (let* ((located (locate-library "flycheck-guile.el"))
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
                         (cons file (fg462-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/flycheck-guile.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car fg462-test-manifest)))
      (error "Unexpected installed flycheck-guile payload: %S"
             (or manifest files)))
    (dolist (entry fg462-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (fg462-test-sha file) expected))
          (error "Unexpected installed flycheck-guile source: %S"
                 (cons entry manifest)))))
    (list :tree fg462-test-tree
          :manifest manifest
          :feature (featurep 'flycheck-guile)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'flycheck-guile package-alist)))))))

(defun fg462-test-diagnostics (errors)
  (mapcar
   (lambda (err)
     (list :line (flycheck-error-line err)
           :column (flycheck-error-column err)
           :level (flycheck-error-level err)
           :id (flycheck-error-id err)
           :message (flycheck-error-message err)))
   errors))

(defun fg462-test-verify-messages ()
  (mapcar (lambda (result)
            (list :label (flycheck-verification-result-label result)
                  :message (flycheck-verification-result-message result)))
          (funcall (flycheck-checker-get 'guile 'verify) 'guile)))

(defun fg462-test-mask-args (args)
  (mapcar (lambda (arg)
            (replace-regexp-in-string
             "flycheck[A-Za-z0-9]+/notes\\.scm\\'"
             "flycheckNNNN/notes.scm"
             arg))
          args))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYCHECK_GUILE_MELPA_PIN, "flycheck-guile.el")
        .expect("prepare pinned flycheck-guile source below ./tmp")
        .with_melpa_dependency(FLYCHECK_MELPA_PIN)
        .expect("prepare pinned flycheck dependency below ./tmp")
        .with_melpa_dependency(GEISER_MELPA_PIN)
        .expect("prepare pinned geiser dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn registers_checker_defaults_and_verify_without_geiser() -> ParityBatchCase {
    ParityBatchCase::value(
        "registers_checker_defaults_and_verify_without_geiser",
        r####"
(list :source (fg462-test-source-state)
      :registered (and (memq 'guile flycheck-checkers) t)
      :warnings flycheck-guile-warnings
      :modes (flycheck-checker-get 'guile 'modes)
      :verify (fg462-test-verify-messages))
"####,
        expect![[
            r#"OK (:source (:tree "c989f6576568bef96bf11e97df5ecae2b4eee5a0" :manifest (("flycheck-guile-pkg.el" . "c075e3e2eefda4b7c06dbccf377f1ceb56f8b19e9546e685988da14049ba8c02") ("flycheck-guile.el" . "4365e7f26af89e7746d3235f2327388451123fa48a5e562ca9d8a5d3ca8ae1a3")) :feature t :version "20230405.1154") :registered t :warnings ("unbound-variable" "macro-use-before-definition" "use-before-definition" "non-idempotent-definition" "arity-mismatch" "duplicate-case-datum" "bad-case-datum" "format") :modes (scheme-mode geiser-mode) :verify ((:label "executable" :message "Not found") (:label "Geiser Implementation" :message "Geiser not active")))"#
        ]],
    )
}

fn command_includes_warnings_and_project_load_path() -> ParityBatchCase {
    ParityBatchCase::value(
        "command_includes_warnings_and_project_load_path",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "fg-project"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "notes.scm" root))
       (lib (expand-file-name "lib/café" root))
       (flycheck-guile-args '("--no-auto-compile"))
       buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory lib t)
        (write-region "(define (hello) 'café)\n" nil source nil 'silent)
        (set 'geiser-guile-load-path (list lib))
        (set 'geiser-repl-add-project-paths t)
        (set 'geiser-repl-current-project-function (lambda () root))
        (setq buf (generate-new-buffer "notes.scm"))
        (with-current-buffer buf
          (insert-file-contents source)
          (setq buffer-file-name source)
          (let ((delay-mode-hooks t))
            (scheme-mode))
          (setq-local geiser-mode t)
          (setq-local geiser-impl--implementation 'guile)
          (list :source (fg462-test-source-state)
                :load-path (flycheck-guile--load-path-args)
                :command (fg462-test-mask-args
                          (flycheck-checker-substituted-arguments 'guile)))))
    (set 'geiser-guile-load-path nil)
    (set 'geiser-repl-add-project-paths nil)
    (set 'geiser-repl-current-project-function nil)
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "c989f6576568bef96bf11e97df5ecae2b4eee5a0" :manifest (("flycheck-guile-pkg.el" . "c075e3e2eefda4b7c06dbccf377f1ceb56f8b19e9546e685988da14049ba8c02") ("flycheck-guile.el" . "4365e7f26af89e7746d3235f2327388451123fa48a5e562ca9d8a5d3ca8ae1a3")) :feature t :version "20230405.1154") :load-path ("-L" "[ORACLE-SANDBOX]/fg-project" "-L" "[ORACLE-SANDBOX]/fg-project/lib/café") :command ("compile" "-O0" "--no-auto-compile" "-W" "unbound-variable" "-W" "macro-use-before-definition" "-W" "use-before-definition" "-W" "non-idempotent-definition" "-W" "arity-mismatch" "-W" "duplicate-case-datum" "-W" "bad-case-datum" "-W" "format" "-L" "[ORACLE-SANDBOX]/fg-project" "-L" "[ORACLE-SANDBOX]/fg-project/lib/café" "[ORACLE-TMPDIR]/flycheckNNNN/notes.scm"))"#
        ]],
    )
}

fn planted_guild_output_parses_warning_and_error_columns() -> ParityBatchCase {
    ParityBatchCase::value(
        "planted_guild_output_parses_warning_and_error_columns",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "fg-parse"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "notes.scm" root))
       buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (write-region "(define (hello)\n  (let ((x 1))\n    (café-missing)))\n" nil source nil 'silent)
        (setq buf (generate-new-buffer "notes.scm"))
        (with-current-buffer buf
          (insert-file-contents source)
          (setq buffer-file-name source)
          (let ((delay-mode-hooks t))
            (scheme-mode))
          (setq-local geiser-mode t)
          (setq-local geiser-impl--implementation 'guile)
          (let* ((output
                  (concat source ":3:4: warning: unused-variable `x'\n"
                          "<unknown-location>: warning: café unknown\n"
                          source ":5:2: Unbound variable: café-missing\n"))
                 (parsed (flycheck-parse-output output 'guile (current-buffer)))
                 (errors (funcall (flycheck-checker-get 'guile 'error-filter)
                                  parsed))
                 (predicate (funcall (flycheck-checker-get 'guile 'predicate))))
            (list :source (fg462-test-source-state)
                  :predicate (and predicate t)
                  :errors (fg462-test-diagnostics errors)))))
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "c989f6576568bef96bf11e97df5ecae2b4eee5a0" :manifest (("flycheck-guile-pkg.el" . "c075e3e2eefda4b7c06dbccf377f1ceb56f8b19e9546e685988da14049ba8c02") ("flycheck-guile.el" . "4365e7f26af89e7746d3235f2327388451123fa48a5e562ca9d8a5d3ca8ae1a3")) :feature t :version "20230405.1154") :predicate t :errors ((:line 3 :column 5 :level warning :id nil :message " unused-variable `x'") (:line 0 :column nil :level warning :id nil :message " café unknown") (:line 5 :column 3 :level error :id nil :message "Unbound variable: café-missing\n")))"#
        ]],
    )
}

fn predicate_rejects_non_guile_geiser() -> ParityBatchCase {
    ParityBatchCase::value(
        "predicate_rejects_non_guile_geiser",
        r####"
(let ((buf (generate-new-buffer "fg-pred.scm")))
  (unwind-protect
      (with-current-buffer buf
        (let ((delay-mode-hooks t))
          (scheme-mode))
        (let* ((predicate (flycheck-checker-get 'guile 'predicate))
               (inactive (list :geiser-mode (bound-and-true-p geiser-mode)
                               :usable (and (funcall predicate) t)
                               :verify (fg462-test-verify-messages))))
          (setq-local geiser-mode t)
          (setq-local geiser-impl--implementation 'racket)
          (let ((racket (list :usable (and (funcall predicate) t)
                              :verify (fg462-test-verify-messages))))
            (setq-local geiser-impl--implementation 'guile)
            (list :source (fg462-test-source-state)
                  :inactive inactive
                  :racket racket
                  :guile (list :usable (and (funcall predicate) t)
                               :verify (fg462-test-verify-messages))))))
    (when (buffer-live-p buf) (kill-buffer buf))))
"####,
        expect![[
            r#"OK (:source (:tree "c989f6576568bef96bf11e97df5ecae2b4eee5a0" :manifest (("flycheck-guile-pkg.el" . "c075e3e2eefda4b7c06dbccf377f1ceb56f8b19e9546e685988da14049ba8c02") ("flycheck-guile.el" . "4365e7f26af89e7746d3235f2327388451123fa48a5e562ca9d8a5d3ca8ae1a3")) :feature t :version "20230405.1154") :inactive (:geiser-mode nil :usable nil :verify ((:label "executable" :message "Not found") (:label "Geiser Implementation" :message "Geiser not active"))) :racket (:usable nil :verify ((:label "executable" :message "Not found") (:label "Geiser Implementation" :message "Other: racket"))) :guile (:usable t :verify ((:label "executable" :message "Not found") (:label "Geiser Implementation" :message "Guile"))))"#
        ]],
    )
}

#[test]
fn flycheck_guile_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        registers_checker_defaults_and_verify_without_geiser(),
        command_includes_warnings_and_project_load_path(),
        planted_guild_output_parses_warning_and_error_columns(),
        predicate_rejects_non_guile_geiser(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "flycheck-guile-rank462",
        "flycheck_guile_parity",
        &cases,
    );
}
