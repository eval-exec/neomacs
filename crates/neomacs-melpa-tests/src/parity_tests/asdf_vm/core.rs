use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_current_parses_realistic_multi_tool_output_and_reports_interactive_table()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_current_parses_realistic_multi_tool_output_and_reports_interactive_table",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       (concat
                        "ruby 3.3.1 /work/.tool-versions\n"
                        "nodejs 20.11.0 /work/.tool-versions\n"
                        "python ______ No version is set\n"))))
                 (list
                  (asdf-vm-current)
                  (asdf-vm-current
                   "ruby" 1)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ((("nodejs" "20.11.0" "/work/.tool-versions") ("python" "______" "No version is set")) (("nodejs" "20.11.0" "/work/.tool-versions") ("python" "______" "No version is set")) ((:call :command current :command-arguments nil :output t) (:call :command current :command-arguments ("ruby") :output t)))"#
        ]],
    )
}

fn asdf_vm_help_formats_long_lines_into_read_only_help_buffer_and_displays_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_help_formats_long_lines_into_read_only_help_buffer_and_displays_it",
        r##"(let ((asdf-vm-help-buffer-name
                    "*asdf-vm-test-help*")
                   (asdf-vm-help-fill-column-width
                    24)
                   calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       (concat
                        "Ruby plugin documentation has a deliberately long first line for wrapping.\n"
                        "short second line\n")))
                    ((symbol-function
                      'pop-to-buffer)
                     (lambda (buffer &rest arguments)
                       (push
                        (list
                         :display
                         (buffer-name buffer)
                         arguments)
                        calls)
                       buffer)))
                 (asdf-vm-help
                  "ruby"
                  "3.3.1")
                 (with-current-buffer
                     asdf-vm-help-buffer-name
                   (list
                    (buffer-string)
                    major-mode
                    buffer-read-only
                    (nreverse calls)))))"##,
        expect![[
            r#"OK ("Ruby plugin\ndocumentation has a\ndeliberately long first\nline for wrapping.\nshort second line\n" help-mode t ((:call :command help :command-arguments ("ruby" "3.3.1") :output t) (:display "*asdf-vm-test-help*" nil)))"#
        ]],
    )
}

fn asdf_vm_install_forwards_tool_versions_or_explicit_version_and_blocking_intent()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_install_forwards_tool_versions_or_explicit_version_and_blocking_intent",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       :started)))
                 (list
                  (asdf-vm-install
                   "ruby")
                  (asdf-vm-install
                   "ruby"
                   "3.3.1"
                   1)
                  (nreverse calls))))"##,
        expect![[
            r#"OK (:started :started ((:call :command install :command-arguments ("ruby") :blocking nil) (:call :command install :command-arguments ("ruby" "3.3.1") :blocking 1)))"#
        ]],
    )
}

