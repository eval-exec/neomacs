use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_mode_enable_disable_installs_and_removes_local_hooks_and_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_mode_enable_disable_installs_and_removes_local_hooks_and_state",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (let ((auto-highlight-symbol-mode-hook
                                  (list
                                   (lambda ()
                                     (push
                                      (if
                                          auto-highlight-symbol-mode
                                          :enabled
                                        :disabled)
                                      auto-highlight-symbol-test-events)))))
                             (let ((before
                                    (auto-highlight-symbol-test-mode-state)))
                               (auto-highlight-symbol-mode 1)
                               (let ((enabled
                                      (auto-highlight-symbol-test-mode-state)))
                                 (auto-highlight-symbol-mode -1)
                                 (list
                                  before
                                  enabled
                                  (auto-highlight-symbol-test-mode-state)
                                  auto-highlight-symbol-test-events)))))"##,
        expect![[
            r#"OK ((nil nil nil nil nil nil 0 0) (t #1=((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) " HS" nil (ahs-start-timer eldoc-schedule-timer t) (ahs-start-timer t) 0 0) (nil #1# " HS" nil nil nil 0 0) (:disabled :enabled))"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_repeated_enable_disable_is_hook_idempotent_and_buffer_local()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_repeated_enable_disable_is_hook_idempotent_and_buffer_local",
        r##"(let ((first
                                (generate-new-buffer
                                 " *ahs-first*"))
                               (second
                                (generate-new-buffer
                                 " *ahs-second*")))
                           (unwind-protect
                               (progn
                                 (with-current-buffer first
                                   (auto-highlight-symbol-mode 1)
                                   (auto-highlight-symbol-mode 1))
                                 (with-current-buffer second
                                   (auto-highlight-symbol-mode 1))
                                 (let ((enabled
                                        (mapcar
                                         (lambda (buffer)
                                           (with-current-buffer buffer
                                             (list
                                              auto-highlight-symbol-mode
                                              (length
                                               (seq-filter
                                                (lambda (function)
                                                  (eq
                                                   function
                                                   'ahs-start-timer))
                                                post-command-hook))
                                              ahs-current-range)))
                                         (list first second))))
                                   (with-current-buffer first
                                     (auto-highlight-symbol-mode -1))
                                   (list
                                    enabled
                                    (with-current-buffer first
                                      (auto-highlight-symbol-test-mode-state))
                                    (with-current-buffer second
                                      (auto-highlight-symbol-test-mode-state)))))
                             (kill-buffer first)
                             (kill-buffer second)))"##,
        expect![[
            r#"OK (((t 1 #1=((name . "display area") (lighter . "HS") (start . window-start) (end . window-end))) (t 1 #1#)) (nil #1# " HS" nil nil nil 0 0) (t #1# " HS" nil (ahs-start-timer t) (ahs-start-timer t) 0 0))"#
        ]],
    )
}

fn auto_highlight_symbol_global_mode_enables_only_configured_major_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_global_mode_enables_only_configured_major_modes",
        r##"(let ((elisp-buffer
                                (generate-new-buffer
                                 " *ahs-global-elisp*"))
                               (text-buffer
                                (generate-new-buffer
                                 " *ahs-global-text*"))
                               (fundamental-buffer
                                (generate-new-buffer
                                 " *ahs-global-fundamental*"))
                               (ahs-modes
                                '(emacs-lisp-mode
                                  text-mode)))
                           (unwind-protect
                               (progn
                                 (global-auto-highlight-symbol-mode
                                  -1)
                                 (with-current-buffer elisp-buffer
                                   (emacs-lisp-mode))
                                 (with-current-buffer text-buffer
                                   (text-mode))
                                 (with-current-buffer fundamental-buffer
                                   (fundamental-mode))
                                 (global-auto-highlight-symbol-mode
                                  1)
                                 (let ((enabled
                                        (mapcar
                                         (lambda (buffer)
                                           (with-current-buffer buffer
                                             (list
                                              major-mode
                                              auto-highlight-symbol-mode)))
                                         (list
                                          elisp-buffer
                                          text-buffer
                                          fundamental-buffer))))
                                   (global-auto-highlight-symbol-mode
                                    -1)
                                   (list
                                    enabled
                                    (mapcar
                                     (lambda (buffer)
                                       (with-current-buffer buffer
                                         auto-highlight-symbol-mode))
                                     (list
                                      elisp-buffer
                                      text-buffer
                                      fundamental-buffer)))))
                             (global-auto-highlight-symbol-mode
                              -1)
                             (kill-buffer elisp-buffer)
                             (kill-buffer text-buffer)
                             (kill-buffer fundamental-buffer)))"##,
        expect!["OK (((emacs-lisp-mode t) (text-mode t) (fundamental-mode nil)) (nil nil nil))"],
    )
}

