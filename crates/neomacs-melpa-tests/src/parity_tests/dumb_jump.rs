use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DUMB_JUMP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const DUMB_JUMP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const DUMB_JUMP_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'xref)
(require 'dumb-jump)

(setq dumb-jump-force-searcher 'rg
      dumb-jump-rg-cmd "rg"
      dumb-jump-rg-search-args "--pcre2"
      dumb-jump-fallback-search nil
      dumb-jump-quiet t
      xref-after-jump-hook nil
      xref-after-return-hook nil
      xref-history-storage #'xref-global-history)
(add-hook 'xref-backend-functions #'dumb-jump-xref-activate)

(defun neomacs-dumb-jump-test-root (name)
  "Create and return a deterministic sandbox directory for NAME."
  (let ((root (expand-file-name
               (concat "dumb-jump-" name "/")
               (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-dumb-jump-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-dumb-jump-test-visit (path needle)
  "Visit PATH and move to the beginning of NEEDLE."
  (switch-to-buffer (find-file-noselect path))
  (goto-char (point-min))
  (search-forward needle)
  (goto-char (match-beginning 0))
  (current-buffer))

(defun neomacs-dumb-jump-test-location (&optional root)
  "Describe point, using ROOT-relative file names when supplied."
  (list :file (if root
                  (file-relative-name buffer-file-name root)
                (file-name-nondirectory buffer-file-name))
        :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun neomacs-dumb-jump-test-marker (marker root)
  "Describe MARKER using a path relative to ROOT."
  (let ((buffer (marker-buffer marker)))
    (when buffer
      (with-current-buffer buffer
        (list (file-relative-name buffer-file-name root)
              (marker-position marker)
              (line-number-at-pos marker)
              (save-excursion
                (goto-char marker)
                (current-column)))))))

(defun neomacs-dumb-jump-test-history (root)
  "Describe the global xref history relative to ROOT."
  (let ((history (xref-global-history)))
    (list :backward
          (mapcar (lambda (marker)
                    (neomacs-dumb-jump-test-marker marker root))
                  (car history))
          :forward
          (mapcar (lambda (marker)
                    (neomacs-dumb-jump-test-marker marker root))
                  (cdr history)))))

(defun neomacs-dumb-jump-test-reset-history ()
  "Install an empty private global xref history."
  (xref-global-history (cons nil nil)))

(defun neomacs-dumb-jump-test-xrefs (xrefs root)
  "Describe XREFS with stable paths relative to ROOT."
  (mapcar
   (lambda (xref)
     (let ((location (xref-item-location xref)))
       (list :summary (xref-item-summary xref)
             :file (file-relative-name
                    (xref-file-location-file location) root)
             :line (xref-file-location-line location)
             :column (xref-file-location-column location))))
   xrefs))

(defun neomacs-dumb-jump-test-cleanup (root)
  "Kill buffers and remove files belonging to ROOT."
  (dolist (buffer (buffer-list))
    (when (and (buffer-file-name buffer)
               (string-prefix-p root (buffer-file-name buffer)))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (neomacs-dumb-jump-test-reset-history)
  (when (file-exists-p root)
    (delete-directory root t)))
"###;

fn dumb_jump_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DUMB_JUMP_MELPA_PIN, "dumb-jump.el")
        .expect("prepare revision-pinned Dumb Jump source below ./tmp")
        .with_prelude(DUMB_JUMP_TEST_PRELUDE)
        .with_timeout(DUMB_JUMP_TEST_TIMEOUT)
}

fn meta_dot_jumps_through_the_real_xref_backend_and_meta_comma_restores_the_call_site()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-dumb-jump-test-root "jump-back"))
       (definition (expand-file-name "src/release-service.js" root))
       (caller (expand-file-name "app/deploy.js" root)))
  (unwind-protect
      (progn
        (neomacs-dumb-jump-test-write
         (expand-file-name ".dumbjump" root)
         "language javascript\n")
        (neomacs-dumb-jump-test-write
         definition
         "export function deployRelease(release, region) {\n  return `${release}:${region}`;\n}\n")
        (neomacs-dumb-jump-test-write
         caller
         "const plan = deployRelease(\"REL-417\", \"us-east\");\nconsole.log(plan);\n")
        (neomacs-dumb-jump-test-visit caller "deployRelease")
        (neomacs-dumb-jump-test-reset-history)
        (let* ((backend (xref-find-backend))
               (identifier (xref-backend-identifier-at-point backend))
               (origin (neomacs-dumb-jump-test-location root))
               (context (get-text-property 0 :dumb-jump-ctx identifier)))
          (execute-kbd-macro (kbd "M-."))
          (let ((target (neomacs-dumb-jump-test-location root))
                (jump-history (neomacs-dumb-jump-test-history root)))
            (execute-kbd-macro (kbd "M-,"))
            (list :backend backend
                  :identifier (substring-no-properties identifier)
                  :context context
                  :origin origin
                  :target target
                  :jump-history jump-history
                  :returned (neomacs-dumb-jump-test-location root)
                  :return-history (neomacs-dumb-jump-test-history root)))))
    (neomacs-dumb-jump-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:backend dumb-jump :identifier "deployRelease" :context (:left "const plan = " :right "(\"REL-417\", \"us-east\");\n") :origin (:file "app/deploy.js" :line 1 :column 13 :text "const plan = deployRelease(\"REL-417\", \"us-east\");") :target (:file "src/release-service.js" :line 1 :column 0 :text "export function deployRelease(release, region) {") :jump-history (:backward (("app/deploy.js" 14 1 13)) :forward nil) :returned (:file "app/deploy.js" :line 1 :column 13 :text "const plan = deployRelease(\"REL-417\", \"us-east\");") :return-history (:backward nil :forward (("src/release-service.js" 1 1 0))))"#
    ]];
    ParityBatchCase::value(
        "meta_dot_jumps_through_the_real_xref_backend_and_meta_comma_restores_the_call_site",
        elisp_form,
        expected,
    )
}

fn references_report_real_calls_across_files_while_filtering_definitions_and_comments()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-dumb-jump-test-root "references"))
       (definition (expand-file-name "src/release-service.js" root))
       (caller (expand-file-name "app/deploy.js" root))
       (worker (expand-file-name "workers/retry.js" root)))
  (unwind-protect
      (progn
        (neomacs-dumb-jump-test-write
         (expand-file-name ".dumbjump" root)
         "language javascript\n")
        (neomacs-dumb-jump-test-write
         definition
         "export function deployRelease(release) {\n  return release.status === \"ready\";\n}\n")
        (neomacs-dumb-jump-test-write
         caller
         "const first = deployRelease(primary);\nconst second = deployRelease(canary);\n// deployRelease(exampleOnly);\nconst runbook = \"deployRelease(manual)\";\n")
        (neomacs-dumb-jump-test-write
         worker
         "export const retry = (release) => deployRelease(release);\n")
        (neomacs-dumb-jump-test-visit caller "deployRelease")
        (let* ((backend (xref-find-backend))
               (identifier (xref-backend-identifier-at-point backend)))
          (list :identifier (substring-no-properties identifier)
                :references
                (neomacs-dumb-jump-test-xrefs
                 (xref-backend-references backend identifier) root))))
    (neomacs-dumb-jump-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:identifier "deployRelease" :references ((:summary "const runbook = \"deployRelease(manual)\";" :file "app/deploy.js" :line 4 :column 0) (:summary "const second = deployRelease(canary);" :file "app/deploy.js" :line 2 :column 0) (:summary "export const retry = (release) => deployRelease(release);" :file "workers/retry.js" :line 1 :column 0)))"#
    ]];
    ParityBatchCase::value(
        "references_report_real_calls_across_files_while_filtering_definitions_and_comments",
        elisp_form,
        expected,
    )
}

