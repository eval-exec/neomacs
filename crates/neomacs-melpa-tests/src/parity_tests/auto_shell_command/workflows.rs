use expect_test::expect;

use super::ParityBatchCase;

fn pristine_byte_compiled_dispatch_surfaces_the_missing_runtime_find_if() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (let* ((root
         (file-name-as-directory
           (expand-file-name "auto-shell-pristine-runtime"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/source.txt" root))
         (find-if-was-bound (fboundp 'find-if))
         (old-find-if (and find-if-was-bound (symbol-function 'find-if)))
         buffer outcome result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (progn
            (neomacs-ascmd-test--write-file source "before\n")
            (ascmd:add (list (regexp-quote source) "printf unreachable"))
            (setq buffer (find-file-noselect source))
            ;; Remove compatibility state loaded by the test harness so this
            ;; probe reflects the package's own runtime requirements.
            (fmakunbound 'find-if)
            (setq outcome
                  (condition-case error-data
                      (with-current-buffer buffer
                        (goto-char (point-max))
                        (insert "after\n")
                        (save-buffer)
                        'returned)
                    (error error-data)))
            (setq result
                  (list
                   :outcome outcome
                   :disk (neomacs-ascmd-test--file-text source)
                   :modified (with-current-buffer buffer (buffer-modified-p))
                   :queue ascmd:process-queue
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name))))
        (if find-if-was-bound
            (fset 'find-if old-find-if)
          (fmakunbound 'find-if))
        (neomacs-ascmd-test--cleanup (list buffer) root)))
    result))
"####;
    let expect = expect![
        r#"OK (:outcome (void-function find-if) :disk "before\nafter\n" :modified nil :queue nil :result-buffer nil)"#
    ];
    ParityBatchCase::value(
        "pristine_byte_compiled_dispatch_surfaces_the_missing_runtime_find_if",
        elisp_form,
        expect,
    )
}

fn saving_a_matching_file_runs_a_real_command_with_file_and_directory_substitution()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let* ((root
          (file-name-as-directory
           (expand-file-name "auto-shell-success"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/src/report.txt" root))
         (program (expand-file-name "bin/build-report" root))
         (log (expand-file-name "invocations.log" root))
         (origin (current-buffer))
         buffer result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (progn
            (neomacs-ascmd-test--write-file source "draft\n")
            (neomacs-ascmd-test--write-program
             program
             "printf 'cwd=%s\\nfile=%s\\n' \"$PWD\" \"$1\" >> \"$2\"\nprintf 'compiled:%s\\n' \"$(cat \"$1\")\"\n")
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s \"$FILE\" %s"
                      (shell-quote-argument program)
                      (shell-quote-argument log))))
            (setq buffer (find-file-noselect source))
            (with-current-buffer buffer
              (goto-char (point-max))
              (insert "saved Ω\n")
              (save-buffer))
            (neomacs-ascmd-test--wait-for-idle)
            (setq result
                  (list
                   :disk (neomacs-ascmd-test--file-text source)
                   :invocation (neomacs-ascmd-test--file-text log)
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :queue ascmd:process-queue
                   :display (ascmd:display-process-count)
                   :current-buffer-preserved (eq (current-buffer) origin))))
        (neomacs-ascmd-test--cleanup (list buffer) root)))
    result))
"####;
    let expect = expect![[
        r#"OK (:disk "draft\nsaved Ω\n" :invocation "cwd=[ORACLE-SANDBOX]/auto-shell-success/project/src\nfile=report.txt\n" :result-buffer "compiled:draft\nsaved Ω\n" :queue nil :display nil :current-buffer-preserved t)"#
    ]];
    ParityBatchCase::value(
        "saving_a_matching_file_runs_a_real_command_with_file_and_directory_substitution",
        elisp_form,
        expect,
    )
}

fn newest_matching_rule_has_priority_in_a_real_save_workflow() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let* ((root
          (file-name-as-directory
           (expand-file-name "auto-shell-priority"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/docs/guide.txt" root))
         (program (expand-file-name "bin/record-build" root))
         (log (expand-file-name "selected-rule.log" root))
         buffer result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (progn
            (neomacs-ascmd-test--write-file source "guide v1\n")
            (neomacs-ascmd-test--write-program
             program
             "printf '%s:%s\\n' \"$1\" \"$(cat \"$2\")\" >> \"$3\"\nprintf 'selected:%s\\n' \"$1\"\n")
            (ascmd:add
             (list
              (regexp-quote (expand-file-name "project" root))
              (format "%s broad \"$FILE\" %s"
                      (shell-quote-argument program)
                      (shell-quote-argument log))))
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s specific \"$FILE\" %s"
                      (shell-quote-argument program)
                      (shell-quote-argument log))))
            (setq buffer (find-file-noselect source))
            (with-current-buffer buffer
              (erase-buffer)
              (insert "guide v2 Ω\n")
              (save-buffer))
            (neomacs-ascmd-test--wait-for-idle)
            (setq result
                  (list
                   :configured-rules (length ascmd:setting)
                   :selected-log (neomacs-ascmd-test--file-text log)
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :queue ascmd:process-queue)))
        (neomacs-ascmd-test--cleanup (list buffer) root)))
    result))
"####;
    let expect = expect![[
        r#"OK (:configured-rules 2 :selected-log "specific:guide v2 Ω\n" :result-buffer "selected:specific\n" :queue nil)"#
    ]];
    ParityBatchCase::value(
        "newest_matching_rule_has_priority_in_a_real_save_workflow",
        elisp_form,
        expect,
    )
}

fn repeated_saves_queue_one_duplicate_and_suppress_later_adjacent_duplicates() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let* ((root
          (file-name-as-directory
           (expand-file-name "auto-shell-queue"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/job.txt" root))
         (program (expand-file-name "bin/blocked-build" root))
         (log (expand-file-name "queue.log" root))
         buffer queued result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (progn
            (neomacs-ascmd-test--write-file source "version 0\n")
            (neomacs-ascmd-test--write-program
             program
             "printf 'start:%s\\n' \"$1\" >> \"$2\"\nIFS= read -r token\nprintf 'done:%s:%s\\n' \"$1\" \"$token\" >> \"$2\"\nprintf 'result:%s:%s\\n' \"$1\" \"$token\"\n")
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s \"$FILE\" %s"
                      (shell-quote-argument program)
                      (shell-quote-argument log))))
            (setq buffer (find-file-noselect source))
            (with-current-buffer buffer
              (goto-char (point-max))
              (insert "version 1\n")
              (save-buffer))
            (neomacs-ascmd-test--wait-for-file log)
            (with-current-buffer buffer
              (goto-char (point-max))
              (insert "version 2\n")
              (save-buffer)
              (goto-char (point-max))
              (insert "version 3\n")
              (save-buffer))
            (setq queued
                  (list
                   :count (ascmd:process-count)
                   :display (ascmd:display-process-count)
                   :commands-identical
                   (and (= (length ascmd:process-queue) 2)
                        (string-equal (car ascmd:process-queue)
                                      (cadr ascmd:process-queue)))))
            (process-send-string
             (car (neomacs-ascmd-test--deferred-processes))
             "release-one\n")
            (unless
                (neomacs-ascmd-test--wait-until
                 (lambda ()
                   (let ((text (neomacs-ascmd-test--file-text log)))
                     (and text
                          (string-match-p
                           "done:job.txt:release-one\nstart:job.txt\n"
                           text)))))
              (error "second queued build did not start: %S"
                     (neomacs-ascmd-test--file-text log)))
            (process-send-string
             (car (neomacs-ascmd-test--deferred-processes))
             "release-two\n")
            (neomacs-ascmd-test--wait-for-idle)
            (setq result
                  (list
                   :while-blocked queued
                   :execution-log (neomacs-ascmd-test--file-text log)
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :final-queue ascmd:process-queue
                   :final-display (ascmd:display-process-count))))
        (neomacs-ascmd-test--cleanup (list buffer) root)))
    result))
