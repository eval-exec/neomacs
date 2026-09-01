use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PACKAGE_LINT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PACKAGE_LINT_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PACKAGE_LINT_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'package-lint)

(defun neomacs-package-lint-test-source (name summary headers commentary body
                                              &optional footer)
  "Build a complete single-file package source for a release audit."
  (concat ";;; " name ".el --- " summary " -*- lexical-binding: t; -*-\n"
          headers
          ";;; Commentary:\n;; " commentary "\n\n"
          ";;; Code:\n"
          body "\n"
          "(provide '" name ")\n"
          (or footer (concat ";;; " name ".el ends here\n"))))

(defun neomacs-package-lint-test-run (source filename &optional main-file)
  "Lint SOURCE as FILENAME, optionally as a secondary file of MAIN-FILE."
  (with-temp-buffer
    (let ((delay-mode-hooks t))
      (emacs-lisp-mode))
    (insert source)
    (setq buffer-file-name
          (expand-file-name filename (getenv "HOME")))
    (let ((package-lint-main-file main-file))
      (package-lint-buffer))))

(defun neomacs-package-lint-test-descriptor (source filename)
  "Parse SOURCE as FILENAME and return stable package metadata."
  (with-temp-buffer
    (let ((delay-mode-hooks t))
      (emacs-lisp-mode))
    (insert source)
    (setq buffer-file-name
          (expand-file-name filename (getenv "HOME")))
    (let* ((descriptor (package-buffer-info))
           (extras (package-desc-extras descriptor)))
      (list :name (package-desc-name descriptor)
            :version (package-version-join
                      (package-desc-version descriptor))
            :summary (package-desc-summary descriptor)
            :requirements
            (mapcar
             (lambda (requirement)
               (list (car requirement)
                     (package-version-join (cadr requirement))))
             (package-desc-reqs descriptor))
            :url (alist-get :url extras)
            :keywords (alist-get :keywords extras)
            :authors (alist-get :authors extras)
            :maintainer (alist-get :maintainer extras)))))

(defun neomacs-package-lint-test-reset ()
  "Restore globals changed by deterministic archive and report probes."
  (setq package-archive-contents nil
        package-alist nil
        package-lint-main-file nil
        package-lint-batch-fail-on-warnings t)
  (when-let ((buffer (get-buffer "*Package-Lint*")))
    (kill-buffer buffer)))

(defun neomacs-package-lint-test-with-reset (function)
  "Run FUNCTION without leaking package-lint state to another case."
  (neomacs-package-lint-test-reset)
  (unwind-protect
      (funcall function)
    (neomacs-package-lint-test-reset)))
"###;

fn package_lint_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PACKAGE_LINT_MELPA_PIN, "package-lint.el")
        .expect("prepare revision-pinned Package Lint source below ./tmp")
        .with_prelude(PACKAGE_LINT_TEST_PRELUDE)
        .with_timeout(PACKAGE_LINT_TEST_TIMEOUT)
}

fn release_candidate_with_complete_metadata_is_publishable_and_parseable() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let ((source
          (neomacs-package-lint-test-source
           "deploy-dashboard"
           "Inspect deployment state"
           ";; Package-Version: 1.4.2\n;; Package-Requires: ((emacs \"28.1\"))\n;; Author: Release Team <release@example.test>\n;; Maintainer: Operator <operator@example.test>\n;; URL: https://example.test/deploy-dashboard\n;; Keywords: tools, convenience\n"
           "Inspect release health and provide explicit operator actions."
           "(defgroup deploy-dashboard nil\n  \"Deployment dashboards.\"\n  :group 'tools)\n\n(defcustom deploy-dashboard-refresh-seconds 30\n  \"Seconds between deployment refreshes.\"\n  :type 'integer\n  :group 'deploy-dashboard)\n\n(defun deploy-dashboard-status (release environment)\n  \"Return a stable status line for RELEASE in ENVIRONMENT.\"\n  (format \"%s is healthy in %s\" release environment))\n\n;;;###autoload\n(define-minor-mode deploy-dashboard-mode\n  \"Annotate the current deployment dashboard.\"\n  :lighter \" Deploy\")")))
     (list :diagnostics
           (neomacs-package-lint-test-run
            source "deploy-dashboard.el")
           :descriptor
           (neomacs-package-lint-test-descriptor
            source "deploy-dashboard.el")))))