fn project_config_searches_a_shared_library_but_excludes_generated_vendor_definitions()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-dumb-jump-test-root "project-config"))
       (project (expand-file-name "checkout/" root))
       (shared (expand-file-name "shared-lib/" root))
       (caller (expand-file-name "app/deploy.js" project)))
  (unwind-protect
      (progn
        (neomacs-dumb-jump-test-write
         (expand-file-name ".dumbjump" project)
         "language javascript\n-vendor\n+../shared-lib\n")
        (neomacs-dumb-jump-test-write
         caller
         "const receipt = buildReceipt(release);\n")
        (neomacs-dumb-jump-test-write
         (expand-file-name "src/receipt.js" project)
         "export function buildReceipt(release) { return release.id; }\n")
        (neomacs-dumb-jump-test-write
         (expand-file-name "vendor/generated.js" project)
         "export function buildReceipt(release) { return \"generated\"; }\n")
        (neomacs-dumb-jump-test-write
         (expand-file-name "receipt-audit.js" shared)
         "export function buildReceipt(release) { return `audit:${release.id}`; }\n")
        (neomacs-dumb-jump-test-visit caller "buildReceipt")
        (let* ((backend (xref-find-backend))
               (identifier (xref-backend-identifier-at-point backend))
               (config (dumb-jump-read-config project ".dumbjump")))
          (list
           :project-root (file-relative-name
                          (dumb-jump-get-project-root caller) root)
           :config
           (list :language (plist-get config :language)
                 :exclude
                 (mapcar (lambda (path) (file-relative-name path root))
                         (plist-get config :exclude))
                 :include
                 (mapcar (lambda (path) (file-relative-name path root))
                         (plist-get config :include)))
           :definitions
           (neomacs-dumb-jump-test-xrefs
            (xref-backend-definitions backend identifier) root))))
    (neomacs-dumb-jump-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:project-root "checkout" :config (:language "javascript" :exclude ("checkout/vendor") :include ("shared-lib")) :definitions ((:summary "export function buildReceipt(release) { return release.id; }" :file "checkout/src/receipt.js" :line 1 :column 0) (:summary "export function buildReceipt(release) { return `audit:${release.id}`; }" :file "shared-lib/receipt-audit.js" :line 1 :column 0)))"#
    ]];
    ParityBatchCase::value(
        "project_config_searches_a_shared_library_but_excludes_generated_vendor_definitions",
        elisp_form,
        expected,
    )
}