"####;
    let expect = expect![[
        r#"OK (:while-blocked (:count 2 :display "[ascmd:2] " :commands-identical t) :execution-log "start:job.txt\ndone:job.txt:release-one\nstart:job.txt\ndone:job.txt:release-two\n" :result-buffer "result:job.txt:release-two\n" :final-queue nil :final-display nil)"#
    ]];
    ParityBatchCase::value(
        "repeated_saves_queue_one_duplicate_and_suppress_later_adjacent_duplicates",
        elisp_form,
        expect,
    )
}

fn toggle_stops_save_dispatch_then_resumes_the_same_project_rule() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let* ((root
          (file-name-as-directory
           (expand-file-name "auto-shell-toggle"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/article.txt" root))
         (program (expand-file-name "bin/render" root))
         (log (expand-file-name "renders.log" root))
         buffer stopped result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (progn
            (neomacs-ascmd-test--write-file source "first\n")
            (neomacs-ascmd-test--write-program
             program
             "printf 'render:%s\\n' \"$1\" >> \"$2\"\nprintf 'rendered:%s\\n' \"$(cat \"$1\")\"\n")
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s \"$FILE\" %s"
                      (shell-quote-argument program)
                      (shell-quote-argument log))))
            (setq buffer (find-file-noselect source))
            (ascmd:toggle)
            (with-current-buffer buffer
              (goto-char (point-max))
              (insert "while stopped\n")
              (save-buffer))
            (setq stopped
                  (list
                   :active ascmd:active
                   :display (ascmd:display-process-count)
                   :queue ascmd:process-queue
                   :log (neomacs-ascmd-test--file-text log)))
            (ascmd:toggle)
            (with-current-buffer buffer
              (goto-char (point-max))
              (insert "after resume Ω\n")
              (save-buffer))
            (neomacs-ascmd-test--wait-for-idle)
            (setq result
                  (list
                   :stopped-save stopped
                   :active-after-resume ascmd:active
                   :log-after-resume (neomacs-ascmd-test--file-text log)
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :queue ascmd:process-queue)))
        (neomacs-ascmd-test--cleanup (list buffer) root)))
    result))
