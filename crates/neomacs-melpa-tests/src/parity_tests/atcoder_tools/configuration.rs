use expect_test::expect;

use super::ParityBatchCase;

fn atcoder_tools_c_mode_selects_complete_gcc_and_clang_configurations() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_c_mode_selects_complete_gcc_and_clang_configurations",
        r##"(mapcar
          (lambda (compiler)
            (let ((atcoder-tools-c-compiler
                   compiler))
              (list
               compiler
               (atcoder-tools-test-error-data
                (lambda ()
                  (atcoder-tools-test-config-snapshot
                   (atcoder-tools--run-config-for-mode
                    'c-mode)))))))
          '(gcc clang nil t "gcc" custom))"##,
        expect![[
            r#"OK ((gcc (:ok (("gcc -x c -std=gnu11 -o %e -lm -O2 %s" "atcoder-tools test -e %e -d %d") t 2))) (clang (:ok (("clang -x c -lm -O2 -o %e %s" "atcoder-tools test -e %e -d %d") t 2))) (nil (:error error ("Invalid atcoder-tools-c-compiler value: nil"))) (t (:error error ("Invalid atcoder-tools-c-compiler value: t"))) ("gcc" (:error error ("Invalid atcoder-tools-c-compiler value: \"gcc\""))) (custom (:error error ("Invalid atcoder-tools-c-compiler value: custom"))))"#
        ]],
    )
}

fn atcoder_tools_cxx_selection_practically_records_cross_customization_behavior() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atcoder_tools_cxx_selection_practically_records_cross_customization_behavior",
        r##"(let (observations)
          (dolist (c-value '(gcc clang))
            (dolist (cxx-value '(gcc clang invalid))
              (let ((atcoder-tools-c-compiler
                     c-value)
                    (atcoder-tools-c++-compiler
                     cxx-value))
                (push
                 (list
                  c-value
                  cxx-value
                  (atcoder-tools-test-config-snapshot
                   (atcoder-tools--run-config-for-mode
                    'c++-mode)))
                 observations))))
          (nreverse observations))"##,
        expect![[
            r#"OK ((gcc gcc (("g++ -std=gnu++1y -O2 -o %e %s" "atcoder-tools test -e %e -d %d") t 2)) (gcc clang (("g++ -std=gnu++1y -O2 -o %e %s" "atcoder-tools test -e %e -d %d") t 2)) (gcc invalid (("g++ -std=gnu++1y -O2 -o %e %s" "atcoder-tools test -e %e -d %d") t 2)) (clang gcc (("clang++ -std=c++14 -stdlib=libc++ -O2 -o %e %s" "atcoder-tools test -e %e -d %d") t 2)) (clang clang (("clang++ -std=c++14 -stdlib=libc++ -O2 -o %e %s" "atcoder-tools test -e %e -d %d") t 2)) (clang invalid (("clang++ -std=c++14 -stdlib=libc++ -O2 -o %e %s" "atcoder-tools test -e %e -d %d") t 2)))"#
        ]],
    )
}

fn atcoder_tools_cxx_invalid_selector_reports_the_cxx_named_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_cxx_invalid_selector_reports_the_cxx_named_contract",
        r##"(mapcar
          (lambda (c-value)
            (let ((atcoder-tools-c-compiler
                   c-value)
                  (atcoder-tools-c++-compiler
                   'gcc))
              (atcoder-tools-test-error-data
               (lambda ()
                 (atcoder-tools--run-config-for-mode
                  'c++-mode)))))
          '(nil t "clang" c++-gcc))"##,
        expect![[
            r#"OK ((:error error ("Invalid atcoder-tools-c++-compiler value: gcc")) (:error error ("Invalid atcoder-tools-c++-compiler value: gcc")) (:error error ("Invalid atcoder-tools-c++-compiler value: gcc")) (:error error ("Invalid atcoder-tools-c++-compiler value: gcc")))"#
        ]],
    )
}

fn atcoder_tools_rust_mode_uses_generalized_lisp_truth_for_rustup_choice() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_rust_mode_uses_generalized_lisp_truth_for_rustup_choice",
        r##"(mapcar
          (lambda (value)
            (let ((atcoder-tools-rust-use-rustup
                   value))
              (list
               value
               (atcoder-tools-test-config-snapshot
                (atcoder-tools--run-config-for-mode
                 'rust-mode)))))
          '(nil t 0 "" rustc ()))"##,
        expect![[
            r#"OK ((nil (("rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") t 2)) (t (("rustup run --install 1.15.1 rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") t 2)) (0 (("rustup run --install 1.15.1 rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") t 2)) ("" (("rustup run --install 1.15.1 rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") t 2)) (rustc (("rustup run --install 1.15.1 rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") t 2)) (nil (("rustc -Oo %e %s" "env RUST_BACKTRACE=1 atcoder-tools test -e %e -d %d") t 2)))"#
        ]],
    )
}

fn atcoder_tools_unsupported_modes_preserve_exact_error_payloads() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_unsupported_modes_preserve_exact_error_payloads",
        r##"(mapcar
          (lambda (mode)
            (list
             mode
             (atcoder-tools-test-error-data
              (lambda ()
                (atcoder-tools--run-config-for-mode
                 mode)))))
          '(python-mode
            fundamental-mode
            nil
            "c-mode"
            42
            (c-mode)))"##,
        expect![[
            r#"OK ((python-mode (:error error ("No run configuration found for python-mode"))) (fundamental-mode (:error error ("No run configuration found for fundamental-mode"))) (nil (:error error ("No run configuration found for nil"))) ("c-mode" (:error error ("No run configuration found for \"c-mode\""))) (42 (:error error ("No run configuration found for 42"))) ((c-mode) (:error error ("No run configuration found for (c-mode)"))))"#
        ]],
    )
}

fn atcoder_tools_resolved_configuration_aliases_the_live_table_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_resolved_configuration_aliases_the_live_table_entry",
        r##"(let ((saved
                (copy-tree
                 atcoder-tools--run-config-alist)))
          (unwind-protect
              (let ((config
                     (atcoder-tools--run-config-for-mode
                      'c-mode)))
                (setcdr
                 (assq 'remove-exec config)
                 nil)
                (setcdr
                 (assq 'cmd-templates config)
                 '("custom %s"))
                (list
                 (atcoder-tools-test-config-snapshot
                  config)
                 (atcoder-tools-test-config-snapshot
                  (atcoder-tools--run-config-for-mode
                   'c-mode))
                 (eq
                  config
                  (atcoder-tools--run-config-for-mode
                   'c-mode))))
            (setq
             atcoder-tools--run-config-alist
             saved)))"##,
        expect![[r#"OK ((("custom %s") nil 2) (("custom %s") nil 2) t)"#]],
    )
}

fn atcoder_tools_customization_setters_change_runtime_selection_and_restore_defaults()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_customization_setters_change_runtime_selection_and_restore_defaults",
        r##"(let ((saved-c
                atcoder-tools-c-compiler)
               (saved-cxx
                atcoder-tools-c++-compiler)
               (saved-rust
                atcoder-tools-rust-use-rustup))
          (unwind-protect
              (progn
                (customize-set-variable
                 'atcoder-tools-c-compiler
                 'clang)
                (customize-set-variable
                 'atcoder-tools-c++-compiler
                 'gcc)
                (customize-set-variable
                 'atcoder-tools-rust-use-rustup
                 nil)
                (list
                 atcoder-tools-c-compiler
                 atcoder-tools-c++-compiler
                 atcoder-tools-rust-use-rustup
                 (car
                  (alist-get
                   'cmd-templates
                   (atcoder-tools--run-config-for-mode
                    'c-mode)))
                 (car
                  (alist-get
                   'cmd-templates
                   (atcoder-tools--run-config-for-mode
                    'c++-mode)))
                 (car
                  (alist-get
                   'cmd-templates
                   (atcoder-tools--run-config-for-mode
                    'rust-mode)))))
            (customize-set-variable
             'atcoder-tools-c-compiler
             saved-c)
            (customize-set-variable
             'atcoder-tools-c++-compiler
             saved-cxx)
            (customize-set-variable
             'atcoder-tools-rust-use-rustup
             saved-rust)))"##,
        expect![[
            r#"OK (clang gcc nil "clang -x c -lm -O2 -o %e %s" "clang++ -std=c++14 -stdlib=libc++ -O2 -o %e %s" "rustc -Oo %e %s")"#
        ]],
    )
}

fn atcoder_tools_shadow_configuration_table_surfaces_missing_and_malformed_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atcoder_tools_shadow_configuration_table_surfaces_missing_and_malformed_entries",
        r##"(mapcar
          (lambda (table)
            (let ((atcoder-tools--run-config-alist
                   table))
              (atcoder-tools-test-error-data
               (lambda ()
                 (let ((config
                        (atcoder-tools--run-config-for-mode
                         'c-mode)))
                   (list
                    config
                    (alist-get
                     'cmd-templates
                     config)
                    (alist-get
                     'remove-exec
                     config)))))))
          '(nil
            ((c-gcc))
            ((c-gcc . malformed))
            ((c-gcc
              (cmd-templates)
              (remove-exec)))
            ((c-gcc
              (cmd-templates . ("one"))
              (remove-exec . nil)))))"##,
        expect![[
            r#"OK ((:ok (nil nil nil)) (:ok (nil nil nil)) (:error wrong-type-argument (listp malformed)) (:ok (((cmd-templates) (remove-exec)) nil nil)) (:ok (((cmd-templates . #1=("one")) (remove-exec)) #1# nil)))"#
        ]],
    )
}

pub(super) fn configuration_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atcoder_tools_c_mode_selects_complete_gcc_and_clang_configurations(),
        atcoder_tools_cxx_selection_practically_records_cross_customization_behavior(),
        atcoder_tools_cxx_invalid_selector_reports_the_cxx_named_contract(),
        atcoder_tools_rust_mode_uses_generalized_lisp_truth_for_rustup_choice(),
        atcoder_tools_unsupported_modes_preserve_exact_error_payloads(),
        atcoder_tools_resolved_configuration_aliases_the_live_table_entry(),
        atcoder_tools_customization_setters_change_runtime_selection_and_restore_defaults(),
        atcoder_tools_shadow_configuration_table_surfaces_missing_and_malformed_entries(),
    ]
}
