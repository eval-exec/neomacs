use expect_test::expect;

use super::ParityBatchCase;

fn which_key_keymap_based_replacements_cover_cons_string_and_created_prefixes() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_keymap_based_replacements_cover_cons_string_and_created_prefixes",
        r##"(let ((map (make-sparse-keymap))
                    (prefix-map (make-sparse-keymap)))
               (define-key prefix-map "x" #'ignore)
               (define-key map "\C-a" 'complete)
               (define-key map "\C-b" prefix-map)
               (which-key-add-keymap-based-replacements map
                 "C-a" '("mycomplete" . complete)
                 "C-b" "mymap"
                 "C-c" "mymap2")
               (define-key map "\C-ca" 'foo)
               (list
                (which-key--get-keymap-bindings map)
                (lookup-key map (kbd "C-a"))
                (keymapp (lookup-key map (kbd "C-b")))
                (keymapp (lookup-key map (kbd "C-c")))))"##,
        expect![[
            r#"OK ((("C-a" . "mycomplete") ("C-b" . "group:mymap") ("C-c" . "group:mymap2")) complete t t)"#
        ]],
    )
}

fn which_key_named_prefix_commands_retain_their_symbolic_description() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_named_prefix_commands_retain_their_symbolic_description",
        r##"(progn
               (define-prefix-command 'neomacs-which-key-named-map)
               (let ((map (make-sparse-keymap)))
                 (define-key map "\C-a" 'neomacs-which-key-named-map)
                 (list
                  (which-key--get-keymap-bindings map)
                  (keymapp neomacs-which-key-named-map)
                  (commandp 'neomacs-which-key-named-map))))"##,
        expect![[r#"OK ((("C-a" . "neomacs-which-key-named-map")) t nil)"#]],
    )
}

fn which_key_global_and_major_mode_prefix_declarations_are_applied_separately() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_global_and_major_mode_prefix_declarations_are_applied_separately",
        r##"(let* ((major-mode 'neomacs-which-key-test-mode)
                    which-key-replacement-alist
                    which-key--prefix-title-alist)
               (which-key-add-key-based-replacements
                 "SPC C-c" '("complete" . "complete title")
                 "SPC C-k" "cancel")
               (which-key-add-major-mode-key-based-replacements
                   'neomacs-which-key-test-mode
                 "C-c C-c" '("complete" . "complete title")
                 "C-c C-k" "cancel")
               (list
                (which-key--maybe-replace '("SPC C-k" . ""))
                (which-key--maybe-replace '("C-c C-c" . ""))
                (which-key--maybe-get-prefix-title "SPC C-c")
                (which-key--maybe-get-prefix-title "C-c C-c")
                which-key-replacement-alist
                which-key--prefix-title-alist))"##,
        expect![[
            r#"OK (("SPC C-k" . "cancel") ("C-c C-c" . "complete") "complete title" "complete" ((neomacs-which-key-test-mode (("\\`C-c C-k\\'") nil . "cancel") (("\\`C-c C-c\\'") nil . "complete")) (("\\`SPC C-k\\'") nil . "cancel") (("\\`SPC C-c\\'") nil . "complete")) ((neomacs-which-key-test-mode ("C-c C-c" . "complete title")) ("SPC C-c" . "complete title")))"#
        ]],
    )
}

fn which_key_replacement_matching_handles_regex_quoting_lambdas_and_bad_regex_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_replacement_matching_handles_regex_quoting_lambdas_and_bad_regex_text",
        r##"(let ((which-key-replacement-alist
                     '((("C-c [a-d]" . nil) . ("C-c a" . "c-c a"))
                       (("C-c .+" . nil) . ("C-c *" . "c-c *"))))
                    (test-mode-1 t)
                    (test-mode-2 nil)
                    which-key-allow-multiple-replacements)
               (which-key-add-key-based-replacements
                 "C-c ." "test ."
                 "SPC ." "SPC ."
                 "C-c \\" "regexp quoting"
                 "C-c [" "bad regexp"
                 "SPC t1" (lambda (kb)
                            (cons (car kb)
                                  (if test-mode-1
                                      "[x] test mode"
                                    "[ ] test mode")))
                 "SPC t2" (lambda (kb)
                            (cons (car kb)
                                  (if test-mode-2
                                      "[x] test mode"
                                    "[ ] test mode"))))
               (mapcar
                #'which-key--maybe-replace
                '(("C-c g" . "test")
                  ("C-c b" . "test")
                  ("C-c ." . "not test .")
                  ("C-c +" . "not test .")
                  ("C-c [" . "orig bad regexp")
                  ("C-c \\" . "pre quoting")
                  ("SPC . ." . "don't replace")
                  ("SPC t 1" . "test mode")
                  ("SPC t 2" . "test mode"))))"##,
        expect![[
            r#"OK (("C-c *" . "c-c *") ("C-c a" . "c-c a") ("C-c ." . "test .") ("C-c *" . "c-c *") ("C-c [" . "bad regexp") ("C-c \\" . "regexp quoting") ("SPC . ." . "don't replace") ("SPC t 1" . "[x] test mode") ("SPC t 2" . "[ ] test mode"))"#
        ]],
    )
}