"####;
    let expect = expect![[
        r#"OK (:stopped-save (:active nil :display "[ascmd:stop]" :queue nil :log nil) :active-after-resume t :log-after-resume "render:article.txt\n" :result-buffer "rendered:first\nwhile stopped\nafter resume Ω\n" :queue nil)"#
    ]];
    ParityBatchCase::value(
        "toggle_stops_save_dispatch_then_resumes_the_same_project_rule",
        elisp_form,
        expect,
    )
}

fn failed_real_command_preserves_diagnostics_then_next_save_recovers() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let* ((root
          (file-name-as-directory
           (expand-file-name "auto-shell-failure"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/broken.txt" root))
         (program (expand-file-name "bin/failing-check" root))
         (recovery-program (expand-file-name "bin/recovery-check" root))
         (recovery-log (expand-file-name "recovery.log" root))
         (message-start
          (with-current-buffer (messages-buffer) (point-max)))
         buffer failed-state result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (save-window-excursion
            (delete-other-windows)
            (neomacs-ascmd-test--write-file source "broken v1\n")
            (neomacs-ascmd-test--write-program
             program
             "printf 'diagnostic Ω for %s\\n' \"$1\"\nexit 7\n")
            (neomacs-ascmd-test--write-program
             recovery-program
             "printf 'recovered:%s\\n' \"$1\" >> \"$2\"\nprintf 'clean:%s\\n' \"$(cat \"$1\")\"\n")
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s \"$FILE\""
                      (shell-quote-argument program))))
            (setq buffer (find-file-noselect source))
            (switch-to-buffer buffer)
            (goto-char (point-max))
            (insert "broken v2\n")
            (save-buffer)
            (neomacs-ascmd-test--wait-for-idle)
            (setq failed-state
                  (list
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :result-visible
                   (and (get-buffer-window ascmd:buffer-name) t)
                   :selected-buffer (buffer-name (window-buffer))
                   :window-count (length (window-list))
                   :queue ascmd:process-queue
                   :display (ascmd:display-process-count)))
            (ascmd:remove-all)
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s \"$FILE\" %s"
                      (shell-quote-argument recovery-program)
                      (shell-quote-argument recovery-log))))
            (with-current-buffer buffer
              (goto-char (point-max))
              (insert "recovered v3\n")
              (save-buffer))
            (neomacs-ascmd-test--wait-for-idle)
            (setq result
                  (list
                   :failed failed-state
                   :recovery-log
                   (neomacs-ascmd-test--file-text recovery-log)
                   :recovery-result
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :messages
                   (neomacs-ascmd-test--messages message-start)
                   :final-queue ascmd:process-queue
                   :final-display (ascmd:display-process-count))))
        (neomacs-ascmd-test--cleanup (list buffer) root)))
    result))
