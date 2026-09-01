use expect_test::expect;

use super::ParityBatchCase;

fn library_guessing_scans_real_includes_skips_local_headers_and_adds_utility_directory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "library_guessing_scans_real_includes_skips_local_headers_and_adds_utility_directory",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-library-scan-" t))
                         (libraries
                          (expand-file-name
                           "libraries" root))
                         (servo
                          (expand-file-name
                           "Servo" libraries))
                         (sketch
                          (expand-file-name
                           "LibraryDemo.ino" root))
                         (project
                          (make-instance
                           'ede-arduino-project
                           :name "LibraryDemo"
                           :directory
                           (file-name-as-directory root)
                           :file sketch
                           :targets nil))
                         (ede-object-project project))
                    (unwind-protect
                        (progn
                          (make-directory
                           (expand-file-name
                            "utility" servo)
                           t)
                          (make-directory
                           (expand-file-name
                            "Wire" libraries)
                           t)
                          (with-temp-file
                              (expand-file-name
                               "Local.h" root)
                            (insert "#pragma once\n"))
                          (with-temp-file sketch
                            (insert
                             "#include <Servo.h>\n"
                             "#include <Wire.h>\n"
                             "#include <Local.h>\n"
                             "#include \"Quoted.h\"\n"))
                          (let ((default-directory
                                  (file-name-as-directory
                                   root)))
                            (cl-letf
                                (((symbol-function
                                   'ede-arduino-libdir)
                                  (lambda (&optional library)
                                    (if library
                                        (expand-file-name
                                         library libraries)
                                      libraries))))
                              (ede-arduino-guess-libs))))
                      (when (get-file-buffer sketch)
                        (kill-buffer
                         (get-file-buffer sketch)))
                      (delete-directory root t)))"##,
        expect![[
            r#"OK (#("Wire" 0 4 (fontified nil)) #("Servo" 0 5 (fontified nil)) #("Servo/utility" 0 5 (fontified nil)))"#
        ]],
    )
}

fn buffer_local_library_override_preserves_declared_order_without_scanning_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_local_library_override_preserves_declared_order_without_scanning_source",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-library-override-" t))
                         (sketch
                          (expand-file-name
                           "Override.ino" root))
                         (project
                          (make-instance
                           'ede-arduino-project
                           :name "Override"
                           :directory
                           (file-name-as-directory root)
                           :file sketch
                           :targets nil))
                         (ede-object-project project))
                    (unwind-protect
                        (progn
                          (with-temp-file sketch
                            (insert
                             "#include <Ignored.h>\n"))
                          (let ((buffer
                                 (find-file-noselect
                                  sketch)))
                            (with-current-buffer buffer
                              (setq-local
                               arduino-libraries
                               "Ethernet SD Servo"))
                            (unwind-protect
                                (ede-arduino-guess-libs)
                              (kill-buffer buffer))))
                      (delete-directory root t)))"##,
        expect![[r#"OK ("Servo" "SD" "Ethernet")"#]],
    )
}

