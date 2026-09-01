use expect_test::expect;

use super::ParityBatchCase;

fn enabling_mode_runs_bundled_space_tracer_and_splits_aligned_windows() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_mode_runs_bundled_space_tracer_and_splits_aligned_windows",
        r####"
(neomacs-live-py-test-run
 "basic"
 "x = 1 * 10\nx = 2 * 10\nx = 3 * 10\ndef foo():\n    pass\n"
 (live-py-mode 1)
 (neomacs-live-py-test-state))
"####,
        expect![[
            r#"OK (:mode t :lighter " Live" :dir "[ORACLE-SANDBOX]/live-py-basic/package/" :path nil :version "python" :args "" :driver nil :module "module" :trace-name "*live-py-trace_module.py_*" :trace "x = 10\nx = 20\nx = 30\n\n\n" :hooks (t t t) :windows ((:kind source :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t) (:kind trace :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t)) :source-truncate t :trace-truncate t)"#
        ]],
    )
}

fn editing_source_immediately_retraces_values_and_preserves_two_window_layout() -> ParityBatchCase {
    ParityBatchCase::value(
        "editing_source_immediately_retraces_values_and_preserves_two_window_layout",
        r####"
(neomacs-live-py-test-run
 "edit"
 "value = 6 * 7\nmessage = 'old'\n"
 (live-py-mode 1)
 (let ((before (neomacs-live-py-test-state)))
   (goto-char (point-min))
   (search-forward "6 * 7")
   (replace-match "8 * 9")
   (search-forward "old")
   (replace-match "new Ω")
   (list :before before
         :after (neomacs-live-py-test-state)
         :source (buffer-string)
         :modified (buffer-modified-p))))
"####,
        expect![[
            r#"OK (:before (:mode t :lighter " Live" :dir "[ORACLE-SANDBOX]/live-py-edit/package/" :path nil :version "python" :args "" :driver nil :module "module" :trace-name "*live-py-trace_module.py_*" :trace "value = 42\nmessage = 'old'\n" :hooks (t t t) :windows ((:kind source :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t) (:kind trace :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t)) :source-truncate t :trace-truncate t) :after (:mode t :lighter " Live" :dir "[ORACLE-SANDBOX]/live-py-edit/package/" :path nil :version "python" :args "" :driver nil :module "module" :trace-name "*live-py-trace_module.py_*" :trace "value = 72\nmessage = 'new Ω'\n" :hooks (t t t) :windows ((:kind source :start-line 1 :point-line 2 :column 16 :hscroll 0 :truncate t) (:kind trace :start-line 1 :point-line 2 :column 0 :hscroll 0 :truncate t)) :source-truncate t :trace-truncate t) :source "value = 8 * 9\nmessage = 'new Ω'\n" :modified t)"#
        ]],
    )
}

