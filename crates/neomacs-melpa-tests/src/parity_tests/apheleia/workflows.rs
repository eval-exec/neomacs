use expect_test::expect;

use super::ParityBatchCase;

fn apheleia_ports_upstream_word_replacement_workflow_and_keeps_point_on_the_same_word()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_ports_upstream_word_replacement_workflow_and_keeps_point_on_the_same_word",
        r##"(with-temp-buffer
         (insert
          "The quick brown fox jumped over the lazy dog.")
         (goto-char
          (point-min))
         (search-forward
          "brown")
         (backward-char 3)
         (let ((apheleia-formatters
                '((study
                   . ("sed"
                      "-e"
                      "s/quick/slow/"
                      "-e"
                      "s/lazy/studious/")))))
           (list
            (apheleia-test-format-buffer
             'study)
            (buffer-string)
            (point)
            (current-word)
            (current-column)
            (buffer-modified-p))))"##,
        expect![[
            r#"OK ((:error nil) "The slow brown fox jumped over the studious dog." 12 "brown" 11 t)"#
        ]],
    )
}

fn apheleia_preserves_two_displayed_windows_point_mark_and_mark_ring_through_a_real_patch()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_preserves_two_displayed_windows_point_mark_and_mark_ring_through_a_real_patch",
        r##"(save-window-excursion
         (let ((buffer
                (generate-new-buffer
                 "apheleia-displayed-source"))
               first-window
               second-window)
           (unwind-protect
               (progn
                 (with-current-buffer buffer
                   (dotimes (index 50)
                     (insert
                      (if
                          (= index 20)
                          (format "    line %02d\n" index)
                        (format "line %02d\n" index))))
                   (goto-char
                    (point-min))
                   (forward-line 30)
                   (push-mark
                    (point)
                    t
                    t)
                   (goto-char
                    (point-min))
                   (forward-line 35)
                   (push-mark
                    (point)
                    t
                    t))
                 (delete-other-windows)
                 (switch-to-buffer buffer)
                 (setq first-window
                       (selected-window)
                       second-window
                       (split-window-right))
                 (set-window-buffer
                  second-window
                  buffer)
                 (with-current-buffer buffer
                   (goto-char
                    (point-min))
                   (forward-line 10)
                   (set-window-start
                    first-window
                    (point))
                   (set-window-start
                    second-window
                    (point))
                   (goto-char
                    (point-min))
                   (forward-line 30)
                   (forward-char 2)
                   (set-window-point
                    second-window
                    (point))
                   (goto-char
                    (point-min))
                   (forward-line 20)
                   (forward-char 4)
                   (set-window-point
                    first-window
                    (point)))
                 (with-selected-window first-window
                   (let ((apheleia-formatters
                          '((remove-indent
                             . ("sed"
                                "s/^    line 20$/line 20/")))))
                     (let ((callback
                            (apheleia-test-format-buffer
                             'remove-indent)))
                       (with-current-buffer buffer
                         (list
                          :callback callback
                          :changed-line
                          (save-excursion
                            (goto-char
                             (point-min))
                            (forward-line 20)
                            (buffer-substring-no-properties
                             (line-beginning-position)
                             (line-end-position)))
                          :point
                          (list
                           (line-number-at-pos)
                           (current-column)
                           (current-word))
                          :mark
                          (save-excursion
                            (goto-char
                             (mark))
                            (list
                             (line-number-at-pos)
                             (current-column)))
                          :mark-active mark-active
                          :mark-ring
                          (mapcar
                           (lambda (marker)
                             (save-excursion
                               (goto-char marker)
                               (list
                                (line-number-at-pos)
                                (current-column))))
                           mark-ring)
                          :windows
                          (mapcar
                           (lambda (window)
                             (list
                              :start
                              (line-number-at-pos
                               (window-start window))
                              :point
                              (line-number-at-pos
                               (window-point window))
                              :column
                              (save-excursion
                                (goto-char
                                 (window-point window))
                                (current-column))))
                           (list
                            first-window
                            second-window))
                          :modified
                          (buffer-modified-p)))))))
             (when
                 (buffer-live-p buffer)
               (with-current-buffer buffer
                 (set-buffer-modified-p nil))
               (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (:callback (:error nil) :changed-line "line 20" :point (21 0 "line") :mark (36 0) :mark-active t :mark-ring ((31 0)) :windows ((:start 11 :point 21 :column 0) (:start 11 :point 31 :column 2)) :modified t)"#
        ]],
    )
}

fn apheleia_ports_upstream_line_reordering_workflow_without_moving_point_from_line_two()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_ports_upstream_line_reordering_workflow_without_moving_point_from_line_two",
        r##"(with-temp-buffer
         (insert
          "line one\n"
          "line two with cursor\n"
          "line three\n"
          "line four moves first\n")
         (goto-char
          (point-min))
         (forward-line 1)
         (search-forward
          "cursor")
         (let ((apheleia-formatters
                '((move-fourth
                   . ("awk"
                      "{ lines[NR] = $0 } END { print lines[4]; for (i = 1; i <= 3; i++) print lines[i] }")))))
           (list
            (apheleia-test-format-buffer
             'move-fourth)
            (buffer-string)
            (line-number-at-pos)
            (current-column)
            (buffer-substring-no-properties
             (line-beginning-position)
             (line-end-position)))))"##,
        expect![[
            r#"OK ((:error nil) "line four moves first\nline one\nline two with cursor\nline three\n" 3 20 "line two with cursor")"#
        ]],
    )
}

