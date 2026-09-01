use expect_test::expect;

use super::ParityBatchCase;

fn applescript_mode_authors_comments_navigates_and_saves_a_real_script() -> ParityBatchCase {
    ParityBatchCase::value(
        "applescript_mode_authors_comments_navigates_and_saves_a_real_script",
        r##"(let* ((sandbox
                  (getenv
                   "NEOMACS_TEST_SANDBOX_ROOT"))
                 (path
                  (expand-file-name
                   "weekly-report.scpt"
                   sandbox))
                 (source
                  (concat
                   "on buildReport(ownerName, completedCount)\n"
                   "    tell application \"Finder\"\n"
                   "        if completedCount is 0 then\n"
                   "            set reportText to ownerName & \": no completed tasks\"\n"
                   "        else\n"
                   "            set reportText to ownerName & \": \" & completedCount\n"
                   "        end if\n"
                   "        display dialog reportText buttons {\"OK\"} default button \"OK\"\n"
                   "    end tell\n"
                   "end buildReport\n"
                   "\n"
                   "buildReport(\"Ada\", 7)")))
           (unwind-protect
               (with-temp-buffer
                 (setq-local
                  buffer-file-name
                  path)
                 (insert source)
                 (set-auto-mode)
                 (font-lock-ensure)
                 (let ((mode-state
                        (list
                         major-mode
                         mode-name
                         (applescript-test-face-at
                          "buildReport")
                         (applescript-test-face-at
                          "tell")
                         (applescript-test-face-at
                          "else")
                         (applescript-test-face-at
                          "no completed tasks"))))
                   (goto-char
                    (point-min))
                   (outline-minor-mode 1)
                   (outline-hide-subtree)
                   (forward-line 1)
                   (let ((body-hidden
                          (and
                           (invisible-p
                            (point))
                           t)))
                     (goto-char
                      (point-min))
                     (outline-show-subtree)
                     (outline-next-heading)
                     (let ((next-heading
                            (buffer-substring-no-properties
                             (line-beginning-position)
                             (line-end-position))))
                       (search-forward
                        "display dialog")
                       (let ((start
                              (line-beginning-position)))
                         (comment-region
                          start
                          (line-end-position))
                         (let ((commented-line
                                (buffer-substring-no-properties
                                 (line-beginning-position)
                                 (line-end-position)))
                               (inside-comment
                                (progn
                                  (search-backward
                                   "display")
                                  (nth
                                   4
                                   (syntax-ppss)))))
                           (uncomment-region
                            (line-beginning-position)
                            (line-end-position))
                           (let ((make-backup-files nil)
                                 (backup-inhibited t)
                                 (auto-save-default nil))
                             (save-buffer))
                           (list
                            mode-state
                            body-hidden
                            next-heading
                            commented-line
                            inside-comment
                            (buffer-substring-no-properties
                             (point-min)
                             (point-max))
                            (with-temp-buffer
                              (insert-file-contents-literally
                               path)
                              (buffer-string))
                            (buffer-modified-p))))))))
             (when
                 (file-exists-p path)
               (delete-file path))))"##,
        expect![[
            r#"OK ((applescript-mode "AppleScript" font-lock-function-name-face font-lock-keyword-face font-lock-keyword-face font-lock-string-face) t "    tell application \"Finder\"" "        -- display dialog reportText buttons {\"OK\"} default button \"OK\"" t "on buildReport(ownerName, completedCount)\n    tell application \"Finder\"\n        if completedCount is 0 then\n            set reportText to ownerName & \": no completed tasks\"\n        else\n            set reportText to ownerName & \": \" & completedCount\n        end if\n        display dialog reportText buttons {\"OK\"} default button \"OK\"\n    end tell\nend buildReport\n\nbuildReport(\"Ada\", 7)\n" "on buildReport(ownerName, completedCount)\n    tell application \"Finder\"\n        if completedCount is 0 then\n            set reportText to ownerName & \": no completed tasks\"\n        else\n            set reportText to ownerName & \": \" & completedCount\n        end if\n        display dialog reportText buttons {\"OK\"} default button \"OK\"\n    end tell\nend buildReport\n\nbuildReport(\"Ada\", 7)\n" nil)"#
        ]],
    )
}

