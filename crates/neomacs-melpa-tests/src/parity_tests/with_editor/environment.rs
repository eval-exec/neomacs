use expect_test::expect;

use super::ParityBatchCase;

fn with_editor_macro_scopes_editor_to_sleeping_fallback_and_restores_environment() -> ParityBatchCase
{
    ParityBatchCase::value(
        "with_editor_macro_scopes_editor_to_sleeping_fallback_and_restores_environment",
        r##"(let ((process-environment
                    (cons "EDITOR=original"
                          (copy-sequence process-environment)))
                   (with-editor-emacsclient-executable nil))
               (let ((inside
                      (with-editor
                        (list
                         (getenv "EDITOR")
                         (getenv "ALTERNATE_EDITOR")
                         with-editor--envvar))))
                 (list inside
                       (getenv "EDITOR")
                       (getenv "ALTERNATE_EDITOR")
                       with-editor--envvar)))"##,
        expect![[
            r#"OK (("sh -c 'printf \"\\nWITH-EDITOR: $$ OPEN $0\\037$1\\037 IN $(pwd)\\n\"; sleep 604800 & sleep=$!; trap \"kill $sleep; exit 0\" USR1; trap \"kill $sleep; exit 1\" USR2; wait $sleep'" nil "EDITOR") "original" nil nil)"#
        ]],
    )
}

fn with_editor_literal_and_dynamic_macros_set_only_requested_environment_variable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "with_editor_literal_and_dynamic_macros_set_only_requested_environment_variable",
        r##"(let ((process-environment
                    (copy-sequence process-environment))
                   (with-editor-emacsclient-executable nil)
                   (name "HG_EDITOR"))
               (setenv "EDITOR" "outer-editor")
               (setenv "GIT_EDITOR" "outer-git")
               (setenv "HG_EDITOR" "outer-hg")
               (list
                (with-editor "GIT_EDITOR"
                  (list (getenv "EDITOR")
                        (getenv "GIT_EDITOR")
                        with-editor--envvar))
                (with-editor* name
                  (list (getenv "EDITOR")
                        (getenv "HG_EDITOR")
                        with-editor--envvar))
                (list (getenv "EDITOR")
                      (getenv "GIT_EDITOR")
                      (getenv "HG_EDITOR"))))"##,
        expect![[
            r#"OK (("outer-editor" "sh -c 'printf \"\\nWITH-EDITOR: $$ OPEN $0\\037$1\\037 IN $(pwd)\\n\"; sleep 604800 & sleep=$!; trap \"kill $sleep; exit 0\" USR1; trap \"kill $sleep; exit 1\" USR2; wait $sleep'" "GIT_EDITOR") ("outer-editor" "sh -c 'printf \"\\nWITH-EDITOR: $$ OPEN $0\\037$1\\037 IN $(pwd)\\n\"; sleep 604800 & sleep=$!; trap \"kill $sleep; exit 0\" USR1; trap \"kill $sleep; exit 1\" USR2; wait $sleep'" "HG_EDITOR") ("outer-editor" "outer-git" "outer-hg"))"#
        ]],
    )
}

fn with_editor_server_window_uses_first_matching_rule_then_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "with_editor_server_window_uses_first_matching_rule_then_fallback",
        r##"(let ((with-editor-server-window-alist
                    '(("\\.git/" . git-window)
                      ("COMMIT_EDITMSG\\'" . commit-window)))
                   (server-window 'fallback-window))
               (with-temp-buffer
                 (setq buffer-file-name
                       "/repo/.git/COMMIT_EDITMSG")
                 (let ((first (with-editor-server-window)))
                   (setq buffer-file-name "/repo/notes.txt")
                   (list first
                         (with-editor-server-window)))))"##,
        expect![[r#"OK (git-window fallback-window)"#]],
    )
}

fn with_editor_export_editor_rejects_unsupported_major_mode() -> ParityBatchCase {
    ParityBatchCase::signal(
        "with_editor_export_editor_rejects_unsupported_major_mode",
        r##"(with-temp-buffer
               (fundamental-mode)
               (with-editor-export-editor "EDITOR"))"##,
        expect![[r#"ERR (error "Cannot export environment variables in this buffer")"#]],
    )
}

pub(super) fn environment_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        with_editor_macro_scopes_editor_to_sleeping_fallback_and_restores_environment(),
        with_editor_literal_and_dynamic_macros_set_only_requested_environment_variable(),
        with_editor_server_window_uses_first_matching_rule_then_fallback(),
        with_editor_export_editor_rejects_unsupported_major_mode(),
    ]
}
