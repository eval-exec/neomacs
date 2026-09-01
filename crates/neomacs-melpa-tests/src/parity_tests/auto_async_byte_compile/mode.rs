use expect_test::expect;

use super::ParityBatchCase;

fn auto_async_byte_compile_mode_numeric_toggle_and_return_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_mode_numeric_toggle_and_return_contract_match",
        r##"(with-temp-buffer
          (list
           auto-async-byte-compile-mode
           (auto-async-byte-compile-mode nil)
           auto-async-byte-compile-mode
           (auto-async-byte-compile-mode -33)
           auto-async-byte-compile-mode
           (auto-async-byte-compile-mode 33)
           auto-async-byte-compile-mode
           (auto-async-byte-compile-mode 'toggle)
           auto-async-byte-compile-mode
           (auto-async-byte-compile-mode 'toggle)
           auto-async-byte-compile-mode))"##,
        expect!["OK (nil t t nil nil t t nil nil t t)"],
    )
}

fn auto_async_byte_compile_mode_installs_and_removes_one_buffer_local_save_hook() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_async_byte_compile_mode_installs_and_removes_one_buffer_local_save_hook",
        r##"(with-temp-buffer
          (let ((global-before
                 (default-value
                  'after-save-hook)))
            (list
             (local-variable-p
              'after-save-hook)
             (auto-async-byte-compile-mode 1)
             (local-variable-p
              'after-save-hook)
             after-save-hook
             (auto-async-byte-compile-mode 1)
             (length
              (seq-filter
               (lambda (function)
                 (eq
                  function
                  #'auto-async-byte-compile))
               after-save-hook))
             (auto-async-byte-compile-mode -1)
             after-save-hook
             (local-variable-p
              'after-save-hook)
             (equal
              global-before
              (default-value
               'after-save-hook)))))"##,
        expect!["OK (nil t t (auto-async-byte-compile t) t 1 nil nil nil t)"],
    )
}

fn auto_async_byte_compile_mode_isolated_across_real_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_mode_isolated_across_real_buffers",
        r##"(let ((first
                                (generate-new-buffer
                                 " *aabc-first*"))
                               (second
                                (generate-new-buffer
                                 " *aabc-second*")))
          (unwind-protect
              (progn
                (with-current-buffer first
                  (auto-async-byte-compile-mode 1))
                (list
                 (with-current-buffer first
                   (list
                    auto-async-byte-compile-mode
                    (memq
                     #'auto-async-byte-compile
                     after-save-hook)))
                 (with-current-buffer second
                   (list
                    auto-async-byte-compile-mode
                    (memq
                     #'auto-async-byte-compile
                     after-save-hook)))
                 (with-current-buffer first
                   (auto-async-byte-compile-mode -1))
                 (with-current-buffer second
                   (list
                    auto-async-byte-compile-mode
                    (memq
                     #'auto-async-byte-compile
                     after-save-hook)))))
            (kill-buffer first)
            (kill-buffer second)))"##,
        expect!["OK ((t (auto-async-byte-compile t)) (nil nil) nil (nil nil))"],
    )
}

fn auto_async_byte_compile_enable_helper_forces_mode_on_idempotently() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_enable_helper_forces_mode_on_idempotently",
        r##"(with-temp-buffer
          (list
           (enable-auto-async-byte-compile-mode)
           auto-async-byte-compile-mode
           (enable-auto-async-byte-compile-mode)
           auto-async-byte-compile-mode
           (length
            (seq-filter
             (lambda (function)
               (eq
                function
                #'auto-async-byte-compile))
             after-save-hook))))"##,
        expect!["OK (t t t t 1)"],
    )
}

fn auto_async_byte_compile_file_filter_matrix_uses_default_case_folding_and_exact_suffix_boundary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_file_filter_matrix_uses_default_case_folding_and_exact_suffix_boundary",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'aabc/doit)
                (lambda ()
                  (push buffer-file-name calls)
                  (list
                   :compiled
                   buffer-file-name))))
            (list
             (mapcar
              (lambda (file)
                (with-temp-buffer
                  (setq buffer-file-name file)
                  (list
                   file
                   (auto-async-byte-compile))))
              '(nil
                "init.el"
                "/workspace/module.el"
                "/workspace/MODULE.EL"
                "/workspace/module.el.gpg"
                "/workspace/module.el~"
                "/workspace/.el"
                "/workspace/notel"))
             (nreverse calls))))"##,
        expect![[
            r#"OK (((nil nil) ("init.el" (:compiled "init.el")) ("/workspace/module.el" (:compiled "/workspace/module.el")) ("/workspace/MODULE.EL" (:compiled "/workspace/MODULE.EL")) ("/workspace/module.el.gpg" nil) ("/workspace/module.el~" nil) ("/workspace/.el" (:compiled "/workspace/.el")) ("/workspace/notel" nil)) ("init.el" "/workspace/module.el" "/workspace/MODULE.EL" "/workspace/.el"))"#
        ]],
    )
}