"####;
    let expect = expect![[
        r#"OK (:failed (:result-buffer "Deferred process exited abnormally:\n  command: cd [ORACLE-SANDBOX]/auto-shell-failure/project/ && ([ORACLE-SANDBOX]/auto-shell-failure/bin/failing-check \"broken.txt\")\n  exit status: exit 7\n  event: exited abnormally with code 7\n  buffer contents: \"diagnostic Ω for broken.txt\\n\"" :result-visible t :selected-buffer "broken.txt" :window-count 2 :queue nil :display nil) :recovery-log "recovered:broken.txt\n" :recovery-result "clean:broken v1\nbroken v2\nrecovered v3\n" :messages ("failed" "success") :final-queue nil :final-display nil)"#
    ]];
    ParityBatchCase::value(
        "failed_real_command_preserves_diagnostics_then_next_save_recovers",
        elisp_form,
        expect,
    )
}

fn external_rewrite_does_not_dispatch_and_revert_then_real_save_resumes_dispatch() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let* ((root
          (file-name-as-directory
           (expand-file-name "auto-shell-external-rewrite"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/source.txt" root))
         (external-program (expand-file-name "bin/external-writer" root))
         (build-program (expand-file-name "bin/build-after-save" root))
         (log (expand-file-name "builds.log" root))
         buffer external-status before-revert after-revert result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (progn
            (neomacs-ascmd-test--write-file source "original\n")
            (neomacs-ascmd-test--write-program
             external-program
             "printf 'external Ω\\n' > \"$1\"\ntouch -m -d '2035-01-02 03:04:05 UTC' \"$1\"\n")
            (neomacs-ascmd-test--write-program
             build-program
             "printf 'saved:%s\\n' \"$(cat \"$1\")\" >> \"$2\"\nprintf 'built:%s\\n' \"$(cat \"$1\")\"\n")
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s \"$FILE\" %s"
                      (shell-quote-argument build-program)
                      (shell-quote-argument log))))
            (setq buffer (find-file-noselect source))
            (setq external-status
                  (call-process external-program nil nil nil source))
            (with-current-buffer buffer
              (setq before-revert
                    (list
                     :external-status external-status
                     :buffer-text
                     (buffer-substring-no-properties (point-min) (point-max))
                     :disk-text (neomacs-ascmd-test--file-text source)
                     :visited-modtime-current (verify-visited-file-modtime buffer)
                     :build-log (neomacs-ascmd-test--file-text log)
                     :queue ascmd:process-queue))
              (let ((message-start
                     (with-current-buffer (messages-buffer) (point-max))))
                (revert-buffer t t)
                (setq after-revert
                      (list
                       :buffer-text
                       (buffer-substring-no-properties (point-min) (point-max))
                       :modified (buffer-modified-p)
                       :visited-modtime-current
                       (verify-visited-file-modtime buffer)
                       :messages
                       (neomacs-ascmd-test--messages message-start))))
              (goto-char (point-max))
              (insert "local save Ω\n")
              (save-buffer))
            (neomacs-ascmd-test--wait-for-idle)
            (setq result
                  (list
                   :before-revert before-revert
                   :after-revert after-revert
                   :build-log (neomacs-ascmd-test--file-text log)
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :queue ascmd:process-queue)))
        (neomacs-ascmd-test--cleanup (list buffer) root)))
    result))
