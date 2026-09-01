use expect_test::expect;

use super::ParityBatchCase;

fn auto_compile_start_creates_loadable_bytecode_with_real_runtime_behavior() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_start_creates_loadable_bytecode_with_real_runtime_behavior",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/real-library.el"
                  "(defun auto-compile-real-value (x) (+ x 37))\n(provide 'auto-compile-real-library)\n"))
                (dest
                 (auto-compile-test-dest source))
                (result
                 (auto-compile-byte-compile
                  source t)))
         (when (featurep 'auto-compile-real-library)
           (unload-feature
            'auto-compile-real-library t))
         (when (fboundp 'auto-compile-real-value)
           (fmakunbound 'auto-compile-real-value))
         (let ((loaded
                (load dest nil nil t)))
           (list
            result
            (file-exists-p dest)
            (> (file-attribute-size
                (file-attributes dest))
               0)
            loaded
            (featurep
             'auto-compile-real-library)
            (auto-compile-real-value 5))))"##,
        expect!["OK (t t t t t 42)"],
    )
}

fn auto_compile_without_start_does_not_create_missing_destination() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_without_start_does_not_create_missing_destination",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/not-enabled.el"
                  "(provide 'auto-compile-not-enabled)\n"))
                (dest
                 (auto-compile-test-dest source)))
         (list
          (file-exists-p dest)
          (auto-compile-byte-compile source)
          (file-exists-p dest)
          (get-file-buffer source)))"##,
        expect!["OK (nil nil nil nil)"],
    )
}

fn auto_compile_existing_destination_is_rebuilt_to_run_new_source_behavior() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_existing_destination_is_rebuilt_to_run_new_source_behavior",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/update.el"
                  "(defun auto-compile-update-value () 'old)\n(provide 'auto-compile-update)\n"))
                (dest
                 (auto-compile-test-dest source)))
         (byte-compile-file source)
         (auto-compile-test-set-time dest 1000)
         (auto-compile-test-write
          "compile/update.el"
          "(defun auto-compile-update-value () 'new)\n(provide 'auto-compile-update)\n")
         (auto-compile-test-set-time source 2000)
         (let ((result
                (auto-compile-byte-compile source)))
           (when (featurep 'auto-compile-update)
             (unload-feature 'auto-compile-update t))
           (when (fboundp
                  'auto-compile-update-value)
             (fmakunbound
              'auto-compile-update-value))
           (load dest nil nil t)
           (list
            result
            (file-newer-than-file-p dest source)
            (auto-compile-update-value))))"##,
        expect!["OK (t t new)"],
    )
}

fn auto_compile_after_save_rebuilds_enabled_visited_library_end_to_end() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_after_save_rebuilds_enabled_visited_library_end_to_end",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/save-hook.el"
                  "(defun auto-compile-save-value () 1)\n(provide 'auto-compile-save-hook)\n"))
                (dest
                 (auto-compile-test-dest source))
                (buffer
                 (find-file-noselect source)))
         (byte-compile-file source)
         (auto-compile-test-set-time dest 1000)
         (unwind-protect
             (with-current-buffer buffer
               (emacs-lisp-mode)
               (auto-compile-mode 1)
               (erase-buffer)
               (insert
                "(defun auto-compile-save-value () 42)\n(provide 'auto-compile-save-hook)\n")
               (save-buffer)
               (when (featurep
                      'auto-compile-save-hook)
                 (unload-feature
                  'auto-compile-save-hook t))
               (when (fboundp
                      'auto-compile-save-value)
                 (fmakunbound
                  'auto-compile-save-value))
               (load dest nil nil t)
               (list
                (file-exists-p dest)
                (buffer-modified-p)
                (and
                 (memq
                  #'auto-compile-byte-compile
                  after-save-hook)
                 t)
                (auto-compile-save-value)))
           (kill-buffer buffer)))"##,
        expect!["OK (t nil t 42)"],
    )
}

fn auto_compile_no_byte_compile_cookie_returns_marker_without_destination() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_no_byte_compile_cookie_returns_marker_without_destination",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/no-byte.el"
                  ";;; -*- no-byte-compile: t -*-\n(provide 'auto-compile-no-byte)\n"))
                (dest
                 (auto-compile-test-dest source)))
         (list
          (auto-compile-byte-compile source t)
          (file-exists-p dest)
          (featurep 'auto-compile-no-byte)))"##,
        expect!["OK (no-byte-compile nil nil)"],
    )
}

