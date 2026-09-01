use expect_test::expect;

use super::ParityBatchCase;

fn atcoder_tools_cpp_contest_workflow_preserves_samples_builds_tests_and_opens_problem()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_cpp_contest_workflow_preserves_samples_builds_tests_and_opens_problem",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (directory "past-sample/A")
               (source
                (atcoder-tools-test-write-file
                 root
                 (concat directory "/main.cpp")
                 (concat
                  "#include <algorithm>\n"
                  "#include <iostream>\n"
                  "int main() { int n, a, b; std::cin >> n >> a >> b;"
                  " std::cout << std::min(n * a, b) << '\\n'; }\n")))
               (metadata
                (atcoder-tools-test-write-file
                 root
                 (concat directory "/metadata.json")
                 (concat
                  "{"
                  "\"code_filename\":\"main.cpp\","
                  "\"judge\":{\"judge_type\":\"normal\"},"
                  "\"lang\":\"cpp\","
                  "\"problem\":{\"alphabet\":\"A\","
                  "\"contest\":{\"contest_id\":\"past-sample\"},"
                  "\"problem_id\":\"abc133_a\"},"
                  "\"sample_in_pattern\":\"in_*.txt\","
                  "\"sample_out_pattern\":\"out_*.txt\""
                  "}")))
               (samples
                '(("in_1.txt" . "4 2 9\n")
                  ("out_1.txt" . "8\n")
                  ("in_2.txt" . "4 2 7\n")
                  ("out_2.txt" . "7\n")
                  ("in_3.txt" . "4 2 8\n")
                  ("out_3.txt" . "8\n")))
               compile-call
               browse-call)
          (dolist (sample samples)
            (atcoder-tools-test-write-file
             root
             (concat
              directory
              "/"
              (car sample))
             (cdr sample)))
          (atcoder-tools-test-write-file
           root
           (concat directory "/main")
           "stale")
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional comint)
                  (setq compile-call
                        (list
                         (atcoder-tools-test-normalize
                          command root)
                         comint
                         (and
                          (boundp
                           'comint-terminfo-terminal)
                          comint-terminfo-terminal)
                         (mapcar
                          (lambda (sample)
                            (atcoder-tools-test-read-file
                             (expand-file-name
                              (concat
                               directory
                               "/"
                               (car sample))
                              root)))
                          samples)))
                  :started))
               ((symbol-function 'browse-url)
                (lambda (url &rest arguments)
                  (setq browse-call
                        (list url arguments))
                  :opened)))
            (with-temp-buffer
              (setq
               major-mode 'c++-mode
               buffer-file-name source)
              (let ((atcoder-tools-c-compiler
                     'gcc)
                    (atcoder-tools-c++-compiler
                     'clang))
                (list
                 (atcoder-tools-test)
                 (atcoder-tools-open-problem)
                 compile-call
                 browse-call
                 (file-exists-p
                  (file-name-sans-extension
                   source))
                 (file-readable-p metadata)
                 (atcoder-tools-test-tree
                  root))))))"##,
        expect![[
            r#"OK (nil :opened ("g++ -std=gnu++1y -O2 -o [ROOT]/past-sample/A/main [ROOT]/past-sample/A/main.cpp && atcoder-tools test -e [ROOT]/past-sample/A/main -d [ROOT]/past-sample/A" t nil ("4 2 9\n" "8\n" "4 2 7\n" "7\n" "4 2 8\n" "8\n")) ("https://atcoder.jp/contests/past-sample/tasks/abc133_a" nil) nil t (("past-sample/A/in_1.txt" 6 "33399bbffd66ec9083771f5224dd9a9afe55e67aabab47a8a43f400ad7e23d0a") ("past-sample/A/in_2.txt" 6 "a7c79c0176b7dd39f239fdd06be7e3c17f49cd8798f5457ce592c21207fd5131") ("past-sample/A/in_3.txt" 6 "278406cab0cff99b76045838f9f70ac099cf439cc1551c6fcebbf228615f9da8") ("past-sample/A/main.cpp" 135 "a4ed3f14e1580c6d2b6d0b37b6993e69bbfb446b88cf4e974e6e84e425750e20") ("past-sample/A/metadata.json" 227 "f074a51c0133a6949b75e21453e81bb3d70bbcd98c11eba1d4216695ea3b1ddb") ("past-sample/A/out_1.txt" 2 "aa67a169b0bba217aa0aa88a65346920c84c42447c36ba5f7ea65f422c1fe5d8") ("past-sample/A/out_2.txt" 2 "10159baf262b43a92d95db59dae1f72c645127301661e0a3ce4e38b295a97c58") ("past-sample/A/out_3.txt" 2 "aa67a169b0bba217aa0aa88a65346920c84c42447c36ba5f7ea65f422c1fe5d8")))"#
        ]],
    )
    .fresh_process()
}

