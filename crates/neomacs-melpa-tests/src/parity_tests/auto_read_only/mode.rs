use expect_test::expect;

use super::ParityBatchCase;

fn auto_read_only_global_mode_exposes_exact_initial_state_lighter_and_map_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_global_mode_exposes_exact_initial_state_lighter_and_map_contract",
        r##"(list
         auto-read-only-mode
         (default-value
          'auto-read-only-mode)
         auto-read-only-mode-lighter
         (boundp
          'auto-read-only-mode-map)
         (and
          (boundp 'auto-read-only-mode-map)
          auto-read-only-mode-map)
         (get
          'auto-read-only-mode
          'variable-documentation)
         (get
          'auto-read-only-mode
          'custom-type)
         (get
          'auto-read-only-mode
          'globalized-minor-mode)
         (assq
          'auto-read-only-mode
          minor-mode-alist))"##,
        expect![[
            r#"OK (nil nil " AutoRO" nil nil "Non-nil if Auto-Read-Only mode is enabled.\nSee the `auto-read-only-mode' command\nfor a description of this minor mode.\nSetting this variable directly does not take effect;\neither customize it (see the info node `Easy Customization')\nor call the function `auto-read-only-mode'." boolean nil (auto-read-only-mode auto-read-only-mode-lighter))"#
        ]],
    )
}

fn auto_read_only_mode_enable_and_disable_install_and_remove_one_global_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_mode_enable_and_disable_install_and_remove_one_global_hook",
        r##"(let ((find-file-hook nil))
         (list
          (list
           :initial
           auto-read-only-mode
           (auto-read-only-test-hook-count
            'auto-read-only--hook-find-file
            'find-file-hook))
          (list
           :enable
           (auto-read-only-mode 1)
           auto-read-only-mode
           find-file-hook
           (auto-read-only-test-hook-count
            'auto-read-only--hook-find-file
            'find-file-hook))
          (list
           :disable
           (auto-read-only-mode -1)
           auto-read-only-mode
           find-file-hook
           (auto-read-only-test-hook-count
            'auto-read-only--hook-find-file
            'find-file-hook))))"##,
        expect![
            "OK ((:initial nil 0) (:enable t t (auto-read-only--hook-find-file) 1) (:disable nil nil nil 0))"
        ],
    )
}

fn auto_read_only_mode_repeated_enable_disable_and_toggle_are_hook_idempotent() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_mode_repeated_enable_disable_and_toggle_are_hook_idempotent",
        r##"(let ((find-file-hook nil)
                states)
         (dolist
             (argument
              '(1 1 -1 -1 toggle toggle))
           (auto-read-only-mode argument)
           (push
            (list
             argument
             auto-read-only-mode
             (auto-read-only-test-hook-count
              'auto-read-only--hook-find-file
              'find-file-hook)
             find-file-hook)
            states))
         (auto-read-only-mode -1)
         (nreverse states))"##,
        expect![
            "OK ((1 t 1 #1=(auto-read-only--hook-find-file)) (1 t 1 #1#) (-1 nil 0 nil) (-1 nil 0 nil) (toggle t 1 (auto-read-only--hook-find-file)) (toggle nil 0 nil))"
        ],
    )
}

fn auto_read_only_mode_disable_removes_preexisting_identical_hook_registration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_read_only_mode_disable_removes_preexisting_identical_hook_registration",
        r##"(let ((find-file-hook
                '(sentinel-before
                  auto-read-only--hook-find-file
                  sentinel-after)))
         (auto-read-only-mode 1)
         (let ((enabled
                (list
                 find-file-hook
                 (auto-read-only-test-hook-count
                  'auto-read-only--hook-find-file
                  'find-file-hook))))
           (auto-read-only-mode -1)
           (list
            enabled
            find-file-hook
            (auto-read-only-test-hook-count
             'auto-read-only--hook-find-file
             'find-file-hook))))"##,
        expect![
            "OK (((sentinel-before auto-read-only--hook-find-file sentinel-after) 1) (sentinel-before sentinel-after) 0)"
        ],
    )
}

fn auto_read_only_mode_state_and_hook_are_global_across_unrelated_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_mode_state_and_hook_are_global_across_unrelated_buffers",
        r##"(let ((find-file-hook nil)
                (first
                 (generate-new-buffer
                  " *auto-read-only-mode-first*"))
                (second
                 (generate-new-buffer
                  " *auto-read-only-mode-second*")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (auto-read-only-mode 1))
               (list
                (with-current-buffer first
                  (list
                   auto-read-only-mode
                   (local-variable-p
                    'auto-read-only-mode)))
                (with-current-buffer second
                  (list
                   auto-read-only-mode
                   (local-variable-p
                    'auto-read-only-mode)))
                (default-value
                 'auto-read-only-mode)
                (auto-read-only-test-hook-count
                 'auto-read-only--hook-find-file
                 'find-file-hook)))
           (auto-read-only-mode -1)
           (when (buffer-live-p first)
             (kill-buffer first))
           (when (buffer-live-p second)
             (kill-buffer second))))"##,
        expect!["OK ((t nil) (t nil) t 1)"],
    )
}

fn auto_read_only_mode_runs_mode_hook_with_each_requested_state_and_current_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_mode_runs_mode_hook_with_each_requested_state_and_current_buffer",
        r##"(let* ((find-file-hook nil)
                 events
                 (auto-read-only-mode-hook
                  (list
                   (lambda ()
                     (push
                      (list
                       auto-read-only-mode
                       (buffer-name))
                      events)))))
         (with-temp-buffer
           (rename-buffer
            " *auto-read-only-mode-hook*")
           (dolist (argument
                    '(1 1 -1 toggle))
             (auto-read-only-mode argument))
           (prog1
               (list
                auto-read-only-mode
                (nreverse events)
                (auto-read-only-test-hook-count
                 'auto-read-only--hook-find-file
                 'find-file-hook))
             (auto-read-only-mode -1))))"##,
        expect![[
            r#"OK (t ((t " *auto-read-only-mode-hook*") (t " *auto-read-only-mode-hook*") (nil " *auto-read-only-mode-hook*") (t " *auto-read-only-mode-hook*")) 1)"#
        ]],
    )
}

fn auto_read_only_mode_autoload_loads_source_and_enables_global_hook_in_one_call() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_read_only_mode_autoload_loads_source_and_enables_global_hook_in_one_call",
        r##"(let ((find-file-hook nil))
         (list
          (featurep 'auto-read-only)
          (autoloadp
           (symbol-function
            'auto-read-only-mode))
          (auto-read-only-mode 1)
          (featurep 'auto-read-only)
          (autoloadp
           (symbol-function
            'auto-read-only-mode))
          auto-read-only-mode
          find-file-hook
          (auto-read-only-test-hook-count
           'auto-read-only--hook-find-file
           'find-file-hook)
          (prog1
              (file-name-nondirectory
               (symbol-file
                'auto-read-only-mode
                'defun))
            (auto-read-only-mode -1))))"##,
        expect![[r#"OK (nil t t t nil t (auto-read-only--hook-find-file) 1 "auto-read-only.el")"#]],
    )
}

pub(super) fn mode_auto_read_only_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_read_only_global_mode_exposes_exact_initial_state_lighter_and_map_contract(),
        auto_read_only_mode_enable_and_disable_install_and_remove_one_global_hook(),
        auto_read_only_mode_repeated_enable_disable_and_toggle_are_hook_idempotent(),
        auto_read_only_mode_disable_removes_preexisting_identical_hook_registration(),
        auto_read_only_mode_state_and_hook_are_global_across_unrelated_buffers(),
        auto_read_only_mode_runs_mode_hook_with_each_requested_state_and_current_buffer(),
    ]
}

pub(super) fn mode_auto_read_only_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_read_only_mode_autoload_loads_source_and_enables_global_hook_in_one_call()]
}
