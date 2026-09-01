use expect_test::expect;

use super::ParityBatchCase;

fn auto_async_byte_compile_process_args_without_init_file_preserve_exact_load_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_process_args_without_init_file_preserve_exact_load_path",
        r##"(let ((load-path
                                '("/fixture/first"
                                  "/fixture/with space"
                                  "/fixture/密钥"))
                               (auto-async-byte-compile-init-file
                                "/fixture/missing-init.el"))
          (cl-letf
              (((symbol-function 'aabc/emacs-command)
                (lambda ()
                  "/fixture/emacs")))
            (aabc/byte-compile-start-process-args
             "/workspace/source file.el")))"##,
        expect![[
            r#"OK ("/fixture/emacs" "-Q" "-batch" "--eval" "(setq load-path (cons \".\" '(\"/fixture/first\" \"/fixture/with space\" \"/fixture/密钥\")))" "-f" "batch-byte-compile" "/workspace/source file.el")"#
        ]],
    )
}

fn auto_async_byte_compile_process_args_include_existing_init_file_in_exact_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_process_args_include_existing_init_file_in_exact_order",
        r##"(let* ((root
                                 (getenv
                                  "NEOMACS_TEST_SANDBOX_ROOT"))
                                (init
                                 (expand-file-name
                                  "fixture init.el"
                                  root))
                                (load-path
                                 '("/one"
                                   "/two"))
                                (auto-async-byte-compile-init-file
                                 init))
          (unwind-protect
              (progn
                (with-temp-file init
                  (insert
                   "(setq fixture-init-loaded t)\n"))
                (cl-letf
                    (((symbol-function 'aabc/emacs-command)
                      (lambda ()
                        "/fixture/editor")))
                  (let ((arguments
                         (aabc/byte-compile-start-process-args
                          "/project/module.el")))
                    (list
                     arguments
                     (equal
                      (nth 5 arguments)
                      "-l")
                     (equal
                      (nth 6 arguments)
                      init)))))
            (when
                (file-exists-p init)
              (delete-file init))))"##,
        expect![[
            r#"OK (("/fixture/editor" "-Q" "-batch" "--eval" "(setq load-path (cons \".\" '(\"/one\" \"/two\")))" "-l" "[ORACLE-SANDBOX]/fixture init.el" "-f" "batch-byte-compile" "/project/module.el") t t)"#
        ]],
    )
}

fn auto_async_byte_compile_emacs_command_returns_first_command_line_argument_verbatim()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_emacs_command_returns_first_command_line_argument_verbatim",
        r##"(mapcar
          (lambda (arguments)
            (let ((command-line-args
                   arguments))
              (list
               arguments
               (aabc/emacs-command))))
          '(nil
            ("emacs")
            ("/path/with spaces/emacs" "-Q")
            (neomacs-symbol "--batch")))"##,
        expect![[
            r#"OK ((nil nil) (("emacs") "emacs") (("/path/with spaces/emacs" "-Q") "/path/with spaces/emacs") ((neomacs-symbol "--batch") neomacs-symbol))"#
        ]],
    )
}

