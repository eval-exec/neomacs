use expect_test::expect;

use super::ParityBatchCase;

fn evil_ex_parser_builds_exact_command_range_and_argument_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_ex_parser_builds_exact_command_range_and_argument_forms",
        r##"(list
               (evil-ex-parse "5cmd arg")
               (evil-ex-parse "5cmd !arg")
               (evil-ex-parse "5 arg")
               (evil-ex-parse "+1,+2t-1")
               (evil-ex-parse "ido-mode")
               (evil-ex-parse "yas/reload-all")
               (evil-ex-parse "make-frame"))"##,
        expect![[
            r#"OK ((evil-ex-call-command (string-to-number "5") "cmd" "arg") (evil-ex-call-command (string-to-number "5") "cmd" "!arg") (evil-ex-call-command (string-to-number "5") "arg" nil) (evil-ex-call-command (let ((l1 (evil-ex-line nil (+ (evil-ex-signed-number (intern "+") (string-to-number "1")))))) (save-excursion (and l1 (string= "," ";") (goto-line l1)) (evil-ex-range l1 (evil-ex-line nil (+ (evil-ex-signed-number (intern "+") (string-to-number "2"))))))) "t" "-1") (evil-ex-call-command nil "ido-mode" nil) (evil-ex-call-command nil "yas/reload-all" nil) (evil-ex-call-command nil "make-frame" nil))"#
        ]],
    )
}

