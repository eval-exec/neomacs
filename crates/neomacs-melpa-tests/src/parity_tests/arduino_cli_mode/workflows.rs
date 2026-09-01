use expect_test::expect;

use super::ParityBatchCase;

fn compiles_a_saved_weather_station_sketch_from_the_live_minor_mode_keymap() -> ParityBatchCase {
    ParityBatchCase::value(
        "compiles_a_saved_weather_station_sketch_from_the_live_minor_mode_keymap",
        r##"(let* ((project
                          (expand-file-name
                           "weather station/"
                           temporary-file-directory))
                         (source
                          (expand-file-name
                           "weather_station.ino"
                           project))
                         (bin-dir
                          (expand-file-name
                           "fake-arduino-bin/"
                           temporary-file-directory))
                         (arduino-cli
                          (expand-file-name "arduino-cli" bin-dir))
                         (calls
                          (expand-file-name
                           "arduino-cli-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         (compilation-ask-about-save nil))
                    (make-directory project t)
                    (make-directory bin-dir t)
                    (with-temp-file source
                      (insert
                       "int samples = 0;\n"
                       "void setup() { Serial.begin(115200); }\n"))
                    (with-temp-file arduino-cli
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "{ printf 'CALL'; "
                       "for arg in \"$@\"; do printf ' <%s>' \"$arg\"; done; "
                       "printf ' CWD=<%s>\\n' \"$PWD\"; } "
                       ">> \"${ARDUINO_CLI_LOG:?}\"\n"
                       "if [ \"$1\" = board ] && [ \"$2\" = list ] "
                       "&& [ \"$3\" = --format ] && [ \"$4\" = json ]; then\n"
                       "  printf '%s\\n' "
                       "'{\"detected_ports\":[{\"port\":{\"address\":"
                       "\"/dev/ttyACM0\"},\"matching_boards\":[{\"name\":"
                       "\"Arduino Uno\",\"fqbn\":\"arduino:avr:uno\"}],"
                       "\"boards\":[{\"name\":\"Arduino Uno\","
                       "\"fqbn\":\"arduino:avr:uno\"}]}]}'\n"
                       "elif [ \"$1\" = compile ]; then\n"
                       "  printf '%s\\n' "
                       "'Compiled weather_station.ino: 928 bytes, 3% flash.'\n"
                       "else\n"
                       "  printf 'unexpected arduino-cli call\\n' >&2\n"
                       "  exit 64\n"
                       "fi\n"))
                    (set-file-modes arduino-cli #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "ARDUINO_CLI_LOG" calls)
                    (with-current-buffer (find-file-noselect source)
                      (unwind-protect
                          (progn
                            (goto-char (point-max))
                            (insert
                             "void loop() {\n"
                             "  samples++;\n"
                             "  Serial.println(samples);\n"
                             "}\n")
                            (arduino-cli-mode 1)
                            (setq-local
                             arduino-cli-verify t
                             arduino-cli-warnings 'all
                             arduino-cli-verbosity 'verbose
                             arduino-cli-compile-color nil)
                            (let ((command
                                   (key-binding
                                    (kbd "C-c C-a c"))))
                              (call-interactively command)
                              (acli-test-await-compilation
                                     arduino-cli--compilation-buffer)
                              (list
                               command
                               (with-temp-buffer
                                 (insert-file-contents source)
                                 (buffer-string))
                               (with-current-buffer
                                   arduino-cli--compilation-buffer
                                 (list
                                  major-mode
                                  (and
                                   (string-match
                                    "Compiled weather_station\\.ino:[^\n]+"
                                    (buffer-string))
                                   (match-string
                                    0
                                    (buffer-string)))))
                               (with-temp-buffer
                                 (insert-file-contents calls)
                                 (replace-regexp-in-string
                                  (regexp-quote
                                   temporary-file-directory)
                                  "<TMP>/"
                                  (buffer-string)
                                  t t)))))
                        (kill-buffer (current-buffer)))))"##,
        expect![[
            r#"OK (arduino-cli-compile "int samples = 0;\nvoid setup() { Serial.begin(115200); }\nvoid loop() {\n  samples++;\n  Serial.println(samples);\n}\n" (arduino-cli-compilation-mode "Compiled weather_station.ino: 928 bytes, 3% flash.") "CALL <board> <list> <--format> <json> CWD=<<TMP>/weather station>\nCALL <compile> <--fqbn> <arduino:avr:uno> <<TMP>/weather station/> <-t> <--warnings> <all> <--verbose> <--no-color> CWD=<<TMP>/weather station>\n")"#
        ]],
    )
}

