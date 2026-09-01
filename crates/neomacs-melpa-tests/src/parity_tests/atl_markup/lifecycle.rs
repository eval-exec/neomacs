use expect_test::expect;

use super::ParityBatchCase;

fn atl_markup_direct_enable_disable_are_buffer_local_idempotent_and_preserve_global_hook()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_direct_enable_disable_are_buffer_local_idempotent_and_preserve_global_hook",
        r##"(let ((original-default
                (default-value
                 'post-command-hook)))
          (unwind-protect
              (progn
                (set-default
                 'post-command-hook
                 '(atl-markup-test-global-observer))
                (with-temp-buffer
                  (let (snapshots)
                    (push
                     (list
                      :initial
                      (local-variable-p
                       'post-command-hook)
                      (copy-sequence
                       post-command-hook)
                      (copy-sequence
                       (default-value
                        'post-command-hook)))
                     snapshots)
                    (atl-markup--enable)
                    (push
                     (list
                      :enabled
                      (local-variable-p
                       'post-command-hook)
                      (copy-sequence
                       post-command-hook)
                      (and
                       (memq
                        'atl-markup--post-command-hook
                        post-command-hook)
                       t))
                     snapshots)
                    (atl-markup--enable)
                    (push
                     (list
                      :enabled-again
                      (copy-sequence
                       post-command-hook)
                      (cl-count
                       'atl-markup--post-command-hook
                       post-command-hook))
                     snapshots)
                    (atl-markup--disable)
                    (push
                     (list
                      :disabled
                      (local-variable-p
                       'post-command-hook)
                      (copy-sequence
                       post-command-hook)
                      (and
                       (memq
                        'atl-markup--post-command-hook
                        post-command-hook)
                       t))
                     snapshots)
                    (list
                     (nreverse snapshots)
                     (copy-sequence
                      (default-value
                       'post-command-hook))))))
            (set-default
             'post-command-hook
             original-default)))"##,
        expect![
            "OK (((:initial nil (atl-markup-test-global-observer) (atl-markup-test-global-observer)) (:enabled t (atl-markup--post-command-hook t) t) (:enabled-again (atl-markup--post-command-hook t) 1) (:disabled nil (atl-markup-test-global-observer) nil)) (atl-markup-test-global-observer))"
        ],
    )
}

fn atl_markup_minor_mode_argument_sequence_runs_mode_hook_and_updates_local_hook_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_minor_mode_argument_sequence_runs_mode_hook_and_updates_local_hook_exactly",
        r##"(with-temp-buffer
          (setq-local
           post-command-hook
           nil)
          (let (events
                snapshots)
            (let ((atl-markup-mode-hook
                   (list
                    (lambda ()
                      (push
                       (list
                        'mode-hook
                        atl-markup-mode)
                       events)))))
              (dolist
                  (argument
                   '(1 1 -1 nil nil 0 2))
                (push
                 (list
                  argument
                  (atl-markup-mode argument)
                  atl-markup-mode
                  (and
                   (memq
                    'atl-markup--post-command-hook
                    post-command-hook)
                   t)
                  (copy-sequence
                   post-command-hook))
                 snapshots))
              (list
               (nreverse snapshots)
               (nreverse events)
               (local-variable-p
                'atl-markup-mode)
               (local-variable-p
                'post-command-hook)))))"##,
        expect![
            "OK (((1 t t t (atl-markup--post-command-hook)) (1 t t t (atl-markup--post-command-hook)) (-1 nil nil nil nil) (nil t t t (atl-markup--post-command-hook)) (nil t t t (atl-markup--post-command-hook)) (0 nil nil nil nil) (2 t t t (atl-markup--post-command-hook))) ((mode-hook t) (mode-hook t) (mode-hook nil) (mode-hook t) (mode-hook t) (mode-hook nil) (mode-hook t)) t t)"
        ],
    )
}

fn atl_markup_minor_mode_and_post_command_hook_are_isolated_across_live_buffers() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_minor_mode_and_post_command_hook_are_isolated_across_live_buffers",
        r##"(let ((first
                (generate-new-buffer
                 " *atl-mode-first*"))
               (second
                (generate-new-buffer
                 " *atl-mode-second*")))
          (unwind-protect
              (progn
                (with-current-buffer first
                  (setq-local
                   post-command-hook nil)
                  (atl-markup-mode 1))
                (with-current-buffer second
                  (setq-local
                   post-command-hook nil))
                (let ((before
                       (list
                        (with-current-buffer first
                          (list
                           atl-markup-mode
                           (copy-sequence
                            post-command-hook)))
                        (with-current-buffer second
                          (list
                           atl-markup-mode
                           (copy-sequence
                            post-command-hook))))))
                  (with-current-buffer second
                    (atl-markup-mode 1))
                  (with-current-buffer first
                    (atl-markup-mode -1))
                  (list
                   before
                   (with-current-buffer first
                     (list
                      atl-markup-mode
                      (copy-sequence
                       post-command-hook)))
                   (with-current-buffer second
                     (list
                      atl-markup-mode
                      (copy-sequence
                       post-command-hook)))
                   (default-value
                    'atl-markup-mode))))
            (kill-buffer first)
            (kill-buffer second)))"##,
        expect![
            "OK (((t (atl-markup--post-command-hook)) (nil nil)) (nil nil) (t (atl-markup--post-command-hook)) nil)"
        ],
    )
}

fn atl_markup_buffer_local_custom_values_drive_independent_guard_and_timer_behavior()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_buffer_local_custom_values_drive_independent_guard_and_timer_behavior",
        r##"(let ((first
                (generate-new-buffer
                 " *atl-custom-first*"))
               (second
                (generate-new-buffer
                 " *atl-custom-second*"))
               events)
          (unwind-protect
              (progn
                (with-current-buffer first
                  (atl-markup-test-place-marker
                   "<tag>x|value</tag>")
                  (setq-local
                   atl-markup-ignore-regex
                   "x")
                  (setq-local
                   atl-markup-delay
                   0.25))
                (with-current-buffer second
                  (atl-markup-test-place-marker
                   "<tag>x|value</tag>")
                  (setq-local
                   atl-markup-ignore-regex
                   "z")
                  (setq-local
                   atl-markup-delay
                   4.5))
                (cl-letf
                    (((symbol-function 'timerp)
                      (lambda (_value)
                        nil))
                     ((symbol-function 'run-with-idle-timer)
                      (lambda (&rest arguments)
                        (push arguments events)
                        :scheduled))
                     ((symbol-function 'toggle-truncate-lines)
                      (lambda (argument)
                        (push
                         (list 'toggle argument)
                         events)
                        argument)))
                  (list
                   (with-current-buffer first
                     (list
                      (atl-markup--web-truncate-lines-by-face)
                      (atl-markup--post-command-hook)
                      atl-markup-ignore-regex
                      atl-markup-delay))
                   (with-current-buffer second
                     (list
                      (atl-markup--web-truncate-lines-by-face)
                      (atl-markup--post-command-hook)
                      atl-markup-ignore-regex
                      atl-markup-delay))
                   (nreverse events)
                   (list
                    (default-value
                     'atl-markup-ignore-regex)
                    (default-value
                     'atl-markup-delay)))))
            (kill-buffer first)
            (kill-buffer second)))"##,
        expect![[
            r#"OK ((nil :scheduled "x" 0.25) (-1 :scheduled "z" 4.5) ((0.25 nil atl-markup--web-truncate-lines-by-face) (toggle -1) (4.5 nil atl-markup--web-truncate-lines-by-face)) ("[ \11\15\n]" 0.1))"#
        ]],
    )
}

fn atl_markup_enabled_mode_drives_deterministic_navigation_workflow_through_queued_callbacks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_enabled_mode_drives_deterministic_navigation_workflow_through_queued_callbacks",
        r##"(with-temp-buffer
          (insert
           "<main class=\"page\">\n"
           "  <p>Hello world</p>\n"
           "</main>")
          (setq-local
           post-command-hook nil)
          (setq-local
           truncate-lines nil)
          (atl-markup-mode 1)
          (let (scheduled
                snapshots)
            (cl-letf
                (((symbol-function 'timerp)
                  (lambda (_value)
                    nil))
                 ((symbol-function 'run-with-idle-timer)
                  (lambda (delay repeat function &rest arguments)
                    (let ((token
                           (list
                            :timer
                            delay
                            repeat
                            function
                            arguments)))
                      (push token scheduled)
                      token))))
              (dolist
                  (needle
                   '("class"
                     "Hello"
                     "</ma"))
                (goto-char
                 (point-min))
                (search-forward needle)
                (run-hooks
                 'post-command-hook)
                (let ((timer
                       (car scheduled)))
                  (apply
                   (nth 3 timer)
                   (nth 4 timer)))
                (push
                 (list
                  needle
                  truncate-lines
                  (point)
                  (car scheduled))
                 snapshots))
              (atl-markup-mode -1)
              (let ((count-before
                     (length scheduled)))
                (goto-char
                 (point-min))
                (search-forward "Hello")
                (run-hooks
                 'post-command-hook)
                (list
                 (nreverse snapshots)
                 count-before
                 (length scheduled)
                 atl-markup-mode
                 (copy-sequence
                  post-command-hook)
                 truncate-lines)))))"##,
        expect![[
            r#"OK ((("class" t 12 (:timer 0.1 nil atl-markup--web-truncate-lines-by-face nil)) ("Hello" nil 31 (:timer 0.1 nil atl-markup--web-truncate-lines-by-face nil)) ("</ma" t 46 (:timer 0.1 nil atl-markup--web-truncate-lines-by-face nil))) 3 3 nil nil t)"#
        ]],
    )
}

fn atl_markup_enable_prepends_package_hook_and_disable_restores_existing_hook_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_enable_prepends_package_hook_and_disable_restores_existing_hook_order",
        r##"(with-temp-buffer
          (let (events
                first-hook
                second-hook)
            (setq first-hook
                  (lambda ()
                    (push
                     'existing-first
                     events)))
            (setq second-hook
                  (lambda ()
                    (push
                     'existing-second
                     events)))
            (setq-local
             post-command-hook
             (list
              first-hook
              second-hook))
            (cl-letf
                (((symbol-function 'timerp)
                  (lambda (_value)
                    nil))
                 ((symbol-function 'run-with-idle-timer)
                  (lambda (&rest arguments)
                    (push
                     (cons 'package arguments)
                     events)
                    :scheduled)))
              (cl-labels
                  ((hook-names
                    (hooks)
                    (mapcar
                     (lambda (hook)
                       (cond
                        ((eq
                          hook
                          'atl-markup--post-command-hook)
                         'package)
                        ((eq hook first-hook)
                         'existing-first)
                        ((eq hook second-hook)
                         'existing-second)
                        (t :unexpected)))
                     hooks)))
                (let ((before
                       (hook-names
                        post-command-hook)))
                  (atl-markup--enable)
                  (let ((enabled
                         (hook-names
                          post-command-hook)))
                    (run-hooks
                     'post-command-hook)
                    (let ((ran
                           (nreverse events)))
                      (atl-markup--disable)
                      (list
                       before
                       enabled
                       ran
                       (hook-names
                        post-command-hook)))))))))"##,
        expect![
            "OK ((existing-first existing-second) (package existing-first existing-second) ((package 0.1 nil atl-markup--web-truncate-lines-by-face) existing-first existing-second) (existing-first existing-second))"
        ],
    )
}

fn atl_markup_disabling_mode_leaves_existing_timer_and_truncation_state_untouched()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_disabling_mode_leaves_existing_timer_and_truncation_state_untouched",
        r##"(with-temp-buffer
          (setq-local
           post-command-hook nil)
          (setq-local
           truncate-lines t)
          (let ((atl-markup--timer
                 :pending-idle-timer)
                cancellations)
            (cl-letf
                (((symbol-function 'cancel-timer)
                  (lambda (timer)
                    (push timer cancellations))))
              (atl-markup-mode 1)
              (let ((enabled
                     (list
                      atl-markup-mode
                      truncate-lines
                      atl-markup--timer
                      (copy-sequence
                       post-command-hook))))
                (atl-markup-mode -1)
                (list
                 enabled
                 (list
                  atl-markup-mode
                  truncate-lines
                  atl-markup--timer
                  (copy-sequence
                   post-command-hook))
                 cancellations)))))"##,
        expect![
            "OK ((t t :pending-idle-timer (atl-markup--post-command-hook)) (nil t :pending-idle-timer nil) nil)"
        ],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_markup_direct_enable_disable_are_buffer_local_idempotent_and_preserve_global_hook(),
        atl_markup_minor_mode_argument_sequence_runs_mode_hook_and_updates_local_hook_exactly(),
        atl_markup_minor_mode_and_post_command_hook_are_isolated_across_live_buffers(),
        atl_markup_buffer_local_custom_values_drive_independent_guard_and_timer_behavior(),
        atl_markup_enabled_mode_drives_deterministic_navigation_workflow_through_queued_callbacks(),
        atl_markup_enable_prepends_package_hook_and_disable_restores_existing_hook_order(),
        atl_markup_disabling_mode_leaves_existing_timer_and_truncation_state_untouched(),
    ]
}
