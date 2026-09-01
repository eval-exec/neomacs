use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_MAKE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_MAKE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HELM_MAKE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'helm-make)

(defvar neomacs-helm-make-test-project-root nil)

(defun projectile-project-root ()
  "Return the project root controlled by the parity workflow."
  neomacs-helm-make-test-project-root)

(provide 'projectile)

(defun neomacs-helm-make-test-root (name)
  "Create a deterministic sandbox directory for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "helm-make-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-helm-make-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-helm-make-test-read (path)
  "Read PATH without visiting it."
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun neomacs-helm-make-test-normalize (root value)
  "Replace ROOT and its abbreviated spelling in string VALUE."
  (let ((normalized value))
    (dolist (spelling
             (delete-dups
              (list root
                    (abbreviate-file-name root)
                    (shell-quote-argument root)
                    (shell-quote-argument (abbreviate-file-name root)))))
      (setq normalized
            (replace-regexp-in-string
             (regexp-quote spelling) "<ROOT>/" normalized t t)))
    normalized))

(defun neomacs-helm-make-test-cleanup (root)
  "Kill workflow buffers, reset package state, and remove ROOT."
  (dolist (buffer (buffer-list))
    (when (or (and (buffer-file-name buffer)
                   (string-prefix-p root (buffer-file-name buffer)))
              (string-prefix-p "*compilation" (buffer-name buffer)))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (helm-make-reset-cache)
  (when (file-exists-p root)
    (delete-directory root t)))
"####;

fn helm_make_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_MAKE_MELPA_PIN, "helm-make.el")
        .expect("prepare revision-pinned helm-make source below ./tmp")
        .with_prelude(HELM_MAKE_TEST_PRELUDE)
        .with_timeout(HELM_MAKE_TEST_TIMEOUT)
}

fn nested_project_build_saves_changed_source_and_launches_selected_target() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-helm-make-test-root "nested project"))
       (service-directory (expand-file-name "services/api/" root))
       (makefile (expand-file-name "Makefile" root))
       (source-file (expand-file-name "services/api/release.el" root))
       (default-directory service-directory)
       (helm-make-directory-functions-list
        '(helm-make-current-directory helm-make-dominating-directory))
       (helm-make-completion-method 'ido)
       (helm-make-do-save t)
       (helm-make-sort-targets nil)
       (helm-make-cache-targets nil)
       (helm-make-named-buffer t)
       (helm-make-comint nil)
       (helm-make-niceness 7)
       (helm-make-nproc 12)
       (helm-make-arguments "-j%d --keep-going")
       (helm-make-target-history nil)
       (helm-make--last-item nil)
       selection compile-call compile-buffer)
  (unwind-protect
      (progn
        (neomacs-helm-make-test-write
         makefile
         ".PHONY: deploy rollback\ndeploy:\n\t@echo deploy\nrollback:\n\t@echo rollback\ndeploy:\n\t@echo audit\n")
        (neomacs-helm-make-test-write
         source-file
         "(defconst release-state 'staged)\n")
        (with-current-buffer (find-file-noselect source-file)
          (goto-char (point-max))
          (insert "(defconst release-approved t)\n"))
        (cl-letf (((symbol-function 'ido-completing-read)
                   (lambda (prompt choices predicate require-match initial history
                            &optional default inherit)
                     (setq selection
                           (list :prompt prompt
                                 :choices (copy-sequence choices)
                                 :predicate predicate
                                 :require-match require-match
                                 :initial initial
                                 :history history
                                 :default default
                                 :inherit inherit))
                     "deploy"))
                  ((symbol-function 'compile)
                   (lambda (command comint)
                     (setq compile-call
                           (list :command
                                 (neomacs-helm-make-test-normalize root command)
                                 :comint comint
                                 :directory
                                 (file-relative-name default-directory root)))
                     (setq compile-buffer
                           (get-buffer-create "*compilation*")))))
          (helm-make '(4)))
        (list
         :selection selection
         :compile compile-call
         :buffer
         (neomacs-helm-make-test-normalize root (buffer-name compile-buffer))
         :last-target helm-make--last-item
         :saved-source (neomacs-helm-make-test-read source-file)
         :source-modified
         (with-current-buffer (find-file-noselect source-file)
           (buffer-modified-p))))
    (neomacs-helm-make-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:selection (:prompt "Target: " :choices ("deploy" "rollback") :predicate nil :require-match nil :initial nil :history helm-make-target-history :default nil :inherit nil) :compile (:command "nice -n 7 make -C <ROOT>/ -j4 --keep-going deploy" :comint nil :directory "./") :buffer "*compilation in <ROOT>/ (deploy)*" :last-target "deploy" :saved-source "(defconst release-state 'staged)\n(defconst release-approved t)\n" :source-modified nil)"#
    ]];
    ParityBatchCase::value(
        "nested_project_build_saves_changed_source_and_launches_selected_target",
        elisp_form,
        expected,
    )
}

fn target_cache_reuses_sorted_candidates_until_makefile_modtime_changes() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-helm-make-test-root "cache-lifecycle"))
       (makefile (expand-file-name "Makefile" root))
       (default-directory root)
       (first-time (encode-time 0 0 12 1 1 2025 t))
       (second-time (encode-time 0 0 12 2 1 2025 t))
       (helm-make-directory-functions-list '(helm-make-current-directory))
       (helm-make-completion-method 'ido)
       (helm-make-cache-targets t)
       (helm-make-sort-targets t)
       (helm-make-named-buffer nil)
       (helm-make-comint nil)
       (helm-make-niceness 0)
       (helm-make-nproc 2)
       (helm-make-arguments "-j%d")
       choices-log command-log)
  (unwind-protect
      (progn
        (helm-make-reset-cache)
        (neomacs-helm-make-test-write
         makefile
         "zeta:\n\t@true\nalpha:\n\t@true\nzeta:\n\t@true\n")
        (set-file-times makefile first-time)
        (cl-letf (((symbol-function 'ido-completing-read)
                   (lambda (_prompt choices &rest _)
                     (push (copy-sequence choices) choices-log)
                     (car choices)))
                  ((symbol-function 'compile)
                   (lambda (command _comint)
                     (push (neomacs-helm-make-test-normalize root command)
                           command-log)
                     (get-buffer-create "*compilation*"))))
          (helm-make)
          (neomacs-helm-make-test-write makefile "beta:\n\t@true\n")
          (set-file-times makefile first-time)
          (helm-make)
          (set-file-times makefile second-time)
          (helm-make)
          (helm-make-reset-cache)
          (neomacs-helm-make-test-write
           makefile
           "release:\n\t@true\ngamma:\n\t@true\n")
          (set-file-times makefile second-time)
          (helm-make))
        (list :candidate-sets (nreverse choices-log)
              :commands (nreverse command-log)
              :cache-size (hash-table-count helm-make-db)))
    (neomacs-helm-make-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:candidate-sets (("alpha" "zeta") ("alpha" "zeta") ("beta") ("gamma" "release")) :commands ("make -C <ROOT>/ -j2 alpha" "make -C <ROOT>/ -j2 alpha" "make -C <ROOT>/ -j2 beta" "make -C <ROOT>/ -j2 gamma") :cache-size 1)"#
    ]];
    ParityBatchCase::value(
        "target_cache_reuses_sorted_candidates_until_makefile_modtime_changes",
        elisp_form,
        expected,
    )
}

fn projectile_prefers_custom_ninja_build_directory_and_honors_prefix_jobs() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-helm-make-test-root "projectile-ninja"))
       (custom-build (expand-file-name "out/release/" root))
       (ninja-file (expand-file-name "build.ninja" custom-build))
       (default-directory (expand-file-name "src/" root))
       (neomacs-helm-make-test-project-root root)
       (helm-make-build-dir "out/release")
       (helm-make-completion-method 'ido)
       (helm-make-sort-targets t)
       (helm-make-cache-targets nil)
       (helm-make-ninja-executable "ninja-custom")
       (helm-make-nproc 99)
       (helm-make-arguments "--jobs=%d --verbose")
       (helm-make-comint t)
       (helm-make-named-buffer nil)
       process-call selection compile-call)
  (unwind-protect
      (progn
        (neomacs-helm-make-test-write
         (expand-file-name "Makefile" root)
         "fallback:\n\t@true\n")
        (neomacs-helm-make-test-write
         (expand-file-name "build/Makefile" root)
         "secondary:\n\t@true\n")
        (neomacs-helm-make-test-write ninja-file "# generated ninja graph\n")
        (cl-letf (((symbol-function 'call-process)
                   (lambda (program infile destination display &rest args)
                     (setq process-call
                           (list :program program
                                 :infile infile
                                 :destination destination
                                 :display display
                                 :args args
                                 :directory
                                 (file-relative-name default-directory root)))
                     (when destination
                       (insert "all: phony\ndeploy-prod: phony\nclean: phony\n"))
                     0))
                  ((symbol-function 'ido-completing-read)
                   (lambda (prompt choices &rest _)
                     (setq selection
                           (list :prompt prompt :choices (copy-sequence choices)))
                     "deploy-prod"))
                  ((symbol-function 'compile)
                   (lambda (command comint)
                     (setq compile-call
                           (list :command
                                 (neomacs-helm-make-test-normalize root command)
                                 :comint comint
                                 :directory
                                 (file-relative-name default-directory root)))
                     (get-buffer-create "*compilation*"))))
          (helm-make-projectile -3))
        (list :build-system helm--make-build-system
              :selected-file (file-relative-name ninja-file root)
              :process process-call
              :selection selection
              :command-template
              (neomacs-helm-make-test-normalize root helm-make-command)
              :compile compile-call))
    (neomacs-helm-make-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:build-system ninja :selected-file "out/release/build.ninja" :process (:program "ninja-custom" :infile nil :destination t :display t :args ("-f" "build.ninja" "-t" "targets" "all") :directory "out/release/") :selection (:prompt "Target: " :choices ("all" "clean" "deploy-prod")) :command-template "ninja-custom -C <ROOT>/out/release/ --jobs=3 --verbose %s" :compile (:command "ninja-custom -C <ROOT>/out/release/ --jobs=3 --verbose deploy-prod" :comint t :directory "out/release/"))"#
    ]];
    ParityBatchCase::value(
        "projectile_prefers_custom_ninja_build_directory_and_honors_prefix_jobs",
        elisp_form,
        expected,
    )
}

fn qp_database_workflow_filters_internal_and_non_targets_before_compile() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-helm-make-test-root "qp project"))
       (makefile (expand-file-name "Makefile" root))
       (default-directory root)
       (helm-make-directory-functions-list '(helm-make-current-directory))
       (helm-make-list-target-method 'qp)
       (helm-make-completion-method 'ido)
       (helm-make-sort-targets t)
       (helm-make-cache-targets nil)
       (helm-make-nproc 5)
       (helm-make-arguments "-j%d --output-sync")
       database-command selection compile-call)
  (unwind-protect
      (progn
        (neomacs-helm-make-test-write makefile "release:\n\t@true\n")
        (cl-letf (((symbol-function 'shell-command-to-string)
                   (lambda (command)
                     (setq database-command
                           (neomacs-helm-make-test-normalize root command))
                     (concat
                      "GNU Make database\n# Files\n"
                      "deploy: ; @true\n"
                      "# Not a target:\nphantom:\n"
                      "services/api/.stamp: ; @true\n"
                      ".internal: ; @true\n"
                      "release: ; @true\n"
                      "docs: ; @true\n")))
                  ((symbol-function 'ido-completing-read)
                   (lambda (prompt choices &rest _)
                     (setq selection
                           (list :prompt prompt :choices (copy-sequence choices)))
                     "release"))
                  ((symbol-function 'compile)
                   (lambda (command comint)
                     (setq compile-call
                           (list :command
                                 (neomacs-helm-make-test-normalize root command)
                                 :comint comint))
                     (get-buffer-create "*compilation*"))))
          (helm-make))
        (list :database-command database-command
              :selection selection
              :compile compile-call))
    (neomacs-helm-make-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:database-command "make -f <ROOT>/Makefile -nqp __BASH_MAKE_COMPLETION__=1 .DEFAULT 2>/dev/null" :selection (:prompt "Target: " :choices ("deploy" "docs" "release")) :compile (:command "make -C <ROOT>/ -j5 --output-sync release" :comint nil))"#
    ]];
    ParityBatchCase::value(
        "qp_database_workflow_filters_internal_and_non_targets_before_compile",
        elisp_form,
        expected,
    )
}

fn missing_build_file_reports_public_command_error_before_selection() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-helm-make-test-root "missing-build"))
       (default-directory (expand-file-name "services/api/" root))
       (helm-make-directory-functions-list '(helm-make-current-directory))
       (helm-make-completion-method 'ido)
       (completion-count 0)
       (compile-count 0))
  (unwind-protect
      (progn
        (make-directory default-directory t)
        (cl-letf (((symbol-function 'ido-completing-read)
                   (lambda (&rest _)
                     (setq completion-count (1+ completion-count))))
                  ((symbol-function 'compile)
                   (lambda (&rest _)
                     (setq compile-count (1+ compile-count)))))
          (condition-case error-data
              (list :value (helm-make))
            (error
             (list :signal (car error-data)
                   :data
                   (mapcar
                    (lambda (item)
                      (if (stringp item)
                          (neomacs-helm-make-test-normalize root item)
                        item))
                    (cdr error-data))
                   :message
                   (neomacs-helm-make-test-normalize
                   root (error-message-string error-data))
                   :completion-count completion-count
                   :compile-count compile-count)))))
    (neomacs-helm-make-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:signal error :data ("No build file in <ROOT>/services/api/") :message "No build file in <ROOT>/services/api/" :completion-count 0 :compile-count 0)"#
    ]];
    ParityBatchCase::value(
        "missing_build_file_reports_public_command_error_before_selection",
        elisp_form,
        expected,
    )
}

#[test]
fn helm_make_package_batch() {
    let cases = vec![
        nested_project_build_saves_changed_source_and_launches_selected_target(),
        target_cache_reuses_sorted_candidates_until_makefile_modtime_changes(),
        projectile_prefers_custom_ninja_build_directory_and_honors_prefix_jobs(),
        qp_database_workflow_filters_internal_and_non_targets_before_compile(),
        missing_build_file_reports_public_command_error_before_selection(),
    ];
    assert_oracle_batch_cases(
        helm_make_oracle(),
        "helm-make-package-batch",
        "helm-make",
        &cases,
    );
}