fn atcoder_tools_rust_contest_workflow_records_file_local_toggle_safety_and_preserves_fixture()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_rust_contest_workflow_records_file_local_toggle_safety_and_preserves_fixture",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "abc133/A/main.rs"
                 (concat
                  "// -*- atcoder-tools-rust-use-rustup: nil -*-\n"
                  "fn main() { println!(\"8\"); }\n")))
               (metadata
                (atcoder-tools-test-write-file
                 root
                 "abc133/A/metadata.json"
                 "{\"problem\":{\"contest\":{\"contest_id\":\"abc133\"},\"problem_id\":\"abc133_a\"}}"))
               commands
               urls)
          (dolist (sample
                   '(("in_1.txt" . "4 2 9\n")
                     ("out_1.txt" . "8\n")
                     ("in_2.txt" . "4 2 7\n")
                     ("out_2.txt" . "7\n")))
            (atcoder-tools-test-write-file
             root
             (concat
              "abc133/A/"
              (car sample))
             (cdr sample)))
          (atcoder-tools-test-write-file
           root
           "abc133/A/main"
           "old")
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional _)
                  (push
                   (atcoder-tools-test-normalize
                    command root)
                   commands)
                  :started))
               ((symbol-function 'browse-url)
                (lambda (url &rest _)
                  (push url urls)
                  :opened)))
            (with-temp-buffer
              (insert-file-contents source)
              (setq
               buffer-file-name source
               major-mode 'rust-mode)
              (hack-local-variables)
              (list
               atcoder-tools-rust-use-rustup
               (local-variable-p
                'atcoder-tools-rust-use-rustup)
               (atcoder-tools-test)
               (atcoder-tools-open-problem)
               (nreverse commands)
               (nreverse urls)
               (file-exists-p
                (file-name-sans-extension
                 source))
               (file-readable-p metadata)
               (mapcar
                #'car
                (atcoder-tools-test-tree
                 root))))))"##,
        expect![[
            r#"OK (t nil nil :opened ("rustup run --install 1.15.1 rustc -Oo [ROOT]/abc133/A/main [ROOT]/abc133/A/main.rs && env RUST_BACKTRACE=1 atcoder-tools test -e [ROOT]/abc133/A/main -d [ROOT]/abc133/A") ("https://atcoder.jp/contests/abc133/tasks/abc133_a") nil t ("abc133/A/in_1.txt" "abc133/A/in_2.txt" "abc133/A/main.rs" "abc133/A/metadata.json" "abc133/A/out_1.txt" "abc133/A/out_2.txt"))"#
        ]],
    )
}