fn auto_async_byte_compile_exclusion_regexp_prevents_matching_files_and_propagates_bad_regexps()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_exclusion_regexp_prevents_matching_files_and_propagates_bad_regexps",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'aabc/doit)
                (lambda ()
                  (push buffer-file-name calls)
                  :started)))
            (list
             (mapcar
              (lambda (case)
                (with-temp-buffer
                  (setq buffer-file-name
                        (car case)
                        auto-async-byte-compile-exclude-files-regexp
                        (cadr case))
                  (list
                   case
                   (auto-async-byte-compile-test-error-data
                    #'auto-async-byte-compile))))
              '(("/project/src/main.el" nil)
                ("/project/generated/out.el" "/generated/")
                ("/project/src/generated-name.el" "generated")
                ("/project/src/main.el" "")
                ("/project/src/main.el" "[broken")))
             (nreverse calls))))"##,
        expect![[
            r#"OK (((("/project/src/main.el" nil) (:ok :started)) (("/project/generated/out.el" "/generated/") (:ok nil)) (("/project/src/generated-name.el" "generated") (:ok nil)) (("/project/src/main.el" "") (:ok nil)) (("/project/src/main.el" "[broken") (:error invalid-regexp ("Unmatched [ or [^")))) ("/project/src/main.el"))"#
        ]],
    )
}

fn auto_async_byte_compile_real_save_runs_mode_hook_with_saved_file_contents() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_real_save_runs_mode_hook_with_saved_file_contents",
        r##"(let* ((root
                                 (getenv
                                  "NEOMACS_TEST_SANDBOX_ROOT"))
                                (file
                                 (expand-file-name
                                  "save-lifecycle.el"
                                  root))
                                calls
                                buffer)
          (unwind-protect
              (progn
                (setq buffer
                      (find-file-noselect file))
                (with-current-buffer buffer
                  (erase-buffer)
                  (insert
                   "(setq aabc-save-fixture :saved)\n")
                  (auto-async-byte-compile-mode 1)
                  (cl-letf
                      (((symbol-function 'aabc/doit)
                        (lambda ()
                          (push
                           (list
                            buffer-file-name
                            (buffer-substring-no-properties
                             (point-min)
                             (point-max))
                            (buffer-modified-p)
                            (file-exists-p
                             buffer-file-name)
                            (auto-async-byte-compile-test-read-file
                             buffer-file-name))
                           calls)
                          :queued)))
                    (save-buffer)
                    (list
                     calls
                     (buffer-modified-p)
                     (file-exists-p file)
                     (auto-async-byte-compile-test-read-file
                      file)))))
            (when
                (buffer-live-p buffer)
              (set-buffer-modified-p nil)
              (kill-buffer buffer))
            (when
                (file-exists-p file)
              (delete-file file))))"##,
        expect![[
            r#"OK ((("[ORACLE-SANDBOX]/save-lifecycle.el" "(setq aabc-save-fixture :saved)\n" nil t "(setq aabc-save-fixture :saved)\n")) nil t "(setq aabc-save-fixture :saved)\n")"#
        ]],
    )
    .fresh_process()
}

fn auto_async_byte_compile_real_save_without_mode_never_starts_compilation() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_real_save_without_mode_never_starts_compilation",
        r##"(let* ((root
                                 (getenv
                                  "NEOMACS_TEST_SANDBOX_ROOT"))
                                (file
                                 (expand-file-name
                                  "save-without-mode.el"
                                  root))
                                calls
                                buffer)
          (unwind-protect
              (progn
                (setq buffer
                      (find-file-noselect file))
                (with-current-buffer buffer
                  (erase-buffer)
                  (insert
                   "(setq aabc-no-mode :saved)\n")
                  (cl-letf
                      (((symbol-function 'aabc/doit)
                        (lambda ()
                          (push :unexpected calls))))
                    (save-buffer)
                    (list
                     calls
                     auto-async-byte-compile-mode
                     (memq
                      #'auto-async-byte-compile
                      after-save-hook)
                     (auto-async-byte-compile-test-read-file
                      file)))))
            (when
                (buffer-live-p buffer)
              (set-buffer-modified-p nil)
              (kill-buffer buffer))
            (when
                (file-exists-p file)
              (delete-file file))))"##,
        expect![[r#"OK (nil nil nil "(setq aabc-no-mode :saved)\n")"#]],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_async_byte_compile_mode_numeric_toggle_and_return_contract_match(),
        auto_async_byte_compile_mode_installs_and_removes_one_buffer_local_save_hook(),
        auto_async_byte_compile_mode_isolated_across_real_buffers(),
        auto_async_byte_compile_enable_helper_forces_mode_on_idempotently(),
        auto_async_byte_compile_file_filter_matrix_uses_default_case_folding_and_exact_suffix_boundary(),
        auto_async_byte_compile_exclusion_regexp_prevents_matching_files_and_propagates_bad_regexps(),
        auto_async_byte_compile_real_save_runs_mode_hook_with_saved_file_contents(),
        auto_async_byte_compile_real_save_without_mode_never_starts_compilation(),
    ]
}
