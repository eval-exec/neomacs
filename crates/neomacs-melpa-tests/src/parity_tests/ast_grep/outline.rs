use expect_test::expect;

use super::ParityBatchCase;

fn ast_grep_outline_title_mapping_covers_canonical_and_unknown_symbol_types() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_outline_title_mapping_covers_canonical_and_unknown_symbol_types",
        r##"(list
          ast-grep--outline-type-titles
          (mapcar
           #'ast-grep--outline-group-title
           '("class" "interface" "function" "method"
             "constant" "macro" "event-handler" "" nil)))"##,
        expect![[
            r#"OK ((("class" . "Classes") ("interface" . "Interfaces") ("struct" . "Structs") ("enum" . "Enums") ("trait" . "Traits") ("object" . "Objects") ("module" . "Modules") ("namespace" . "Namespaces") ("function" . "Functions") ("method" . "Methods") ("constructor" . "Constructors") ("field" . "Fields") ("property" . "Properties") ("constant" . "Constants") ("variable" . "Variables") ("type" . "Types") ("macro" . "Macros")) ("Classes" "Interfaces" "Functions" "Methods" "Constants" "Macros" "Event-Handler" "" "Other"))"#
        ]],
    )
}

fn ast_grep_outline_command_and_real_process_use_expanded_file_and_outline_label() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_outline_command_and_real_process_use_expanded_file_and_outline_label",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "outline/src/demo.ts"
                 "function run() {}\n"))
               (log (ast-grep-test-path "outline-argv.log"))
               (program
                (ast-grep-test-make-executable
                 "sg-outline"
                 (format
                  "printf '%%s\\n' \"$@\" > %s\nprintf '%%s\\n' '{\"items\":[]}'"
                  (shell-quote-argument log))))
               (ast-grep-executable program))
          (list
           (mapcar
            (lambda (part)
              (if (equal part file) "$FILE" part))
            (ast-grep--build-outline-command file))
           (ast-grep--run-outline file)
           (replace-regexp-in-string
            (regexp-quote file) "$FILE"
            (ast-grep-test-read-file log))))"##,
        expect![[
            r#"OK (("[ORACLE-SANDBOX]/bin/sg-outline" "outline" "--json=stream" "$FILE") "{\"items\":[]}\n" "outline\n--json=stream\n$FILE\n")"#
        ]],
    )
}

fn ast_grep_outline_parse_flattens_multi_file_stream_and_skips_malformed_lines() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_outline_parse_flattens_multi_file_stream_and_skips_malformed_lines",
        r##"(let ((output
               (concat
                "{\"file\":\"a.ts\",\"items\":[{\"name\":\"A\",\"symbolType\":\"class\",\"range\":{\"start\":{\"line\":0,\"column\":0}},\"members\":[]}]}\n"
                "malformed\n"
                "{\"file\":\"b.ts\",\"items\":[{\"name\":\"run\",\"symbolType\":\"function\",\"range\":{\"start\":{\"line\":3,\"column\":2}},\"members\":[]},{\"name\":\"value\",\"symbolType\":\"variable\",\"range\":{\"start\":{\"line\":5,\"column\":1}},\"members\":[]}]}\n"
                "{\"file\":\"empty.ts\",\"items\":[]}\n")))
          (list
           (ast-grep--outline-parse output)
           (ast-grep--outline-parse "")
           (ast-grep--outline-parse nil)))"##,
        expect![[
            r#"OK (((:name "A" :symbolType "class" :range (:start (:line 0 :column 0)) :members nil) (:name "run" :symbolType "function" :range (:start (:line 3 :column 2)) :members nil) (:name "value" :symbolType "variable" :range (:start (:line 5 :column 1)) :members nil)) nil nil)"#
        ]],
    )
}