"###;
    let expected = expect![[
        r#"OK (:diagnostics nil :descriptor (:name deploy-dashboard :version "1.4.2" :summary "Inspect deployment state" :requirements ((emacs "28.1")) :url "https://example.test/deploy-dashboard" :keywords ("tools" "convenience") :authors (("Release Team" . "release@example.test")) :maintainer ("Operator" . "operator@example.test")))"#
    ]];
    ParityBatchCase::value(
        "release_candidate_with_complete_metadata_is_publishable_and_parseable",
        elisp_form,
        expected,
    )
}

fn broken_release_audit_reports_metadata_keybinding_prefix_and_style_failures() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let ((source
          (concat
           ";;; broken-release.el --- broken release package. -*- lexical-binding: t; -*-\n"
           ";; Package-Version: invalid\n"
           ";; URL: git://example.test/broken-release\n"
           ";; Keywords: deployment-only\n"
           ";;; Commentary:\n;;   \n\n"
           ";;; Code:\n"
           "\"~/.emacs.d/broken-release.state\"\n"
           ";;;###autoload\n"
           "(defun broken-release--private () \"Private helper.\" t)\n"
           "(defun publish-now () \"Publish immediately.\" t)\n"
           "(global-set-key (kbd \"C-c z\") #'publish-now)\n"
           "(eval-after-load 'server '(publish-now))\n"
           "(defgroup broken-release-mode nil \"Broken release.\")\n"
           "(message (format \"release=%1$s\" \"REL-2048\"))\n"
           "(list 'dangling\n"
           ")\n"
           "(provide 'broken-release)\n"
           ";;; broken-release.el ends here\n")))
     (neomacs-package-lint-test-run source "broken-release.el"))))
"###;
    let expected = expect![[
        r#"OK ((1 0 error "package.el cannot parse this buffer: Invalid version syntax: ‘invalid’ (must start with a number)") (1 54 warning "You should depend on (emacs \"24.1\") if you need lexical-binding.") (2 20 warning "\"invalid\" is not a valid version. MELPA will handle this, but other archives will not.") (3 8 error "Package URLs should be a single HTTPS or HTTP URL.") (4 13 warning "You should include standard keywords: see the variable `finder-known-keywords'.") (5 0 error "Package should have a non-empty ;;; Commentary section.") (9 11 warning "Use variable `user-emacs-directory' or function `locate-user-emacs-file' instead of a literal path to the Emacs user directory or files.") (10 0 warning "Private functions generally should not be autoloaded.") (12 0 error "\"publish-now\" doesn't start with package's prefix \"broken-release\".") (13 44 error "This key sequence is reserved (see Key Binding Conventions in the Emacs Lisp manual)") (14 1 warning "`eval-after-load' is for use in configurations, and should rarely be used in packages.") (15 0 error "Customization groups should not end in \"-mode\" unless that name would conflict with their parent group.") (15 0 error "Customization groups should specify a parent via `:group'.") (16 9 error "You should depend on (emacs \"26.1\") if you need format field numbers.") (18 0 warning "Closing parens should not be wrapped onto new lines."))"#
    ]];
    ParityBatchCase::value(
        "broken_release_audit_reports_metadata_keybinding_prefix_and_style_failures",
        elisp_form,
        expected,
    )
}

