use expect_test::expect;

use super::ParityBatchCase;

fn package_candidates_are_normalized_across_markers_and_real_world_version_shapes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "package_candidates_are_normalized_across_markers_and_real_world_version_shapes",
        r##"(mapcar
               (lambda (candidate)
                 (cons
                  candidate
                  (all-the-icons-ivy-rich-package-name
                   candidate)))
               '("dash"
                 "+dash-2.19.1"
                 "-ivy-rich-0.1.0"
                 "all-the-icons-5.0.0.1"
                 "package-with-digits-2fa-1.20"
                 "unversioned-package"))"##,
        expect![[
            r#"OK (("dash" . "dash") ("+dash-2.19.1" . "dash") ("-ivy-rich-0.1.0" . "ivy-rich") ("all-the-icons-5.0.0.1" . "all-the-icons") ("package-with-digits-2fa-1.20" . "package-with-digits-2fa") ("unversioned-package" . "unversioned-package"))"#
        ]],
    )
}

fn documentation_truncation_extracts_only_the_first_line_and_enforces_eighty_columns()
-> ParityBatchCase {
    ParityBatchCase::value(
        "documentation_truncation_extracts_only_the_first_line_and_enforces_eighty_columns",
        r##"(list
               (all-the-icons-ivy-rich--truncate-docstring nil)
               (all-the-icons-ivy-rich--truncate-docstring "")
               (all-the-icons-ivy-rich--truncate-docstring
                "first line\nsecond line")
               (all-the-icons-ivy-rich--truncate-docstring
                (concat
                 (make-string 75 ?a)
                 "界界界界界界"
                 "\nignored"))
               (length
                (all-the-icons-ivy-rich--truncate-docstring
                 (make-string 100 ?x))))"##,
        expect![[
            r#"OK ("" "" "first line" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa界界" 80)"#
        ]],
    )
}