fn applescript_mode_executes_a_selected_japanese_script_through_the_real_command_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "applescript_mode_executes_a_selected_japanese_script_through_the_real_command_path",
        r##"(save-window-excursion
           (let ((applescript-test-source-buffer
                  (generate-new-buffer
                   "*weekly-report-source*"))
                 (applescript-test-result-buffer
                  (get-buffer-create
                   as-output-buffer))
                 received)
             (unwind-protect
                 (progn
                   (with-current-buffer applescript-test-result-buffer
                     (erase-buffer)
                     (insert
                      "Earlier run: cancelled\n"))
                   (switch-to-buffer applescript-test-source-buffer)
                   (applescript-mode)
                   (insert
                    "set exportPath to \"Macintosh HD:Users:Ada:週報.txt\"\n"
                    "set reportText to \"完了: 7\\\\10\"\n"
                    "return reportText\n")
                   (let ((source-window
                          (selected-window)))
                     (cl-letf
                         (((symbol-function
                            'do-applescript)
                           (lambda (encoded-source)
                             (setq received
                                   encoded-source)
                             (as-encode-string
                              "完了: 7/10"))))
                       (as-execute-region
                        (point-min)
                        (point-max)))
                     (list
                      (string-to-list
                       received)
                      (with-current-buffer applescript-test-result-buffer
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max)))
                      (eq
                       source-window
                       (selected-window))
                      (eq
                       applescript-test-source-buffer
                       (current-buffer))
                      (buffer-string))))
               (applescript-test-kill-buffers
                "\\(weekly-report-source\\|AppleScript Output\\)"))))"##,
        expect![[
            r#"OK ((115 101 116 32 101 120 112 111 114 116 80 97 116 104 32 116 111 32 34 77 97 99 105 110 116 111 115 104 32 72 68 58 85 115 101 114 115 58 65 100 97 58 143 84 149 241 46 116 120 116 34 13 115 101 116 32 114 101 112 111 114 116 84 101 120 116 32 116 111 32 34 138 174 151 185 58 32 55 92 92 92 92 49 48 34 13 114 101 116 117 114 110 32 114 101 112 111 114 116 84 101 120 116 13) "Earlier run: cancelled\n完了: 7/10" t t "set exportPath to \"Macintosh HD:Users:Ada:週報.txt\"\nset reportText to \"完了: 7\\\\10\"\nreturn reportText\n")"#
        ]],
    )
}

fn applescript_mode_preserves_previous_results_and_exposes_a_failed_rerun() -> ParityBatchCase {
    ParityBatchCase::value(
        "applescript_mode_preserves_previous_results_and_exposes_a_failed_rerun",
        r##"(save-window-excursion
           (let ((source
                  (generate-new-buffer
                   "*invoice-source*"))
                 (output
                  (get-buffer-create
                   as-output-buffer))
                 (attempt 0)
                 events
                 source-window)
             (unwind-protect
                 (progn
                   (with-current-buffer output
                     (erase-buffer)
                     (insert
                      "Invoice 1041 exported\n"))
                   (switch-to-buffer source)
                   (setq source-window
                         (selected-window))
                   (insert
                    "set invoiceNumber to 1042\n"
                    "return \"Invoice \" & invoiceNumber & \" exported\"\n")
                   (cl-letf
                       (((symbol-function
                         'do-applescript)
                         (lambda (encoded-source)
                           (setq attempt
                                 (1+ attempt))
                           (setq events
                                 (append
                                  events
                                  (list
                                   (list
                                    (string-bytes
                                     encoded-source)
                                    (if
                                        (= attempt 1)
                                        :success
                                      :compile-error)))))
                           (if
                               (= attempt 1)
                               (as-encode-string
                                "Invoice 1042 exported")
                             (signal
                              'error
                              '("AppleScript compile failed: Expected end of line"
                                2
                                14))))))
                     (as-execute-buffer)
                     (let ((after-success
                            (with-current-buffer output
                              (buffer-string))))
                       (let ((failure
                              (condition-case error
                                  (as-execute-buffer)
                                (error
                                 (list
                                  (car error)
                                  (cadr error)
                                  (caddr error)
                                  (cadddr error))))))
                         (list
                          after-success
                          (with-current-buffer output
                            (buffer-string))
                          failure
                          events
                          (eq
                           source-window
                           (selected-window))
                          (buffer-name
                           (current-buffer)))))))
               (kill-buffer source)
               (kill-buffer output))))"##,
        expect![[
            r#"OK ("Invoice 1041 exported\nInvoice 1042 exported" "Invoice 1041 exported\nInvoice 1042 exported" (error "AppleScript compile failed: Expected end of line" 2 14) ((74 :success) (74 :compile-error)) nil "*AppleScript Output*")"#
        ]],
    )
}