fn dependency_resolution_uses_the_highest_available_release_and_rejects_bad_pins() -> ParityBatchCase
{
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let ((package-archive-contents
          `((widget-kit
             ,(package-desc-from-define
               "widget-kit" "1.2.0" "Widget toolkit" nil :kind 'single)
             ,(package-desc-from-define
               "widget-kit" "2.0.0" "Widget toolkit" nil :kind 'single))
            (snapshot-kit
             ,(package-desc-from-define
               "snapshot-kit" "20270101.1000" "Snapshot toolkit" nil
               :kind 'single))
            (zero-kit
             ,(package-desc-from-define
               "zero-kit" "1.0.0" "Zero toolkit" nil :kind 'single))))
         (source
          (neomacs-package-lint-test-source
           "deploy-deps"
           "Validate deployment dependencies"
           ";; Package-Version: 2.0.0\n;; Package-Requires: ((emacs \"25.1\") (widget-kit \"3.0\") (missing-kit \"1.0\") (snapshot-kit \"20260101.1200\") (zero-kit \"0\"))\n;; URL: https://example.test/deploy-deps\n;; Keywords: tools\n"
           "Validate the dependency floor used by a release pipeline."
           "(defun deploy-deps-check ()\n  \"Return non-nil when dependency metadata is loaded.\"\n  t)")))
     (list :diagnostics
           (neomacs-package-lint-test-run source "deploy-deps.el")
           :available
           (mapcar
            (lambda (name)
              (list name
                    (package-version-join
                     (package-lint--highest-installable-version-of name))))
            '(widget-kit snapshot-kit zero-kit))))))
"###;
    let expected = expect![[
        r#"OK (:diagnostics ((3 38 warning "Version dependency for widget-kit appears too high: try 2.0.0") (3 57 error "Package missing-kit is not installable.") (3 77 warning "Use a non-snapshot version number for dependency on \"snapshot-kit\" if possible.") (3 108 warning "Use a properly versioned dependency on \"zero-kit\" if possible.")) :available ((widget-kit "2.0.0") (snapshot-kit "20270101.1000") (zero-kit "1.0.0")))"#
    ]];
    ParityBatchCase::value(
        "dependency_resolution_uses_the_highest_available_release_and_rejects_bad_pins",
        elisp_form,
        expected,
    )
}

fn compatibility_audit_distinguishes_old_modern_and_guarded_runtime_paths() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let* ((old-source
           (neomacs-package-lint-test-source
            "compat-report"
            "Build compatibility reports"
            ";; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"24.1\"))\n;; URL: https://example.test/compat-report\n;; Keywords: compatibility\n"
            "Report release data while supporting deliberately old Emacs versions."
            "(require 'subr-x)\n(require 'seq)\n\n(defun compat-report-build (items text)\n  \"Build a compatibility report for ITEMS and TEXT.\"\n  (format \"%1$s:%s:%s\"\n          (seq-length items)\n          (proper-list-p items)\n          (string-split text)))"))
          (modern-source
           (string-replace
            "(emacs \"24.1\")" "(emacs \"29.1\")" old-source))
          (guarded-source
           (neomacs-package-lint-test-source
            "guarded-report"
            "Build guarded compatibility reports"
            ";; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"24.1\"))\n;; URL: https://example.test/guarded-report\n;; Keywords: compatibility\n"
            "Use optional runtime capabilities without raising the declared floor."
            "(defun guarded-report-split (text)\n  \"Split TEXT when the runtime provides the modern helper.\"\n  (if (fboundp 'string-split)\n      (string-split text)\n    (split-string text)))")))
     (list :old
           (neomacs-package-lint-test-run
            old-source "compat-report.el")
           :modern
           (neomacs-package-lint-test-run
            modern-source "compat-report.el")
           :guarded
           (neomacs-package-lint-test-run
            guarded-source "guarded-report.el")))))
"###;
    let expected = expect![[
        r#"OK (:old ((5 13 warning "You should include standard keywords: see the variable `finder-known-keywords'.") (10 10 error "You should depend on (emacs \"24.4\") if you need `subr-x'.") (11 10 error "You should depend on (emacs \"25.1\") or the seq package if you need `seq'.") (15 2 error "You should depend on (emacs \"26.1\") if you need format field numbers.") (16 11 error "You should depend on (emacs \"25.1\") or the seq package if you need `seq-length'.") (17 11 error "You should depend on (emacs \"27.1\") or the compat package if you need `proper-list-p'.") (18 11 error "You should depend on (emacs \"29.1\") or the compat package if you need `string-split'.")) :modern ((5 13 warning "You should include standard keywords: see the variable `finder-known-keywords'.")) :guarded ((5 13 warning "You should include standard keywords: see the variable `finder-known-keywords'.")))"#
    ]];
    ParityBatchCase::value(
        "compatibility_audit_distinguishes_old_modern_and_guarded_runtime_paths",
        elisp_form,
        expected,
    )
}

