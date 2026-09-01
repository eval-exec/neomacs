use expect_test::expect;

use super::ParityBatchCase;

fn atcoder_tools_descriptor_and_archive_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_descriptor_and_archive_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'atcoder-tools
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name
                    name
                    directory))
                 '("atcoder-tools-pkg.el"
                   "atcoder-tools.el"))))
          (list
           (list
            (package-desc-name descriptor)
            (package-version-join
             (package-desc-version descriptor))
            (package-desc-summary descriptor)
            (package-desc-reqs descriptor)
            (package-desc-extras descriptor))
           (mapcar
            (lambda (file)
              (list
               (file-name-nondirectory file)
               (file-attribute-size
                (file-attributes file))
               (with-temp-buffer
                 (insert-file-contents-literally file)
                 (secure-hash
                  'sha256
                  (current-buffer)))))
            sources)))"##,
        expect![[
            r#"OK ((atcoder-tools "20200109.1236" "An atcoder-tools client." ((emacs (26)) (f (0 20)) (s (1 12))) ((:maintainers ("Seong Yong-ju" . "sei40kr@gmail.com")) (:authors ("Seong Yong-ju" . "sei40kr@gmail.com")) (:keywords "extensions" "tools") (:revdesc . "cfe61ed18ea9") (:commit . "cfe61ed18ea9b3b1bfb6f9e7d80a47599680cd1f") (:url . "https://github.com/sei40kr/atcoder-tools"))) (("atcoder-tools-pkg.el" 463 "5a650e75ad0066da1a043e2892697be63d7e77ca624903c98e7150891c2dcc78") ("atcoder-tools.el" 6486 "5d71662d89c5f569b8e687a5a19ee5e1e571db950360669fbe4e8d7ae980a806")))"#
        ]],
    )
}