fn auto_highlight_symbol_start_timer_cleans_old_state_and_uses_window_switch_delay_policy()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_start_timer_cleans_old_state_and_uses_window_switch_delay_policy",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (setq
                              auto-highlight-symbol-mode
                              t
                              ahs-idle-interval
                              1.25
                              ahs-highlight-upon-window-switch
                              t)
                             (let ((old-timer
                                    'fixture-old-timer)
                                   calls)
                               (cl-letf
                                   (((symbol-function
                                      'timerp)
                                     (lambda (value)
                                       (eq
                                        value
                                        old-timer)))
                                    ((symbol-function
                                      'cancel-timer)
                                     (lambda (timer)
                                       (push
                                        (list :cancel timer)
                                        calls)))
                                    ((symbol-function
                                      'run-with-idle-timer)
                                     (lambda (delay repeat function)
                                       (push
                                        (list
                                         :schedule
                                         delay
                                         repeat
                                         function)
                                        calls)
                                       (list
                                        :timer
                                        delay)))
                                    ((symbol-function
                                      'ahs-edit-post-command-hook-function)
                                     (lambda ()
                                       (push
                                        :edit-post
                                        calls)))
                                    ((symbol-function
                                      'ahs-unhighlight)
                                     (lambda (&optional force)
                                       (push
                                        (list
                                         :unhighlight
                                         force)
                                        calls))))
                                 (mapcar
                                  (lambda (case)
                                    (setq
                                     calls nil
                                     ahs-idle-timer
                                     old-timer
                                     ahs-selected-window
                                     (if
                                         (eq case 'same-window)
                                         (selected-window)
                                       nil)
                                     ahs-highlight-upon-window-switch
                                     (not
                                      (eq case 'switch-disabled)))
                                    (ahs-start-timer)
                                    (list
                                     case
                                     ahs-idle-timer
                                     (nreverse calls)))
                                  '(same-window
                                    switched-window
                                    switch-disabled))))))"##,
        expect![
            "OK ((same-window (:timer 1.25) (:edit-post (:unhighlight nil) (:cancel fixture-old-timer) (:schedule 1.25 nil ahs-idle-function))) (switched-window (:timer 0) (:edit-post (:unhighlight nil) (:cancel fixture-old-timer) (:schedule 0 nil ahs-idle-function))) (switch-disabled (:timer 1.25) (:edit-post (:unhighlight nil) (:cancel fixture-old-timer) (:schedule 1.25 nil ahs-idle-function))))"
        ],
    )
}

fn auto_highlight_symbol_stop_timer_cancels_only_live_timer_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_stop_timer_cancels_only_live_timer_values",
        r##"(let (calls)
                           (cl-letf
                               (((symbol-function
                                  'timerp)
                                 (lambda (value)
                                   (eq
                                    value
                                    'live)))
                                ((symbol-function
                                  'cancel-timer)
                                 (lambda (timer)
                                   (push
                                    timer
                                    calls))))
                             (mapcar
                              (lambda (value)
                                (setq
                                 ahs-idle-timer
                                 value)
                                (list
                                 value
                                 (ahs-stop-timer)
                                 calls))
                              '(nil
                                stale
                                live))))"##,
        expect!["OK ((nil nil nil) (stale nil nil) (live #1=(live) #1#))"],
    )
}

fn auto_highlight_symbol_idle_function_dispatches_selected_or_all_windows_deterministically()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_idle_function_dispatches_selected_or_all_windows_deterministically",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (let ((second
                                    (split-window-right))
                                   calls)
                               (cl-letf
                                   (((symbol-function
                                      'ahs--do-hl)
                                     (lambda ()
                                       (push
                                        (if
                                            (eq
                                             (selected-window)
                                             second)
                                            :second
                                          :first)
                                        calls))))
                                 (mapcar
                                  (lambda (all)
                                    (setq
                                     calls nil
                                     ahs-highlight-all-windows
                                     all)
                                    (select-window
                                     (frame-first-window))
                                    (ahs-idle-function)
                                    (list
                                     all
                                     (nreverse calls)
                                     (eq
                                      ahs-selected-window
                                      (selected-window))))
                                  '(nil t))))))"##,
        expect!["OK ((nil (:first) t) (t (:first :second) t))"],
    )
}

fn auto_highlight_symbol_focus_hooks_obey_configuration_and_change_selected_window_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_focus_hooks_obey_configuration_and_change_selected_window_state",
        r##"(let (calls)
                           (cl-letf
                               (((symbol-function
                                  'ahs-highlight-now)
                                 (lambda ()
                                   (push
                                    :highlight
                                    calls)))
                                ((symbol-function
                                  'ahs-unfocus-all)
                                 (lambda ()
                                   (push
                                    :unfocus
                                    calls))))
                             (mapcar
                              (lambda (enabled)
                                (setq
                                 calls nil
                                 ahs-enable-focus-hooks
                                 enabled)
                                (list
                                 enabled
                                 (ahs-focus-in
                                  :ignored)
                                 (ahs-focus-out
                                  :ignored)
                                 (nreverse calls)))
                              '(nil t))))"##,
        expect!["OK ((nil nil nil nil) (t #2=(:highlight . #1=(:unfocus)) #1# #2#))"],
    )
}

fn auto_highlight_symbol_clear_exits_edit_then_second_call_removes_map_and_hooks() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_clear_exits_edit_then_second_call_removes_map_and_hooks",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight
                              "alpha"
                              1
                              6)
                             (ht-set
                              ahs-window-map
                              (selected-window)
                              'fixture)
                             (ahs-edit-mode-on)
                             (let ((before
                                    (list
                                     (auto-highlight-symbol-test-mode-state)
                                     (ht-size
                                      ahs-window-map)
                                     (auto-highlight-symbol-test-overlays))))
                               (ahs-clear t)
                               (let ((after-edit-exit
                                      (list
                                       (auto-highlight-symbol-test-mode-state)
                                       (ht-size
                                        ahs-window-map)
                                       (auto-highlight-symbol-test-overlays))))
                                 (ahs-clear t)
                                 (list
                                  before
                                  after-edit-exit
                                  (auto-highlight-symbol-test-mode-state)
                                  (ht-size
                                   ahs-window-map)
                                  (auto-highlight-symbol-test-overlays))))))"##,
        expect![[
            r#"OK (((t #1=((name . "whole buffer") (lighter . "HSA") (face . ahs-plugin-whole-buffer-face) (start . point-min) (end . point-max)) " *HSA*" t #2=(ahs-start-timer t) #3=(ahs-start-timer t) 1 2) 1 ((1 6 current ahs-edit-mode-face 1000 t t) (1 6 others ahs-face nil t t) (7 12 others ahs-face nil t t))) ((t #1# " HSA" nil #2# #3# 0 0) 1 nil) (t #1# " HSA" nil nil nil 0 0) 0 nil)"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_set_idle_interval_distinguishes_integer_zero_float_zero_and_non_numbers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_set_idle_interval_distinguishes_integer_zero_float_zero_and_non_numbers",
        r##"(let ((ahs-idle-interval
                                1.0))
                           (mapcar
                            (lambda (value)
                              (list
                               value
                               (ahs-set-idle-interval
                                value)
                               ahs-idle-interval))
                            '(0
                              0.0
                              0.25
                              -1
                              "2"
                              nil)))"##,
        expect![[
            r#"OK ((0 nil 1.0) (0.0 0.0 0.0) (0.25 0.25 0.25) (-1 -1 -1) ("2" nil -1) (nil nil -1))"#
        ]],
    )
}

fn auto_highlight_symbol_mode_maybe_obeys_mode_list_and_does_not_enable_other_buffers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_mode_maybe_obeys_mode_list_and_does_not_enable_other_buffers",
        r##"(let ((ahs-modes
                                '(emacs-lisp-mode
                                  text-mode)))
                           (mapcar
                            (lambda (mode)
                              (with-temp-buffer
                                (funcall mode)
                                (let ((result
                                       (ahs-mode-maybe)))
                                  (list
                                   mode
                                   result
                                   auto-highlight-symbol-mode
                                   ahs-current-range))))
                            '(emacs-lisp-mode
                              text-mode
                              fundamental-mode
                              python-mode)))"##,
        expect![[
            r#"OK ((emacs-lisp-mode t t #1=((name . "display area") (lighter . "HS") (start . window-start) (end . window-end))) (text-mode t t #1#) (fundamental-mode nil nil nil) (python-mode nil nil nil))"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_mode_enable_disable_installs_and_removes_local_hooks_and_state(),
        auto_highlight_symbol_repeated_enable_disable_is_hook_idempotent_and_buffer_local(),
        auto_highlight_symbol_global_mode_enables_only_configured_major_modes(),
        auto_highlight_symbol_start_timer_cleans_old_state_and_uses_window_switch_delay_policy(),
        auto_highlight_symbol_stop_timer_cancels_only_live_timer_values(),
        auto_highlight_symbol_idle_function_dispatches_selected_or_all_windows_deterministically(),
        auto_highlight_symbol_focus_hooks_obey_configuration_and_change_selected_window_state(),
        auto_highlight_symbol_clear_exits_edit_then_second_call_removes_map_and_hooks(),
        auto_highlight_symbol_set_idle_interval_distinguishes_integer_zero_float_zero_and_non_numbers(),
        auto_highlight_symbol_mode_maybe_obeys_mode_list_and_does_not_enable_other_buffers(),
    ]
}