fn function_argument_annotations_cover_commands_lambdas_macros_subrs_and_unknown_symbols()
-> ParityBatchCase {
    ParityBatchCase::value(
        "function_argument_annotations_cover_commands_lambdas_macros_subrs_and_unknown_symbols",
        r##"(progn
               (defun all-the-icons-ivy-rich-fixture-command
                   (path &optional force &rest switches)
                 "Fixture command."
                 (interactive "fPath: ")
                 (list path force switches))
               (defmacro all-the-icons-ivy-rich-fixture-macro
                   (binding &rest body)
                 "Fixture macro."
                 `(let ((,binding t)) ,@body))
               (mapcar
                (lambda (candidate)
                  (cons
                   candidate
                   (all-the-icons-ivy-rich-function-args
                    candidate)))
                '("all-the-icons-ivy-rich-fixture-command"
                  "all-the-icons-ivy-rich-fixture-macro"
                  "mapcar"
                  "if"
                  "all-the-icons-ivy-rich-not-defined")))"##,
        expect![[
            r#"OK (("all-the-icons-ivy-rich-fixture-command" . "(PATH &optional FORCE &rest SWITCHES)") ("all-the-icons-ivy-rich-fixture-macro" . "(BINDING &rest BODY)") ("mapcar" . "(FUNCTION SEQUENCE)") ("if" . "(COND THEN ELSE...)") ("all-the-icons-ivy-rich-not-defined" . ""))"#
        ]],
    )
}

fn variable_annotations_render_practical_scalar_collection_and_opaque_runtime_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "variable_annotations_render_practical_scalar_collection_and_opaque_runtime_values",
        r##"(let ((symbols
                    '(all-the-icons-ivy-rich-value-unbound
                      all-the-icons-ivy-rich-value-nil
                      all-the-icons-ivy-rich-value-true
                      all-the-icons-ivy-rich-value-number
                      all-the-icons-ivy-rich-value-symbol
                      all-the-icons-ivy-rich-value-string
                      all-the-icons-ivy-rich-value-list
                      all-the-icons-ivy-rich-value-keymap
                      all-the-icons-ivy-rich-value-bool-vector
                      all-the-icons-ivy-rich-value-hash-table
                      all-the-icons-ivy-rich-value-syntax-table
                      all-the-icons-ivy-rich-value-char-table
                      all-the-icons-ivy-rich-value-function)))
               (makunbound 'all-the-icons-ivy-rich-value-unbound)
               (set 'all-the-icons-ivy-rich-value-nil nil)
               (set 'all-the-icons-ivy-rich-value-true t)
               (set 'all-the-icons-ivy-rich-value-number 42.5)
               (set 'all-the-icons-ivy-rich-value-symbol 'ready)
               (set 'all-the-icons-ivy-rich-value-string
                    "alpha\nbeta")
               (set 'all-the-icons-ivy-rich-value-list
                    '(alpha (beta . gamma) 3))
               (set 'all-the-icons-ivy-rich-value-keymap
                    (make-sparse-keymap))
               (set 'all-the-icons-ivy-rich-value-bool-vector
                    (make-bool-vector 4 t))
               (set 'all-the-icons-ivy-rich-value-hash-table
                    (make-hash-table :test 'equal))
               (set 'all-the-icons-ivy-rich-value-syntax-table
                    (make-syntax-table))
               (set 'all-the-icons-ivy-rich-value-char-table
                    (make-char-table nil))
               (set 'all-the-icons-ivy-rich-value-function
                    'forward-char)
               (mapcar
                (lambda (symbol)
                  (list
                   symbol
                   (all-the-icons-ivy-rich-variable-value
                    (symbol-name symbol))))
                symbols))"##,
        expect![[
            r##"OK ((all-the-icons-ivy-rich-value-unbound #("#<unbound>" 0 10 (face all-the-icons-ivy-rich-null-face))) (all-the-icons-ivy-rich-value-nil #("nil" 0 3 (face all-the-icons-ivy-rich-null-face))) (all-the-icons-ivy-rich-value-true #("t" 0 1 (face all-the-icons-ivy-rich-true-face))) (all-the-icons-ivy-rich-value-number #("42.5" 0 4 (face all-the-icons-ivy-rich-number-face))) (all-the-icons-ivy-rich-value-symbol #("ready" 0 5 (face all-the-icons-ivy-rich-symbol-face))) (all-the-icons-ivy-rich-value-string #("\"alpha\\nbeta\"" 0 13 (face all-the-icons-ivy-rich-string-face))) (all-the-icons-ivy-rich-value-list #("(alpha (beta . gamma) 3)" 0 24 (face all-the-icons-ivy-rich-list-face))) (all-the-icons-ivy-rich-value-keymap #("#<keymap>" 0 9 (face all-the-icons-ivy-rich-value-face))) (all-the-icons-ivy-rich-value-bool-vector #("#<bool-vector>" 0 14 (face all-the-icons-ivy-rich-value-face))) (all-the-icons-ivy-rich-value-hash-table #("#<hash-table>" 0 13 (face all-the-icons-ivy-rich-value-face))) (all-the-icons-ivy-rich-value-syntax-table #("#<syntax-table>" 0 15 (face all-the-icons-ivy-rich-value-face))) (all-the-icons-ivy-rich-value-char-table #("#<char-table>" 0 13 (face all-the-icons-ivy-rich-value-face))) (all-the-icons-ivy-rich-value-function #("#'forward-char" 0 14 (face all-the-icons-ivy-rich-function-face))))"##
        ]],
    )
}

fn variable_annotation_print_limits_and_escaping_match_interactive_describe_usage()
-> ParityBatchCase {
    ParityBatchCase::value(
        "variable_annotation_print_limits_and_escaping_match_interactive_describe_usage",
        r##"(let ((all-the-icons-ivy-rich-field-width 3))
               (set 'all-the-icons-ivy-rich-long-string
                    (propertize "ab\n界cd" 'fixture t))
               (set 'all-the-icons-ivy-rich-long-list
                    '(one two three four five))
               (set 'all-the-icons-ivy-rich-control-string
                    (string ?a 1 ?b))
               (list
                (all-the-icons-ivy-rich-variable-value
                 "all-the-icons-ivy-rich-long-string")
                (all-the-icons-ivy-rich-variable-value
                 "all-the-icons-ivy-rich-long-list")
                (all-the-icons-ivy-rich-variable-value
                 "all-the-icons-ivy-rich-control-string")))"##,
        expect![[
            r#"OK (#("\"ab\\n\"" 0 6 (face all-the-icons-ivy-rich-string-face)) #("(one two three ...)" 0 19 (face all-the-icons-ivy-rich-list-face)) #("\"a\\1b\"" 0 6 (face all-the-icons-ivy-rich-string-face)))"#
        ]],
    )
}

fn symbol_classes_combine_command_macro_special_advice_custom_local_obsolete_and_face_traits()
-> ParityBatchCase {
    ParityBatchCase::value(
        "symbol_classes_combine_command_macro_special_advice_custom_local_obsolete_and_face_traits",
        r##"(progn
               (require 'cl-lib)
               (defun all-the-icons-ivy-rich-class-command ()
                 "Command fixture."
                 (interactive))
               (defun all-the-icons-ivy-rich-class-function ()
                 "Function fixture.")
               (defmacro all-the-icons-ivy-rich-class-macro
                   (&rest body)
                 "Macro fixture."
                 `(progn ,@body))
               (defun all-the-icons-ivy-rich-class-advice
                   (function &rest arguments)
                 (apply function arguments))
               (advice-add
                'all-the-icons-ivy-rich-class-function
                :around
                #'all-the-icons-ivy-rich-class-advice)
               (defcustom all-the-icons-ivy-rich-class-custom 1
                 "Custom fixture."
                 :type 'integer)
               (setq all-the-icons-ivy-rich-class-custom 2)
               (defvar all-the-icons-ivy-rich-class-obsolete 1)
               (make-obsolete-variable
                'all-the-icons-ivy-rich-class-obsolete
                'all-the-icons-ivy-rich-class-custom
                "1.0")
               (defface all-the-icons-ivy-rich-class-face
                 '((t :inherit default))
                 "Face fixture.")
               (with-temp-buffer
                 (setq-local
                  all-the-icons-ivy-rich-class-local
                  'local)
                 (mapcar
                  (lambda (candidate)
                    (cons
                     candidate
                     (string-trim-right
                      (all-the-icons-ivy-rich-symbol-class
                       (symbol-name candidate)))))
                  '(all-the-icons-ivy-rich-class-command
                    all-the-icons-ivy-rich-class-function
                    all-the-icons-ivy-rich-class-macro
                    if
                    all-the-icons-ivy-rich-class-custom
                    all-the-icons-ivy-rich-class-local
                    all-the-icons-ivy-rich-class-obsolete
                    all-the-icons-ivy-rich-class-face))))"##,
        expect![[
            r#"OK ((all-the-icons-ivy-rich-class-command . "c") (all-the-icons-ivy-rich-class-function . "f!") (all-the-icons-ivy-rich-class-macro . "m") (if . "M") (all-the-icons-ivy-rich-class-custom . "U") (all-the-icons-ivy-rich-class-local . "lv") (all-the-icons-ivy-rich-class-obsolete . "v-") (all-the-icons-ivy-rich-class-face . "a"))"#
        ]],
    )
}

fn symbol_documentation_routes_functions_variables_faces_and_unknowns_to_their_real_sources()
-> ParityBatchCase {
    ParityBatchCase::value(
        "symbol_documentation_routes_functions_variables_faces_and_unknowns_to_their_real_sources",
        r##"(progn
               (defun all-the-icons-ivy-rich-doc-function
                   (argument)
                 "Function first line.
Function second line."
                 argument)
               (defcustom all-the-icons-ivy-rich-doc-variable nil
                 "Variable first line.
Variable second line."
                 :type 'boolean)
               (defface all-the-icons-ivy-rich-doc-face
                 '((t :inherit default))
                 "Face first line.
Face second line.")
               (mapcar
                (lambda (candidate)
                  (cons
                   candidate
                   (all-the-icons-ivy-rich-symbol-docstring
                    candidate)))
                '("all-the-icons-ivy-rich-doc-function"
                  "all-the-icons-ivy-rich-doc-variable"
                  "all-the-icons-ivy-rich-doc-face"
                  ":keyword"
                  "all-the-icons-ivy-rich-doc-missing")))"##,
        expect![[
            r#"OK (("all-the-icons-ivy-rich-doc-function" . "Function first line.") ("all-the-icons-ivy-rich-doc-variable" . "Variable first line.") ("all-the-icons-ivy-rich-doc-face" . "Face used for documentation string.") (":keyword" . "") ("all-the-icons-ivy-rich-doc-missing" . ""))"#
        ]],
    )
}

fn imenu_annotations_parse_grouped_candidates_and_follow_the_current_major_mode() -> ParityBatchCase
{
    ParityBatchCase::value(
        "imenu_annotations_parse_grouped_candidates_and_follow_the_current_major_mode",
        r##"(progn
               (defun all-the-icons-ivy-rich-imenu-fixture
                   (value)
                 "Fixture shown by an Imenu annotation."
                 value)
               (let ((candidate
                      "Functions: all-the-icons-ivy-rich-imenu-fixture"))
                 (list
                  (all-the-icons-ivy-rich--counsel-imenu-symbol
                   candidate)
                  (with-temp-buffer
                    (emacs-lisp-mode)
                    (list
                     (all-the-icons-ivy-rich-imenu-class
                      candidate)
                     (all-the-icons-ivy-rich-imenu-docstring
                      candidate)))
                  (with-temp-buffer
                    (fundamental-mode)
                    (list
                     (all-the-icons-ivy-rich-imenu-class
                      candidate)
                     (all-the-icons-ivy-rich-imenu-docstring
                      candidate))))))"##,
        expect![[
            r#"OK ("all-the-icons-ivy-rich-imenu-fixture" ("f" "Fixture shown by an Imenu annotation.") ("" ""))"#
        ]],
    )
}