fn atcoder_tools_activates_exact_pinned_dependency_closure_and_source_digests() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_activates_exact_pinned_dependency_closure_and_source_digests",
        r##"(let ((expected
                '((atcoder-tools
                   "20200109.1236"
                   "atcoder-tools")
                  (f
                   "20241003.1131"
                   "f")
                  (s
                   "20220902.1511"
                   "s")
                  (dash
                   "20260221.1346"
                   "dash"))))
         (list
          package-load-list
          (mapcar
           (lambda (entry)
             (pcase-let
                 ((`(,name
                     ,expected-version
                     ,library)
                   entry))
               (let* ((descriptor
                       (package--get-activatable-pkg
                        name))
                      (path
                       (locate-library library))
                      (directory
                       (and
                        path
                        (file-name-nondirectory
                         (directory-file-name
                          (file-name-directory
                           path))))))
                 (list
                  name
                  (package-version-join
                   (package-desc-version
                    descriptor))
                  (and
                   (memq
                    name
                    package-activated-list)
                   t)
                  directory
                  (with-temp-buffer
                    (insert-file-contents-literally
                     path)
                    (secure-hash
                     'sha256
                     (current-buffer)))
                  (equal
                   (package-version-join
                    (package-desc-version
                     descriptor))
                   expected-version)))))
           expected)))"##,
        expect![[
            r#"OK ((all (atcoder-tools "20200109.1236") (f "20241003.1131") (s "20220902.1511") (dash "20260221.1346")) ((atcoder-tools "20200109.1236" t "atcoder-tools-20200109.1236" "5d71662d89c5f569b8e687a5a19ee5e1e571db950360669fbe4e8d7ae980a806" t) (f "20241003.1131" t "f-20241003.1131" "6c50127cfb8ff86ada7667f0e6a4242002f41b4e132f11877de095be5cf3683e" t) (s "20220902.1511" t "s-20220902.1511" "fbb8ef1b861eef414fbb424ff3c55363f5b7a96866deec515c84a0523e61bed3" t) (dash "20260221.1346" t "dash-20260221.1346" "ce8043bfcfe64bfe69a411ee29e4c704213abd93aaa9a6da8b6791d3110d7f48" t)))"#
        ]],
    )
}

fn atcoder_tools_complete_prefixed_symbol_inventory_records_every_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_complete_prefixed_symbol_inventory_records_every_surface",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "atcoder-tools"
                    name)
                    (not
                     (string-prefix-p
                      "atcoder-tools-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (and
                    (custom-variable-p symbol)
                    t)
                   (get symbol 'custom-group)
                   (when (fboundp symbol)
                     (copy-tree
                      (help-function-arglist
                       symbol
                       t))))
                  symbols)))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name (car left))
              (symbol-name (car right))))))"##,
        expect![
            "OK ((atcoder-tools nil nil nil ((atcoder-tools-c-compiler custom-variable) (atcoder-tools-c++-compiler custom-variable) (atcoder-tools-rust-use-rustup custom-variable)) nil) (atcoder-tools- nil nil nil nil nil) (atcoder-tools--expand-cmd-templates t nil nil nil (cmd-templates working-directory src-file-name exec-file-name)) (atcoder-tools--open-problem t nil nil nil (metadata-file-name)) (atcoder-tools--run-config-alist nil t nil nil nil) (atcoder-tools--run-config-for-mode t nil nil nil (mode)) (atcoder-tools--test t nil nil nil (mode src-file-name)) (atcoder-tools-autoloads nil nil nil nil nil) (atcoder-tools-c++-compiler nil t t nil nil) (atcoder-tools-c-compiler nil t t nil nil) (atcoder-tools-open-problem t nil nil nil nil) (atcoder-tools-rust-use-rustup nil t t nil nil) (atcoder-tools-test t nil nil nil nil))"
        ],
    )
}

fn atcoder_tools_all_functions_have_exact_call_and_documentation_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_all_functions_have_exact_call_and_documentation_contracts",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (commandp symbol)
             (interactive-form symbol)
             (copy-tree
              (help-function-arglist symbol t))
             (documentation symbol t)))
          '(atcoder-tools--run-config-for-mode
            atcoder-tools--expand-cmd-templates
            atcoder-tools--test
            atcoder-tools--open-problem
            atcoder-tools-test
            atcoder-tools-open-problem))"##,
        expect![[
            r#"OK ((atcoder-tools--run-config-for-mode t nil nil (mode) "Return an alist of run configuration for MODE.") (atcoder-tools--expand-cmd-templates t nil nil (cmd-templates working-directory src-file-name exec-file-name) "Expand each command in CMD-TEMPLATES, a list of command templates.\n\n%d in the template will be replaced with WORKING-DIRECTORY.\n%s in the template will be replaced with SRC-FILE-NAME.\n%e in the template will be replaced with EXEC-FILE-NAME.") (atcoder-tools--test t nil nil (mode src-file-name) "Internally called by `atcoder-tools-test'.\n\nMODE is the major mode of the solution buffer to test.\nSRC-FILE-NAME is the name of the solution file.") (atcoder-tools--open-problem t nil nil (metadata-file-name) "Internally called by `atcoder-tools-open-problem'.\n\nMETADATA-FILE-NAME is the path to metadata.json generated by atcoder-tools.") (atcoder-tools-test t t (interactive nil) nil "Test your solution using atcoder-tools.\n\nAn executable of the solution will be built if needed.") (atcoder-tools-open-problem t t (interactive nil) nil "Open the AtCoder's task page of current buffer in a web browser."))"#
        ]],
    )
}

fn atcoder_tools_customization_schema_defaults_and_group_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_customization_schema_defaults_and_group_are_exact",
        r##"(list
          (get 'atcoder-tools 'custom-group)
          (documentation-property
           'atcoder-tools
           'group-documentation
           t)
          (mapcar
           (lambda (symbol)
             (let* ((standard-value
                     (get symbol 'standard-value))
                    (one-standard-form
                     (=
                      (length standard-value)
                      1))
                    (default-value
                     (and
                      one-standard-form
                      (eval
                       (car standard-value)
                       t))))
               (list
                symbol
                (and
                 (custom-variable-p symbol)
                 t)
                (symbol-value symbol)
                one-standard-form
                default-value
                (equal
                 (symbol-value symbol)
                 default-value)
                (get symbol 'custom-type)
                (get symbol 'custom-group)
                (documentation-property
                 symbol
                 'variable-documentation
                 t))))
           '(atcoder-tools-c-compiler
             atcoder-tools-c++-compiler
             atcoder-tools-rust-use-rustup)))"##,
        expect![[
            r#"OK (((atcoder-tools-c-compiler custom-variable) (atcoder-tools-c++-compiler custom-variable) (atcoder-tools-rust-use-rustup custom-variable)) "atcoder-tools client" ((atcoder-tools-c-compiler t gcc t gcc t (choice (const gcc) (const clang)) nil "The compiler to use to compile C code. Possible values are `gcc' and `clang'.") (atcoder-tools-c++-compiler t gcc t gcc t (choice (const gcc) (const clang)) nil "The compiler to use to compile C++ code. Possible values are `gcc' and `clang'.") (atcoder-tools-rust-use-rustup t t t t t bool nil "If non-nil, Rustup is used to compile Rust code.")))"#
        ]],
    )
}

fn atcoder_tools_run_configuration_table_is_exact_and_independently_copyable() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_run_configuration_table_is_exact_and_independently_copyable",
        r##"(let ((original
                (copy-tree
                 atcoder-tools--run-config-alist))
               (copy
                (copy-tree
                 atcoder-tools--run-config-alist)))
          (setcdr
           (assq 'remove-exec
                 (alist-get 'c-gcc copy))
           nil)
          (list
           original
           copy
           (equal
            original
            atcoder-tools--run-config-alist)
           (eq
            original
            atcoder-tools--run-config-alist)))"##,
        expect![[
            r#"OK (((c-gcc (cmd-templates "gcc -x c -std=gnu11 -o %e -lm -O2 %s" "atcoder-tools test -e %e -d %d") (remove-exec . t)) (c-clang (cmd-templates "clang -x c -lm -O2 -o %e %s" "atcoder-tools test -e %e -d %d") (remove-exec . t)) (c++-gcc (cmd-templates "g++ -std=gnu++1y -O2 -o %e %s" "atcoder-tools test -e %e -d %d") (remove-exec . t)) (c++-clang (cmd-templates "clang++ -std=c++14 -stdlib=libc++ -O2 -o %e %s" "atcoder-tools test -e %e -d %d") (remove-exec . t)) (rust-rustc (cmd-templates "rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") (remove-exec . t)) (rust-rustup (cmd-templates "rustup run --install 1.15.1 rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") (remove-exec . t))) ((c-gcc (cmd-templates "gcc -x c -std=gnu11 -o %e -lm -O2 %s" "atcoder-tools test -e %e -d %d") (remove-exec)) (c-clang (cmd-templates "clang -x c -lm -O2 -o %e %s" "atcoder-tools test -e %e -d %d") (remove-exec . t)) (c++-gcc (cmd-templates "g++ -std=gnu++1y -O2 -o %e %s" "atcoder-tools test -e %e -d %d") (remove-exec . t)) (c++-clang (cmd-templates "clang++ -std=c++14 -stdlib=libc++ -O2 -o %e %s" "atcoder-tools test -e %e -d %d") (remove-exec . t)) (rust-rustc (cmd-templates "rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") (remove-exec . t)) (rust-rustup (cmd-templates "rustup run --install 1.15.1 rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") (remove-exec . t))) t nil)"#
        ]],
    )
}

