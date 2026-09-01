use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FLYCHECK_PACKAGE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const FLYCHECK_PACKAGE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const FLYCHECK_PACKAGE_TEST_PRELUDE: &str = r###"
(require 'cl-lib)

(defun neomacs-flycheck-package-test-source (name summary headers commentary body)
  "Build a realistic single-file package NAME for a Flycheck workflow."
  (concat ";;; " name ".el --- " summary " -*- lexical-binding: t; -*-\n"
          headers
          ";;; Commentary:\n;; " commentary "\n\n"
          ";;; Code:\n"
          body "\n"
          "(provide '" name ")\n"
          ";;; " name ".el ends here\n"))

(defun neomacs-flycheck-package-test-with-setup (function)
  "Run FUNCTION with flycheck-package installed into an isolated checker graph."
  (let ((saved-checkers (copy-sequence flycheck-checkers))
        (saved-elisp-next
         (copy-tree (flycheck-checker-get 'emacs-lisp 'next-checkers)))
        (saved-checkdoc-next
         (copy-tree (flycheck-checker-get 'emacs-lisp-checkdoc 'next-checkers))))
    (unwind-protect
        (progn
          (flycheck-package-setup)
          (funcall function))
      (setq flycheck-checkers saved-checkers)
      (setf (flycheck-checker-get 'emacs-lisp 'next-checkers)
            saved-elisp-next)
      (setf (flycheck-checker-get 'emacs-lisp-checkdoc 'next-checkers)
            saved-checkdoc-next))))

(defun neomacs-flycheck-package-test-with-buffer (source filename function)
  "Visit SOURCE as FILENAME and call FUNCTION in its Elisp buffer."
  (let ((buffer (generate-new-buffer " *flycheck-package-workflow*")))
    (unwind-protect
        (with-current-buffer buffer
          (insert source)
          (setq buffer-file-name
                (expand-file-name filename
                                  (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          (let ((delay-mode-hooks t))
            (emacs-lisp-mode))
          (set-buffer-modified-p nil)
          (funcall function))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-flycheck-package-test-diagnostics ()
  "Return stable fields from the current Flycheck diagnostics."
  (mapcar
   (lambda (diagnostic)
     (list :line (flycheck-error-line diagnostic)
           :column (flycheck-error-column diagnostic)
           :level (flycheck-error-level diagnostic)
           :checker (flycheck-error-checker diagnostic)
           :message (flycheck-error-message diagnostic)))
   flycheck-current-errors))

(defun neomacs-flycheck-package-test-overlay-locations ()
  "Return the buffer and diagnostic coordinates of every Flycheck overlay."
  (mapcar
   (lambda (overlay)
     (let ((diagnostic (overlay-get overlay 'flycheck-error)))
       (save-excursion
         (goto-char (overlay-start overlay))
         (list :buffer-line (line-number-at-pos)
               :buffer-column (current-column)
               :diagnostic-line (flycheck-error-line diagnostic)
               :diagnostic-column (flycheck-error-column diagnostic)))))
   (sort (flycheck-overlays-in (point-min) (point-max))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun neomacs-flycheck-package-test-run ()
  "Run only flycheck-package in the current buffer and return its UI state."
  (let ((flycheck-checker 'emacs-lisp-package)
        (flycheck-checkers '(emacs-lisp-package))
        (flycheck-check-syntax-automatically nil)
        (text-quoting-style 'straight))
    (unless flycheck-mode
      (flycheck-mode 1))
    (flycheck-buffer)
    (list :status flycheck-last-status-change
          :checker flycheck-checker
          :diagnostics
          (neomacs-flycheck-package-test-diagnostics)
          :overlay-count
          (length (flycheck-overlays-in (point-min) (point-max))))))

(defun neomacs-flycheck-package-test-run-selected-to-completion (checker)
  "Run CHECKER and its configured chain to completion in the current buffer."
  (let ((flycheck-checker checker)
        (flycheck-check-syntax-automatically nil)
        (text-quoting-style 'straight)
        finished
        (rounds 0))
    (add-hook 'flycheck-after-syntax-check-hook
              (lambda () (setq finished t)) nil t)
    (flycheck-mode 1)
    (flycheck-buffer)
    (while (and (not finished) (< rounds 600))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds)))
    (unless finished
      (error "Timed out waiting for %S and its next checkers; status is %S"
             checker flycheck-last-status-change))
    (list :status flycheck-last-status-change
          :selected flycheck-checker
          :diagnostics (neomacs-flycheck-package-test-diagnostics)
          :overlay-count
          (length (flycheck-overlays-in (point-min) (point-max))))))
"###;

fn flycheck_package_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYCHECK_PACKAGE_MELPA_PIN, "flycheck-package.el")
        .expect("prepare pinned flycheck-package source below ./tmp")
        .with_prelude(FLYCHECK_PACKAGE_TEST_PRELUDE)
        .with_timeout(FLYCHECK_PACKAGE_TEST_TIMEOUT)
}

fn setup_registers_one_checker_at_the_end_of_both_elisp_chains() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (setq flycheck-checkers '(emacs-lisp emacs-lisp-checkdoc))
   (setf (flycheck-checker-get 'emacs-lisp 'next-checkers)
         '(emacs-lisp-checkdoc))
   (setf (flycheck-checker-get 'emacs-lisp-checkdoc 'next-checkers) nil)
   (flycheck-package-setup)
   (flycheck-package-setup)
   (list
    :checkers flycheck-checkers
    :checker-count (cl-count 'emacs-lisp-package flycheck-checkers)
    :emacs-lisp-chain
    (flycheck-checker-get 'emacs-lisp 'next-checkers)
    :checkdoc-chain
    (flycheck-checker-get 'emacs-lisp-checkdoc 'next-checkers))))
"###;
    let expected = expect![[
        r#"OK (:checkers (emacs-lisp emacs-lisp-checkdoc emacs-lisp-package) :checker-count 1 :emacs-lisp-chain (emacs-lisp-checkdoc emacs-lisp-package) :checkdoc-chain (emacs-lisp-package))"#
    ]];
    ParityBatchCase::value(
        "setup_registers_one_checker_at_the_end_of_both_elisp_chains",
        elisp_form,
        expected,
    )
}

fn setup_drives_package_lint_after_the_existing_checker_finishes() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (let* ((source-text
           (neomacs-flycheck-package-test-source
            "handoff-audit" "Audit checker handoffs"
            ";; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"28.1\"))\n;; URL: https://example.test/handoff-audit\n;; Keywords: tools\n"
            "Audit a release through Checkdoc and package metadata checks."
            "(defun publish-release ()\n  \"Publish the audited release.\"\n  t)"))
          (source-file
           (expand-file-name "handoff-audit.el"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          buffer)
     (write-region source-text nil source-file nil 'silent)
     (setq buffer (find-file-noselect source-file))
     (unwind-protect
         (with-current-buffer buffer
           (list
            :configured-chain
            (flycheck-checker-get 'emacs-lisp-checkdoc 'next-checkers)
            :run
            (neomacs-flycheck-package-test-run-selected-to-completion
             'emacs-lisp-checkdoc)))
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (set-buffer-modified-p nil))
         (kill-buffer buffer))
       (when (file-exists-p source-file)
         (delete-file source-file))))))
"###;
    let expected = expect![[
        r#"OK (:configured-chain (emacs-lisp-package) :run (:status finished :selected emacs-lisp-checkdoc :diagnostics ((:line 10 :column 1 :level error :checker emacs-lisp-package :message "\"publish-release\" doesn't start with package's prefix \"handoff-audit\".")) :overlay-count 1))"#
    ]];
    ParityBatchCase::value(
        "setup_drives_package_lint_after_the_existing_checker_finishes",
        elisp_form,
        expected,
    )
}

fn checker_detects_release_headers_even_when_the_buffer_is_narrowed() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (mapcar
    (lambda (entry)
      (pcase-let ((`(,label ,headers ,narrowed) entry))
        (let ((source
               (neomacs-flycheck-package-test-source
                "release-probe" "Inspect a release"
                headers
                "Inspect a release before publication."
                "(defun release-probe-run () \"Inspect the release.\" t)")))
          (neomacs-flycheck-package-test-with-buffer
           source "release-probe.el"
           (lambda ()
             (when narrowed
               (goto-char (point-min))
               (re-search-forward ";;; Code:")
               (narrow-to-region (line-beginning-position) (point-max)))
             (list label
                   (funcall
                    (flycheck-checker-get
                     'emacs-lisp-package 'predicate))))))))
    '((package-requires
       ";; Package-Requires: ((emacs \"28.1\"))\n" nil)
      (package-version ";; Package-Version: 2.4.1\n" nil)
      (version ";; Version: 2.4.1\n" nil)
      (case-insensitive ";; pAcKaGe-VeRsIoN: 2.4.1\n" nil)
      (narrowed ";; Package-Version: 2.4.1\n" t)
      (ordinary-library "" nil)))))
"###;
    let expected = expect![
        "OK ((package-requires 93) (package-version 92) (version 84) (case-insensitive 92) (narrowed 92) (ordinary-library nil))"
    ];
    ParityBatchCase::value(
        "checker_detects_release_headers_even_when_the_buffer_is_narrowed",
        elisp_form,
        expected,
    )
}

fn clean_release_finishes_without_diagnostics_or_overlays() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (let ((source
          (neomacs-flycheck-package-test-source
           "deploy-console" "Inspect deployment health"
           ";; Package-Version: 1.4.2\n;; Package-Requires: ((emacs \"28.1\"))\n;; Author: Release Team <release@example.test>\n;; Maintainer: Operator <operator@example.test>\n;; URL: https://example.test/deploy-console\n;; Keywords: tools, convenience\n"
           "Inspect release health and provide explicit operator actions."
           "(defgroup deploy-console nil\n  \"Deployment consoles.\"\n  :group 'tools)\n\n(defun deploy-console-status (release environment)\n  \"Return a stable status line for RELEASE in ENVIRONMENT.\"\n  (format \"%s is healthy in %s\" release environment))")))
     (neomacs-flycheck-package-test-with-buffer
      source "deploy-console.el"
      #'neomacs-flycheck-package-test-run))))
"###;
    let expected = expect![
        "OK (:status finished :checker emacs-lisp-package :diagnostics nil :overlay-count 0)"
    ];
    ParityBatchCase::value(
        "clean_release_finishes_without_diagnostics_or_overlays",
        elisp_form,
        expected,
    )
}

fn broken_release_becomes_precise_flycheck_diagnostics_and_navigation() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (let ((source
          (concat
           ";;; broken-release.el --- Broken release package. -*- lexical-binding: t; -*-\n"
           ";; Package-Version: invalid\n"
           ";; URL: git://example.test/broken-release\n"
           ";; Keywords: deployment-only\n"
           ";;; Commentary:\n;;   \n\n"
           ";;; Code:\n"
           "(defun publish-now () \"Publish immediately.\" t)\n"
           "(global-set-key (kbd \"C-c z\") #'publish-now)\n"
           "(provide 'broken-release)\n"
           ";;; broken-release.el ends here\n")))
     (neomacs-flycheck-package-test-with-buffer
      source "broken-release.el"
      (lambda ()
        (let ((result (neomacs-flycheck-package-test-run))
              navigation navigation-end
              backward-navigation)
          (goto-char (point-min))
          (condition-case err
              (while t
                (flycheck-next-error 1 (null navigation))
                (push (list (line-number-at-pos) (current-column))
                      navigation))
            (user-error
             (setq navigation-end (error-message-string err))))
          (goto-char (point-max))
          (dotimes
              (_ (length
                  (delete-dups
                   (mapcar #'overlay-start
                           (flycheck-overlays-in
                            (point-min) (point-max))))))
            (flycheck-previous-error 1)
            (push (list (line-number-at-pos) (current-column))
                  backward-navigation))
          (append result
                  (list
                   :overlays
                   (neomacs-flycheck-package-test-overlay-locations)
                   :forward-navigation (nreverse navigation)
                   :forward-navigation-end navigation-end
                   :backward-navigation
                   (nreverse backward-navigation)))))))))
"###;
    let expected = expect![[
        r#"OK (:status finished :checker emacs-lisp-package :diagnostics ((:line 1 :column 1 :level error :checker emacs-lisp-package :message "package.el cannot parse this buffer: Invalid version syntax: 'invalid' (must start with a number)") (:line 1 :column 55 :level warning :checker emacs-lisp-package :message "You should depend on (emacs \"24.1\") if you need lexical-binding.") (:line 2 :column 21 :level warning :checker emacs-lisp-package :message "\"invalid\" is not a valid version. MELPA will handle this, but other archives will not.") (:line 3 :column 9 :level error :checker emacs-lisp-package :message "Package URLs should be a single HTTPS or HTTP URL.") (:line 4 :column 14 :level warning :checker emacs-lisp-package :message "You should include standard keywords: see the variable `finder-known-keywords'.") (:line 5 :column 1 :level error :checker emacs-lisp-package :message "Package should have a non-empty ;;; Commentary section.") (:line 9 :column 1 :level error :checker emacs-lisp-package :message "\"publish-now\" doesn't start with package's prefix \"broken-release\".") (:line 10 :column 45 :level error :checker emacs-lisp-package :message "This key sequence is reserved (see Key Binding Conventions in the Emacs Lisp manual)")) :overlay-count 8 :overlays ((:buffer-line 1 :buffer-column 0 :diagnostic-line 1 :diagnostic-column 1) (:buffer-line 1 :buffer-column 54 :diagnostic-line 1 :diagnostic-column 55) (:buffer-line 2 :buffer-column 20 :diagnostic-line 2 :diagnostic-column 21) (:buffer-line 3 :buffer-column 8 :diagnostic-line 3 :diagnostic-column 9) (:buffer-line 4 :buffer-column 13 :diagnostic-line 4 :diagnostic-column 14) (:buffer-line 5 :buffer-column 0 :diagnostic-line 5 :diagnostic-column 1) (:buffer-line 9 :buffer-column 0 :diagnostic-line 9 :diagnostic-column 1) (:buffer-line 10 :buffer-column 44 :diagnostic-line 10 :diagnostic-column 45)) :forward-navigation ((1 54) (2 20) (3 8) (4 13) (5 0) (9 0) (10 44)) :forward-navigation-end "No more Flycheck errors" :backward-navigation ((10 44) (9 0) (5 0) (4 13) (3 8) (2 20) (1 54) (1 0)))"#
    ]];
    ParityBatchCase::value(
        "broken_release_becomes_precise_flycheck_diagnostics_and_navigation",
        elisp_form,
        expected,
    )
}

fn fixing_metadata_clears_stale_diagnostics_on_the_next_check() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (let* ((broken
           (neomacs-flycheck-package-test-source
            "release-gate" "Gate a release"
            ";; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"28.1\"))\n;; Keywords: tools\n"
            "Gate a release after checking its package metadata."
            "(defun ship-now () \"Ship the release immediately.\" t)"))
          (fixed
           (replace-regexp-in-string
            "(defun ship-now" "(defun release-gate-ship-now"
            (replace-regexp-in-string
            ";; Keywords: tools\n"
             ";; URL: https://example.test/release-gate\n;; Keywords: tools\n"
             broken t t)
            t t)))
     (neomacs-flycheck-package-test-with-buffer
      broken "release-gate.el"
      (lambda ()
        (let ((before (neomacs-flycheck-package-test-run)))
          (erase-buffer)
          (insert fixed)
          (set-buffer-modified-p nil)
          (let ((after (neomacs-flycheck-package-test-run)))
            (list :before before :after after))))))))
"###;
    let expected = expect![[
        r#"OK (:before (:status finished :checker emacs-lisp-package :diagnostics ((:line 1 :column 1 :level error :checker emacs-lisp-package :message "Package should have a Homepage or URL header.") (:line 9 :column 1 :level error :checker emacs-lisp-package :message "\"ship-now\" doesn't start with package's prefix \"release-gate\".")) :overlay-count 2) :after (:status finished :checker emacs-lisp-package :diagnostics nil :overlay-count 0))"#
    ]];
    ParityBatchCase::value(
        "fixing_metadata_clears_stale_diagnostics_on_the_next_check",
        elisp_form,
        expected,
    )
}

fn secondary_package_file_is_checked_through_main_file_ownership() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (let* ((source
          ";;; release-suite-ui.el --- Release suite UI -*- lexical-binding: t; -*-\n;;; Commentary:\n;; Render release-suite status for an operator.\n\n;;; Code:\n(require 'release-suite)\n(defun render-release-now () \"Render release status.\" t)\n(provide 'release-suite-ui)\n;;; release-suite-ui.el ends here\n")
          (main-file
           (expand-file-name "release-suite.el"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          (main-source
           (neomacs-flycheck-package-test-source
            "release-suite" "Coordinate release operations"
            ";; Package-Version: 3.2.1\n;; Package-Requires: ((emacs \"28.1\"))\n;; URL: https://example.test/release-suite\n;; Keywords: tools\n"
            "Coordinate a multi-file release operations package."
            "(defun release-suite-run () \"Run the release workflow.\" t)")))
     (write-region main-source nil main-file nil 'silent)
     (unwind-protect
         (let ((package-lint-main-file main-file))
           (neomacs-flycheck-package-test-with-buffer
            source "release-suite-ui.el"
            (lambda ()
              (list
               :predicate
               (file-relative-name
                (funcall
                 (flycheck-checker-get 'emacs-lisp-package 'predicate))
                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
               :run (neomacs-flycheck-package-test-run)))))
       (when (file-exists-p main-file)
         (delete-file main-file))))))
"###;
    let expected = expect![[
        r#"OK (:predicate "release-suite.el" :run (:status finished :checker emacs-lisp-package :diagnostics ((:line 7 :column 1 :level error :checker emacs-lisp-package :message "\"render-release-now\" doesn't start with package's prefix \"release-suite\".")) :overlay-count 1))"#
    ]];
    ParityBatchCase::value(
        "secondary_package_file_is_checked_through_main_file_ownership",
        elisp_form,
        expected,
    )
}

fn incomplete_keybinding_edit_reports_failure_and_resignals_the_reader_error() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flycheck-package-test-with-setup
 (lambda ()
   (let ((source
          (neomacs-flycheck-package-test-source
           "release-shortcuts" "Operate releases from the keyboard"
           ";; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"28.1\"))\n;; URL: https://example.test/release-shortcuts\n;; Keywords: tools\n"
           "Provide keyboard-driven release operations."
           "(defun release-shortcuts-publish ()\n  \"Publish the current release.\"\n  t)\n\n(global-set-key (kbd \"C-c z\"")))
     (neomacs-flycheck-package-test-with-buffer
      source "release-shortcuts.el"
      (lambda ()
        (condition-case err
            (progn
              (neomacs-flycheck-package-test-run)
              :unexpected-success)
          (error
           (list :condition (car err)
                 :message (error-message-string err)
                 :status flycheck-last-status-change
                 :running (flycheck-running-p)
                 :diagnostics
                 (neomacs-flycheck-package-test-diagnostics)
                 :overlay-count
                 (length
                  (flycheck-overlays-in (point-min) (point-max)))))))))))
"###;
    let expected = expect![[
        r#"OK (:condition end-of-file :message "End of file during parsing:  *flycheck-package-workflow*" :status errored :running nil :diagnostics nil :overlay-count 0)"#
    ]];
    ParityBatchCase::value(
        "incomplete_keybinding_edit_reports_failure_and_resignals_the_reader_error",
        elisp_form,
        expected,
    )
}

#[test]
fn flycheck_package_package_batch() {
    let cases = [
        setup_registers_one_checker_at_the_end_of_both_elisp_chains(),
        setup_drives_package_lint_after_the_existing_checker_finishes(),
        checker_detects_release_headers_even_when_the_buffer_is_narrowed(),
        clean_release_finishes_without_diagnostics_or_overlays(),
        broken_release_becomes_precise_flycheck_diagnostics_and_navigation(),
        fixing_metadata_clears_stale_diagnostics_on_the_next_check(),
        secondary_package_file_is_checked_through_main_file_ownership(),
        incomplete_keybinding_edit_reports_failure_and_resignals_the_reader_error(),
    ];
    assert_oracle_batch_cases(
        flycheck_package_oracle(),
        "flycheck-package-package-batch",
        "flycheck-package parity",
        &cases,
    );
}