fn uploads_a_file_local_default_board_through_the_live_mode_menu() -> ParityBatchCase {
    ParityBatchCase::value(
        "uploads_a_file_local_default_board_through_the_live_mode_menu",
        r##"(let* ((project
                          (expand-file-name
                           "greenhouse-controller/"
                           temporary-file-directory))
                         (source
                          (expand-file-name
                           "greenhouse_controller.ino"
                           project))
                         (bin-dir
                          (expand-file-name
                           "fake-arduino-upload-bin/"
                           temporary-file-directory))
                         (arduino-cli
                          (expand-file-name "arduino-cli" bin-dir))
                         (calls
                          (expand-file-name
                           "arduino-upload-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         (compilation-ask-about-save nil)
                         (safe-local-variable-values
                          (append
                           '((arduino-cli-default-fqbn
                              . "esp32:esp32:esp32c3")
                             (arduino-cli-default-port
                              . "/dev/ttyUSB9"))
                           safe-local-variable-values)))
                    (make-directory project t)
                    (make-directory bin-dir t)
                    (with-temp-file source
                      (insert
                       "// -*- arduino-cli-default-fqbn: "
                       "\"esp32:esp32:esp32c3\"; "
                       "arduino-cli-default-port: \"/dev/ttyUSB9\"; -*-\n"
                       "void setup() { pinMode(LED_BUILTIN, OUTPUT); }\n"
                       "void loop() { digitalWrite(LED_BUILTIN, HIGH); }\n"))
                    (with-temp-file arduino-cli
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "{ printf 'CALL'; "
                       "for arg in \"$@\"; do printf ' <%s>' \"$arg\"; done; "
                       "printf ' CWD=<%s>\\n' \"$PWD\"; } "
                       ">> \"${ARDUINO_CLI_LOG:?}\"\n"
                       "if [ \"$1\" = board ] && [ \"$2\" = list ]; then\n"
                       "  printf '%s\\n' '{\"detected_ports\":[]}'\n"
                       "elif [ \"$1\" = upload ]; then\n"
                       "  printf '%s\\n' "
                       "'Uploaded greenhouse_controller.ino to /dev/ttyUSB9.'\n"
                       "else\n"
                       "  printf 'unexpected arduino-cli call\\n' >&2\n"
                       "  exit 64\n"
                       "fi\n"))
                    (set-file-modes arduino-cli #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "ARDUINO_CLI_LOG" calls)
                    (with-current-buffer (find-file-noselect source)
                      (unwind-protect
                          (progn
                            (arduino-cli-mode 1)
                            (setq-local
                             arduino-cli-verbosity 'quiet
                             arduino-cli-compile-only-verbosity nil)
                            (goto-char (point-max))
                            (insert
                             "// Calibrated for the east greenhouse.\n")
                            (let ((menu-command
                                   (key-binding
                                    [menu-bar arduino-cli
                                              Upload\ Project])))
                              (call-interactively menu-command)
                              (acli-test-await-compilation
                                     arduino-cli--compilation-buffer)
                              (list
                               menu-command
                               (list
                                arduino-cli-default-fqbn
                                arduino-cli-default-port)
                               (with-temp-buffer
                                 (insert-file-contents source)
                                 (buffer-string))
                               (with-current-buffer
                                   arduino-cli--compilation-buffer
                                 (and
                                  (string-match
                                   "Uploaded greenhouse_controller\\.ino[^\n]+"
                                   (buffer-string))
                                  (match-string 0 (buffer-string))))
                               (with-temp-buffer
                                 (insert-file-contents calls)
                                 (replace-regexp-in-string
                                  (regexp-quote
                                   temporary-file-directory)
                                  "<TMP>/"
                                  (buffer-string)
                                  t t)))))
                        (kill-buffer (current-buffer)))))"##,
        expect![[
            r#"OK (arduino-cli-upload ("esp32:esp32:esp32c3" "/dev/ttyUSB9") "// -*- arduino-cli-default-fqbn: \"esp32:esp32:esp32c3\"; arduino-cli-default-port: \"/dev/ttyUSB9\"; -*-\nvoid setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); }\n// Calibrated for the east greenhouse.\n" "Uploaded greenhouse_controller.ino to /dev/ttyUSB9." "CALL <board> <list> <--format> <json> CWD=<<TMP>/greenhouse-controller>\nCALL <upload> <--fqbn> <esp32:esp32:esp32c3> <--port> </dev/ttyUSB9> <<TMP>/greenhouse-controller/> <--quiet> CWD=<<TMP>/greenhouse-controller>\n")"#
        ]],
    )
}