fn multi_file_package_and_theme_checks_respect_main_file_ownership() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let* ((main-path (expand-file-name "release-tools.el" (getenv "HOME")))
          (main-source
           (neomacs-package-lint-test-source
            "release-tools"
            "Coordinate release operations"
            ";; Package-Version: 3.2.1\n;; Package-Requires: ((emacs \"28.1\"))\n;; URL: https://example.test/release-tools\n;; Keywords: tools\n"
            "Coordinate a multi-file release operations package."
            "(defun release-tools-run ()\n  \"Run the release workflow.\"\n  t)"))
          (secondary-with-deps
           ";;; release-tools-ui.el --- Release operations UI -*- lexical-binding: t; -*-\n;; Package-Requires: ((emacs \"28.1\"))\n;;; Commentary:\n;; Render release operations.\n;;; Code:\n(require 'release-tools)\n(defun release-tools-ui-render () \"Render release status.\" t)\n(provide 'release-tools-ui)\n;;; release-tools-ui.el ends here\n")
          (secondary-clean
           (string-replace
            ";; Package-Requires: ((emacs \"28.1\"))\n" ""
            secondary-with-deps))
          (theme-source
           ";;; aurora-theme.el --- Operational status theme -*- lexical-binding: t; -*-\n;; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"28.1\"))\n;; URL: https://example.test/aurora-theme\n;; Keywords: faces\n;;; Commentary:\n;; Theme for operational dashboards.\n;;; Code:\n(deftheme aurora \"Operational status colors.\")\n(custom-theme-set-faces 'aurora '(default ((t (:foreground \"white\" :background \"black\")))))\n(provide-theme 'aurora)\n;;; aurora-theme.el ends here\n"))
     (write-region main-source nil main-path nil 'silent)
     (unwind-protect
         (list
          :main (neomacs-package-lint-test-run
                 main-source "release-tools.el")
          :secondary-with-deps
          (neomacs-package-lint-test-run
           secondary-with-deps "release-tools-ui.el" main-path)
          :secondary-clean
          (neomacs-package-lint-test-run
           secondary-clean "release-tools-ui.el" main-path)
          :theme
          (neomacs-package-lint-test-run
           theme-source "aurora-theme.el"))
       (when (file-exists-p main-path)
         (delete-file main-path))))))
"###;
    let expected = expect![[
        r#"OK (:main nil :secondary-with-deps ((2 0 error "Package-Requires outside the main file have no effect.")) :secondary-clean nil :theme nil)"#
    ]];
    ParityBatchCase::value(
        "multi_file_package_and_theme_checks_respect_main_file_ownership",
        elisp_form,
        expected,
    )
}

fn interactive_report_buffer_is_read_only_navigable_and_actionable() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let ((source
          (neomacs-package-lint-test-source
           "operator-check"
           "Check operator actions"
           ";; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"28.1\"))\n;; Keywords: tools\n"
           "Check release operator actions before publishing."
           "(defun unprefixed-action ()\n  \"Run an improperly named action.\"\n  t)"))
         displayed)
     (with-temp-buffer
       (let ((delay-mode-hooks t))
         (emacs-lisp-mode))
       (insert source)
       (setq buffer-file-name
             (expand-file-name "operator-check.el" (getenv "HOME")))
       (cl-letf (((symbol-function 'display-buffer)
                  (lambda (buffer-or-name &rest _)
                    (setq displayed
                          (if (bufferp buffer-or-name)
                              (buffer-name buffer-or-name)
                            buffer-or-name))
                    nil)))
         (package-lint-current-buffer)))
     (with-current-buffer "*Package-Lint*"
       (list :displayed displayed
             :text (buffer-string)
             :mode major-mode
             :read-only buffer-read-only
             :view-mode view-mode
             :point (point))))))
"###;
    let expected = expect![[
        r#"OK (:displayed "*Package-Lint*" :text "2 issues found:\n\n1:0: error: Package should have a Homepage or URL header.\n9:0: error: \"unprefixed-action\" doesn't start with package's prefix \"operator-check\".\n" :mode special-mode :read-only t :view-mode t :point 162)"#
    ]];
    ParityBatchCase::value(
        "interactive_report_buffer_is_read_only_navigable_and_actionable",
        elisp_form,
        expected,
    )
}