fn ast_grep_outline_flatten_builds_qualified_nested_names_at_character_positions() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_outline_flatten_builds_qualified_nested_names_at_character_positions",
        r##"(with-temp-buffer
          (insert
           "class Widget {\n"
           "\tmethod render() {\n"
           "\t\tfunction inner() {}\n"
           "\t}\n"
           "}\n"
           "const standalone = 1;\n")
          (let ((items
                 '((:name "Widget" :symbolType "class"
                    :range (:start (:line 0 :column 6))
                    :members
                    ((:name "render" :symbolType "method"
                      :range (:start (:line 1 :column 8))
                      :members
                      ((:name "inner" :symbolType "function"
                        :range (:start (:line 2 :column 11))
                        :members nil))))
                   (:name "standalone" :symbolType "constant"
                    :range (:start (:line 5 :column 6))
                    :members nil)
                   (:name nil :symbolType "variable"
                    :range (:start (:line 5 :column 0))
                    :members nil)
                   (:name "missing-position" :symbolType "field"
                    :range nil :members nil)))))
            (mapcar
             (lambda (entry)
               (list
                (nth 0 entry)
                (nth 1 entry)
                (nth 2 entry)
                (char-after (nth 2 entry))))
             (ast-grep--outline-flatten items nil))))"##,
        expect![[
            r#"OK (("class" "Widget" 7 87) ("method" "Widget.render" 24 114) ("function" "Widget.render.inner" 46 105))"#
        ]],
    )
}

fn ast_grep_outline_group_orders_types_and_deduplicates_reachable_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_outline_group_orders_types_and_deduplicates_reachable_names",
        r##"(let ((entries
               '(("variable" "item" 80)
                 ("function" "run" 10)
                 ("method" "Widget.run" 30)
                 ("function" "run" 20)
                 ("class" "Widget" 1)
                 ("event-handler" "on-click" 90)
                 ("function" "run" 25)
                 ("class" "Gadget" 50))))
          (list
           (ast-grep--outline-dedupe-names
            '(("same" . 1) ("other" . 2)
              ("same" . 3) ("same" . 4)))
           (ast-grep--outline-group entries)))"##,
        expect![[
            r#"OK ((("same" . 1) ("other" . 2) ("same<2>" . 3) ("same<3>" . 4)) (("Classes" ("Widget" . 1) ("Gadget" . 50)) ("Functions" ("run" . 10) ("run<2>" . 20) ("run<3>" . 25)) ("Methods" ("Widget.run" . 30)) ("Variables" ("item" . 80)) ("Event-Handler" ("on-click" . 90))))"#
        ]],
    )
}

fn ast_grep_outline_imenu_index_runs_real_stub_cli_and_returns_jumpable_groups() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_outline_imenu_index_runs_real_stub_cli_and_returns_jumpable_groups",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "outline/real.ts"
                 "class Widget {}\nfunction run() {}\n"))
               (program
                (ast-grep-test-make-executable
                 "sg-outline-index"
                 "printf '%s\\n' '{\"items\":[{\"name\":\"Widget\",\"symbolType\":\"class\",\"range\":{\"start\":{\"line\":0,\"column\":6}},\"members\":[]},{\"name\":\"run\",\"symbolType\":\"function\",\"range\":{\"start\":{\"line\":1,\"column\":9}},\"members\":[]}]}'"))
               (ast-grep-executable program))
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (let ((index (ast-grep--outline-imenu-index)))
                  (list
                   index
                   (mapcar
                    (lambda (group)
                      (mapcar
                       (lambda (leaf)
                         (list
                          (car leaf)
                          (cdr leaf)
                          (char-after (cdr leaf))))
                       (cdr group)))
                    index))))
            (ast-grep-test-kill-file-buffer file)))"##,
        expect![[
            r#"OK ((("Classes" ("Widget" . 7)) ("Functions" ("run" . 26))) ((("Widget" 7 87)) (("run" 26 114))))"#
        ]],
    )
}

