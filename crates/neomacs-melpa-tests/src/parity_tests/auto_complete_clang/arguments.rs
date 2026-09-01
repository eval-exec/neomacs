use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_prefix_header_setter_clears_whitespace_and_preserves_nonblank_input()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_prefix_header_setter_clears_whitespace_and_preserves_nonblank_input",
        r##"(let ((ac-clang-prefix-header
                "old.pch"))
         (list
          (progn
            (ac-clang-set-prefix-header "")
            ac-clang-prefix-header)
          (progn
            (ac-clang-set-prefix-header
             " \t ")
            ac-clang-prefix-header)
          (progn
            (ac-clang-set-prefix-header
             "prefix.pch")
            ac-clang-prefix-header)
          (progn
            (ac-clang-set-prefix-header
             "  named.pch  ")
            ac-clang-prefix-header)))"##,
        expect![[r#"OK (nil nil "prefix.pch" "  named.pch  ")"#]],
    )
}

fn auto_complete_clang_cflags_command_splits_interactive_input_using_emacs_word_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_cflags_command_splits_interactive_input_using_emacs_word_rules",
        r##"(let ((ac-clang-flags
                '("-DOLD"))
               (prompts nil))
         (cl-letf
             (((symbol-function
                'read-string)
               (lambda (prompt
                        &rest _arguments)
                 (push prompt prompts)
                 "  -Iinclude  -DNAME=hello\\ world\t-std=c++20  ")))
           (call-interactively
            #'ac-clang-set-cflags)
           (list ac-clang-flags
                 (nreverse prompts))))"##,
        expect![[r#"OK (("-Iinclude" "-DNAME=hello\\" "world" "-std=c++20") ("New cflags: "))"#]],
    )
}

fn auto_complete_clang_shell_cflags_command_passes_current_file_default_and_splits_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_shell_cflags_command_passes_current_file_default_and_splits_output",
        r##"(let ((buffer-file-name
                (expand-file-name
                 "src/main.cpp"
                 default-directory))
               (ac-clang-flags nil)
               (reads nil)
               (commands nil))
         (cl-letf
             (((symbol-function
                'read-shell-command)
               (lambda (prompt
                        initial-input
                        history
                        default-value)
                 (push
                  (list prompt initial-input
                        history default-value)
                  reads)
                 "pkg-config --cflags demo"))
              ((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push command commands)
                 "-I/opt/demo\n-DDEMO=1  -pthread\n")))
           (call-interactively
            #'ac-clang-set-cflags-from-shell-command)
           (list
            ac-clang-flags
            (nreverse reads)
            (nreverse commands))))"##,
        expect![[
            r#"OK (("-I/opt/demo" "-DDEMO=1" "-pthread") (("Shell command: " nil nil "src/main.cpp")) ("pkg-config --cflags demo"))"#
        ]],
    )
}

fn auto_complete_clang_build_location_tracks_stdin_or_saved_file_line_and_column() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_build_location_tracks_stdin_or_saved_file_line_and_column",
        r##"(with-temp-buffer
         (insert "one\n  alpha beta\nlast")
         (setq buffer-file-name
               (expand-file-name
                "source.cpp"
                default-directory))
         (list
          (let ((ac-clang-auto-save nil))
            (mapcar
             #'ac-clang-build-location
             '(1 5 7 15 24)))
          (let ((ac-clang-auto-save t))
            (mapcar
             #'ac-clang-build-location
             '(1 5 7 15 24)))))"##,
        expect![[
            r#"OK (("-:1:1" "-:2:1" "-:2:3" "-:2:11" "-:3:5") ("[ORACLE-SANDBOX]/source.cpp:1:1" "[ORACLE-SANDBOX]/source.cpp:2:1" "[ORACLE-SANDBOX]/source.cpp:2:3" "[ORACLE-SANDBOX]/source.cpp:2:11" "[ORACLE-SANDBOX]/source.cpp:3:5"))"#
        ]],
    )
}

fn auto_complete_clang_language_option_covers_c_cpp_objc_extensions_and_fallback() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_language_option_covers_c_cpp_objc_extensions_and_fallback",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (setq major-mode (car case))
             (setq buffer-file-name
                   (cadr case))
             (let ((ac-clang-lang-option-function
                    nil))
               (list
                (car case)
                (cadr case)
                (ac-clang-test-error
                 #'ac-clang-lang-option)))))
         '((c-mode "/work/main.c")
           (c++-mode "/work/main.cc")
           (objc-mode "/work/object.m")
           (objc-mode "/work/object.mm")
           (objc-mode nil)
           (rust-mode "/work/main.rs")
           (fundamental-mode nil)))"##,
        expect![[
            r#"OK ((c-mode "/work/main.c" (:value "c")) (c++-mode "/work/main.cc" (:value "c++")) (objc-mode "/work/object.m" (:value "objective-c")) (objc-mode "/work/object.mm" (:value "objective-c++")) (objc-mode nil (:signal wrong-type-argument (stringp nil))) (rust-mode "/work/main.rs" (:value "c++")) (fundamental-mode nil (:value "c++")))"#
        ]],
    )
}

fn auto_complete_clang_custom_language_option_has_priority_and_is_called_each_time()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_custom_language_option_has_priority_and_is_called_each_time",
        r##"(let* ((calls 0)
               (major-mode 'c-mode)
               (ac-clang-lang-option-function
                (lambda ()
                  (setq calls (1+ calls))
                  (if (= calls 1)
                      "cuda"
                    nil))))
         (list
          (ac-clang-lang-option)
          (ac-clang-lang-option)
          calls))"##,
        expect![[r#"OK ("cuda" "c" 2)"#]],
    )
}

fn auto_complete_clang_unsaved_complete_args_include_language_flags_prefix_header_location_and_stdin()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_unsaved_complete_args_include_language_flags_prefix_header_location_and_stdin",
        r##"(with-temp-buffer
         (let ((default-directory
                 (file-name-as-directory
                  (expand-file-name
                   "clang-args"
                   default-directory)))
               (major-mode 'c++-mode)
               (ac-clang-auto-save nil)
               (ac-clang-flags
                '("-Iinclude"
                  "-DNAME=two words"))
               (ac-clang-prefix-header
                "precompiled.pch"))
           (insert "int main() {\n  ret")
           (ac-clang-build-complete-args
            (point))))"##,
        expect![[
            r#"OK ("-cc1" "-fsyntax-only" "-x" "c++" "-Iinclude" "-DNAME=two words" "-include-pch" "[ORACLE-SANDBOX]/clang-args/precompiled.pch" "-code-completion-at" "-:2:6" "-")"#
        ]],
    )
}

