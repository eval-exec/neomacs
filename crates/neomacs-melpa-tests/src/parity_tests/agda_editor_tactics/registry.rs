use expect_test::expect;

use super::ParityBatchCase;

fn agda_editor_tactics_registry_metadata_and_defaults_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_registry_metadata_and_defaults_match",
        r##"(let ((descriptor
          (cadr
           (assq
            'agda-editor-tactics
            package-alist))))
         (list
         (package-desc-name descriptor)
         (package-version-join
          (package-desc-version descriptor))
         (package-desc-reqs descriptor)
         (package-desc-summary descriptor)
         (copy-tree
          (package-desc-extras descriptor))
         (featurep 'agda-editor-tactics)
         (featurep 's)
         (featurep 'dash)
         agda-editor-tactics-version
         agda-editor-tactics-format-Σ-naming
         (default-value 'agda-editor-tactics-mode)
         (local-variable-if-set-p 'agda-editor-tactics-mode)
         (get 'agda-editor-tactics-mode 'custom-type)
         (get 'agda-editor-tactics-mode 'variable-documentation)
         (boundp 'agda-editor-tactics-mode-hook)
         (and
          (boundp 'agda-editor-tactics-mode-hook)
          (copy-tree agda-editor-tactics-mode-hook))
         (boundp 'agda-editor-tactics-mode-map)
         (and
          (boundp 'agda-editor-tactics-mode-map)
          (keymapp agda-editor-tactics-mode-map))
         (and
          (memq 'agda-editor-tactics-mode minor-mode-list)
          t)
         (assq 'agda-editor-tactics-mode minor-mode-alist)
         (assq 'agda-editor-tactics-mode minor-mode-map-alist)))"##,
        expect![[
            r#"OK (agda-editor-tactics "20211024.2357" ((s (1 12 0)) (dash (2 16 0)) (emacs (27 1)) (org (9 1))) "An editor tactic to produce Σ-types from Agda records." ((:maintainers ("Musa Al-hassy" . "alhassy@gmail.com")) (:authors ("Musa Al-hassy" . "alhassy@gmail.com")) (:keywords "abbrev" "convenience" "languages" "agda" "tools") (:revdesc . "06e374516cb2") (:commit . "06e374516cb2ab17018985f3dc4fccdc4acefd08") (:url . "https://github.com/alhassy/next-700-module-systems")) t t t "20211024.2357" "%s′" nil t nil "Non-nil if Agda-Editor-Tactics mode is enabled.\nUse the command `agda-editor-tactics-mode' to change this variable." t nil nil nil t nil nil)"#
        ]],
    )
}

fn agda_editor_tactics_complete_callable_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_complete_callable_surface_matches",
        r##"(mapcar
         (lambda (symbol)
           (list symbol
                 (help-function-arglist symbol t)
                 (commandp symbol)
                 (subrp (symbol-function symbol))))
         '(agda-editor-tactics-version
           agda-editor-tactics-mode
           agda-editor-tactics-indent
           agda-editor-tactics-record-info
           agda-editor-tactics-as-Σ-nested))"##,
        expect![
            "OK ((agda-editor-tactics-version nil t nil) (agda-editor-tactics-mode (&optional arg) t nil) (agda-editor-tactics-indent (s) nil nil) (agda-editor-tactics-record-info (r) nil nil) (agda-editor-tactics-as-Σ-nested (r) nil nil))"
        ],
    )
}

fn agda_editor_tactics_version_command_emits_the_installed_version() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_version_command_emits_the_installed_version",
        r##"(let ((captured nil))
         (cl-letf (((symbol-function 'message)
                    (lambda (&rest arguments)
                      (setq captured arguments)
                      (apply #'format-message arguments))))
           (list
            (agda-editor-tactics-version)
            captured
            agda-editor-tactics-version)))"##,
        expect![[r#"OK ("20211024.2357" ("20211024.2357") "20211024.2357")"#]],
    )
}

fn agda_editor_tactics_autoload_registry_exposes_the_minor_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_autoload_registry_exposes_the_minor_mode",
        r##"(list
         (featurep 'agda-editor-tactics)
         (autoloadp (symbol-function 'agda-editor-tactics-mode))
         (copy-tree (symbol-function 'agda-editor-tactics-mode))
         (commandp 'agda-editor-tactics-mode)
         (boundp 'agda-editor-tactics-format-Σ-naming)
         (fboundp 'agda-editor-tactics-record-info)
         (assq 'agda-editor-tactics-mode minor-mode-alist)
         (assq 'agda-editor-tactics-mode minor-mode-map-alist))"##,
        expect![[
            r#"OK (nil t (autoload "agda-editor-tactics" "An Emacs editor tactic to produce Σ-types from Agda records.\n\nThis is a minor mode.  If called interactively, toggle the\n`Agda-Editor-Tactics mode' mode.  If the prefix argument is positive,\nenable the mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable `agda-editor-tactics-mode'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled.\n\n(fn &optional ARG)" t nil) t nil nil nil nil)"#
        ]],
    )
}

pub(super) fn registry_agda_editor_tactics_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agda_editor_tactics_registry_metadata_and_defaults_match(),
        agda_editor_tactics_complete_callable_surface_matches(),
        agda_editor_tactics_version_command_emits_the_installed_version(),
    ]
}

pub(super) fn registry_agda_editor_tactics_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![agda_editor_tactics_autoload_registry_exposes_the_minor_mode()]
}
