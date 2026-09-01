use expect_test::expect;

use super::ParityBatchCase;

fn real_git_project_keeps_aiken_buffers_project_scoped_and_auto_selected() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_git_project_keeps_aiken_buffers_project_scoped_and_auto_selected",
        r##"
(let* ((root (make-temp-file "aiken-project-" t))
       (default-directory (file-name-as-directory root))
       (source (expand-file-name "validators/payment.ak" root))
       (library (expand-file-name "lib/helpers.ak" root))
       source-buffer library-buffer)
  (unwind-protect
      (progn
        (make-directory (file-name-directory source) t)
        (make-directory (file-name-directory library) t)
        (with-temp-file (expand-file-name "aiken.toml" root)
          (insert "name = \"acme/payment\"\nversion = \"0.0.0\"\n"))
        (with-temp-file source
          (insert "validator payment { spend(_d, _r, _o, _t) { True } }\n"))
        (with-temp-file library
          (insert "pub fn positive(value: Int) { value > 0 }\n"))
        (call-process "git" nil nil nil "init" "-q")
        (setq source-buffer (find-file-noselect source)
              library-buffer (find-file-noselect library))
        (list
         (with-current-buffer source-buffer
           (list major-mode
                 (file-relative-name buffer-file-name root)
                 (equal
                  (file-name-as-directory
                   (project-root (project-current nil root)))
                  (file-name-as-directory root))))
         (with-current-buffer library-buffer
           (list major-mode
                 (file-relative-name buffer-file-name root)
                 (equal
                  (file-name-as-directory
                   (project-root (project-current nil root)))
                  (file-name-as-directory root))))
         (equal
          (with-current-buffer source-buffer default-directory)
          (file-name-directory source))))
    (when (buffer-live-p source-buffer) (kill-buffer source-buffer))
    (when (buffer-live-p library-buffer) (kill-buffer library-buffer))
    (delete-directory root t)))
"##,
        expect![[
            r#"OK ((aiken-mode "validators/payment.ak" t) (aiken-mode "lib/helpers.ak" t) t)"#
        ]],
    )
}