fn selects_the_nonfirst_connected_board_then_uploads_through_real_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "selects_the_nonfirst_connected_board_then_uploads_through_real_completion",
        r##"(let* ((project
                          (expand-file-name
                           "portable-air-quality/"
                           temporary-file-directory))
                         (source
                          (expand-file-name
                           "portable_air_quality.ino"
                           project))
                         (bin-dir
                          (expand-file-name
                           "fake-arduino-multiboard-bin/"
                           temporary-file-directory))
                         (arduino-cli
                          (expand-file-name "arduino-cli" bin-dir))
                         (calls
                          (expand-file-name
                           "arduino-multiboard-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         (compilation-ask-about-save nil)
                         prompts)
                    (make-directory project t)
                    (make-directory bin-dir t)
                    (with-temp-file source
                      (insert
                       "void setup() { Serial.begin(9600); }\n"
                       "void loop() { Serial.println(\"air-quality\"); }\n"))
                    (with-temp-file arduino-cli
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "{ printf 'CALL'; "
                       "for arg in \"$@\"; do printf ' <%s>' \"$arg\"; done; "
                       "printf ' CWD=<%s>\\n' \"$PWD\"; } "
                       ">> \"${ARDUINO_CLI_LOG:?}\"\n"
                       "if [ \"$1\" = board ] && [ \"$2\" = list ]; then\n"
                       "  printf '%s\\n' "
                       "'{\"detected_ports\":["
                       "{\"port\":{\"address\":\"/dev/ttyACM0\"},"
                       "\"matching_boards\":[{\"name\":\"Arduino Uno\","
                       "\"fqbn\":\"arduino:avr:uno\"}],"
                       "\"boards\":[{\"name\":\"Arduino Uno\","
                       "\"fqbn\":\"arduino:avr:uno\"}]},"
                       "{\"port\":{\"address\":\"/dev/ttyUSB2\"},"
                       "\"matching_boards\":[{\"name\":\"Arduino Nano\","
                       "\"fqbn\":\"arduino:avr:nano\"}],"
                       "\"boards\":[{\"name\":\"Arduino Nano\","
                       "\"fqbn\":\"arduino:avr:nano\"}]}]}'\n"
                       "elif [ \"$1\" = upload ]; then\n"
                       "  printf '%s\\n' "
                       "'Uploaded air-quality firmware to the field Nano.'\n"
                       "else\n"
                       "  printf 'unexpected arduino-cli call\\n' >&2\n"
                       "  exit 64\n"
                       "fi\n"))
                    (set-file-modes arduino-cli #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "ARDUINO_CLI_LOG" calls)
                    (with-current-buffer (find-file-noselect source)
                      (unwind-protect
                          (save-window-excursion
                            (switch-to-buffer (current-buffer))
                            (arduino-cli-mode 1)
                            (let* ((command
                                    (key-binding
                                     (kbd "C-c C-a u")))
                                   (minibuffer-setup-hook
                                    (list
                                     (lambda ()
                                       (push
                                        (minibuffer-prompt)
                                        prompts)))))
                              (execute-kbd-macro
                               (vconcat
                                (kbd "C-c C-a u")
                                "Arduino Nano @ /dev/ttyUSB"
                                (kbd "TAB RET")))
                              (acli-test-await-compilation
                                     arduino-cli--compilation-buffer)
                              (list
                               command
                               (nreverse prompts)
                               (with-current-buffer
                                   arduino-cli--compilation-buffer
                                 (and
                                  (string-match
                                   "Uploaded air-quality firmware[^\n]+"
                                   (buffer-string))
                                  (match-string 0 (buffer-string))))
                               (with-temp-buffer
                                 (insert-file-contents calls)
                                 (replace-regexp-in-string
                                  (regexp-quote
                                   temporary-file-directory)
                                  "<TMP>/"
                                  (buffer-string)
                                  t t)))))
                        (kill-buffer (current-buffer)))))"##,
        expect![[
            r#"OK (arduino-cli-upload ("Board ") "Uploaded air-quality firmware to the field Nano." "CALL <board> <list> <--format> <json> CWD=<<TMP>/portable-air-quality>\nCALL <upload> <--fqbn> <arduino:avr:nano> <--port> </dev/ttyUSB2> <<TMP>/portable-air-quality/> CWD=<<TMP>/portable-air-quality>\n")"#
        ]],
    )
}

