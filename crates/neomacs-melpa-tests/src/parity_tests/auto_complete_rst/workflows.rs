use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_rst_real_rst_mode_hook_installs_completion_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_real_rst_mode_hook_installs_completion_environment",
        r##"(let
                             ((ac-modes '(text-mode))
                              (rst-mode-hook nil))
                           (cl-letf
                               (((symbol-function
                                  'auto-complete-rst-genesource-eval)
                                 (lambda ()
                                   (with-temp-buffer
                                     (auto-complete-rst-test-insert-generated-source)
                                     (eval-buffer)))))
                             (auto-complete-rst-init)
                             (with-temp-buffer
                               (insert
                                "Title\n=====\n\n"
                                ".. note:: Practical warning\n")
                               (rst-mode)
                               (list
                                major-mode
                                (memq 'rst-mode ac-modes)
                                ac-sources
                                (key-binding (kbd ":"))
                                (key-binding (kbd "SPC"))
                                (auto-complete-rst-directives-candidates)
                                (auto-complete-rst-roles-candidates)))))"##,
        expect![[
            r####"OK (rst-mode (rst-mode text-mode) (ac-source-rst-options ac-source-rst-roles ac-source-rst-directives ac-source-words-in-same-mode-buffers) auto-complete-rst-complete-colon auto-complete-rst-complete-space ("note::" "code-block::" "image::" "py:function::") ("ref:" "doc:" "py:class:" "emphasis:"))"####
        ]],
    )
}

fn auto_complete_rst_practical_directive_completion_selects_generated_code_block() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_rst_practical_directive_completion_selects_generated_code_block",
        r##"(let
                             ((auto-complete-rst-directive-options-map
                               (make-hash-table :test 'equal)))
                           (with-temp-buffer
                             (auto-complete-rst-test-insert-generated-source)
                             (eval-buffer))
                           (with-temp-buffer
                             (rst-mode)
                             (insert
                              "Example\n=======\n\n"
                              "The implementation follows.\n\n"
                              ".. code-bl")
                             (goto-char (point-max))
                             (let*
                                 ((regexp
                                   (cdr
                                    (assq
                                     'prefix
                                     ac-source-rst-directives)))
                                  (matched
                                   (and
                                    (string-match
                                     regexp
                                     (buffer-string))
                                    (match-string
                                     1
                                     (buffer-string))))
                                  (candidates
                                   (auto-complete-rst-directives-candidates))
                                  (selected
                                   (seq-filter
                                    (lambda (candidate)
                                      (string-prefix-p
                                       matched
                                       candidate))
                                    candidates)))
                               (delete-region
                                (- (point) (length matched))
                                (point))
                               (insert (car selected))
                               (list
                                matched
                                candidates
                                selected
                                (buffer-string)
                                (point)))))"##,
        expect![[
            r####"OK ("code-bl" ("note::" "code-block::" "image::" "py:function::") ("code-block::") "Example\n=======\n\nThe implementation follows.\n\n.. code-block::" 62)"####
        ]],
    )
}

fn auto_complete_rst_practical_role_completion_inserts_target_delimiters() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_practical_role_completion_inserts_target_delimiters",
        r##"(let
                             ((auto-complete-rst-directive-options-map
                               (make-hash-table :test 'equal)))
                           (with-temp-buffer
                             (auto-complete-rst-test-insert-generated-source)
                             (eval-buffer))
                           (with-temp-buffer
                             (rst-mode)
                             (insert
                              "See :py:cla")
                             (goto-char (point-max))
                             (let*
                                 ((regexp
                                   (cdr
                                    (assq
                                     'prefix
                                     ac-source-rst-roles)))
                                  (matched
                                   (and
                                    (string-match
                                     regexp
                                     (buffer-string))
                                    (match-string
                                     1
                                     (buffer-string))))
                                  (selected
                                   (seq-find
                                    (lambda (candidate)
                                      (string-prefix-p
                                       matched
                                       candidate))
                                    (auto-complete-rst-roles-candidates))))
                               (delete-region
                                (- (point) (length matched))
                                (point))
                               (insert selected)
                               (auto-complete-rst-insert-two-backquotes)
                               (insert
                                "collections.OrderedDict")
                               (list
                                matched
                                selected
                                (buffer-string)
                                (point)
                                (char-after)))))"##,
        expect![[
            r####"OK ("py:cla" "py:class:" "See :py:class:`collections.OrderedDict`" 39 96)"####
        ]],
    )
}