fn apheleia_ports_upstream_whitespace_insertion_alignment_case_at_an_expression() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apheleia_ports_upstream_whitespace_insertion_alignment_case_at_an_expression",
        r##"(with-temp-buffer
         (insert
          "alpha\n"
          "a=calculate(value)\n"
          "omega\n")
         (goto-char
          (point-min))
         (forward-line 1)
         (search-forward
          "calculate")
         (let ((apheleia-formatters
                '((space-expression
                   . ("sed"
                      "s/^a=/    a = /")))))
           (list
            (apheleia-test-format-buffer
             'space-expression)
            (buffer-string)
            (line-number-at-pos)
            (current-column)
            (current-word))))"##,
        expect![[
            r#"OK ((:error nil) "alpha\n    a = calculate(value)\nomega\n" 2 17 "calculate")"#
        ]],
    )
}

fn apheleia_chains_two_real_processes_in_order_and_emits_one_hook_event_per_formatter()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_chains_two_real_processes_in_order_and_emits_one_hook_event_per_formatter",
        r##"(with-temp-buffer
         (insert
          "alpha beta\n"
           "beta gamma\n")
         (let ((apheleia-test-hook-events nil)
               (apheleia-formatters
                '((uppercase
                   . ("tr"
                      "[:lower:]"
                      "[:upper:]"))
                  (rename
                   . ("sed"
                      "s/BETA/DELTA/g"))))
               (apheleia-formatter-exited-hook
                '((lambda (&rest properties)
                    (setq apheleia-test-hook-events
                          (append
                           apheleia-test-hook-events
                           (list
                            (list
                             (plist-get
                              properties
                              :formatter)
                             (plist-get
                              properties
                              :error)
                             (and
                              (plist-get
                               properties
                               :log)
                              t)))))))))
           (list
            (apheleia-test-format-buffer
             '(uppercase rename))
            (buffer-string)
            apheleia-test-hook-events)))"##,
        expect![[
            r#"OK ((:error nil) "ALPHA DELTA\nDELTA GAMMA\n" ((uppercase nil nil) (rename nil nil)))"#
        ]],
    )
}

fn apheleia_input_output_and_inplace_placeholders_drive_real_file_based_formatters()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_input_output_and_inplace_placeholders_drive_real_file_based_formatters",
        r##"(mapcar
         (lambda (spec)
           (with-temp-buffer
             (rename-buffer
              (format
               "apheleia-%s.demo"
               (car spec))
              t)
             (insert
              "mixed Case\n"
              "second Line\n")
             (let ((apheleia-formatters
                    (list
                     (cons
                      (car spec)
                      (cadr spec)))))
               (list
                (car spec)
                (apheleia-test-format-buffer
                 (car spec))
                (buffer-string)))))
         '((input-file
            ("sh"
             "-c"
             "tr '[:lower:]' '[:upper:]' < \"$1\""
             "formatter"
             input))
           (output-file
            ("sh"
             "-c"
             "tr '[:lower:]' '[:upper:]' > \"$1\""
             "formatter"
             output))
           (inplace-file
            ("sh"
             "-c"
             "tr '[:lower:]' '[:upper:]' < \"$1\" > \"$1.next\" && mv \"$1.next\" \"$1\""
             "formatter"
             inplace))))"##,
        expect![[
            r#"OK ((input-file (:error nil) "MIXED CASE\nSECOND LINE\n") (output-file (:error nil) "MIXED CASE\nSECOND LINE\n") (inplace-file (:error nil) "MIXED CASE\nSECOND LINE\n"))"#
        ]],
    )
}