fn ast_grep_outline_imenu_index_degrades_cli_failure_to_message_and_empty_index() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_outline_imenu_index_degrades_cli_failure_to_message_and_empty_index",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "outline/failure.ts"
                 "function nope() {}\n"))
               messages)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (cl-letf (((symbol-function 'ast-grep--run-outline)
                           (lambda (_file)
                             (error "unsupported outline version")))
                          ((symbol-function 'message)
                           (lambda (format-string &rest args)
                             (push
                              (apply #'format format-string args)
                              messages))))
                  (list
                   (ast-grep--outline-imenu-index)
                   (nreverse messages))))
            (ast-grep-test-kill-file-buffer file)))"##,
        expect![[r#"OK (nil ("ast-grep outline failed: unsupported outline version"))"#]],
    )
}

fn ast_grep_outline_mode_restores_prior_imenu_function_and_invalidates_all_caches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_outline_mode_restores_prior_imenu_function_and_invalidates_all_caches",
        r##"(with-temp-buffer
          (setq-local imenu-create-index-function #'ignore)
          (setq-local imenu--index-alist '((old . 1)))
          (setq-local consult-imenu--cache 'consult-old)
          (setq-local helm-cached-imenu-alist 'helm-alist)
          (setq-local helm-cached-imenu-candidates 'helm-candidates)
          (setq-local helm-cached-imenu-tick 42)
          (let ((before
                 (list
                  imenu-create-index-function
                  imenu--index-alist
                  consult-imenu--cache
                  helm-cached-imenu-alist
                  helm-cached-imenu-candidates
                  helm-cached-imenu-tick
                  ast-grep--outline-saved-imenu-function)))
            (ast-grep-outline-mode 1)
            (let ((enabled
                   (list
                    ast-grep-outline-mode
                    imenu-create-index-function
                    imenu--index-alist
                    (local-variable-p 'consult-imenu--cache)
                    (local-variable-p 'helm-cached-imenu-alist)
                    (local-variable-p 'helm-cached-imenu-candidates)
                    (local-variable-p 'helm-cached-imenu-tick)
                    ast-grep--outline-saved-imenu-function)))
              ;; Re-entrant enable must not overwrite the genuine saved fn.
              (ast-grep-outline-mode 1)
              (ast-grep-outline-mode -1)
              (list
               before
               enabled
               (list
                ast-grep-outline-mode
                imenu-create-index-function
                ast-grep--outline-saved-imenu-function
                imenu--index-alist
                (local-variable-p 'consult-imenu--cache)
                (local-variable-p 'helm-cached-imenu-alist))))))"##,
        expect![
            "OK ((ignore ((old . 1)) consult-old helm-alist helm-candidates 42 unset) (t ast-grep--outline-imenu-index nil nil nil nil nil ignore) (nil ignore unset nil nil nil))"
        ],
    )
}

fn ast_grep_outline_mode_without_prior_local_function_restores_global_binding() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_outline_mode_without_prior_local_function_restores_global_binding",
        r##"(with-temp-buffer
          (kill-local-variable 'imenu-create-index-function)
          (let ((global (default-value 'imenu-create-index-function)))
            (ast-grep-outline-mode 1)
            (let ((during
                   (list
                    (local-variable-p 'imenu-create-index-function)
                    imenu-create-index-function
                    ast-grep--outline-saved-imenu-function)))
              (ast-grep-outline-mode -1)
              (list
               during
               (local-variable-p 'imenu-create-index-function)
               (eq imenu-create-index-function global)
               ast-grep--outline-saved-imenu-function))))"##,
        expect!["OK ((t ast-grep--outline-imenu-index unset) nil t unset)"],
    )
}

fn ast_grep_outline_picker_dispatch_respects_active_ivy_helm_consult_priority() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_outline_picker_dispatch_respects_active_ivy_helm_consult_priority",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "outline/picker.el"
                 "(defun sample () t)\n"))
               (program
                (ast-grep-test-make-executable
                 "sg-present"
                 "exit 0"))
               (ast-grep-executable program)
               calls)
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (cl-letf (((symbol-function 'require)
                           (lambda (feature &optional _filename _noerror)
                             (memq feature
                                   '(counsel helm-imenu consult-imenu))))
                          ((symbol-function 'counsel-imenu)
                           (lambda ()
                             (interactive)
                             (push 'counsel calls)))
                          ((symbol-function 'helm-imenu)
                           (lambda ()
                             (interactive)
                             (push 'helm calls)))
                          ((symbol-function 'consult-imenu)
                           (lambda ()
                             (interactive)
                             (push 'consult calls)))
                          ((symbol-function 'imenu)
                           (lambda ()
                             (interactive)
                             (push 'builtin calls))))
                  (let ((ivy-mode t) (helm-mode nil))
                    (ast-grep-outline))
                  (let ((ivy-mode nil) (helm-mode t))
                    (ast-grep-outline))
                  (let ((ivy-mode nil) (helm-mode nil))
                    (ast-grep-outline))
                  (nreverse calls)))
            (ast-grep-test-kill-file-buffer file)))"##,
        expect!["OK (counsel helm consult)"],
    )
}

fn ast_grep_outline_one_shot_restores_origin_state_when_picker_switches_buffers() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_outline_one_shot_restores_origin_state_when_picker_switches_buffers",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "outline/origin.el"
                 "(defun origin () t)\n"))
               (other (get-buffer-create "*ast-grep-other*"))
               (program
                (ast-grep-test-make-executable
                 "sg-present"
                 "exit 0"))
               (ast-grep-executable program))
          (unwind-protect
              (with-current-buffer (find-file-noselect file)
                (setq-local imenu-create-index-function #'ignore)
                (setq-local imenu--index-alist '((saved . 17)))
                (setq-local consult-imenu--cache 'saved-consult)
                (cl-letf (((symbol-function 'require)
                           (lambda (&rest _) nil))
                          ((symbol-function 'imenu)
                           (lambda ()
                             (interactive)
                             (set-buffer other)
                             :switched)))
                  (ast-grep-outline)
                  (with-current-buffer (find-buffer-visiting file)
                    (list
                     imenu-create-index-function
                     imenu--index-alist
                     consult-imenu--cache
                     (local-variable-p 'consult-imenu--cache)
                     (buffer-name (current-buffer))))))
            (ast-grep-test-kill-file-buffer file)
            (when (buffer-live-p other)
              (kill-buffer other))))"##,
        expect![[r#"OK (ignore ((saved . 17)) saved-consult t "origin.el")"#]],
    )
}

fn ast_grep_outline_command_rejects_missing_executable_and_non_file_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_outline_command_rejects_missing_executable_and_non_file_buffers",
        r##"(list
          (let ((ast-grep-executable "definitely-no-such-sg"))
            (ast-grep-test-error-data #'ast-grep-outline))
          (let* ((program
                  (ast-grep-test-make-executable
                   "sg-present"
                   "exit 0"))
                 (ast-grep-executable program))
            (with-temp-buffer
              (ast-grep-test-error-data #'ast-grep-outline))))"##,
        expect![[
            r#"OK ((:error error ("The ast-grep executable not found. Please install ast-grep")) (:error user-error ("Current buffer is not visiting a file")))"#
        ]],
    )
}

pub(super) fn outline_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ast_grep_outline_title_mapping_covers_canonical_and_unknown_symbol_types(),
        ast_grep_outline_command_and_real_process_use_expanded_file_and_outline_label(),
        ast_grep_outline_parse_flattens_multi_file_stream_and_skips_malformed_lines(),
        ast_grep_outline_flatten_builds_qualified_nested_names_at_character_positions(),
        ast_grep_outline_group_orders_types_and_deduplicates_reachable_names(),
        ast_grep_outline_imenu_index_runs_real_stub_cli_and_returns_jumpable_groups(),
        ast_grep_outline_imenu_index_degrades_cli_failure_to_message_and_empty_index(),
        ast_grep_outline_mode_restores_prior_imenu_function_and_invalidates_all_caches(),
        ast_grep_outline_mode_without_prior_local_function_restores_global_binding(),
        ast_grep_outline_picker_dispatch_respects_active_ivy_helm_consult_priority(),
        ast_grep_outline_one_shot_restores_origin_state_when_picker_switches_buffers(),
        ast_grep_outline_command_rejects_missing_executable_and_non_file_buffers(),
    ]
}