fn custom_charset_coding_and_input_method_annotations_use_real_emacs_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_charset_coding_and_input_method_annotations_use_real_emacs_metadata",
        r##"(let ((input-method-alist
                    (cons
                     '("neomacs-fixture"
                       "Fixture"
                       nil
                       nil
                       "Fixture input method documentation.")
                     input-method-alist)))
               (defgroup all-the-icons-ivy-rich-custom-group nil
                 "Fixture custom group documentation."
                 :group 'applications)
               (defcustom all-the-icons-ivy-rich-custom-option t
                 "Fixture custom option documentation."
                 :type 'boolean
                 :group 'all-the-icons-ivy-rich-custom-group)
               (list
                (all-the-icons-ivy-rich-custom-group-docstring
                 "all-the-icons-ivy-rich-custom-group")
                (all-the-icons-ivy-rich-custom-variable-docstring
                 "all-the-icons-ivy-rich-custom-option")
                (all-the-icons-ivy-rich-charset-docstring "ascii")
                (all-the-icons-ivy-rich-coding-system-docstring
                 "utf-8")
                (all-the-icons-ivy-rich-input-method-docstring
                 "neomacs-fixture")))"##,
        expect![[
            r#"OK ("Fixture custom group documentation." "Fixture custom option documentation." "ASCII (ISO646 IRV)" "UTF-8 (no signature (BOM))" "Fixture input method documentation.")"#
        ]],
    )
}

