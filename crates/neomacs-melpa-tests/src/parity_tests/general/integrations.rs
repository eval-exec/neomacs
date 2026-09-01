use expect_test::expect;

use super::ParityBatchCase;

fn general_use_package_keyword_accepts_primary_positional_and_repeated_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_use_package_keyword_accepts_primary_positional_and_repeated_forms",
        r##"(progn
               (require 'use-package)
               (defvar
                 neomacs-general-use-package-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-use-package-map
                (make-sparse-keymap))
               (use-package
                   neomacs-general-no-library
                 :ensure nil
                 :general
                 (:keymaps
                  'neomacs-general-use-package-map
                  "a" #'forward-char)
                 :general
                 ("b" #'backward-char
                  :keymaps
                  'neomacs-general-use-package-map)
                 :general
                 (neomacs-general-use-package-map
                  "c" #'next-line))
               (list
                (lookup-key
                 neomacs-general-use-package-map
                 (kbd "a"))
                (lookup-key
                 neomacs-general-use-package-map
                 (kbd "b"))
                (lookup-key
                 neomacs-general-use-package-map
                 (kbd "c"))))"##,
        expect![[r#"OK (forward-char backward-char next-line)"#]],
    )
}

fn general_ghook_keyword_infers_mode_functions_and_supports_explicit_lists() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_ghook_keyword_infers_mode_functions_and_supports_explicit_lists",
        r##"(progn
               (require 'use-package)
               (defvar
                 neomacs-general-hook-a nil)
               (defvar
                 neomacs-general-hook-b nil)
               (defvar
                 neomacs-general-hooks
                 '(neomacs-general-hook-a
                   neomacs-general-hook-b))
               (setq
                neomacs-general-hook-a nil
                neomacs-general-hook-b nil
                neomacs-general-hooks
                '(neomacs-general-hook-a
                  neomacs-general-hook-b))
               (use-package
                   neomacs-general-fake
                 :ghook
                 neomacs-general-hooks)
               (use-package
                   neomacs-general-explicit
                 :ghook
                 ('neomacs-general-hook-a
                  '(forward-char
                    backward-char)
                  t))
               (list
                neomacs-general-hook-a
                neomacs-general-hook-b))"##,
        expect![[
            r#"OK ((neomacs-general-fake-mode forward-char backward-char) (neomacs-general-fake-mode))"#
        ]],
    )
}

fn general_gfhook_keyword_infers_hook_names_and_preserves_append_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_gfhook_keyword_infers_hook_names_and_preserves_append_order",
        r##"(progn
               (require 'use-package)
               (defvar
                 neomacs-general-fake-mode-hook nil)
               (defvar
                 neomacs-general-other-hook nil)
               (defvar
                 neomacs-general-functions
                 '(forward-char
                   backward-char))
               (setq
                neomacs-general-fake-mode-hook nil
                neomacs-general-other-hook nil
                neomacs-general-functions
                '(forward-char
                  backward-char))
               (use-package
                   neomacs-general-fake
                 :defer t
                 :gfhook
                 neomacs-general-functions
                 (nil #'next-line t)
                 ('neomacs-general-other-hook
                  '(previous-line
                    beginning-of-line)))
               (list
                neomacs-general-fake-mode-hook
                neomacs-general-other-hook))"##,
        expect![[
            r#"OK ((backward-char forward-char next-line) (beginning-of-line previous-line))"#
        ]],
    )
}

pub(super) fn integrations_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        general_use_package_keyword_accepts_primary_positional_and_repeated_forms(),
        general_ghook_keyword_infers_mode_functions_and_supports_explicit_lists(),
        general_gfhook_keyword_infers_hook_names_and_preserves_append_order(),
    ]
}