fn auto_complete_rst_practical_option_completion_uses_enclosing_directive_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_practical_option_completion_uses_enclosing_directive_map",
        r##"(let
                             ((auto-complete-rst-directive-options-map
                               (make-hash-table :test 'equal)))
                           (puthash
                            "image"
                            '("alt:" "height:" "width:")
                            auto-complete-rst-directive-options-map)
                           (with-temp-buffer
                             (rst-mode)
                             (insert
                              "Architecture\n============\n\n"
                              ".. image:: diagram.svg\n"
                              "    :hei")
                             (goto-char (point-max))
                             (let*
                                 ((directive
                                   (auto-complete-rst-directive-name-at-option))
                                  (candidates
                                   (auto-complete-rst-options-candidates))
                                  (prefix "hei")
                                  (selected
                                   (seq-find
                                    (lambda (candidate)
                                      (string-prefix-p
                                       prefix
                                       candidate))
                                    candidates)))
                               (delete-region
                                (- (point) (length prefix))
                                (point))
                               (insert selected " 320px")
                               (list
                                directive
                                candidates
                                selected
                                (buffer-string)
                                (point)))))"##,
        expect![[
            r####"OK ("image" ("alt:" "height:" "width:") "height:" "Architecture\n============\n\n.. image:: diagram.svg\n    :height: 320px" 69)"####
        ]],
    )
}

fn auto_complete_rst_bound_keys_drive_real_editing_commands_and_source_routing() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_rst_bound_keys_drive_real_editing_commands_and_source_routing",
        r##"(let
                             ((auto-complete-rst-other-sources
                               '(ac-source-filename))
                              calls)
                           (cl-letf
                               (((symbol-function 'auto-complete)
                                 (lambda (sources)
                                   (push sources calls)
                                   :shown)))
                             (with-temp-buffer
                               (rst-mode)
                               (auto-complete-rst-add-sources)
                               (setq auto-complete-mode t)
                               (insert "..")
                               (call-interactively
                                (key-binding (kbd "SPC")))
                               (insert "note")
                               (call-interactively
                                (key-binding (kbd ":")))
                               (insert "\n    ")
                               (call-interactively
                                (key-binding (kbd ":")))
                               (list
                                (buffer-string)
                                (nreverse calls)
                                ac-sources
                                (point)))))"##,
        expect![[
            r####"OK (".. note:\n    :" ((ac-source-rst-directives) #1=(ac-source-rst-directives ac-source-rst-options ac-source-rst-roles) #1#) (ac-source-rst-options ac-source-rst-roles ac-source-rst-directives ac-source-filename) 15)"####
        ]],
    )
}

fn auto_complete_rst_buffer_local_setup_does_not_leak_sources_or_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_rst_buffer_local_setup_does_not_leak_sources_or_keys",
        r##"(let
                             ((auto-complete-rst-other-sources
                               '(ac-source-filename))
                              first)
                           (with-temp-buffer
                             (rst-mode)
                             (auto-complete-rst-add-sources)
                             (setq
                              first
                              (list
                               ac-sources
                               (key-binding (kbd ":"))
                               (key-binding (kbd "SPC"))
                               (local-variable-p
                                'ac-sources))))
                           (with-temp-buffer
                             (fundamental-mode)
                             (list
                              first
                              ac-sources
                              (key-binding (kbd ":"))
                              (key-binding (kbd "SPC"))
                              (local-variable-p
                               'ac-sources))))"##,
        expect![[
            r####"OK (((ac-source-rst-options ac-source-rst-roles ac-source-rst-directives ac-source-filename) auto-complete-rst-complete-colon auto-complete-rst-complete-space t) (ac-source-words-in-same-mode-buffers) self-insert-command self-insert-command nil)"####
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_rst_real_rst_mode_hook_installs_completion_environment(),
        auto_complete_rst_practical_directive_completion_selects_generated_code_block(),
        auto_complete_rst_practical_role_completion_inserts_target_delimiters(),
        auto_complete_rst_practical_option_completion_uses_enclosing_directive_map(),
        auto_complete_rst_bound_keys_drive_real_editing_commands_and_source_routing(),
        auto_complete_rst_buffer_local_setup_does_not_leak_sources_or_keys(),
    ]
}