fn ci_batch_policy_can_treat_warnings_as_fatal_without_masking_errors() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let* ((directory
           (expand-file-name "package-lint-ci" (getenv "HOME")))
          (clean-path (expand-file-name "clean-release.el" directory))
          (warning-path (expand-file-name "warning-release.el" directory))
          (error-path (expand-file-name "error-release.el" directory))
          (clean
           (neomacs-package-lint-test-source
            "clean-release" "Validate clean releases"
            ";; Package-Version: 1.0.0\n;; Package-Requires: ((emacs \"28.1\"))\n;; URL: https://example.test/clean-release\n;; Keywords: tools\n"
            "Validate a clean release in continuous integration."
            "(defun clean-release-check () \"Validate the release.\" t)"))
          (warning
           (replace-regexp-in-string
            ";; Keywords: tools" ";; Keywords: release-only"
            (replace-regexp-in-string
             "clean-release" "warning-release" clean t t)
            t t))
          (error-source
           (replace-regexp-in-string
            "(provide 'error-release)\n" ""
            (replace-regexp-in-string
             "clean-release" "error-release" clean t t)
            t t))
          messages warnings-fatal warnings-allowed errors-still-fatal)
     (make-directory directory t)
     (write-region clean nil clean-path nil 'silent)
     (write-region warning nil warning-path nil 'silent)
     (write-region error-source nil error-path nil 'silent)
     (unwind-protect
         (cl-letf (((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((text (apply #'format format-string arguments)))
                        (unless (string-prefix-p "Entering directory" text)
                          (push text messages))))))
           (let ((package-lint-batch-fail-on-warnings t))
             (setq warnings-fatal
                   (package-lint-batch-and-exit-1
                    (list clean-path warning-path))))
           (let ((package-lint-batch-fail-on-warnings nil))
             (setq warnings-allowed
                   (package-lint-batch-and-exit-1
                    (list clean-path warning-path))
                   errors-still-fatal
                   (package-lint-batch-and-exit-1
                    (list clean-path error-path))))
           (list :warnings-fatal warnings-fatal
                 :warnings-allowed warnings-allowed
                 :errors-still-fatal errors-still-fatal
                 :messages (nreverse messages)))
       (when (file-directory-p directory)
         (delete-directory directory t))))))
"###;
    let expected = expect![[
        r#"OK (:warnings-fatal nil :warnings-allowed t :errors-still-fatal nil :messages ("warning-release.el:5:13: warning: You should include standard keywords: see the variable `finder-known-keywords'." "error-release.el:1:0: error: There is no (provide 'error-release) form."))"#
    ]];
    ParityBatchCase::value(
        "ci_batch_policy_can_treat_warnings_as_fatal_without_masking_errors",
        elisp_form,
        expected,
    )
}

fn version_maintenance_updates_the_header_then_reparses_the_release() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-package-lint-test-with-reset
 (lambda ()
   (let ((source
          (neomacs-package-lint-test-source
           "release-notes"
           "Maintain release notes"
           ";; Version: 1.2.0\n;; Package-Requires: ((emacs \"28.1\"))\n;; URL: https://example.test/release-notes\n;; Keywords: tools\n"
           "Maintain versioned release notes for operators."
           "(defun release-notes-current ()\n  \"Return the current release note.\"\n  \"Canary is healthy\")")))
     (with-temp-buffer
       (let ((delay-mode-hooks t))
         (emacs-lisp-mode))
       (insert source)
       (setq buffer-file-name
             (expand-file-name "release-notes.el" (getenv "HOME")))
       (package-lint--update-or-insert-version "2.0.0")
       (let* ((updated (buffer-string))
              (descriptor (package-buffer-info))
              (diagnostics (package-lint-buffer)))
         (list :version-lines
               (seq-filter
                (lambda (line) (string-match-p ";; Version:" line))
                (split-string updated "\n"))
               :parsed-version
               (package-version-join
                (package-desc-version descriptor))
               :diagnostics diagnostics
               :footer-present
               (string-suffix-p
                ";;; release-notes.el ends here\n" updated)))))))
"###;
    let expected = expect![[
        r#"OK (:version-lines (";; Version: 2.0.0" ";; Version: 1.2.0") :parsed-version "2.0.0" :diagnostics nil :footer-present t)"#
    ]];
    ParityBatchCase::value(
        "version_maintenance_updates_the_header_then_reparses_the_release",
        elisp_form,
        expected,
    )
}

#[test]
fn package_lint_package_batch() {
    let cases = vec![
        release_candidate_with_complete_metadata_is_publishable_and_parseable(),
        broken_release_audit_reports_metadata_keybinding_prefix_and_style_failures(),
        dependency_resolution_uses_the_highest_available_release_and_rejects_bad_pins(),
        compatibility_audit_distinguishes_old_modern_and_guarded_runtime_paths(),
        multi_file_package_and_theme_checks_respect_main_file_ownership(),
        interactive_report_buffer_is_read_only_navigable_and_actionable(),
        ci_batch_policy_can_treat_warnings_as_fatal_without_masking_errors(),
        version_maintenance_updates_the_header_then_reparses_the_release(),
    ];
    assert_oracle_batch_cases(
        package_lint_oracle(),
        "package-lint-package-batch",
        "Package Lint",
        &cases,
    );
}
