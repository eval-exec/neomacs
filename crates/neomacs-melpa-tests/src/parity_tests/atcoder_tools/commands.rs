use expect_test::expect;

use super::ParityBatchCase;

fn atcoder_tools_expands_every_placeholder_repetition_and_preserves_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_expands_every_placeholder_repetition_and_preserves_order",
        r##"(atcoder-tools--expand-cmd-templates
          '("compile %s -o %e"
            "test -d %d -e %e"
            "%d|%s|%e|%d|%s|%e"
            "literal %% %x")
          "/workspace/contest"
          "/workspace/contest/main.cpp"
          "/workspace/contest/main")"##,
        expect![[
            r#"OK ("compile /workspace/contest/main.cpp -o /workspace/contest/main" "test -d /workspace/contest -e /workspace/contest/main" "/workspace/contest|/workspace/contest/main.cpp|/workspace/contest/main|/workspace/contest|/workspace/contest/main.cpp|/workspace/contest/main" "literal %% %x")"#
        ]],
    )
}

fn atcoder_tools_command_expansion_shell_quotes_spaces_quotes_unicode_and_metacharacters()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_command_expansion_shell_quotes_spaces_quotes_unicode_and_metacharacters",
        r##"(atcoder-tools--expand-cmd-templates
          '("cd %d && compiler %s -o %e")
          "/work/AtCoder Finals; echo owned"
          "/work/AtCoder Finals/it's λ.cpp"
          "/work/AtCoder Finals/it's λ")"##,
        expect![[
            r#"OK ("cd /work/AtCoder\\ Finals\\;\\ echo\\ owned && compiler /work/AtCoder\\ Finals/it\\'s\\ \\λ.cpp -o /work/AtCoder\\ Finals/it\\'s\\ \\λ")"#
        ]],
    )
}

fn atcoder_tools_replacement_is_nonrecursive_for_placeholder_text_inside_quoted_paths()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_replacement_is_nonrecursive_for_placeholder_text_inside_quoted_paths",
        r##"(atcoder-tools--expand-cmd-templates
          '("dir=%d src=%s exec=%e")
          "/contest/%s/%e"
          "/source/%e/main.c"
          "/bin/%d/main")"##,
        expect![[r#"OK ("dir=/contest/\\%s/\\%e src=/source/\\%e/main.c exec=/bin/\\%d/main")"#]],
    )
}

fn atcoder_tools_command_expansion_handles_empty_templates_and_surfaces_bad_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_command_expansion_handles_empty_templates_and_surfaces_bad_values",
        r##"(list
          (atcoder-tools--expand-cmd-templates
           nil
           "/work"
           "/work/main.c"
           "/work/main")
          (atcoder-tools--expand-cmd-templates
           '()
           "/work"
           "/work/main.c"
           "/work/main")
          (atcoder-tools--expand-cmd-templates
           '("")
           ""
           ""
           "")
          (mapcar
           (lambda (templates)
             (atcoder-tools-test-error-data
              (lambda ()
                (atcoder-tools--expand-cmd-templates
                 templates
                 "/work"
                 "/work/main.c"
                 "/work/main"))))
           '((nil)
             (42)
             (symbol)
             ("ok" nil))))"##,
        expect![[
            r#"OK (nil nil ("") ((:error wrong-type-argument (arrayp nil)) (:error wrong-type-argument (sequencep 42)) (:error wrong-type-argument (sequencep symbol)) (:error wrong-type-argument (arrayp nil))))"#
        ]],
    )
}

