//! Practical parity for magit-todos scanning and jump.
//!
//! These cases plant a Git repository and an `rg` scanner stand-in, list
//! TODO/FIXME items (including Unicode and Org headings), jump to a
//! source line, record exclude-glob argv, and recover when the scanner
//! fails.

use std::time::Duration;

use expect_test::expect;

use crate::{
    ASYNC_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, HL_TODO_MELPA_PIN,
    MAGIT_MELPA_PIN, MAGIT_TODOS_MELPA_PIN, PCRE2EL_MELPA_PIN, S_MELPA_PIN, TRANSIENT_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'magit)
(require 'hl-todo)
(require 'magit-todos)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")
(setq magit-todos-nice nil
      magit-git-global-arguments
      (append '("-c" "init.defaultBranch=master"
                "-c" "user.name=A U Thor"
                "-c" "user.email=a.u.thor@example.com")
              (and (boundp 'magit-git-global-arguments)
                   magit-git-global-arguments)))
(customize-set-variable 'magit-todos-keywords '("TODO" "FIXME"))
(setq magit-todos-scanner #'magit-todos--scan-with-rg)

(defconst mt456-test-tree
  "807cfa2ba954e06519ee8e587b618a72616dabf2")
(defconst mt456-test-manifest
  '(("magit-todos-pkg.el" . "a726cea2b1982863a5106c0f8b60141f044b32e4f51b9b42d73bf8167b89f1e6")
    ("magit-todos.el" . "b41d9890f0b319936cdb031953bd7fdaf9d2ad3aac98e5998ccd477fb6eb9c4c")))

(defvar mt456-test-case-index 0)
(defvar mt456-test-root nil)

(defun mt456-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun mt456-test-source-state ()
  (let* ((located (locate-library "magit-todos.el"))
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
                         (cons file (mt456-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/magit-todos.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car mt456-test-manifest)))
      (error "Unexpected installed magit-todos payload: %S"
             (or manifest files)))
    (dolist (entry mt456-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (mt456-test-sha file) expected))
          (error "Unexpected installed magit-todos source: %S"
                 (cons entry manifest)))))
    (list :tree mt456-test-tree
          :manifest manifest
          :feature (featurep 'magit-todos)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'magit-todos package-alist)))))))