fn apheleia_lisp_formatter_receives_real_context_and_can_transform_chained_scratch_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_lisp_formatter_receives_real_context_and_can_transform_chained_scratch_text",
        r##"(progn
         (cl-defun apheleia-test-lisp-formatter
             (&key buffer scratch formatter
                   remote async callback
                   &allow-other-keys)
           (unless
               (and
                (equal
                 (buffer-name buffer)
                 "apheleia-lisp-original")
                (not remote)
                async
                (eq formatter 'lisp-transform)
                (equal
                 (with-current-buffer scratch
                   (buffer-string))
                 "ALPHA BETA\n"))
             (error "Custom formatter received the wrong context"))
           (with-current-buffer scratch
             (goto-char
              (point-min))
             (while
                 (search-forward
                  "ALPHA"
                  nil
                  t)
               (replace-match
                "OMEGA"
                t
                t)))
           (funcall callback))
         (with-temp-buffer
           (rename-buffer
            "apheleia-lisp-original"
            t)
           (insert
            "alpha beta\n")
           (let ((apheleia-formatters
                  '((upper
                     . ("tr"
                        "[:lower:]"
                        "[:upper:]"))
                    (lisp-transform
                     . apheleia-test-lisp-formatter))))
             (list
              (apheleia-test-format-buffer
               '(upper lisp-transform))
              (buffer-string)))))"##,
        expect![[r#"OK ((:error nil) "OMEGA BETA\n")"#]],
    )
}

fn apheleia_builtin_lisp_formatter_reindents_a_practical_function_without_losing_point()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_builtin_lisp_formatter_reindents_a_practical_function_without_losing_point",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert
          "(defun example (items)\n"
          "(mapcar (lambda (item)\n"
          "(when item\n"
          "(list :value item)))\n"
          "items))\n")
         (goto-char
          (point-min))
         (search-forward
          ":value")
         (let ((apheleia-formatters
                '((lisp-indent
                   . apheleia-indent-lisp-buffer))))
           (list
            (apheleia-test-format-buffer
             'lisp-indent)
            (buffer-string)
            (line-number-at-pos)
            (current-column)
            (current-word))))"##,
        expect![[
            r#"OK ((:error nil) "(defun example (items)\n  (mapcar (lambda (item)\n\11    (when item\n\11      (list :value item)))\n\11  items))\n" 4 26 ":value")"#
        ]],
    )
}

fn apheleia_mode_formats_and_resaves_a_real_file_after_save() -> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_mode_formats_and_resaves_a_real_file_after_save",
        r##"(let* ((root
                  (apheleia-test-root
                   "apheleia-save"))
                 (path
                  (expand-file-name
                   "project/source.txt"
                   root))
                 (buffer nil))
         (unwind-protect
             (progn
               (apheleia-test-cleanup root)
               (make-directory
                (file-name-directory path)
                t)
               (with-temp-file path
                 (insert
                  "first line\n"
                  "mixed case\n"))
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (let ((apheleia-formatters
                        '((upper
                           . ("tr"
                              "[:lower:]"
                              "[:upper:]"))))
                       (apheleia-formatter
                        'upper)
                       (apheleia-post-format-hook
                        '((lambda ()
                            (setq apheleia-test-hook-events
                                  (list
                                   (buffer-string)
                                   (line-number-at-pos)
                                   (current-column)
                                   (current-word)
                                   (buffer-modified-p)))))))
                   (setq apheleia-test-hook-events
                         :not-called)
                   (apheleia-mode 1)
                   (goto-char
                    (point-max))
                   (insert
                    "saved addition\n")
                   (goto-char
                    (point-min))
                   (search-forward "case")
                   (backward-char 2)
                   (save-buffer)
                   (apheleia-test-await
                    (lambda ()
                      (not
                       (eq apheleia-test-hook-events
                           :not-called)))
                    "Apheleia post-format hook")
                   (list
                    :hook
                    apheleia-test-hook-events
                    :disk
                    (apheleia-test-read-file
                     path)
                    :buffer
                    (buffer-string)
                    :point
                    (list
                     (line-number-at-pos)
                     (current-column)
                     (current-word))
                    :modified
                    (buffer-modified-p)))))
           (when
               (buffer-live-p buffer)
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer))
           (apheleia-test-cleanup root)))"##,
        expect![[
            r#"OK (:hook ("FIRST LINE\nMIXED CASE\nSAVED ADDITION\n" 2 8 "CASE" nil) :disk "FIRST LINE\nMIXED CASE\nSAVED ADDITION\n" :buffer "FIRST LINE\nMIXED CASE\nSAVED ADDITION\n" :point (2 8 "CASE") :modified nil)"#
        ]],
    )
}