fn evil_ex_parser_builds_exact_percent_marker_numeric_and_relative_ranges() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_ex_parser_builds_exact_percent_marker_numeric_and_relative_ranges",
        r##"(list
               (evil-ex-parse "%" nil 'range)
               (evil-ex-parse "*" nil 'range)
               (evil-ex-parse "5,27" nil 'range)
               (evil-ex-parse "5,$" nil 'range)
               (evil-ex-parse "5,'x" nil 'range)
               (evil-ex-parse "`x,`y" nil 'range)
               (evil-ex-parse "5,+" nil 'range)
               (evil-ex-parse ".+42" nil 'range)
               (evil-ex-parse ";']" nil 'range))"##,
        expect![[
            r#"OK ((evil-ex-full-range) (evil-ex-last-visual-range) (let ((l1 (evil-ex-line (string-to-number "5") nil))) (save-excursion (and l1 (string= "," ";") (goto-line l1)) (evil-ex-range l1 (evil-ex-line (string-to-number "27") nil)))) (let ((l1 (evil-ex-line (string-to-number "5") nil))) (save-excursion (and l1 (string= "," ";") (goto-line l1)) (evil-ex-range l1 (evil-ex-line (evil-ex-last-line) nil)))) (let ((l1 (evil-ex-line (string-to-number "5") nil))) (save-excursion (and l1 (string= "," ";") (goto-line l1)) (evil-ex-range l1 (evil-ex-line (evil-ex-marker "x") nil)))) (evil-ex-char-marker-range "x" "y") (let ((l1 (evil-ex-line (string-to-number "5") nil))) (save-excursion (and l1 (string= "," ";") (goto-line l1)) (evil-ex-range l1 (evil-ex-line nil (+ (evil-ex-signed-number (intern "+") nil)))))) (evil-ex-range (evil-ex-line (evil-ex-current-line) (+ (evil-ex-signed-number (intern "+") (string-to-number "42"))))) (let ((l1 nil)) (save-excursion (and l1 (string= ";" ";") (goto-line l1)) (evil-ex-range l1 (evil-ex-line (evil-ex-marker "]") nil)))))"#
        ]],
    )
}

fn evil_ex_line_and_range_helpers_clamp_offsets_and_cover_the_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_ex_line_and_range_helpers_clamp_offsets_and_cover_the_buffer",
        r##"(with-temp-buffer
               (insert "one\ntwo\nthree\nfour\n")
               (goto-char 7)
               (list
                (line-number-at-pos)
                (evil-ex-first-line)
                (evil-ex-current-line)
                (evil-ex-last-line)
                (evil-ex-line 2)
                (evil-ex-line 2 1)
                (evil-ex-line 2 -8)
                (evil-ex-line 99)
                (evil-ex-range 2)
                (evil-ex-range 2 4)
                (evil-ex-full-range)))"##,
        expect![
            "OK (2 1 2 4 2 3 -6 99 (5 9 line :expanded t) (5 20 line :expanded t) (1 20 line :expanded t))"
        ],
    )
}

fn evil_ex_command_definitions_support_exact_prefix_and_completed_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_ex_command_definitions_support_exact_prefix_and_completed_bindings",
        r##"(let ((evil-ex-commands nil))
               (evil-ex-define-cmd "neotest" #'forward-char)
               (evil-ex-define-cmd "neotoggle" #'backward-char)
               (evil-ex-define-cmd "neoalias" "neotest")
               (list
                evil-ex-commands
                (evil-ex-binding "neotest")
                (evil-ex-binding "neoalias")
                (evil-ex-completed-binding "neotes")
                (evil-ex-binding "missing" t)))"##,
        expect![[
            r#"OK ((("neoalias" . "neotest") ("neotoggle" . backward-char) ("neotest" . forward-char)) forward-char forward-char forward-char nil)"#
        ]],
    )
}

fn evil_ex_completed_binding_rejects_an_ambiguous_command_prefix() -> ParityBatchCase {
    ParityBatchCase::signal(
        "evil_ex_completed_binding_rejects_an_ambiguous_command_prefix",
        r##"(let ((evil-ex-commands nil))
               (evil-ex-define-cmd "neotest" #'forward-char)
               (evil-ex-define-cmd "neotoggle" #'backward-char)
               (evil-ex-completed-binding "neot"))"##,
        expect![[r#"ERR (user-error "Unknown command: ‘neot’")"#]],
    )
}

fn evil_ex_regex_case_helpers_cover_smart_explicit_and_escaped_overrides() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_ex_regex_case_helpers_cover_smart_explicit_and_escaped_overrides",
        r##"(list
               (evil-ex-regex-without-case "cdeCDE")
               (evil-ex-regex-without-case "\\ccde\\CCDE")
               (evil-ex-regex-without-case "\\\\ccde\\\\CCDE")
               (evil-ex-regex-without-case "\\\\\\ccde\\\\\\CCDE")
               (mapcar
                (lambda (case)
                  (evil-ex-regex-case (car case) (cdr case)))
                '(("cde" . smart)
                  ("cDe" . smart)
                  ("cde" . sensitive)
                  ("cde" . insensitive)
                  ("\\ccde" . smart)
                  ("\\Ccde" . smart)
                  ("\\ccd\\Ce" . smart)
                  ("\\Ccd\\ce" . smart))))"##,
        expect![[
            r#"OK ("cdeCDE" "cdeCDE" "\\\\ccde\\\\CCDE" "\\\\cde\\\\CDE" (insensitive sensitive sensitive insensitive insensitive sensitive insensitive sensitive))"#
        ]],
    )
}

fn evil_search_moves_forward_backward_honors_case_and_wrap_settings() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_search_moves_forward_backward_honors_case_and_wrap_settings",
        r##"(with-temp-buffer
               (insert "start you YOU You you")
               (goto-char (point-min))
               (let ((evil-ex-search-case 'smart)
                     (evil-search-wrap t)
                     positions)
                 (dolist (spec
                          '(("you" t)
                            ("you" t)
                            ("You" t)
                            ("YOU" nil)
                            ("missing" t)))
                   (push
                    (condition-case err
                        (list
                         (not
                          (null
                           (apply #'evil-search spec)))
                         (point))
                      (error
                       (list 'signal
                             (car err)
                             (cdr err)
                             (point))))
                    positions))
                 (nreverse positions)))"##,
        expect![[
            r#"OK ((t 7) (t 11) (t 15) (t 11) (signal user-error ("\"missing\": string not found") 11))"#
        ]],
    )
}

fn evil_ex_execute_applies_substitute_delete_copy_move_and_sort_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_ex_execute_applies_substitute_delete_copy_move_and_sort_commands",
        r##"(mapcar
               (lambda (case)
                 (with-temp-buffer
                   (insert "delta foo\nalpha foo\ncharlie foo\nbravo foo\n")
                   (goto-char (point-min))
                   (evil-local-mode 1)
                   (evil-normal-state)
                   (evil-ex-execute case)
                   (list
                    case
                    (buffer-string)
                    (point)
                    evil-state)))
               '("%s/foo/BAR/g"
                 "2delete"
                 "1copy 4"
                 "1move 3"
                 "%sort"))"##,
        expect![[
            r#"OK (("%s/foo/BAR/g" "delta BAR\nalpha BAR\ncharlie BAR\nbravo BAR\n" 33 normal) ("2delete" "delta foo\ncharlie foo\nbravo foo\n" 11 normal) ("1copy 4" "delta foo\nalpha foo\ncharlie foo\nbravo foo\ndelta foo\n" 43 normal) ("1move 3" "alpha foo\ncharlie foo\ndelta foo\nbravo foo\n" 23 normal) ("%sort" "alpha foo\nbravo foo\ncharlie foo\ndelta foo\n" 1 normal))"#
        ]],
    )
}

pub(super) fn ex_search_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        evil_ex_parser_builds_exact_command_range_and_argument_forms(),
        evil_ex_parser_builds_exact_percent_marker_numeric_and_relative_ranges(),
        evil_ex_line_and_range_helpers_clamp_offsets_and_cover_the_buffer(),
        evil_ex_command_definitions_support_exact_prefix_and_completed_bindings(),
        evil_ex_completed_binding_rejects_an_ambiguous_command_prefix(),
        evil_ex_regex_case_helpers_cover_smart_explicit_and_escaped_overrides(),
        evil_search_moves_forward_backward_honors_case_and_wrap_settings(),
        evil_ex_execute_applies_substitute_delete_copy_move_and_sort_commands(),
    ]
}
