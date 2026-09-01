use expect_test::expect;

use super::ParityBatchCase;

fn optional_module_loads_real_pinned_flycheck_and_registers_complete_checker_definition()
-> ParityBatchCase {
    ParityBatchCase::value(
        "optional_module_loads_real_pinned_flycheck_and_registers_complete_checker_definition",
        r##"(let* ((descriptor
                          (cadr
                           (assq 'flycheck package-alist)))
                         (command
                          (flycheck-checker-get
                           'arduino 'command))
                         (patterns
                          (flycheck-checker-get
                           'arduino 'error-patterns)))
                    (list
                     (featurep 'flycheck)
                     (featurep 'flycheck-arduino)
                     (package-version-join
                      (package-desc-version descriptor))
                     command
                     (flycheck-checker-get
                      'arduino 'modes)
                     (length patterns)
                     (mapcar #'cdr patterns)
                     (documentation
                      'flycheck-arduino-setup)
                     flycheck-arduino-board))"##,
        expect![[
            r#"OK (t t "20260728.931" ("arduino" "--verify" source) (arduino-mode) 2 (warning error) "Setup Flycheck Arduino.\nAdd ‘arduino’ to ‘flycheck-checkers’." nil)"#
        ]],
    )
}

fn setup_adds_arduino_checker_once_and_keeps_existing_checker_order_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_adds_arduino_checker_once_and_keeps_existing_checker_order_stable",
        r##"(let ((flycheck-checkers
                         '(emacs-lisp
                           c/c++-clang
                           emacs-lisp-checkdoc)))
                    (flycheck-arduino-setup)
                    (flycheck-arduino-setup)
                    (list
                     flycheck-checkers
                     (length
                      (seq-filter
                       (lambda (checker)
                         (eq checker 'arduino))
                       flycheck-checkers))))"##,
        expect!["OK ((arduino emacs-lisp c/c++-clang emacs-lisp-checkdoc) 1)"],
    )
}

fn realistic_compiler_warning_and_fatal_error_parse_into_precise_flycheck_objects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "realistic_compiler_warning_and_fatal_error_parse_into_precise_flycheck_objects",
        r##"(let ((buffer
                         (get-buffer-create
                          " *arduino-flycheck-source*")))
                    (unwind-protect
                        (with-current-buffer buffer
                          (setq buffer-file-name
                                "/workspace/Blink/Blink.ino")
                          (mapcar
                           (lambda (error)
                             (list
                              (flycheck-error-filename
                               error)
                              (flycheck-error-line error)
                              (flycheck-error-column
                               error)
                              (flycheck-error-level error)
                              (flycheck-error-message
                               error)
                              (flycheck-error-checker
                               error)
                              (eq
                               (flycheck-error-buffer
                                error)
                               buffer)))
                           (flycheck-parse-with-patterns
                            (concat
                             "/workspace/Blink/Blink.ino:12:7: warning: unused variable 'reading'\n"
                             "/workspace/Blink/Blink.ino:18:3: fatal error: Servo.h: No such file or directory\n"
                             "collect2: unrelated noise\n")
                            'arduino buffer)))
                      (kill-buffer buffer)))"##,
        expect![[
            r#"OK (("/workspace/Blink/Blink.ino" 12 7 warning "unused variable 'reading'" arduino t) ("/workspace/Blink/Blink.ino" 18 3 error "Servo.h: No such file or directory" arduino t))"#
        ]],
    )
}

fn checker_supports_only_arduino_mode_and_substitutes_real_source_argument() -> ParityBatchCase {
    ParityBatchCase::value(
        "checker_supports_only_arduino_mode_and_substitutes_real_source_argument",
        r##"(let ((source
                         (make-temp-file
                          "arduino-flycheck-" nil ".ino")))
                    (unwind-protect
                        (with-temp-buffer
                          (setq buffer-file-name source)
                          (list
                           (flycheck-checker-supports-major-mode-p
                            'arduino 'arduino-mode)
                           (flycheck-checker-supports-major-mode-p
                            'arduino 'c-mode)
                           (mapcar
                            (lambda (argument)
                              (cond
                               ((stringp argument)
                               argument)
                               ((eq argument 'source)
                                (let ((filename
                                       (file-name-nondirectory
                                        (car
                                         (flycheck-substitute-argument
                                          argument
                                          'arduino)))))
                                  (list
                                   (string-prefix-p
                                    "arduino-flycheck-"
                                    filename)
                                   (string-suffix-p
                                    ".ino" filename))))
                               (t argument)))
                            (flycheck-checker-get
                             'arduino 'command))))
                      (delete-file source)))"##,
        expect![[r#"OK ((arduino-mode) nil ("arduino" "--verify" (t t)))"#]],
    )
}

pub(super) fn flycheck_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        optional_module_loads_real_pinned_flycheck_and_registers_complete_checker_definition(),
        setup_adds_arduino_checker_once_and_keeps_existing_checker_order_stable(),
        realistic_compiler_warning_and_fatal_error_parse_into_precise_flycheck_objects(),
        checker_supports_only_arduino_mode_and_substitutes_real_source_argument(),
    ]
}