fn auto_complete_clang_saved_complete_args_omit_language_and_use_file_for_location_and_input()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_saved_complete_args_omit_language_and_use_file_for_location_and_input",
        r##"(with-temp-buffer
         (let ((buffer-file-name
                 (expand-file-name
                  "saved.c"
                  default-directory))
               (major-mode 'c-mode)
               (ac-clang-auto-save t)
               (ac-clang-flags
                '("-Wall"))
               (ac-clang-prefix-header nil))
           (insert "int value;\nval")
           (ac-clang-build-complete-args
            (- (point) 3))))"##,
        expect![[
            r#"OK ("-cc1" "-fsyntax-only" "-Wall" "-code-completion-at" "[ORACLE-SANDBOX]/saved.c:2:1" "[ORACLE-SANDBOX]/saved.c")"#
        ]],
    )
}

fn auto_complete_clang_document_cleanup_removes_placeholders_and_normalizes_optional_markers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_document_cleanup_removes_placeholders_and_normalizes_optional_markers",
        r##"(mapcar
         #'ac-clang-clean-document
         '(nil
           ""
           "int fn(<#int x#>, <#const char *name#>)"
           "void log([#int level#], <#const char *fmt#>, ...)"
           "[#only#]"
           "nested <#std::vector<int>#> [#tail#]"))"##,
        expect![[
            r#"OK (nil "" "int fn(int x, const char *name)" "void log(int level , const char *fmt, ...)" "only " "nested std::vector<int> tail ")"#
        ]],
    )
}

fn auto_complete_clang_document_reads_help_property_only_from_string_candidates() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_document_reads_help_property_only_from_string_candidates",
        r##"(let ((candidate
                (propertize
                 "function"
                 'ac-clang-help
                 "int function(<#int value#>)")))
         (list
          (ac-clang-document candidate)
          (ac-clang-document
           (substring-no-properties
            candidate))
          (ac-clang-document nil)
          (ac-clang-document
           '(not a string))))"##,
        expect![[r#"OK ("int function(int value)" nil nil nil)"#]],
    )
}

fn auto_complete_clang_balance_counter_and_argument_splitter_handle_nested_types() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_balance_counter_and_argument_splitter_handle_nested_types",
        r##"(list
         (mapcar
          (lambda (string)
            (list
             string
             (ac-clang-same-count-in-string
              ?\( ?\) string)
             (ac-clang-same-count-in-string
              ?\< ?\> string)))
          '("" "(x)" "(x" "std::vector<int>"
            "map<string, vector<int>>"
            "fn(a, pair<int, int>)"))
         (mapcar
          #'ac-clang-split-args
          '("int a, char b"
            "std::pair<int, int> value, double scale"
            "void (*callback)(int, char), int flags"
            "map<string, vector<pair<int, int>>> data, bool ok"
            ""
            "single")))"##,
        expect![[
            r#"OK ((("" t t) ("(x)" t t) ("(x" nil t) ("std::vector<int>" t t) ("map<string, vector<int>>" t t) ("fn(a, pair<int, int>)" t t)) (("int a" "char b") ("std::pair<int, int> value" "double scale") ("void (*callback)(int, char)" "int flags") ("map<string, vector<pair<int, int>>> data" "bool ok") ("") ("single")))"#
        ]],
    )
}

pub(super) fn arguments_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_prefix_header_setter_clears_whitespace_and_preserves_nonblank_input(),
        auto_complete_clang_cflags_command_splits_interactive_input_using_emacs_word_rules(),
        auto_complete_clang_shell_cflags_command_passes_current_file_default_and_splits_output(),
        auto_complete_clang_build_location_tracks_stdin_or_saved_file_line_and_column(),
        auto_complete_clang_language_option_covers_c_cpp_objc_extensions_and_fallback(),
        auto_complete_clang_custom_language_option_has_priority_and_is_called_each_time(),
        auto_complete_clang_unsaved_complete_args_include_language_flags_prefix_header_location_and_stdin(),
        auto_complete_clang_saved_complete_args_omit_language_and_use_file_for_location_and_input(),
        auto_complete_clang_document_cleanup_removes_placeholders_and_normalizes_optional_markers(),
        auto_complete_clang_document_reads_help_property_only_from_string_candidates(),
        auto_complete_clang_balance_counter_and_argument_splitter_handle_nested_types(),
    ]
}