fn applescript_mode_runs_a_one_off_script_and_displays_its_result() -> ParityBatchCase {
    ParityBatchCase::value(
        "applescript_mode_runs_a_one_off_script_and_displays_its_result",
        r##"(save-window-excursion
           (let ((as-output-buffer
                  "*AppleScript One-Off Output*")
                 (source-buffer
                  (generate-new-buffer
                   "*AppleScript Caller*"))
                 received)
             (unwind-protect
                 (progn
                   (switch-to-buffer source-buffer)
                   (insert
                    "Notes for the release")
                   (let ((source-window
                          (selected-window)))
                     (cl-letf
                         (((symbol-function
                            'do-applescript)
                           (lambda (encoded-source)
                             (setq received
                                   encoded-source)
                             (as-encode-string
                              "\"Release Notes\""))))
                       (as-execute-string
                        "tell application \"Finder\" to get name of front window"))
                     (list
                      (as-decode-string
                       received)
                      (with-current-buffer as-output-buffer
                        (buffer-string))
                      (eq
                       source-window
                       (selected-window))
                      (eq
                       source-buffer
                       (current-buffer))
                      (buffer-string))))
               (applescript-test-kill-buffers
                "\\(AppleScript One-Off Output\\|AppleScript Caller\\)"))))"##,
        expect![[
            r#"OK ("tell application \"Finder\" to get name of front window" "tell application \"Finder\" to get name of front window\"Release Notes\"" t t "Notes for the release")"#
        ]],
    )
}

fn applescript_mode_parses_realistic_osascript_structured_results_for_application_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "applescript_mode_parses_realistic_osascript_structured_results_for_application_code",
        r##"(cl-letf
           (((symbol-function
              'do-applescript)
             (lambda (_encoded-source)
               (as-encode-string
                "{name:\"Ada\",completed:7,priority:\"urgent\"}"))))
         (let* ((raw-result
                 (as-execute-code
                  "tell application \"Task Manager\" to get project summary"))
                (parsed-result
                 (as-parse-result
                  raw-result))
                (name
                 (cdr
                  (assq
                   'name
                   parsed-result)))
                (completed
                 (cdr
                  (assq
                   'completed
                   parsed-result)))
                (priority
                 (cdr
                  (assq
                   'priority
                   parsed-result))))
           (list
            raw-result
            parsed-result
            (format
             "%s completed %d tasks with %s priority"
             name
             completed
             priority))))"##,
        expect![[
            r#"OK ("{name:\"Ada\",completed:7,priority:\"urgent\"}" ((name . "Ada") (completed . 7) (priority . "urgent")) "Ada completed 7 tasks with urgent priority")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        applescript_mode_authors_comments_navigates_and_saves_a_real_script(),
        applescript_mode_executes_a_selected_japanese_script_through_the_real_command_path(),
        applescript_mode_preserves_previous_results_and_exposes_a_failed_rerun(),
        applescript_mode_runs_a_one_off_script_and_displays_its_result(),
        applescript_mode_parses_realistic_osascript_structured_results_for_application_code(),
    ]
}