fn apheleia_aborts_delayed_formatting_when_the_user_edits_the_buffer_in_flight() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apheleia_aborts_delayed_formatting_when_the_user_edits_the_buffer_in_flight",
        r##"(with-temp-buffer
         (insert
          "original text\n")
         (let ((apheleia-formatters
                '((delayed
                   . ("sh"
                      "-c"
                      "sleep 0.2; tr '[:lower:]' '[:upper:]'")))))
           (setq apheleia-test-callback-result
                 :not-called)
           (apheleia-format-buffer
            'delayed
            nil
            :callback
            (lambda (&rest properties)
              (setq apheleia-test-callback-result
                    properties)))
           (goto-char
            (point-max))
           (insert
            "user edit\n")
           (list
            (apheleia-test-await-callback)
            (buffer-string)
            (buffer-modified-p))))"##,
        expect![[
            r#"OK ((:error (error . "Contents have changed")) "original text\nuser edit\n" t)"#
        ]],
    )
}

fn apheleia_surfaces_unknown_and_missing_formatters_without_modifying_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_surfaces_unknown_and_missing_formatters_without_modifying_content",
        r##"(list
         (with-temp-buffer
           (insert
            "untouched\n")
           (let ((apheleia-formatters nil))
             (condition-case error
                 (apheleia-test-format-buffer
                  'undefined)
               (error
                (list
                 (car error)
                 (cadr error)
                 (buffer-string))))))
         (with-temp-buffer
           (insert
            "also untouched\n")
           (let ((apheleia-formatters
                  '((missing
                     . ("apheleia-executable-that-does-not-exist"
                        "--format")))))
             (list
              (apheleia-test-format-buffer
               'missing)
              (buffer-string)))))"##,
        expect![[
            r#"OK ((user-error "No such formatter defined in ‘apheleia-formatters’: undefined" "untouched\n") ((:error (error . "Could not find executable for formatter missing, skipping")) "also untouched\n"))"#
        ]],
    )
}

fn apheleia_uses_a_project_configuration_file_in_a_real_formatter_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_uses_a_project_configuration_file_in_a_real_formatter_command",
        r##"(let* ((root
                  (apheleia-test-root
                   "apheleia-project-config"))
                 (project
                  (expand-file-name
                   "service/"
                   root))
                 (source
                  (expand-file-name
                   "src/report.demo"
                   project))
                 (config
                  (expand-file-name
                   ".format-prefix"
                   project))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apheleia-test-cleanup root)
               (make-directory
                (file-name-directory source)
                t)
               (with-temp-file config
                 (insert "PROJECT"))
               (with-temp-file source
                 (insert
                  "alpha\n"
                  "beta\n"))
               (setq buffer
                     (find-file-noselect source))
               (with-current-buffer buffer
                 (goto-char
                  (point-min))
                 (search-forward "beta")
                 (backward-char 2)
                 (let ((apheleia-formatters
                        '((project-prefix
                           . ("sh"
                              "-c"
                              "test \"$1\" = --config || exit 9; prefix=$(cat \"$2\"); sed \"s/^/${prefix}:/\" \"$3\""
                              "formatter"
                              (apheleia-formatters-locate-file
                               "--config"
                               ".format-prefix")
                              input)))))
                   (setq result
                         (list
                          (apheleia-test-format-buffer
                           'project-prefix)
                          (buffer-string)
                          (line-number-at-pos)
                          (current-column)
                          (current-word)
                          (apheleia-test-read-file
                           source))))))
           (apheleia-test-cleanup root))
         result)"##,
        expect![[
            r#"OK ((:error nil) "PROJECT:alpha\nPROJECT:beta\n" 2 10 "beta" "alpha\nbeta\n")"#
        ]],
    )
}