fn atcoder_tools_installed_source_preserves_warning_fatal_invalid_custom_type_rejection()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_installed_source_preserves_warning_fatal_invalid_custom_type_rejection",
        r##"(progn
          (require 'bytecomp)
          (let* ((root
                (atcoder-tools-test-root))
               (source
                (expand-file-name
                 "byte-compile/atcoder-tools.el"
                 root))
               (compiled
                (byte-compile-dest-file
                 source))
               messages)
          (make-directory
           (file-name-directory source)
           t)
          (copy-file
           (getenv "NEOMACS_PACKAGE_SOURCE")
           source
           t)
          (let ((byte-compile-error-on-warn
                 t)
                (byte-compile-warnings
                 t))
            (cl-letf
                (((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (push
                     (atcoder-tools-test-normalize
                      (apply
                       #'format
                       format-string
                       arguments)
                      root)
                     messages))))
              (let* ((outcome
                      (atcoder-tools-test-normalize-tree
                       (atcoder-tools-test-error-data
                        (lambda ()
                          (byte-compile-file
                           source)))
                       root))
                     (normalized-messages
                      (nreverse messages))
                     (invalid-type-diagnostic
                      (seq-find
                       (lambda (text)
                         (and
                          (string-match-p
                           "atcoder-tools-rust-use-rustup"
                           text)
                          (string-match-p
                           "bool.*not a valid type"
                           text)))
                       normalized-messages)))
                (list
                 outcome
                 (file-exists-p compiled)
                 (secure-hash
                  'sha256
                  (atcoder-tools-test-read-file
                   source))
                 (and
                  invalid-type-diagnostic
                  'invalid-bool-custom-type)
                 (and
                  invalid-type-diagnostic
                  (string-match-p
                   "\\[ROOT\\]/byte-compile/atcoder-tools\\.el:52:[0-9]+: Error"
                   invalid-type-diagnostic)
                  t)))))))"##,
        expect![[
            r#"OK ((:ok nil) nil "5d71662d89c5f569b8e687a5a19ee5e1e571db950360669fbe4e8d7ae980a806" invalid-bool-custom-type t)"#
        ]],
    )
}

