//! Practical parity for Ecukes' public Cucumber workflows.
//!
//! These cases scaffold a real project, parse outlines/tables/py-strings,
//! load support plus Espuds steps, run the spec reporter, select scenarios
//! by tags and patterns, and recover after an exact failing step.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ECUKES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'ansi-color)
(require 'ecukes)
(require 'ecukes-new)
(require 'ecukes-hooks)
(require 'ecukes-reporter)
(require 'espuds)
(set-window-configuration (current-window-configuration))
(ecukes-reporter-use "spec")

(defconst ecukes423-test-tree
  "fa3fefe477c795ba762eb0d290e7a4e5cf3afc50")
(defconst ecukes423-test-manifest
  '(("ecukes-byte-compile.el" . "3a61aebb1cb4039f0f9676c9a84535fe90d86cbf28f28a8e270980df974ff083")
    ("ecukes-cli.el" . "a99d493c04c55fe38895ca71672203cf143cb3b7cbb2bcf3fa1171afe84b5f4d")
    ("ecukes-core.el" . "6a06dae23e30bb9924802eb7e92b1928c3a19cdbfc25176537f37ff9d9dc04c4")
    ("ecukes-def.el" . "5a36a9b960275c137278b90c6280bf3bcc31bb02d040b9420121a9a12bbb0589")
    ("ecukes-helpers.el" . "22e38026ca1fba1abd6225c897149f460b668e2052f1c4e08712bb04e49fa45f")
    ("ecukes-hooks.el" . "af672b67acea4d661168026caf1f209f464fb84470a54a34395da030bf50167b")
    ("ecukes-load.el" . "6b147fb7bbbb3dc33c7bf5a9a29625ef3dd75d6bf2f6cc62f449ea0fa095e44f")
    ("ecukes-new.el" . "b0e20f13c55fb83436bd1937afe8a2fdb22c8f9a5b3adab00f74dfef6efc66c0")
    ("ecukes-parse.el" . "e4f9e5afc36eaaa05ff4f4440f7c35857fe81995a910deef5e20e5a4ba36d6e6")
    ("ecukes-pkg.el" . "71e32c571d9041cde08bf17ed6908f6dcd89734e571275a23ab3f0359556d74e")
    ("ecukes-project.el" . "2113f2da42bd80ada38940f012813506b05487f020908ad5c68fff949767fa99")
    ("ecukes-reporter.el" . "69ed3caca958ae2291de64691cf82cb12cc7dc341c46ed374b2b144157820e55")
    ("ecukes-run.el" . "80e0e6f42defbc25be9ddd946a7c3927a5f5f50b1607e2a267881e3a8daeea56")
    ("ecukes-stats.el" . "903947752f3e25437bb3d0acbeb9f737e0a94f41465ac3af9c4bf349b951dcaf")
    ("ecukes-steps.el" . "97e3f7af649868780bac7c827278c2fc3355a6c2dffb7458d8dbac3a44c09856")
    ("ecukes-template.el" . "5a6e2b7683be2f8ddfc9fe98fcee240893d5cca32d503ed3377e2e9499695d20")
    ("ecukes.el" . "3f06d0bb38c381ef8c1d3a178cae43dc5aab7c810d75215b212e4fe8b9b95c91")
    ("reporters/ecukes-reporter-dot.el" . "74a05c40c649b0d26d93463e547054600c87f7d2decd80eba7fb7f9d7ae042ef")
    ("reporters/ecukes-reporter-landing.el" . "2834a917965a20d370085cc4b144944238c53324ca68385e658275c8276f88cc")
    ("reporters/ecukes-reporter-magnars.el" . "2a14acf31d7851b7812b75ac02732f385c0087e40d92c7f93264337ebfc2311e")
    ("reporters/ecukes-reporter-progress.el" . "24852648b81afaa0cfa09728b2078543a22626b70b4998afdcdf4f71257ae77b")
    ("reporters/ecukes-reporter-spec.el" . "7d46313f218eb4599b0ca51d3331da6059cc3f5f8b562cd4dc783fd429cb4168")))

(defvar ecukes423-test-case-index 0)
(defvar ecukes423-test-root nil)
(defvar ecukes423-test-root-owned nil)
(defvar ecukes423-test-ledger nil)
(defvar ecukes423-test-stock nil)

(defun ecukes423-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ecukes423-test-source-state ()
  (let* ((located (symbol-file 'ecukes 'defun))
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
                         (cons file (ecukes423-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car ecukes423-test-manifest)))
      (error "Unexpected installed Ecukes payload: %S" (or manifest files)))
    (dolist (entry ecukes423-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (ecukes423-test-sha file) (cdr entry)))
          (error "Unexpected installed Ecukes source: %S" (cons entry manifest)))))
    (list :tree ecukes423-test-tree
          :manifest ecukes423-test-manifest
          :feature (featurep 'ecukes)
          :espuds (featurep 'espuds)
          :version "20241226.1759"
          :reporters (mapcar #'car ecukes-reporters))))

(defun ecukes423-test-window-state ()
  (mapcar
   (lambda (window)
     (list window
           (eq window (selected-window))
           (window-buffer window)
           (window-point window)
           (window-start window)
           (window-hscroll window)
           (window-dedicated-p window)
           (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun ecukes423-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (ecukes423-test-plain item)
                             (copy-tree item)))
                         (cdr condition))
           :message (ecukes423-test-plain (error-message-string condition))))))

(defun ecukes423-test-plain (string)
  (copy-sequence (ansi-color-filter-apply (or string ""))))

(defun ecukes423-test-report ()
  (apply #'concat
         (mapcar
          (lambda (entry)
            (ecukes423-test-plain (format "%s" (cdr entry))))
          (seq-filter (lambda (entry) (eq (car entry) 'princ))
                      ecukes-internal-message-log))))

(defun ecukes423-test-stats ()
  (list :scenarios ecukes-stats-scenarios
        :scenarios-passed ecukes-stats-scenarios-passed
        :scenarios-failed ecukes-stats-scenarios-failed
        :steps ecukes-stats-steps
        :steps-passed ecukes-stats-steps-passed
        :steps-failed ecukes-stats-steps-failed
        :steps-skipped ecukes-stats-steps-skipped))

(defun ecukes423-test-step (step)
  (list :head (ecukes-step-head step)
        :body (copy-sequence (ecukes-step-body step))
        :type (ecukes-step-type step)
        :arg (copy-tree (ecukes-step-arg step))
        :status (ecukes-step-status step)
        :err (and (ecukes-step-err step)
                  (ecukes423-test-plain (ecukes-step-err step)))))

(defun ecukes423-test-scenario (scenario)
  (list :name (copy-sequence (ecukes-scenario-name scenario))
        :tags (mapcar #'copy-sequence (ecukes-scenario-tags scenario))
        :steps (mapcar #'ecukes423-test-step (ecukes-scenario-steps scenario))))

(defun ecukes423-test-feature (feature)
  (let ((intro (ecukes-feature-intro feature))
        (background (ecukes-feature-background feature)))
    (list :intro
          (and intro
               (list :header (copy-sequence (ecukes-intro-header intro))
                     :description (mapcar #'copy-sequence
                                          (ecukes-intro-description intro))))
          :background
          (and background
               (mapcar #'ecukes423-test-step
                       (ecukes-background-steps background)))
          :outlines
          (mapcar
           (lambda (outline)
             (list :name (copy-sequence (ecukes-outline-name outline))
                   :tags (mapcar #'copy-sequence (ecukes-outline-tags outline))
                   :table (copy-tree (ecukes-outline-table outline))
                   :steps (mapcar #'ecukes423-test-step
                                  (ecukes-outline-steps outline))))
           (ecukes-feature-outlines feature))
          :scenarios (mapcar #'ecukes423-test-scenario
                             (ecukes-feature-scenarios feature)))))

(defun ecukes423-test-tree-listing (directory)
  (when (file-directory-p directory)
    (sort
     (mapcar (lambda (file) (file-relative-name file directory))
             (directory-files-recursively directory "" nil))
     #'string<)))

(defun ecukes423-test-read (relative)
  (let ((file (expand-file-name relative ecukes423-test-root)))
    (unless (file-in-directory-p file ecukes423-test-root)
      (error "Refusing Ecukes read outside owned root: %S" file))
    (when (file-exists-p file)
      (with-temp-buffer
        (insert-file-contents file)
        (buffer-string)))))

(defun ecukes423-test-write (relative contents)
  (let ((file (expand-file-name relative ecukes423-test-root)))
    (unless (and ecukes423-test-root-owned
                 (file-in-directory-p file ecukes423-test-root))
      (error "Refusing Ecukes write outside owned root: %S" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix)
          (enable-local-variables nil))
      (with-temp-file file (insert contents)))
    file))

(defun ecukes423-test-forbid-external (operation &rest arguments)
  (error "Unexpected Ecukes external boundary: %S %S" operation arguments))

(defun ecukes423-test-reset-run-state ()
  (ecukes-stats-reset)
  (setq ecukes-internal-message-log nil
        ecukes-message-log nil
        ecukes-reporter-failed-scenarios nil
        ecukes423-test-ledger nil
        ecukes423-test-stock nil))

(defun ecukes423-test-run (body)
  (let* ((index (cl-incf ecukes423-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "ecukes-%d" index) sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (ecukes423-test-window-state))
         (source-before (ecukes423-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (steps-before ecukes-steps-definitions)
         (before-hooks-before ecukes-hooks-before)
         (after-hooks-before ecukes-hooks-after)
         (setup-hooks-before ecukes-hooks-setup)
         (teardown-hooks-before ecukes-hooks-teardown)
         (fail-hooks-before ecukes-hooks-fail)
         (include-before ecukes-include-tags)
         (exclude-before ecukes-exclude-tags)
         (patterns-before ecukes-patterns)
         (anti-before ecukes-anti-patterns)
         (only-failing-before ecukes-only-failing)
         (failed-before ecukes-reporter-failed-scenarios)
         (internal-log-before ecukes-internal-message-log)
         (message-log-before ecukes-message-log)
         (stats-before (ecukes423-test-stats))
         (ecukes423-test-root root)
         (ecukes423-test-root-owned nil)
         result source-after cleanup-errors)
    (unwind-protect
        (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute Ecukes sandbox root"))
          (when (file-exists-p root)
            (error "Ecukes sandbox root exists: %S" root))
          (make-directory root)
          (setq ecukes423-test-root-owned t
                enable-local-variables nil
                debug-on-error nil
                print-circle nil)
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external 'call-process args)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external
                              'call-process-region args)))
                    ((symbol-function 'process-file)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external 'process-file args)))
                    ((symbol-function 'start-process)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external 'start-process args)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external
                              'start-file-process args)))
                    ((symbol-function 'make-process)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external 'make-process args)))
                    ((symbol-function 'make-network-process)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external
                              'make-network-process args)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external 'url-retrieve args)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external
                              'url-retrieve-synchronously args)))
                    ((symbol-function 'kill-emacs)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external 'kill-emacs args)))
                    ((symbol-function 'ecukes-quit)
                     (lambda (&rest args)
                       (apply #'ecukes423-test-forbid-external 'ecukes-quit args))))
            (setq result (funcall body)))
          (setq source-after (ecukes423-test-source-state))
          (unless (equal source-before source-after)
            (error "Ecukes source changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (setq ecukes-steps-definitions steps-before
              ecukes-hooks-before before-hooks-before
              ecukes-hooks-after after-hooks-before
              ecukes-hooks-setup setup-hooks-before
              ecukes-hooks-teardown teardown-hooks-before
              ecukes-hooks-fail fail-hooks-before
              ecukes-include-tags include-before
              ecukes-exclude-tags exclude-before
              ecukes-patterns patterns-before
              ecukes-anti-patterns anti-before
              ecukes-only-failing only-failing-before
              ecukes-reporter-failed-scenarios failed-before
              ecukes-internal-message-log internal-log-before
              ecukes-message-log message-log-before
              ecukes-stats-steps (plist-get stats-before :steps)
              ecukes-stats-steps-passed (plist-get stats-before :steps-passed)
              ecukes-stats-steps-failed (plist-get stats-before :steps-failed)
              ecukes-stats-steps-skipped (plist-get stats-before :steps-skipped)
              ecukes-stats-scenarios (plist-get stats-before :scenarios)
              ecukes-stats-scenarios-passed
              (plist-get stats-before :scenarios-passed)
              ecukes-stats-scenarios-failed
              (plist-get stats-before :scenarios-failed)
              default-directory directory-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              ecukes423-test-ledger nil
              ecukes423-test-stock nil)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (with-current-buffer buffer
                         (let ((kill-buffer-hook nil)
                               (kill-buffer-query-functions nil))
                           (set-buffer-modified-p nil)
                           (kill-buffer buffer)))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when ecukes423-test-root-owned
          (attempt 'sandbox (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :steps-restored (eq ecukes-steps-definitions steps-before)
                 :hooks-restored
                 (and (eq ecukes-hooks-before before-hooks-before)
                      (eq ecukes-hooks-after after-hooks-before)
                      (eq ecukes-hooks-setup setup-hooks-before)
                      (eq ecukes-hooks-teardown teardown-hooks-before)
                      (eq ecukes-hooks-fail fail-hooks-before))
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (ecukes423-test-window-state) window-state-before))
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Ecukes cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ECUKES_MELPA_PIN, "ecukes.el")
        .expect("prepare exact Ecukes source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_new_scaffolds_project_files_and_refuses_a_second_setup() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_new_scaffolds_project_files_and_refuses_a_second_setup",
        r####"
(ecukes423-test-run
 (lambda ()
   (let* ((project (expand-file-name "cafe-ledger" ecukes423-test-root))
          (ecukes-new-features-path (expand-file-name "features" project))
          (ecukes-new-project-name "cafe-ledger")
          created second recovered)
     (make-directory project t)
     (let ((default-directory project))
       (ecukes423-test-reset-run-state)
       (ecukes-new)
       (setq created
             (list :tree (ecukes423-test-tree-listing project)
                   :feature-matches
                   (equal (ecukes423-test-read "cafe-ledger/features/cafe-ledger.feature")
                          (ecukes-template-get 'feature))
                   :steps-match
                   (equal (ecukes423-test-read "cafe-ledger/features/step-definitions/cafe-ledger-steps.el")
                          (ecukes-template-get 'step-definition))
                   :env-matches
                   (equal (ecukes423-test-read "cafe-ledger/features/support/env.el")
                          (ecukes-template-get
                           'env '(("project-name" . "cafe-ledger"))))
                   :feature (ecukes423-test-read "cafe-ledger/features/cafe-ledger.feature")
                   :steps (ecukes423-test-read "cafe-ledger/features/step-definitions/cafe-ledger-steps.el")
                   :env (ecukes423-test-read "cafe-ledger/features/support/env.el")
                   :messages
                   (mapcar (lambda (entry)
                             (ecukes423-test-plain (format "%s" (cdr entry))))
                           ecukes-internal-message-log)))
       (setq second
             (ecukes423-test-condition
              (lambda ()
                (let ((ecukes-new-features-path (expand-file-name "features" project))
                      (ecukes-new-project-name "cafe-ledger"))
                  (ecukes-new)))))
       (delete-directory (expand-file-name "features" project) t)
       (ecukes423-test-reset-run-state)
       (let ((ecukes-new-features-path (expand-file-name "features" project))
             (ecukes-new-project-name "cafe-ledger"))
         (ecukes-new))
       (setq recovered (ecukes423-test-tree-listing project)))
     (list :created created :second second :recovered recovered))))
"####,
        expect![[
            r#"OK (:source (:tree "fa3fefe477c795ba762eb0d290e7a4e5cf3afc50" :manifest (("ecukes-byte-compile.el" . "3a61aebb1cb4039f0f9676c9a84535fe90d86cbf28f28a8e270980df974ff083") ("ecukes-cli.el" . "a99d493c04c55fe38895ca71672203cf143cb3b7cbb2bcf3fa1171afe84b5f4d") ("ecukes-core.el" . "6a06dae23e30bb9924802eb7e92b1928c3a19cdbfc25176537f37ff9d9dc04c4") ("ecukes-def.el" . "5a36a9b960275c137278b90c6280bf3bcc31bb02d040b9420121a9a12bbb0589") ("ecukes-helpers.el" . "22e38026ca1fba1abd6225c897149f460b668e2052f1c4e08712bb04e49fa45f") ("ecukes-hooks.el" . "af672b67acea4d661168026caf1f209f464fb84470a54a34395da030bf50167b") ("ecukes-load.el" . "6b147fb7bbbb3dc33c7bf5a9a29625ef3dd75d6bf2f6cc62f449ea0fa095e44f") ("ecukes-new.el" . "b0e20f13c55fb83436bd1937afe8a2fdb22c8f9a5b3adab00f74dfef6efc66c0") ("ecukes-parse.el" . "e4f9e5afc36eaaa05ff4f4440f7c35857fe81995a910deef5e20e5a4ba36d6e6") ("ecukes-pkg.el" . "71e32c571d9041cde08bf17ed6908f6dcd89734e571275a23ab3f0359556d74e") ("ecukes-project.el" . "2113f2da42bd80ada38940f012813506b05487f020908ad5c68fff949767fa99") ("ecukes-reporter.el" . "69ed3caca958ae2291de64691cf82cb12cc7dc341c46ed374b2b144157820e55") ("ecukes-run.el" . "80e0e6f42defbc25be9ddd946a7c3927a5f5f50b1607e2a267881e3a8daeea56") ("ecukes-stats.el" . "903947752f3e25437bb3d0acbeb9f737e0a94f41465ac3af9c4bf349b951dcaf") ("ecukes-steps.el" . "97e3f7af649868780bac7c827278c2fc3355a6c2dffb7458d8dbac3a44c09856") ("ecukes-template.el" . "5a6e2b7683be2f8ddfc9fe98fcee240893d5cca32d503ed3377e2e9499695d20") ("ecukes.el" . "3f06d0bb38c381ef8c1d3a178cae43dc5aab7c810d75215b212e4fe8b9b95c91") ("reporters/ecukes-reporter-dot.el" . "74a05c40c649b0d26d93463e547054600c87f7d2decd80eba7fb7f9d7ae042ef") ("reporters/ecukes-reporter-landing.el" . "2834a917965a20d370085cc4b144944238c53324ca68385e658275c8276f88cc") ("reporters/ecukes-reporter-magnars.el" . "2a14acf31d7851b7812b75ac02732f385c0087e40d92c7f93264337ebfc2311e") ("reporters/ecukes-reporter-progress.el" . "24852648b81afaa0cfa09728b2078543a22626b70b4998afdcdf4f71257ae77b") ("reporters/ecukes-reporter-spec.el" . "7d46313f218eb4599b0ca51d3331da6059cc3f5f8b562cd4dc783fd429cb4168")) :feature t :espuds t :version "20241226.1759" :reporters (landing magnars progress spec dot)) :result (:created (:tree ("features/cafe-ledger.feature" "features/step-definitions/cafe-ledger-steps.el" "features/support/env.el") :feature-matches t :steps-match t :env-matches t :feature "Feature: Do Some things\n  In order to do something\n  As a user\n  I want to do something\n\n  Scenario: Do Something\n    Given I have \"something\"\n    When I have \"something\"\n    Then I should have \"something\"\n    And I should have \"something\"\n    But I should not have \"something\"\n" :steps ";; This file contains your project specific step definitions. All\n;; files in this directory whose names end with \"-steps.el\" will be\n;; loaded automatically by Ecukes.\n\n(Given \"^I have \\\"\\\\(.+\\\\)\\\"$\"\n  (lambda (something)\n    ;; ...\n    ))\n\n(When \"^I have \\\"\\\\(.+\\\\)\\\"$\"\n  (lambda (something)\n    ;; ...\n    ))\n\n(Then \"^I should have \\\"\\\\(.+\\\\)\\\"$\"\n  (lambda (something)\n    ;; ...\n    ))\n\n(And \"^I have \\\"\\\\(.+\\\\)\\\"$\"\n  (lambda (something)\n    ;; ...\n    ))\n\n(But \"^I should not have \\\"\\\\(.+\\\\)\\\"$\"\n  (lambda (something)\n    ;; ...\n    ))\n" :env "(require 'f)\n\n(defvar cafe-ledger-support-path\n  (f-dirname load-file-name))\n\n(defvar cafe-ledger-features-path\n  (f-parent cafe-ledger-support-path))\n\n(defvar cafe-ledger-root-path\n  (f-parent cafe-ledger-features-path))\n\n(add-to-list 'load-path cafe-ledger-root-path)\n\n;; Ensure that we don't load old byte-compiled versions\n(let ((load-prefer-newer t))\n  (require 'cafe-ledger)\n  (require 'espuds)\n  (require 'ert))\n\n(Setup\n ;; Before anything has run\n )\n\n(Before\n ;; Before each scenario is run\n )\n\n(After\n ;; After each scenario is run\n )\n\n(Teardown\n ;; After when everything has been run\n )\n" :messages ("create features\n" "create   step-definition\n" "create     cafe-ledger-steps.el\n" "create   support\n" "create     env.el\n" "create   cafe-ledger.feature\n")) :second (:error error :data ("Ecukes already exists for this project") :message "Ecukes already exists for this project") :recovered ("features/cafe-ledger.feature" "features/step-definitions/cafe-ledger-steps.el" "features/support/env.el")) :cleanup (:source-unchanged t :steps-restored t :hooks-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_parser_expands_outlines_tables_pystrings_tags_and_unicode() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_parser_expands_outlines_tables_pystrings_tags_and_unicode",
        r####"
(ecukes423-test-run
 (lambda ()
   (let* ((project (expand-file-name "cafe-ledger" ecukes423-test-root))
          (feature
           (ecukes423-test-write
            "cafe-ledger/features/stock.feature"
            "@inventory
Feature: Café ledger 界
  In order to track café stock
  As a barista
  I want substitutions

  Background:
    Given I start with 2 bags

  @keep
  Scenario: Sell a bag
    When I sell 1 bag
    Then I should have 1 bags

  @skip
  Scenario: Ignored path
    Then this should not run

  @keep
  Scenario Outline: Restock
    When I restock <count> of <item>
    Then the stock should mention \"<item>\"
    And I record:
      \"\"\"
      item: <item>
      note: café 界
      \"\"\"
    And I tabulate:
      | sku    | qty     |
      | <item> | <count> |

    Examples:
      | item  | count |
      | beans | 3     |
      | cups  | 5     |
")))
     (list :parsed (ecukes423-test-feature (ecukes-parse-feature feature))
           :project-name
           (let ((default-directory project))
             (ecukes-project-name))))))
"####,
        expect![[
            r#"OK (:source (:tree "fa3fefe477c795ba762eb0d290e7a4e5cf3afc50" :manifest (("ecukes-byte-compile.el" . "3a61aebb1cb4039f0f9676c9a84535fe90d86cbf28f28a8e270980df974ff083") ("ecukes-cli.el" . "a99d493c04c55fe38895ca71672203cf143cb3b7cbb2bcf3fa1171afe84b5f4d") ("ecukes-core.el" . "6a06dae23e30bb9924802eb7e92b1928c3a19cdbfc25176537f37ff9d9dc04c4") ("ecukes-def.el" . "5a36a9b960275c137278b90c6280bf3bcc31bb02d040b9420121a9a12bbb0589") ("ecukes-helpers.el" . "22e38026ca1fba1abd6225c897149f460b668e2052f1c4e08712bb04e49fa45f") ("ecukes-hooks.el" . "af672b67acea4d661168026caf1f209f464fb84470a54a34395da030bf50167b") ("ecukes-load.el" . "6b147fb7bbbb3dc33c7bf5a9a29625ef3dd75d6bf2f6cc62f449ea0fa095e44f") ("ecukes-new.el" . "b0e20f13c55fb83436bd1937afe8a2fdb22c8f9a5b3adab00f74dfef6efc66c0") ("ecukes-parse.el" . "e4f9e5afc36eaaa05ff4f4440f7c35857fe81995a910deef5e20e5a4ba36d6e6") ("ecukes-pkg.el" . "71e32c571d9041cde08bf17ed6908f6dcd89734e571275a23ab3f0359556d74e") ("ecukes-project.el" . "2113f2da42bd80ada38940f012813506b05487f020908ad5c68fff949767fa99") ("ecukes-reporter.el" . "69ed3caca958ae2291de64691cf82cb12cc7dc341c46ed374b2b144157820e55") ("ecukes-run.el" . "80e0e6f42defbc25be9ddd946a7c3927a5f5f50b1607e2a267881e3a8daeea56") ("ecukes-stats.el" . "903947752f3e25437bb3d0acbeb9f737e0a94f41465ac3af9c4bf349b951dcaf") ("ecukes-steps.el" . "97e3f7af649868780bac7c827278c2fc3355a6c2dffb7458d8dbac3a44c09856") ("ecukes-template.el" . "5a6e2b7683be2f8ddfc9fe98fcee240893d5cca32d503ed3377e2e9499695d20") ("ecukes.el" . "3f06d0bb38c381ef8c1d3a178cae43dc5aab7c810d75215b212e4fe8b9b95c91") ("reporters/ecukes-reporter-dot.el" . "74a05c40c649b0d26d93463e547054600c87f7d2decd80eba7fb7f9d7ae042ef") ("reporters/ecukes-reporter-landing.el" . "2834a917965a20d370085cc4b144944238c53324ca68385e658275c8276f88cc") ("reporters/ecukes-reporter-magnars.el" . "2a14acf31d7851b7812b75ac02732f385c0087e40d92c7f93264337ebfc2311e") ("reporters/ecukes-reporter-progress.el" . "24852648b81afaa0cfa09728b2078543a22626b70b4998afdcdf4f71257ae77b") ("reporters/ecukes-reporter-spec.el" . "7d46313f218eb4599b0ca51d3331da6059cc3f5f8b562cd4dc783fd429cb4168")) :feature t :espuds t :version "20241226.1759" :reporters (landing magnars progress spec dot)) :result (:parsed (:intro (:header "Café ledger 界" :description ("In order to track café stock" "As a barista" "I want substitutions")) :background ((:head "Given" :body "I start with 2 bags" :type regular :arg nil :status nil :err nil)) :outlines ((:name "Restock" :tags ("keep") :table (("item" "count") ("beans" "3") ("cups" "5")) :steps ((:head "When" :body "I restock <count> of <item>" :type regular :arg nil :status nil :err nil) (:head "Then" :body "the stock should mention \"<item>\"" :type regular :arg nil :status nil :err nil) (:head "And" :body "I record:" :type py-string :arg "item: <item>\nnote: café 界" :status nil :err nil) (:head "And" :body "I tabulate:" :type table :arg (("sku" "qty") ("<item>" "<count>")) :status nil :err nil)))) :scenarios ((:name "Sell a bag" :tags ("inventory" "keep") :steps ((:head "When" :body "I sell 1 bag" :type regular :arg nil :status nil :err nil) (:head "Then" :body "I should have 1 bags" :type regular :arg nil :status nil :err nil))) (:name "Ignored path" :tags ("inventory" "skip") :steps ((:head "Then" :body "this should not run" :type regular :arg nil :status nil :err nil))) (:name "Restock" :tags ("inventory" "keep") :steps ((:head "When" :body "I restock 3 of beans" :type regular :arg nil :status nil :err nil) (:head "Then" :body "the stock should mention \"beans\"" :type regular :arg nil :status nil :err nil) (:head "And" :body "I record:" :type py-string :arg "item: beans\nnote: café 界" :status nil :err nil) (:head "And" :body "I tabulate:" :type table :arg (("sku" "qty") ("beans" "3")) :status nil :err nil))) (:name "Restock" :tags ("inventory" "keep") :steps ((:head "When" :body "I restock 5 of cups" :type regular :arg nil :status nil :err nil) (:head "Then" :body "the stock should mention \"cups\"" :type regular :arg nil :status nil :err nil) (:head "And" :body "I record:" :type py-string :arg "item: cups\nnote: café 界" :status nil :err nil) (:head "And" :body "I tabulate:" :type table :arg (("sku" "qty") ("cups" "5")) :status nil :err nil))))) :project-name "cafe-ledger") :cleanup (:source-unchanged t :steps-restored t :hooks-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_load_run_executes_hooks_espuds_steps_and_spec_reporter() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_load_run_executes_hooks_espuds_steps_and_spec_reporter",
        r####"
(ecukes423-test-run
 (lambda ()
   (let* ((project (expand-file-name "cafe-ledger" ecukes423-test-root))
          (ecukes-new-features-path (expand-file-name "features" project))
          (ecukes-new-project-name "cafe-ledger"))
     (make-directory project t)
     (let ((default-directory project))
       (ecukes-new)
       (ecukes423-test-write
        "cafe-ledger/cafe-ledger.el"
        ";;; -*- lexical-binding: t; -*-\n(defun cafe-ledger-label (item) (format \"%s 界\" item))\n(provide 'cafe-ledger)\n")
       (ecukes423-test-write
        "cafe-ledger/features/cafe-ledger.feature"
        "Feature: Café counter
  In order to greet guests
  As a barista
  I want the buffer to show the special

  Background:
    Given I switch to buffer \"*cafe-ledger*\"
    And I clear the buffer

  Scenario: Insert the special
    When I insert \"café 界\"
    Then I should see \"café 界\"
    And I remember the special
")
       (ecukes423-test-write
        "cafe-ledger/features/step-definitions/cafe-ledger-steps.el"
        ";;; -*- lexical-binding: t; -*-\n(Then \"^I remember the special$\"
  (lambda ()
    (push (cons 'remembered (buffer-string)) ecukes423-test-ledger)))\n")
       (ecukes423-test-reset-run-state)
       (Setup (push 'setup ecukes423-test-ledger))
       (Before (push 'before ecukes423-test-ledger))
       (After (push 'after ecukes423-test-ledger))
       (Teardown (push 'teardown ecukes423-test-ledger))
       (let ((warning-minimum-level :error)
             (warning-minimum-log-level :error))
         (ecukes-load))
       (ecukes-run (list (expand-file-name "features/cafe-ledger.feature" project)))
       (list :tree (ecukes423-test-tree-listing project)
             :stats (ecukes423-test-stats)
             :hooks (reverse ecukes423-test-ledger)
             :report (ecukes423-test-report)
             :failing-file
             (and (file-exists-p (expand-file-name ".ecukes-failing-scenarios" project))
                  t))))))
"####,
        expect![[
            r#"OK (:source (:tree "fa3fefe477c795ba762eb0d290e7a4e5cf3afc50" :manifest (("ecukes-byte-compile.el" . "3a61aebb1cb4039f0f9676c9a84535fe90d86cbf28f28a8e270980df974ff083") ("ecukes-cli.el" . "a99d493c04c55fe38895ca71672203cf143cb3b7cbb2bcf3fa1171afe84b5f4d") ("ecukes-core.el" . "6a06dae23e30bb9924802eb7e92b1928c3a19cdbfc25176537f37ff9d9dc04c4") ("ecukes-def.el" . "5a36a9b960275c137278b90c6280bf3bcc31bb02d040b9420121a9a12bbb0589") ("ecukes-helpers.el" . "22e38026ca1fba1abd6225c897149f460b668e2052f1c4e08712bb04e49fa45f") ("ecukes-hooks.el" . "af672b67acea4d661168026caf1f209f464fb84470a54a34395da030bf50167b") ("ecukes-load.el" . "6b147fb7bbbb3dc33c7bf5a9a29625ef3dd75d6bf2f6cc62f449ea0fa095e44f") ("ecukes-new.el" . "b0e20f13c55fb83436bd1937afe8a2fdb22c8f9a5b3adab00f74dfef6efc66c0") ("ecukes-parse.el" . "e4f9e5afc36eaaa05ff4f4440f7c35857fe81995a910deef5e20e5a4ba36d6e6") ("ecukes-pkg.el" . "71e32c571d9041cde08bf17ed6908f6dcd89734e571275a23ab3f0359556d74e") ("ecukes-project.el" . "2113f2da42bd80ada38940f012813506b05487f020908ad5c68fff949767fa99") ("ecukes-reporter.el" . "69ed3caca958ae2291de64691cf82cb12cc7dc341c46ed374b2b144157820e55") ("ecukes-run.el" . "80e0e6f42defbc25be9ddd946a7c3927a5f5f50b1607e2a267881e3a8daeea56") ("ecukes-stats.el" . "903947752f3e25437bb3d0acbeb9f737e0a94f41465ac3af9c4bf349b951dcaf") ("ecukes-steps.el" . "97e3f7af649868780bac7c827278c2fc3355a6c2dffb7458d8dbac3a44c09856") ("ecukes-template.el" . "5a6e2b7683be2f8ddfc9fe98fcee240893d5cca32d503ed3377e2e9499695d20") ("ecukes.el" . "3f06d0bb38c381ef8c1d3a178cae43dc5aab7c810d75215b212e4fe8b9b95c91") ("reporters/ecukes-reporter-dot.el" . "74a05c40c649b0d26d93463e547054600c87f7d2decd80eba7fb7f9d7ae042ef") ("reporters/ecukes-reporter-landing.el" . "2834a917965a20d370085cc4b144944238c53324ca68385e658275c8276f88cc") ("reporters/ecukes-reporter-magnars.el" . "2a14acf31d7851b7812b75ac02732f385c0087e40d92c7f93264337ebfc2311e") ("reporters/ecukes-reporter-progress.el" . "24852648b81afaa0cfa09728b2078543a22626b70b4998afdcdf4f71257ae77b") ("reporters/ecukes-reporter-spec.el" . "7d46313f218eb4599b0ca51d3331da6059cc3f5f8b562cd4dc783fd429cb4168")) :feature t :espuds t :version "20241226.1759" :reporters (landing magnars progress spec dot)) :result (:tree ("cafe-ledger.el" "features/cafe-ledger.feature" "features/step-definitions/cafe-ledger-steps.el" "features/support/env.el") :stats (:scenarios 1 :scenarios-passed 1 :scenarios-failed 0 :steps 5 :steps-passed 5 :steps-failed 0 :steps-skipped 0) :hooks (setup before (remembered . "café 界") after teardown) :report "Feature: Café counter\n  In order to greet guests\n  As a barista\n  I want the buffer to show the special\n\n  Background:\n    Given I switch to buffer \"*cafe-ledger*\"\n    And I clear the buffer\n\n  Scenario: Insert the special\n    When I insert \"café 界\"\n    Then I should see \"café 界\"\n    And I remember the special\n\n1 scenarios (0 failed, 1 passed)\n5 steps (0 failed, 0 skipped, 5 passed)\n" :failing-file nil) :cleanup (:source-unchanged t :steps-restored t :hooks-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_tags_and_patterns_select_scenarios_and_missing_steps_are_listed() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_tags_and_patterns_select_scenarios_and_missing_steps_are_listed",
        r####"
(ecukes423-test-run
 (lambda ()
   (let* ((feature
           (ecukes423-test-write
            "cafe-ledger/features/select.feature"
            "Feature: Selection
  Scenario: Keep beans
    Given I start with 2 bags
    When I sell 1 bag
    Then I should have 1 bags

  @wip
  Scenario: Skip roast
    Given I start with 2 bags
    Then the roast is ready

  @wip
  Scenario: Keep leftover
    Given I start with 8 bags
    When I sell 1 bag
    Then I should have 7 bags

  Scenario: Extra roast later
    Given I start with 3 bags
    When I sell 1 bag
    Then I should have 2 bags

  Scenario: Keep cups later
    Given I start with 4 bags
    When I sell 2 bag
    Then I should have 2 bags
"))
          missing tagged patterned both)
     (make-directory (expand-file-name "cafe-ledger" ecukes423-test-root) t)
     (Given "^I start with \\([0-9]+\\) bags$"
       (lambda (count)
         (setq ecukes423-test-stock (string-to-number count))))
     (When "^I sell \\([0-9]+\\) bag$"
       (lambda (count)
         (setq ecukes423-test-stock
               (- ecukes423-test-stock (string-to-number count)))))
     (Then "^I should have \\([0-9]+\\) bags$"
       (lambda (count)
         (unless (equal ecukes423-test-stock (string-to-number count))
           (error "stock %s wanted %s" ecukes423-test-stock count))))
     (let ((default-directory (expand-file-name "cafe-ledger" ecukes423-test-root)))
       (ecukes423-test-reset-run-state)
       (ecukes-run (list feature))
       (setq missing
             (list :stats (ecukes423-test-stats)
                   :report (ecukes423-test-report)
                   :undefined
                   (mapcar (lambda (step)
                             (copy-sequence (ecukes-step-body step)))
                           (ecukes-steps-without-definition
                            (ecukes-feature-steps
                             (list (ecukes-parse-feature feature)))))))
       (Then "^the roast is ready$"
         (lambda () (push 'roast ecukes423-test-ledger)))
       (setq ecukes-exclude-tags '("wip")
             ecukes-patterns nil)
       (ecukes423-test-reset-run-state)
       (ecukes-run (list feature))
       (setq tagged
             (list :stats (ecukes423-test-stats)
                   :report (ecukes423-test-report)
                   :stock ecukes423-test-stock))
       (setq ecukes-exclude-tags nil
             ecukes-patterns '("\\`keep"))
       (ecukes423-test-reset-run-state)
       (ecukes-run (list feature))
       (setq patterned
             (list :stats (ecukes423-test-stats)
                   :report (ecukes423-test-report)
                   :stock ecukes423-test-stock))
       (setq ecukes-exclude-tags '("wip")
             ecukes-patterns '("\\`keep"))
       (ecukes423-test-reset-run-state)
       (ecukes-run (list feature))
       (setq both
             (list :stats (ecukes423-test-stats)
                   :report (ecukes423-test-report)
                   :stock ecukes423-test-stock
                   :hooks (reverse ecukes423-test-ledger))))
     (list :missing missing :tagged tagged :patterned patterned :both both))))
"####,
        expect![[
            r#"OK (:source (:tree "fa3fefe477c795ba762eb0d290e7a4e5cf3afc50" :manifest (("ecukes-byte-compile.el" . "3a61aebb1cb4039f0f9676c9a84535fe90d86cbf28f28a8e270980df974ff083") ("ecukes-cli.el" . "a99d493c04c55fe38895ca71672203cf143cb3b7cbb2bcf3fa1171afe84b5f4d") ("ecukes-core.el" . "6a06dae23e30bb9924802eb7e92b1928c3a19cdbfc25176537f37ff9d9dc04c4") ("ecukes-def.el" . "5a36a9b960275c137278b90c6280bf3bcc31bb02d040b9420121a9a12bbb0589") ("ecukes-helpers.el" . "22e38026ca1fba1abd6225c897149f460b668e2052f1c4e08712bb04e49fa45f") ("ecukes-hooks.el" . "af672b67acea4d661168026caf1f209f464fb84470a54a34395da030bf50167b") ("ecukes-load.el" . "6b147fb7bbbb3dc33c7bf5a9a29625ef3dd75d6bf2f6cc62f449ea0fa095e44f") ("ecukes-new.el" . "b0e20f13c55fb83436bd1937afe8a2fdb22c8f9a5b3adab00f74dfef6efc66c0") ("ecukes-parse.el" . "e4f9e5afc36eaaa05ff4f4440f7c35857fe81995a910deef5e20e5a4ba36d6e6") ("ecukes-pkg.el" . "71e32c571d9041cde08bf17ed6908f6dcd89734e571275a23ab3f0359556d74e") ("ecukes-project.el" . "2113f2da42bd80ada38940f012813506b05487f020908ad5c68fff949767fa99") ("ecukes-reporter.el" . "69ed3caca958ae2291de64691cf82cb12cc7dc341c46ed374b2b144157820e55") ("ecukes-run.el" . "80e0e6f42defbc25be9ddd946a7c3927a5f5f50b1607e2a267881e3a8daeea56") ("ecukes-stats.el" . "903947752f3e25437bb3d0acbeb9f737e0a94f41465ac3af9c4bf349b951dcaf") ("ecukes-steps.el" . "97e3f7af649868780bac7c827278c2fc3355a6c2dffb7458d8dbac3a44c09856") ("ecukes-template.el" . "5a6e2b7683be2f8ddfc9fe98fcee240893d5cca32d503ed3377e2e9499695d20") ("ecukes.el" . "3f06d0bb38c381ef8c1d3a178cae43dc5aab7c810d75215b212e4fe8b9b95c91") ("reporters/ecukes-reporter-dot.el" . "74a05c40c649b0d26d93463e547054600c87f7d2decd80eba7fb7f9d7ae042ef") ("reporters/ecukes-reporter-landing.el" . "2834a917965a20d370085cc4b144944238c53324ca68385e658275c8276f88cc") ("reporters/ecukes-reporter-magnars.el" . "2a14acf31d7851b7812b75ac02732f385c0087e40d92c7f93264337ebfc2311e") ("reporters/ecukes-reporter-progress.el" . "24852648b81afaa0cfa09728b2078543a22626b70b4998afdcdf4f71257ae77b") ("reporters/ecukes-reporter-spec.el" . "7d46313f218eb4599b0ca51d3331da6059cc3f5f8b562cd4dc783fd429cb4168")) :feature t :espuds t :version "20241226.1759" :reporters (landing magnars progress spec dot)) :result (:missing (:stats (:scenarios 0 :scenarios-passed 0 :scenarios-failed 0 :steps 0 :steps-passed 0 :steps-failed 0 :steps-skipped 0) :report "Please implement the following step definitions\n\n(Then \"^the roast is ready$\"\n  (lambda ()\n\n    ))\n\n" :undefined ("the roast is ready")) :tagged (:stats (:scenarios 3 :scenarios-passed 3 :scenarios-failed 0 :steps 9 :steps-passed 9 :steps-failed 0 :steps-skipped 0) :report "Feature: Selection\n  Scenario: Keep beans\n    Given I start with 2 bags\n    When I sell 1 bag\n    Then I should have 1 bags\n\n  Scenario: Extra roast later\n    Given I start with 3 bags\n    When I sell 1 bag\n    Then I should have 2 bags\n\n  Scenario: Keep cups later\n    Given I start with 4 bags\n    When I sell 2 bag\n    Then I should have 2 bags\n\n3 scenarios (0 failed, 3 passed)\n9 steps (0 failed, 0 skipped, 9 passed)\n" :stock 2) :patterned (:stats (:scenarios 3 :scenarios-passed 3 :scenarios-failed 0 :steps 9 :steps-passed 9 :steps-failed 0 :steps-skipped 0) :report "Feature: Selection\n  Scenario: Keep beans\n    Given I start with 2 bags\n    When I sell 1 bag\n    Then I should have 1 bags\n\n  @wip\n  Scenario: Keep leftover\n    Given I start with 8 bags\n    When I sell 1 bag\n    Then I should have 7 bags\n\n  Scenario: Keep cups later\n    Given I start with 4 bags\n    When I sell 2 bag\n    Then I should have 2 bags\n\n3 scenarios (0 failed, 3 passed)\n9 steps (0 failed, 0 skipped, 9 passed)\n" :stock 2) :both (:stats (:scenarios 2 :scenarios-passed 2 :scenarios-failed 0 :steps 6 :steps-passed 6 :steps-failed 0 :steps-skipped 0) :report "Feature: Selection\n  Scenario: Keep beans\n    Given I start with 2 bags\n    When I sell 1 bag\n    Then I should have 1 bags\n\n  Scenario: Keep cups later\n    Given I start with 4 bags\n    When I sell 2 bag\n    Then I should have 2 bags\n\n2 scenarios (0 failed, 2 passed)\n6 steps (0 failed, 0 skipped, 6 passed)\n" :stock 2 :hooks nil)) :cleanup (:source-unchanged t :steps-restored t :hooks-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn failing_step_is_atomic_then_public_recovery_succeeds() -> ParityBatchCase {
    ParityBatchCase::value(
        "failing_step_is_atomic_then_public_recovery_succeeds",
        r####"
(ecukes423-test-run
 (lambda ()
   (let* ((project (expand-file-name "cafe-ledger" ecukes423-test-root))
          (feature
           (ecukes423-test-write
            "cafe-ledger/features/fail.feature"
            "Feature: Recovery
  Scenario: Recount the bags
    Given I start with 2 bags
    When I sell 1 bag
    Then I should have 9 bags
    And I close the till
"))
          failed recovered)
     (make-directory project t)
     (Given "^I start with \\([0-9]+\\) bags$"
       (lambda (count)
         (setq ecukes423-test-stock (string-to-number count))
         (push (cons 'start ecukes423-test-stock) ecukes423-test-ledger)))
     (When "^I sell \\([0-9]+\\) bag$"
       (lambda (count)
         (setq ecukes423-test-stock
               (- ecukes423-test-stock (string-to-number count)))
         (push (cons 'sold ecukes423-test-stock) ecukes423-test-ledger)))
     (Then "^I should have \\([0-9]+\\) bags$"
       (lambda (count)
         (let ((wanted (string-to-number count)))
           (unless (equal ecukes423-test-stock wanted)
             (error "stock %s wanted %s" ecukes423-test-stock wanted))
           (push (cons 'checked wanted) ecukes423-test-ledger))))
     (And "^I close the till$"
       (lambda ()
         (push 'closed ecukes423-test-ledger)))
     (Fail (push 'fail-hook ecukes423-test-ledger))
     (let ((default-directory project))
       (ecukes423-test-reset-run-state)
       (ecukes-run (list feature))
       (setq failed
             (list :stats (ecukes423-test-stats)
                   :hooks (reverse ecukes423-test-ledger)
                   :stock ecukes423-test-stock
                   :report (ecukes423-test-report)
                   :failing-file (ecukes423-test-read "cafe-ledger/.ecukes-failing-scenarios")))
       (Then "^I should have \\([0-9]+\\) bags$"
         (lambda (count)
           (let ((wanted (string-to-number count)))
             (setq ecukes423-test-stock wanted)
             (push (cons 'checked wanted) ecukes423-test-ledger))))
       (ecukes423-test-write
        "cafe-ledger/features/fail.feature"
        "Feature: Recovery
  Scenario: Recount the bags
    Given I start with 2 bags
    When I sell 1 bag
    Then I should have 1 bags
    And I close the till
")
       (ecukes423-test-reset-run-state)
       (ecukes-run (list feature))
       (setq recovered
             (list :stats (ecukes423-test-stats)
                   :hooks (reverse ecukes423-test-ledger)
                   :stock ecukes423-test-stock
                   :report (ecukes423-test-report)
                   :failing-file
                   (and (file-exists-p
                         (expand-file-name ".ecukes-failing-scenarios" project))
                        (ecukes423-test-read "cafe-ledger/.ecukes-failing-scenarios")))))
     (list :failed failed :recovered recovered))))
"####,
        expect![[
            r#"OK (:source (:tree "fa3fefe477c795ba762eb0d290e7a4e5cf3afc50" :manifest (("ecukes-byte-compile.el" . "3a61aebb1cb4039f0f9676c9a84535fe90d86cbf28f28a8e270980df974ff083") ("ecukes-cli.el" . "a99d493c04c55fe38895ca71672203cf143cb3b7cbb2bcf3fa1171afe84b5f4d") ("ecukes-core.el" . "6a06dae23e30bb9924802eb7e92b1928c3a19cdbfc25176537f37ff9d9dc04c4") ("ecukes-def.el" . "5a36a9b960275c137278b90c6280bf3bcc31bb02d040b9420121a9a12bbb0589") ("ecukes-helpers.el" . "22e38026ca1fba1abd6225c897149f460b668e2052f1c4e08712bb04e49fa45f") ("ecukes-hooks.el" . "af672b67acea4d661168026caf1f209f464fb84470a54a34395da030bf50167b") ("ecukes-load.el" . "6b147fb7bbbb3dc33c7bf5a9a29625ef3dd75d6bf2f6cc62f449ea0fa095e44f") ("ecukes-new.el" . "b0e20f13c55fb83436bd1937afe8a2fdb22c8f9a5b3adab00f74dfef6efc66c0") ("ecukes-parse.el" . "e4f9e5afc36eaaa05ff4f4440f7c35857fe81995a910deef5e20e5a4ba36d6e6") ("ecukes-pkg.el" . "71e32c571d9041cde08bf17ed6908f6dcd89734e571275a23ab3f0359556d74e") ("ecukes-project.el" . "2113f2da42bd80ada38940f012813506b05487f020908ad5c68fff949767fa99") ("ecukes-reporter.el" . "69ed3caca958ae2291de64691cf82cb12cc7dc341c46ed374b2b144157820e55") ("ecukes-run.el" . "80e0e6f42defbc25be9ddd946a7c3927a5f5f50b1607e2a267881e3a8daeea56") ("ecukes-stats.el" . "903947752f3e25437bb3d0acbeb9f737e0a94f41465ac3af9c4bf349b951dcaf") ("ecukes-steps.el" . "97e3f7af649868780bac7c827278c2fc3355a6c2dffb7458d8dbac3a44c09856") ("ecukes-template.el" . "5a6e2b7683be2f8ddfc9fe98fcee240893d5cca32d503ed3377e2e9499695d20") ("ecukes.el" . "3f06d0bb38c381ef8c1d3a178cae43dc5aab7c810d75215b212e4fe8b9b95c91") ("reporters/ecukes-reporter-dot.el" . "74a05c40c649b0d26d93463e547054600c87f7d2decd80eba7fb7f9d7ae042ef") ("reporters/ecukes-reporter-landing.el" . "2834a917965a20d370085cc4b144944238c53324ca68385e658275c8276f88cc") ("reporters/ecukes-reporter-magnars.el" . "2a14acf31d7851b7812b75ac02732f385c0087e40d92c7f93264337ebfc2311e") ("reporters/ecukes-reporter-progress.el" . "24852648b81afaa0cfa09728b2078543a22626b70b4998afdcdf4f71257ae77b") ("reporters/ecukes-reporter-spec.el" . "7d46313f218eb4599b0ca51d3331da6059cc3f5f8b562cd4dc783fd429cb4168")) :feature t :espuds t :version "20241226.1759" :reporters (landing magnars progress spec dot)) :result (:failed (:stats (:scenarios 1 :scenarios-passed 0 :scenarios-failed 1 :steps 4 :steps-passed 2 :steps-failed 1 :steps-skipped 1) :hooks ((start . 2) (sold . 1) fail-hook) :stock 1 :report "Feature: Recovery\n  Scenario: Recount the bags\n    Given I start with 2 bags\n    When I sell 1 bag\n    Then I should have 9 bags\n      stock 1 wanted 9\n    And I close the till\n\n1 scenarios (1 failed, 0 passed)\n4 steps (1 failed, 1 skipped, 2 passed)\n" :failing-file "recount the bags") :recovered (:stats (:scenarios 1 :scenarios-passed 1 :scenarios-failed 0 :steps 4 :steps-passed 4 :steps-failed 0 :steps-skipped 0) :hooks ((start . 2) (sold . 1) (checked . 1) closed) :stock 1 :report "Feature: Recovery\n  Scenario: Recount the bags\n    Given I start with 2 bags\n    When I sell 1 bag\n    Then I should have 1 bags\n    And I close the till\n\n1 scenarios (0 failed, 1 passed)\n4 steps (0 failed, 0 skipped, 4 passed)\n" :failing-file nil)) :cleanup (:source-unchanged t :steps-restored t :hooks-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn ecukes_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_new_scaffolds_project_files_and_refuses_a_second_setup(),
        public_parser_expands_outlines_tables_pystrings_tags_and_unicode(),
        public_load_run_executes_hooks_espuds_steps_and_spec_reporter(),
        public_tags_and_patterns_select_scenarios_and_missing_steps_are_listed(),
        failing_step_is_atomic_then_public_recovery_succeeds(),
    ];
    assert_oracle_batch_cases(oracle(), "ecukes-rank423", "ecukes_parity", &cases);
}
