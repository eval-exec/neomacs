use expect_test::expect;

use super::ParityBatchCase;

fn realistic_sketch_enters_arduino_mode_and_fontifies_language_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "realistic_sketch_enters_arduino_mode_and_fontifies_language_semantics",
        r##"(with-temp-buffer
                    (insert
                     "const unsigned long interval = 1000;\n"
                     "void setup() {\n"
                     "  pinMode(LED_BUILTIN, OUTPUT);\n"
                     "  Serial.begin(9600);\n"
                     "}\n"
                     "void loop() {\n"
                     "  if (digitalRead(2) == HIGH) {\n"
                     "    digitalWrite(LED_BUILTIN, LOW);\n"
                     "  }\n"
                     "}\n")
                    (arduino-mode)
                    (font-lock-ensure)
                    (let ((face-at
                           (lambda (token)
                             (goto-char (point-min))
                             (search-forward token)
                             (get-text-property
                              (match-beginning 0) 'face))))
                      (list
                       major-mode mode-name
                       (funcall face-at "const")
                       (funcall face-at "unsigned")
                       (funcall face-at "setup")
                       (funcall face-at "pinMode")
                       (funcall face-at "LED_BUILTIN")
                       (funcall face-at "Serial")
                       (funcall face-at "if")
                       (funcall face-at "digitalWrite"))))"##,
        expect![[
            r#"OK (arduino-mode "arduino/*l" font-lock-type-face font-lock-type-face font-lock-type-face font-lock-keyword-face font-lock-constant-face font-lock-keyword-face font-lock-type-face font-lock-keyword-face)"#
        ]],
    )
}

fn practical_sketch_indentation_comments_and_cc_mode_state_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "practical_sketch_indentation_comments_and_cc_mode_state_match",
        r##"(with-temp-buffer
                    (insert
                     "void setup(){\n"
                     "pinMode(LED_BUILTIN,OUTPUT);\n"
                     "if(digitalRead(2)==HIGH){\n"
                     "digitalWrite(LED_BUILTIN,LOW);\n"
                     "}\n"
                     "}\n")
                    (arduino-mode)
                    (indent-region (point-min) (point-max))
                    (list
                     (buffer-string)
                     c-basic-offset tab-width
                     comment-start comment-end
                     (eq
                      (keymap-parent (current-local-map))
                      c-mode-base-map)
                     (derived-mode-p 'c-mode)))"##,
        expect![[
            r#"OK ("void setup(){\n\11pinMode(LED_BUILTIN,OUTPUT);\n\11if(digitalRead(2)==HIGH){\n\11\11digitalWrite(LED_BUILTIN,LOW);\n\11}\n}\n" 2 2 "/* " " */" t c-mode)"#
        ]],
    )
}

fn syntax_table_parses_comments_strings_and_braces_like_real_arduino_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "syntax_table_parses_comments_strings_and_braces_like_real_arduino_code",
        r##"(with-temp-buffer
                    (insert
                     "void loop() {\n"
                     "  // braces { in comment }\n"
                     "  Serial.println(\"value // not comment\");\n"
                     "  /* block comment */\n"
                     "}\n")
                    (arduino-mode)
                    (let (states)
                      (dolist
                          (needle
                           '("braces"
                             "value"
                             "block"
                             "Serial"
                             "}"))
                        (goto-char (point-min))
                        (search-forward needle)
                        (let ((state
                               (syntax-ppss
                                (match-beginning 0))))
                          (push
                           (list
                            needle
                            (nth 3 state)
                            (nth 4 state)
                            (nth 0 state))
                           states)))
                      (nreverse states)))"##,
        expect![[
            r#"OK (("braces" nil t 1) ("value" 34 nil 2) ("block" nil t 1) ("Serial" nil nil 1) ("}" nil t 1))"#
        ]],
    )
}