fn atcoder_tools_generated_autoload_exposes_only_public_interactive_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_generated_autoload_exposes_only_public_interactive_commands",
        r##"(let* ((history
                (seq-find
                 (lambda (entry)
                   (and
                    (stringp
                     (car entry))
                    (string-suffix-p
                     "atcoder-tools-autoloads.el"
                     (car entry))))
                 load-history))
               (history-contract
                (mapcar
                 (lambda (event)
                   (list
                    (car event)
                    (cdr event)))
                 (seq-filter
                  (lambda (event)
                    (memq
                     (car-safe event)
                     '(defun provide)))
                  (cdr history)))))
          (list
           (featurep 'atcoder-tools-autoloads)
           (featurep 'atcoder-tools)
           history-contract
           (and
            (boundp 'definition-prefixes)
            (sort
             (delete-dups
              (copy-sequence
               (gethash
                "atcoder-tools-"
                definition-prefixes)))
             #'string<))
           (mapcar
            (lambda (symbol)
              (let ((definition
                     (and
                      (fboundp symbol)
                      (symbol-function symbol))))
                (list
                 symbol
                 (autoloadp definition)
                 (and
                  (autoloadp definition)
                  (nth 1 definition))
                 (commandp symbol)
                 (help-function-arglist
                  symbol
                  t))))
            '(atcoder-tools-test
              atcoder-tools-open-problem
              atcoder-tools--test
              atcoder-tools--open-problem
              atcoder-tools--run-config-for-mode))))"##,
        expect![[
            r#"OK (t nil ((defun atcoder-tools-test) (defun atcoder-tools-open-problem) (provide atcoder-tools-autoloads)) ("atcoder-tools") ((atcoder-tools-test t "atcoder-tools" t "[Arg list not available until function definition is loaded.]") (atcoder-tools-open-problem t "atcoder-tools" t "[Arg list not available until function definition is loaded.]") (atcoder-tools--test nil nil nil t) (atcoder-tools--open-problem nil nil nil t) (atcoder-tools--run-config-for-mode nil nil nil t)))"#
        ]],
    )
}

pub(super) fn registry_atcoder_tools_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atcoder_tools_descriptor_and_archive_sources_pin_exact_melpa_payload(),
        atcoder_tools_activates_exact_pinned_dependency_closure_and_source_digests(),
        atcoder_tools_complete_prefixed_symbol_inventory_records_every_surface(),
        atcoder_tools_all_functions_have_exact_call_and_documentation_contracts(),
        atcoder_tools_customization_schema_defaults_and_group_are_exact(),
        atcoder_tools_run_configuration_table_is_exact_and_independently_copyable(),
        atcoder_tools_installed_source_preserves_warning_fatal_invalid_custom_type_rejection(),
    ]
}

pub(super) fn registry_atcoder_tools_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![atcoder_tools_generated_autoload_exposes_only_public_interactive_commands()]
}