fn asdf_vm_latest_list_and_list_all_transform_cli_whitespace_and_filters_exactly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_latest_list_and_list_all_transform_cli_whitespace_and_filters_exactly",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push arguments calls)
                       (pcase
                           (plist-get
                            arguments
                            :command)
                         ('latest
                          "  3.3.1-rc1 \n")
                         ('list
                          " *3.3.1  \n  3.2.4\n\n")
                         (`(list all)
                          "3.4.0-dev\n3.3.1\n 3.2.4 \n")
                         (_
                          "unexpected")))))
                 (list
                  (asdf-vm-latest
                   "ruby"
                   "3.3")
                  (asdf-vm-list
                   "ruby"
                   "3")
                  (asdf-vm-list-all
                   "ruby"
                   "3.")
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("3.3.1-rc1" ("*3.3.1" "3.2.4") ("3.4.0-dev" "3.3.1" "3.2.4") ((:command latest :command-arguments ("ruby" "3.3") :output t) (:command list :command-arguments ("ruby" "3") :output t) (:command (list all) :command-arguments ("ruby" "3.") :output t)))"#
        ]],
    )
}

fn asdf_vm_installed_version_completion_forwards_all_options_and_strips_active_marker()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installed_version_completion_forwards_all_options_and_strips_active_marker",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-list)
                     (lambda (name)
                       (push
                        (list :list name)
                        calls)
                       '("* 3.3.1"
                         "3.2.4")))
                    ((symbol-function
                      'completing-read)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :complete arguments)
                        calls)
                       "  * 3.3.1  ")))
                 (list
                  (asdf-vm--installed-package-version-completing-read
                   "ruby"
                   'predicate
                   t
                   "3."
                   'history
                   "3.2.4"
                   t)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("3.3.1" ((:list "ruby") (:complete "Package version: " ("* 3.3.1" "3.2.4") predicate t "3." history "3.2.4" t)))"#
        ]],
    )
}

fn asdf_vm_set_uninstall_and_reshim_build_exact_mutating_commands_and_messages() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_set_uninstall_and_reshim_build_exact_mutating_commands_and_messages",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       :queued)))
                 (list
                  (asdf-vm-set
                   "ruby" "3.3.1" 4)
                  (asdf-vm-uninstall
                   "nodejs" "18.0" nil)
                  (asdf-vm-reshim
                   "python" "3.12.2" 1)
                  (nreverse calls))))"##,
        expect![[
            r#"OK (:queued :queued :queued ((:call :command set :command-arguments ("ruby" "3.3.1") :blocking 4) (:call :command uninstall :command-arguments ("nodejs" "18.0") :blocking nil) (:call :command reshim :command-arguments ("python" "3.12.2") :blocking 1)))"#
        ]],
    )
}

fn asdf_vm_where_which_and_version_trim_paths_versions_and_emit_exact_messages() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_where_which_and_version_trim_paths_versions_and_emit_exact_messages",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       (pcase
                           (plist-get
                            arguments
                            :command)
                         ('where
                          " /opt/asdf/installs/ruby/3.3.1 \n")
                         ('which
                          " /opt/asdf/shims/ruby \n")
                         ('version
                          " v0.16.2 \n")))))
                 (list
                  (asdf-vm-where
                   "ruby" "3.3.1" 1)
                  (asdf-vm-where
                   "ruby" nil 1)
                  (asdf-vm-which
                   "ruby" 1)
                  (asdf-vm-version 1)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("/opt/asdf/installs/ruby/3.3.1" "/opt/asdf/installs/ruby/3.3.1" "/opt/asdf/shims/ruby" "v0.16.2" ((:call :command where :command-arguments ("ruby" "3.3.1") :output t) (:call :command where :command-arguments ("ruby") :output t) (:call :command which :command-arguments ("ruby") :output t) (:call :command version :output t)))"#
        ]],
    )
}

fn asdf_vm_info_preserves_multiline_debug_output_and_interactive_message_payload() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_info_preserves_multiline_debug_output_and_interactive_message_payload",
        r##"(let ((output
                    (concat
                     "OS:\\nLinux fixture\\n"
                     "SHELL:\\nzsh\\n"
                     "ASDF VERSION:\\nv0.16.2\\n"))
                   calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       output)))
                 (list
                  (asdf-vm-info)
                  (asdf-vm-info 1)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("OS:\\nLinux fixture\\nSHELL:\\nzsh\\nASDF VERSION:\\nv0.16.2\\n" "OS:\\nLinux fixture\\nSHELL:\\nzsh\\nASDF VERSION:\\nv0.16.2\\n" ((:call :command info :output t) (:call :command info :output t)))"#
        ]],
    )
}

fn asdf_vm_shim_versions_splits_real_provider_lines_and_preserves_multiword_tail() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_shim_versions_splits_real_provider_lines_and_preserves_multiword_tail",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       (concat
                        "ruby 3.3.1\n"
                        "ruby 3.2.4\n"
                        "custom ref feature branch\n"))))
                 (list
                  (asdf-vm-shim-versions
                   "ruby" 1)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ((("ruby" "3.3.1") ("ruby" "3.2.4") ("custom" "ref" "feature" "branch")) ((:call :command shim-versions :command-arguments ("ruby") :output t)))"#
        ]],
    )
}

fn asdf_vm_core_commands_use_real_stub_executable_and_preserve_cli_parsing_end_to_end()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_core_commands_use_real_stub_executable_and_preserve_cli_parsing_end_to_end",
        r##"(let* ((executable
                     (asdf-vm-test-make-executable
                      "asdf-core"
                      (concat
                       "case \"${1-} ${2-}\" in\n"
                       "  'current ') printf 'ruby 3.3.1 /work/.tool-versions\\nnodejs 20.0 /work/.tool-versions\\n' ;;\n"
                       "  'latest ruby') printf '3.3.1\\n' ;;\n"
                       "  'list ruby') printf ' *3.3.1\\n  3.2.4\\n' ;;\n"
                       "  'where ruby') printf '/opt/ruby/3.3.1\\n' ;;\n"
                       "  'which ruby') printf '/opt/shims/ruby\\n' ;;\n"
                       "  'version ') printf 'v0.16.2\\n' ;;\n"
                       "  'shim-versions ruby') printf 'ruby 3.3.1\\nruby 3.2.4\\n' ;;\n"
                       "  *) printf 'unexpected:<%s>\\n' \"$*\" ;;\n"
                       "esac")))
                    (asdf-vm-process-executable
                     executable)
                    (asdf-vm-process-executable-arguments
                     nil))
               (list
                (asdf-vm-current)
                (asdf-vm-latest
                 "ruby")
                (asdf-vm-list
                 "ruby")
                (asdf-vm-where
                 "ruby")
                (asdf-vm-which
                 "ruby")
                (asdf-vm-version)
                (asdf-vm-shim-versions
                 "ruby")))"##,
        expect![[
            r#"OK ((("nodejs" "20.0" "/work/.tool-versions")) "3.3.1" ("*3.3.1" "3.2.4") "/opt/ruby/3.3.1" "/opt/shims/ruby" "v0.16.2" (("ruby" "3.3.1") ("ruby" "3.2.4")))"#
        ]],
    )
}

pub(super) fn core_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_current_parses_realistic_multi_tool_output_and_reports_interactive_table(),
        asdf_vm_help_formats_long_lines_into_read_only_help_buffer_and_displays_it(),
        asdf_vm_install_forwards_tool_versions_or_explicit_version_and_blocking_intent(),
        asdf_vm_latest_list_and_list_all_transform_cli_whitespace_and_filters_exactly(),
        asdf_vm_installed_version_completion_forwards_all_options_and_strips_active_marker(),
        asdf_vm_set_uninstall_and_reshim_build_exact_mutating_commands_and_messages(),
        asdf_vm_where_which_and_version_trim_paths_versions_and_emit_exact_messages(),
        asdf_vm_info_preserves_multiline_debug_output_and_interactive_message_payload(),
        asdf_vm_shim_versions_splits_real_provider_lines_and_preserves_multiword_tail(),
        asdf_vm_core_commands_use_real_stub_executable_and_preserve_cli_parsing_end_to_end(),
    ]
}