fn atcoder_tools_c_gcc_test_builds_exact_command_environment_and_deletes_stale_executable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_c_gcc_test_builds_exact_command_environment_and_deletes_stale_executable",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "abc321/A/main.c"
                 "int main(void) { return 0; }\n"))
               (executable
                (file-name-sans-extension source))
               compilation)
          (atcoder-tools-test-write-file
           root
           "abc321/A/main"
           "stale executable")
          (let ((atcoder-tools-c-compiler
                 'gcc))
            (cl-letf
                (((symbol-function 'compile)
                  (lambda (command &optional comint)
                    (setq compilation
                          (list
                           (atcoder-tools-test-normalize
                            command root)
                           comint
                           (and
                            (boundp
                             'comint-terminfo-terminal)
                            comint-terminfo-terminal)
                           (file-exists-p source)
                           (file-exists-p executable)))
                    :started)))
              (list
               (atcoder-tools--test
                'c-mode
                source)
               compilation
               (file-exists-p source)
               (file-exists-p executable)
               (atcoder-tools-test-tree root)))))"##,
        expect![[
            r#"OK (nil ("gcc -x c -std=gnu11 -o [ROOT]/abc321/A/main -lm -O2 [ROOT]/abc321/A/main.c && atcoder-tools test -e [ROOT]/abc321/A/main -d [ROOT]/abc321/A" t nil t t) t nil (("abc321/A/main.c" 29 "2ad75d95660563887d8d3f1d0ae1dcf18c2379cbd83a5c72f5ab276351ee6949")))"#
        ]],
    )
}

fn atcoder_tools_preloaded_compilation_lifecycle_binds_ansi_terminal_only_during_compile()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_preloaded_compilation_lifecycle_binds_ansi_terminal_only_during_compile",
        r##"(progn
         (require 'compile)
         (let* ((comint-terminfo-terminal
                "fixture-outer")
                (root
                 (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "abc321/B/main.c"
                 "source"))
               (executable
                (atcoder-tools-test-write-file
                 root
                 "abc321/B/main"
                 "old"))
               observed)
          (let ((before
                 comint-terminfo-terminal))
            (cl-letf
                (((symbol-function 'compile)
                  (lambda (_command &optional _comint)
                    (setq observed
                          comint-terminfo-terminal)
                    :started)))
              (list
               before
               (atcoder-tools--test
                'c-mode
                source)
               observed
               comint-terminfo-terminal
               (file-exists-p executable))))))"##,
        expect![[r#"OK ("fixture-outer" nil "ansi" "fixture-outer" nil)"#]],
    )
}

fn atcoder_tools_c_and_cxx_compiler_variants_construct_exact_practical_commands() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atcoder_tools_c_and_cxx_compiler_variants_construct_exact_practical_commands",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (c-source
                (atcoder-tools-test-write-file
                 root
                 "round 1/C task/main file.c"
                 "int main(void) { return 0; }\n"))
               (cxx-source
                (atcoder-tools-test-write-file
                 root
                 "round 1/C task/main file.cpp"
                 "int main() { return 0; }\n"))
               commands)
          (dolist (file (list
                         (file-name-sans-extension
                          c-source)
                         (file-name-sans-extension
                          cxx-source)))
            (atcoder-tools-test-write-file
             (file-name-directory file)
             (file-name-nondirectory file)
             "old"))
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional comint)
                  (push
                   (list
                    (atcoder-tools-test-normalize
                     command root)
                    comint
                    (and
                     (boundp
                      'comint-terminfo-terminal)
                     comint-terminfo-terminal))
                   commands)
                  :started)))
            (let ((atcoder-tools-c-compiler
                   'clang))
              (atcoder-tools--test
               'c-mode
               c-source))
            (let ((atcoder-tools-c-compiler
                   'clang)
                  (atcoder-tools-c++-compiler
                   'gcc))
              (atcoder-tools--test
               'c++-mode
               cxx-source)))
          (list
           (nreverse commands)
           (file-exists-p
            (file-name-sans-extension
             c-source))
           (file-exists-p
            (file-name-sans-extension
             cxx-source))
           (mapcar
            #'car
            (atcoder-tools-test-tree root))))"##,
        expect![[
            r#"OK ((("clang -x c -lm -O2 -o [ROOT]/round\\ 1/C\\ task/main\\ file [ROOT]/round\\ 1/C\\ task/main\\ file.c && atcoder-tools test -e [ROOT]/round\\ 1/C\\ task/main\\ file -d [ROOT]/round\\ 1/C\\ task" t nil) ("clang++ -std=c++14 -stdlib=libc++ -O2 -o [ROOT]/round\\ 1/C\\ task/main\\ file [ROOT]/round\\ 1/C\\ task/main\\ file.cpp && atcoder-tools test -e [ROOT]/round\\ 1/C\\ task/main\\ file -d [ROOT]/round\\ 1/C\\ task" t nil)) nil nil ("round 1/C task/main file.c" "round 1/C task/main file.cpp"))"#
        ]],
    )
    .fresh_process()
}

fn atcoder_tools_rust_rustc_and_rustup_workflows_build_and_clean_exact_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_rust_rustc_and_rustup_workflows_build_and_clean_exact_paths",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "abc133/A/main.rs"
                 "fn main() { println!(\"8\"); }\n"))
               (executable
                (file-name-sans-extension source))
               commands)
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional comint)
                  (push
                   (list
                    (atcoder-tools-test-normalize
                     command root)
                    comint
                    (and
                     (boundp
                      'comint-terminfo-terminal)
                     comint-terminfo-terminal))
                   commands)
                  :started)))
            (dolist (rustup '(nil t))
              (atcoder-tools-test-write-file
               root
               "abc133/A/main"
               "stale")
              (let ((atcoder-tools-rust-use-rustup
                     rustup))
                (atcoder-tools--test
                 'rust-mode
                 source))
              (push
               (list
                :exists-after
                rustup
                (file-exists-p executable))
               commands)))
          (nreverse commands))"##,
        expect![[
            r#"OK (("rustc -Oo [ROOT]/abc133/A/main [ROOT]/abc133/A/main.rs && env RUST_BACKTRACE=1 atcoder-tools test -e [ROOT]/abc133/A/main -d [ROOT]/abc133/A" t nil) (:exists-after nil nil) ("rustup run --install 1.15.1 rustc -Oo [ROOT]/abc133/A/main [ROOT]/abc133/A/main.rs && env RUST_BACKTRACE=1 atcoder-tools test -e [ROOT]/abc133/A/main -d [ROOT]/abc133/A" t nil) (:exists-after t nil))"#
        ]],
    )
    .fresh_process()
}