fn atcoder_tools_multiple_problem_buffers_keep_paths_commands_and_urls_isolated() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atcoder_tools_multiple_problem_buffers_keep_paths_commands_and_urls_isolated",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (problems
                '((A c-mode "main.c" "abc400_a")
                  (B rust-mode "main.rs" "abc400_b")))
               commands
               urls
               outcomes)
          (dolist (problem problems)
            (let* ((letter (nth 0 problem))
                   (mode (nth 1 problem))
                   (file-name (nth 2 problem))
                   (problem-id (nth 3 problem))
                   (relative-directory
                    (format "abc400/%s" letter))
                   (source
                    (atcoder-tools-test-write-file
                     root
                     (format
                      "%s/%s"
                      relative-directory
                      file-name)
                     "source"))
                   (metadata
                    (atcoder-tools-test-write-file
                     root
                     (format
                      "%s/metadata.json"
                      relative-directory)
                     (format
                      (concat
                       "{\"problem\":{\"contest\":"
                       "{\"contest_id\":\"abc400\"},"
                       "\"problem_id\":\"%s\"}}")
                      problem-id))))
              (atcoder-tools-test-write-file
               root
               (format
                "%s/%s"
                relative-directory
                (file-name-sans-extension
                 file-name))
               "old")
              (cl-letf
                  (((symbol-function 'compile)
                    (lambda (command &optional _)
                      (push
                       (atcoder-tools-test-normalize
                        command root)
                       commands)
                      :started))
                   ((symbol-function 'browse-url)
                    (lambda (url &rest _)
                      (push url urls)
                      :opened)))
                (with-temp-buffer
                  (setq
                   major-mode mode
                   buffer-file-name source)
                  (let ((atcoder-tools-rust-use-rustup
                         nil))
                    (push
                     (list
                      letter
                      (atcoder-tools-test)
                      (atcoder-tools-open-problem)
                      (file-exists-p
                       (file-name-sans-extension
                        source))
                      (file-readable-p metadata))
                     outcomes))))))
          (list
           (nreverse outcomes)
           (nreverse commands)
           (nreverse urls)
           (mapcar
            #'car
            (atcoder-tools-test-tree
             root))))"##,
        expect![[
            r#"OK (((A nil :opened nil t) (B nil :opened nil t)) ("gcc -x c -std=gnu11 -o [ROOT]/abc400/A/main -lm -O2 [ROOT]/abc400/A/main.c && atcoder-tools test -e [ROOT]/abc400/A/main -d [ROOT]/abc400/A" "rustc -Oo [ROOT]/abc400/B/main [ROOT]/abc400/B/main.rs && env RUST_BACKTRACE=1 atcoder-tools test -e [ROOT]/abc400/B/main -d [ROOT]/abc400/B") ("https://atcoder.jp/contests/abc400/tasks/abc400_a" "https://atcoder.jp/contests/abc400/tasks/abc400_b") ("abc400/A/main.c" "abc400/A/metadata.json" "abc400/B/main.rs" "abc400/B/metadata.json"))"#
        ]],
    )
}

fn atcoder_tools_buffer_local_compiler_choices_drive_independent_compilation_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_buffer_local_compiler_choices_drive_independent_compilation_commands",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (gcc-source
                (atcoder-tools-test-write-file
                 root
                 "gcc/A/main.c"
                 "source"))
               (clang-source
                (atcoder-tools-test-write-file
                 root
                 "clang/A/main.c"
                 "source"))
               commands)
          (dolist (file
                   '("gcc/A/main"
                     "clang/A/main"))
            (atcoder-tools-test-write-file
             root file "old"))
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional _)
                  (push
                   (atcoder-tools-test-normalize
                    command root)
                   commands)
                  :started)))
            (dolist
                (spec
                 (list
                  (list gcc-source 'gcc)
                  (list clang-source 'clang)))
              (with-temp-buffer
                (setq
                 buffer-file-name (nth 0 spec)
                 major-mode 'c-mode)
                (set
                 (make-local-variable
                  'atcoder-tools-c-compiler)
                 (nth 1 spec))
                (atcoder-tools-test))))
          (list
           (nreverse commands)
           atcoder-tools-c-compiler
           (file-exists-p
            (file-name-sans-extension
             gcc-source))
           (file-exists-p
            (file-name-sans-extension
             clang-source))))"##,
        expect![[
            r#"OK (("gcc -x c -std=gnu11 -o [ROOT]/gcc/A/main -lm -O2 [ROOT]/gcc/A/main.c && atcoder-tools test -e [ROOT]/gcc/A/main -d [ROOT]/gcc/A" "clang -x c -lm -O2 -o [ROOT]/clang/A/main [ROOT]/clang/A/main.c && atcoder-tools test -e [ROOT]/clang/A/main -d [ROOT]/clang/A") gcc nil nil)"#
        ]],
    )
}

fn atcoder_tools_multi_extension_source_uses_only_final_extension_for_executable() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atcoder_tools_multi_extension_source_uses_only_final_extension_for_executable",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (source
                (atcoder-tools-test-write-file
                 root
                 "weird/A/solution.test.c"
                 "source"))
               (expected-executable
                (expand-file-name
                 "weird/A/solution.test"
                 root))
               (other-file
                (atcoder-tools-test-write-file
                 root
                 "weird/A/solution"
                 "must remain"))
               command)
          (atcoder-tools-test-write-file
           root
           "weird/A/solution.test"
           "remove")
          (cl-letf
              (((symbol-function 'compile)
                (lambda (value &optional _)
                  (setq command
                        (atcoder-tools-test-normalize
                         value root))
                  :started)))
            (list
             (atcoder-tools--test
              'c-mode
              source)
             command
             (file-exists-p
              expected-executable)
             (file-exists-p other-file)
             (atcoder-tools-test-read-file
              other-file))))"##,
        expect![[
            r#"OK (nil "gcc -x c -std=gnu11 -o [ROOT]/weird/A/solution.test -lm -O2 [ROOT]/weird/A/solution.test.c && atcoder-tools test -e [ROOT]/weird/A/solution.test -d [ROOT]/weird/A" nil t "must remain")"#
        ]],
    )
}