fn installs_a_selected_library_version_through_real_minibuffer_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "installs_a_selected_library_version_through_real_minibuffer_completion",
        r##"(let* ((workspace
                          (expand-file-name
                           "firmware-dependencies/"
                           temporary-file-directory))
                         (bin-dir
                          (expand-file-name
                           "fake-arduino-library-bin/"
                           temporary-file-directory))
                         (arduino-cli
                          (expand-file-name "arduino-cli" bin-dir))
                         (calls
                          (expand-file-name
                           "arduino-library-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         prompts)
                    (make-directory workspace t)
                    (make-directory bin-dir t)
                    (with-temp-file arduino-cli
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "{ printf 'CALL'; "
                       "for arg in \"$@\"; do printf ' <%s>' \"$arg\"; done; "
                       "printf ' CWD=<%s>\\n' \"$PWD\"; } "
                       ">> \"${ARDUINO_CLI_LOG:?}\"\n"
                       "if [ \"$1\" = lib ] && [ \"$2\" = search ]; then\n"
                       "  printf '%s\\n' "
                       "'{\"libraries\":[{\"name\":\"ArduinoJson\","
                       "\"available_versions\":[\"7.4.2\",\"7.3.0\"]},"
                       "{\"name\":\"PubSubClient\","
                       "\"available_versions\":[\"2.8.0\"]}]}'\n"
                       "elif [ \"$1\" = lib ] "
                       "&& [ \"$2\" = update-index ]; then\n"
                       "  printf '%s\\n' 'Library index updated.'\n"
                       "elif [ \"$1\" = lib ] && [ \"$2\" = install ]; then\n"
                       "  printf '%s\\n' "
                       "'Installed ArduinoJson 7.3.0 for production firmware.'\n"
                       "else\n"
                       "  printf 'unexpected arduino-cli call\\n' >&2\n"
                       "  exit 64\n"
                       "fi\n"))
                    (set-file-modes arduino-cli #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "ARDUINO_CLI_LOG" calls)
                    (let* ((default-directory workspace)
                           (minibuffer-setup-hook
                            (list
                             (lambda ()
                               (push
                                (minibuffer-prompt)
                                prompts)))))
                      (with-temp-buffer
                        (save-window-excursion
                          (switch-to-buffer (current-buffer))
                          (arduino-cli-mode 1)
                          (let ((command
                                 (key-binding
                                  (kbd "C-c C-a i"))))
                            (execute-kbd-macro
                             (vconcat
                              (kbd "C-u C-c C-a i")
                              "ArduinoJson@7.3"
                              (kbd "TAB RET")))
                            (acli-test-await-compilation
                                   arduino-cli--compilation-buffer)
                            (list
                             command
                             (nreverse prompts)
                             (with-current-buffer
                                 arduino-cli--compilation-buffer
                               (and
                                (string-match
                                 "Installed ArduinoJson 7\\.3\\.0[^\n]+"
                                 (buffer-string))
                                (match-string 0 (buffer-string))))
                             (with-temp-buffer
                               (insert-file-contents calls)
                               (replace-regexp-in-string
                                (regexp-quote
                                 temporary-file-directory)
                                "<TMP>/"
                                (buffer-string)
                                t t))))))))"##,
        expect![[
            r#"OK (arduino-cli-lib-install ("Library ") "Installed ArduinoJson 7.3.0 for production firmware." "CALL <lib> <search> <--format> <json> CWD=<<TMP>/firmware-dependencies>\nCALL <lib> <update-index> CWD=<<TMP>/firmware-dependencies>\nCALL <lib> <install> <ArduinoJson@7.3.0> CWD=<<TMP>/firmware-dependencies>\n")"#
        ]],
    )
}

