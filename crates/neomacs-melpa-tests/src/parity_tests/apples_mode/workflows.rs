use expect_test::expect;

use super::ParityBatchCase;

fn author_completes_indents_navigates_and_saves_a_mail_automation_script() -> ParityBatchCase {
    ParityBatchCase::value(
        "author_completes_indents_navigates_and_saves_a_mail_automation_script",
        r##"(progn
                  (require 'imenu)
                  (let* ((script-file
                          (expand-file-name
                           "publish-report.applescript"
                           temporary-file-directory))
                         (apples-plist
                          (list :AS-version "2.1" :tmp-files nil))
                         result)
                    (with-current-buffer (find-file-noselect script-file)
                      (unwind-protect
                          (progn
                            (apples-mode)
                            (setq-local indent-tabs-mode nil
                                        imenu-auto-rescan t)
                            (insert
                             "property projectName : \"Release\"\n"
                             "-- Build one message for the current recipient.\n"
                             "on publishReport(recipientName)\n"
                             "tell application \"Mail\"\n"
                             "if recipientName is not \"\" then\n"
                             "repeat with attachmentName in {\"report.pdf\", \"chart.png\"}\n"
                             "set reportMessage to \"Ready for \" & recipientName & \": \" & attachmentName\n")
                            (let ((apples-end-completion-hl nil))
                              (dotimes (_ 4)
                                (apples-end-completion)
                                (insert "\n")))
                            (indent-region (point-min) (point-max))
                            (font-lock-ensure)
                            (save-buffer)
                            (let (faces)
                              (dolist (token
                                       '("publishReport" "tell" "repeat"
                                         "reportMessage" "is not"))
                                (goto-char (point-min))
                                (search-forward token)
                                (push
                                 (cons
                                  token
                                  (get-text-property
                                   (- (point) (length token))
                                   'face))
                                 faces))
                              (goto-char (point-max))
                              (let* ((index
                                      (funcall
                                       imenu-create-index-function))
                                     (handlers
                                      (cdr
                                       (assoc "Handlers" index)))
                                     (handler
                                      (assoc
                                       "publishReport(recipientName)"
                                       handlers)))
                                (imenu handler))
                              (setq result
                                    (let* ((edited
                                            (buffer-substring-no-properties
                                             (point-min) (point-max)))
                                           (saved
                                            (with-temp-buffer
                                              (insert-file-contents script-file)
                                              (buffer-string))))
                                      (list
                                       major-mode
                                       edited
                                       (equal edited saved)
                                       (list
                                        (line-number-at-pos)
                                        (buffer-substring-no-properties
                                         (line-beginning-position)
                                         (line-end-position)))
                                       (nreverse faces))))))
                        (kill-buffer (current-buffer))))
                    result))"##,
        expect![[
            r#"OK (apples-mode "property projectName : \"Release\"\n-- Build one message for the current recipient.\non publishReport(recipientName)\n    tell application \"Mail\"\n        if recipientName is not \"\" then\n            repeat with attachmentName in {\"report.pdf\", \"chart.png\"}\n                set reportMessage to \"Ready for \" & recipientName & \": \" & attachmentName\n            end repeat\n        end if\n    end tell\nend publishReport\n" t (3 "on publishReport(recipientName)") (("publishReport" . font-lock-function-name-face) ("tell" . apples-statements) ("repeat" . apples-statements) ("reportMessage" . font-lock-variable-name-face) ("is not" . apples-operators)))"#
        ]],
    )
}

fn runs_a_selected_report_delivery_and_displays_the_external_result() -> ParityBatchCase {
    ParityBatchCase::value(
        "runs_a_selected_report_delivery_and_displays_the_external_result",
        r##"(let* ((bin-dir
                          (expand-file-name
                           "fake-apples-runtime"
                           temporary-file-directory))
                         (osascript (expand-file-name "osascript" bin-dir))
                         (calls
                          (expand-file-name
                           "osascript-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         (apples-plist
                          (list :AS-version "2.1" :tmp-files nil)))
                    (make-directory bin-dir t)
                    (with-temp-file osascript
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "printf '%s\\n' \"$1\" \"$2\" \"$3\" "
                       "> \"${APPLES_SCRIPT_LOG:?}\"\n"
                       "case \"$3\" in\n"
                       "  *'repeat with recipientName in recipients'*"
                       "'return \"Delivered 2 reports\"'*)\n"
                       "    printf '%s\\n' '\"Delivered 2 reports\"' ;;\n"
                       "  *) printf '%s\\n' 'unexpected script' >&2; exit 64 ;;\n"
                       "esac\n"))
                    (set-file-modes osascript #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "APPLES_SCRIPT_LOG" calls)
                    (with-temp-buffer
                      (apples-mode)
                      (insert
                       "set recipients to {\"Ada\", \"Grace\"}\n"
                       "repeat with recipientName in recipients\n"
                       "display dialog \"Delivering to \" & recipientName\n"
                       "end repeat\n"
                       "return \"Delivered 2 reports\"\n")
                      (let ((original (buffer-string))
                            (menu-command
                             (key-binding
                              [menu-bar applescript Execution
                                        Run\ Region\ or\ Buffer])))
                        (goto-char (point-min))
                        (forward-line 1)
                        (push-mark (point-max) t t)
                        (setq mark-active t
                              transient-mark-mode t)
                        (call-interactively menu-command)
                        (let ((process
                               (get-process "apples-do-applescript")))
                          ;; Wait for the sentinel, not for the child to die.  apples-mode
                          ;; computes the value this case pins INSIDE the sentinel it installs
                          ;; (apples-mode.el:498-504), and `process-live-p' going nil is strictly
                          ;; earlier: reaping the child sets `raw_status_new' (src/process.c:7748)
                          ;; and drops the read fd (src/process.c:7760) in one pass, so the bytes
                          ;; still queued are recovered only by the drain in `status_notify'
                          ;; (src/process.c:7896-7911), which runs just before `exec_sentinel'
                          ;; (src/process.c:7937).
                          (unless process
                            (error "apples-mode fixture: the script process never started"))
                          (add-function :after (process-sentinel process)
                                        (lambda (proc &rest _)
                                          (process-put proc 'apples-test-sentinel-ran t)))
                          (let ((deadline (+ (float-time) 30)))
                            (while (and (not (process-get process 'apples-test-sentinel-ran))
                                        (< (float-time) deadline))
                              (accept-process-output nil 0.05)))
                          (unless (process-get process 'apples-test-sentinel-ran)
                            (error "apples-mode fixture: %s never ran its sentinel"
                                   (process-name process))))
                        (list
                         menu-command
                         (substring-no-properties
                          (apples-show-last-result))
                         (equal original (buffer-string))
                         (buffer-substring-no-properties
                          (region-beginning) (region-end))
                         (with-temp-buffer
                           (insert-file-contents calls)
                           (buffer-string))))))"##,
        expect![[
            r#"OK (apples-run-region/buffer "Result: Delivered 2 reports" t "repeat with recipientName in recipients\ndisplay dialog \"Delivering to \" & recipientName\nend repeat\nreturn \"Delivered 2 reports\"\n" "-ss\n-e\nrepeat with recipientName in recipients\ndisplay dialog \"Delivering to \" & recipientName\nend repeat\nreturn \"Delivered 2 reports\"\n\n")"#
        ]],
    )
}

fn failed_region_execution_highlights_the_broken_reference_and_moves_to_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "failed_region_execution_highlights_the_broken_reference_and_moves_to_it",
        r##"(let* ((bin-dir
                          (expand-file-name
                           "fake-apples-error-runtime"
                           temporary-file-directory))
                         (osascript (expand-file-name "osascript" bin-dir))
                         (calls
                          (expand-file-name
                           "osascript-error-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         (apples-plist
                          (list :AS-version "2.1" :tmp-files nil))
                         (apples-follow-error-position t))
                    (make-directory bin-dir t)
                    (with-temp-file osascript
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "printf '%s\\n' \"$1\" \"$2\" \"$3\" "
                       "> \"${APPLES_SCRIPT_LOG:?}\"\n"
                       "case \"$3\" in\n"
                       "  *missingValue*)\n"
                       "    printf '%s\\n' "
                       "'18:30: execution error: "
                       "The variable missingValue is not defined. (-2753)'\n"
                       "    exit 1 ;;\n"
                       "  *) printf '%s\\n' 'unexpected script' >&2; exit 64 ;;\n"
                       "esac\n"))
                    (set-file-modes osascript #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "APPLES_SCRIPT_LOG" calls)
                    (with-temp-buffer
                      (apples-mode)
                      (insert
                       "property reportName : \"Q2\"\n"
                       "set reportPath to missingValue\n"
                       "return reportPath\n")
                      (goto-char (point-min))
                      (forward-line 1)
                      (let ((beg (point))
                            (end (point-max)))
                        (apples-run-region beg end))
                      (let ((process
                             (get-process "apples-do-applescript")))
                        ;; Wait for the sentinel, not for the child to die.  apples-mode
                        ;; computes the value this case pins INSIDE the sentinel it installs
                        ;; (apples-mode.el:498-504), and `process-live-p' going nil is strictly
                        ;; earlier: reaping the child sets `raw_status_new' (src/process.c:7748)
                        ;; and drops the read fd (src/process.c:7760) in one pass, so the bytes
                        ;; still queued are recovered only by the drain in `status_notify'
                        ;; (src/process.c:7896-7911), which runs just before `exec_sentinel'
                        ;; (src/process.c:7937).
                        (unless process
                          (error "apples-mode fixture: the script process never started"))
                        (add-function :after (process-sentinel process)
                                      (lambda (proc &rest _)
                                        (process-put proc 'apples-test-sentinel-ran t)))
                        (let ((deadline (+ (float-time) 30)))
                          (while (and (not (process-get process 'apples-test-sentinel-ran))
                                      (< (float-time) deadline))
                            (accept-process-output nil 0.05)))
                        (unless (process-get process 'apples-test-sentinel-ran)
                          (error "apples-mode fixture: %s never ran its sentinel"
                                 (process-name process))))
                      (let* ((visible-error
                              (substring-no-properties
                               (apples-show-last-result)))
                             (highlight
                              (car
                               (seq-filter
                                (lambda (overlay)
                                  (eq (overlay-get overlay 'face)
                                      'apples-error-highlight))
                                (overlays-in (point-min) (point-max)))))
                             (initial
                              (list
                               visible-error
                               (line-number-at-pos)
                               (current-column)
                               (and highlight
                                    (buffer-substring-no-properties
                                     (overlay-start highlight)
                                     (overlay-end highlight))))))
                        (goto-char (point-min))
                        (search-forward "missingValue")
                        (replace-match "reportName" t t)
                        (list
                         initial
                         (buffer-string)
                         (not
                          (seq-some
                           (lambda (overlay)
                             (eq (overlay-get overlay 'face)
                                 'apples-error-highlight))
                           (overlays-in (point-min) (point-max))))
                         (with-temp-buffer
                           (insert-file-contents calls)
                           (buffer-string))))))"##,
        expect![[
            r#"OK (("execution error: The variable missingValue is not defined. [-2753]" 2 18 "missingValue") "property reportName : \"Q2\"\nset reportPath to reportName\nreturn reportPath\n" t "-ss\n-e\nset reportPath to missingValue\nreturn reportPath\n\n")"#
        ]],
    )
}

fn compiles_then_decompiles_a_script_through_the_documented_toolchain() -> ParityBatchCase {
    ParityBatchCase::value(
        "compiles_then_decompiles_a_script_through_the_documented_toolchain",
        r##"(let* ((root
                          (expand-file-name
                           "apples-toolchain"
                           temporary-file-directory))
                         (bin-dir (expand-file-name "bin" root))
                         (source (expand-file-name "report.applescript" root))
                         (compiled (expand-file-name "build/report.scpt" root))
                         (osacompile (expand-file-name "osacompile" bin-dir))
                         (osadecompile (expand-file-name "osadecompile" bin-dir))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         (apples-compile-create-file-flag t)
                         compiled-artifact)
                    (make-directory bin-dir t)
                    (with-temp-file source
                      (insert
                       "on reportTitle(projectName)\n"
                       "    return \"Status: \" & projectName\n"
                       "end reportTitle\n"))
                    (with-temp-file osacompile
                      (insert
                       "#!/bin/sh\n"
                       "{ printf '%s\\n' 'FAKE-COMPILED'; "
                       "cat \"$3\"; } > \"$2\"\n"))
                    (with-temp-file osadecompile
                      (insert "#!/bin/sh\nsed '1d' \"$1\"\n"))
                    (set-file-modes osacompile #o755)
                    (set-file-modes osadecompile #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (apples-compile source compiled)
                    (let ((process (get-process "apples-compile")))
                      ;; Wait for the sentinel, not for the child to die.  apples-mode
                      ;; computes the value this case pins INSIDE the sentinel it installs
                      ;; (apples-mode.el:498-504), and `process-live-p' going nil is strictly
                      ;; earlier: reaping the child sets `raw_status_new' (src/process.c:7748)
                      ;; and drops the read fd (src/process.c:7760) in one pass, so the bytes
                      ;; still queued are recovered only by the drain in `status_notify'
                      ;; (src/process.c:7896-7911), which runs just before `exec_sentinel'
                      ;; (src/process.c:7937).
                      (unless process
                        (error "apples-mode fixture: the script process never started"))
                      (add-function :after (process-sentinel process)
                                    (lambda (proc &rest _)
                                      (process-put proc 'apples-test-sentinel-ran t)))
                      (let ((deadline (+ (float-time) 30)))
                        (while (and (not (process-get process 'apples-test-sentinel-ran))
                                    (< (float-time) deadline))
                          (accept-process-output nil 0.05)))
                      (unless (process-get process 'apples-test-sentinel-ran)
                        (error "apples-mode fixture: %s never ran its sentinel"
                               (process-name process))))
                    (setq compiled-artifact
                          (with-temp-buffer
                            (insert-file-contents compiled)
                            (buffer-string)))
                    (let ((apples-decompile-callback
                           'apples-handle-decompile)
                          (apples-decompile-query ?o))
                      (apples-decompile compiled)
                      (let ((process (get-process "apples-decompile")))
                        ;; Wait for the sentinel, not for the child to die.  apples-mode
                        ;; computes the value this case pins INSIDE the sentinel it installs
                        ;; (apples-mode.el:498-504), and `process-live-p' going nil is strictly
                        ;; earlier: reaping the child sets `raw_status_new' (src/process.c:7748)
                        ;; and drops the read fd (src/process.c:7760) in one pass, so the bytes
                        ;; still queued are recovered only by the drain in `status_notify'
                        ;; (src/process.c:7896-7911), which runs just before `exec_sentinel'
                        ;; (src/process.c:7937).
                        (unless process
                          (error "apples-mode fixture: the script process never started"))
                        (add-function :after (process-sentinel process)
                                      (lambda (proc &rest _)
                                        (process-put proc 'apples-test-sentinel-ran t)))
                        (let ((deadline (+ (float-time) 30)))
                          (while (and (not (process-get process 'apples-test-sentinel-ran))
                                      (< (float-time) deadline))
                            (accept-process-output nil 0.05)))
                        (unless (process-get process 'apples-test-sentinel-ran)
                          (error "apples-mode fixture: %s never ran its sentinel"
                                 (process-name process)))))
                    (list
                     (file-exists-p compiled)
                     compiled-artifact
                     (with-temp-buffer
                       (insert-file-contents compiled)
                       (buffer-string))))"##,
        expect![[
            r#"OK (t "FAKE-COMPILED\non reportTitle(projectName)\n    return \"Status: \" & projectName\nend reportTitle\n" "on reportTitle(projectName)\n    return \"Status: \" & projectName\nend reportTitle")"#
        ]],
    )
}

fn expands_and_edits_the_installed_tell_application_snippet_in_a_real_script() -> ParityBatchCase {
    ParityBatchCase::value(
        "expands_and_edits_the_installed_tell_application_snippet_in_a_real_script",
        r##"(progn
                  (require 'yasnippet)
                  (let* ((package-root
                          (file-name-directory
                           (getenv "NEOMACS_PACKAGE_SOURCE")))
                         (yas-snippet-dirs
                          (list package-root))
                         (apples-plist
                          (list :AS-version "2.1" :tmp-files nil)))
                    (yas-reload-all)
                    (with-temp-buffer
                      (apples-mode)
                      (setq-local indent-tabs-mode nil)
                      (yas-minor-mode 1)
                      (insert "tell-application")
                      (let ((expanded (yas-expand)))
                        (insert "Mail")
                        (yas-next-field-or-maybe-expand)
                        (insert
                         "display dialog \"Release ready\" "
                         "default answer \"Ship\"")
                        (yas-next-field-or-maybe-expand)
                        (yas-exit-all-snippets)
                        (indent-region (point-min) (point-max))
                        (list
                         expanded
                         (buffer-substring-no-properties
                          (point-min) (point-max))
                         (null (yas-active-snippets)))))))"##,
        expect![[
            r#"OK (t "tell application \"Mail\"\n    display dialog \"Release ready\" default answer \"Ship\"\nend tell" t)"#
        ]],
    )
}

fn scratch_buffer_persists_an_edit_when_killed_and_restores_it_when_reopened() -> ParityBatchCase {
    ParityBatchCase::value(
        "scratch_buffer_persists_an_edit_when_killed_and_restores_it_when_reopened",
        r##"(let* ((apples-tmp-dir
                          (expand-file-name
                           "apples-scratch-workflow"
                           temporary-file-directory))
                         (apples-plist
                          (list :AS-version "2.1"
                                :tmp-files '(apples-tmp-scratch)))
                         restored persisted reopened)
                    (when (get-buffer "*apples-scratch*")
                      (kill-buffer "*apples-scratch*"))
                    (apples-tmp-files-setup)
                    (with-temp-file apples-tmp-scratch
                      (insert "set reportCount to 1\n"))
                    (save-window-excursion
                      (apples-open-scratch)
                      (setq restored (buffer-string))
                      (goto-char (point-max))
                      (insert
                       "set reportCount to reportCount + 1\n"
                       "return reportCount\n")
                      (kill-buffer (current-buffer))
                      (setq persisted
                            (with-temp-buffer
                              (insert-file-contents apples-tmp-scratch)
                              (buffer-string)))
                      (apples-open-scratch)
                      (setq reopened (buffer-string))
                      (kill-buffer (current-buffer)))
                    (list restored persisted reopened))"##,
        expect![[
            r#"OK ("set reportCount to 1\n" "set reportCount to 1\nset reportCount to reportCount + 1\nreturn reportCount\n" "set reportCount to 1\nset reportCount to reportCount + 1\nreturn reportCount\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        author_completes_indents_navigates_and_saves_a_mail_automation_script(),
        runs_a_selected_report_delivery_and_displays_the_external_result(),
        failed_region_execution_highlights_the_broken_reference_and_moves_to_it(),
        compiles_then_decompiles_a_script_through_the_documented_toolchain(),
        expands_and_edits_the_installed_tell_application_snippet_in_a_real_script(),
        scratch_buffer_persists_an_edit_when_killed_and_restores_it_when_reopened(),
    ]
}
