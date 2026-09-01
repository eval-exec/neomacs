use expect_test::expect;

use super::ParityBatchCase;

fn upload_builds_exact_process_request_and_success_sentinel_cleans_source_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "upload_builds_exact_process_request_and_success_sentinel_cleans_source_state",
        r##"(let ((source
                         (get-buffer-create
                          "*arduino-upload-source-contract*"))
                        process-args sentinel events)
                    (unwind-protect
                        (with-current-buffer source
                          (setq buffer-file-name
                                "/workspace/Blink/Blink.ino")
                          (let ((arduino-executable
                                 "arduino-custom")
                                (arduino-spinner-type
                                 'triangle)
                                (spinner-current
                                 'fake-spinner))
                            (cl-letf
                                (((symbol-function 'make-process)
                                  (lambda (&rest args)
                                    (setq process-args args
                                          sentinel
                                          (plist-get
                                           args :sentinel))
                                    'fake-process))
                                 ((symbol-function 'spinner-start)
                                  (lambda (kind)
                                    (push
                                     (list :start kind)
                                     events)))
                                 ((symbol-function 'spinner-stop)
                                  (lambda ()
                                    (push :stop events)))
                                 ((symbol-function 'message)
                                  (lambda (format-string &rest args)
                                    (push
                                     (list
                                      :message
                                      (apply
                                       #'format
                                       format-string args))
                                     events)))
                                 ((symbol-function 'display-buffer)
                                  (lambda (buffer &rest _args)
                                    (push
                                     (list :display buffer)
                                     events))))
                              (arduino-upload)
                              (let ((before mode-line-process))
                                (funcall
                                 sentinel
                                 'fake-process
                                 "finished\n")
                                (list
                                 process-args
                                 arduino-upload-process-buf
                                 before mode-line-process
                                 (nreverse events))))))
                      (kill-buffer source)))"##,
        expect![[
            r#"OK ((:command ("arduino-custom" "--upload" "/workspace/Blink/Blink.ino") :name "arduino-upload" :buffer "*arduino-upload*" :sentinel #[(_proc event) ((if (string= event "finished\n") (progn (save-current-buffer (set-buffer arduino-upload-process-buf) (setq mode-line-process nil)) (message "Arduino upload succeed.")) (save-current-buffer (set-buffer arduino-upload-process-buf) (display-buffer "*arduino-upload*"))) (setq mode-line-process (prog1 nil (make-local-variable 'mode-line-process))) (save-current-buffer (set-buffer arduino-upload-process-buf) (if spinner-current (progn (spinner-stop))))) (t)]) "*arduino-upload-source-contract*" "arduino-upload" nil ((:start triangle) (:message "Arduino upload succeed.") :stop))"#
        ]],
    )
}

fn upload_failure_sentinel_displays_diagnostics_and_stops_active_spinner() -> ParityBatchCase {
    ParityBatchCase::value(
        "upload_failure_sentinel_displays_diagnostics_and_stops_active_spinner",
        r##"(let ((source
                         (get-buffer-create
                          "*arduino-upload-failure-contract*"))
                        sentinel events)
                    (unwind-protect
                        (with-current-buffer source
                          (setq buffer-file-name
                                "/workspace/Blink/Blink.ino")
                          (let ((spinner-current 'fake-spinner))
                            (cl-letf
                                (((symbol-function 'make-process)
                                  (lambda (&rest args)
                                    (setq sentinel
                                          (plist-get
                                           args :sentinel))
                                    'fake-process))
                                 ((symbol-function 'spinner-start)
                                  (lambda (_kind) nil))
                                 ((symbol-function 'spinner-stop)
                                  (lambda ()
                                    (push :stop events)))
                                 ((symbol-function 'display-buffer)
                                  (lambda (buffer &rest _args)
                                    (push
                                     (list :display buffer)
                                     events)
                                    :shown))
                                 ((symbol-function 'message)
                                  (lambda (&rest args)
                                    (push
                                     (cons :message args)
                                     events))))
                              (arduino-upload)
                              (funcall
                               sentinel
                               'fake-process
                               "exited abnormally with code 1\n")
                              (list
                               mode-line-process
                               (nreverse events)))))
                      (kill-buffer source)))"##,
        expect![[r#"OK (nil ((:display "*arduino-upload*") :stop))"#]],
    )
}

fn verify_builds_exact_request_and_both_sentinel_paths_have_expected_lifecycle() -> ParityBatchCase
{
    ParityBatchCase::value(
        "verify_builds_exact_request_and_both_sentinel_paths_have_expected_lifecycle",
        r##"(let ((source
                         (get-buffer-create
                          "*arduino-verify-source-contract*"))
                        process-args sentinel events)
                    (unwind-protect
                        (with-current-buffer source
                          (setq buffer-file-name
                                "/workspace/Sensor/Sensor.ino")
                          (let ((arduino-executable "arduino-cli")
                                (arduino-spinner-type 'moon)
                                (spinner-current 'fake-spinner))
                            (cl-letf
                                (((symbol-function 'make-process)
                                  (lambda (&rest args)
                                    (setq process-args args
                                          sentinel
                                          (plist-get
                                           args :sentinel))
                                    'fake-process))
                                 ((symbol-function 'spinner-start)
                                  (lambda (kind)
                                    (push
                                     (list :start kind)
                                     events)))
                                 ((symbol-function 'spinner-stop)
                                  (lambda ()
                                    (push :stop events)))
                                 ((symbol-function 'message)
                                  (lambda (format-string &rest args)
                                    (push
                                     (list
                                      :message
                                      (apply
                                       #'format
                                       format-string args))
                                     events)))
                                 ((symbol-function 'display-buffer)
                                  (lambda (buffer &rest _args)
                                    (push
                                     (list :display buffer)
                                     events))))
                              (arduino-verify)
                              (let ((before mode-line-process))
                                (funcall
                                 sentinel 'fake-process "finished\n")
                                (setq mode-line-process
                                      "arduino-verify")
                                (funcall
                                 sentinel 'fake-process "failed\n")
                                (list
                                 process-args
                                 arduino-verify-process-buf
                                 before mode-line-process
                                 (nreverse events))))))
                      (kill-buffer source)))"##,
        expect![[
            r#"OK ((:command ("arduino-cli" "--verify" "/workspace/Sensor/Sensor.ino") :name "arduino-verify" :buffer "*arduino-verify*" :sentinel #[(_proc event) ((if (string= event "finished\n") (progn (save-current-buffer (set-buffer arduino-verify-process-buf) (setq mode-line-process nil)) (message "Arduino verify build succeed.")) (display-buffer "*arduino-verify*")) (setq mode-line-process (prog1 nil (make-local-variable 'mode-line-process))) (save-current-buffer (set-buffer arduino-verify-process-buf) (if spinner-current (progn (spinner-stop))))) (t)]) "*arduino-verify-source-contract*" "arduino-verify" nil ((:start moon) (:message "Arduino verify build succeed.") :stop (:display "*arduino-verify*") :stop))"#
        ]],
    )
}

fn open_with_ide_builds_exact_request_and_success_sentinel_reports_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "open_with_ide_builds_exact_request_and_success_sentinel_reports_completion",
        r##"(let ((source
                         (get-buffer-create
                          "*arduino-open-source-contract*"))
                        process-args sentinel events)
                    (unwind-protect
                        (with-current-buffer source
                          (setq buffer-file-name
                                "/workspace/Robot/Robot.ino")
                          (let ((arduino-executable
                                 "/opt/arduino/arduino")
                                (arduino-spinner-type 'rotating-line)
                                (spinner-current 'fake-spinner))
                            (cl-letf
                                (((symbol-function 'make-process)
                                  (lambda (&rest args)
                                    (setq process-args args
                                          sentinel
                                          (plist-get
                                           args :sentinel))
                                    'fake-process))
                                 ((symbol-function 'spinner-start)
                                  (lambda (kind)
                                    (push
                                     (list :start kind)
                                     events)))
                                 ((symbol-function 'spinner-stop)
                                  (lambda ()
                                    (push :stop events)))
                                 ((symbol-function 'message)
                                  (lambda (format-string &rest args)
                                    (push
                                     (list
                                      :message
                                      (apply
                                       #'format
                                       format-string args))
                                     events))))
                              (arduino-open-with-arduino)
                              (let ((before mode-line-process))
                                (funcall
                                 sentinel 'fake-process "finished\n")
                                (list
                                 process-args
                                 arduino-open-process-buf
                                 before mode-line-process
                                 (nreverse events))))))
                      (kill-buffer source)))"##,
        expect![[
            r#"OK ((:command ("/opt/arduino/arduino" "/workspace/Robot/Robot.ino") :name "arduino-open" :buffer "*arduino-open*" :sentinel #[(_proc event) ((if (string= event "finished\n") (progn (save-current-buffer (set-buffer arduino-open-process-buf) (setq mode-line-process nil)) (message "Opened with Arduino succeed."))) (setq mode-line-process (prog1 nil (make-local-variable 'mode-line-process))) (save-current-buffer (set-buffer arduino-open-process-buf) (if spinner-current (progn (spinner-stop))))) (t)]) "*arduino-open-source-contract*" "arduino-open" nil ((:start rotating-line) (:message "Opened with Arduino succeed.") :stop))"#
        ]],
    )
}

fn board_and_library_installers_dispatch_exact_start_process_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "board_and_library_installers_dispatch_exact_start_process_arguments",
        r##"(let ((arduino-executable
                         "/opt/arduino/bin/arduino")
                        calls)
                    (cl-letf
                        (((symbol-function 'start-process)
                          (lambda (&rest args)
                            (push args calls)
                            (intern
                             (format
                              "process-%d"
                              (length calls))))))
                      (list
                       (arduino-install-boards
                        "arduino:samd")
                       (arduino-install-library
                        "Servo:1.2.1")
                       (nreverse calls))))"##,
        expect![[
            r#"OK (process-1 process-2 (("arduino-install-boards" "*arduino-install-boards*" "/opt/arduino/bin/arduino" "--install-boards" "arduino:samd") ("arduino-install-library" "*arduino-install-library*" "/opt/arduino/bin/arduino" "--install-library" "Servo:1.2.1")))"#
        ]],
    )
}

fn interactive_installers_preserve_prompts_defaults_and_user_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "interactive_installers_preserve_prompts_defaults_and_user_values",
        r##"(let (reads calls)
                    (cl-letf
                        (((symbol-function 'read-string)
                          (lambda (prompt initial &rest _args)
                            (push
                             (list prompt initial)
                             reads)
                            (if
                                (string-match-p
                                 "board" prompt)
                                "vendor:board"
                              "Wire:2.0")))
                         ((symbol-function 'start-process)
                          (lambda (&rest args)
                            (push args calls)
                            'fake-process)))
                      (call-interactively
                       #'arduino-install-boards)
                      (call-interactively
                       #'arduino-install-library)
                      (list
                       (nreverse reads)
                       (nreverse calls))))"##,
        expect![[
            r#"OK ((("Arduino install board: " "arduino:sam") ("Arduino install library: " "Bridge:1.0.0")) (("arduino-install-boards" "*arduino-install-boards*" "arduino" "--install-boards" "vendor:board") ("arduino-install-library" "*arduino-install-library*" "arduino" "--install-library" "Wire:2.0")))"#
        ]],
    )
}

fn serial_monitor_switches_to_live_port_or_opens_serial_with_explicit_and_prompted_speed()
-> ParityBatchCase {
    ParityBatchCase::value(
        "serial_monitor_switches_to_live_port_or_opens_serial_with_explicit_and_prompted_speed",
        r##"(let (events)
                    (cl-letf
                        (((symbol-function 'get-buffer-process)
                          (lambda (port)
                            (if
                                (equal port "/dev/ttyLIVE")
                                'live-process
                              nil)))
                         ((symbol-function 'switch-to-buffer)
                          (lambda (buffer &rest _args)
                            (push
                             (list :switch buffer)
                             events)
                            :switched))
                         ((symbol-function 'serial-term)
                          (lambda (port speed)
                            (push
                             (list :serial port speed)
                             events)
                            :serial-opened))
                         ((symbol-function 'serial-read-speed)
                          (lambda ()
                            (push :read-speed events)
                            115200)))
                      (list
                       (arduino-serial-monitor
                        "/dev/ttyLIVE" nil)
                       (arduino-serial-monitor
                        "/dev/ttyUSB0" 57600)
                       (arduino-serial-monitor
                        "/dev/ttyACM0" nil)
                       (nreverse events))))"##,
        expect![[
            r#"OK (:switched :serial-opened :serial-opened ((:switch "/dev/ttyLIVE") (:serial "/dev/ttyUSB0" 57600) :read-speed (:serial "/dev/ttyACM0" 115200)))"#
        ]],
    )
}

pub(super) fn processes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        upload_builds_exact_process_request_and_success_sentinel_cleans_source_state(),
        upload_failure_sentinel_displays_diagnostics_and_stops_active_spinner(),
        verify_builds_exact_request_and_both_sentinel_paths_have_expected_lifecycle(),
        open_with_ide_builds_exact_request_and_success_sentinel_reports_completion(),
        board_and_library_installers_dispatch_exact_start_process_arguments(),
        interactive_installers_preserve_prompts_defaults_and_user_values(),
        serial_monitor_switches_to_live_port_or_opens_serial_with_explicit_and_prompted_speed(),
    ]
}