"####;
    let expect = expect![[
        r#"OK (:before-revert (:external-status 0 :buffer-text "original\n" :disk-text "external Ω\n" :visited-modtime-current nil :build-log nil :queue nil) :after-revert (:buffer-text "external Ω\n" :modified nil :visited-modtime-current t :messages nil) :build-log "saved:external Ω\nlocal save Ω\n" :result-buffer "built:external Ω\nlocal save Ω\n" :queue nil)"#
    ]];
    ParityBatchCase::value(
        "external_rewrite_does_not_dispatch_and_revert_then_real_save_resumes_dispatch",
        elisp_form,
        expect,
    )
}

fn public_settings_commands_support_priority_removal_recovery_and_clipboard_reuse()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let ((ascmd:setting nil)
        (ascmd:process-queue nil)
        (ascmd:active t)
        (kill-ring nil)
        (kill-ring-yank-pointer nil)
        (interprogram-cut-function nil)
        (save-interprogram-paste-before-kill nil)
        (message-start
         (with-current-buffer (messages-buffer) (point-max)))
        before after-first-remove after-remove-all result)
    (unwind-protect
        (progn
          (ascmd:add '("project/.*\\.el" "make check"))
          (ascmd:add '("project/docs/" "make docs"))
          (setq before (copy-tree ascmd:setting))
          (ascmd:remove)
          (setq after-first-remove
                (list
                 :settings (copy-tree ascmd:setting)
                 :clipboard (car kill-ring)
                 :yank-pointer (car kill-ring-yank-pointer)))
          (ascmd:remove-all)
          (setq after-remove-all (copy-tree ascmd:setting))
          (ascmd:remove)
          (setq result
                (list
                 :priority-order before
                 :after-first-remove after-first-remove
                 :after-remove-all after-remove-all
                 :messages
                 (neomacs-ascmd-test--messages message-start))))
      (neomacs-ascmd-test--cleanup nil nil))
    result))
"####;
    let expect = expect![[
        r#"OK (:priority-order (("project/docs/" "make docs") ("project/.*\\.el" "make check")) :after-first-remove (:settings (("project/.*\\.el" "make check")) :clipboard "(ascmd:add '(\"project/docs/\" \"make docs\"))" :yank-pointer "(ascmd:add '(\"project/docs/\" \"make docs\"))") :after-remove-all nil :messages ("Remove : (ascmd:add ’(\"project/docs/\" \"make docs\"))" "Command list is empty."))"#
    ]];
    ParityBatchCase::value(
        "public_settings_commands_support_priority_removal_recovery_and_clipboard_reuse",
        elisp_form,
        expect,
    )
}

fn popup_command_selects_results_normally_and_preserves_selection_with_prefix() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let ((origin (generate-new-buffer "*ascmd project source*"))
        (output (get-buffer-create ascmd:buffer-name))
        normal prefixed result)
    (unwind-protect
        (save-window-excursion
          (with-current-buffer output
            (erase-buffer)
            (insert "build report Ω\n"))
          (delete-other-windows)
          (switch-to-buffer origin)
          (ascmd:popup nil)
          (setq normal
                (list
                 :selected (buffer-name (window-buffer))
                 :windows (length (window-list))
                 :text (neomacs-ascmd-test--buffer-text output)))
          (delete-other-windows)
          (switch-to-buffer origin)
          (ascmd:popup '(4))
          (setq prefixed
                (list
                 :selected (buffer-name (window-buffer))
                 :windows (length (window-list))
                 :output-visible (and (get-buffer-window output) t)
                 :output-selected
                 (eq (get-buffer-window output) (selected-window))))
          (setq result (list :normal normal :prefixed prefixed)))
      (neomacs-ascmd-test--cleanup (list origin output) nil))
    result))
