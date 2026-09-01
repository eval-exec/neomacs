use expect_test::expect;

use super::ParityBatchCase;

fn upload_key_binding_runs_the_real_cli_and_preserves_the_unsaved_sketch() -> ParityBatchCase {
    ParityBatchCase::value(
        "upload_key_binding_runs_the_real_cli_and_preserves_the_unsaved_sketch",
        r##"(let* ((fixture
                          (neomacs-arduino-mode-test-fixture))
                         (sketch (plist-get fixture :sketch))
                         (executable
                          (plist-get fixture :executable))
                         (call-log
                          (plist-get fixture :call-log))
                         (gate (plist-get fixture :gate))
                         source-buffer
                         process
                         messages)
                    (unwind-protect
                        (save-window-excursion
                          (setq source-buffer
                                (find-file-noselect sketch))
                          (switch-to-buffer source-buffer)
                          (with-current-buffer source-buffer
                            (goto-char (point-max))
                            (insert
                             "// Unsaved calibration for sensor A1.\n")
                            (let* ((arduino-executable executable)
                                   (arduino-spinner-type 'triangle)
                                   (command
                                    (key-binding (kbd "C-c C-c")))
                                   (original-message
                                    (symbol-function 'message)))
                              (cl-letf
                                  (((symbol-function 'message)
                                    (lambda
                                        (format-string &rest arguments)
                                      (let ((rendered
                                             (and
                                              format-string
                                              (apply
                                               #'format
                                               format-string arguments))))
                                        (when
                                            (and
                                             rendered
                                             (> (length rendered) 0))
                                          (push rendered messages)))
                                      (apply
                                       original-message
                                       format-string arguments))))
                                (execute-kbd-macro
                                 (kbd "C-c C-c"))
                                (setq process
                                      (get-process
                                       "arduino-upload"))
                                (let ((started
                                       (list
                                        mode-line-process
                                        (neomacs-arduino-mode-test-spinner-active-p))))
                                  (neomacs-arduino-mode-test-observe
                                   process)
                                  (neomacs-arduino-mode-test-write-file
                                   gate "")
                                  (let ((wait-result
                                         (neomacs-arduino-mode-test-wait
                                          process)))
                                    (list
                                     :mode major-mode
                                     :command command
                                     :started started
                                     :process
                                     (list
                                      (process-name process)
                                      (buffer-name
                                       (process-buffer process))
                                      wait-result)
                                     :invocation
                                     (neomacs-arduino-mode-test-read-file
                                      call-log)
                                     :output
                                     (with-current-buffer
                                         "*arduino-upload*"
                                       (buffer-string))
                                     :disk
                                     (neomacs-arduino-mode-test-read-file
                                      sketch)
                                     :buffer
                                     (buffer-substring-no-properties
                                      (point-min) (point-max))
                                     :modified
                                     (buffer-modified-p)
                                     :finished
                                     (list
                                      mode-line-process
                                      (neomacs-arduino-mode-test-spinner-active-p)
                                      (nreverse messages)))))))))
                      (when
                          (and
                           process
                           (process-live-p process))
                        (delete-process process))
                      (when
                          (buffer-live-p source-buffer)
                        (with-current-buffer source-buffer
                          (set-buffer-modified-p nil))
                        (kill-buffer source-buffer))
                      (dolist
                          (buffer-name
                           '("*arduino-upload*"
                             "*arduino-verify*"))
                        (when-let ((buffer
                                    (get-buffer buffer-name)))
                          (kill-buffer buffer)))
                      (setenv
                       "NEOMACS_ARDUINO_MODE_CALL_LOG"
                       (plist-get fixture :previous-call-log))
                      (setenv
                       "NEOMACS_ARDUINO_MODE_GATE"
                       (plist-get fixture :previous-gate))))"##,
        expect![[
            r#"OK (:mode arduino-mode :command arduino-upload :started ("arduino-upload" t) :process ("arduino-upload" "*arduino-upload*" (t exit 0 "finished\n")) :invocation "cwd=[ORACLE-SANDBOX]/customer firmware/greenhouse monitor\narg=--upload\narg=[ORACLE-SANDBOX]/customer firmware/greenhouse monitor/greenhouse_monitor.ino\n" :output "Sketch uses 924 bytes (2%) of program storage space.\nUploaded [ORACLE-SANDBOX]/customer firmware/greenhouse monitor/greenhouse_monitor.ino\nCLI read: const int sensorPin = A0;\n" :disk "const int sensorPin = A0;\nvoid setup() { Serial.begin(115200); }\nvoid loop() { Serial.println(analogRead(sensorPin)); }\n" :buffer "const int sensorPin = A0;\nvoid setup() { Serial.begin(115200); }\nvoid loop() { Serial.println(analogRead(sensorPin)); }\n// Unsaved calibration for sensor A1.\n" :modified t :finished (nil nil ("Arduino upload succeed.")))"#
        ]],
    )
}

fn verify_key_binding_surfaces_real_cli_diagnostics_and_cleans_async_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "verify_key_binding_surfaces_real_cli_diagnostics_and_cleans_async_state",
        r##"(let* ((fixture
                          (neomacs-arduino-mode-test-fixture))
                         (sketch (plist-get fixture :sketch))
                         (executable
                          (plist-get fixture :executable))
                         (call-log
                          (plist-get fixture :call-log))
                         (gate (plist-get fixture :gate))
                         source-buffer
                         process
                         messages)
                    (unwind-protect
                        (save-window-excursion
                          (setq source-buffer
                                (find-file-noselect sketch))
                          (switch-to-buffer source-buffer)
                          (let* ((arduino-executable executable)
                                 (arduino-spinner-type 'moon)
                                 (command
                                  (key-binding (kbd "C-c C-v")))
                                 (original-message
                                  (symbol-function 'message)))
                            (cl-letf
                                (((symbol-function 'message)
                                  (lambda
                                      (format-string &rest arguments)
                                    (let ((rendered
                                           (and
                                            format-string
                                            (apply
                                             #'format
                                             format-string arguments))))
                                      (when
                                          (and
                                           rendered
                                           (> (length rendered) 0))
                                        (push rendered messages)))
                                    (apply
                                     original-message
                                     format-string arguments))))
                              (execute-kbd-macro
                               (kbd "C-c C-v"))
                              (setq process
                                    (get-process "arduino-verify"))
                              (let ((started
                                     (list
                                      mode-line-process
                                      (neomacs-arduino-mode-test-spinner-active-p))))
                                (neomacs-arduino-mode-test-observe
                                 process)
                                (neomacs-arduino-mode-test-write-file
                                 gate "")
                                (let ((wait-result
                                       (neomacs-arduino-mode-test-wait
                                        process)))
                                  (list
                                   :mode major-mode
                                   :command command
                                   :started started
                                   :process
                                   (list
                                    (process-name process)
                                    (buffer-name
                                     (process-buffer process))
                                    wait-result)
                                   :invocation
                                   (neomacs-arduino-mode-test-read-file
                                    call-log)
                                   :diagnostics
                                   (with-current-buffer
                                       "*arduino-verify*"
                                     (buffer-string))
                                   :diagnostics-visible
                                   (window-live-p
                                    (get-buffer-window
                                     "*arduino-verify*" t))
                                   :source-state
                                   (list
                                    mode-line-process
                                    (neomacs-arduino-mode-test-spinner-active-p)
                                    (buffer-modified-p))
                                   :messages
                                   (nreverse messages)))))))
                      (when
                          (and
                           process
                           (process-live-p process))
                        (delete-process process))
                      (when
                          (buffer-live-p source-buffer)
                        (with-current-buffer source-buffer
                          (set-buffer-modified-p nil))
                        (kill-buffer source-buffer))
                      (dolist
                          (buffer-name
                           '("*arduino-upload*"
                             "*arduino-verify*"))
                        (when-let ((buffer
                                    (get-buffer buffer-name)))
                          (kill-buffer buffer)))
                      (setenv
                       "NEOMACS_ARDUINO_MODE_CALL_LOG"
                       (plist-get fixture :previous-call-log))
                      (setenv
                       "NEOMACS_ARDUINO_MODE_GATE"
                       (plist-get fixture :previous-gate))))"##,
        expect![[
            r#"OK (:mode arduino-mode :command arduino-verify :started ("arduino-verify" t) :process ("arduino-verify" "*arduino-verify*" (t exit 17 "exited abnormally with code 17\n")) :invocation "cwd=[ORACLE-SANDBOX]/customer firmware/greenhouse monitor\narg=--verify\narg=[ORACLE-SANDBOX]/customer firmware/greenhouse monitor/greenhouse_monitor.ino\n" :diagnostics "Verifying [ORACLE-SANDBOX]/customer firmware/greenhouse monitor/greenhouse_monitor.ino\n[ORACLE-SANDBOX]/customer firmware/greenhouse monitor/greenhouse_monitor.ino:7:3: error: sensorPin was not declared in this scope\n" :diagnostics-visible t :source-state (nil nil nil) :messages nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        upload_key_binding_runs_the_real_cli_and_preserves_the_unsaved_sketch(),
        verify_key_binding_surfaces_real_cli_diagnostics_and_cleans_async_state(),
    ]
}