fn which_key_multiple_replacements_chain_in_declaration_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_multiple_replacements_chain_in_declaration_order",
        r##"(let ((which-key-replacement-alist
                     '(((nil . "helm") . (nil . "HLM"))
                       ((nil . "projectile") . (nil . "PRJTL"))))
                    (which-key-allow-multiple-replacements t))
               (mapcar
                #'which-key--maybe-replace
                '(("C-c C-c" . "helm-x")
                  ("C-c C-c" . "projectile-x")
                  ("C-c C-c" . "helm-projectile-x")
                  ("C-c C-c" . "unrelated"))))"##,
        expect![[
            r#"OK (("C-c C-c" . "HLM-x") ("C-c C-c" . "PRJTL-x") ("C-c C-c" . "HLM-PRJTL-x") ("C-c C-c" . "unrelated"))"#
        ]],
    )
}

fn which_key_nil_replacement_suppresses_matching_bindings_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_nil_replacement_suppresses_matching_bindings_only",
        r##"(let ((which-key-replacement-alist
                     '(((nil . "winum-select-window-[1-9]") . t))))
               (list
                (which-key--maybe-replace
                 '("C-c C-c" . "winum-select-window-1"))
                (which-key--maybe-replace
                 '("C-c C-c" . "winum-select-window-0"))
                (which-key--maybe-replace
                 '("C-c C-c" . "other-command"))))"##,
        expect![[
            r#"OK (nil ("C-c C-c" . "winum-select-window-0") ("C-c C-c" . "other-command"))"#
        ]],
    )
}

fn which_key_extract_key_preserves_ranges_and_returns_the_final_key() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_extract_key_preserves_ranges_and_returns_the_final_key",
        r##"(mapcar
               #'which-key--extract-key
               '("SPC a"
                 "C-x a"
                 "<left> b a"
                 "<left> a .. c"
                 "M-a a .. c"
                 ""
                 "C-x <f12>"))"##,
        expect![[r#"OK ("a" "a" "a" "a .. c" "a .. c" "" "<f12>")"#]],
    )
}

fn which_key_keymap_replacement_rejects_non_string_non_cons_values() -> ParityBatchCase {
    ParityBatchCase::signal(
        "which_key_keymap_replacement_rejects_non_string_non_cons_values",
        r##"(which-key-add-keymap-based-replacements
               (make-sparse-keymap)
               "C-a"
               42)"##,
        expect![[r#"ERR (user-error "Replacement is neither a cons cell or a string")"#]],
    )
}

fn which_key_major_mode_replacement_rejects_non_symbol_modes() -> ParityBatchCase {
    ParityBatchCase::signal(
        "which_key_major_mode_replacement_rejects_non_symbol_modes",
        r##"(which-key-add-major-mode-key-based-replacements
               "not-a-mode"
               "C-a"
               "alpha")"##,
        expect![[
            r#"ERR (error "‘\"not-a-mode\"’ should be a symbol corresponding to a value of major-mode")"#
        ]],
    )
}

pub(super) fn replacements_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        which_key_keymap_based_replacements_cover_cons_string_and_created_prefixes(),
        which_key_named_prefix_commands_retain_their_symbolic_description(),
        which_key_global_and_major_mode_prefix_declarations_are_applied_separately(),
        which_key_replacement_matching_handles_regex_quoting_lambdas_and_bad_regex_text(),
        which_key_multiple_replacements_chain_in_declaration_order(),
        which_key_nil_replacement_suppresses_matching_bindings_only(),
        which_key_extract_key_preserves_ranges_and_returns_the_final_key(),
        which_key_keymap_replacement_rejects_non_string_non_cons_values(),
        which_key_major_mode_replacement_rejects_non_symbol_modes(),
    ]
}