"####;
    let expect = expect![[
        r#"OK (:normal (:selected "*Auto Shell Command*" :windows 2 :text "build report Ω\n") :prefixed (:selected "*ascmd project source*" :windows 2 :output-visible t :output-selected nil))"#
    ]];
    ParityBatchCase::value(
        "popup_command_selects_results_normally_and_preserves_selection_with_prefix",
        elisp_form,
        expect,
    )
}

fn interactive_exec_runs_the_selected_file_without_changing_the_current_buffer() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (neomacs-ascmd-test--load-package)
  (require 'cl)
  (let* ((root
          (file-name-as-directory
           (expand-file-name "auto-shell-explicit"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (source (expand-file-name "project/manual.txt" root))
         (program (expand-file-name "bin/manual-check" root))
         (log (expand-file-name "manual.log" root))
         (origin (generate-new-buffer "*ascmd command origin*"))
         result)
    (let ((ascmd:setting nil)
          (ascmd:process-queue nil)
          (ascmd:active t))
      (unwind-protect
          (progn
            (neomacs-ascmd-test--write-file source "manual payload Ω\n")
            (neomacs-ascmd-test--write-program
             program
             "printf 'manual:%s:%s\\n' \"$PWD\" \"$1\" >> \"$2\"\nprintf 'checked:%s\\n' \"$(cat \"$1\")\"\n")
            (ascmd:add
             (list
              (regexp-quote source)
              (format "%s \"$FILE\" %s"
                      (shell-quote-argument program)
                      (shell-quote-argument log))))
            (switch-to-buffer origin)
            (cl-letf (((symbol-function 'read-file-name)
                       (lambda (&rest _) source)))
              (call-interactively #'ascmd:exec))
            (neomacs-ascmd-test--wait-for-idle)
            (setq result
                  (list
                   :current-buffer (buffer-name)
                   :target-visited (and (get-file-buffer source) t)
                   :invocation (neomacs-ascmd-test--file-text log)
                   :result-buffer
                   (neomacs-ascmd-test--buffer-text ascmd:buffer-name)
                   :queue ascmd:process-queue)))
        (neomacs-ascmd-test--cleanup (list origin (get-file-buffer source)) root)))
    result))
"####;
    let expect = expect![[
        r#"OK (:current-buffer "*ascmd command origin*" :target-visited nil :invocation "manual:[ORACLE-SANDBOX]/auto-shell-explicit/project:manual.txt\n" :result-buffer "checked:manual payload Ω\n" :queue nil)"#
    ]];
    ParityBatchCase::value(
        "interactive_exec_runs_the_selected_file_without_changing_the_current_buffer",
        elisp_form,
        expect,
    )
}

pub(crate) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        pristine_byte_compiled_dispatch_surfaces_the_missing_runtime_find_if(),
        saving_a_matching_file_runs_a_real_command_with_file_and_directory_substitution(),
        newest_matching_rule_has_priority_in_a_real_save_workflow(),
        repeated_saves_queue_one_duplicate_and_suppress_later_adjacent_duplicates(),
        toggle_stops_save_dispatch_then_resumes_the_same_project_rule(),
        failed_real_command_preserves_diagnostics_then_next_save_recovers(),
        external_rewrite_does_not_dispatch_and_revert_then_real_save_resumes_dispatch(),
        public_settings_commands_support_priority_removal_recovery_and_clipboard_reuse(),
        popup_command_selects_results_normally_and_preserves_selection_with_prefix(),
        interactive_exec_runs_the_selected_file_without_changing_the_current_buffer(),
    ]
}