fn atcoder_tools_public_test_unsaved_buffer_preserves_exact_failure_before_compile()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_public_test_unsaved_buffer_preserves_exact_failure_before_compile",
        r##"(let (compile-calls)
          (cl-letf
              (((symbol-function 'compile)
                (lambda (&rest arguments)
                  (push arguments compile-calls)
                  :started)))
            (with-temp-buffer
              (setq
               major-mode 'c-mode
               buffer-file-name nil)
              (list
               (atcoder-tools-test-error-data
                (lambda ()
                  (atcoder-tools-test)))
               compile-calls))))"##,
        expect!["OK ((:error wrong-type-argument (stringp nil)) nil)"],
    )
}

fn atcoder_tools_failure_in_one_problem_does_not_mutate_global_config_or_other_fixture()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_failure_in_one_problem_does_not_mutate_global_config_or_other_fixture",
        r##"(let* ((root
                (atcoder-tools-test-root))
               (bad-source
                (atcoder-tools-test-write-file
                 root
                 "bad/A/main.c"
                 "bad"))
               (good-source
                (atcoder-tools-test-write-file
                 root
                 "good/B/main.rs"
                 "good"))
               (good-executable
                (atcoder-tools-test-write-file
                 root
                 "good/B/main"
                 "keep until success"))
               commands)
          (atcoder-tools-test-write-file
           root
           "bad/A/main"
           "bad executable")
          (cl-letf
              (((symbol-function 'compile)
                (lambda (command &optional _)
                  (push
                   (atcoder-tools-test-normalize
                    command root)
                   commands)
                  (when
                      (string-match-p
                       "bad/A"
                       command)
                    (error "bad compiler"))
                  :started)))
            (let ((bad
                   (atcoder-tools-test-error-data
                    (lambda ()
                      (let ((atcoder-tools-c-compiler
                             'clang))
                        (atcoder-tools--test
                         'c-mode
                         bad-source))))))
              (let ((atcoder-tools-rust-use-rustup
                     nil))
                (atcoder-tools--test
                 'rust-mode
                 good-source))
              (list
               bad
               (nreverse commands)
               atcoder-tools-c-compiler
               atcoder-tools-rust-use-rustup
               (file-exists-p
                (file-name-sans-extension
                 bad-source))
               (file-exists-p
                good-executable)
               (atcoder-tools-test-tree
                root)))))"##,
        expect![[
            r#"OK ((:error error ("bad compiler")) ("clang -x c -lm -O2 -o [ROOT]/bad/A/main [ROOT]/bad/A/main.c && atcoder-tools test -e [ROOT]/bad/A/main -d [ROOT]/bad/A" "rustc -Oo [ROOT]/good/B/main [ROOT]/good/B/main.rs && env RUST_BACKTRACE=1 atcoder-tools test -e [ROOT]/good/B/main -d [ROOT]/good/B") gcc t t nil (("bad/A/main" 14 "f95ee36aeda8fb7c0e8f676d8de87c5c80b0f1bc0c47db400448f65db5ffa557") ("bad/A/main.c" 3 "2f05d4b689d270cafb02285f35f44866f7dc8a2d368a3f9d1124373eeab31fb1") ("good/B/main.rs" 4 "770e607624d689265ca6c44884d0807d9b054d23c473c106c72be9de08b7376c")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atcoder_tools_cpp_contest_workflow_preserves_samples_builds_tests_and_opens_problem(),
        atcoder_tools_rust_contest_workflow_records_file_local_toggle_safety_and_preserves_fixture(
        ),
        atcoder_tools_multiple_problem_buffers_keep_paths_commands_and_urls_isolated(),
        atcoder_tools_buffer_local_compiler_choices_drive_independent_compilation_commands(),
        atcoder_tools_multi_extension_source_uses_only_final_extension_for_executable(),
        atcoder_tools_public_test_unsaved_buffer_preserves_exact_failure_before_compile(),
        atcoder_tools_failure_in_one_problem_does_not_mutate_global_config_or_other_fixture(),
    ]
}
