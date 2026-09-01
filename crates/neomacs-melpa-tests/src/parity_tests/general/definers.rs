use expect_test::expect;

use super::ParityBatchCase;

fn general_emacs_define_key_accepts_direct_quoted_and_multiple_keymaps() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_emacs_define_key_accepts_direct_quoted_and_multiple_keymaps",
        r##"(progn
               (defvar
                 neomacs-general-emacs-map-a
                 (make-sparse-keymap))
               (defvar
                 neomacs-general-emacs-map-b
                 (make-sparse-keymap))
               (setq
                neomacs-general-emacs-map-a
                (make-sparse-keymap)
                neomacs-general-emacs-map-b
                (make-sparse-keymap))
               (general-emacs-define-key
                   neomacs-general-emacs-map-a
                 "a" #'forward-char)
               (general-emacs-define-key
                   'neomacs-general-emacs-map-a
                 "b" #'backward-char)
               (general-emacs-define-key
                   (neomacs-general-emacs-map-a
                    neomacs-general-emacs-map-b)
                 "c" #'next-line)
               (list
                (lookup-key
                 neomacs-general-emacs-map-a
                 (kbd "a"))
                (lookup-key
                 neomacs-general-emacs-map-a
                 (kbd "b"))
                (lookup-key
                 neomacs-general-emacs-map-a
                 (kbd "c"))
                (lookup-key
                 neomacs-general-emacs-map-b
                 (kbd "c"))))"##,
        expect![[r#"OK (forward-char backward-char next-line next-line)"#]],
    )
}

fn general_def_dispatches_zero_and_one_positional_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_def_dispatches_zero_and_one_positional_arguments",
        r##"(progn
               (defvar
                 neomacs-general-def-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-def-map
                (make-sparse-keymap))
               (general-def
                 :keymaps
                 'neomacs-general-def-map
                 "a" #'forward-char)
               (general-def
                 neomacs-general-def-map
                 "b" #'backward-char)
               (list
                (lookup-key
                 neomacs-general-def-map
                 (kbd "a"))
                (lookup-key
                 neomacs-general-def-map
                 (kbd "b"))))"##,
        expect![[r#"OK (forward-char backward-char)"#]],
    )
}

fn general_defs_splits_independent_positional_and_keyword_sections() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_defs_splits_independent_positional_and_keyword_sections",
        r##"(progn
               (defvar
                 neomacs-general-defs-map-a
                 (make-sparse-keymap))
               (defvar
                 neomacs-general-defs-map-b
                 (make-sparse-keymap))
               (setq
                neomacs-general-defs-map-a
                (make-sparse-keymap)
                neomacs-general-defs-map-b
                (make-sparse-keymap))
               (general-defs
                 neomacs-general-defs-map-a
                 "a" #'forward-char
                 [?b] #'backward-char
                 neomacs-general-defs-map-b
                 "c" #'next-line
                 :keymaps
                 'neomacs-general-defs-map-a
                 "d" #'previous-line)
               (list
                (lookup-key
                 neomacs-general-defs-map-a
                 (kbd "a"))
                (lookup-key
                 neomacs-general-defs-map-a
                 (kbd "b"))
                (lookup-key
                 neomacs-general-defs-map-b
                 (kbd "c"))
                (lookup-key
                 neomacs-general-defs-map-a
                 (kbd "d"))
                (lookup-key
                 neomacs-general-defs-map-b
                 (kbd "d"))))"##,
        expect![[r#"OK (forward-char backward-char next-line previous-line nil)"#]],
    )
}

fn general_unbind_supports_nil_ignore_and_positional_keymap_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_unbind_supports_nil_ignore_and_positional_keymap_forms",
        r##"(progn
               (defvar
                 neomacs-general-unbind-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-unbind-map
                (make-sparse-keymap))
               (general-define-key
                :keymaps
                'neomacs-general-unbind-map
                "a" #'forward-char
                "b" #'backward-char
                "c" #'next-line)
               (general-unbind
                 :keymaps
                 'neomacs-general-unbind-map
                 "a" "b")
               (general-unbind
                   neomacs-general-unbind-map
                 :with #'ignore
                 "c")
               (list
                (lookup-key
                 neomacs-general-unbind-map
                 (kbd "a"))
                (lookup-key
                 neomacs-general-unbind-map
                 (kbd "b"))
                (lookup-key
                 neomacs-general-unbind-map
                 (kbd "c"))))"##,
        expect![[r#"OK (nil nil ignore)"#]],
    )
}

fn general_create_definer_applies_defaults_and_allows_local_overrides() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_create_definer_applies_defaults_and_allows_local_overrides",
        r##"(progn
               (defvar
                 neomacs-general-created-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-created-map
                (make-sparse-keymap))
               (general-create-definer
                   neomacs-general-leader
                 :keymaps
                 'neomacs-general-created-map
                 :prefix "C-c")
               (neomacs-general-leader
                 "a" #'forward-char)
               (neomacs-general-leader
                 :prefix "C-x"
                 "b" #'backward-char)
               (list
                (macrop
                 'neomacs-general-leader)
                (string-match-p
                 "neomacs-general-created-map"
                 (documentation
                  'neomacs-general-leader))
                (lookup-key
                 neomacs-general-created-map
                 (kbd "C-c a"))
                (lookup-key
                 neomacs-general-created-map
                 (kbd "C-x b"))
                (lookup-key
                 neomacs-general-created-map
                 (kbd "C-c b"))))"##,
        expect![[r#"OK (t 72 forward-char backward-char nil)"#]],
    )
}

fn general_public_definer_macros_expand_to_the_exact_primary_definer_calls() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_public_definer_macros_expand_to_the_exact_primary_definer_calls",
        r##"(list
               (macroexpand
                '(general-emacs-define-key
                     neomacs-general-map
                   "a" #'forward-char))
               (macroexpand
                '(general-evil-define-key
                     (normal visual)
                     neomacs-general-map
                   "b" #'backward-char))
               (macroexpand
                '(general-def
                   :keymaps
                   'neomacs-general-map
                   "c" #'next-line)))"##,
        expect![[
            r#"OK ((general-define-key :keymaps 'neomacs-general-map "a" #'forward-char) (general-define-key :states '(normal visual) :keymaps 'neomacs-general-map "b" #'backward-char) (general-define-key :keymaps 'neomacs-general-map "c" #'next-line))"#
        ]],
    )
}

fn general_evil_setup_creates_the_documented_long_and_short_definer_macros() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_evil_setup_creates_the_documented_long_and_short_definer_macros",
        r##"(progn
               (general-evil-setup t)
               (mapcar
                (lambda (symbol)
                  (list
                   symbol
                   (macrop symbol)
                   (and
                    (fboundp symbol)
                    t)))
                '(general-imap
                  general-emap
                  general-nmap
                  general-vmap
                  general-mmap
                  general-omap
                  general-rmap
                  general-iemap
                  general-nvmap
                  general-itomap
                  general-otomap
                  general-tomap
                  imap emap nmap vmap mmap
                  omap rmap iemap nvmap
                  itomap otomap tomap)))"##,
        expect![[
            r#"OK ((general-imap t t) (general-emap t t) (general-nmap t t) (general-vmap t t) (general-mmap t t) (general-omap t t) (general-rmap t t) (general-iemap t t) (general-nvmap t t) (general-itomap t t) (general-otomap t t) (general-tomap t t) (imap t t) (emap t t) (nmap t t) (vmap t t) (mmap t t) (omap t t) (rmap t t) (iemap t t) (nvmap t t) (itomap t t) (otomap t t) (tomap t t))"#
        ]],
    )
}

fn general_lambda_builds_an_interactive_command_and_preserves_body_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_lambda_builds_an_interactive_command_and_preserves_body_values",
        r##"(let ((command
                     (general-lambda
                       (list 'result 42))))
               (list
                (commandp command)
                (funcall command)
                (eq
                 (indirect-function
                  'general-l)
                 (indirect-function
                  'general-lambda))))"##,
        expect![[r#"OK (t (result 42) t)"#]],
    )
}

fn general_chord_encodes_ascii_and_multibyte_pairs_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_chord_encodes_ascii_and_multibyte_pairs_exactly",
        r##"(list
               (general-chord "ab")
               (general-chord "ba")
               (general-chord "λλ")
               (equal
                (general-chord "ab")
                (general-chord "ba")))"##,
        expect![[r#"OK ([key-chord 97 98] [key-chord 98 97] [key-chord 187 187] nil)"#]],
    )
}

fn general_chord_rejects_any_key_count_other_than_two() -> ParityBatchCase {
    ParityBatchCase::signal(
        "general_chord_rejects_any_key_count_other_than_two",
        r##"(general-chord "a")"##,
        expect![[r#"ERR (error "Key-chord keys must have two elements")"#]],
    )
}

pub(super) fn definers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        general_emacs_define_key_accepts_direct_quoted_and_multiple_keymaps(),
        general_def_dispatches_zero_and_one_positional_arguments(),
        general_defs_splits_independent_positional_and_keyword_sections(),
        general_unbind_supports_nil_ignore_and_positional_keymap_forms(),
        general_create_definer_applies_defaults_and_allows_local_overrides(),
        general_public_definer_macros_expand_to_the_exact_primary_definer_calls(),
        general_evil_setup_creates_the_documented_long_and_short_definer_macros(),
        general_lambda_builds_an_interactive_command_and_preserves_body_values(),
        general_chord_encodes_ascii_and_multibyte_pairs_exactly(),
        general_chord_rejects_any_key_count_other_than_two(),
    ]
}
