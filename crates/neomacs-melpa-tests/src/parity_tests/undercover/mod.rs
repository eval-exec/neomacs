use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, SHUT_UP_MELPA_PIN, UNDERCOVER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

// Undercover deliberately replaces process-global Edebug handlers and leaves
// Edebug data on every instrumented definition.  Each logical workflow uses a
// fresh editor process because there is no package API that reverses either
// mutation.  Files and buffers are nevertheless owned and cleaned explicitly.
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'json)
(require 'undercover)

(defvar undercover375-test-sandbox
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst undercover375-test-core-source
  (concat
   ";;; core.el --- owned coverage fixture -*- lexical-binding: t -*-\n"
   "(defun undercover375-add (left right)\n"
   "  (+ left right))\n"
   "\n"
   "(defun undercover375-classify (value)\n"
   "  (cond\n"
   "   ((< value 0) (list 'negative value))\n"
   "   ((zerop value) 'zero)\n"
   "   (t (list 'positive value \"界\"))))\n"
   "\n"
   "(provide 'undercover375-core)\n"))

(defconst undercover375-test-extra-source
  (concat
   ";;; extra.el --- second owned coverage fixture -*- lexical-binding: t -*-\n"
   "(defun undercover375-order-total (prices)\n"
   "  (let ((total 0))\n"
   "    (dolist (price prices total)\n"
   "      (setq total (+ total price)))))\n"
   "(provide 'undercover375-extra)\n"))

(defconst undercover375-test-excluded-source
  (concat
   ";;; excluded.el --- deliberately excluded fixture -*- lexical-binding: t -*-\n"
   "(defun undercover375-excluded (value)\n"
   "  (* value value))\n"
   "(provide 'undercover375-excluded)\n"))

(defun undercover375-test-root (name)
  (let ((root (file-name-as-directory
               (expand-file-name (concat "undercover375/" name "/")
                                 undercover375-test-sandbox))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun undercover375-test-write (path bytes)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert bytes)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun undercover375-test-read (path)
  (when (file-exists-p path)
    (let ((coding-system-for-read 'utf-8-unix))
      (with-temp-buffer
        (insert-file-contents path)
        (buffer-string)))))

(defun undercover375-test-normalize-string (value root)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name root)) "[ROOT]" value t t))

(defun undercover375-test-relative (path root)
  (when path
    (file-relative-name path root)))

(defun undercover375-test-owned-buffer-p (buffer root)
  (let ((file (buffer-file-name buffer)))
    (or (and file (file-in-directory-p file root))
        (string-prefix-p " *undercover375-" (buffer-name buffer)))))

(defun undercover375-test-run-case (name thunk)
  (let ((root (undercover375-test-root name))
        result)
    (unwind-protect
        (setq result
              (save-excursion
                (save-window-excursion
                  (funcall thunk root))))
      (dolist (buffer (buffer-list))
        (when (undercover375-test-owned-buffer-p buffer root)
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer)))
      (when (file-exists-p root)
        (delete-directory root t)))
    result))

(defun undercover375-test-make-project (root)
  (make-directory (expand-file-name "reports/" root) t)
  (let ((core (undercover375-test-write
               (expand-file-name "project Ω/lib space/core.el" root)
               undercover375-test-core-source))
        (extra (undercover375-test-write
                (expand-file-name "project Ω/lib space/extra.el" root)
                undercover375-test-extra-source))
        (excluded (undercover375-test-write
                   (expand-file-name "project Ω/lib space/excluded.el" root)
                   undercover375-test-excluded-source)))
    (list core extra excluded)))

(defun undercover375-test-load (file)
  (load file nil nil t))

(defun undercover375-test-handler-count ()
  (cl-count 'undercover-file-handler file-name-handler-alist
            :key #'cdr :test #'eq))

(defun undercover375-test-tracked-files (root)
  (sort (mapcar (lambda (file)
                  (undercover375-test-relative file root))
                undercover--files)
        #'string-lessp))

(defun undercover375-test-coverage (root)
  (mapcar
   (lambda (file)
     (let ((lines nil))
       (maphash (lambda (line hits) (push (cons line hits) lines))
                (gethash file undercover--files-coverage-statistics))
       (list (undercover375-test-relative file root)
             (sort lines (lambda (left right) (< (car left) (car right)))))))
   (sort (copy-sequence undercover--files) #'string-lessp)))

(defun undercover375-test-edebug-state (symbols)
  (mapcar
   (lambda (symbol)
     (list symbol
           :instrumented (and (get symbol 'edebug) t)
           :counts (and (get symbol 'edebug-freq-count)
                        (append (get symbol 'edebug-freq-count) nil))))
   symbols))

(defun undercover375-test-last-message ()
  (with-current-buffer (messages-buffer)
    (save-excursion
      (goto-char (point-max))
      (skip-chars-backward "\n")
      (buffer-substring-no-properties
       (line-beginning-position) (line-end-position)))))

(defun undercover375-test-text-report-state (path)
  (let* ((lines (split-string (or (undercover375-test-read path) "")
                              "\n" t))
         (header (car lines))
         (rows (sort (copy-sequence (cdr lines)) #'string-lessp)))
    (list :header header :rows rows)))

(defun undercover375-test-clean-environment ()
  (let ((names '("CI" "CI_NAME" "GITHUB_ACTIONS" "GITHUB_SHA" "GITHUB_REF"
                 "GITHUB_RUN_ID" "GITHUB_RUN_NUMBER" "TRAVIS"
                 "SHIPPABLE" "DRONE" "JENKINS_URL" "JENKINS_HOME"
                 "CIRCLECI" "WERCKER" "GITLAB_CI" "APPVEYOR"
                 "SURF_SHA1" "BUILDKITE" "SEMAPHORE" "CF_REVISION"
                 "UNDERCOVER_FORCE" "UNDERCOVER_CONFIG"
                 "UNDERCOVER_CI_TYPE" "UNDERCOVER_CI_NAME"
                 "UNDERCOVER_COMMIT" "UNDERCOVER_REF"
                 "UNDERCOVER_PULL_REQUEST" "UNDERCOVER_BUILD_ID"
                 "UNDERCOVER_BUILD_NUMBER" "UNDERCOVER_JOB_ID"
                 "UNDERCOVER_JOB_NUMBER" "UNDERCOVER_JOB_NAME")))
    (seq-remove
     (lambda (entry)
       (seq-some (lambda (name)
                   (string-prefix-p (concat name "=") entry))
                 names))
     process-environment)))

(defun undercover375-test-json-table (&rest pairs)
  (let ((table (make-hash-table :test 'equal)))
    (while pairs
      (puthash (pop pairs) (pop pairs) table))
    table))

(defun undercover375-test-simplecov-state (path root)
  (let ((json-object-type 'hash-table)
        (json-array-type 'list))
    (let* ((report (json-read-file path))
           (entry (gethash "undercover.el" report))
           (coverage (gethash "coverage" entry))
           files)
      (maphash
       (lambda (file values)
         (push (list (undercover375-test-relative file root) values) files))
       coverage)
      (list :timestamp-integer (integerp (gethash "timestamp" entry))
            :files (sort files (lambda (left right)
                                 (string-lessp (car left) (car right))))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(UNDERCOVER_MELPA_PIN, "undercover.el")
        .expect("prepare pinned undercover source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned Dash dependency below ./tmp")
        .with_melpa_dependency(SHUT_UP_MELPA_PIN)
        .expect("prepare pinned shut-up dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_setup_instruments_included_files_and_writes_a_text_report() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_setup_instruments_included_files_and_writes_a_text_report",
        r####"(undercover375-test-run-case
 "instrument-and-text"
 (lambda (root)
   (pcase-let* ((`(,core ,extra ,excluded)
                 (undercover375-test-make-project root))
                (report (expand-file-name "reports/coverage.txt" root))
                (default-directory (expand-file-name "project Ω/lib space/" root))
                (undercover-force-coverage t)
                (kill-emacs-hook nil))
     (undercover "*.el" (:exclude "excluded.el")
                 (:report-format 'text) (:report-file report)
                 (:report-on-kill nil) (:send-report nil)
                 (:merge-report nil) (:verbosity 0))
     (undercover375-test-load core)
     (undercover375-test-load extra)
     (undercover375-test-load excluded)
     (let ((values (list (undercover375-add 7 5)
                         (undercover375-classify 0)
                         (undercover375-classify 9)
                         (undercover375-order-total '(3 5 8))
                         (undercover375-excluded 6))))
       (undercover-report)
       (list :values values
             :tracked (undercover375-test-tracked-files root)
             :handlers (undercover375-test-handler-count)
             :edebug (undercover375-test-edebug-state
                      '(undercover375-add undercover375-classify
                        undercover375-order-total undercover375-excluded))
             :coverage (undercover375-test-coverage root)
             :report (undercover375-test-text-report-state report)
             :hook kill-emacs-hook)))))"####,
        expect![[r#"OK (:values (12 zero (positive 9 "界") 16 36) :tracked ("project Ω/lib space/core.el" "project Ω/lib space/extra.el") :handlers 1 :edebug ((undercover375-add :instrumented t :counts (1 1 1 1)) (undercover375-classify :instrumented t :counts (2 2 2 2 0 0 0 2 2 2 1 1 1 2)) (undercover375-order-total :instrumented t :counts (1 1 1 1 1 1 1 1 1 1 1 1)) (undercover375-excluded :instrumented nil :counts nil)) :coverage (("project Ω/lib space/core.el" ((3 . 1) (6 . 2) (7 . 0) (8 . 2) (9 . 1))) ("project Ω/lib space/extra.el" ((3 . 1) (4 . 1) (5 . 1)))) :report (:header "== Code coverage text report ==" :rows ("core : Percent 80% [Relevant: 5 Covered: 4 Missed: 1]" "extra : Percent 100% [Relevant: 3 Covered: 3 Missed: 0]")) :hook nil)"#]],
    )
    .fresh_process()
}

fn ci_and_environment_configuration_select_exact_files_and_lcov_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "ci_and_environment_configuration_select_exact_files_and_lcov_output",
        r####"(undercover375-test-run-case
 "ci-config-lcov"
 (lambda (root)
   (pcase-let* ((`(,core ,extra ,excluded)
                 (undercover375-test-make-project root))
                (report (expand-file-name "reports/lcov.info" root))
                (default-directory root)
                (process-environment (undercover375-test-clean-environment))
                (undercover-force-coverage nil)
                (undercover--env nil)
                (kill-emacs-hook nil))
     (setenv "GITHUB_ACTIONS" "true")
     (setenv "GITHUB_SHA" "0123456789abcdef0123456789abcdef01234567")
     (setenv "GITHUB_REF" "refs/heads/parity-界")
     (setenv "GITHUB_RUN_ID" "37501")
     (setenv
      "UNDERCOVER_CONFIG"
      (prin1-to-string
       (list (list :files core extra)
             (list :exclude extra)
             (list :report-format 'lcov)
             (list :report-file report)
             (list :report-on-kill nil)
             (list :send-report nil)
             (list :merge-report nil)
             (list :verbosity 0))))
     (undercover "never-matches-*.el" (:report-format 'text))
     (undercover375-test-load core)
     (undercover375-test-load extra)
     (undercover375-test-load excluded)
     (let ((value (undercover375-classify -4)))
       (undercover-report)
       (list :enabled (and (undercover-enabled-p) t)
             :value value
             :tracked (undercover375-test-tracked-files root)
             :edebug (undercover375-test-edebug-state
                      '(undercover375-classify undercover375-order-total
                        undercover375-excluded))
             :format undercover--report-format
             :hook kill-emacs-hook
             :lcov (undercover375-test-normalize-string
                    (undercover375-test-read report) root))))))"####,
        expect![[r#"OK (:enabled t :value (negative -4) :tracked ("project Ω/lib space/core.el") :edebug ((undercover375-classify :instrumented t :counts (1 1 1 1 1 1 1 0 0 0 0 0 0 1)) (undercover375-order-total :instrumented nil :counts nil) (undercover375-excluded :instrumented nil :counts nil)) :format lcov :hook nil :lcov "SF:[ROOT]/project Ω/lib space/core.el\nDA:3,0\nDA:6,1\nDA:7,1\nDA:8,0\nDA:9,0\nend_of_record\n")"#]],
    )
    .fresh_process()
}

fn simplecov_report_merges_existing_coverage_and_accumulates_public_runs() -> ParityBatchCase {
    ParityBatchCase::value(
        "simplecov_report_merges_existing_coverage_and_accumulates_public_runs",
        r####"(undercover375-test-run-case
 "simplecov-merge"
 (lambda (root)
   (pcase-let* ((`(,core ,_extra ,_excluded)
                 (undercover375-test-make-project root))
                (legacy (undercover375-test-write
                         (expand-file-name "legacy/retired.el" root)
                         ";;; retired\n"))
                (report (expand-file-name "reports/.resultset.json" root))
                (default-directory root)
                (undercover-force-coverage t)
                (kill-emacs-hook nil))
     (undercover375-test-write
      report
      (json-encode
       (undercover375-test-json-table
        "undercover.el"
        (undercover375-test-json-table
         "timestamp" 1
         "coverage"
         (undercover375-test-json-table
          core (make-list 20 1)
          legacy '(nil 4 0))))))
     (undercover (:files core)
                 (:report-format 'simplecov) (:report-file report)
                 (:report-on-kill nil) (:send-report nil)
                 (:merge-report t) (:verbosity 0))
     (undercover375-test-load core)
     (undercover375-add 2 3)
     (undercover375-classify 0)
     (undercover-report)
     (let ((first (undercover375-test-simplecov-state report root)))
       (undercover375-add 5 8)
       (undercover375-classify 11)
       (undercover-report)
       (list :first first
             :second (undercover375-test-simplecov-state report root)
             :tracked (undercover375-test-tracked-files root)
             :coverage (undercover375-test-coverage root))))))"####,
        expect![[r#"OK (:first (:timestamp-integer t :files (("legacy/retired.el" (nil 4 0)) ("project Ω/lib space/core.el" (1 1 2 1 1 2 1 2 1 1 1 1 1 1 1 1 1 1 1 1)))) :second (:timestamp-integer t :files (("legacy/retired.el" (nil 4 0)) ("project Ω/lib space/core.el" (1 1 4 1 1 4 1 4 2 1 1 1 1 1 1 1 1 1 1 1)))) :tracked ("project Ω/lib space/core.el") :coverage (("project Ω/lib space/core.el" ((3 . 2) (6 . 2) (7 . 0) (8 . 2) (9 . 1)))))"#]],
    )
    .fresh_process()
}

fn report_on_kill_is_idempotent_and_visible_report_failure_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "report_on_kill_is_idempotent_and_visible_report_failure_recovers",
        r####"(undercover375-test-run-case
 "lifecycle-recovery"
 (lambda (root)
   (pcase-let* ((`(,core ,_extra ,_excluded)
                 (undercover375-test-make-project root))
                (report (expand-file-name "reports/lifecycle.txt" root))
                (default-directory root)
                (undercover-force-coverage t)
                (kill-emacs-hook nil))
     (undercover (:files core)
                 (:report-format 'text) (:report-file report)
                 (:report-on-kill t) (:send-report nil)
                 (:merge-report nil) (:verbosity 0))
     (undercover-report-on-kill)
     (undercover375-test-load core)
     (let* ((value (undercover375-classify 12))
            (hook-before (copy-sequence kill-emacs-hook))
            (_hook-result (run-hooks 'kill-emacs-hook))
            (first-report (undercover375-test-read report))
            (first-digest (secure-hash 'sha256 first-report))
            (failure (condition-case error
                         (progn (undercover-report 'unsupported) :no-error)
                       (error error)))
            (after-failure (undercover375-test-read report)))
       (undercover-report 'text)
       (list :value value
             :hook hook-before
             :hook-count (cl-count 'undercover-safe-report hook-before :test #'eq)
             :first-report first-report
             :failure failure
             :failure-atomic
             (and (equal first-report after-failure)
                  (equal first-digest (secure-hash 'sha256 after-failure)))
             :recovered-report (undercover375-test-read report)
             :coverage (undercover375-test-coverage root))))))"####,
        expect![[r#"OK (:value (positive 12 "界") :hook (undercover-safe-report) :hook-count 1 :first-report "== Code coverage text report ==\ncore : Percent 60% [Relevant: 5 Covered: 3 Missed: 2]\n" :failure (error "UNDERCOVER: Unsupported report-format") :failure-atomic t :recovered-report "== Code coverage text report ==\ncore : Percent 60% [Relevant: 5 Covered: 3 Missed: 2]\n" :coverage (("project Ω/lib space/core.el" ((3 . 0) (6 . 1) (7 . 0) (8 . 1) (9 . 1)))))"#]],
    )
    .fresh_process()
}

fn disabled_setup_is_a_noop_and_the_same_public_flow_can_then_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabled_setup_is_a_noop_and_the_same_public_flow_can_then_recover",
        r####"(undercover375-test-run-case
 "disabled-recovery"
 (lambda (root)
   (pcase-let* ((`(,core ,_extra ,_excluded)
                 (undercover375-test-make-project root))
                (report (expand-file-name "reports/recovered.txt" root))
                (default-directory root)
                (process-environment (undercover375-test-clean-environment))
                (undercover-force-coverage nil)
                (undercover--env nil)
                (kill-emacs-hook nil))
     (undercover (:files core)
                 (:report-format 'text) (:report-file report)
                 (:report-on-kill nil) (:send-report nil))
     (undercover375-test-load core)
     (undercover-report)
     (let ((disabled
            (list :enabled (and (undercover-enabled-p) t)
                  :tracked undercover--files
                  :instrumented (and (get 'undercover375-add 'edebug) t)
                  :handlers (undercover375-test-handler-count)
                  :report-exists (file-exists-p report)
                  :message (undercover375-test-last-message))))
       (let ((undercover-force-coverage t))
         (undercover (:files core)
                     (:report-format 'text) (:report-file report)
                     (:report-on-kill nil) (:send-report nil)
                     (:merge-report nil) (:verbosity 0))
         (undercover375-test-load core)
         (let ((value (undercover375-add 21 21)))
           (undercover-report)
           (list :disabled disabled
                 :recovered
                 (list :enabled (and (undercover-enabled-p) t)
                       :value value
                       :tracked (undercover375-test-tracked-files root)
                       :instrumented (and (get 'undercover375-add 'edebug) t)
                       :handlers (undercover375-test-handler-count)
                       :report (undercover375-test-read report)
                       :coverage (undercover375-test-coverage root)))))))))"####,
        expect![[r#"OK (:disabled (:enabled nil :tracked nil :instrumented nil :handlers 0 :report-exists nil :message "UNDERCOVER: No coverage information. Make sure that your files are not compiled?") :recovered (:enabled t :value 42 :tracked ("project Ω/lib space/core.el") :instrumented t :handlers 1 :report "== Code coverage text report ==\ncore : Percent 20% [Relevant: 5 Covered: 1 Missed: 4]\n" :coverage (("project Ω/lib space/core.el" ((3 . 1) (6 . 0) (7 . 0) (8 . 0) (9 . 0))))))"#]],
    )
    .fresh_process()
}

#[test]
fn undercover_practical_workflows_batch() {
    let cases = vec![
        public_setup_instruments_included_files_and_writes_a_text_report(),
        ci_and_environment_configuration_select_exact_files_and_lcov_output(),
        simplecov_report_merges_existing_coverage_and_accumulates_public_runs(),
        report_on_kill_is_idempotent_and_visible_report_failure_recovers(),
        disabled_setup_is_a_noop_and_the_same_public_flow_can_then_recover(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "undercover_practical_workflows_batch",
        "undercover_parity",
        &cases,
    );
}