fn creates_edits_and_saves_a_new_sketch_through_the_live_minor_mode_keymap() -> ParityBatchCase {
    ParityBatchCase::value(
        "creates_edits_and_saves_a_new_sketch_through_the_live_minor_mode_keymap",
        r##"(let* ((sketch-root
                          (expand-file-name
                           "customer-sketches/"
                           temporary-file-directory))
                         (bin-dir
                          (expand-file-name
                           "fake-arduino-sketch-bin/"
                           temporary-file-directory))
                         (arduino-cli
                          (expand-file-name "arduino-cli" bin-dir))
                         (calls
                          (expand-file-name
                           "arduino-sketch-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         prompts)
                    (make-directory sketch-root t)
                    (make-directory bin-dir t)
                    (with-temp-file arduino-cli
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "{ printf 'CALL'; "
                       "for arg in \"$@\"; do printf ' <%s>' \"$arg\"; done; "
                       "printf ' CWD=<%s>\\n' \"$PWD\"; } "
                       ">> \"${ARDUINO_CLI_LOG:?}\"\n"
                       "if [ \"$1\" = sketch ] && [ \"$2\" = new ] "
                       "&& [ \"$3\" = FieldLogger ]; then\n"
                       "  mkdir -p FieldLogger\n"
                       "  : > FieldLogger/FieldLogger.ino\n"
                       "  printf '%s\\n' 'Sketch created in FieldLogger.'\n"
                       "else\n"
                       "  printf 'unexpected arduino-cli call\\n' >&2\n"
                       "  exit 64\n"
                       "fi\n"))
                    (set-file-modes arduino-cli #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "ARDUINO_CLI_LOG" calls)
                    (with-temp-buffer
                      (save-window-excursion
                        (switch-to-buffer (current-buffer))
                        (arduino-cli-mode 1)
                        (let* ((command
                                (key-binding
                                 (kbd "C-c C-a n")))
                               (minibuffer-setup-hook
                                (list
                                 (lambda ()
                                   (push
                                    (minibuffer-prompt)
                                    prompts)))))
                          (execute-kbd-macro
                           (vconcat
                            (kbd "C-c C-a n")
                            "FieldLogger"
                            (kbd "RET")
                            sketch-root
                            (kbd "RET")))
                          (let* ((source
                                  (expand-file-name
                                   "FieldLogger/FieldLogger.ino"
                                   sketch-root))
                                 (buffer
                                  (find-file-noselect source)))
                            (with-current-buffer buffer
                              (insert
                               "void setup() {\n"
                               "  Serial.begin(9600);\n"
                               "}\n\n"
                               "void loop() {\n"
                               "  Serial.println(analogRead(A0));\n"
                               "  delay(1000);\n"
                               "}\n")
                              (save-buffer)
                              (kill-buffer buffer))
                            (list
                             command
                             (nreverse prompts)
                             (file-directory-p
                              (file-name-directory source))
                             (with-temp-buffer
                               (insert-file-contents source)
                               (buffer-string))
                             (with-temp-buffer
                               (insert-file-contents calls)
                               (replace-regexp-in-string
                                (regexp-quote
                                 temporary-file-directory)
                                "<TMP>/"
                                (buffer-string)
                                t t))))))))"##,
        expect![[
            r#"OK (arduino-cli-new-sketch ("Sketch name: " "Sketch path: ") t "void setup() {\n  Serial.begin(9600);\n}\n\nvoid loop() {\n  Serial.println(analogRead(A0));\n  delay(1000);\n}\n" "CALL <sketch> <new> <FieldLogger> CWD=<<TMP>/customer-sketches>\n")"#
        ]],
    )
}