(defun mt456-test-write (root rel text)
  (let ((file (expand-file-name rel root)))
    (make-directory (file-name-directory file) t)
    (write-region text nil file nil 'silent)
    file))

(defun mt456-test-git (root &rest args)
  (let ((default-directory root))
    (apply #'call-process "git" nil nil nil args)))

(defun mt456-test-item-snapshot (pair)
  (let ((item (cdr pair)))
    (list :display (substring-no-properties (car pair))
          :file (magit-todos-item-filename item)
          :line (magit-todos-item-line item)
          :keyword (magit-todos-item-keyword item)
          :org (magit-todos-item-org-level item)
          :description (magit-todos-item-description item))))

(defun mt456-test-run (planted-output body)
  (let* ((index (cl-incf mt456-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "magit-todos-%d" index) sandbox))))
         (bin (expand-file-name "bin" root))
         (rg (expand-file-name "rg" bin))
         (log (expand-file-name "rg.log" root))
         (path-before (getenv "PATH"))
         (exec-before (copy-sequence exec-path))
         (buffers-before (buffer-list))
         (dir-before default-directory)
         (mt456-test-root root)
         result)
    (unwind-protect
        (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute magit-todos sandbox root"))
          (make-directory bin t)
          (mt456-test-write root "src/app.el"
                            ";;; app.el\n\n;; TODO: ship café\n\n(defun app ())\n\n;; FIXME: handle spaces\n")
          (mt456-test-write root "notes.org" "* TODO write docs\n")
          (mt456-test-write root "tmp/skip.el" ";; TODO: should be excluded\n")
          (mt456-test-write root "bin/rg"
                            (concat "#!/bin/sh\n"
                                    "printf '%s\\n' \"$*\" >> \"$MT456_RG_LOG\"\n"
                                    "printf '%s' \"$MT456_RG_OUTPUT\"\n"
                                    "exit ${MT456_RG_EXIT:-0}\n"))
          (set-file-modes rg #o755)
          (setenv "MT456_RG_LOG" log)
          (setenv "MT456_RG_OUTPUT" planted-output)
          (setenv "MT456_RG_EXIT" "0")
          (setenv "PATH" (concat bin path-separator path-before))
          (setq exec-path (cons bin exec-path)
                default-directory root)
          (mt456-test-git root "init" "--quiet")
          (mt456-test-git root "add" "-A")
          (mt456-test-git root "commit" "--quiet" "-m" "seed")
          (setq result (funcall body root log)))
      (setq default-directory dir-before)
      (setenv "PATH" path-before)
      (setq exec-path exec-before)
      (when (bound-and-true-p magit-todos-mode)
        (magit-todos-mode -1))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (when (buffer-live-p buffer)
            (with-current-buffer buffer
              (set-buffer-modified-p nil))
            (ignore-errors (kill-buffer buffer)))))
      (when (and root (file-exists-p root))
        (ignore-errors (delete-directory root t))))
    result))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAGIT_TODOS_MELPA_PIN, "magit-todos.el")
        .expect("prepare pinned magit-todos source below ./tmp")
        .with_melpa_dependency(MAGIT_MELPA_PIN)
        .expect("prepare pinned magit dependency below ./tmp")
        .with_melpa_dependency(HL_TODO_MELPA_PIN)
        .expect("prepare pinned hl-todo dependency below ./tmp")
        .with_melpa_dependency(PCRE2EL_MELPA_PIN)
        .expect("prepare pinned pcre2el dependency below ./tmp")
        .with_melpa_dependency(ASYNC_MELPA_PIN)
        .expect("prepare pinned async dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_melpa_dependency(TRANSIENT_MELPA_PIN)
        .expect("prepare pinned transient dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn candidates_list_todo_fixme_and_org_heading() -> ParityBatchCase {
    ParityBatchCase::value(
        "candidates_list_todo_fixme_and_org_heading",
        r####"
(mt456-test-run
 "src/app.el:3:;; TODO: ship café\nsrc/app.el:7:;; FIXME: handle spaces\nnotes.org:1:* TODO write docs\n"
 (lambda (_root _log)
   (list :source (mt456-test-source-state)
         :items (mapcar #'mt456-test-item-snapshot (magit-todos-candidates)))))
"####,
        expect![[
            r#"OK (:source (:tree "807cfa2ba954e06519ee8e587b618a72616dabf2" :manifest (("magit-todos-pkg.el" . "a726cea2b1982863a5106c0f8b60141f044b32e4f51b9b42d73bf8167b89f1e6") ("magit-todos.el" . "b41d9890f0b319936cdb031953bd7fdaf9d2ad3aac98e5998ccd477fb6eb9c4c")) :feature t :version "20250928.1611") :items ((:display "src/app.el TODO: ship café" :file "src/app.el" :line 3 :keyword "TODO" :org nil :description "ship café") (:display "src/app.el FIXME: handle spaces" :file "src/app.el" :line 7 :keyword "FIXME" :org nil :description "handle spaces") (:display "notes.org * TODO write docs" :file "notes.org" :line 1 :keyword "TODO" :org "*" :description "write docs")))"#
        ]],
    )
}

fn jump_to_item_opens_the_source_keyword() -> ParityBatchCase {
    ParityBatchCase::value(
        "jump_to_item_opens_the_source_keyword",
        r####"
(mt456-test-run
 "src/app.el:3:;; TODO: ship café\n"
 (lambda (_root _log)
   (let* ((pair (car (magit-todos-candidates)))
          (item (cdr pair)))
     (save-window-excursion
       (magit-todos-jump-to-item :item item)
       (list :source (mt456-test-source-state)
             :file (file-relative-name (buffer-file-name) mt456-test-root)
             :line (line-number-at-pos)
             :text (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position)))))))
"####,
        expect![[
            r#"OK (:source (:tree "807cfa2ba954e06519ee8e587b618a72616dabf2" :manifest (("magit-todos-pkg.el" . "a726cea2b1982863a5106c0f8b60141f044b32e4f51b9b42d73bf8167b89f1e6") ("magit-todos.el" . "b41d9890f0b319936cdb031953bd7fdaf9d2ad3aac98e5998ccd477fb6eb9c4c")) :feature t :version "20250928.1611") :file "src/app.el" :line 3 :text ";; TODO: ship café")"#
        ]],
    )
}

fn exclude_globs_are_passed_to_the_scanner() -> ParityBatchCase {
    ParityBatchCase::value(
        "exclude_globs_are_passed_to_the_scanner",
        r####"
(let ((magit-todos-exclude-globs '("tmp/*" "*.elc")))
  (mt456-test-run
   "src/app.el:3:;; TODO: ship café\n"
   (lambda (_root log)
     (magit-todos-candidates)
     (list :source (mt456-test-source-state)
           :argv (with-temp-buffer
                   (insert-file-contents log)
                   (split-string (buffer-string)))))))
"####,
        expect![[
            r#"OK (:source (:tree "807cfa2ba954e06519ee8e587b618a72616dabf2" :manifest (("magit-todos-pkg.el" . "a726cea2b1982863a5106c0f8b60141f044b32e4f51b9b42d73bf8167b89f1e6") ("magit-todos.el" . "b41d9890f0b319936cdb031953bd7fdaf9d2ad3aac98e5998ccd477fb6eb9c4c")) :feature t :version "20250928.1611") :argv ("--no-heading" "--line-number" "--glob" "!tmp/*" "--glob" "!*.elc" "^(\\*+)[[:blank:]]+(D(?:EBUG|ONT)|F(?:AIL|IXME)|H(?:ACK|OLD)|KLUDGE|MAYBE|NEXT|OKAY|PROG|T(?:EMP|HEM|ODO)|WIP|XXXX\\*)[[:space:]]+(.+)|(?:^|[[:blank:]]+)(D(?:EBUG|ONT)|F(?:AIL|IXME)|H(?:ACK|OLD)|KLUDGE|MAYBE|NEXT|OKAY|PROG|T(?:EMP|HEM|ODO)|WIP|XXXX\\*)(?:[\\[(][^\\])]+[)\\]])?:(?:[[:blank:]]+(.+))?"))"#
        ]],
    )
}

fn mode_toggles_status_hook_and_failed_scan_signals() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_toggles_status_hook_and_failed_scan_signals",
        r####"
(mt456-test-run
 "src/app.el:3:;; TODO: ship café\n"
 (lambda (_root _log)
   (let ((before (memq #'magit-todos--insert-todos magit-status-sections-hook)))
     (magit-todos-mode 1)
     (let ((enabled (and (memq #'magit-todos--insert-todos
                               magit-status-sections-hook)
                         t)))
       (magit-todos-mode -1)
       (setenv "MT456_RG_EXIT" "2")
       (let ((failed
              (condition-case err
                  (magit-todos-candidates)
                (error (list (car err)
                             (substring-no-properties
                              (error-message-string err)))))))
         (list :source (mt456-test-source-state)
               :before before
               :enabled enabled
               :after (memq #'magit-todos--insert-todos
                            magit-status-sections-hook)
               :failed failed))))))
"####,
        expect![[
            r#"OK (:source (:tree "807cfa2ba954e06519ee8e587b618a72616dabf2" :manifest (("magit-todos-pkg.el" . "a726cea2b1982863a5106c0f8b60141f044b32e4f51b9b42d73bf8167b89f1e6") ("magit-todos.el" . "b41d9890f0b319936cdb031953bd7fdaf9d2ad3aac98e5998ccd477fb6eb9c4c")) :feature t :version "20250928.1611") :before nil :enabled t :after nil :failed (user-error "rg failed"))"#
        ]],
    )
}

#[test]
fn magit_todos_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        candidates_list_todo_fixme_and_org_heading(),
        jump_to_item_opens_the_source_keyword(),
        exclude_globs_are_passed_to_the_scanner(),
        mode_toggles_status_hook_and_failed_scan_signals(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "magit-todos-rank456",
        "magit_todos_parity",
        &cases,
    );
}