fn language_tables_cover_representative_types_constants_functions_and_primary_objects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "language_tables_cover_representative_types_constants_functions_and_primary_objects",
        r##"(list
                    (mapcar
                     (lambda (word)
                       (cons
                        word
                        (not
                         (null
                          (member
                           word
                           (c-lang-const
                            c-primitive-type-kwds arduino))))))
                     '("boolean" "unsigned long"
                       "setup" "PROGMEM" "class"))
                    (mapcar
                     (lambda (word)
                       (cons
                        word
                        (not
                         (null
                          (member
                           word
                           (c-lang-const
                            c-constant-kwds arduino))))))
                     '("HIGH" "INPUT_PULLUP"
                       "LED_BUILTIN" "nullptr"))
                    (mapcar
                     (lambda (word)
                       (cons
                        word
                        (not
                         (null
                          (member
                           word
                           (c-lang-const
                            c-simple-stmt-kwds arduino))))))
                     '("digitalWrite" "pulseInLong"
                       "isHexadecimalDigit"
                       "releaseAll" "malloc"))
                    (mapcar
                     (lambda (word)
                       (cons
                        word
                        (not
                         (null
                          (member
                           word
                           (c-lang-const
                            c-primary-expr-kwds arduino))))))
                     '("Serial" "Keyboard" "Mouse" "Wire")))"##,
        expect![[
            r#"OK ((("boolean" . t) ("unsigned long" . t) ("setup" . t) ("PROGMEM" . t) ("class")) (("HIGH" . t) ("INPUT_PULLUP" . t) ("LED_BUILTIN" . t) ("nullptr" . t)) (("digitalWrite" . t) ("pulseInLong" . t) ("isHexadecimalDigit" . t) ("releaseAll" . t) ("malloc")) (("Serial" . t) ("Keyboard" . t) ("Mouse" . t) ("Wire")))"#
        ]],
    )
}

fn mode_activation_calls_optional_flycheck_setup_only_when_available() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_activation_calls_optional_flycheck_setup_only_when_available",
        r##"(list
                    (let (events)
                      (cl-letf
                          (((symbol-function 'flycheck-mode)
                            (lambda () nil))
                           ((symbol-function
                             'flycheck-arduino-setup)
                            (lambda ()
                              (push :setup events))))
                        (with-temp-buffer
                          (arduino-mode)
                          (list
                           (nreverse events)
                           major-mode))))
                    (let (events)
                      (when
                          (fboundp 'flycheck-mode)
                        (fmakunbound 'flycheck-mode))
                      (cl-letf
                          (((symbol-function
                             'flycheck-arduino-setup)
                            (lambda ()
                              (push :setup events))))
                        (with-temp-buffer
                          (arduino-mode)
                          (list
                           events major-mode)))))"##,
        expect!["OK (((:setup) arduino-mode) (nil arduino-mode))"],
    )
}

fn new_sketch_uses_expanded_arduino_home_as_the_visit_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "new_sketch_uses_expanded_arduino_home_as_the_visit_directory",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-mode-home-" t))
                         (arduino-mode-home root)
                         visited)
                    (unwind-protect
                        (cl-letf
                            (((symbol-function 'find-file)
                              (lambda (filename &rest _args)
                                (setq visited
                                      (list
                                       filename
                                       default-directory))
                                :visited)))
                          (list
                           (arduino-sketch-new
                            "Blink/Blink.ino")
                           (car visited)
                           (file-equal-p
                            (cadr visited)
                            root)))
                      (delete-directory root t)))"##,
        expect![[r#"OK (:visited "Blink/Blink.ino" t)"#]],
    )
}

fn include_path_generator_creates_real_default_content_then_visits_existing_file() -> ParityBatchCase
{
    ParityBatchCase::value(
        "include_path_generator_creates_real_default_content_then_visits_existing_file",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-include-path-" t))
                         (arduino-mode-home root)
                         (target
                          (expand-file-name
                           ".clang_complete" root))
                         first-result second-result visited)
                    (unwind-protect
                        (progn
                          (setq first-result
                                (arduino-generate-include-path-file))
                          (let ((first-content
                                 (with-temp-buffer
                                   (insert-file-contents target)
                                   (buffer-string))))
                            (cl-letf
                                (((symbol-function 'y-or-n-p)
                                  (lambda (_prompt) t))
                                 ((symbol-function 'find-file)
                                  (lambda (filename &rest _args)
                                    (setq visited filename)
                                    :editing)))
                              (setq second-result
                                    (arduino-generate-include-path-file)))
                            (list
                             first-result first-content
                             second-result
                             (file-name-nondirectory visited)
                             (file-exists-p target))))
                      (delete-directory root t)))"##,
        expect![[
            r#"OK (nil "-I/home/stardiviner/Arduino/libraries/" :editing ".clang_complete" t)"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        realistic_sketch_enters_arduino_mode_and_fontifies_language_semantics(),
        practical_sketch_indentation_comments_and_cc_mode_state_match(),
        syntax_table_parses_comments_strings_and_braces_like_real_arduino_code(),
        language_tables_cover_representative_types_constants_functions_and_primary_objects(),
        mode_activation_calls_optional_flycheck_setup_only_when_available(),
        new_sketch_uses_expanded_arduino_home_as_the_visit_directory(),
        include_path_generator_creates_real_default_content_then_visits_existing_file(),
    ]
}