fn keybinding_annotations_extract_the_command_after_the_fixed_descbinds_prefix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "keybinding_annotations_extract_the_command_after_the_fixed_descbinds_prefix",
        r##"(list
               (all-the-icons-ivy-rich-keybinding-docstring
                "C-x C-f             find-file")
               (all-the-icons-ivy-rich-keybinding-docstring
                "C-x 8 RET           insert-char")
               (all-the-icons-ivy-rich-keybinding-docstring
                "fixture ignore")
               (all-the-icons-ivy-rich-keybinding-docstring
                "too-short        backward-char"))"##,
        expect![[
            r#"OK ("Edit file FILENAME." "Insert COUNT copies of CHARACTER." "" "Move point N characters backward (forward if N is negative).")"#
        ]],
    )
}

fn grep_and_magit_todo_transformers_preserve_payloads_while_annotating_location_fields()
-> ParityBatchCase {
    ParityBatchCase::value(
        "grep_and_magit_todo_transformers_preserve_payloads_while_annotating_location_fields",
        r##"(mapcar
               (lambda (candidate)
                 (let ((grep
                        (all-the-icons-ivy-rich-grep-transformer
                         candidate))
                       (todo
                        (all-the-icons-ivy-rich-magit-todos-transformer
                         candidate)))
                   (list
                    candidate
                    grep
                    todo)))
               '("src/main.rs:42:TODO handle edge:case"
                 "src/main.rs:error(permission denied)"
                 "README.md TODO improve installation"
                 "single"))"##,
        expect![[
            r#"OK (("src/main.rs:42:TODO handle edge:case" #("src/main.rs:42:TODO handle edge:case" 0 11 (face ivy-grep-info) 12 14 (face ivy-grep-info)) #("src/main.rs:42:TODO handle edge:case" 0 19 (face ivy-grep-info))) ("src/main.rs:error(permission denied)" #("src/main.rs:error(permission denied)" 0 11 (face ivy-grep-info) 18 35 (face error)) #("src/main.rs:error(permission denied)" 0 28 (face ivy-grep-info))) ("README.md TODO improve installation" "README.md TODO improve installation" #("README.md TODO improve installation" 0 9 (face ivy-grep-info))) ("single" "single" #("single " 0 6 (face ivy-grep-info))))"#
        ]],
    )
}