fn opens_reads_and_stops_a_real_serial_monitor_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_reads_and_stops_a_real_serial_monitor_process",
        r##"(let* ((project
                          (expand-file-name
                           "telemetry-node/"
                           temporary-file-directory))
                         (bin-dir
                          (expand-file-name
                           "fake-arduino-monitor-bin/"
                           temporary-file-directory))
                         (arduino-cli
                          (expand-file-name "arduino-cli" bin-dir))
                         (calls
                          (expand-file-name
                           "arduino-monitor-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path)))
                    (make-directory project t)
                    (make-directory bin-dir t)
                    (with-temp-file arduino-cli
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "{ printf 'CALL'; "
                       "for arg in \"$@\"; do printf ' <%s>' \"$arg\"; done; "
                       "printf ' CWD=<%s>\\n' \"$PWD\"; } "
                       ">> \"${ARDUINO_CLI_LOG:?}\"\n"
                       "if [ \"$1\" = board ] && [ \"$2\" = list ]; then\n"
                       "  printf '%s\\n' "
                       "'{\"detected_ports\":[{\"port\":{\"address\":"
                       "\"/dev/ttyUSB4\"},\"matching_boards\":[{\"name\":"
                       "\"Nano 33 IoT\",\"fqbn\":\"arduino:samd:nano_33_iot\"}],"
                       "\"boards\":[{\"name\":\"Nano 33 IoT\","
                       "\"fqbn\":\"arduino:samd:nano_33_iot\"}]}]}'\n"
                       "elif [ \"$1\" = monitor ]; then\n"
                       "  printf '%s\\n' "
                       "'temperature=22.4C humidity=51% sequence=17'\n"
                       "  read ignored || true\n"
                       "else\n"
                       "  printf 'unexpected arduino-cli call\\n' >&2\n"
                       "  exit 64\n"
                       "fi\n"))
                    (set-file-modes arduino-cli #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "ARDUINO_CLI_LOG" calls)
                    (let ((default-directory project)
                          (arduino-cli-verbosity 'quiet)
                          (arduino-cli--monitor-buffer nil))
                      (arduino-cli-start-serial-monitor 57600)
                      (let ((process
                             (get-buffer-process
                              arduino-cli--monitor-buffer))
                            (attempts 0))
                        (while
                            (and
                             process
                             (< attempts 100)
                             (with-current-buffer
                                 arduino-cli--monitor-buffer
                               (not
                                (string-match-p
                                 "temperature=22\\.4C"
                                 (buffer-string)))))
                          (setq attempts (1+ attempts))
                          (accept-process-output process 0.05))
                        (let ((active-before
                               (arduino-cli--serial-monitor-is-active))
                              (telemetry
                              (with-current-buffer
                                  arduino-cli--monitor-buffer
                                 (and
                                  (string-match
                                   "temperature=22\\.4C[^\n]+"
                                   (buffer-string))
                                  (substring-no-properties
                                   (match-string
                                    0
                                    (buffer-string)))))))
                          (arduino-cli-stop-serial-monitor
                           "after capturing one telemetry sample")
                          (while
                              (and process
                                   (process-live-p process))
                            (accept-process-output process 0.05))
                          (list
                           active-before
                           telemetry
                           (process-live-p process)
                           (with-temp-buffer
                             (insert-file-contents calls)
                             (replace-regexp-in-string
                              (regexp-quote
                               temporary-file-directory)
                              "<TMP>/"
                              (buffer-string)
                              t t)))))))"##,
        expect![[
            r#"OK (t "temperature=22.4C humidity=51% sequence=17" nil "CALL <board> <list> <--format> <json> CWD=<<TMP>/telemetry-node>\nCALL <monitor> <--port> </dev/ttyUSB4> <--config> <baudrate=57600> CWD=<<TMP>/telemetry-node>\n")"#
        ]],
    )
}