fn auto_async_byte_compile_doit_clears_result_and_forwards_exact_process_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_doit_clears_result_and_forwards_exact_process_contract",
        r##"(let ((buffer-file-name
                                "/project/nested/fixture module.el")
                               start-calls
                               sentinel-calls)
          (with-current-buffer
              (get-buffer-create
               aabc/result-buffer)
            (erase-buffer)
            (insert
             "stale compiler output"))
          (cl-letf
              (((symbol-function
                 'aabc/byte-compile-start-process-args)
                (lambda (file)
                  (list
                   "/fixture/editor"
                   "--fixture"
                   file)))
               ((symbol-function 'start-process)
                (lambda (&rest arguments)
                  (push arguments start-calls)
                  'fixture-process))
               ((symbol-function 'set-process-sentinel)
                (lambda (process sentinel)
                  (push
                   (list process sentinel)
                   sentinel-calls)
                  :sentinel-installed)))
            (list
             (aabc/doit)
             (nreverse start-calls)
             (nreverse sentinel-calls)
             (with-current-buffer
                 aabc/result-buffer
               (buffer-string)))))"##,
        expect![[
            r#"OK (:sentinel-installed (("auto-async-byte-compile fixture module.el" " *auto-async-byte-compile*" "/fixture/editor" "--fixture" "/project/nested/fixture module.el")) ((fixture-process aabc/process-sentinel)) "")"#
        ]],
    )
}

fn auto_async_byte_compile_doit_start_failure_leaves_existing_result_buffer_empty()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_doit_start_failure_leaves_existing_result_buffer_empty",
        r##"(let ((buffer-file-name
                                "/project/failing.el"))
          (with-current-buffer
              (get-buffer-create
               aabc/result-buffer)
            (erase-buffer)
            (insert
             "old diagnostics"))
          (cl-letf
              (((symbol-function
                 'aabc/byte-compile-start-process-args)
                (lambda (_)
                  '("/missing/editor")))
               ((symbol-function 'start-process)
                (lambda (&rest _)
                  (error
                   "fixture process start failed"))))
            (list
             (auto-async-byte-compile-test-error-data
              #'aabc/doit)
             (with-current-buffer
                 aabc/result-buffer
               (buffer-string)))))"##,
        expect![[r#"OK ((:error error ("fixture process start failed")) "")"#]],
    )
}

fn auto_async_byte_compile_real_save_launches_batch_compiler_and_creates_loadable_elc()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_real_save_launches_batch_compiler_and_creates_loadable_elc",
        r##"(let* ((root
                                 (getenv
                                  "NEOMACS_TEST_SANDBOX_ROOT"))
                                (file
                                 (expand-file-name
                                  "real async fixture.el"
                                  root))
                                (compiled
                                 (concat file "c"))
                                (auto-async-byte-compile-init-file
                                 (expand-file-name
                                  "missing-init.el"
                                  root))
                                (auto-async-byte-compile-display-function
                                 #'ignore)
                                (auto-async-byte-compile-suppress-warnings
                                 t)
                                process
                                buffer)
          (unwind-protect
              (progn
                (setq buffer
                      (find-file-noselect file))
                (with-current-buffer buffer
                  (erase-buffer)
                  (insert
                   ";;; -*- lexical-binding: t; -*-\n"
                   "(defun aabc-real-compiled-value ()\n"
                   "  (list :compiled 42 \"密钥\"))\n")
                  (auto-async-byte-compile-mode 1)
                  (save-buffer)
                  (setq process
                        (get-process
                         (format
                          "auto-async-byte-compile %s"
                          (file-name-nondirectory
                           file)))))
                (let ((wait-result
                       (and
                        process
                        (auto-async-byte-compile-test-wait
                         process))))
                  (list
                   (processp process)
                   wait-result
                   (file-exists-p compiled)
                   (and
                    (file-exists-p compiled)
                    (file-attribute-size
                     (file-attributes compiled))
                    t)
                   (and
                    (file-exists-p compiled)
                    (progn
                      (load compiled nil t)
                      (aabc-real-compiled-value)))
                   (with-current-buffer
                       aabc/result-buffer
                     (buffer-string)))))
            (when
                (buffer-live-p buffer)
              (set-buffer-modified-p nil)
              (kill-buffer buffer))
            (when
                (and
                 process
                 (process-live-p process))
              (delete-process process))
            (when
                (file-exists-p compiled)
              (delete-file compiled))
            (when
                (file-exists-p file)
              (delete-file file))
            (when
                (fboundp
                 'aabc-real-compiled-value)
              (fmakunbound
               'aabc-real-compiled-value))))"##,
        expect![[r#"OK (t (t exit 0) t t (:compiled 42 "密钥") "")"#]],
    )
    .fresh_process()
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_async_byte_compile_process_args_without_init_file_preserve_exact_load_path(),
        auto_async_byte_compile_process_args_include_existing_init_file_in_exact_order(),
        auto_async_byte_compile_emacs_command_returns_first_command_line_argument_verbatim(),
        auto_async_byte_compile_doit_clears_result_and_forwards_exact_process_contract(),
        auto_async_byte_compile_doit_start_failure_leaves_existing_result_buffer_empty(),
        auto_async_byte_compile_real_save_launches_batch_compiler_and_creates_loadable_elc(),
    ]
}
