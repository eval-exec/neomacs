use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, OPEN_JUNK_FILE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'open-junk-file)

(defun neomacs-open-junk-file-test-root (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun neomacs-open-junk-file-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-open-junk-file-test-kill-under (root)
  (dolist (buffer (buffer-list))
    (with-current-buffer buffer
      (when (and buffer-file-name
                 (string-prefix-p (file-truename root)
                                  (file-truename buffer-file-name)))
        (set-buffer-modified-p nil)
        (kill-buffer buffer)))))
"####;

fn default_workflow_creates_the_dated_directory_and_passes_the_editable_name_to_the_opener()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-open-junk-file-test-root "open-junk-default"))
       (open-junk-file-format
        (concat root "/notes/%Y/%m/%d-%H%M%S."))
       (open-junk-file-find-file-function
        (lambda (file) (list :opened file)))
       prompts)
  (unwind-protect
      (cl-letf (((symbol-function 'current-time)
                 (lambda () 1704164645))
                ((symbol-function 'read-string)
                 (lambda (prompt initial &rest _)
                   (push (list prompt initial) prompts)
                   (concat initial "el"))))
        (let ((result (open-junk-file)))
          (list :result result
                :prompts (nreverse prompts)
                :year-directory (file-directory-p
                                 (concat root "/notes/2024"))
                :month-directory (file-directory-p
                                  (concat root "/notes/2024/01")))))
    (when (file-exists-p root) (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:result (:opened "[ORACLE-SANDBOX]/open-junk-default/notes/2024/01/02-030405.el") :prompts (("Junk Code (Enter extension): " "[ORACLE-SANDBOX]/open-junk-default/notes/2024/01/02-030405.")) :year-directory t :month-directory t)"#
    ]];
    ParityBatchCase::value(
        "default_workflow_creates_the_dated_directory_and_passes_the_editable_name_to_the_opener",
        elisp_form,
        expected,
    )
}

fn opening_a_real_junk_file_runs_the_hook_and_persists_searchable_scratch_code() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root (neomacs-open-junk-file-test-root "open-junk-real-file"))
       (open-junk-file-format
        (concat root "/journal/%Y/%m/%d-%H%M%S."))
       (open-junk-file-find-file-function #'find-file-noselect)
       hook-events prompts buffer)
  (unwind-protect
      (let ((open-junk-file-hook
             (list (lambda ()
                     (push (list (file-relative-name buffer-file-name root)
                                 major-mode)
                           hook-events)))))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () 1704164645))
                  ((symbol-function 'read-string)
                   (lambda (prompt initial &rest _)
                     (push (list prompt initial) prompts)
                     (concat initial "el"))))
          (setq buffer (open-junk-file))
          (with-current-buffer buffer
            (let ((file buffer-file-name))
              (insert "(message \"release experiment Ω\")\n")
              (save-buffer)
              (list :file (file-relative-name file root)
                    :mode major-mode
                    :contents (buffer-substring-no-properties
                               (point-min) (point-max))
                    :disk (with-temp-buffer
                            (insert-file-contents file)
                            (buffer-string))
                    :modified (buffer-modified-p)
                    :hooks (nreverse hook-events)
                    :prompts (nreverse prompts))))))
    (neomacs-open-junk-file-test-kill-under root)
    (when (file-exists-p root) (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:file "journal/2024/01/02-030405.el" :mode emacs-lisp-mode :contents "(message \"release experiment Ω\")\n" :disk "(message \"release experiment Ω\")\n" :modified nil :hooks (("journal/2024/01/02-030405.el" emacs-lisp-mode)) :prompts (("Junk Code (Enter extension): " "[ORACLE-SANDBOX]/open-junk-real-file/journal/2024/01/02-030405.")))"#
    ]];
    ParityBatchCase::value(
        "opening_a_real_junk_file_runs_the_hook_and_persists_searchable_scratch_code",
        elisp_form,
        expected,
    )
}

