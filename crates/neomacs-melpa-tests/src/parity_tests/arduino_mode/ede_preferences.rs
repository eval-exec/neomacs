use expect_test::expect;

use super::ParityBatchCase;

fn preferences_reader_parses_real_file_expands_sketchbook_and_attaches_board_data()
-> ParityBatchCase {
    ParityBatchCase::value(
        "preferences_reader_parses_real_file_expands_sketchbook_and_attaches_board_data",
        r##"(let ((prefs-file
                         (make-temp-file
                          "arduino-preferences-"))
                        (ede-arduino-active-prefs nil)
                        board-lookups)
                    (unwind-protect
                        (progn
                          (with-temp-file prefs-file
                            (insert
                             "editor.font=Monospaced,plain,12\n"
                             "serial.port=/dev/ttyACM2\n"
                             "board=mega\n"
                             "sketchbook.path=~/Embedded Projects\n"))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-board-data)
                                (lambda (board)
                                  (push board board-lookups)
                                  (list :board board))))
                            (ede-arduino-read-prefs prefs-file)
                            (list
                             (oref
                              ede-arduino-active-prefs port)
                             (oref
                              ede-arduino-active-prefs board)
                             (oref
                              ede-arduino-active-prefs
                              sketchbook)
                             (oref
                              ede-arduino-active-prefs
                              boardobj)
                             (integerp
                              (oref
                               ede-arduino-active-prefs
                               prefssize))
                             (consp
                              (oref
                               ede-arduino-active-prefs
                               timestamp))
                             (nreverse board-lookups))))
                      (delete-file prefs-file)))"##,
        expect![[
            r#"OK ("/dev/ttyACM2" "mega" "[ORACLE-HOME]/Embedded Projects/" (:board "mega") t t ("mega"))"#
        ]],
    )
}

fn preferences_cache_skips_unchanged_file_then_refreshes_when_size_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "preferences_cache_skips_unchanged_file_then_refreshes_when_size_changes",
        r##"(let ((prefs-file
                         (make-temp-file
                          "arduino-preferences-cache-"))
                        (ede-arduino-active-prefs nil)
                        board-lookups)
                    (unwind-protect
                        (cl-letf
                            (((symbol-function
                               'ede-arduino-board-data)
                              (lambda (board)
                                (push board board-lookups)
                                (list :board board))))
                          (with-temp-file prefs-file
                            (insert
                             "serial.port=/dev/ttyUSB0\n"
                             "board=uno\n"
                             "sketchbook.path=~/Arduino\n"))
                          (ede-arduino-read-prefs prefs-file)
                          (ede-arduino-read-prefs prefs-file)
                          (with-temp-file prefs-file
                            (insert
                             "serial.port=/dev/ttyUSB123\n"
                             "board=nano\n"
                             "sketchbook.path=~/Arduino Projects\n"))
                          (ede-arduino-read-prefs prefs-file)
                          (list
                           (oref
                            ede-arduino-active-prefs port)
                           (oref
                            ede-arduino-active-prefs board)
                           (file-name-nondirectory
                            (directory-file-name
                             (oref
                              ede-arduino-active-prefs
                              sketchbook)))
                           (nreverse board-lookups)))
                      (delete-file prefs-file)))"##,
        expect![[r#"OK ("/dev/ttyUSB123" "nano" "Arduino Projects" ("uno" "nano"))"#]],
    )
}

fn malformed_preferences_report_each_missing_required_key_precisely() -> ParityBatchCase {
    ParityBatchCase::value(
        "malformed_preferences_report_each_missing_required_key_precisely",
        r##"(let ((root
                         (make-temp-file
                          "arduino-malformed-prefs-" t))
                        outcomes)
                    (unwind-protect
                        (progn
                          (dolist
                              (fixture
                               '(("missing-port"
                                  "board=uno\nsketchbook.path=~/Arduino\n")
                                 ("missing-board"
                                  "serial.port=/dev/ttyUSB0\nsketchbook.path=~/Arduino\n")
                                 ("missing-sketchbook"
                                  "serial.port=/dev/ttyUSB0\nboard=uno\n")))
                            (let ((file
                                   (expand-file-name
                                    (car fixture) root))
                                  (ede-arduino-active-prefs
                                   nil))
                              (with-temp-file file
                                (insert (cadr fixture)))
                              (push
                               (condition-case error-data
                                   (progn
                                     (ede-arduino-read-prefs
                                      file)
                                     :no-error)
                                 (error
                                  (list
                                   (car error-data)
                                   (cadr error-data))))
                               outcomes)))
                          (nreverse outcomes))
                      (delete-directory root t)))"##,
        expect![[
            r#"OK ((error "Cannot find serial.port from the arduino preferences") (error "Cannot find board from the arduino preferences") (error "Cannot find sketchbook.path from the arduino preferences"))"#
        ]],
    )
}