fn javascript_context_selects_the_callable_or_the_configuration_value_for_the_same_name()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-dumb-jump-test-root "context"))
       (caller (expand-file-name "app/deploy.js" root)))
  (unwind-protect
      (progn
        (neomacs-dumb-jump-test-write
         (expand-file-name ".dumbjump" root)
         "language javascript\n")
        (neomacs-dumb-jump-test-write
         (expand-file-name "services/release.js" root)
         "export function deployRelease(release) { return release.id; }\n")
        (neomacs-dumb-jump-test-write
         (expand-file-name "config/release.js" root)
         "export const deployRelease = \"canary-only\";\n")
        (neomacs-dumb-jump-test-write
         caller
         "const receipt = deployRelease(release);\nconsole.log(deployRelease);\n")
        (neomacs-dumb-jump-test-visit caller "deployRelease")
        (let* ((backend (xref-find-backend))
               (call-id (xref-backend-identifier-at-point backend))
               (call-context (get-text-property 0 :dumb-jump-ctx call-id))
               (call-definitions
                (xref-backend-definitions backend call-id)))
          (goto-char (point-min))
          (search-forward "console.log(deployRelease")
          (search-backward "deployRelease")
          (goto-char (match-beginning 0))
          (let* ((value-id (xref-backend-identifier-at-point backend))
                 (value-context (get-text-property 0 :dumb-jump-ctx value-id))
                 (value-definitions
                  (xref-backend-definitions backend value-id)))
            (list
             :call
             (list :context call-context
                   :definitions
                   (neomacs-dumb-jump-test-xrefs call-definitions root))
             :value
             (list :context value-context
                   :definitions
                   (neomacs-dumb-jump-test-xrefs value-definitions root))))))
    (neomacs-dumb-jump-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:call (:context (:left "const receipt = " :right "(release);\n") :definitions ((:summary "export function deployRelease(release) { return release.id; }" :file "services/release.js" :line 1 :column 0))) :value (:context (:left "console.log(" :right ");\n") :definitions ((:summary "export const deployRelease = \"canary-only\";" :file "config/release.js" :line 1 :column 0))))"#
    ]];
    ParityBatchCase::value(
        "javascript_context_selects_the_callable_or_the_configuration_value_for_the_same_name",
        elisp_form,
        expected,
    )
}

fn a_missing_definition_reports_the_real_xref_error_without_moving_the_user() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-dumb-jump-test-root "missing"))
       (caller (expand-file-name "app/deploy.js" root)))
  (unwind-protect
      (progn
        (neomacs-dumb-jump-test-write
         (expand-file-name ".dumbjump" root)
         "language javascript\n")
        (neomacs-dumb-jump-test-write
         caller
         "const receipt = publishMissingRelease(release);\n")
        (neomacs-dumb-jump-test-visit caller "publishMissingRelease")
        (neomacs-dumb-jump-test-reset-history)
        (let ((origin (neomacs-dumb-jump-test-location root))
              outcome)
          (condition-case error-data
              (progn
                (execute-kbd-macro (kbd "M-."))
                (setq outcome :unexpected-success))
            (error
             (setq outcome
                   (list :signal (car error-data)
                         :message (error-message-string error-data)))))
          (list :outcome outcome
                :origin origin
                :after (neomacs-dumb-jump-test-location root)
                :history (neomacs-dumb-jump-test-history root))))
    (neomacs-dumb-jump-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:outcome (:signal user-error :message "No definitions found for: publishMissingRelease") :origin (:file "app/deploy.js" :line 1 :column 16 :text "const receipt = publishMissingRelease(release);") :after (:file "app/deploy.js" :line 1 :column 16 :text "const receipt = publishMissingRelease(release);") :history (:backward nil :forward nil))"#
    ]];
    ParityBatchCase::value(
        "a_missing_definition_reports_the_real_xref_error_without_moving_the_user",
        elisp_form,
        expected,
    )
}

#[test]
fn dumb_jump_package_batch() {
    assert_oracle_batch_cases(
        dumb_jump_oracle(),
        "dumb-jump-package-batch",
        "Dumb Jump",
        &[
            meta_dot_jumps_through_the_real_xref_backend_and_meta_comma_restores_the_call_site(),
            references_report_real_calls_across_files_while_filtering_definitions_and_comments(),
            project_config_searches_a_shared_library_but_excludes_generated_vendor_definitions(),
            javascript_context_selects_the_callable_or_the_configuration_value_for_the_same_name(),
            a_missing_definition_reports_the_real_xref_error_without_moving_the_user(),
        ],
    );
}