fn junk_hook_uses_canonical_paths_and_ignores_files_outside_the_configured_tree() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root (neomacs-open-junk-file-test-root "open-junk-hook-routing"))
       (real-junk (concat root "/storage/junk"))
       (alias-junk (concat root "/linked-junk"))
       (junk-file (concat real-junk "/2024/incident.txt"))
       (outside-file (concat root "/storage/ordinary.txt"))
       (open-junk-file-format (concat alias-junk "/%Y/%m/%d-%H%M%S."))
       hook-events buffers)
  (unwind-protect
      (progn
        (make-directory (file-name-directory junk-file) t)
        (make-directory (file-name-directory outside-file) t)
        (with-temp-file junk-file (insert "incident scratch\n"))
        (with-temp-file outside-file (insert "ordinary note\n"))
        (make-symbolic-link real-junk alias-junk)
        (let ((open-junk-file-hook
               (list (lambda ()
                       (push (file-relative-name buffer-file-name root)
                             hook-events)))))
          (push (find-file-noselect junk-file) buffers)
          (push (find-file-noselect outside-file) buffers))
        (list :format-root (file-truename
                            (replace-regexp-in-string
                             "%.*$" "" open-junk-file-format))
              :junk-truename (file-truename junk-file)
              :hook-events (nreverse hook-events)
              :hook-installed
              (and (memq #'find-file-hook--open-junk-file find-file-hook) t)))
    (dolist (buffer buffers)
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))
    (when (file-exists-p root) (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:format-root "[ORACLE-SANDBOX]/open-junk-hook-routing/storage/junk/" :junk-truename "[ORACLE-SANDBOX]/open-junk-hook-routing/storage/junk/2024/incident.txt" :hook-events ("storage/junk/2024/incident.txt") :hook-installed t)"#
    ]];
    ParityBatchCase::value(
        "junk_hook_uses_canonical_paths_and_ignores_files_outside_the_configured_tree",
        elisp_form,
        expected,
    )
}

fn variable_alias_and_per_call_overrides_support_distinct_junk_workflows() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-open-junk-file-test-root "open-junk-overrides"))
       (saved-format open-junk-file-format)
       (saved-finder open-junk-file-find-file-function)
       default-calls override-calls prompts format-from-directory)
  (unwind-protect
      (progn
        (setq open-junk-file-directory
              (concat root "/default/%Y%m%d-%H%M%S."))
        (setq format-from-directory open-junk-file-format)
        (setq open-junk-file-find-file-function
              (lambda (file) (push file default-calls) :default-opened))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () 1704164645))
                  ((symbol-function 'read-string)
                   (lambda (prompt initial &rest _)
                     (push (list prompt initial) prompts)
                     (concat initial "txt"))))
          (let ((default-result (open-junk-file))
                (override-result
                 (open-junk-file
                  (concat root "/override/%Y-%m-%d-%H%M%S.")
                  (lambda (file)
                    (push file override-calls)
                    :override-opened))))
            (setq open-junk-file-format
                  (concat root "/renamed/%Y."))
            (list :default-result default-result
                  :override-result override-result
                  :default-calls (nreverse default-calls)
                  :override-calls (nreverse override-calls)
                  :prompts (nreverse prompts)
                  :format-after-directory-set format-from-directory
                  :directory-after-format-set open-junk-file-directory
                  :same-storage
                  (eq (indirect-variable 'open-junk-file-format)
                      'open-junk-file-directory)))))
    (setq open-junk-file-format saved-format
          open-junk-file-find-file-function saved-finder)
    (when (file-exists-p root) (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:default-result :default-opened :override-result :override-opened :default-calls ("[ORACLE-SANDBOX]/open-junk-overrides/default/20240102-030405.txt") :override-calls ("[ORACLE-SANDBOX]/open-junk-overrides/override/2024-01-02-030405.txt") :prompts (("Junk Code (Enter extension): " "[ORACLE-SANDBOX]/open-junk-overrides/default/20240102-030405.") ("Junk Code (Enter extension): " "[ORACLE-SANDBOX]/open-junk-overrides/override/2024-01-02-030405.")) :format-after-directory-set "[ORACLE-SANDBOX]/open-junk-overrides/default/%Y%m%d-%H%M%S." :directory-after-format-set "[ORACLE-SANDBOX]/open-junk-overrides/renamed/%Y." :same-storage t)"#
    ]];
    ParityBatchCase::value(
        "variable_alias_and_per_call_overrides_support_distinct_junk_workflows",
        elisp_form,
        expected,
    )
}

fn invalid_formats_unvisited_buffers_and_failing_openers_preserve_exact_failure_state()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-open-junk-file-test-root "open-junk-failures"))
       (failure-format (concat root "/created-before-opener/%Y%m%d."))
       prompts)
  (unwind-protect
      (cl-letf (((symbol-function 'current-time)
                 (lambda () 1704164645))
                ((symbol-function 'read-string)
                 (lambda (prompt initial &rest _)
                   (push (list prompt initial) prompts)
                   (concat initial "el"))))
        (let ((missing-directory
               (neomacs-open-junk-file-test-capture
                (lambda ()
                  (open-junk-file "memo-%Y%m%d-%H%M%S."
                                  (lambda (file) file)))))
              (unvisited-hook
               (with-temp-buffer
                 (neomacs-open-junk-file-test-capture
                  #'find-file-hook--open-junk-file)))
              (opener-failure
               (neomacs-open-junk-file-test-capture
                (lambda ()
                  (open-junk-file
                   failure-format
                   (lambda (file)
                     (error "fixture opener rejected %s"
                            (file-name-nondirectory file))))))))
          (list :missing-directory missing-directory
                :unvisited-hook unvisited-hook
                :opener-failure opener-failure
                :directory-remains
                (file-directory-p (file-name-directory
                                   (format-time-string failure-format
                                                       1704164645)))
                :prompts (nreverse prompts))))
    (when (file-exists-p root) (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:missing-directory (:error wrong-type-argument :data (stringp nil) :message "Wrong type argument: stringp, nil") :unvisited-hook (:error wrong-type-argument :data (arrayp nil) :message "Wrong type argument: arrayp, nil") :opener-failure (:error error :data ("fixture opener rejected 20240102.el") :message "fixture opener rejected 20240102.el") :directory-remains t :prompts (("Junk Code (Enter extension): " "[ORACLE-SANDBOX]/open-junk-failures/created-before-opener/20240102.")))"#
    ]];
    ParityBatchCase::value(
        "invalid_formats_unvisited_buffers_and_failing_openers_preserve_exact_failure_state",
        elisp_form,
        expected,
    )
}

fn bug_report_command_submits_complete_package_metadata_without_sending_mail() -> ParityBatchCase {
    let elisp_form = r####"
(cl-letf (((symbol-function 'reporter-submit-bug-report)
           (lambda (address package variables pre-hooks post-hooks salutation)
             (list :address address
                   :package package
                   :variables
                   (sort (mapcar #'symbol-name variables) #'string<)
                   :pre-hooks pre-hooks
                   :post-hooks post-hooks
                   :salutation-lines (length (split-string salutation "\n"))
                   :has-reproduction-guide
                   (and (string-match-p "precise recipe" salutation)
                        (string-match-p "paste \\*Backtrace\\*" salutation)
                        t)))))
  (list :result (open-junk-file-send-bug-report)
        :command (commandp 'open-junk-file-send-bug-report)
        :maintainer open-junk-file-maintainer-mail-address
        :hook-variable (listp open-junk-file-hook)))
"####;
    let expected = expect![[
        r#"OK (:result (:address "rubikitch@ruby-lang.org" :package "open-junk-file.el" :variables ("open-junk-file-bug-report-salutation" "open-junk-file-directory" "open-junk-file-find-file-function" "open-junk-file-format" "open-junk-file-hook" "open-junk-file-maintainer-mail-address") :pre-hooks nil :post-hooks nil :salutation-lines 11 :has-reproduction-guide t) :command t :maintainer "rubikitch@ruby-lang.org" :hook-variable t)"#
    ]];
    ParityBatchCase::value(
        "bug_report_command_submits_complete_package_metadata_without_sending_mail",
        elisp_form,
        expected,
    )
}

#[test]
fn open_junk_file_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(OPEN_JUNK_FILE_MELPA_PIN, "open-junk-file.el")
            .expect("prepare revision-pinned Open Junk File source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "open-junk-file-package-batch",
        "Open Junk File",
        &[
            default_workflow_creates_the_dated_directory_and_passes_the_editable_name_to_the_opener(
            ),
            opening_a_real_junk_file_runs_the_hook_and_persists_searchable_scratch_code(),
            junk_hook_uses_canonical_paths_and_ignores_files_outside_the_configured_tree(),
            variable_alias_and_per_call_overrides_support_distinct_junk_workflows(),
            invalid_formats_unvisited_buffers_and_failing_openers_preserve_exact_failure_state(),
            bug_report_command_submits_complete_package_metadata_without_sending_mail(),
        ],
    );
}