fn auto_compile_inhibit_hook_short_circuits_before_any_file_is_created() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_inhibit_hook_short_circuits_before_any_file_is_created",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/inhibited.el"
                  "(provide 'auto-compile-inhibited)\n"))
                (dest
                 (auto-compile-test-dest source))
                (events nil)
                (auto-compile-inhibit-compile-hook
                 (list
                  (lambda ()
                    (push 'first events)
                    nil)
                  (lambda ()
                    (push 'inhibit events)
                    'stop)
                  (lambda ()
                    (push 'never events)
                    nil))))
         (list
          (auto-compile-byte-compile source t)
          (nreverse events)
          (file-exists-p dest)))"##,
        expect!["OK (nil (first inhibit) nil)"],
    )
}

fn auto_compile_unbalanced_visited_source_marks_retry_and_modified_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_unbalanced_visited_source_marks_retry_and_modified_state",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/unbalanced.el"
                  "(defun auto-compile-unbalanced ()\n  (list 1 2 3)\n"))
                (dest
                 (auto-compile-test-dest source))
                (buffer
                 (find-file-noselect source))
                (auto-compile-ding nil)
                (auto-compile-check-parens t)
                (auto-compile-visit-failed t)
                (auto-compile-mark-failed-modified t))
         (unwind-protect
             (with-current-buffer buffer
               (list
                (auto-compile-byte-compile source t)
                (file-exists-p dest)
                auto-compile-pretend-byte-compiled
                (buffer-modified-p)
                (current-message)))
           (set-buffer-modified-p nil)
           (kill-buffer buffer)))"##,
        expect!["OK (nil nil t t nil)"],
    )
}

fn auto_compile_byte_compiler_nil_result_preserves_stale_destination_without_retry_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_byte_compiler_nil_result_preserves_stale_destination_without_retry_state",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/failure.el"
                  "(defun auto-compile-failure () 1)\n(provide 'auto-compile-failure)\n"))
                (dest
                 (auto-compile-test-dest source)))
         (byte-compile-file source)
         (auto-compile-test-write
          "compile/failure.el"
          "(defun auto-compile-failure () (let ((x 1)) x)\n")
         (let* ((buffer
                 (find-file-noselect source))
                (auto-compile-ding nil)
                (auto-compile-check-parens nil)
                (auto-compile-visit-failed t)
                (auto-compile-mark-failed-modified nil))
           (unwind-protect
               (with-current-buffer buffer
                 (list
                  (auto-compile-byte-compile source t)
                  (file-exists-p dest)
                  auto-compile-pretend-byte-compiled
                  (buffer-modified-p)
                  (current-message)))
             (kill-buffer buffer))))"##,
        expect!["OK (nil t nil nil nil)"],
    )
}

fn auto_compile_delete_destination_clears_retry_marker_in_visiting_source_buffer() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_compile_delete_destination_clears_retry_marker_in_visiting_source_buffer",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/delete.el"
                  "(provide 'auto-compile-delete)\n"))
                (dest
                 (auto-compile-test-write
                  "compile/delete.elc"
                  "placeholder"))
                (buffer
                 (find-file-noselect source)))
         (unwind-protect
             (with-current-buffer buffer
               (setq auto-compile-pretend-byte-compiled t)
               (auto-compile-delete-dest dest)
               (list
                (file-exists-p dest)
                (local-variable-p
                 'auto-compile-pretend-byte-compiled)
                auto-compile-pretend-byte-compiled
                (current-message)))
           (kill-buffer buffer)))"##,
        expect!["OK (nil nil nil nil)"],
    )
}

fn auto_compile_delete_failure_is_contained_and_dings_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_delete_failure_is_contained_and_dings_once",
        r##"(let* ((directory
                 (auto-compile-test-path
                  "compile/not-a-file.elc"))
                (dings 0)
                (auto-compile-ding t))
         (make-directory directory t)
         (cl-letf (((symbol-function 'ding)
                    (lambda (&rest _)
                      (setq dings (1+ dings)))))
           (list
            (auto-compile-delete-dest directory)
            (file-directory-p directory)
            dings
            (current-message))))"##,
        expect![[
            r#"OK ("Deleting [ORACLE-SANDBOX]/auto-compile-fixture/compile/not-a-file.elc...failed" t 1 nil)"#
        ]],
    )
}