fn scrolling_point_and_narrowing_keep_trace_line_alignment() -> ParityBatchCase {
    ParityBatchCase::value(
        "scrolling_point_and_narrowing_keep_trace_line_alignment",
        r####"
(neomacs-live-py-test-run
 "alignment"
 "a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\n"
 (live-py-mode 1)
 (goto-char (point-min))
 (forward-line 2)
 (set-window-start (selected-window) (line-beginning-position 0))
 (setq this-command 'next-line)
 (run-hooks 'post-command-hook)
 (let ((moved (neomacs-live-py-test-state)))
   (narrow-to-region (line-beginning-position 0)
                     (save-excursion (forward-line 3) (point)))
   (goto-char (point-min))
   (forward-line 1)
   (setq this-command 'narrow-to-region)
   (run-hooks 'post-command-hook)
   (list :moved moved :narrowed (neomacs-live-py-test-state)
         :restriction (list (point-min) (point-max)))))
"####,
        expect![[
            r#"OK (:moved (:mode t :lighter " Live" :dir "[ORACLE-SANDBOX]/live-py-alignment/package/" :path nil :version "python" :args "" :driver nil :module "module" :trace-name "*live-py-trace_module.py_*" :trace "a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\n" :hooks (t t t) :windows ((:kind source :start-line 2 :point-line 3 :column 0 :hscroll 0 :truncate t) (:kind trace :start-line 2 :point-line 3 :column 0 :hscroll 0 :truncate t)) :source-truncate t :trace-truncate t) :narrowed (:mode t :lighter " Live" :dir "[ORACLE-SANDBOX]/live-py-alignment/package/" :path nil :version "python" :args "" :driver nil :module "module" :trace-name "*live-py-trace_module.py_*" :trace "a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\n" :hooks (t t t) :windows ((:kind source :start-line 1 :point-line 2 :column 0 :hscroll 0 :truncate t) (:kind trace :start-line 2 :point-line 3 :column 0 :hscroll 0 :truncate t)) :source-truncate t :trace-truncate t) :restriction (7 31))"#
        ]],
    )
}

fn driver_args_and_python_path_are_applied_to_real_trace_execution() -> ParityBatchCase {
    ParityBatchCase::value(
        "driver_args_and_python_path_are_applied_to_real_trace_execution",
        r####"
(neomacs-live-py-test-run
 "driver"
 "def greet(name):\n    return 'hello ' + name\n"
 (let* ((root (plist-get fixture :root))
        (driver (expand-file-name "driver.py" root))
        (helpers (expand-file-name "helpers" root)))
   (make-directory helpers t)
   (with-temp-file (expand-file-name "extra.py" helpers)
     (insert "suffix = ' Ω'\n"))
   (with-temp-file driver
     (insert "from package.module import greet\nfrom extra import suffix\nprint(greet('Ada') + suffix)\n"))
   (live-py-mode 1)
   (setq live-py-path helpers
         live-py-args "-B"
         live-py-driver driver)
   (live-py-update-all)
   (neomacs-live-py-test-state)))
"####,
        expect![[
            r#"OK (:mode t :lighter " Live" :dir "[ORACLE-SANDBOX]/live-py-driver/package/" :path "[ORACLE-SANDBOX]/live-py-driver/helpers" :version "python" :args "-B" :driver "[ORACLE-SANDBOX]/live-py-driver/driver.py" :module "module" :trace-name "*live-py-trace_module.py_*" :trace "name = 'Ada'\nreturn 'hello Ada'\n" :hooks (t t t) :windows ((:kind source :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t) (:kind trace :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t)) :source-truncate t :trace-truncate t)"#
        ]],
    )
}

fn set_directory_computes_dotted_module_and_rejects_non_parent_directories() -> ParityBatchCase {
    ParityBatchCase::value(
        "set_directory_computes_dotted_module_and_rejects_non_parent_directories",
        r####"
(neomacs-live-py-test-run
 "directory"
 "answer = 42\n"
 (live-py-mode 1)
 (let* ((root (plist-get fixture :root))
        (outside (expand-file-name "outside" root))
        valid invalid)
   (make-directory outside t)
   (cl-letf (((symbol-function 'read-directory-name)
              (lambda (&rest _) root)))
     (live-py-set-dir)
     (setq valid (list :dir live-py-dir :module live-py-module
                       :trace (with-current-buffer live-py-trace-name
                                (buffer-string)))))
   (setq invalid
         (condition-case err
             (cl-letf (((symbol-function 'read-directory-name)
                        (lambda (&rest _) outside)))
               (list :value (live-py-set-dir)))
           (error (list :signal (car err)
                        :message (error-message-string err)))))
   (list :valid valid :invalid invalid)))
"####,
        expect![[
            r#"OK (:valid (:dir "[ORACLE-SANDBOX]/live-py-directory/" :module "package.module" :trace "answer = 42\n") :invalid (:signal user-error :message "Working directory [ORACLE-SANDBOX]/live-py-directory/outside must be a parent of [ORACLE-SANDBOX]/live-py-directory/package/module.py"))"#
        ]],
    )
}

fn disabling_restores_truncate_lines_removes_hooks_and_kills_trace_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_restores_truncate_lines_removes_hooks_and_kills_trace_buffer",
        r####"
(neomacs-live-py-test-run
 "disable"
 "x = 10\n"
 (live-py-mode 1)
 (let ((trace-name live-py-trace-name)
       (enabled (neomacs-live-py-test-state)))
   (live-py-mode -1)
   (list :enabled enabled
         :disabled (list :mode live-py-mode
                         :trace-live (and (get-buffer trace-name) t)
                         :truncate truncate-lines
                         :hooks (list (memq #'live-py-after-change-function after-change-functions)
                                      (memq #'live-py-post-command-function post-command-hook)
                                      (memq #'live-py-mode-off kill-buffer-hook))
                         :windows (length (window-list))))))
"####,
        expect![[
            r#"OK (:enabled (:mode t :lighter " Live" :dir "[ORACLE-SANDBOX]/live-py-disable/package/" :path nil :version "python" :args "" :driver nil :module "module" :trace-name "*live-py-trace_module.py_*" :trace "x = 10\n" :hooks (t t t) :windows ((:kind source :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t) (:kind trace :start-line 1 :point-line 1 :column 0 :hscroll 0 :truncate t)) :source-truncate t :trace-truncate t) :disabled (:mode nil :trace-live nil :truncate nil :hooks (nil nil nil) :windows 1))"#
        ]],
    )
}

fn enabling_without_a_visited_file_reports_a_user_error_without_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_without_a_visited_file_reports_a_user_error_without_side_effects",
        r####"
(with-temp-buffer
  (python-mode)
  (condition-case err
      (list :value (live-py-mode 1))
    (error
     (list :signal (car err)
           :message (error-message-string err)
           :mode live-py-mode
           :windows (length (window-list))
           :hooks (list (memq #'live-py-after-change-function after-change-functions)
                        (memq #'live-py-post-command-function post-command-hook))))))
"####,
        expect![[
            r#"OK (:signal user-error :message "Current buffer has no associated file" :mode t :windows 1 :hooks (nil nil))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_mode_runs_bundled_space_tracer_and_splits_aligned_windows(),
        editing_source_immediately_retraces_values_and_preserves_two_window_layout(),
        scrolling_point_and_narrowing_keep_trace_line_alignment(),
        driver_args_and_python_path_are_applied_to_real_trace_execution(),
        set_directory_computes_dotted_module_and_rejects_non_parent_directories(),
        disabling_restores_truncate_lines_removes_hooks_and_kills_trace_buffer(),
        enabling_without_a_visited_file_reports_a_user_error_without_side_effects(),
    ]
}