fn sync_declining_to_launch_ide_when_preferences_are_missing_signals_exact_error() -> ParityBatchCase
{
    ParityBatchCase::signal(
        "sync_declining_to_launch_ide_when_preferences_are_missing_signals_exact_error",
        r##"(let ((ede-arduino-preferences-file
                         (expand-file-name
                          "definitely-missing-preferences.txt"
                          temporary-file-directory)))
                    (cl-letf
                        (((symbol-function 'y-or-n-p)
                          (lambda (_prompt) nil)))
                      (ede-arduino-sync)))"##,
        expect![[
            r#"ERR (error "EDE cannot build/upload arduino projects without preferences from the arduino IDE")"#
        ]],
    )
}

fn sync_accepting_missing_preferences_launches_ide_then_reads_and_returns_active_object()
-> ParityBatchCase {
    ParityBatchCase::value(
        "sync_accepting_missing_preferences_launches_ide_then_reads_and_returns_active_object",
        r##"(let ((ede-arduino-preferences-file
                         (expand-file-name
                          "missing-but-created-by-ide.txt"
                          temporary-file-directory))
                        (ede-arduino-active-prefs nil)
                        events)
                    (cl-letf
                        (((symbol-function 'file-exists-p)
                          (lambda (_file) nil))
                         ((symbol-function 'y-or-n-p)
                          (lambda (prompt)
                            (push
                             (list :confirm prompt)
                             events)
                            t))
                         ((symbol-function 'ede-arduino)
                          (lambda ()
                            (push :launch events)))
                         ((symbol-function
                           'ede-arduino-read-prefs)
                          (lambda (file)
                            (push
                             (list
                              :read
                              (file-name-nondirectory file))
                             events)
                            (setq
                             ede-arduino-active-prefs
                             (make-instance
                              'ede-arduino-prefs)))))
                      (let ((result
                             (ede-arduino-sync)))
                        (list
                         (eq
                          result
                          ede-arduino-active-prefs)
                         (nreverse events)))))"##,
        expect![[
            r#"OK (t ((:confirm "Can't find arduino preferences.  Start IDE to configure? ") :launch (:read "missing-but-created-by-ide.txt")))"#
        ]],
    )
}

fn configured_install_paths_honor_container_prefix_and_host_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "configured_install_paths_honor_container_prefix_and_host_fallback",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-install-paths-" t))
                         (host
                          (expand-file-name "host-app" root))
                         (container-root
                          (file-name-as-directory
                           (expand-file-name
                            "container" root)))
                         (container-app
                          (expand-file-name
                           "opt/arduino"
                           container-root)))
                    (unwind-protect
                        (progn
                          (make-directory host t)
                          (make-directory container-app t)
                          (list
                           (let ((ede-arduino-appdir host)
                                 (ede-arduino-container-prefix
                                  nil))
                             (file-equal-p
                              (ede-arduino-find-install)
                              host))
                           (let ((ede-arduino-appdir
                                  "opt/arduino")
                                 (ede-arduino-container-prefix
                                  container-root))
                             (list
                              (ede-arduino-find-install)
                              (file-equal-p
                               (ede-arduino-find-install t)
                               container-app)))))
                      (delete-directory root t)))"##,
        expect![[r#"OK (t ("opt/arduino" t))"#]],
    )
}

fn install_discovery_parses_appdir_from_a_real_arduino_launcher_script() -> ParityBatchCase {
    ParityBatchCase::value(
        "install_discovery_parses_appdir_from_a_real_arduino_launcher_script",
        r##"(let ((launcher
                         (make-temp-file
                          "arduino-launcher-"))
                        (ede-arduino-appdir nil))
                    (unwind-protect
                        (progn
                          (with-temp-file launcher
                            (insert
                             "#!/bin/sh\n"
                             "APPDIR=/opt/arduino-1.8.19\n"
                             "exec java \"$@\"\n"))
                          (let ((ede-arduino-arduino-command
                                 launcher))
                            (list
                             (ede-arduino-find-install)
                             ede-arduino-appdir)))
                      (delete-file launcher)))"##,
        expect!["OK (nil nil)"],
    )
}

fn missing_install_command_reports_exact_discovery_error() -> ParityBatchCase {
    ParityBatchCase::signal(
        "missing_install_command_reports_exact_discovery_error",
        r##"(let ((ede-arduino-appdir nil)
                        (ede-arduino-arduino-command
                         "arduino-command-that-cannot-exist")
                        (exec-path nil))
                    (ede-arduino-find-install))"##,
        expect!["ERR (wrong-type-argument stringp nil)"],
    )
}