fn bookmark_annotations_use_real_bookmark_records_for_name_path_and_compact_context()
-> ParityBatchCase {
    ParityBatchCase::value(
        "bookmark_annotations_use_real_bookmark_records_for_name_path_and_compact_context",
        r##"(let ((bookmark-alist
                    '(("fixture"
                       (filename . "/workspace/notes.txt")
                       (front-context-string
                        . " alpha\n   beta\tgamma "))
                      ("handler"
                       (handler . ignore)))))
               (list
                (all-the-icons-ivy-rich-bookmark-name "fixture")
                (all-the-icons-ivy-rich-bookmark-filename
                 "fixture")
                (all-the-icons-ivy-rich-bookmark-context
                 "fixture")
                (all-the-icons-ivy-rich-bookmark-filename
                 "handler")
                (all-the-icons-ivy-rich-bookmark-context
                 "handler")))"##,
        expect![[r#"OK ("fixture" "/workspace/notes.txt" "alpha\\n beta gamma…" "" "")"#]],
    )
}

fn installed_package_annotations_report_real_version_archive_summary_and_status() -> ParityBatchCase
{
    ParityBatchCase::value(
        "installed_package_annotations_report_real_version_archive_summary_and_status",
        r##"(let
               ((package-load-list '(all))
                (package-selected-packages
                 '(all-the-icons-ivy-rich)))
               (mapcar
                (lambda (candidate)
                  (list
                   candidate
                   (all-the-icons-ivy-rich-package-name
                    candidate)
                   (all-the-icons-ivy-rich-package-version
                    candidate)
                   (all-the-icons-ivy-rich-package-archive-summary
                    candidate)
                   (all-the-icons-ivy-rich-package-install-summary
                    candidate)
                   (all-the-icons-ivy-rich-package-status
                    candidate)))
                '("all-the-icons-ivy-rich-20230420.1234"
                  "ivy-rich-20230425.1422"
                  "not-a-real-package-9.9")))"##,
        expect![[
            r#"OK (("all-the-icons-ivy-rich-20230420.1234" "all-the-icons-ivy-rich" "" "" "" #("installed" 0 9 (face all-the-icons-ivy-rich-package-status-installed-face))) ("ivy-rich-20230425.1422" "ivy-rich" "" "" "" #("dependency" 0 10 (face all-the-icons-ivy-rich-package-status-installed-face))) ("not-a-real-package-9.9" "not-a-real-package" "" "" "" #("orphan" 0 6 (face all-the-icons-ivy-rich-error-face))))"#
        ]],
    )
}

fn library_buffer_and_kill_annotations_follow_live_editor_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "library_buffer_and_kill_annotations_follow_live_editor_state",
        r##"(let* ((buffer
                     (generate-new-buffer
                      " *all-the-icons-ivy-rich-fixture*"))
                    (name (buffer-name buffer))
                    loaded
                    unloaded
                    mode
                    killed)
               (unwind-protect
                   (progn
                     (setq loaded
                           (all-the-icons-ivy-rich-library-transformer
                            "all-the-icons-ivy-rich")
                           unloaded
                           (all-the-icons-ivy-rich-library-transformer
                            "all-the-icons-ivy-rich-not-loaded"))
                     (with-current-buffer buffer
                       (emacs-lisp-mode))
                     (setq mode
                           (all-the-icons-ivy-rich-switch-buffer-major-mode
                            name))
                     (all-the-icons-ivy-rich-kill-buffer
                      #'kill-buffer
                      name)
                     (setq killed
                           (not (get-buffer name)))
                     (list
                      (list
                       loaded
                       (get-text-property 0 'face loaded))
                      (list
                       unloaded
                       (get-text-property 0 'face unloaded))
                      mode
                      killed))
                 (when (buffer-live-p buffer)
                   (kill-buffer buffer))))"##,
        expect![[
            r#"OK (("all-the-icons-ivy-rich" nil) (#("all-the-icons-ivy-rich-not-loaded" 0 33 (face all-the-icons-ivy-rich-off-face)) all-the-icons-ivy-rich-off-face) "" t)"#
        ]],
    )
}

pub(super) fn annotations_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        package_candidates_are_normalized_across_markers_and_real_world_version_shapes(),
        documentation_truncation_extracts_only_the_first_line_and_enforces_eighty_columns(),
        function_argument_annotations_cover_commands_lambdas_macros_subrs_and_unknown_symbols(),
        variable_annotations_render_practical_scalar_collection_and_opaque_runtime_values(),
        variable_annotation_print_limits_and_escaping_match_interactive_describe_usage(),
        symbol_classes_combine_command_macro_special_advice_custom_local_obsolete_and_face_traits(),
        symbol_documentation_routes_functions_variables_faces_and_unknowns_to_their_real_sources(),
        imenu_annotations_parse_grouped_candidates_and_follow_the_current_major_mode(),
        custom_charset_coding_and_input_method_annotations_use_real_emacs_metadata(),
        keybinding_annotations_extract_the_command_after_the_fixed_descbinds_prefix(),
        grep_and_magit_todo_transformers_preserve_payloads_while_annotating_location_fields(),
        bookmark_annotations_use_real_bookmark_records_for_name_path_and_compact_context(),
        installed_package_annotations_report_real_version_archive_summary_and_status(),
        library_buffer_and_kill_annotations_follow_live_editor_state(),
    ]
}