fn makefile_generation_supplies_complete_real_project_board_and_library_contract() -> ParityBatchCase
{
    ParityBatchCase::value(
        "makefile_generation_supplies_complete_real_project_board_and_library_contract",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-makefile-" t))
                         (sketch
                          (expand-file-name
                           "BuildDemo.ino" root))
                         (project
                          (make-instance
                           'ede-arduino-project
                           :name "BuildDemo"
                           :directory
                           (file-name-as-directory root)
                           :file sketch
                           :targets nil))
                         (board
                          (make-instance
                           'ede-arduino-board
                           :name "Arduino Uno"
                           :protocol "arduino"
                           :speed "115200"
                           :maximum-size "32256"
                           :mcu "atmega328p"
                           :f_cpu "16000000L"
                           :core "arduino"))
                         (prefs
                          (make-instance
                           'ede-arduino-prefs))
                         (ede-arduino-avrdude-baudrate
                          "57600")
                         events)
                    (oset prefs port "/dev/ttyACM0")
                    (oset prefs boardobj board)
                    (unwind-protect
                        (progn
                          (with-temp-file sketch
                            (insert "void setup() {}\n"))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-sync)
                                (lambda ()
                                  (push :sync events)
                                  prefs))
                               ((symbol-function
                                 'ede-arduino-Arduino-Version)
                                (lambda () "1.8.19"))
                               ((symbol-function
                                 'ede-arduino-guess-sketch)
                                (lambda () sketch))
                               ((symbol-function
                                 'ede-arduino-guess-libs)
                                (lambda ()
                                  '("Servo"
                                    "Ethernet/utility")))
                               ((symbol-function
                                 'ede-arduino-Arduino.mk)
                                (lambda ()
                                  "/opt/arduino/Arduino.mk"))
                               ((symbol-function
                                 'ede-arduino-find-install)
                                (lambda (&optional _full)
                                  "/opt/arduino"))
                               ((symbol-function
                                 'ede-srecode-setup)
                                (lambda ()
                                  (push :setup events)))
                               ((symbol-function
                                 'ede-srecode-insert)
                                (lambda (&rest args)
                                  (push
                                   (cons :insert args)
                                   events)
                                  (insert
                                   "# generated makefile\n"))))
                            (ede-arduino-create-makefile
                             project)
                            (list
                             (with-temp-buffer
                               (insert-file-contents
                                (expand-file-name
                                 "Makefile" root))
                               (buffer-string))
                             (nreverse events)
                             (get-file-buffer
                              (expand-file-name
                               "Makefile" root)))))
                      (when
                          (get-file-buffer sketch)
                        (kill-buffer
                         (get-file-buffer sketch)))
                      (when
                          (get-file-buffer
                           (expand-file-name
                            "Makefile" root))
                        (kill-buffer
                         (get-file-buffer
                          (expand-file-name
                           "Makefile" root))))
                      (delete-directory root t)))"##,
        expect![[
            r##"OK ("# generated makefile\n" (:sync :setup (:insert "arduino:ede-empty" "TARGET" "BuildDemo" "ARDUINO_LIBS" "Servo Ethernet/utility" "MCU" "atmega328p" "F_CPU" "16000000L" "PORT" "/dev/ttyACM0" "AVRDUDE_ARD_BAUDRATE" "57600" "AVRDUDE_ARD_PROGRAMMER" "arduino" "ARDUINO_MK" "/opt/arduino/Arduino.mk" "ARDUINO_HOME" "/opt/arduino")) nil)"##
        ]],
    )
}

fn makefile_generation_rejects_ino_before_one_zero_and_pde_at_or_after_one_zero() -> ParityBatchCase
{
    ParityBatchCase::value(
        "makefile_generation_rejects_ino_before_one_zero_and_pde_at_or_after_one_zero",
        r##"(let* ((project
                          (make-instance
                           'ede-arduino-project
                           :name "Versioned"
                           :directory "/workspace/Versioned/"
                           :file
                           "/workspace/Versioned/Versioned.ino"
                           :targets nil))
                         (prefs
                          (make-instance
                           'ede-arduino-prefs))
                         outcomes)
                    (dolist
                        (case
                         '(("0.23"
                            "/workspace/Versioned/Versioned.ino")
                           ("1.0"
                            "/workspace/Versioned/Versioned.pde")))
                      (cl-letf
                          (((symbol-function
                             'ede-arduino-sync)
                            (lambda () prefs))
                           ((symbol-function
                             'ede-arduino-Arduino-Version)
                            (lambda () (car case)))
                           ((symbol-function
                             'ede-arduino-guess-sketch)
                            (lambda () (cadr case))))
                        (push
                         (condition-case error-data
                             (progn
                               (ede-arduino-create-makefile
                                project)
                               :no-error)
                           (error
                            (list
                             (car error-data)
                             (cadr error-data))))
                         outcomes)))
                    (nreverse outcomes))"##,
        expect![[
            r#"OK ((error "Makefile doesn’t support .ino files until Arduino 1.0") (error "Makefile doesn’t support .pde files after Arduino 1.0"))"#
        ]],
    )
}

fn makefile_generation_refuses_to_replace_unmanaged_content_when_user_declines() -> ParityBatchCase
{
    ParityBatchCase::signal(
        "makefile_generation_refuses_to_replace_unmanaged_content_when_user_declines",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-makefile-refusal-" t))
                         (makefile
                          (expand-file-name "Makefile" root))
                         (sketch
                          (expand-file-name
                           "Refusal.ino" root))
                         (project
                          (make-instance
                           'ede-arduino-project
                           :name "Refusal"
                           :directory
                           (file-name-as-directory root)
                           :file sketch
                           :targets nil))
                         (prefs
                          (make-instance
                           'ede-arduino-prefs)))
                    (unwind-protect
                        (progn
                          (with-temp-file makefile
                            (insert
                             "# maintained by the developer\n"))
                          (with-temp-file sketch
                            (insert "void setup() {}\n"))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-sync)
                                (lambda () prefs))
                               ((symbol-function
                                 'ede-arduino-Arduino-Version)
                                (lambda () "1.8.19"))
                               ((symbol-function
                                 'ede-arduino-guess-sketch)
                                (lambda () sketch))
                               ((symbol-function 'y-or-n-p)
                                (lambda (_prompt) nil)))
                            (ede-arduino-create-makefile
                             project)))
                      (when
                          (get-file-buffer makefile)
                        (kill-buffer
                         (get-file-buffer makefile)))
                      (when
                          (get-file-buffer sketch)
                        (kill-buffer
                         (get-file-buffer sketch)))
                      (delete-directory root t)))"##,
        expect![[r#"ERR (error "Not replacing Makefile")"#]],
    )
}

pub(super) fn ede_makefile_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        library_guessing_scans_real_includes_skips_local_headers_and_adds_utility_directory(),
        buffer_local_library_override_preserves_declared_order_without_scanning_source(),
        makefile_generation_supplies_complete_real_project_board_and_library_contract(),
        makefile_generation_rejects_ino_before_one_zero_and_pde_at_or_after_one_zero(),
        makefile_generation_refuses_to_replace_unmanaged_content_when_user_declines(),
    ]
}