fn apheleia_global_mode_enforces_and_releases_buffer_function_and_skip_policies() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apheleia_global_mode_enforces_and_releases_buffer_function_and_skip_policies",
        r##"(let* ((root
                  (apheleia-test-root
                   "apheleia-global-project"))
                 (existing-path
                  (expand-file-name
                   "existing.txt"
                   root))
                 (local-inhibited-path
                  (expand-file-name
                   "local.txt"
                   root))
                 (function-inhibited-path
                  (expand-file-name
                   "policy.txt"
                   root))
                 (skipped-path
                  (expand-file-name
                   "skipped.txt"
                   root))
                 (created-path
                  (expand-file-name
                   "created.txt"
                   root))
                 existing
                 local-inhibited
                 function-inhibited
                 skipped
                 created
                 phase-one
                 result)
         (unwind-protect
             (progn
               (apheleia-test-cleanup root)
               (make-directory root t)
               (dolist
                   (entry
                    `((,existing-path . "existing record\n")
                      (,local-inhibited-path . "local policy\n")
                      (,function-inhibited-path . "function policy\n")
                      (,skipped-path . "skip policy\n")
                      (,created-path . "created record\n")))
                 (with-temp-file
                     (car entry)
                   (insert
                    (cdr entry))))
               (setq existing
                     (find-file-noselect existing-path)
                     local-inhibited
                     (find-file-noselect local-inhibited-path)
                     function-inhibited
                     (find-file-noselect function-inhibited-path)
                     skipped
                     (find-file-noselect skipped-path))
               (with-current-buffer local-inhibited
                 (setq-local
                  apheleia-inhibit
                  t))
               (let ((apheleia-formatters
                      '((upper
                         . ("tr"
                            "[:lower:]"
                            "[:upper:]"))))
                     (apheleia-mode-alist
                      '((text-mode . upper)))
                     (apheleia-inhibit-functions
                      '((lambda ()
                          (and
                           buffer-file-name
                           (string-suffix-p
                            "policy.txt"
                            buffer-file-name)))))
                     (apheleia-skip-functions
                      '((lambda ()
                          (and
                           buffer-file-name
                           (string-suffix-p
                            "skipped.txt"
                            buffer-file-name)))))
                     (apheleia-test-hook-events
                      nil)
                     (apheleia-post-format-hook
                      '((lambda ()
                          (setq apheleia-test-hook-events
                                (append
                                 apheleia-test-hook-events
                                 (list
                                  (file-name-nondirectory
                                   buffer-file-name))))))))
                 (apheleia-global-mode 1)
                 (setq created
                       (find-file-noselect created-path))
                 (dolist
                     (buffer
                      (list
                       existing
                       local-inhibited
                       function-inhibited
                       skipped
                       created))
                   (with-current-buffer buffer
                     (goto-char
                      (point-max))
                     (insert "phase one\n")
                     (save-buffer)))
                 (apheleia-test-await
                  (lambda ()
                    (= (length apheleia-test-hook-events) 2))
                  "two globally enabled formatters")
                 (setq phase-one
                       (list
                        :existing
                        (apheleia-test-read-file
                         existing-path)
                        :local-inhibit
                        (apheleia-test-read-file
                         local-inhibited-path)
                        :function-inhibit
                        (apheleia-test-read-file
                         function-inhibited-path)
                        :skip
                        (apheleia-test-read-file
                         skipped-path)
                        :created
                        (apheleia-test-read-file
                         created-path)))
                 (setq apheleia-inhibit-functions nil
                       apheleia-skip-functions nil)
                 (with-current-buffer function-inhibited
                   (apheleia-mode-maybe)
                   (goto-char
                    (point-max))
                   (insert "phase two\n")
                   (save-buffer))
                 (with-current-buffer skipped
                   (goto-char
                    (point-max))
                   (insert "phase two\n")
                   (save-buffer))
                 (apheleia-test-await
                  (lambda ()
                    (= (length apheleia-test-hook-events) 4))
                  "formatting after project policies were released")
                 (setq result
                       (list
                        :phase-one phase-one
                        :resumed-function
                        (apheleia-test-read-file
                         function-inhibited-path)
                        :resumed-skip
                        (apheleia-test-read-file
                         skipped-path)
                        :formatted-files
                        (sort
                         (copy-sequence
                          apheleia-test-hook-events)
                         #'string<)))
                 (apheleia-global-mode -1)))
           (apheleia-global-mode -1)
           (apheleia-test-cleanup root))
         result)"##,
        expect![[
            r#"OK (:phase-one (:existing "EXISTING RECORD\nPHASE ONE\n" :local-inhibit "local policy\nphase one\n" :function-inhibit "function policy\nphase one\n" :skip "skip policy\nphase one\n" :created "CREATED RECORD\nPHASE ONE\n") :resumed-function "FUNCTION POLICY\nPHASE ONE\nPHASE TWO\n" :resumed-skip "SKIP POLICY\nPHASE ONE\nPHASE TWO\n" :formatted-files ("created.txt" "existing.txt" "policy.txt" "skipped.txt"))"#
        ]],
    )
}

fn apheleia_failed_formatter_preserves_the_file_and_opens_its_real_error_log() -> ParityBatchCase {
    ParityBatchCase::value(
        "apheleia_failed_formatter_preserves_the_file_and_opens_its_real_error_log",
        r##"(let* ((root
                  (apheleia-test-root
                   "apheleia-validation-error"))
                 (source
                  (expand-file-name
                   "config.toml"
                   root))
                 (log-buffer
                  "*apheleia-sh-log*")
                 buffer
                 result)
         (unwind-protect
             (progn
               (apheleia-test-cleanup root)
               (when
                   (get-buffer log-buffer)
                 (kill-buffer log-buffer))
               (make-directory root t)
               (with-temp-file source
                 (insert
                  "[server]\n"
                  "port = invalid\n"))
               (setq buffer
                     (find-file-noselect source))
               (with-current-buffer buffer
                 (let ((apheleia-formatters
                        '((validation
                           . ("sh"
                              "-c"
                              "printf 'config.toml:2:8: invalid port\\n' >&2; exit 7"))))
                       (apheleia-hide-log-buffers nil)
                       (apheleia-log-only-errors t)
                       (apheleia--last-error-marker nil))
                   (cl-letf
                       (((symbol-function
                          'current-time-string)
                         (lambda ()
                           "Sun Jan  2 03:04:05 2000")))
                     (setq apheleia-test-callback-result
                           :not-called)
                     (apheleia-format-buffer
                      'validation
                      nil
                      :callback
                      (lambda (&rest properties)
                        (setq apheleia-test-callback-result
                              properties)))
                     (apheleia-test-await-callback)
                     (save-window-excursion
                       (apheleia-goto-error)
                       (setq result
                             (list
                              :callback
                              apheleia-test-callback-result
                              :buffer
                              (with-current-buffer buffer
                                (buffer-string))
                              :disk
                              (apheleia-test-read-file
                               source)
                              :log-buffer
                              (buffer-name)
                              :log-line
                              (line-number-at-pos)
                              :log-column
                              (current-column)
                              :log
                              (buffer-string))))))))
           (setq apheleia--last-error-marker nil)
           (when
               (buffer-live-p buffer)
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer))
           (when
               (get-buffer log-buffer)
             (kill-buffer log-buffer))
           (apheleia-test-cleanup root))
         result)"##,
        expect![[
            r#"OK (:callback (:error (error . "Failed to run sh: exit status 7 (see buffer *apheleia-sh-log*)")) :buffer "[server]\nport = invalid\n" :disk "[server]\nport = invalid\n" :log-buffer "*apheleia-sh-log*" :log-line 1 :log-column 0 :log "Sun Jan  2 03:04:05 2000 :: [ORACLE-SANDBOX]/apheleia-validation-error/\n$ sh -c printf\\ \\'config.toml\\:2\\:8\\:\\ invalid\\ port\\\\n\\'\\ \\>\\&2\\;\\ exit\\ 7\n\nconfig.toml:2:8: invalid port\n\nCommand failed with exit code 7.\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apheleia_ports_upstream_word_replacement_workflow_and_keeps_point_on_the_same_word(),
        apheleia_preserves_two_displayed_windows_point_mark_and_mark_ring_through_a_real_patch(),
        apheleia_ports_upstream_line_reordering_workflow_without_moving_point_from_line_two(),
        apheleia_ports_upstream_whitespace_insertion_alignment_case_at_an_expression(),
        apheleia_chains_two_real_processes_in_order_and_emits_one_hook_event_per_formatter(),
        apheleia_input_output_and_inplace_placeholders_drive_real_file_based_formatters(),
        apheleia_lisp_formatter_receives_real_context_and_can_transform_chained_scratch_text(),
        apheleia_builtin_lisp_formatter_reindents_a_practical_function_without_losing_point(),
        apheleia_mode_formats_and_resaves_a_real_file_after_save(),
        apheleia_aborts_delayed_formatting_when_the_user_edits_the_buffer_in_flight(),
        apheleia_surfaces_unknown_and_missing_formatters_without_modifying_content(),
        apheleia_uses_a_project_configuration_file_in_a_real_formatter_command(),
        apheleia_global_mode_enforces_and_releases_buffer_function_and_skip_policies(),
        apheleia_failed_formatter_preserves_the_file_and_opens_its_real_error_log(),
    ]
}