fn version_makefile_boards_and_library_helpers_resolve_real_install_layout() -> ParityBatchCase {
    ParityBatchCase::value(
        "version_makefile_boards_and_library_helpers_resolve_real_install_layout",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-layout-" t))
                         (lib
                          (expand-file-name "lib" root))
                         (ede-arduino-boards-file
                          "hardware/vendor/boards.txt"))
                    (unwind-protect
                        (progn
                          (make-directory lib t)
                          (with-temp-file
                              (expand-file-name
                               "version.txt" lib)
                            (insert "2.3.4\nextra\n"))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-find-install)
                                (lambda (&optional _full)
                                  root)))
                            (list
                             (ede-arduino-Arduino-Version)
                             (file-relative-name
                              (ede-arduino-Arduino.mk)
                              root)
                             (file-relative-name
                              (ede-arduino-boards.txt)
                              root)
                             (file-relative-name
                              (ede-arduino-libdir)
                              root)
                             (file-relative-name
                              (ede-arduino-libdir "Servo")
                              root))))
                      (delete-directory root t)))"##,
        expect![[
            r#"OK ("2.3.4" "Arduino.mk" "hardware/vendor/boards.txt" "libraries" "libraries/Servo")"#
        ]],
    )
}

fn board_reader_builds_complete_board_object_from_realistic_boards_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "board_reader_builds_complete_board_object_from_realistic_boards_file",
        r##"(let ((boards-file
                         (make-temp-file
                          "arduino-boards-")))
                    (unwind-protect
                        (progn
                          (with-temp-file boards-file
                            (insert
                             "uno.name=Arduino Uno\n"
                             "uno.upload.protocol=arduino\n"
                             "uno.upload.speed=115200\n"
                             "uno.upload.maximum_size=32256\n"
                             "uno.build.mcu=atmega328p\n"
                             "uno.build.f_cpu=16000000L\n"
                             "uno.build.core=arduino\n"
                             "mega.name=Arduino Mega\n"))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-boards.txt)
                                (lambda () boards-file)))
                            (let ((board
                                   (ede-arduino-board-data
                                    "uno")))
                              (list
                               (object-of-class-p
                                board
                                'ede-arduino-board)
                               (oref board name)
                               (oref board protocol)
                               (oref board speed)
                               (oref board maximum-size)
                               (oref board mcu)
                               (oref board f_cpu)
                               (oref board core)))))
                      (delete-file boards-file)))"##,
        expect![[
            r#"OK (t "Arduino Uno" "arduino" "115200" "32256" "atmega328p" "16000000L" "arduino")"#
        ]],
    )
}

fn ide_launcher_uses_current_directory_buffer_and_configured_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "ide_launcher_uses_current_directory_buffer_and_configured_command",
        r##"(let ((work
                         (make-temp-file
                          "arduino-ide-work-" t))
                        (ede-arduino-arduino-command
                         "/opt/arduino/arduino")
                        calls)
                    (unwind-protect
                        (let ((default-directory
                                (file-name-as-directory work)))
                          (cl-letf
                              (((symbol-function 'start-process)
                                (lambda (&rest args)
                                  (push
                                   (list
                                    args
                                    (file-equal-p
                                     default-directory
                                     work)
                                    (buffer-string))
                                   calls)
                                  'fake-process)))
                            (with-current-buffer
                                (get-buffer-create
                                 "*Arduino IDE*")
                              (insert "stale output"))
                            (list
                             (ede-arduino)
                             (nreverse calls)
                             (with-current-buffer
                                 "*Arduino IDE*"
                               (buffer-string)))))
                      (when (get-buffer "*Arduino IDE*")
                        (kill-buffer "*Arduino IDE*"))
                      (delete-directory work t)))"##,
        expect![[
            r#"OK (fake-process ((("arduino" (:buffer nil) "/opt/arduino/arduino") t "")) "")"#
        ]],
    )
}

pub(super) fn ede_preferences_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        preferences_reader_parses_real_file_expands_sketchbook_and_attaches_board_data(),
        preferences_cache_skips_unchanged_file_then_refreshes_when_size_changes(),
        malformed_preferences_report_each_missing_required_key_precisely(),
        sync_declining_to_launch_ide_when_preferences_are_missing_signals_exact_error(),
        sync_accepting_missing_preferences_launches_ide_then_reads_and_returns_active_object(),
        configured_install_paths_honor_container_prefix_and_host_fallback(),
        install_discovery_parses_appdir_from_a_real_arduino_launcher_script(),
        missing_install_command_reports_exact_discovery_error(),
        version_makefile_boards_and_library_helpers_resolve_real_install_layout(),
        board_reader_builds_complete_board_object_from_realistic_boards_file(),
        ide_launcher_uses_current_directory_buffer_and_configured_command(),
    ]
}