fn compile_and_upload_stops_then_restarts_the_live_serial_monitor() -> ParityBatchCase {
    ParityBatchCase::value(
        "compile_and_upload_stops_then_restarts_the_live_serial_monitor",
        r##"(let* ((project
                          (expand-file-name
                           "irrigation-controller/"
                           temporary-file-directory))
                         (source
                          (expand-file-name
                           "irrigation_controller.ino"
                           project))
                         (bin-dir
                          (expand-file-name
                           "fake-arduino-lifecycle-bin/"
                           temporary-file-directory))
                         (arduino-cli
                          (expand-file-name "arduino-cli" bin-dir))
                         (calls
                          (expand-file-name
                           "arduino-lifecycle-calls.log"
                           temporary-file-directory))
                         (process-environment
                          (copy-sequence process-environment))
                         (exec-path (cons bin-dir exec-path))
                         (compilation-ask-about-save nil)
                         (compilation-finish-functions nil)
                         (arduino-cli-default-fqbn
                          "arduino:avr:mega")
                         (arduino-cli-default-port
                          "/dev/ttyACM7")
                         (arduino-cli-verbosity 'quiet))
                    (make-directory project t)
                    (make-directory bin-dir t)
                    (with-temp-file source
                      (insert
                       "void setup() { Serial.begin(38400); }\n"
                       "void loop() { Serial.println(\"pump=ready\"); }\n"))
                    (with-temp-file arduino-cli
                      (insert
                       "#!/bin/sh\n"
                       "set -eu\n"
                       "{ printf 'CALL'; "
                       "for arg in \"$@\"; do printf ' <%s>' \"$arg\"; done; "
                       "printf ' CWD=<%s>\\n' \"$PWD\"; } "
                       ">> \"${ARDUINO_CLI_LOG:?}\"\n"
                       "if [ \"$1\" = board ] && [ \"$2\" = list ]; then\n"
                       "  printf '%s\\n' '{\"detected_ports\":[]}'\n"
                       "elif [ \"$1\" = monitor ]; then\n"
                       "  printf '%s\\n' "
                       "'monitor-online pump=ready flow=2.7L/min'\n"
                       "  read ignored || true\n"
                       "elif [ \"$1\" = compile ]; then\n"
                       "  printf '%s\\n' "
                       "'Compiled and uploaded irrigation_controller.ino.'\n"
                       "else\n"
                       "  printf 'unexpected arduino-cli call\\n' >&2\n"
                       "  exit 64\n"
                       "fi\n"))
                    (set-file-modes arduino-cli #o755)
                    (setenv "PATH"
                            (concat bin-dir path-separator (getenv "PATH")))
                    (setenv "ARDUINO_CLI_LOG" calls)
                    (with-current-buffer (find-file-noselect source)
                      (unwind-protect
                          (save-window-excursion
                            (switch-to-buffer (current-buffer))
                            (arduino-cli-mode 1)
                            (let ((command
                                   (key-binding
                                    (kbd "C-c C-a b"))))
                              (arduino-cli-start-serial-monitor
                               38400)
                              (let ((initial-process
                                     (get-buffer-process
                                      arduino-cli--monitor-buffer))
                                    (attempts 0))
                                (while
                                    (and
                                     initial-process
                                     (< attempts 100)
                                     (with-current-buffer
                                         arduino-cli--monitor-buffer
                                       (not
                                        (string-match-p
                                         "monitor-online"
                                         (buffer-string)))))
                                  (setq attempts (1+ attempts))
                                  (accept-process-output
                                   initial-process 0.05))
                                (let ((active-before
                                       (arduino-cli--serial-monitor-is-active)))
                                  (call-interactively command)
                                  (acli-test-await-compilation
                                         arduino-cli--compilation-buffer)
                                  (let ((restarted-process
                                         (get-buffer-process
                                          arduino-cli--monitor-buffer))
                                        (attempts 0))
                                    (while
                                        (and
                                         restarted-process
                                         (< attempts 100)
                                         (with-current-buffer
                                             arduino-cli--monitor-buffer
                                           (<
                                            (count-matches
                                             "monitor-online"
                                             (point-min)
                                             (point-max))
                                            2)))
                                      (setq attempts (1+ attempts))
                                      (accept-process-output
                                       restarted-process 0.05))
                                    (let ((compilation-result
                                           (with-current-buffer
                                               arduino-cli--compilation-buffer
                                             (and
                                              (string-match
                                               "Compiled and uploaded[^\n]+"
                                               (buffer-string))
                                              (match-string
                                               0
                                               (buffer-string)))))
                                          (restart-state
                                           (list
                                            (not
                                             (eq
                                              initial-process
                                              restarted-process))
                                            (and
                                             (process-live-p
                                              restarted-process)
                                             t)
                                            (with-current-buffer
                                                arduino-cli--monitor-buffer
                                              (count-matches
                                               "monitor-online"
                                               (point-min)
                                               (point-max))))))
                                      (arduino-cli-stop-serial-monitor
                                       "after verifying the restarted monitor")
                                      (while
                                          (and
                                           restarted-process
                                           (process-live-p
                                            restarted-process))
                                        (accept-process-output
                                         restarted-process 0.05))
                                      (list
                                       command
                                       active-before
                                       (process-live-p
                                        initial-process)
                                       compilation-result
                                       restart-state
                                       (process-live-p
                                        restarted-process)
                                       compilation-finish-functions
                                       (with-temp-buffer
                                         (insert-file-contents
                                          calls)
                                         (replace-regexp-in-string
                                          (regexp-quote
                                           temporary-file-directory)
                                          "<TMP>/"
                                          (buffer-string)
                                          t t)))))))))
                        (when
                            (buffer-live-p
                             arduino-cli--monitor-buffer)
                          (let ((process
                                 (get-buffer-process
                                  arduino-cli--monitor-buffer)))
                            (when
                                (and process
                                     (process-live-p process))
                              (kill-process process)))
                          (kill-buffer
                           arduino-cli--monitor-buffer))
                        (kill-buffer (current-buffer)))))"##,
        expect![[
            r#"OK (arduino-cli-compile-and-upload t nil "Compiled and uploaded irrigation_controller.ino." (t t 2) nil nil "CALL <board> <list> <--format> <json> CWD=<<TMP>/irrigation-controller>\nCALL <monitor> <--port> </dev/ttyACM7> <--config> <baudrate=38400> CWD=<<TMP>/irrigation-controller>\nCALL <board> <list> <--format> <json> CWD=<<TMP>/irrigation-controller>\nCALL <compile> <--fqbn> <arduino:avr:mega> <--port> </dev/ttyACM7> <--upload> <<TMP>/irrigation-controller/> <--quiet> CWD=<<TMP>/irrigation-controller>\nCALL <board> <list> <--format> <json> CWD=<<TMP>/irrigation-controller>\nCALL <monitor> <--port> </dev/ttyACM7> <--config> <baudrate=115200> CWD=<<TMP>/irrigation-controller>\n")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        compiles_a_saved_weather_station_sketch_from_the_live_minor_mode_keymap(),
        uploads_a_file_local_default_board_through_the_live_mode_menu(),
        selects_the_nonfirst_connected_board_then_uploads_through_real_completion(),
        installs_a_selected_library_version_through_real_minibuffer_completion(),
        creates_edits_and_saves_a_new_sketch_through_the_live_minor_mode_keymap(),
        opens_reads_and_stops_a_real_serial_monitor_process(),
        compile_and_upload_stops_then_restarts_the_live_serial_monitor(),
    ]
}