fn auto_compile_warning_advice_counts_real_byte_compiler_diagnostics_per_buffer() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_compile_warning_advice_counts_real_byte_compiler_diagnostics_per_buffer",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/warning.el"
                  "(defun auto-compile-warning () (auto-compile-undefined-function 1))\n(provide 'auto-compile-warning)\n"))
                (buffer
                 (find-file-noselect source))
                (auto-compile-display-buffer nil)
                (auto-compile-verbose nil))
         (unwind-protect
             (with-current-buffer buffer
               (emacs-lisp-mode)
               (list
                (auto-compile-byte-compile source t)
                (> auto-compile-warnings 0)
                auto-compile-warnings
                (file-exists-p
                 (auto-compile-test-dest
                  source))))
           (kill-buffer buffer)))"##,
        expect!["OK (t t 2 t)"],
    )
}

fn auto_compile_single_file_toggle_creates_then_removes_loadable_destination() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_single_file_toggle_creates_then_removes_loadable_destination",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/toggle.el"
                  "(defun auto-compile-toggle-value () 'ready)\n(provide 'auto-compile-toggle-library)\n"))
                (dest
                 (auto-compile-test-dest source)))
         (toggle-auto-compile source 'start)
         (let ((after-start
                (list
                 (file-exists-p dest)
                 (> (file-attribute-size
                     (file-attributes dest))
                    0))))
           (toggle-auto-compile source 'quit)
           (list
            after-start
            (file-exists-p dest)
            (current-message))))"##,
        expect!["OK ((t t) nil nil)"],
    )
}

fn auto_compile_native_option_dispatches_only_after_successful_byte_compile() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_native_option_dispatches_only_after_successful_byte_compile",
        r##"(let* ((source
                 (auto-compile-test-write
                  "compile/native.el"
                  "(provide 'auto-compile-native)\n"))
                (events nil)
                (already-provided
                 (featurep 'native-compile))
                (auto-compile-native-compile t))
         (unwind-protect
             (progn
               (provide 'native-compile)
               (cl-letf (((symbol-function
                           'auto-compile--byte-compile-file)
                          (lambda (file)
                            (push
                             (list 'byte
                                   (file-name-nondirectory
                                    file))
                             events)
                            t))
                         ((symbol-function
                           'native-comp-available-p)
                          (lambda () t))
                         ((symbol-function
                           'native-compile-async)
                          (lambda (file &rest _)
                            (push
                             (list 'native
                                   (file-name-nondirectory
                                    file))
                             events)
                            'queued)))
                 (list
                  (auto-compile-byte-compile
                   source t)
                  (nreverse events))))
           (unless already-provided
             (setq features
                   (delq 'native-compile
                         features)))))"##,
        expect![[r#"OK (t ((byte "native.el") (native "native.el")))"#]],
    )
}

pub(super) fn compilation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_compile_start_creates_loadable_bytecode_with_real_runtime_behavior(),
        auto_compile_without_start_does_not_create_missing_destination(),
        auto_compile_existing_destination_is_rebuilt_to_run_new_source_behavior(),
        auto_compile_after_save_rebuilds_enabled_visited_library_end_to_end(),
        auto_compile_no_byte_compile_cookie_returns_marker_without_destination(),
        auto_compile_inhibit_hook_short_circuits_before_any_file_is_created(),
        auto_compile_unbalanced_visited_source_marks_retry_and_modified_state(),
        auto_compile_byte_compiler_nil_result_preserves_stale_destination_without_retry_state(),
        auto_compile_delete_destination_clears_retry_marker_in_visiting_source_buffer(),
        auto_compile_delete_failure_is_contained_and_dings_once(),
        auto_compile_warning_advice_counts_real_byte_compiler_diagnostics_per_buffer(),
        auto_compile_single_file_toggle_creates_then_removes_loadable_destination(),
        auto_compile_native_option_dispatches_only_after_successful_byte_compile(),
    ]
}