fn compile_workflow_routes_aiken_check_from_project_without_starting_tool() -> ParityBatchCase {
    ParityBatchCase::value(
        "compile_workflow_routes_aiken_check_from_project_without_starting_tool",
        r##"
(progn
  (setq aiken-mode-test-compile-event nil)
  (unwind-protect
      (let* ((root (make-temp-file "aiken-compile-" t))
             (default-directory (file-name-as-directory root)))
        (unwind-protect
            (progn
              (with-temp-file (expand-file-name "aiken.toml" root)
                (insert "name = \"acme/payment\"\n"))
              (with-temp-buffer
                (setq default-directory
                      (file-name-as-directory root))
                (aiken-mode)
                (setq-local compile-command "aiken check")
                (cl-letf
                    (((symbol-function 'compilation-start)
                      (lambda (command &optional mode name-function
                                       highlight-regexp)
                        (setq aiken-mode-test-compile-event
                              (list command mode
                                    (functionp name-function)
                                    highlight-regexp
                                    default-directory
                                    major-mode))
                        'fake-compilation-buffer)))
                  (list
                   (compile compile-command)
                   (let ((event aiken-mode-test-compile-event))
                     (setf (nth 4 event)
                           (equal (nth 4 event)
                                  (file-name-as-directory root)))
                     event)
                   compile-command))))
          (delete-directory root t)))
    (makunbound 'aiken-mode-test-compile-event)))
"##,
        expect![[
            r#"OK (fake-compilation-buffer ("aiken check" nil nil nil t aiken-mode) "aiken check")"#
        ]],
    )
}

fn user_formatter_hook_can_replace_buffer_through_mocked_aiken_cli_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "user_formatter_hook_can_replace_buffer_through_mocked_aiken_cli_boundary",
        r##"
(progn
  (setq aiken-mode-test-format-event nil)
  (defvar aiken-mode-test-call-process nil)
  (fset
   'aiken-mode-test-format-buffer
   (lambda ()
     (interactive)
     (let ((point-before (point)))
       (unless
           (zerop
            (funcall
             aiken-mode-test-call-process
             (point-min) (point-max)
             "aiken" t t nil "fmt" "-"))
         (error "aiken fmt failed"))
       (goto-char (min point-before (point-max))))))
  (unwind-protect
      (with-temp-buffer
        (aiken-mode)
        (insert "fn add(x:Int,y:Int){x+y}\n")
        (goto-char 8)
        (let ((aiken-mode-test-call-process
               (lambda (start end program delete destination
                              display &rest args)
                 (setq aiken-mode-test-format-event
                       (list
                        (buffer-substring-no-properties start end)
                        program delete destination display args))
                 (delete-region start end)
                 (insert "fn add(x: Int, y: Int) { x + y }\n")
                 0)))
          (aiken-mode-test-format-buffer)
          (list
           aiken-mode-test-format-event
           (buffer-string)
           (point)
           major-mode
           (buffer-modified-p))))
    (fmakunbound 'aiken-mode-test-format-buffer)
    (makunbound 'aiken-mode-test-format-event)))
"##,
        expect![[
            r#"OK (("fn add(x:Int,y:Int){x+y}\n" "aiken" t t nil ("fmt" "-")) "fn add(x: Int, y: Int) { x + y }\n" 8 aiken-mode t)"#
        ]],
    )
}

fn project_save_hook_formats_only_aiken_buffers_and_preserves_plain_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_save_hook_formats_only_aiken_buffers_and_preserves_plain_files",
        r##"
(progn
  (setq aiken-mode-test-hook-events nil)
  (fset
   'aiken-mode-test-format-on-save
   (lambda ()
     (when (derived-mode-p 'aiken-mode)
       (push (list (buffer-name) major-mode)
             aiken-mode-test-hook-events)
       (goto-char (point-min))
       (while (search-forward "x:Int" nil t)
         (replace-match "x: Int")))))
  (unwind-protect
      (let ((contract (generate-new-buffer "validator.ak"))
            (notes (generate-new-buffer "notes.txt")))
        (unwind-protect
            (progn
              (with-current-buffer contract
                (aiken-mode)
                (insert "fn valid(x:Int) { True }\n")
                (add-hook 'before-save-hook
                          #'aiken-mode-test-format-on-save nil t)
                (run-hooks 'before-save-hook))
              (with-current-buffer notes
                (text-mode)
                (insert "x:Int is documentation\n")
                (add-hook 'before-save-hook
                          #'aiken-mode-test-format-on-save nil t)
                (run-hooks 'before-save-hook))
              (list
               (nreverse aiken-mode-test-hook-events)
               (with-current-buffer contract (buffer-string))
               (with-current-buffer notes (buffer-string))))
          (kill-buffer contract)
          (kill-buffer notes)))
    (fmakunbound 'aiken-mode-test-format-on-save)
    (makunbound 'aiken-mode-test-hook-events)))
"##,
        expect![[
            r#"OK ((("validator.ak" aiken-mode)) "fn valid(x: Int) { True }\n" "x:Int is documentation\n")"#
        ]],
    )
}

fn compilation_error_text_retains_aiken_file_line_column_navigation_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "compilation_error_text_retains_aiken_file_line_column_navigation_metadata",
        r##"
(let* ((root (make-temp-file "aiken-errors-" t))
       (source (expand-file-name "validators/payment.ak" root)))
  (unwind-protect
      (progn
        (make-directory (file-name-directory source) t)
        (with-temp-file source
          (insert "validator payment {\n  spend(_d, _r, _o, _t) {\n    False\n  }\n}\n"))
        (with-temp-buffer
          (setq default-directory
                (file-name-as-directory root))
          (compilation-mode)
          (let ((inhibit-read-only t))
            (insert
             "Checking acme/payment\nvalidators/payment.ak:3:5: error: expected Bool\n"))
          (font-lock-ensure)
          (goto-char (point-min))
          (search-forward "validators/payment.ak:3:5")
          (let* ((message
                  (get-text-property
                   (- (point) (length "validators/payment.ak:3:5"))
                   'compilation-message))
                 (location
                  (and message
                       (compilation--message->loc message))))
            (list
             (buffer-substring-no-properties
              (point-min) (point-max))
             (and message t)
             (and location
                  (car (car (nth 2 location))))
             (and location (nth 1 location))
             (and location (nth 0 location))
             (get-text-property
              (- (point) (length "validators/payment.ak:3:5"))
              'face)))))
    (delete-directory root t)))
"##,
        expect![[
            r#"OK ("Checking acme/payment\nvalidators/payment.ak:3:5: error: expected Bool\n" t "validators/payment.ak" 3 5 font-lock-function-name-face)"#
        ]],
    )
}

fn mode_can_run_deterministic_process_filter_workflow_for_aiken_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_can_run_deterministic_process_filter_workflow_for_aiken_output",
        r##"
(progn
  (setq aiken-mode-test-process-events nil)
  (unwind-protect
      (with-temp-buffer
        (aiken-mode)
        (cl-letf
            (((symbol-function 'make-process)
              (lambda (&rest plist)
                (setq aiken-mode-test-process-events
                      (list
                       (plist-get plist :name)
                       (plist-get plist :command)
                       (eq (plist-get plist :buffer)
                           (current-buffer))
                       (functionp (plist-get plist :filter))
                       (functionp (plist-get plist :sentinel))))
                'fake-aiken-process)))
          (let ((process
                 (make-process
                  :name "aiken-check"
                  :buffer (current-buffer)
                  :command '("aiken" "check")
                  :filter #'comint-output-filter
                  :sentinel #'ignore)))
            (list process aiken-mode-test-process-events
                  major-mode (buffer-string)))))
    (makunbound 'aiken-mode-test-process-events)))
"##,
        expect![[
            r#"OK (fake-aiken-process ("aiken-check" ("aiken" "check") t t t) aiken-mode "")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        real_git_project_keeps_aiken_buffers_project_scoped_and_auto_selected(),
        compile_workflow_routes_aiken_check_from_project_without_starting_tool(),
        user_formatter_hook_can_replace_buffer_through_mocked_aiken_cli_boundary(),
        project_save_hook_formats_only_aiken_buffers_and_preserves_plain_files(),
        compilation_error_text_retains_aiken_file_line_column_navigation_metadata(),
        mode_can_run_deterministic_process_filter_workflow_for_aiken_output(),
    ]
}