fn atcoder_tools_custom_run_configuration_can_keep_executable_and_join_many_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_custom_run_configuration_can_keep_executable_and_join_many_commands",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "practice/A/solution.xyz"
                 "source"))
               (executable
                (file-name-sans-extension source))
               (atcoder-tools--run-config-alist
                '((c-gcc
                   (cmd-templates
                    . ("prepare %d"
                       "build %s %e"
                       "verify %e"))
                   (remove-exec . nil))))
               observed)
          (atcoder-tools-test-write-file
           root
           "practice/A/solution"
           "keep")
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional comint)
                  (setq observed
                        (list
                         (atcoder-tools-test-normalize
                          command root)
                         comint
                         (and
                          (boundp
                           'comint-terminfo-terminal)
                          comint-terminfo-terminal)))
                  :started)))
            (list
             (atcoder-tools--test
              'c-mode
              source)
             observed
             (file-exists-p executable)
             (atcoder-tools-test-read-file
              executable))))"##,
        expect![[
            r#"OK (nil ("prepare [ROOT]/practice/A && build [ROOT]/practice/A/solution.xyz [ROOT]/practice/A/solution && verify [ROOT]/practice/A/solution" t nil) t "keep")"#
        ]],
    )
    .fresh_process()
}

fn atcoder_tools_compile_signal_preserves_existing_executable_and_propagates() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_compile_signal_preserves_existing_executable_and_propagates",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "abc999/Z/main.cpp"
                 "source"))
               (executable
                (atcoder-tools-test-write-file
                 root
                 "abc999/Z/main"
                 "existing")))
          (cl-letf
              (((symbol-function 'compile)
                (lambda (&rest _)
                  (error "compilation refused"))))
            (list
             (atcoder-tools-test-error-data
              (lambda ()
                (atcoder-tools--test
                 'c++-mode
                 source)))
             (file-exists-p executable)
             (atcoder-tools-test-read-file
              executable))))"##,
        expect![[r#"OK ((:error error ("compilation refused")) t "existing")"#]],
    )
}

fn atcoder_tools_missing_executable_is_tolerated_after_compile_was_started() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_missing_executable_is_tolerated_after_compile_was_started",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "abc777/B/main.c"
                 "source"))
               observed)
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional comint)
                  (setq observed
                        (list
                         (atcoder-tools-test-normalize
                          command root)
                         comint))
                  :started)))
            (list
             (atcoder-tools-test-normalize-tree
              (atcoder-tools-test-error-data
               (lambda ()
                 (atcoder-tools--test
                  'c-mode
                  source)))
              root)
             observed
             (file-exists-p source)
             (file-exists-p
              (file-name-sans-extension
               source)))))"##,
        expect![[
            r#"OK ((:ok nil) ("gcc -x c -std=gnu11 -o [ROOT]/abc777/B/main -lm -O2 [ROOT]/abc777/B/main.c && atcoder-tools test -e [ROOT]/abc777/B/main -d [ROOT]/abc777/B" t) t nil)"#
        ]],
    )
}

fn atcoder_tools_public_test_command_forwards_live_buffer_mode_and_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_public_test_command_forwards_live_buffer_mode_and_file",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'atcoder-tools--test)
                (lambda (mode file)
                  (push
                   (list mode file)
                   calls)
                  :delegated)))
            (with-temp-buffer
              (setq
               major-mode 'rust-mode
               buffer-file-name
               "/contest/abc133/A/main.rs")
              (list
               (atcoder-tools-test)
               (call-interactively
                #'atcoder-tools-test)
               (nreverse calls)))))"##,
        expect![[
            r#"OK (:delegated :delegated ((rust-mode "/contest/abc133/A/main.rs") (rust-mode "/contest/abc133/A/main.rs")))"#
        ]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atcoder_tools_expands_every_placeholder_repetition_and_preserves_order(),
        atcoder_tools_command_expansion_shell_quotes_spaces_quotes_unicode_and_metacharacters(),
        atcoder_tools_replacement_is_nonrecursive_for_placeholder_text_inside_quoted_paths(),
        atcoder_tools_command_expansion_handles_empty_templates_and_surfaces_bad_values(),
        atcoder_tools_c_gcc_test_builds_exact_command_environment_and_deletes_stale_executable(),
        atcoder_tools_preloaded_compilation_lifecycle_binds_ansi_terminal_only_during_compile(),
        atcoder_tools_c_and_cxx_compiler_variants_construct_exact_practical_commands(),
        atcoder_tools_rust_rustc_and_rustup_workflows_build_and_clean_exact_paths(),
        atcoder_tools_custom_run_configuration_can_keep_executable_and_join_many_commands(),
        atcoder_tools_compile_signal_preserves_existing_executable_and_propagates(),
        atcoder_tools_missing_executable_is_tolerated_after_compile_was_started(),
        atcoder_tools_public_test_command_forwards_live_buffer_mode_and_file(),
    ]
}
